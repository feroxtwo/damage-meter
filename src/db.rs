//! The run history: every dungeon run, its party and its boss fights, in SQLite.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
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

/// Columns added after the first release, for databases created by it.
const MIGRATIONS: &[(&str, &str, &str)] = &[
    (
        "fight_players",
        "hits_received",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("fight_players", "died", "INTEGER NOT NULL DEFAULT 0"),
];

fn migrate(conn: &Connection) -> Result<()> {
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

pub fn skill_rows(
    skills: &[a2tools_dps_meter_lib::entity::details_context::DetailSkillEntry],
    actor: i32,
) -> Value {
    let mut rows: Vec<Value> = skills.iter().filter(|s|s.actor_id==actor).map(|s| {
        let hits=s.time.max(1) as f64;
        json!({"code":s.code,"name":if s.name.is_empty(){format!("#{}",s.code)}else{s.name.clone()},
          "damage":s.dmg,"hits":s.time,"crit_rate":s.crit as f64*100.0/hits,
          "back_rate":s.back as f64*100.0/hits,"frontal_rate":s.frontal as f64*100.0/hits,
          "perfect_rate":s.perfect as f64*100.0/hits,"parry_rate":s.parry as f64*100.0/hits,
          "double_rate":s.double as f64*100.0/hits,"multi_hit_count":s.multi_hit_count,
          "multi_hit_damage":s.multi_hit_damage,"max":s.max_dmg,"min":s.min_dmg,
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
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
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
    /// under way when the fight started.
    fn run_for_fight(conn: &Connection, dungeon_id: i32, started_at: i64) -> Result<Option<i64>> {
        if dungeon_id <= 0 {
            return Ok(None);
        }
        Ok(conn
            .query_row(
                "SELECT id FROM runs WHERE dungeon_id = ?1 AND started_at <= ?2 + 60000
                   AND (ended_at IS NULL OR ended_at >= ?2)
                 ORDER BY started_at DESC LIMIT 1",
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
            let total: i64 = damage.values().sum::<i64>().max(1);
            let secs = (record.duration_ms as f64 / 1000.0).max(1.0);

            tx.execute(
                "INSERT INTO fights(id, run_id, boss_name, mob_code, target_id, dungeon_id, started_at,
                                               duration_ms, total_damage, max_hp, is_train, record_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET run_id=excluded.run_id, boss_name=excluded.boss_name,
                  mob_code=excluded.mob_code, target_id=excluded.target_id, dungeon_id=excluded.dungeon_id,
                  max_hp=excluded.max_hp,is_train=excluded.is_train,
                  duration_ms=excluded.duration_ms, total_damage=excluded.total_damage, record_json=excluded.record_json",
                params![
                    record.id,
                    run_id,
                    record.boss_name,
                    record.mob_code,
                    record.target_id,
                    dungeon_id,
                    record.start_time_ms,
                    record.duration_ms,
                    record.total_damage,
                    record.details.max_hp,
                    record.is_train,
                    serde_json::to_string(record)?
                ],
            )?;
            tx.execute(
                "DELETE FROM fight_players WHERE fight_id = ?1",
                params![record.id],
            )?;
            let mut actors = record.actors.clone();
            for skill in &record.details.heal_skills {
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
                        dmg as f64 * 100.0 / total as f64,
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

    pub fn delete_run(&self, run_id: i64) -> Result<()> {
        self.conn
            .lock()
            .execute("DELETE FROM runs WHERE id = ?1", params![run_id])?;
        Ok(())
    }

    pub fn set_run_note(&self, run_id: i64, note: &str) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE runs SET note = ?2 WHERE id = ?1",
            params![run_id, note],
        )?;
        Ok(())
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
    ) -> Result<Value> {
        let conn = self.conn.lock();
        let filter = dungeon.unwrap_or("");
        let mut stmt = conn.prepare(
            "SELECT r.id, r.dungeon_id, r.dungeon_name, r.difficulty, r.kind, r.started_at, r.ended_at,
                    r.character, r.note,
                    (SELECT COUNT(*) FROM fights f WHERE f.run_id = r.id AND f.is_train = 0) AS fights,
                    (SELECT GROUP_CONCAT(name || '|' || COALESCE(job, '') || '|' || is_self, ';')
                       FROM run_members m WHERE m.run_id = r.id) AS members,
                    (SELECT ROUND(AVG(fp.dps)) FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
                       WHERE f.run_id = r.id AND fp.is_self = 1) AS my_dps
             FROM runs r
             WHERE (?3 = '' OR r.dungeon_name = ?3) AND (?4 = '' OR r.character = ?4)
             ORDER BY r.started_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let mut rows = rows_to_json(&mut stmt, params![limit, offset, filter, character])?;
        for row in &mut rows {
            let members: Vec<Value> = row["members"]
                .as_str()
                .unwrap_or("")
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|m| {
                    let mut parts = m.splitn(3, '|');
                    let name = parts.next().unwrap_or("");
                    let job = parts.next().unwrap_or("");
                    let is_self = parts.next() == Some("1");
                    let info = names::class_info(job);
                    json!({ "name": name, "class_key": info.key, "class_name": info.name, "is_self": is_self })
                })
                .collect();
            row["members"] = Value::Array(members);
        }
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM runs WHERE (?1 = '' OR dungeon_name = ?1) AND (?2 = '' OR character = ?2)",
            params![filter, character],
            |r| r.get(0),
        )?;
        let dungeons: Vec<String> = conn
            .prepare("SELECT DISTINCT dungeon_name FROM runs WHERE (?1 = '' OR character = ?1) ORDER BY dungeon_name")?
            .query_map(params![character], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(json!({ "runs": rows, "total": total, "dungeons": dungeons }))
    }

    pub fn run_detail(&self, run_id: i64) -> Result<Option<Value>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT * FROM runs WHERE id = ?1")?;
        let Some(mut run) = rows_to_json(&mut stmt, params![run_id])?.into_iter().next() else {
            return Ok(None);
        };
        let mut stmt = conn.prepare(
            "SELECT name, job, server_id, level, gear_score, combat_power, is_self FROM run_members
             WHERE run_id = ?1 ORDER BY is_self DESC, name",
        )?;
        let mut members = rows_to_json(&mut stmt, params![run_id])?;
        decorate_job(&mut members);
        let mut stmt = conn.prepare(
            "SELECT id, boss_name, mob_code, started_at, duration_ms, total_damage, max_hp, is_train
             FROM fights WHERE run_id = ?1 ORDER BY started_at",
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
                    SUM(fp.damage) * 1000.0 / MAX(SUM(f.duration_ms), 1) AS dps, MAX(fp.is_self) AS is_self,
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
        Ok(Some(run))
    }

    pub fn fight_detail(&self, fight_id: &str) -> Result<Option<Value>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, run_id, boss_name, mob_code, dungeon_id, started_at, duration_ms, total_damage, max_hp,
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
            "SELECT actor_id, name, job, damage, dps, share, heal, damage_received, hits_received, died,
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
            for p in &mut players {
                let actor = p["actor_id"].as_i64().unwrap_or(0) as i32;
                p["skills"] = skill_rows(&record.details.skills, actor);
                p["heal_skills"] = skill_rows(&record.details.heal_skills, actor);
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

    pub fn save_analytics(&self, id: &str, data: &Value) -> Result<()> {
        self.conn.lock().execute("INSERT INTO fight_analytics(fight_id,data) VALUES(?1,?2) ON CONFLICT(fight_id) DO UPDATE SET data=excluded.data",params![id,data.to_string()])?;
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
    pub fn search_fights(
        &self,
        query: &str,
        character: &str,
        from: i64,
        to: i64,
        favorites: bool,
        offset: i64,
    ) -> Result<Value> {
        let conn = self.conn.lock();
        let sql="SELECT f.id,f.boss_name,f.dungeon_id,f.started_at,f.duration_ms,f.total_damage,f.is_train,
          COALESCE(a.favorite,0) AS favorite,COALESCE(a.note,'') AS note,COALESCE(a.tags,'') AS tags,
          (SELECT p.dps FROM fight_players p WHERE p.fight_id=f.id AND p.is_self=1 LIMIT 1) AS my_dps
          FROM fights f LEFT JOIN fight_annotations a ON a.fight_id=f.id
          WHERE (instr(lower(f.boss_name||' '||COALESCE(a.note,'')||' '||COALESCE(a.tags,'')),lower(?1))>0)
           AND (?2='' OR EXISTS(SELECT 1 FROM fight_players p WHERE p.fight_id=f.id AND p.is_self=1 AND p.name=?2))
           AND f.started_at>=?3 AND f.started_at<=?4 AND (?5=0 OR a.favorite=1)
          ORDER BY f.started_at DESC LIMIT 101 OFFSET ?6";
        let mut rows = rows_to_json(
            &mut conn.prepare(sql)?,
            params![query, character, from, to, favorites, offset],
        )?;
        let more = rows.len() > 100;
        rows.truncate(100);
        for r in &mut rows {
            r["difficulty"] = json!(names::dungeon_difficulty(
                r["dungeon_id"].as_i64().unwrap_or(0) as i32
            ));
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

    /// Your DPS on every kill of each boss, oldest first, for the trend chart.
    pub fn boss_history(&self, character: &str) -> Result<Value> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT f.boss_name AS boss, f.id AS fight_id, f.started_at, f.duration_ms, f.dungeon_id,
                    fp.dps, fp.share, fp.died, fp.job
             FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
             WHERE fp.is_self = 1 AND f.is_train = 0 AND f.boss_name <> '' AND (?1 = '' OR fp.name = ?1)
             ORDER BY f.started_at",
        )?;
        let rows = rows_to_json(&mut stmt, params![character])?;
        let mut bosses: Vec<(String, Vec<Value>)> = Vec::new();
        for mut row in rows {
            let boss = row["boss"].as_str().unwrap_or_default().to_string();
            let id = row["dungeon_id"].as_i64().unwrap_or(0) as i32;
            row["difficulty"] = json!(names::dungeon_difficulty(id));
            match bosses.iter_mut().find(|(b, _)| *b == boss) {
                Some((_, list)) => list.push(row),
                None => bosses.push((boss, vec![row])),
            }
        }
        bosses.sort_by_key(|(_, list)| std::cmp::Reverse(list.len()));
        Ok(Value::Array(
            bosses
                .into_iter()
                .map(|(boss, kills)| json!({ "boss": boss, "kills": kills }))
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
            "SELECT COUNT(*) FROM fights f WHERE is_train = 0 AND (?1 = '' OR EXISTS
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
                    MIN(CASE WHEN ended_at IS NOT NULL THEN ended_at - started_at END) AS fastest_ms
             FROM runs WHERE (?1 = '' OR character = ?1)
             GROUP BY dungeon_name, difficulty ORDER BY runs DESC",
        )?;
        let per_dungeon = rows_to_json(&mut stmt, params![c])?;
        let mut stmt = conn.prepare(
            "SELECT f.boss_name, MAX(fp.dps) AS best_dps, fp.job, COUNT(*) AS kills
             FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
             WHERE fp.is_self = 1 AND f.is_train = 0 AND (?1 = '' OR fp.name = ?1)
             GROUP BY f.boss_name ORDER BY best_dps DESC LIMIT 10",
        )?;
        let mut my_best = rows_to_json(&mut stmt, params![c])?;
        decorate_job(&mut my_best);
        let mut stmt = conn.prepare(
            "SELECT strftime('%Y-%m-%d', started_at / 1000, 'unixepoch', 'localtime') AS day, COUNT(*) AS runs
             FROM runs WHERE started_at >= (strftime('%s', 'now') - 30 * 86400) * 1000
               AND (?1 = '' OR character = ?1)
             GROUP BY day ORDER BY day",
        )?;
        let per_day = rows_to_json(&mut stmt, params![c])?;
        let my_deaths: i64 = conn.query_row(
            "SELECT COALESCE(SUM(fp.died), 0) FROM fight_players fp JOIN fights f ON f.id = fp.fight_id
             WHERE fp.is_self = 1 AND f.is_train = 0 AND (?1 = '' OR fp.name = ?1)",
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
        let kills = history[0]["kills"].as_array().unwrap();
        assert_eq!(kills.len(), 2);
        assert_eq!(kills[0]["dps"], 150.0);
        assert_eq!(kills[1]["dps"], 300.0);
        assert_eq!(kills[0]["difficulty"], "Schwer");
        assert_eq!(db.summary("").unwrap()["my_deaths"], 1);
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

        assert_eq!(db.list_runs(10, 0, None, "").unwrap()["total"], 3);
        assert_eq!(db.list_runs(10, 0, None, "Main").unwrap()["total"], 1);
        assert_eq!(
            db.list_runs(10, 0, None, "Twink").unwrap()["dungeons"]
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
        assert_eq!(db.list_runs(10, 0, None, "").unwrap()["total"], 0);
    }

    #[test]
    fn runs_list_and_summary() {
        let db = Db::in_memory().unwrap();
        let run = db.start_run(600092, 0, Some("Me"), 1304).unwrap();
        db.upsert_members(run, &[member("Me", true), member("Anna", false)])
            .unwrap();
        db.end_run(run, 60_000).unwrap();
        let list = db.list_runs(10, 0, None, "").unwrap();
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
        assert!(db.annotate("qol", true, "new gear", "rotation").unwrap());
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
            db.search_fights("rotation", "Me", 0, 2000, true, 0)
                .unwrap()["fights"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            db.search_fights("rotation", "Other", 0, 2000, true, 0)
                .unwrap()["fights"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            db.search_fights("", "Me", 2000, 3000, false, 0).unwrap()["fights"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(!db.annotate("missing", true, "", "").unwrap());
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
