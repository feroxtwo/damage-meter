//! The run history: every dungeon run, its party and its boss fights, in SQLite.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use a2tools_dps_meter_lib::entity::fight_record::FightRecord;

use crate::names;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT
);

-- Characters this meter has recorded as "you". Excluded from partner stats.
CREATE TABLE IF NOT EXISTS my_characters (
    name      TEXT PRIMARY KEY,
    server_id INTEGER,
    job       TEXT,
    level     INTEGER,
    last_seen INTEGER
);

-- One entry into an instance until you leave it.
CREATE TABLE IF NOT EXISTS runs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    dungeon_id   INTEGER NOT NULL,
    dungeon_name TEXT,
    difficulty   TEXT,
    kind         TEXT NOT NULL DEFAULT 'expedition',
    started_at   INTEGER NOT NULL,
    ended_at     INTEGER,
    character    TEXT,
    server_id    INTEGER,
    note         TEXT
);
CREATE INDEX IF NOT EXISTS runs_started ON runs(started_at);

CREATE TABLE IF NOT EXISTS run_members (
    run_id       INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    job          TEXT,
    server_id    INTEGER,
    level        INTEGER,
    gear_score   INTEGER,
    combat_power INTEGER,
    dbid         INTEGER,
    is_self      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (run_id, name)
);
CREATE INDEX IF NOT EXISTS run_members_name ON run_members(name);

CREATE TABLE IF NOT EXISTS fights (
    id           TEXT PRIMARY KEY,
    run_id       INTEGER REFERENCES runs(id) ON DELETE CASCADE,
    boss_name    TEXT,
    mob_code     INTEGER,
    target_id    INTEGER,
    dungeon_id   INTEGER,
    started_at   INTEGER,
    duration_ms  INTEGER,
    total_damage INTEGER,
    max_hp       INTEGER,
    is_train     INTEGER NOT NULL DEFAULT 0,
    record_json  TEXT
);
CREATE INDEX IF NOT EXISTS fights_started ON fights(started_at);
CREATE INDEX IF NOT EXISTS fights_run ON fights(run_id);
CREATE INDEX IF NOT EXISTS fights_boss_index ON fights(dungeon_id, mob_code);
CREATE TABLE IF NOT EXISTS community_snapshots (
    source_id     TEXT NOT NULL,
    balance_id    TEXT NOT NULL,
    source_url    TEXT NOT NULL,
    captured_at   INTEGER NOT NULL,
    from_ms       INTEGER NOT NULL,
    until_ms      INTEGER NOT NULL,
    imported_at   INTEGER NOT NULL,
    metric        TEXT NOT NULL DEFAULT 'unknown',
    outcome       TEXT NOT NULL DEFAULT 'unknown',
    aggregation   TEXT NOT NULL DEFAULT 'unknown',
    patch_id      TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (source_id, balance_id)
);
CREATE TABLE IF NOT EXISTS community_reference_rows (
    source_id     TEXT NOT NULL,
    balance_id    TEXT NOT NULL,
    region        TEXT NOT NULL,
    dungeon_id    INTEGER NOT NULL,
    mob_code      INTEGER NOT NULL,
    class_key     TEXT NOT NULL,
    cp_min        INTEGER NOT NULL,
    cp_max        INTEGER NOT NULL,
    median_dps    REAL NOT NULL,
    samples       INTEGER NOT NULL,
    FOREIGN KEY (source_id, balance_id)
        REFERENCES community_snapshots(source_id, balance_id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, balance_id, region, dungeon_id, mob_code, class_key, cp_min, cp_max)
);
CREATE INDEX IF NOT EXISTS community_ref_boss ON community_reference_rows(dungeon_id, mob_code, class_key);
CREATE TABLE IF NOT EXISTS fight_annotations (
 fight_id TEXT PRIMARY KEY REFERENCES fights(id) ON DELETE CASCADE,
 favorite INTEGER NOT NULL DEFAULT 0, note TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS fight_analytics (
 fight_id TEXT PRIMARY KEY REFERENCES fights(id) ON DELETE CASCADE, data TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS fight_players (
    fight_id        TEXT NOT NULL REFERENCES fights(id) ON DELETE CASCADE,
    actor_id        INTEGER NOT NULL,
    name            TEXT,
    job             TEXT,
    damage          INTEGER,
    dps             REAL,
    share           REAL,
    heal            INTEGER,
    damage_received INTEGER,
    combat_power    INTEGER,
    server_id       INTEGER,
    is_self         INTEGER NOT NULL DEFAULT 0,
    hits_received   INTEGER NOT NULL DEFAULT 0,
    died            INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (fight_id, actor_id)
);
CREATE INDEX IF NOT EXISTS fight_players_name ON fight_players(name);

-- Buffs on fighters and debuffs on the boss: share of the fight they were up.
CREATE TABLE IF NOT EXISTS fight_effects (
    fight_id  TEXT NOT NULL REFERENCES fights(id) ON DELETE CASCADE,
    entity_id INTEGER NOT NULL,
    code      INTEGER NOT NULL,
    name      TEXT,
    caster_id INTEGER,
    uptime    REAL NOT NULL,
    on_boss   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (fight_id, entity_id, code)
);
"#;

/// Offline provider observations: reference information only, never a score
/// cohort. A local cache of `--reference-db`, so an older layout is rebuilt.
const PROVIDER_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS provider_archives (
    source_id       TEXT PRIMARY KEY,
    source_url      TEXT NOT NULL,
    captured_at     INTEGER NOT NULL,
    detail          TEXT NOT NULL,
    evidence        TEXT NOT NULL,
    row_count       INTEGER NOT NULL,
    rights_confirmed INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS provider_observations (
    source_id  TEXT NOT NULL REFERENCES provider_archives(source_id) ON DELETE CASCADE,
    row_id     INTEGER NOT NULL,
    class_key  TEXT NOT NULL,
    dungeon_id INTEGER,
    mob_code   INTEGER,
    payload    TEXT NOT NULL,
    PRIMARY KEY (source_id, row_id)
);
CREATE INDEX IF NOT EXISTS provider_observations_mob ON provider_observations(mob_code);
CREATE INDEX IF NOT EXISTS provider_observations_dungeon ON provider_observations(dungeon_id);
"#;

/// Columns added after the first release, for databases created by it.
const MIGRATIONS: &[(&str, &str, &str)] = &[
    (
        "fight_players",
        "hits_received",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("fight_players", "died", "INTEGER NOT NULL DEFAULT 0"),
    ("runs", "favorite", "INTEGER NOT NULL DEFAULT 0"),
    // NULL: no verifiable record. Stored so history never re-parses record_json.
    ("fights", "numeric_limited", "INTEGER"),
    (
        "community_snapshots",
        "metric",
        "TEXT NOT NULL DEFAULT 'unknown'",
    ),
    (
        "community_snapshots",
        "outcome",
        "TEXT NOT NULL DEFAULT 'unknown'",
    ),
    (
        "community_snapshots",
        "aggregation",
        "TEXT NOT NULL DEFAULT 'unknown'",
    ),
    (
        "community_snapshots",
        "patch_id",
        "TEXT NOT NULL DEFAULT ''",
    ),
];

/// Derives `fights.numeric_limited` from a saved record. `instr` skips the JSON
/// parse for the common case without any capped skill value.
const NUMERIC_LIMITED_SQL: &str = "CASE WHEN record_json IS NULL THEN NULL
      WHEN instr(record_json, '2147483647') = 0 THEN 0
      WHEN json_valid(record_json) THEN
        EXISTS(SELECT 1 FROM json_each(record_json, '$.details.skills')
               WHERE json_extract(value, '$.dmg') = 2147483647)
        OR EXISTS(SELECT 1 FROM json_each(record_json, '$.details.healSkills')
                  WHERE json_extract(value, '$.dmg') = 2147483647)
      ELSE NULL END";

/// SQLite's `lower()` folds ASCII only, so a search for "kälte" would miss
/// "Kälte"; `fold()` lowercases with Rust's Unicode rules instead.
fn register_functions(conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "fold",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            Ok(ctx
                .get::<Option<String>>(0)?
                .map(|s| s.to_lowercase())
                .unwrap_or_default())
        },
    )?;
    conn.create_scalar_function(
        "required_bosses",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(serde_json::to_string(&names::dungeon_bosses(ctx.get(0)?)).unwrap()),
    )?;
    set_world_mob_check(conn, |_| false)?;
    set_activity_check(conn, |id, _| {
        names::dungeon_activity(id).unwrap_or("unclassified").into()
    })
}

/// `world_mob(mob_code)` is true for known monsters that are neither bosses
/// (field bosses included) nor training dummies, i.e. ordinary world mobs and
/// trash. Unknown codes count as notable so nothing vanishes from the list.
fn set_world_mob_check(
    conn: &Connection,
    is_world_mob: impl Fn(i32) -> bool + Send + 'static,
) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "world_mob",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let code = ctx.get::<Option<i64>>(0)?.unwrap_or(0);
            Ok(code > 0 && is_world_mob(code as i32))
        },
    )?;
    Ok(())
}

fn set_activity_check(
    conn: &Connection,
    classify: impl Fn(i32, i32) -> String + Send + 'static,
) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "activity_kind",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let id = ctx.get::<Option<i32>>(0)?.unwrap_or(-1);
            let code = ctx.get::<Option<i32>>(1)?.unwrap_or(0);
            Ok(classify(id, code))
        },
    )?;
    Ok(())
}

/// Which saved fights a fight search returns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FightKind {
    #[default]
    All,
    /// Bosses, field bosses, training and unknown targets.
    Boss,
    /// Ordinary world mobs and trash, see `world_mob()`.
    Mob,
}

/// Filters of the dashboard's fight library.
#[derive(Debug, Clone, Default)]
pub struct FightSearch {
    pub query: String,
    pub character: String,
    pub from: i64,
    pub to: i64,
    pub favorites: bool,
    pub kind: FightKind,
    pub limit: i64,
    pub offset: i64,
}

fn migrate(conn: &Connection) -> Result<()> {
    let outdated: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='provider_observations')
            AND NOT EXISTS(SELECT 1 FROM pragma_table_info('provider_observations')
                           WHERE name='mob_code')",
        [],
        |r| r.get(0),
    )?;
    if outdated {
        conn.execute_batch(
            "DROP TABLE provider_observations; DROP TABLE IF EXISTS provider_archives;",
        )?;
    }
    conn.execute_batch(PROVIDER_SCHEMA)?;
    for (table, column, decl) in MIGRATIONS {
        let exists: bool = conn.query_row(
            &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?1"),
            params![column],
            |r| r.get(0),
        )?;
        if !exists {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))?;
        }
    }
    // Fights saved before the column existed; each is derived once.
    conn.execute(
        &format!(
            "UPDATE fights SET numeric_limited = {NUMERIC_LIMITED_SQL}
             WHERE numeric_limited IS NULL AND record_json IS NOT NULL"
        ),
        [],
    )?;
    let ids: Vec<i32> = conn
        .prepare("SELECT DISTINCT dungeon_id FROM runs")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in ids {
        if names::dungeon_activity(id) == Some("transcendence") {
            conn.execute(
                "UPDATE runs SET difficulty = ?1 WHERE dungeon_id = ?2",
                params![names::dungeon_difficulty(id), id],
            )?;
        }
    }
    repair_masked_names_once(conn)
}

/// Periodic boss saves used to keep the upstream upload masking ("Mo****t")
/// for everyone but you, which split partners into a full and a masked entry.
/// Where the run's roster names exactly one member with that mask, use the name.
fn repair_masked_names_once(conn: &Connection) -> Result<()> {
    use a2tools_dps_meter_lib::entity::fight_record::obscure_nickname;
    const KEY: &str = "repair_masked_names_v1";
    let done: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM meta WHERE key = ?1)",
        params![KEY],
        |r| r.get(0),
    )?;
    if done {
        return Ok(());
    }
    let masked: Vec<(i64, String)> = conn
        .prepare(
            "SELECT run_id, name FROM run_members WHERE instr(name, '*') > 0
             UNION
             SELECT f.run_id, p.name FROM fight_players p JOIN fights f ON f.id = p.fight_id
             WHERE f.run_id IS NOT NULL AND instr(p.name, '*') > 0",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let tx = conn.unchecked_transaction()?;
    for (run_id, mask) in masked {
        let full: Vec<String> = tx
            .prepare("SELECT name FROM run_members WHERE run_id = ?1 AND instr(name, '*') = 0")?
            .query_map(params![run_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?
            .into_iter()
            .filter(|n| obscure_nickname(n) == mask)
            .collect();
        let [name] = full.as_slice() else { continue };
        tx.execute(
            "UPDATE fight_players SET name = ?3
             WHERE name = ?2 AND fight_id IN (SELECT id FROM fights WHERE run_id = ?1)",
            params![run_id, mask, name],
        )?;
        tx.execute(
            "DELETE FROM run_members WHERE run_id = ?1 AND name = ?2",
            params![run_id, mask],
        )?;
    }
    tx.execute(
        "INSERT INTO meta(key, value) VALUES (?1, '1')",
        params![KEY],
    )?;
    tx.commit()?;
    Ok(())
}

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Member {
    pub name: String,
    pub job: String,
    pub server_id: u16,
    pub level: i32,
    pub gear_score: i32,
    pub combat_power: i64,
    pub dbid: u64,
    pub is_self: bool,
}

/// One effect's uptime in one fight.
#[derive(Debug, Clone)]
pub struct EffectRow {
    pub fight_id: String,
    pub entity_id: i32,
    pub code: u32,
    pub name: String,
    pub caster_id: i64,
    /// 0..=100
    pub uptime: f64,
    pub on_boss: bool,
}

fn rows_to_json(
    stmt: &mut rusqlite::Statement,
    params: impl rusqlite::Params,
) -> Result<Vec<Value>> {
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map(params, |row| {
        let mut obj = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(n) => json!(n),
                rusqlite::types::ValueRef::Real(f) => json!(f),
                rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                rusqlite::types::ValueRef::Blob(_) => Value::Null,
            };
            obj.insert(name.clone(), v);
        }
        Ok(Value::Object(obj))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Median of observed values, also when the count is even.
fn skill_median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

fn skill_reference(peers: &mut HashMap<(String, i64), Vec<f64>>, own_dps: f64) -> Value {
    let mut values: Vec<f64> = peers
        .values_mut()
        .filter_map(|observations| skill_median(observations))
        .collect();
    let count = values.len();
    if count < 5 {
        return json!({"status":"insufficient","peer_count":count,
                      "reference_dps":null,"score":null});
    }
    let reference = skill_median(&mut values).unwrap_or(0.0);
    if !reference.is_finite() || reference <= 0.0 {
        return json!({"status":"insufficient","peer_count":count,
                      "reference_dps":null,"score":null});
    }
    json!({"status":"ready","peer_count":count,"reference_dps":reference,
           "score":own_dps / reference * 100.0})
}

/// Adds the display fields the UI wants to rows carrying `job` (Korean class
/// name from the parser).
fn decorate_job(rows: &mut [Value]) {
    for row in rows {
        if let Some(job) = row.get("job").and_then(|j| j.as_str()).map(str::to_string) {
            let info = names::class_info(&job);
            row["class_key"] = json!(info.key);
            row["class_name"] = json!(info.name);
        }
    }
}

fn observed_rate(count: i32, hits: f64) -> Option<f64> {
    (count > 0 && hits > 0.0 && count as f64 <= hits).then_some(count as f64 * 100.0 / hits)
}
pub fn skill_rows(
    skills: &[a2tools_dps_meter_lib::entity::details_context::DetailSkillEntry],
    actor: i32,
    duration_ms: i64,
) -> Value {
    let total: i64 = skills
        .iter()
        .filter(|s| s.actor_id == actor)
        .map(|s| i64::from(s.dmg))
        .sum();
    let mut rows: Vec<Value> = skills.iter().filter(|s|s.actor_id==actor).map(|s| {
        let hits=s.time.max(0) as f64;
        json!({"code":s.code,"name":if s.name.is_empty(){format!("#{}",s.code)}else{s.name.clone()},
          "damage":s.dmg,"hits":s.time,"average":(hits>0.0).then_some(s.dmg as f64/hits),
          "dps":s.dmg as f64*1000.0/duration_ms.max(1000) as f64,
          "share":(total>0).then_some(s.dmg as f64*100.0/total as f64),
          "quality_coverage":"unknown","crit_rate":observed_rate(s.crit,hits),
          "back_rate":observed_rate(s.back,hits),"frontal_rate":observed_rate(s.frontal,hits),
          "perfect_rate":observed_rate(s.perfect,hits),"parry_rate":observed_rate(s.parry,hits),
          "double_rate":observed_rate(s.double,hits),"multi_hit_count":(s.multi_hit_count>0).then_some(s.multi_hit_count),
          "multi_hit_damage":(s.multi_hit_damage>0).then_some(s.multi_hit_damage),
          "block_rate":observed_rate(s.shield_block,hits),"perfect_block_rate":observed_rate(s.perfect_block,hits),
          "endurance_rate":observed_rate(s.iron_wall,hits),"regeneration_rate":observed_rate(s.regeneration,hits),
          "miss_count":(s.miss>0).then_some(s.miss),"resist_count":(s.resist>0).then_some(s.resist),"max":s.max_dmg,"min":s.min_dmg,
          "is_dot":s.is_dot,"hit_timestamps":s.hit_timestamps})
    }).collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r["damage"].as_i64().unwrap_or(0)));
    Value::Array(rows)
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        register_functions(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// A saved boss fight with only the self row, for comparison tests.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn insert_test_fight(
        &self,
        id: &str,
        mob: i64,
        dungeon: i64,
        job: &str,
        combat_power: i64,
        dps: f64,
        analytics: &str,
    ) {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO fights(id,boss_name,mob_code,dungeon_id,started_at,duration_ms,
                                is_train,numeric_limited)
             VALUES (?1,'Boss',?2,?3,1785000000000,30000,0,0)",
            params![id, mob, dungeon],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fight_players(fight_id,actor_id,name,job,damage,dps,combat_power,is_self)
             VALUES (?1,1,'Me',?2,?3,?4,?5,1)",
            params![id, job, (dps * 30.0) as i64, dps, combat_power],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fight_analytics(fight_id,data) VALUES (?1,?2)",
            params![id, analytics],
        )
        .unwrap();
    }

    #[cfg(test)]
    pub fn fail_fight_writes(&self, fail: bool) {
        self.conn.lock().execute_batch(if fail {
            "CREATE TRIGGER test_write_error BEFORE INSERT ON fights BEGIN SELECT RAISE(ABORT,'test disk failure'); END;"
        } else {"DROP TRIGGER test_write_error;"}).unwrap();
    }
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        register_functions(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn note_my_character(
        &self,
        name: &str,
        server_id: u16,
        job: &str,
        level: u32,
        now: i64,
    ) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO my_characters(name, server_id, job, level, last_seen) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET
               server_id = CASE WHEN ?2 > 0 THEN ?2 ELSE server_id END,
               job = CASE WHEN ?3 <> '' THEN ?3 ELSE job END,
               level = CASE WHEN ?4 > 0 THEN ?4 ELSE level END,
               last_seen = ?5",
            params![name, server_id, job, level, now],
        )?;
        Ok(())
    }

    pub fn start_run(
        &self,
        dungeon_id: i32,
        started_at: i64,
        character: Option<&str>,
        server_id: u16,
    ) -> Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO runs(dungeon_id, dungeon_name, difficulty, kind, started_at, character, server_id)
             VALUES (?1, ?2, ?3, 'expedition', ?4, ?5, ?6)",
            params![
                dungeon_id,
                names::dungeon_name(dungeon_id).unwrap_or_else(|| format!("Instanz {dungeon_id}")),
                names::dungeon_difficulty(dungeon_id),
                started_at,
                character,
                server_id
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn end_run(&self, run_id: i64, ended_at: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE runs SET ended_at = ?2 WHERE id = ?1",
            params![run_id, ended_at],
        )?;
        // An entry with no fight and nobody else in it was a walk-through
        // (a wrong portal, a disconnect): not worth keeping.
        conn.execute(
            "DELETE FROM runs WHERE id = ?1
               AND NOT EXISTS (SELECT 1 FROM fights WHERE run_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM run_members WHERE run_id = ?1 AND is_self = 0)",
            params![run_id],
        )?;
        Ok(())
    }

    /// Runs left open by a meter that was closed mid-run.
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .lock()
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Name saved fights whose monster code is known but whose name was not
    /// (saved before the name table had it, or in another language).
    pub fn fill_missing_boss_names(&self, name_of: impl Fn(i32) -> String) -> Result<usize> {
        let conn = self.conn.lock();
        let unnamed: Vec<(String, i32)> = conn
            .prepare(
                "SELECT id, mob_code FROM fights
                 WHERE COALESCE(boss_name, '') = '' AND COALESCE(mob_code, 0) > 0",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut named = 0;
        for (id, code) in unnamed {
            let name = name_of(code);
            if !name.is_empty() {
                named += conn.execute(
                    "UPDATE fights SET boss_name = ?2 WHERE id = ?1",
                    params![id, name],
                )?;
            }
        }
        Ok(named)
    }

    pub fn close_dangling_runs(&self) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE runs SET ended_at = COALESCE(
                 (SELECT MAX(started_at + duration_ms) FROM fights WHERE run_id = runs.id), started_at)
             WHERE ended_at IS NULL",
            [],
        )?;
        Ok(())
    }

    pub fn set_run_character(&self, run_id: i64, character: &str, server_id: u16) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE runs SET character = COALESCE(character, ?2),
                             server_id = CASE WHEN COALESCE(server_id, 0) = 0 THEN ?3 ELSE server_id END
             WHERE id = ?1",
            params![run_id, character, server_id],
        )?;
        Ok(())
    }

    pub fn upsert_members(&self, run_id: i64, members: &[Member]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        for m in members {
            tx.execute(
                "INSERT INTO run_members(run_id, name, job, server_id, level, gear_score, combat_power, dbid, is_self)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(run_id, name) DO UPDATE SET
                   job = CASE WHEN excluded.job <> '' THEN excluded.job ELSE job END,
                   server_id = MAX(server_id, excluded.server_id),
                   level = MAX(level, excluded.level),
                   gear_score = MAX(gear_score, excluded.gear_score),
                   combat_power = MAX(combat_power, excluded.combat_power),
                   dbid = CASE WHEN excluded.dbid <> 0 THEN excluded.dbid ELSE dbid END,
                   is_self = MAX(is_self, excluded.is_self)",
                params![
                    run_id,
                    m.name,
                    m.job,
                    m.server_id,
                    m.level,
                    m.gear_score,
                    m.combat_power,
                    m.dbid as i64,
                    m.is_self
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The run a fight belongs to: the newest run of its dungeon that was
    /// under way when the fight started. A run may start up to a minute after
    /// its first fight (the roster naming the instance comes late), but one
    /// that had already started wins: after a restart of the same expedition
    /// the old instance's last fight stays with the old run.
    fn run_for_fight(conn: &Connection, dungeon_id: i32, started_at: i64) -> Result<Option<i64>> {
        if dungeon_id <= 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                "SELECT id FROM runs WHERE dungeon_id = ?1 AND started_at <= ?2 + 60000
                   AND (ended_at IS NULL OR ended_at >= ?2)
                 ORDER BY started_at <= ?2 DESC, started_at DESC LIMIT 1",
                params![dungeon_id, started_at],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Store boss fights. A fight saved again (it went on after a pause)
    /// replaces its earlier copy.
    pub fn save_fights(
        &self,
        records: &[FightRecord],
        fallback_dungeon: i32,
        self_names: &[String],
        local_actor: Option<i64>,
        dead: &HashSet<i32>,
    ) -> Result<usize> {
        if records.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        for record in records {
            let dungeon_id = if record.dungeon_id > 0 {
                record.dungeon_id
            } else {
                fallback_dungeon
            };
            let run_id = Self::run_for_fight(&tx, dungeon_id, record.start_time_ms)?;

            // Damage per actor, from the skill breakdown.
            let mut damage: HashMap<i32, i64> = HashMap::new();
            for s in &record.details.skills {
                *damage.entry(s.actor_id).or_default() += s.dmg as i64;
            }
            let mut heal: HashMap<i32, i64> = HashMap::new();
            for s in &record.details.heal_skills {
                *heal.entry(s.actor_id).or_default() += s.dmg as i64;
            }
            let total: i64 = damage.values().sum::<i64>();
            let secs = (record.duration_ms as f64 / 1000.0).max(1.0);

            tx.execute(
                "INSERT INTO fights(id, run_id, boss_name, mob_code, target_id, dungeon_id, started_at,
                                               duration_ms, total_damage, max_hp, is_train, record_json, numeric_limited)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(id) DO UPDATE SET run_id=COALESCE(excluded.run_id, fights.run_id), boss_name=excluded.boss_name,
                  mob_code=excluded.mob_code, target_id=excluded.target_id,
                  dungeon_id=CASE WHEN excluded.dungeon_id > 0 THEN excluded.dungeon_id ELSE fights.dungeon_id END,
                  max_hp=excluded.max_hp,is_train=excluded.is_train,
                  duration_ms=excluded.duration_ms, total_damage=excluded.total_damage, record_json=excluded.record_json,
                  numeric_limited=excluded.numeric_limited",
                params![
                    record.id,
                    run_id,
                    record.boss_name,
                    record.mob_code,
                    record.target_id,
                    dungeon_id,
                    record.start_time_ms,
                    record.duration_ms,
                    total,
                    record.details.max_hp,
                    record.is_train,
                    serde_json::to_string(record)?,
                    record
                        .details
                        .skills
                        .iter()
                        .chain(&record.details.heal_skills)
                        .any(|s| s.dmg == i32::MAX)
                ],
            )?;
            tx.execute(
                "DELETE FROM fight_players WHERE fight_id = ?1",
                params![record.id],
            )?;
            let mut actors = record.actors.clone();
            for skill in record
                .details
                .skills
                .iter()
                .chain(&record.details.heal_skills)
            {
                if !actors.iter().any(|a| a.actor_id == skill.actor_id) {
                    actors.push(serde_json::from_value(json!({"actorId":skill.actor_id,"nickname":format!("#{}",skill.actor_id),"job":skill.job}))?);
                }
            }
            for actor in &actors {
                let dmg = damage.get(&actor.actor_id).copied().unwrap_or(0);
                // The upstream heal skill totals already include ally healing.
                let healed = heal
                    .get(&actor.actor_id)
                    .copied()
                    .unwrap_or(actor.party_heal);
                if dmg == 0 && healed == 0 && actor.damage_received == 0 {
                    continue;
                }
                let is_self = self_names.iter().any(|n| n == &actor.nickname)
                    || local_actor == Some(actor.actor_id as i64);
                tx.execute(
                    "INSERT OR REPLACE INTO fight_players(fight_id, actor_id, name, job, damage, dps, share, heal,
                                                         damage_received, combat_power, server_id, is_self,
                                                         hits_received, died)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        record.id,
                        actor.actor_id,
                        actor.nickname,
                        actor.job,
                        dmg,
                        dmg as f64 / secs,
                        dmg as f64 * 100.0 / total.max(1) as f64,
                        healed,
                        actor.damage_received,
                        actor.combat_power,
                        actor.server_id,
                        is_self,
                        actor.hits_received,
                        dead.contains(&actor.actor_id)
                    ],
                )?;
                // Everyone who fought alongside you in a run is in its party,
                // even when no roster reached the meter.
                if let Some(run_id) = run_id
                    && (actor.dbid != 0 || is_self)
                {
                    tx.execute(
                            "INSERT INTO run_members(run_id, name, job, server_id, level, gear_score, combat_power, dbid, is_self)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                             ON CONFLICT(run_id, name) DO UPDATE SET
                               job = CASE WHEN excluded.job <> '' THEN excluded.job ELSE job END,
                               is_self = MAX(is_self, excluded.is_self)",
                            params![
                                run_id,
                                actor.nickname,
                                actor.job,
                                actor.server_id,
                                actor.level,
                                actor.gear_score,
                                actor.combat_power,
                                actor.dbid as i64,
                                is_self
                            ],
                        )?;
                }
            }
        }
        tx.commit()?;
        Ok(records.len())
    }

    /// Replace the effect uptimes of these fights.
    pub fn save_effects(&self, records: &[FightRecord], effects: &[EffectRow]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        for record in records {
            tx.execute(
                "DELETE FROM fight_effects WHERE fight_id = ?1",
                params![record.id],
            )?;
        }
        for e in effects {
            tx.execute(
                "INSERT OR REPLACE INTO fight_effects(fight_id, entity_id, code, name, caster_id, uptime, on_boss)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![e.fight_id, e.entity_id, e.code, e.name, e.caster_id, e.uptime, e.on_boss],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_run(&self, run_id: i64) -> Result<bool> {
        let count = self.conn.lock().execute(
            "DELETE FROM runs WHERE id = ?1 AND ended_at IS NOT NULL",
            params![run_id],
        )?;
        Ok(count > 0)
    }

    pub fn set_run_favorite(&self, run_id: i64, favorite: bool) -> Result<bool> {
        let count = self.conn.lock().execute(
            "UPDATE runs SET favorite = ?2 WHERE id = ?1",
            params![run_id, favorite],
        )?;
        Ok(count > 0)
    }

    pub fn set_run_note(&self, run_id: i64, note: &str) -> Result<bool> {
        let count = self.conn.lock().execute(
            "UPDATE runs SET note = ?2 WHERE id = ?1",
            params![run_id, note],
        )?;
        Ok(count > 0)
    }

    // ── queries for the web app ───────────────────────────────────────────

    /// `character` limits everything to runs played on that character; empty
    /// means all of them.
    pub fn list_runs(
        &self,
        limit: i64,
        offset: i64,
        dungeon: Option<&str>,
        character: &str,
        favorites: bool,
    ) -> Result<Value> {
        let conn = self.conn.lock();
        let filter = dungeon.unwrap_or("");
        let mut stmt = conn.prepare(
            "SELECT r.id, r.dungeon_id, r.dungeon_name, r.difficulty, r.kind, r.started_at, r.ended_at,
                    r.character, r.note, r.favorite,
                    CASE WHEN r.ended_at IS NULL THEN 'active'
                         WHEN json_array_length(required_bosses(r.dungeon_id)) > 0
    AND NOT EXISTS (SELECT 1 FROM json_each(required_bosses(r.dungeon_id)) expected
        WHERE NOT EXISTS (SELECT 1 FROM fights f JOIN fight_analytics a ON a.fight_id=f.id
            WHERE f.run_id=r.id AND f.mob_code=expected.value AND f.is_train=0
              AND f.numeric_limited=0 AND json_extract(a.data,'$.outcome')='kill')) THEN 'completed' ELSE 'incomplete' END AS outcome,
                    (SELECT COUNT(*) FROM fights f WHERE f.run_id = r.id AND f.is_train = 0) AS fights,
                    (SELECT json_group_array(json_object('name',name,'job',COALESCE(job,''),'is_self',is_self))
                       FROM run_members m WHERE m.run_id = r.id) AS members,
                    (SELECT ROUND(AVG(fp.dps)) FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
                       WHERE f.run_id = r.id AND f.is_train = 0 AND fp.is_self = 1) AS my_dps
             FROM runs r
             WHERE (?3 = '' OR r.dungeon_name = ?3) AND (?4 = '' OR r.character = ?4)
               AND (?5 = 0 OR r.favorite = 1)
             ORDER BY r.started_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let mut rows = rows_to_json(
            &mut stmt,
            params![limit, offset, filter, character, favorites],
        )?;
        for row in &mut rows {
            let mut members: Vec<Value> =
                serde_json::from_str(row["members"].as_str().unwrap_or("[]"))?;
            decorate_job(&mut members);
            row["members"] = Value::Array(members);
        }
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM runs WHERE (?1 = '' OR dungeon_name = ?1) AND (?2 = '' OR character = ?2)
               AND (?3 = 0 OR favorite = 1)",
            params![filter, character, favorites],
            |r| r.get(0),
        )?;
        let dungeons: Vec<String> = conn
            .prepare("SELECT DISTINCT dungeon_name FROM runs WHERE (?1 = '' OR character = ?1) ORDER BY dungeon_name")?
            .query_map(params![character], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(json!({ "runs": rows, "total": total, "dungeons": dungeons }))
    }

    pub fn run_detail(&self, run_id: i64) -> Result<Option<Value>> {
        Self::run_detail_from(&self.conn.lock(), run_id)
    }

    fn run_detail_from(conn: &Connection, run_id: i64) -> Result<Option<Value>> {
        let mut stmt = conn.prepare("SELECT * FROM runs WHERE id = ?1")?;
        let Some(mut run) = rows_to_json(&mut stmt, params![run_id])?.into_iter().next() else {
            return Ok(None);
        };
        let complete: bool = conn.query_row("SELECT json_array_length(required_bosses(r.dungeon_id)) > 0
    AND NOT EXISTS (SELECT 1 FROM json_each(required_bosses(r.dungeon_id)) expected
        WHERE NOT EXISTS (SELECT 1 FROM fights f JOIN fight_analytics a ON a.fight_id=f.id
            WHERE f.run_id=r.id AND f.mob_code=expected.value AND f.is_train=0
              AND f.numeric_limited=0 AND json_extract(a.data,'$.outcome')='kill')) FROM runs r WHERE r.id=?1",
            params![run_id], |row| row.get(0))?;
        run["outcome"] = json!(if run["ended_at"].is_null() {
            "active"
        } else if complete {
            "completed"
        } else {
            "incomplete"
        });
        let mut stmt = conn.prepare(
            "SELECT name, job, server_id, level, gear_score, combat_power, is_self FROM run_members
             WHERE run_id = ?1 ORDER BY is_self DESC, name",
        )?;
        let mut members = rows_to_json(&mut stmt, params![run_id])?;
        decorate_job(&mut members);
        let mut stmt = conn.prepare(
            "SELECT id, boss_name, mob_code, started_at, duration_ms, total_damage, max_hp, is_train
             FROM fights WHERE run_id = ?1 ORDER BY started_at, id",
        )?;
        let mut fights = rows_to_json(&mut stmt, params![run_id])?;
        let mut stmt = conn.prepare(
            "SELECT name, job, damage, dps, share, heal, damage_received, hits_received, died, is_self
             FROM fight_players WHERE fight_id = ?1 ORDER BY damage DESC",
        )?;
        for fight in &mut fights {
            let id = fight["id"].as_str().unwrap_or_default().to_string();
            let mut players = rows_to_json(&mut stmt, params![id])?;
            decorate_job(&mut players);
            fight["players"] = Value::Array(players);
        }
        let mut stmt = conn.prepare(
            "SELECT fp.name, MAX(fp.job) AS job, SUM(fp.damage) AS damage,
                    SUM(fp.damage) * 1000.0 / MAX((SELECT SUM(MAX(duration_ms,1000)) FROM fights WHERE run_id=?1 AND is_train=0),1000) AS dps, MAX(fp.is_self) AS is_self,
                    SUM(fp.damage_received) AS damage_received, SUM(fp.died) AS deaths
             FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
             WHERE f.run_id = ?1 AND f.is_train = 0
             GROUP BY fp.name ORDER BY damage DESC",
        )?;
        let mut totals = rows_to_json(&mut stmt, params![run_id])?;
        decorate_job(&mut totals);
        run["members"] = Value::Array(members);
        run["fights"] = Value::Array(fights);
        run["totals"] = Value::Array(totals);
        run["totals_scope"] = json!(
            "Summe über erfasste Kämpfe ohne Training. DPS: gemeinsame Summe ihrer Zeitfenster, mindestens 1 Sekunde je Kampf. Fehlende Teilnahme zählt als 0 Schaden."
        );
        Ok(Some(run))
    }

    pub fn fight_detail(&self, fight_id: &str) -> Result<Option<Value>> {
        Self::fight_detail_from(&self.conn.lock(), fight_id)
    }

    fn fight_detail_from(conn: &Connection, fight_id: &str) -> Result<Option<Value>> {
        let mut stmt = conn.prepare(
            "SELECT id, run_id, boss_name, mob_code, target_id, dungeon_id, started_at, duration_ms, total_damage, max_hp,
                    is_train, record_json FROM fights WHERE id = ?1",
        )?;
        let Some(mut fight) = rows_to_json(&mut stmt, params![fight_id])?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let record: Option<FightRecord> = fight["record_json"]
            .as_str()
            .and_then(|s| serde_json::from_str(s).ok());
        fight.as_object_mut().map(|o| o.remove("record_json"));
        let mut stmt = conn.prepare(
            "SELECT actor_id, name, job, server_id, damage, dps, share, heal, damage_received, hits_received, died,
                    combat_power, is_self
             FROM fight_players WHERE fight_id = ?1 ORDER BY damage DESC",
        )?;
        let mut players = rows_to_json(&mut stmt, params![fight_id])?;
        decorate_job(&mut players);

        let mut stmt = conn.prepare(
            "SELECT e.entity_id, e.code, e.name, e.uptime, e.on_boss, e.caster_id, p.name AS caster
             FROM fight_effects e
             LEFT JOIN fight_players p ON p.fight_id = e.fight_id AND p.actor_id = e.caster_id
             WHERE e.fight_id = ?1 ORDER BY e.uptime DESC",
        )?;
        let effects = rows_to_json(&mut stmt, params![fight_id])?;
        let (boss, buffs): (Vec<Value>, Vec<Value>) = effects
            .into_iter()
            .partition(|e| e["on_boss"].as_i64() == Some(1));
        for p in &mut players {
            let actor = p["actor_id"].as_i64();
            p["buffs"] = Value::Array(
                buffs
                    .iter()
                    .filter(|e| e["entity_id"].as_i64() == actor)
                    .cloned()
                    .collect(),
            );
        }
        fight["boss_debuffs"] = Value::Array(boss);
        if let Some(record) = record {
            fight["numeric_limited"] = json!(
                record
                    .details
                    .skills
                    .iter()
                    .chain(&record.details.heal_skills)
                    .any(|s| s.dmg == i32::MAX)
            );
            for p in &mut players {
                let actor = p["actor_id"].as_i64().unwrap_or(0) as i32;
                p["skills"] = skill_rows(&record.details.skills, actor, record.duration_ms);
                p["heal_skills"] =
                    skill_rows(&record.details.heal_skills, actor, record.duration_ms);
                let heals: Vec<_> = record
                    .details
                    .heal_skills
                    .iter()
                    .filter(|s| s.actor_id == actor)
                    .collect();
                if !heals.is_empty() {
                    p["heal"] = json!(heals.iter().map(|s| i64::from(s.dmg)).sum::<i64>());
                }
                p["hps"] = json!(
                    p["heal"].as_f64().unwrap_or(0.0) * 1000.0
                        / record.duration_ms.max(1000) as f64
                );
            }
            fight["ping_history"] = json!(record.details.ping_history);
            fight["healing_scope"] =
                json!("Erfasste Heilung seit dem letzten Parser-Reset, keine effektive Heilung");
            fight["timeline_available"] = json!(
                record
                    .details
                    .skills
                    .iter()
                    .any(|s| !s.hit_timestamps.is_empty())
            );
        }
        fight["difficulty"] = json!(names::dungeon_difficulty(
            fight["dungeon_id"].as_i64().unwrap_or(0) as i32
        ));
        fight["outcome"] = json!("unknown");
        let analytics: Option<String> = conn
            .query_row(
                "SELECT data FROM fight_analytics WHERE fight_id=?1",
                params![fight_id],
                |r| r.get(0),
            )
            .optional()?;
        fight["analytics"] = analytics
            .and_then(|a| serde_json::from_str::<Value>(&a).ok())
            .unwrap_or(Value::Null);
        let annotation: Option<(bool, String, String)> = conn
            .query_row(
                "SELECT favorite,note,tags FROM fight_annotations WHERE fight_id=?1",
                params![fight_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((favorite, note, tags)) = annotation {
            fight["favorite"] = json!(favorite);
            fight["note"] = json!(note);
            fight["tags"] = json!(tags);
        }
        fight["players"] = Value::Array(players);
        Ok(Some(fight))
    }

    /// One consistent read of the recorded run, without training. Reuse the
    /// fight-detail decoder so legacy records and skill metadata behave alike.
    pub fn run_analysis(&self, run_id: i64) -> Result<Option<Value>> {
        let conn = self.conn.lock();
        let Some(mut run) = Self::run_detail_from(&conn, run_id)? else {
            return Ok(None);
        };
        let mut fights = Vec::new();
        for fight in run["fights"].as_array().into_iter().flatten() {
            if fight["is_train"].as_i64() == Some(1) {
                continue;
            }
            if let Some(detail) =
                Self::fight_detail_from(&conn, fight["id"].as_str().unwrap_or_default())?
            {
                fights.push(detail);
            }
        }
        run["fights"] = Value::Array(fights);
        Ok(Some(run))
    }

    pub fn save_analytics(&self, id: &str, data: &Value) -> Result<()> {
        self.conn.lock().execute("INSERT INTO fight_analytics(fight_id,data) VALUES(?1,?2) ON CONFLICT(fight_id) DO UPDATE SET data=excluded.data",params![id,data.to_string()])?;
        Ok(())
    }
    pub fn finish_attempts(&self, ids: &[String], reason: &str) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        for id in ids {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM fights WHERE id=?1)",
                params![id],
                |r| r.get(0),
            )?;
            if !exists {
                continue;
            }
            let raw: Option<String> = tx
                .query_row(
                    "SELECT data FROM fight_analytics WHERE fight_id=?1",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?;
            let mut data = raw
                .and_then(|r| serde_json::from_str::<Value>(&r).ok())
                .unwrap_or_else(|| json!({}));
            // A reset after a kill must not turn it into a wipe.
            if data["end_reason"].is_string() {
                continue;
            }
            data["end_reason"] = json!(reason);
            if reason == "wipe" && data["outcome"] != "kill" {
                data["outcome"] = json!("wipe");
            }
            tx.execute("INSERT INTO fight_analytics(fight_id,data) VALUES(?1,?2) ON CONFLICT(fight_id) DO UPDATE SET data=excluded.data",params![id,data.to_string()])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn annotate(&self, id: &str, favorite: bool, note: &str, tags: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM fights WHERE id=?1)",
            params![id],
            |r| r.get(0),
        )?;
        if exists {
            conn.execute("INSERT INTO fight_annotations VALUES(?1,?2,?3,?4) ON CONFLICT(fight_id) DO UPDATE SET favorite=excluded.favorite,note=excluded.note,tags=excluded.tags",params![id,favorite,note,tags])?;
        }
        Ok(exists)
    }
    /// Tells the database which mob codes are ordinary world mobs.
    pub fn set_world_mob_check(&self, is_world_mob: impl Fn(i32) -> bool + Send + 'static) {
        if let Err(e) = set_world_mob_check(&self.conn.lock(), is_world_mob) {
            tracing::error!("Registering world mob check failed: {e:#}");
        }
    }

    /// Classification uses known instance metadata and the existing NPC lookup.
    pub fn set_activity_check(&self, classify: impl Fn(i32, i32) -> String + Send + 'static) {
        if let Err(e) = set_activity_check(&self.conn.lock(), classify) {
            tracing::error!("Registering activity check failed: {e:#}");
        }
    }

    pub fn search_fights(&self, s: &FightSearch) -> Result<Value> {
        let conn = self.conn.lock();
        let kind = match s.kind {
            FightKind::All => 0,
            FightKind::Boss => 1,
            FightKind::Mob => 2,
        };
        let sql="SELECT f.id,f.boss_name,f.dungeon_id,f.started_at,f.duration_ms,f.total_damage,f.is_train,
          COALESCE(a.favorite,0) AS favorite,COALESCE(a.note,'') AS note,COALESCE(a.tags,'') AS tags,
          (SELECT p.dps FROM fight_players p WHERE p.fight_id=f.id AND p.is_self=1 LIMIT 1) AS my_dps
          FROM fights f LEFT JOIN fight_annotations a ON a.fight_id=f.id
          WHERE (instr(fold(f.boss_name||' '||COALESCE(a.note,'')||' '||COALESCE(a.tags,'')),fold(?1))>0)
           AND (?2='' OR EXISTS(SELECT 1 FROM fight_players p WHERE p.fight_id=f.id AND p.is_self=1 AND p.name=?2))
           AND f.started_at>=?3 AND f.started_at<=?4 AND (?5=0 OR a.favorite=1)
           AND (?8=0 OR (f.is_train=0 AND world_mob(f.mob_code))=(?8=2))
          ORDER BY f.started_at DESC LIMIT ?7 OFFSET ?6";
        let mut rows = rows_to_json(
            &mut conn.prepare(sql)?,
            params![
                s.query,
                s.character,
                s.from,
                s.to,
                s.favorites,
                s.offset,
                s.limit + 1,
                kind
            ],
        )?;
        // One extra row tells the dashboard whether a next page exists.
        let more = rows.len() as i64 > s.limit;
        rows.truncate(s.limit.max(0) as usize);
        for r in &mut rows {
            let dungeon = r["dungeon_id"].as_i64().unwrap_or(0) as i32;
            r["difficulty"] = json!(names::dungeon_difficulty(dungeon));
            // Where the fight happened, for the history list; null in the open world.
            r["dungeon_name"] = json!(if dungeon > 0 {
                names::dungeon_name(dungeon)
            } else {
                None
            });
        }
        Ok(json!({"fights":rows,"more":more}))
    }

    /// The players you most often ran expeditions with.
    pub fn top_partners(&self, limit: i64, character: &str) -> Result<Vec<Value>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT m.name, MAX(m.job) AS job, MAX(m.server_id) AS server_id,
                    COUNT(DISTINCT m.run_id) AS runs,
                    MAX(r.started_at) AS last_run,
                    MAX(m.combat_power) AS combat_power,
                    (SELECT ROUND(AVG(fp.dps)) FROM fight_players fp
                       JOIN fights f ON f.id = fp.fight_id LEFT JOIN runs r3 ON r3.id = f.run_id
                       WHERE fp.name = m.name AND fp.is_self = 0 AND (?2 = '' OR r3.character = ?2)) AS avg_dps,
                    (SELECT r2.dungeon_name FROM run_members m2 JOIN runs r2 ON r2.id = m2.run_id
                       WHERE m2.name = m.name AND (?2 = '' OR r2.character = ?2)
                       GROUP BY r2.dungeon_name ORDER BY COUNT(*) DESC LIMIT 1) AS favourite
             FROM run_members m JOIN runs r ON r.id = m.run_id
             WHERE m.is_self = 0 AND r.kind = 'expedition' AND (?2 = '' OR r.character = ?2)
               AND m.name NOT IN (SELECT name FROM my_characters)
             GROUP BY m.name
             ORDER BY runs DESC, last_run DESC
             LIMIT ?1",
        )?;
        let mut rows = rows_to_json(&mut stmt, params![limit, character])?;
        decorate_job(&mut rows);
        Ok(rows)
    }

    /// Your characters, most recently played first: the ones the meter saw
    /// you log in with, and any a run was recorded on.
    pub fn characters(&self) -> Result<Vec<Value>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT c.name, c.server_id, c.job, c.level, c.last_seen,
                    (SELECT COUNT(*) FROM runs r WHERE r.character = c.name) AS runs
             FROM my_characters c
             UNION ALL
             SELECT r.character, MAX(r.server_id), '', 0, MAX(r.started_at), COUNT(*)
             FROM runs r
             WHERE r.character IS NOT NULL AND r.character <> ''
               AND r.character NOT IN (SELECT name FROM my_characters)
             GROUP BY r.character
             ORDER BY 5 DESC",
        )?;
        let mut rows = rows_to_json(&mut stmt, [])?;
        decorate_job(&mut rows);
        Ok(rows)
    }

    /// Recorded target attempts, grouped by stable area and mob IDs; no kill claim.
    pub fn boss_history(&self, character: &str) -> Result<Value> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT f.boss_name AS boss, f.id AS fight_id, f.started_at, f.duration_ms, f.dungeon_id,
                    fp.dps, fp.share, fp.died, fp.job, fp.server_id, f.numeric_limited, f.mob_code,
                    activity_kind(f.dungeon_id, f.mob_code) AS activity
             FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
             WHERE fp.is_self = 1 AND f.is_train = 0 AND f.boss_name <> '' AND (?1 = '' OR fp.name = ?1)
             ORDER BY f.started_at, f.id",
        )?;
        let rows = rows_to_json(&mut stmt, params![character])?;
        let mut bosses: HashMap<(String, Option<i32>, i32, String), Vec<Value>> = HashMap::new();
        for mut row in rows {
            let boss = row["boss"].as_str().unwrap_or_default().to_string();
            let id = row["dungeon_id"].as_i64().map(|id| id as i32);
            row["difficulty"] = json!(id.and_then(names::dungeon_difficulty));
            let mob = row["mob_code"].as_i64().unwrap_or(0) as i32;
            let activity = row["activity"]
                .as_str()
                .unwrap_or("unclassified")
                .to_string();
            bosses
                .entry((boss, id, mob, activity))
                .or_default()
                .push(row);
        }
        let mut bosses: Vec<_> = bosses.into_iter().collect();
        bosses.sort_by_key(|(_, list)| std::cmp::Reverse(list.len()));
        Ok(Value::Array(
            bosses
                .into_iter()
                .map(|((boss, id, mob, activity), attempts)| {
                    json!({ "boss": boss, "dungeon_id": id, "mob_code": mob,
                        "dungeon_name": id.and_then(names::dungeon_name), "activity": activity,
                        "scope": "boss", "difficulty": id.and_then(names::dungeon_difficulty),
                        "attempts": attempts })
                })
                .collect(),
        ))
    }

    /// Read-only, on-device Skill Index: comparable recorded boss and power bracket.
    /// Every distinct observed peer contributes one median across their attempts,
    /// so running the same boss repeatedly cannot dominate the reference.
    /// This is not a global ranking or a claim that an attempt was a kill.
    #[allow(clippy::type_complexity)]
    pub fn skill_index(&self, fight_id: &str) -> Result<Option<Value>> {
        let conn = self.conn.lock();
        let fight: Option<(
            i64,
            i64,
            i64,
            Option<i64>,
            i64,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<f64>,
        )> = conn
            .query_row(
                "SELECT COALESCE(f.dungeon_id, 0), COALESCE(f.mob_code, 0), f.is_train,
                    f.numeric_limited, COALESCE(f.duration_ms, 0),
                    p.name, p.job, p.combat_power, p.dps
             FROM fights f LEFT JOIN fight_players p
               ON p.fight_id = f.id AND p.is_self = 1
             WHERE f.id = ?1 ORDER BY p.damage DESC LIMIT 1",
                params![fight_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            dungeon_id,
            mob_code,
            is_train,
            numeric_limited,
            duration_ms,
            self_name,
            job,
            cp,
            dps,
        )) = fight
        else {
            return Ok(None);
        };
        let combat_power = cp.unwrap_or(0);
        let own_dps = dps.unwrap_or(0.0);
        let class_key = names::class_info(job.as_deref().unwrap_or("")).key;
        let cp_min = combat_power.saturating_sub(10_000).max(1);
        let cp_max = combat_power.saturating_add(10_000);
        let mut result = json!({
            "source": "local",
            "status": "unavailable",
            "reason": null,
            "own_dps": own_dps,
            "combat_power": combat_power,
            "cp_min": cp_min,
            "cp_max": cp_max,
            "class_key": class_key,
            "required_peers": 5,
            "overall": null,
            "same_class": null
        });
        let reason = if is_train != 0 {
            Some("training")
        } else if numeric_limited != Some(0) {
            Some("limited_data")
        } else if mob_code <= 0 {
            Some("unknown_boss")
        } else if duration_ms < 10_000 {
            Some("short_fight")
        } else if combat_power <= 0 {
            Some("missing_cp")
        } else if !own_dps.is_finite() || own_dps <= 0.0 {
            Some("missing_dps")
        } else {
            None
        };
        if let Some(reason) = reason {
            result["reason"] = json!(reason);
            return Ok(Some(result));
        }
        let world_mob: bool =
            conn.query_row("SELECT world_mob(?1)", params![mob_code], |r| r.get(0))?;
        if world_mob {
            result["reason"] = json!("not_boss");
            return Ok(Some(result));
        }

        // No external upload or service: comparisons use only locally saved
        // fight players. Self characters, anonymized names, incomplete fights
        // and peers without known combat power never enter the reference.
        let mut stmt = conn.prepare(
            "SELECT p.name, COALESCE(p.server_id, 0), COALESCE(p.job, ''), p.dps
             FROM fights f JOIN fight_players p ON p.fight_id = f.id
             WHERE f.dungeon_id = ?1 AND f.mob_code = ?2
               AND f.is_train = 0 AND f.numeric_limited = 0
               AND f.duration_ms >= 10000
               AND p.combat_power BETWEEN ?3 AND ?4 AND p.dps > 0
               AND p.is_self = 0
               AND p.name IS NOT NULL AND length(p.name) > 0
               AND p.name NOT LIKE '#%' AND p.name NOT LIKE '%*%'
               AND p.name <> COALESCE(?5, '')
               AND p.name NOT IN (SELECT name FROM my_characters)",
        )?;
        let rows = stmt.query_map(
            params![dungeon_id, mob_code, cp_min, cp_max, self_name.as_deref()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            },
        )?;
        let mut overall: HashMap<(String, i64), Vec<f64>> = HashMap::new();
        let mut same_class: HashMap<(String, i64), Vec<f64>> = HashMap::new();
        for row in rows {
            let (name, server, peer_job, peer_dps) = row?;
            if !peer_dps.is_finite() || peer_dps <= 0.0 {
                continue;
            }
            if class_key != "unknown" && names::class_info(&peer_job).key == class_key {
                same_class
                    .entry((name.clone(), server))
                    .or_default()
                    .push(peer_dps);
            }
            overall.entry((name, server)).or_default().push(peer_dps);
        }
        result["overall"] = skill_reference(&mut overall, own_dps);
        if class_key != "unknown" {
            result["same_class"] = skill_reference(&mut same_class, own_dps);
        }
        if result["overall"]["status"] == "ready" {
            result["status"] = json!("ready");
        } else {
            result["status"] = json!("insufficient");
            result["reason"] = json!("few_peers");
        }
        Ok(Some(result))
    }

    /// Replace one balance-period snapshot atomically, without affecting combat history.
    pub fn import_community(&self, snapshot: crate::community::CommunitySnapshot) -> Result<Value> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let source = &snapshot.source;
        let balance = &snapshot.balance;
        let (metric, outcome, aggregation, patch_id) = snapshot
            .methodology
            .as_ref()
            .map(|m| {
                (
                    m.metric.as_str(),
                    m.outcome.as_str(),
                    m.aggregation.as_str(),
                    m.patch_id.as_str(),
                )
            })
            .unwrap_or(("unknown", "unknown", "unknown", ""));
        tx.execute(
            "INSERT INTO community_snapshots(
                source_id, balance_id, source_url, captured_at, from_ms, until_ms,
                metric, outcome, aggregation, patch_id, imported_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, strftime('%s','now')*1000)
             ON CONFLICT(source_id, balance_id) DO UPDATE SET
                source_url=excluded.source_url, captured_at=excluded.captured_at,
                from_ms=excluded.from_ms, until_ms=excluded.until_ms,
                metric=excluded.metric, outcome=excluded.outcome,
                aggregation=excluded.aggregation, patch_id=excluded.patch_id,
                imported_at=excluded.imported_at",
            params![
                source.id,
                balance.id,
                source.url,
                source.captured_at,
                balance.from_ms,
                balance.until_ms,
                metric,
                outcome,
                aggregation,
                patch_id
            ],
        )?;
        tx.execute(
            "DELETE FROM community_reference_rows WHERE source_id=?1 AND balance_id=?2",
            params![source.id, balance.id],
        )?;
        let mut stmt = tx.prepare(
            "INSERT INTO community_reference_rows(
                source_id, balance_id, region, dungeon_id, mob_code, class_key,
                cp_min, cp_max, median_dps, samples
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for row in &snapshot.rows {
            stmt.execute(params![
                source.id,
                balance.id,
                row.region,
                row.dungeon_id,
                row.mob_code,
                row.class_key,
                row.cp_min,
                row.cp_max,
                row.median_dps,
                row.samples
            ])?;
        }
        drop(stmt);
        tx.commit()?;
        Ok(json!({
            "source": source.id,
            "balance": balance.id,
            "imported": snapshot.rows.len(),
            "local_only": true
        }))
    }

    /// Delete an imported aggregate snapshot. Cascades into its reference rows.
    /// Never touches local fights or the local Skill Index.
    pub fn delete_community(&self, source: &str, balance: &str) -> Result<bool> {
        let deleted = self.conn.lock().execute(
            "DELETE FROM community_snapshots WHERE source_id=?1 AND balance_id=?2",
            params![source, balance],
        )?;
        Ok(deleted > 0)
    }

    /// Import normalized observations as observations, never as comparable score
    /// cohorts. An identical data state is not rewritten on every start; an older
    /// capture never replaces a newer one, so generations are not mixed.
    pub fn import_provider_archives(
        &self,
        archives: &[crate::reference_bundle::ProviderArchive],
    ) -> Result<Value> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let mut outcome = serde_json::Map::new();
        for archive in archives {
            let source = &archive.source;
            let evidence = serde_json::to_string(&archive.evidence)?;
            let current: Option<(i64, i64, String, String, String)> = tx
                .query_row(
                    "SELECT captured_at, row_count, source_url, detail, evidence
                     FROM provider_archives WHERE source_id=?1",
                    params![source.id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            if let Some((captured, rows, url, detail, stored)) = current {
                if captured > source.captured_at {
                    outcome.insert(source.id.clone(), json!("newer_kept"));
                    continue;
                }
                if captured == source.captured_at
                    && rows == archive.rows.len() as i64
                    && url == source.url
                    && detail == archive.detail
                    && stored == evidence
                {
                    outcome.insert(source.id.clone(), json!("unchanged"));
                    continue;
                }
            }
            tx.execute(
                "INSERT INTO provider_archives(source_id, source_url, captured_at, detail,
                    evidence, row_count, rights_confirmed) VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(source_id) DO UPDATE SET source_url=excluded.source_url,
                    captured_at=excluded.captured_at, detail=excluded.detail,
                    evidence=excluded.evidence, row_count=excluded.row_count,
                    rights_confirmed=excluded.rights_confirmed",
                params![
                    source.id,
                    source.url,
                    source.captured_at,
                    archive.detail,
                    evidence,
                    archive.rows.len(),
                    source.rights_confirmed
                ],
            )?;
            tx.execute(
                "DELETE FROM provider_observations WHERE source_id=?1",
                params![source.id],
            )?;
            let mut stmt = tx.prepare(
                "INSERT INTO provider_observations(source_id, row_id, class_key, dungeon_id,
                    mob_code, payload) VALUES (?1,?2,?3,?4,?5,?6)",
            )?;
            for (id, row) in archive.rows.iter().enumerate() {
                stmt.execute(params![
                    source.id,
                    id,
                    row.class_key,
                    row.dungeon_id,
                    row.mob_code,
                    serde_json::to_string(row)?
                ])?;
            }
            outcome.insert(source.id.clone(), json!("imported"));
        }
        tx.commit()?;
        Ok(Value::Object(outcome))
    }

    /// One page of a provider archive. Bounded: at most 200 rows per request.
    pub fn provider_observations(
        &self,
        source: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Option<Value>> {
        let limit = limit.clamp(1, 200);
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT source_id,source_url,captured_at,detail,row_count,evidence,rights_confirmed
            FROM provider_archives WHERE source_id=?1",
        )?;
        let mut sources = rows_to_json(&mut stmt, params![source])?;
        let Some(mut result) = sources.pop() else {
            return Ok(None);
        };
        let mut stmt = conn.prepare(
            "SELECT payload FROM provider_observations WHERE source_id=?1
             ORDER BY row_id LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt
            .query_map(params![source, limit, offset], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str::<Value>(&r?)?))
            .collect::<Result<Vec<_>>>()?;
        let total = result["row_count"].as_u64().unwrap_or(0);
        let next = u64::from(offset) + rows.len() as u64;
        result["evidence"] = serde_json::from_str(result["evidence"].as_str().unwrap_or("[]"))?;
        result["rights_confirmed"] = json!(result["rights_confirmed"].as_i64() == Some(1));
        result["rows"] = json!(rows);
        result["offset"] = json!(offset);
        result["limit"] = json!(limit);
        result["next_offset"] = json!((next < total).then_some(next));
        // Structural: provider observations are never a score cohort.
        result["score_eligible"] = json!(false);
        Ok(Some(result))
    }

    /// Offline provider data about this boss or dungeon. Information only: the
    /// caller reports why it is not comparable and never derives a score.
    fn offline_observations(
        conn: &Connection,
        dungeon: i64,
        mob: i64,
        class_key: &str,
    ) -> Result<Value> {
        let mut stmt = conn.prepare(
            "SELECT o.source_id, o.class_key, a.rights_confirmed,
                    json_extract(o.payload, '$.compatibility') AS compatibility
             FROM provider_observations o JOIN provider_archives a USING(source_id)
             WHERE (o.mob_code=?1 AND ?1>0)
                OR (o.mob_code IS NULL AND o.dungeon_id=?2 AND ?2>0)",
        )?;
        let mut sources = std::collections::BTreeSet::new();
        let mut blockers = std::collections::BTreeSet::new();
        let (mut rows, mut same_class) = (0, 0);
        let mut query = stmt.query(params![mob, dungeon])?;
        while let Some(row) = query.next()? {
            rows += 1;
            sources.insert(row.get::<_, String>(0)?);
            if row.get::<_, String>(1)? == class_key {
                same_class += 1;
            }
            if row.get::<_, i64>(2)? != 1 {
                blockers.insert("rights_unconfirmed".to_string());
            }
            let flags: Vec<String> =
                serde_json::from_str(&row.get::<_, String>(3).unwrap_or_default())
                    .unwrap_or_default();
            blockers.extend(flags);
        }
        Ok(json!({
            "rows": rows,
            "same_class": same_class,
            "sources": sources,
            "blockers": blockers,
            "score_eligible": false
        }))
    }

    pub fn community_sources(&self) -> Result<Value> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT s.source_id, s.balance_id, s.source_url, s.captured_at,
                    s.from_ms, s.until_ms, s.imported_at,
                    s.metric, s.outcome, s.aggregation, s.patch_id,
                    COUNT(r.source_id) AS rows
             FROM community_snapshots s LEFT JOIN community_reference_rows r
               ON s.source_id=r.source_id AND s.balance_id=r.balance_id
             GROUP BY s.source_id, s.balance_id
             ORDER BY s.imported_at DESC, s.source_id LIMIT 100",
        )?;
        let imports = rows_to_json(&mut stmt, [])?;
        let mut archives = conn.prepare(
            "SELECT source_id, source_url, captured_at, detail, row_count,
                    rights_confirmed = 1 AS rights_confirmed, 0 AS score_eligible
            FROM provider_archives ORDER BY source_id",
        )?;
        let offline_data = rows_to_json(&mut archives, [])?;
        Ok(json!({
            "offline_data": offline_data,
            "providers": crate::community::provider_catalog(),
            "imports": imports,
            "automatic_fetch": false,
            "automatic_upload": false
        }))
    }

    /// Select a reference only from the fight's own balance period, the exact
    /// boss/difficulty, same class and a CP bracket covering the observed CP.
    /// Imported provider medians stay separate; no average of incompatible
    /// leaderboards and no percentile claim.
    pub fn community_index(&self, fight_id: &str, region: &str) -> Result<Option<Value>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT f.dungeon_id, f.mob_code, f.started_at, f.duration_ms,
                    f.is_train, f.numeric_limited, p.job, p.combat_power, p.dps
             FROM fights f LEFT JOIN fight_players p
               ON p.fight_id=f.id AND p.is_self=1
             WHERE f.id=?1 ORDER BY p.damage DESC LIMIT 1",
        )?;
        let mut fights = rows_to_json(&mut stmt, params![fight_id])?;
        let Some(fight) = fights.pop() else {
            return Ok(None);
        };
        let cp = fight["combat_power"].as_i64().unwrap_or(0);
        let dps = fight["dps"].as_f64().unwrap_or(0.0);
        let job = fight["job"].as_str().unwrap_or("");
        let class_key = names::class_info(job).key;
        let mob = fight["mob_code"].as_i64().unwrap_or(0);
        let dungeon = fight["dungeon_id"].as_i64().unwrap_or(0);
        let started_at = fight["started_at"].as_i64().unwrap_or(0);
        let duration = fight["duration_ms"].as_i64().unwrap_or(0);
        let analytics: Option<String> = conn
            .query_row(
                "SELECT data FROM fight_analytics WHERE fight_id=?1",
                params![fight_id],
                |r| r.get(0),
            )
            .optional()?;
        let analytics = analytics.and_then(|v| serde_json::from_str::<Value>(&v).ok());
        let confirmed_kill = analytics.as_ref().is_some_and(|a| a["outcome"] == "kill");
        let capture_partial = analytics.as_ref().is_none_or(|a| a["partial"] == true);

        let reason = if fight["is_train"].as_i64() != Some(0) {
            Some("training")
        } else if fight["numeric_limited"].as_i64() != Some(0) {
            Some("limited_data")
        } else if mob <= 0 {
            Some("unknown_boss")
        } else if duration < 10_000 {
            Some("short_fight")
        } else if cp <= 0 {
            Some("missing_cp")
        } else if !dps.is_finite() || dps <= 0.0 {
            Some("missing_dps")
        } else {
            None
        };
        let mut response = json!({
            "status": "unavailable",
            "reason": reason,
            "region": region,
            "combat_power": cp,
            "own_dps": dps,
            "class_key": class_key,
            "comparisons": [],
            "data_origin": "manual_import",
            "fight_outcome": if confirmed_kill { "confirmed_kill" } else { "unknown" },
            "capture_partial": capture_partial
        });
        if reason.is_some() {
            return Ok(Some(response));
        }
        let world_mob: bool = conn.query_row("SELECT world_mob(?1)", params![mob], |r| r.get(0))?;
        if world_mob {
            response["reason"] = json!("not_boss");
            return Ok(Some(response));
        }

        let mut stmt = conn.prepare(
            "SELECT r.source_id, s.source_url, r.balance_id, r.region,
                    r.class_key, r.median_dps AS reference_dps, r.samples,
                    r.cp_min, r.cp_max, s.captured_at,
                    s.metric, s.outcome, s.aggregation, s.patch_id
             FROM community_reference_rows r
             JOIN community_snapshots s
               ON s.source_id=r.source_id AND s.balance_id=r.balance_id
             WHERE r.dungeon_id=?1 AND r.mob_code=?2
               AND (r.region=?3 OR r.region='ALL')
               AND (r.class_key=?4 OR r.class_key='all')
               AND r.cp_min<=?5 AND r.cp_max>=?5
               AND s.from_ms<=?6 AND s.until_ms>=?6
             ORDER BY CASE WHEN r.class_key=?4 THEN 0 ELSE 1 END,
                      CASE WHEN r.region=?3 THEN 0 ELSE 1 END,
                      r.cp_max-r.cp_min, r.samples DESC, s.captured_at DESC",
        )?;
        let candidates = rows_to_json(
            &mut stmt,
            params![dungeon, mob, region, class_key, cp, started_at],
        )?;
        // Fail closed: a score needs the exact region and class cohort, a
        // declared full-fight method on confirmed kills and a complete local
        // capture. Everything else is reported as withheld, never scored.
        let mut used = HashSet::new();
        let mut comparisons = Vec::new();
        let mut withheld = Vec::new();
        for mut row in candidates {
            let source = row["source_id"].as_str().unwrap_or_default().to_string();
            let balance = row["balance_id"].as_str().unwrap_or_default().to_string();
            let class = row["class_key"].as_str().unwrap_or_default().to_string();
            if !used.insert((source, balance, class.clone())) {
                continue;
            }
            let method_claims_kill = row["outcome"] == "confirmed_kill";
            let method_matches = method_claims_kill
                && row["metric"] == "fight_dps"
                && row["aggregation"] == "median_unique_players"
                && !row["patch_id"].as_str().unwrap_or_default().is_empty();
            let reference = row["reference_dps"].as_f64().unwrap_or(0.0);
            let blocker = if class != class_key {
                Some("other_class_cohort")
            } else if row["region"] != region {
                Some("pooled_region")
            } else if !method_matches {
                Some("methodology_unconfirmed")
            } else if !confirmed_kill || capture_partial {
                Some("kill_unconfirmed")
            } else if !reference.is_finite() || reference <= 0.0 {
                Some("invalid_reference")
            } else {
                None
            };
            if let Some(blocker) = blocker {
                withheld.push(json!({
                    "source_id": row["source_id"], "balance_id": row["balance_id"],
                    "class_key": row["class_key"], "region": row["region"],
                    "reason": blocker
                }));
                continue;
            }
            let score = dps / reference * 100.0;
            if !score.is_finite() {
                continue;
            }
            row["comparison_quality"] = json!("structured");
            row["score"] = json!(score);
            row["scope"] = json!("same_class");
            comparisons.push(row);
        }
        let offline = Self::offline_observations(&conn, dungeon, mob, class_key)?;
        let has_unscored = !withheld.is_empty() || offline["rows"].as_u64().unwrap_or(0) > 0;
        response["status"] = json!(if comparisons.is_empty() {
            "insufficient"
        } else {
            "ready"
        });
        response["reason"] = json!(if !comparisons.is_empty() {
            None
        } else if has_unscored {
            Some("comparison_withheld")
        } else {
            Some("no_matching_reference")
        });
        response["withheld"] = json!(withheld);
        response["offline_observations"] = offline;
        response["comparisons"] = json!(comparisons);
        Ok(Some(response))
    }

    /// Ended runs only. Match run-detail's shared recorded fight windows,
    /// including zero contribution in fights the selected character missed.
    pub fn run_history(&self, character: &str) -> Result<Value> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "WITH windows AS (
                SELECT run_id, SUM(MAX(duration_ms,1000)) AS duration_ms, COUNT(*) AS fight_count
                FROM fights WHERE is_train=0 AND run_id IS NOT NULL GROUP BY run_id
             ), own AS (
                SELECT f.run_id, SUM(fp.damage) AS damage, MAX(fp.died) AS died,
                       CASE WHEN COUNT(fp.job)=COUNT(*) AND COUNT(DISTINCT fp.job)=1
                            THEN MAX(fp.job) ELSE NULL END AS job,
                       CASE WHEN COUNT(f.numeric_limited)=COUNT(*)
                            THEN MAX(f.numeric_limited) ELSE NULL END AS numeric_limited
                FROM fight_players fp JOIN fights f ON f.id=fp.fight_id
                WHERE fp.is_self=1 AND f.is_train=0 AND (?1='' OR fp.name=?1)
                GROUP BY f.run_id
             )
             SELECT r.id AS run_id, r.dungeon_id, r.started_at, w.duration_ms, w.fight_count,
                    o.damage, o.damage*1000.0/w.duration_ms AS dps, o.died, o.job,
                    o.numeric_limited, NULL AS share
             FROM runs r JOIN windows w ON w.run_id=r.id JOIN own o ON o.run_id=r.id
             WHERE r.ended_at IS NOT NULL ORDER BY r.started_at, r.id",
        )?;
        let rows = rows_to_json(&mut stmt, params![character])?;
        let mut groups: HashMap<i32, Vec<Value>> = HashMap::new();
        for row in rows {
            let id = row["dungeon_id"].as_i64().unwrap_or(0) as i32;
            groups.entry(id).or_default().push(row);
        }
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.sort_by_key(|(id, _)| *id);
        Ok(Value::Array(
            groups
                .into_iter()
                .map(|(id, attempts)| {
                    json!({ "boss": "Gesamter Run", "dungeon_id": id,
                        "dungeon_name": names::dungeon_name(id), "scope": "run",
                        "activity": names::dungeon_activity(id).unwrap_or("unclassified"),
                        "difficulty": names::dungeon_difficulty(id), "attempts": attempts })
                })
                .collect(),
        ))
    }

    pub fn summary(&self, character: &str) -> Result<Value> {
        let characters = self.characters()?;
        let conn = self.conn.lock();
        let c = character;
        let (runs, play_ms): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(COALESCE(ended_at, started_at) - started_at), 0) FROM runs
             WHERE (?1 = '' OR character = ?1)",
            params![c],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let fights: i64 = conn.query_row(
            "SELECT COUNT(*) FROM fights f WHERE is_train = 0 AND NOT world_mob(f.mob_code) AND (?1 = '' OR EXISTS
               (SELECT 1 FROM fight_players fp WHERE fp.fight_id = f.id AND fp.is_self = 1 AND fp.name = ?1))",
            params![c],
            |r| r.get(0),
        )?;
        let partners: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT m.name) FROM run_members m JOIN runs r ON r.id = m.run_id
             WHERE m.is_self = 0 AND (?1 = '' OR r.character = ?1)
               AND m.name NOT IN (SELECT name FROM my_characters)",
            params![c],
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT dungeon_name, difficulty, COUNT(*) AS runs,
                    MIN(CASE WHEN r.ended_at IS NOT NULL AND json_array_length(required_bosses(r.dungeon_id)) > 0
    AND NOT EXISTS (SELECT 1 FROM json_each(required_bosses(r.dungeon_id)) expected
        WHERE NOT EXISTS (SELECT 1 FROM fights f JOIN fight_analytics a ON a.fight_id=f.id
            WHERE f.run_id=r.id AND f.mob_code=expected.value AND f.is_train=0
              AND f.numeric_limited=0 AND json_extract(a.data,'$.outcome')='kill'))
                        THEN r.ended_at - r.started_at END) AS fastest_ms
             FROM runs r WHERE (?1 = '' OR character = ?1)
             GROUP BY dungeon_name, difficulty ORDER BY runs DESC",
        )?;
        let per_dungeon = rows_to_json(&mut stmt, params![c])?;
        let mut stmt = conn.prepare(
            "SELECT f.boss_name, f.dungeon_id, fp.name, MAX(fp.dps) AS best_dps, fp.job, COUNT(*) AS attempts
             FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
             WHERE fp.is_self = 1 AND f.is_train = 0 AND NOT world_mob(f.mob_code) AND (?1 = '' OR fp.name = ?1)
             GROUP BY f.boss_name, f.dungeon_id, fp.name, fp.job ORDER BY best_dps DESC LIMIT 10",
        )?;
        let mut my_best = rows_to_json(&mut stmt, params![c])?;
        decorate_job(&mut my_best);
        for row in &mut my_best {
            row["difficulty"] = json!(names::dungeon_difficulty(
                row["dungeon_id"].as_i64().unwrap_or(0) as i32
            ));
        }
        let mut stmt = conn.prepare(
            "SELECT strftime('%Y-%m-%d', started_at / 1000, 'unixepoch', 'localtime') AS day, COUNT(*) AS runs
             FROM runs WHERE started_at >= (strftime('%s', 'now') - 30 * 86400) * 1000
               AND (?1 = '' OR character = ?1)
             GROUP BY day ORDER BY day",
        )?;
        let per_day = rows_to_json(&mut stmt, params![c])?;
        let my_deaths: i64 = conn.query_row(
            "SELECT COALESCE(SUM(fp.died), 0) FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
             WHERE fp.is_self = 1 AND f.is_train = 0 AND NOT world_mob(f.mob_code) AND (?1 = '' OR fp.name = ?1)",
            params![c],
            |r| r.get(0),
        )?;
        Ok(json!({
            "runs": runs,
            "fights": fights,
            "play_ms": play_ms,
            "partners": partners,
            "per_dungeon": per_dungeon,
            "my_best": my_best,
            "per_day": per_day,
            "characters": characters,
            "my_deaths": my_deaths,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aborted_runs_never_beat_confirmed_complete_runs() {
        let db = Db::in_memory().unwrap();
        {
            let conn = db.conn.lock();
            conn.execute_batch("INSERT INTO runs(id,dungeon_id,dungeon_name,difficulty,started_at,ended_at)
                VALUES (1,600063,'Test','Stufe 1',1000,2000), (2,600063,'Test','Stufe 1',3000,13000);").unwrap();
            for (i, code) in names::dungeon_bosses(600063).iter().enumerate() {
                conn.execute("INSERT INTO fights(id,run_id,mob_code,started_at,duration_ms,is_train,numeric_limited)
                    VALUES (?1,2,?2,3000,1000,0,0)", params![format!("boss{i}"), code]).unwrap();
                conn.execute("INSERT INTO fight_analytics(fight_id,data) VALUES (?1,'{\"outcome\":\"kill\"}')",
                    params![format!("boss{i}")]).unwrap();
            }
        }
        assert_eq!(
            db.summary("").unwrap()["per_dungeon"][0]["fastest_ms"],
            10000
        );
        assert_eq!(db.run_detail(1).unwrap().unwrap()["outcome"], "incomplete");
        assert_eq!(db.run_detail(2).unwrap().unwrap()["outcome"], "completed");
        db.conn
            .lock()
            .execute(
                "UPDATE fight_analytics SET data='{\"outcome\":\"wipe\"}' WHERE fight_id='boss0'",
                [],
            )
            .unwrap();
        assert!(db.summary("").unwrap()["per_dungeon"][0]["fastest_ms"].is_null());
        assert_eq!(db.run_detail(2).unwrap().unwrap()["outcome"], "incomplete");
    }

    fn member(name: &str, is_self: bool) -> Member {
        Member {
            name: name.into(),
            job: "검성".into(),
            server_id: 1304,
            level: 45,
            gear_score: 0,
            combat_power: 1000,
            dbid: 1,
            is_self,
        }
    }

    #[test]
    fn community_v2_requires_confirmed_kill_and_explicit_full_fight_method() {
        let db = Db::in_memory().unwrap();
        {
            let conn = db.conn.lock();
            conn.execute_batch(
                "INSERT INTO fights(id,boss_name,mob_code,dungeon_id,started_at,
                                    duration_ms,is_train,numeric_limited)
                 VALUES ('confirmed','Boss',2300409,600093,1785000000000,30000,0,0);
                 INSERT INTO fight_players(fight_id,actor_id,name,job,damage,dps,combat_power,is_self)
                 VALUES ('confirmed',1,'Me','검성',600000,20000,70000,1);
                 INSERT INTO fight_analytics(fight_id,data)
                 VALUES ('confirmed','{\"outcome\":\"kill\",\"partial\":false}');",
            ).unwrap();
        }
        let dataset: crate::community::CommunitySnapshot = serde_json::from_value(json!({
            "schema": "a2m-community-v2",
            "source": {
                "id": "community", "url": "https://example.org/authorized",
                "captured_at": 1_790_000_000_000_i64, "rights_confirmed": true
            },
            "balance": {
                "id": "full-fight-patch", "from_ms": 1_780_000_000_000_i64,
                "until_ms": 1_792_000_000_000_i64
            },
            "methodology": {
                "metric": "fight_dps", "outcome": "confirmed_kill",
                "aggregation": "median_unique_players", "patch_id": "global-2026.10"
            },
            "rows": [{
                "region": "EU", "dungeon_id": 600093, "mob_code": 2300409,
                "class_key": "gladiator", "cp_min": 60000, "cp_max": 80000,
                "median_dps": 10000.0, "samples": 50
            }]
        }))
        .unwrap();
        db.import_community(dataset.validate().unwrap()).unwrap();
        let result = db.community_index("confirmed", "EU").unwrap().unwrap();
        assert_eq!(result["status"], "ready");
        assert_eq!(result["comparisons"][0]["score"], 200.0);
        assert_eq!(result["comparisons"][0]["comparison_quality"], "structured");
        assert_eq!(
            db.community_sources().unwrap()["imports"][0]["metric"],
            "fight_dps"
        );

        let conn = db.conn.lock();
        conn.execute(
            "UPDATE fight_analytics SET data=?1 WHERE fight_id='confirmed'",
            params![r#"{"outcome":"wipe","partial":false}"#],
        )
        .unwrap();
        drop(conn);
        let wiped = db.community_index("confirmed", "EU").unwrap().unwrap();
        assert_eq!(wiped["status"], "insufficient");
        assert_eq!(wiped["comparisons"].as_array().unwrap().len(), 0);

        let conn = db.conn.lock();
        conn.execute(
            "UPDATE fight_analytics SET data=?1 WHERE fight_id='confirmed'",
            params![r#"{"outcome":"kill","partial":true}"#],
        )
        .unwrap();
        drop(conn);
        assert_eq!(
            db.community_index("confirmed", "EU").unwrap().unwrap()["status"],
            "insufficient"
        );
    }

    #[test]
    fn community_references_import_and_match_exact_scope_and_period() {
        let db = Db::in_memory().unwrap();
        {
            let conn = db.conn.lock();
            conn.execute_batch(
                "INSERT INTO fights(id,boss_name,mob_code,dungeon_id,started_at,duration_ms,
                                    is_train,numeric_limited)
                 VALUES ('community-fight','Boss',2300409,600093,1785000000000,30000,0,0);
                 INSERT INTO fight_players(
                    fight_id,actor_id,name,job,damage,dps,combat_power,is_self)
                 VALUES ('community-fight',1,'Me','검성',600000,20000,70000,1);",
            )
            .unwrap();
        }
        let dataset: crate::community::CommunitySnapshot = serde_json::from_value(json!({
            "schema": "a2m-community-v1",
            "source": {
                "id": "community", "url": "https://example.org/statistics",
                "captured_at": 1790000000000_i64, "rights_confirmed": true
            },
            "balance": {
                "id": "patch-1", "from_ms": 1780000000000_i64,
                "until_ms": 1792000000000_i64
            },
            "rows": [
                {
                    "region": "EU", "dungeon_id": 600093, "mob_code": 2300409,
                    "class_key": "gladiator", "cp_min": 60000, "cp_max": 80000,
                    "median_dps": 10000.0, "samples": 50
                },
                {
                    "region": "EU", "dungeon_id": 600093, "mob_code": 2300409,
                    "class_key": "all", "cp_min": 60000, "cp_max": 80000,
                    "median_dps": 16000.0, "samples": 120
                },
                {
                    "region": "NAE", "dungeon_id": 600093, "mob_code": 2300409,
                    "class_key": "gladiator", "cp_min": 60000, "cp_max": 80000,
                    "median_dps": 1.0, "samples": 50
                },
                {
                    "region": "EU", "dungeon_id": 600093, "mob_code": 9999,
                    "class_key": "gladiator", "cp_min": 60000, "cp_max": 80000,
                    "median_dps": 1.0, "samples": 50
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            db.import_community(dataset.validate().unwrap()).unwrap()["imported"],
            4
        );
        let result = db
            .community_index("community-fight", "EU")
            .unwrap()
            .unwrap();
        // A legacy snapshot declares no DPS method: rows are found, but withheld.
        assert_eq!(result["status"], "insufficient");
        assert_eq!(result["reason"], "comparison_withheld");
        assert!(result["comparisons"].as_array().unwrap().is_empty());
        let reasons: Vec<_> = result["withheld"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                (
                    w["class_key"].as_str().unwrap(),
                    w["reason"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            reasons,
            [
                ("gladiator", "methodology_unconfirmed"),
                ("all", "other_class_cohort")
            ]
        );
        let empty = db
            .community_index("community-fight", "KR")
            .unwrap()
            .unwrap();
        assert_eq!(empty["status"], "insufficient");
        assert_eq!(db.community_sources().unwrap()["imports"][0]["rows"], 4);
        assert!(db.delete_community("community", "patch-1").unwrap());
        assert!(!db.delete_community("community", "patch-1").unwrap());
        assert_eq!(
            db.community_index("community-fight", "EU")
                .unwrap()
                .unwrap()["status"],
            "insufficient"
        );
        assert!(
            db.community_sources().unwrap()["imports"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(db.community_index("missing", "EU").unwrap().is_none());
    }

    #[test]
    fn skill_index_medians_are_per_distinct_peer_and_scope_is_strict() {
        let db = Db::in_memory().unwrap();
        {
            let conn = db.conn.lock();
            let insert = |id: &str,
                          name: &str,
                          job: &str,
                          mob: i64,
                          cp: i64,
                          rate: f64,
                          limited: i64,
                          training: i64,
                          ms: i64,
                          self_flag: bool| {
                conn.execute(
                    "INSERT INTO fights(id,boss_name,mob_code,dungeon_id,started_at,duration_ms,is_train,numeric_limited)
                     VALUES (?1,'Kargos',?2,123,0,?3,?4,?5)",
                    params![id, mob, ms, training, limited],
                ).unwrap();
                conn.execute(
                    "INSERT INTO fight_players(fight_id,actor_id,name,job,damage,dps,combat_power,server_id,is_self)
                     VALUES (?1,1,?2,?3,10000,?4,?5,1304,?6)",
                    params![id, name, job, rate, cp, self_flag],
                ).unwrap();
            };
            insert("my", "Me", "검성", 77, 620_000, 260.0, 0, 0, 30_000, true);
            for (i, rate) in [100.0, 150.0, 200.0, 250.0, 300.0].iter().enumerate() {
                let id = format!("peer-{i}");
                let name = format!("Peer-{i}");
                let job = if i == 0 { "검성" } else { "치유성" };
                insert(&id, &name, job, 77, 620_000, *rate, 0, 0, 30_000, false);
            }
            for i in 0..8 {
                insert(
                    &format!("repeat-{i}"),
                    "Peer-4",
                    "치유성",
                    77,
                    620_000,
                    300.0,
                    0,
                    0,
                    30_000,
                    false,
                );
            }
            insert(
                "other-boss",
                "WrongBoss",
                "치유성",
                88,
                620_000,
                1.0,
                0,
                0,
                30_000,
                false,
            );
            insert(
                "other-cp",
                "WrongPower",
                "치유성",
                77,
                640_000,
                1.0,
                0,
                0,
                30_000,
                false,
            );
            insert(
                "limited",
                "Limited",
                "치유성",
                77,
                620_000,
                1.0,
                1,
                0,
                30_000,
                false,
            );
            insert(
                "train",
                "Training",
                "치유성",
                77,
                620_000,
                1.0,
                0,
                1,
                30_000,
                false,
            );
            insert(
                "short",
                "Short",
                "치유성",
                77,
                620_000,
                1.0,
                0,
                0,
                4_000,
                false,
            );
            insert(
                "no-peers", "Me", "검성", 99, 620_000, 260.0, 0, 0, 30_000, true,
            );
            insert("no-cp", "Me", "검성", 77, 0, 260.0, 0, 0, 30_000, true);
        }
        let result = db.skill_index("my").unwrap().unwrap();
        assert_eq!(result["status"], "ready");
        assert_eq!(result["source"], "local");
        assert_eq!(result["overall"]["peer_count"], 5);
        assert_eq!(result["overall"]["reference_dps"], 200.0);
        assert_eq!(result["overall"]["score"], 130.0);
        assert_eq!(result["same_class"]["status"], "insufficient");
        assert_eq!(result["same_class"]["peer_count"], 1);
        assert_eq!(
            db.skill_index("no-peers").unwrap().unwrap()["status"],
            "insufficient"
        );
        assert_eq!(
            db.skill_index("no-cp").unwrap().unwrap()["reason"],
            "missing_cp"
        );
        assert!(db.skill_index("missing").unwrap().is_none());
    }

    #[test]
    fn top_partners_counts_runs_and_skips_you() {
        let db = Db::in_memory().unwrap();
        db.note_my_character("Me", 1304, "치유성", 45, 0).unwrap();
        for (i, party) in [
            vec!["Me", "Anna", "Bob", "Cara"],
            vec!["Me", "Anna", "Bob", "Dan"],
            vec!["Me", "Anna", "Eve", "Fay"],
        ]
        .iter()
        .enumerate()
        {
            let run = db
                .start_run(600093, i as i64 * 1000, Some("Me"), 1304)
                .unwrap();
            let members: Vec<_> = party.iter().map(|n| member(n, *n == "Me")).collect();
            db.upsert_members(run, &members).unwrap();
            db.end_run(run, i as i64 * 1000 + 500).unwrap();
        }
        let top = db.top_partners(5, "").unwrap();
        let names: Vec<_> = top
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names[0], "Anna");
        assert_eq!(names[1], "Bob");
        assert_eq!(top[0]["runs"], 3);
        assert_eq!(top[0]["class_name"], "Gladiator");
        assert!(!names.contains(&"Me".to_string()));
        assert_eq!(names.len(), 5);
        assert_eq!(top[0]["favourite"], "Ferocious Horn Den");
    }

    fn record(id: &str, start: i64, my_dmg: i32) -> FightRecord {
        serde_json::from_value(json!({
            "id": id, "bossName": "Kargos", "targetId": 9, "startTimeMs": start, "durationMs": 10_000,
            "totalDamage": my_dmg + 500, "jobs": [], "dungeonId": 600093,
            "details": { "targetId": 9, "totalTargetDamage": my_dmg + 500, "battleTime": 10_000, "skills": [
                { "actorId": 1, "code": 11, "name": "Hieb", "time": 4, "dmg": my_dmg, "multiHitCount": 0,
                  "multiHitDamage": 0, "crit": 1, "parry": 1, "back": 2, "perfect": 0, "double": 0,
                  "smite": 0, "powershard": 0, "regen": 0, "hitTimestamps": [1, 2] },
                { "actorId": 2, "code": 12, "name": "Stich", "time": 2, "dmg": 500, "multiHitCount": 0,
                  "multiHitDamage": 0, "crit": 0, "parry": 0, "back": 0, "perfect": 0, "double": 0,
                  "smite": 0, "powershard": 0, "regen": 0 }
            ]},
            "actors": [
                { "actorId": 1, "nickname": "Me", "job": "치유성", "damageReceived": 300, "hitsReceived": 3 },
                { "actorId": 2, "nickname": "Anna", "job": "검성", "dbid": 5 }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn masked_partner_names_from_older_saves_are_repaired_once() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600093, 0, Some("Me"), 0).unwrap();
        db.upsert_members(
            run,
            &[
                member("Me", true),
                member("Moonlight", false),
                member("Moonbeam", false),
                member("Mo****t", false),
            ],
        )
        .unwrap();
        let mut rec = record("f", 1000, 100);
        rec.actors[1].nickname = "Mo****t".into();
        db.save_fights(&[rec], 0, &["Me".into()], None, &HashSet::new())
            .unwrap();
        // A mask that matches no roster name stays untouched.
        db.upsert_members(run, &[member("Xy****m", false)]).unwrap();
        let conn = db.conn.lock();
        conn.execute("DELETE FROM meta WHERE key = 'repair_masked_names_v1'", [])
            .unwrap();
        repair_masked_names_once(&conn).unwrap();
        let members: Vec<String> = conn
            .prepare("SELECT name FROM run_members WHERE run_id = ?1 ORDER BY name")
            .unwrap()
            .query_map(params![run], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(members, ["Me", "Moonbeam", "Moonlight", "Xy****m"]);
        let player: String = conn
            .query_row(
                "SELECT name FROM fight_players WHERE fight_id = 'f' AND actor_id = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(player, "Moonlight");
        drop(conn);
        let top: Vec<_> = db
            .top_partners(5, "")
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_string())
            .collect();
        assert!(!top.iter().any(|n| n == "Mo****t"));
    }

    #[test]
    fn run_list_dps_ignores_training_dummies_like_the_fight_count() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600093, 0, Some("Me"), 0).unwrap();
        let boss = record("boss", 1000, 100_000); // 10 s: 10 000 DPS
        let mut dummy = record("dummy", 20_000, 1_000_000);
        dummy.is_train = true;
        db.save_fights(&[boss, dummy], 0, &["Me".into()], None, &HashSet::new())
            .unwrap();
        let list = db.list_runs(10, 0, None, "", false).unwrap();
        let row = list["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == run)
            .unwrap();
        assert_eq!(row["fights"], 1);
        assert_eq!(row["my_dps"], 10_000.0);
    }

    #[test]
    fn short_fights_use_one_second_and_run_rates_share_the_same_window() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600093, 0, Some("Me"), 0).unwrap();
        let mut first = record("short", 1000, 100);
        first.duration_ms = 500;
        first.details.skills[1].dmg = 200;
        let mut second = record("long", 10_000, 900);
        second.duration_ms = 9_000;
        second.details.skills.retain(|s| s.actor_id == 1);
        second.actors.retain(|s| s.actor_id == 1);
        db.save_fights(&[first, second], 0, &["Me".into()], None, &HashSet::new())
            .unwrap();
        let f = db.fight_detail("short").unwrap().unwrap();
        assert_eq!(f["target_id"], 9);
        assert_eq!(f["total_damage"], 300);
        let me = f["players"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "Me")
            .unwrap();
        assert_eq!(me["dps"], 100.0);
        assert_eq!(me["skills"][0]["dps"], 100.0);
        assert_eq!(me["skills"][0]["average"], 25.0);
        assert!((me["share"].as_f64().unwrap() - 100.0 / 3.0).abs() < 1e-9);
        let r = db.run_detail(run).unwrap().unwrap();
        assert_eq!(r["totals"][0]["damage"], 1000);
        assert_eq!(r["totals"][0]["dps"], 100.0);
        assert_eq!(r["totals"][1]["damage"], 200);
        assert_eq!(
            r["totals"][1]["dps"], 20.0,
            "late/absent players share the ten-second run window"
        );
    }
    #[test]
    fn large_multi_skill_totals_are_64_bit_and_quality_needs_valid_hits() {
        let db = Db::in_memory().unwrap();
        let mut f = record("large", 1000, 1_500_000_000);
        f.details.skills[1].dmg = 1_500_000_000;
        f.details.skills[0].time = 0;
        db.save_fights(&[f], 0, &[], None, &HashSet::new()).unwrap();
        let f = db.fight_detail("large").unwrap().unwrap();
        assert_eq!(f["total_damage"], 3_000_000_000i64);
        assert!(f["players"][0]["skills"][0]["average"].is_null());
        assert!(f["players"][0]["skills"][0]["crit_rate"].is_null());
    }
    #[test]
    fn member_names_do_not_use_delimiters_and_active_runs_cannot_be_deleted() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600093, 0, Some("Me"), 0).unwrap();
        db.upsert_members(run, &[member("名;|\"<>&", false)])
            .unwrap();
        assert_eq!(
            db.list_runs(10, 0, None, "", false).unwrap()["runs"][0]["members"][0]["name"],
            "名;|\"<>&"
        );
        assert!(!db.delete_run(run).unwrap());
        assert!(db.run_detail(run).unwrap().is_some());
        db.end_run(run, 10).unwrap();
        assert!(db.delete_run(run).unwrap());
        assert!(!db.set_run_note(run, "missing").unwrap());
    }
    #[test]
    fn activity_history_keeps_same_named_targets_and_unknown_areas_separate() {
        let db = Db::in_memory().unwrap();
        db.set_activity_check(|id, code| {
            if id != 0 {
                names::dungeon_activity(id).unwrap_or("unclassified").into()
            } else if code == 100 {
                "field_boss".into()
            } else if code == 200 {
                "open_world".into()
            } else {
                "unclassified".into()
            }
        });
        let mut boss = record("field", 1000, 1000);
        boss.dungeon_id = 0;
        boss.mob_code = 100;
        let mut mob = record("world", 2000, 2000);
        mob.dungeon_id = 0;
        mob.mob_code = 200;
        let mut unknown = record("unknown", 3000, 3000);
        unknown.dungeon_id = 600999;
        db.save_fights(
            &[boss, mob, unknown],
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();
        let groups = db.boss_history("Me").unwrap();
        let groups = groups.as_array().unwrap();
        assert_eq!(groups.len(), 3);
        for (code, activity) in [
            (100, "field_boss"),
            (200, "open_world"),
            (0, "unclassified"),
        ] {
            let group = groups.iter().find(|g| g["mob_code"] == code).unwrap();
            assert_eq!(group["activity"], activity);
            assert_eq!(group["attempts"].as_array().unwrap().len(), 1);
        }
        assert!(
            db.boss_history("Alt")
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn legacy_missing_area_never_collapses_into_explicit_open_world() {
        let db = Db::in_memory().unwrap();
        db.set_activity_check(|id, _| {
            if id == 0 {
                "field_boss"
            } else {
                "unclassified"
            }
            .into()
        });
        let mut world = record("world", 1000, 1000);
        world.dungeon_id = 0;
        world.mob_code = 100;
        let mut legacy = world.clone();
        legacy.id = "legacy".into();
        db.save_fights(&[world, legacy], 0, &["Me".into()], None, &HashSet::new())
            .unwrap();
        db.conn
            .lock()
            .execute("UPDATE fights SET dungeon_id=NULL WHERE id='legacy'", [])
            .unwrap();
        let history = db.boss_history("Me").unwrap();
        let groups = history.as_array().unwrap();
        assert_eq!(groups.len(), 2);
        let legacy = groups.iter().find(|g| g["dungeon_id"].is_null()).unwrap();
        assert_eq!(legacy["activity"], "unclassified");
        assert_eq!(legacy["attempts"][0]["fight_id"], "legacy");
        assert_eq!(
            groups.iter().find(|g| g["dungeon_id"] == 0).unwrap()["activity"],
            "field_boss"
        );
    }

    #[test]
    fn run_history_keeps_tiers_repeated_attempts_and_zero_damage() {
        let db = Db::in_memory().unwrap();
        for (n, area) in [600092, 600093, 900001].into_iter().enumerate() {
            let start = n as i64 * 100_000;
            let run = db.start_run(area, start, Some("Me"), 0).unwrap();
            let mut first = record(&format!("{n}-first"), start + 1000, 0);
            first.dungeon_id = area;
            first.duration_ms = 0;
            first.mob_code = 100;
            let mut retry = record(&format!("{n}-retry"), start + 2000, 2000);
            retry.dungeon_id = area;
            retry.duration_ms = 200;
            retry.mob_code = 100;
            let mut absent = record(&format!("{n}-absent"), start + 3000, 9000);
            absent.dungeon_id = area;
            absent.actors[0].nickname = "Other".into();
            // Saving order is not boss progression, and repeated targets are separate attempts.
            db.save_fights(
                &[absent, retry, first],
                0,
                &["Me".into()],
                None,
                &HashSet::new(),
            )
            .unwrap();
            db.end_run(run, start + 20_000).unwrap();
        }
        let history = db.run_history("Me").unwrap();
        assert_eq!(history.as_array().unwrap().len(), 3);
        for group in history.as_array().unwrap() {
            let point = &group["attempts"][0];
            assert_eq!(point["fight_count"], 3);
            assert_eq!(point["damage"], 2000);
            assert_eq!(point["duration_ms"], 12000);
            assert!((point["dps"].as_f64().unwrap() - 2000.0 / 12.0).abs() < 0.001);
        }
        assert_eq!(history[0]["difficulty"], "Normal");
        assert_eq!(history[1]["difficulty"], "Schwer");
        assert!(history[2]["difficulty"].is_null());
        assert_eq!(history[2]["activity"], "unclassified");
        let bosses = db.boss_history("Me").unwrap();
        for group in bosses.as_array().unwrap() {
            assert_eq!(group["attempts"].as_array().unwrap().len(), 2);
            assert_eq!(group["attempts"][0]["dps"], 0.0);
        }
    }

    #[test]
    fn run_analysis_decodes_all_saved_fights_without_training_or_other_runs() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600093, 0, Some("Me"), 0).unwrap();
        let first = record("first", 1000, 1000);
        let second = record("second", 20_000, 2000);
        let mut training = record("dummy", 40_000, 1_000_000);
        training.is_train = true;
        db.save_fights(
            &[first, second, training],
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();
        db.save_analytics(
            "first",
            &json!({"resolution_ms":500,"points":[{"ms":500,"damage":{"1":1000}}]}),
        )
        .unwrap();
        db.end_run(run, 60_000).unwrap();
        let other = db.start_run(600092, 100_000, Some("Me"), 0).unwrap();
        let mut elsewhere = record("elsewhere", 101_000, 50_000);
        elsewhere.dungeon_id = 600092;
        db.save_fights(&[elsewhere], 0, &["Me".into()], None, &HashSet::new())
            .unwrap();
        db.end_run(other, 120_000).unwrap();
        let analysis = db.run_analysis(run).unwrap().unwrap();
        assert_eq!(analysis["id"], run);
        let fights = analysis["fights"].as_array().unwrap();
        assert_eq!(fights.len(), 2);
        assert_eq!(fights[0]["id"], "first");
        assert_eq!(fights[1]["id"], "second");
        assert_eq!(fights[0]["analytics"]["points"][0]["damage"]["1"], 1000);
        let own = fights[0]["players"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["is_self"] == 1)
            .unwrap();
        assert_eq!(own["skills"][0]["damage"], 1000);
        assert!(own.as_object().unwrap().contains_key("server_id"));
        assert!(db.run_analysis(999999).unwrap().is_none());
        db.conn
            .lock()
            .execute("UPDATE fights SET record_json=NULL WHERE id='second'", [])
            .unwrap();
        let legacy = db.run_analysis(run).unwrap().unwrap();
        assert!(legacy["fights"][1]["players"][0]["skills"].is_null());
        assert_eq!(legacy["fights"][1]["players"][0]["damage"], 2000);
    }

    #[test]
    fn run_history_matches_shared_windows_excludes_training_and_active_runs() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600093, 0, Some("Me"), 0).unwrap();
        let first = record("first", 1000, 100_000);
        let mut second = record("second", 20_000, 900_000);
        second.duration_ms = 30_000;
        let mut absent = record("absent", 60_000, 10_000);
        absent.actors[0].nickname = "Other".into();
        let mut dummy = record("dummy", 80_000, 1_000_000);
        dummy.is_train = true;
        db.save_fights(
            &[first, second, absent, dummy],
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();
        assert!(db.run_history("Me").unwrap().as_array().unwrap().is_empty());
        db.end_run(run, 100_000).unwrap();
        let history = db.run_history("Me").unwrap();
        let group = &history[0];
        assert_eq!(group["activity"], "expedition");
        assert_eq!(group["scope"], "run");
        let point = &group["attempts"][0];
        assert_eq!(point["damage"], 1_000_000);
        assert_eq!(point["duration_ms"], 50_000);
        assert_eq!(point["fight_count"], 3);
        assert_eq!(point["dps"], 20_000.0);
        assert_eq!(point["numeric_limited"], 0);
        let detail = db.run_detail(run).unwrap().unwrap();
        let me = detail["totals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["is_self"] == 1)
            .unwrap();
        assert_eq!(point["dps"], me["dps"]);
        assert!(
            db.run_history("Alt")
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );
        db.conn
            .lock()
            .execute(
                "UPDATE fights SET numeric_limited=NULL WHERE id='second'",
                [],
            )
            .unwrap();
        assert!(db.run_history("Me").unwrap()[0]["attempts"][0]["numeric_limited"].is_null());
    }

    #[test]
    fn boss_trends_keep_difficulty_separate_and_do_not_claim_kills() {
        let db = Db::in_memory().unwrap();
        let mut other = record("b", 10000, 1000);
        other.dungeon_id = 600092;
        db.save_fights(
            &[record("a", 1000, 500), other],
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();
        let history = db.boss_history("").unwrap();
        assert_eq!(history.as_array().unwrap().len(), 2);
        for b in history.as_array().unwrap() {
            assert!(b["kills"].is_null());
            assert_eq!(b["attempts"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn attempt_ending_keeps_kills_and_absent_quality_is_not_a_zero_measurement() {
        let db = Db::in_memory().unwrap();
        db.save_fights(
            &[record("a", 1000, 1500)],
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();
        db.save_analytics("a", &json!({"outcome":"kill"})).unwrap();
        db.finish_attempts(&["a".into()], "wipe").unwrap();
        let f = db.fight_detail("a").unwrap().unwrap();
        assert_eq!(f["analytics"]["outcome"], "kill");
        let rows = &f["players"][0]["skills"];
        assert!(rows[0]["perfect_rate"].is_null());
        assert!(rows[0]["miss_count"].is_null());
        db.finish_attempts(&["a".into()], "manual").unwrap();
        assert_eq!(
            db.fight_detail("a").unwrap().unwrap()["analytics"]["end_reason"],
            "wipe"
        );
    }
    #[test]
    fn fights_keep_deaths_damage_taken_and_feed_the_boss_history() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600093, 0, Some("Me"), 0).unwrap();
        let dead: HashSet<i32> = [1].into();
        db.save_fights(&[record("a", 1_000, 1_500)], 0, &["Me".into()], None, &dead)
            .unwrap();
        db.save_fights(
            &[record("b", 50_000, 3_000)],
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();

        let fight = db.fight_detail("a").unwrap().unwrap();
        let me = &fight["players"][0];
        assert_eq!(me["name"], "Me");
        assert_eq!(me["died"], 1);
        assert_eq!(me["damage_received"], 300);
        assert_eq!(me["hits_received"], 3);
        assert_eq!(me["skills"][0]["parry_rate"], 25.0);
        assert_eq!(fight["run_id"], run);

        let detail = db.run_detail(run).unwrap().unwrap();
        assert_eq!(
            detail["members"].as_array().unwrap().len(),
            2,
            "fighters join the run"
        );
        assert_eq!(detail["totals"][0]["deaths"], 1);

        let history = db.boss_history("").unwrap();
        assert_eq!(history[0]["boss"], "Kargos");
        let kills = history[0]["attempts"].as_array().unwrap();
        assert_eq!(kills.len(), 2);
        assert_eq!(kills[0]["dps"], 150.0);
        assert_eq!(kills[1]["dps"], 300.0);
        assert_eq!(kills[0]["difficulty"], "Schwer");
        assert_eq!(db.summary("").unwrap()["my_deaths"], 1);
    }

    #[test]
    fn boss_history_marks_capped_or_unverifiable_measurements() {
        let db = Db::in_memory().unwrap();
        let normal = record("normal", 1000, 100);
        let mut limited = record("limited", 2000, 100);
        limited.details.skills[0].dmg = i32::MAX;
        let mut healing = record("healing", 3000, 100);
        let mut heal_skill = healing.details.skills[0].clone();
        heal_skill.dmg = i32::MAX;
        healing.details.heal_skills.push(heal_skill);
        db.save_fights(
            &[normal, limited, healing],
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();
        let history = db.boss_history("Me").unwrap();
        let attempts = history[0]["attempts"].as_array().unwrap();
        assert_eq!(attempts[0]["numeric_limited"], 0);
        assert_eq!(attempts[1]["numeric_limited"], 1);
        assert_eq!(attempts[2]["numeric_limited"], 1);
        // History reads the stored flag instead of parsing every record again.
        db.conn
            .lock()
            .execute("UPDATE fights SET record_json = '{}'", [])
            .unwrap();
        let history = db.boss_history("Me").unwrap();
        assert_eq!(history[0]["attempts"][1]["numeric_limited"], 1);
        // Databases from before the column: derived once, unverifiable stays NULL.
        let conn = db.conn.lock();
        conn.execute_batch(
            "UPDATE fights SET numeric_limited = NULL;
             UPDATE fights SET record_json = NULL WHERE id = 'normal';
             UPDATE fights SET record_json = '{\"details\":{\"skills\":[{\"dmg\":2147483647}]}}' WHERE id = 'limited';
             UPDATE fights SET record_json = '{\"details\":{\"skills\":[{\"dmg\":5}]}}' WHERE id = 'healing';",
        )
        .unwrap();
        super::migrate(&conn).unwrap();
        drop(conn);
        let history = db.boss_history("Me").unwrap();
        let attempts = history[0]["attempts"].as_array().unwrap();
        assert!(attempts[0]["numeric_limited"].is_null());
        assert_eq!(attempts[1]["numeric_limited"], 1);
        assert_eq!(attempts[2]["numeric_limited"], 0);
    }

    #[test]
    fn training_deaths_do_not_count_as_boss_deaths() {
        let db = Db::in_memory().unwrap();
        let mut training = record("training", 1000, 500);
        training.is_train = true;
        db.save_fights(&[training], 0, &["Me".into()], None, &[1].into())
            .unwrap();
        assert_eq!(db.summary("Me").unwrap()["my_deaths"], 0);
        assert_eq!(db.summary("").unwrap()["fights"], 0);
        db.save_fights(
            &[record("boss", 2000, 500)],
            0,
            &["Me".into()],
            None,
            &[1].into(),
        )
        .unwrap();
        assert_eq!(db.summary("Me").unwrap()["my_deaths"], 1);
        assert_eq!(db.summary("Other").unwrap()["my_deaths"], 0);
    }

    #[test]
    fn settings_and_late_boss_names() {
        let db = Db::in_memory().unwrap();
        assert_eq!(db.meta("x").unwrap(), None);
        db.set_meta("x", "1").unwrap();
        db.set_meta("x", "0").unwrap();
        assert_eq!(db.meta("x").unwrap().as_deref(), Some("0"));

        let mut unnamed = record("u", 1_000, 100);
        unnamed.boss_name.clear();
        unnamed.mob_code = 2_400_001;
        let mut unknown = record("k", 2_000, 100);
        unknown.boss_name.clear();
        db.save_fights(&[unnamed, unknown], 0, &[], None, &HashSet::new())
            .unwrap();
        let named = db
            .fill_missing_boss_names(|c| {
                if c == 2_400_001 {
                    "Kargos".into()
                } else {
                    String::new()
                }
            })
            .unwrap();
        assert_eq!(named, 1);
        assert_eq!(
            db.fight_detail("u").unwrap().unwrap()["boss_name"],
            "Kargos"
        );
        assert_eq!(db.fight_detail("k").unwrap().unwrap()["boss_name"], "");
    }

    #[test]
    fn everything_filters_by_character() {
        let db = Db::in_memory().unwrap();
        db.note_my_character("Main", 1304, "검성", 45, 10).unwrap();
        db.note_my_character("Twink", 1304, "치유성", 30, 20)
            .unwrap();
        let a = db.start_run(600093, 0, Some("Main"), 1304).unwrap();
        db.upsert_members(a, &[member("Main", true), member("Anna", false)])
            .unwrap();
        let b = db.start_run(600092, 1_000, Some("Twink"), 1304).unwrap();
        db.upsert_members(b, &[member("Twink", true), member("Bob", false)])
            .unwrap();
        // A character only seen on a run still shows up.
        let c = db.start_run(600092, 2_000, Some("Alt"), 1304).unwrap();
        db.upsert_members(c, &[member("Alt", true), member("Bob", false)])
            .unwrap();

        assert_eq!(db.list_runs(10, 0, None, "", false).unwrap()["total"], 3);
        let first = db.list_runs(10, 0, None, "", false).unwrap()["runs"][0]["id"]
            .as_i64()
            .unwrap();
        assert!(db.set_run_favorite(first, true).unwrap());
        assert!(!db.set_run_favorite(-1, true).unwrap());
        let favorites = db.list_runs(10, 0, None, "", true).unwrap();
        assert_eq!(favorites["total"], 1);
        assert_eq!(favorites["runs"][0]["id"], first);
        assert_eq!(favorites["runs"][0]["favorite"], 1);
        assert_eq!(
            db.list_runs(10, 0, None, "Main", false).unwrap()["total"],
            1
        );
        assert_eq!(
            db.list_runs(10, 0, None, "Twink", false).unwrap()["dungeons"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let names = |c: &str| -> Vec<String> {
            db.top_partners(5, c)
                .unwrap()
                .iter()
                .map(|r| r["name"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(names("Main"), vec!["Anna"]);
        assert_eq!(names("Twink"), vec!["Bob"]);
        assert_eq!(names(""), vec!["Bob", "Anna"]);
        assert_eq!(db.summary("Twink").unwrap()["runs"], 1);
        let chars: Vec<String> = db
            .characters()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(chars, vec!["Alt", "Twink", "Main"]);
    }

    #[test]
    fn effects_show_on_their_fighter_and_the_boss() {
        let db = Db::in_memory().unwrap();
        let fight = record("f1", 1_000, 500);
        db.save_fights(std::slice::from_ref(&fight), 0, &[], None, &HashSet::new())
            .unwrap();
        let row = |entity_id, code, on_boss| EffectRow {
            fight_id: "f1".into(),
            entity_id,
            code,
            name: format!("E{code}"),
            caster_id: 1,
            uptime: 50.0,
            on_boss,
        };
        db.save_effects(
            std::slice::from_ref(&fight),
            &[row(1, 10, false), row(fight.target_id, 20, true)],
        )
        .unwrap();
        // Saving again replaces, not duplicates.
        db.save_effects(
            std::slice::from_ref(&fight),
            &[row(1, 10, false), row(fight.target_id, 20, true)],
        )
        .unwrap();
        let detail = db.fight_detail("f1").unwrap().unwrap();
        assert_eq!(detail["boss_debuffs"].as_array().unwrap().len(), 1);
        assert_eq!(detail["boss_debuffs"][0]["name"], "E20");
        let me = detail["players"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["actor_id"] == 1)
            .unwrap();
        assert_eq!(me["buffs"][0]["code"], 10);
        assert_eq!(me["buffs"][0]["caster"], me["name"]);
    }

    #[test]
    fn a_database_from_the_first_release_gains_the_new_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE fight_players (fight_id TEXT NOT NULL, actor_id INTEGER NOT NULL, name TEXT,
             PRIMARY KEY (fight_id, actor_id));",
        )
        .unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        conn.execute("INSERT INTO fight_players(fight_id, actor_id, died, hits_received) VALUES ('x', 1, 1, 2)", [])
            .unwrap();
    }

    #[test]
    fn an_empty_solo_run_is_not_kept() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600001, 0, Some("Me"), 0).unwrap();
        db.upsert_members(run, &[member("Me", true)]).unwrap();
        db.end_run(run, 10).unwrap();
        assert_eq!(db.list_runs(10, 0, None, "", false).unwrap()["total"], 0);
    }

    #[test]
    fn runs_list_and_summary() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600092, 0, Some("Me"), 1304).unwrap();
        db.upsert_members(run, &[member("Me", true), member("Anna", false)])
            .unwrap();
        db.end_run(run, 60_000).unwrap();
        let list = db.list_runs(10, 0, None, "", false).unwrap();
        assert_eq!(list["total"], 1);
        assert_eq!(list["runs"][0]["difficulty"], "Normal");
        assert_eq!(list["runs"][0]["members"].as_array().unwrap().len(), 2);
        let s = db.summary("").unwrap();
        assert_eq!(s["runs"], 1);
        assert_eq!(s["play_ms"], 60_000);
        assert!(db.run_detail(run).unwrap().is_some());
        db.delete_run(run).unwrap();
        assert!(db.run_detail(run).unwrap().is_none());
    }
    #[test]
    fn resaving_preserves_annotations_analytics_and_heal_is_not_counted_twice() {
        let db = Db::in_memory().unwrap();
        let mut f = record("qol", 1000, 1500);
        f.actors[0].party_heal = 300;
        let mut healing = f.details.skills[0].clone();
        healing.dmg = 300;
        f.details.heal_skills = vec![healing];
        db.save_fights(
            std::slice::from_ref(&f),
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();
        assert!(
            db.annotate("qol", true, "Neue Ausrüstung", "rotation")
                .unwrap()
        );
        db.save_analytics("qol", &json!({"points":[],"partial":true}))
            .unwrap();
        db.save_fights(
            std::slice::from_ref(&f),
            0,
            &["Me".into()],
            None,
            &HashSet::new(),
        )
        .unwrap();
        let detail = db.fight_detail("qol").unwrap().unwrap();
        assert_eq!(detail["players"][0]["heal"], 300);
        assert_eq!(detail["players"][0]["hps"], 30.0);
        assert_eq!(
            detail["players"][0]["skills"][0]["hit_timestamps"],
            json!([1, 2])
        );
        assert_eq!(detail["favorite"], true);
        assert_eq!(detail["analytics"]["partial"], true);
        assert_eq!(
            db.search_fights(&FightSearch {
                query: "rotation".into(),
                character: "Me".into(),
                from: 0,
                to: 2000,
                favorites: true,
                limit: 100,
                ..Default::default()
            })
            .unwrap()["fights"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            db.search_fights(&FightSearch {
                query: "rotation".into(),
                character: "Other".into(),
                from: 0,
                to: 2000,
                favorites: true,
                limit: 100,
                ..Default::default()
            })
            .unwrap()["fights"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            db.search_fights(&FightSearch {
                query: "".into(),
                character: "Me".into(),
                from: 2000,
                to: 3000,
                favorites: false,
                limit: 100,
                ..Default::default()
            })
            .unwrap()["fights"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.search_fights(&FightSearch {
                query: "AUSRÜSTUNG".into(),
                character: "".into(),
                from: 0,
                to: 2000,
                favorites: false,
                limit: 100,
                ..Default::default()
            })
            .unwrap()["fights"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(!db.annotate("missing", true, "", "").unwrap());
    }

    #[test]
    fn fight_library_separates_world_mobs_and_pages() {
        let db = Db::in_memory().unwrap();
        let mut fights = Vec::new();
        for (id, code, start) in [("boss", 7, 1000), ("mob1", 8, 2000), ("mob2", 8, 3000)] {
            let mut f = record(id, start, 1500);
            f.mob_code = code;
            fights.push(f);
        }
        db.save_fights(&fights, 0, &["Me".into()], None, &HashSet::new())
            .unwrap();
        db.set_world_mob_check(|code| code == 8);
        let ids = |kind, limit, offset| {
            let found = db
                .search_fights(&FightSearch {
                    to: i64::MAX,
                    kind,
                    limit,
                    offset,
                    ..Default::default()
                })
                .unwrap();
            let ids: Vec<String> = found["fights"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["id"].as_str().unwrap().to_string())
                .collect();
            (ids, found["more"].as_bool().unwrap())
        };
        assert_eq!(ids(FightKind::Boss, 10, 0), (vec!["boss".into()], false));
        assert_eq!(ids(FightKind::Mob, 1, 0), (vec!["mob2".into()], true));
        assert_eq!(ids(FightKind::Mob, 1, 1), (vec!["mob1".into()], false));
        assert_eq!(ids(FightKind::All, 10, 0).0.len(), 3);
    }

    #[test]
    fn world_mobs_stay_out_of_boss_summary_and_fights_name_their_dungeon() {
        let db = Db::in_memory().unwrap();
        let mut boss = record("boss", 1000, 1500);
        boss.mob_code = 7;
        boss.dungeon_id = 600072;
        db.save_fights(&[boss], 600072, &["Me".into()], None, &HashSet::new())
            .unwrap();
        let mut mob = record("mob", 2000, 1500);
        mob.mob_code = 8;
        mob.boss_name = "Wolf".into();
        mob.dungeon_id = 0;
        db.save_fights(&[mob], 0, &["Me".into()], None, &HashSet::new())
            .unwrap();
        db.set_world_mob_check(|code| code == 8);
        let summary = db.summary("").unwrap();
        assert_eq!(summary["fights"], 1);
        let best = summary["my_best"].as_array().unwrap();
        assert_eq!(best.len(), 1);
        assert_eq!(best[0]["boss_name"], "Kargos");
        let found = db
            .search_fights(&FightSearch {
                to: i64::MAX,
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        let place = |id: &str| {
            found["fights"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["id"] == id)
                .unwrap()["dungeon_name"]
                .clone()
        };
        assert_eq!(place("boss"), json!(names::dungeon_name(600072)));
        assert!(place("boss").is_string());
        assert_eq!(place("mob"), Value::Null);
    }
    #[test]
    fn healing_only_actor_is_retained_and_old_records_are_readable() {
        let db = Db::in_memory().unwrap();
        let mut f = record("healer", 1000, 500);
        let mut healing = f.details.skills[0].clone();
        healing.actor_id = 3;
        healing.dmg = 100;
        f.details.heal_skills = vec![healing];
        db.save_fights(&[f], 0, &[], None, &HashSet::new()).unwrap();
        let d = db.fight_detail("healer").unwrap().unwrap();
        assert!(
            d["players"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["actor_id"] == 3 && p["heal"] == 100)
        );
    }
}
