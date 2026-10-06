//! The meter's state: the parser's combat store, the live numbers the overlay
//! and web app show, and the run tracking that fills the database.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, RwLock};
use serde::Serialize;

use a2tools_dps_meter_lib::combat::data_storage::DataStorage;
use a2tools_dps_meter_lib::combat::dps_calculator::DpsCalculator;
use a2tools_dps_meter_lib::combat::ping_tracker::PingTracker;
use a2tools_dps_meter_lib::entity::dps_data::DpsData;
use a2tools_dps_meter_lib::entity::fight_record::FightRecord;
use a2tools_dps_meter_lib::i18n::lookup::{NpcLookup, SkillLookup};

use crate::buffs::{self, BuffTracker};
use crate::db::{Db, EffectRow, Member};
use crate::names;

const TICK: Duration = Duration::from_millis(500);
/// Boss fights are saved this often while they run, and again when they end.
const SAVE_EVERY_TICKS: u64 = 60;
/// Effects below this uptime in a fight are not kept.
const MIN_UPTIME_PERCENT: f64 = 3.0;
const MAX_EFFECTS_PER_ENTITY: usize = 16;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LiveRow {
    pub id: i32,
    pub name: String,
    pub class_key: &'static str,
    pub class_name: &'static str,
    pub color: [u8; 3],
    pub damage: f64,
    pub dps: f64,
    pub share: f64,
    pub combat_power: i64,
    pub is_self: bool,
    /// Died in the current fight (the game's combat-death event).
    pub dead: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CaptureStatus {
    pub permission: bool,
    pub game_running: bool,
    pub locked_port: Option<u16>,
    pub device: Option<String>,
    pub error: Option<String>,
    /// The packet recording being written, if any.
    pub recording: Option<String>,
    /// Recording was asked for; it starts with the game connection.
    pub recording_requested: bool,
    pub recording_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Live {
    pub rows: Vec<LiveRow>,
    pub target_name: String,
    pub target_id: i32,
    pub target_mode: String,
    /// 0..=1, or `None` when the target's HP is unknown.
    pub target_hp: Option<f64>,
    pub battle_time_ms: i64,
    pub total_damage: f64,
    pub ping_ms: Option<i32>,
    pub dungeon_id: i32,
    pub dungeon: String,
    pub run_id: Option<i64>,
    pub run_started_at: Option<i64>,
    pub character: Option<String>,
    pub capture: CaptureStatus,
    pub overlay: OverlaySettings,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct OverlaySettings {
    /// Click-through: the overlay ignores the mouse so it never steals a click
    /// from the game.
    pub locked: bool,
    pub visible: bool,
    /// Background opacity, 0..=1.
    pub opacity: f32,
    pub scale: f32,
    /// Rows shown at most.
    pub max_rows: usize,
    pub show_dps: bool,
    /// Show "Name" as "Na**e" for everyone but you (for streaming).
    pub hide_names: bool,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self { locked: false, visible: true, opacity: 0.72, scale: 1.0, max_rows: 8, show_dps: true, hide_names: false }
    }
}

struct RunState {
    run_id: Option<i64>,
    dungeon_id: i32,
    started_at: i64,
    members_written: HashSet<String>,
}

pub struct Engine {
    pub storage: Arc<DataStorage>,
    pub skills: Arc<SkillLookup>,
    pub npcs: Arc<NpcLookup>,
    pub ping: Arc<PingTracker>,
    pub dot_skill_ids: HashSet<i32>,
    calc: Arc<Mutex<DpsCalculator>>,
    pub db: Arc<Db>,
    live: RwLock<Live>,
    status: RwLock<CaptureStatus>,
    pub overlay: RwLock<OverlaySettings>,
    run: Mutex<RunState>,
    reset_requested: AtomicBool,
    target_mode: RwLock<String>,
    pub buffs: BuffTracker,
    record_wanted: AtomicBool,
    capture_dir: PathBuf,
}

fn load_lookups(language: &str) -> (SkillLookup, NpcLookup) {
    let skills = SkillLookup::new();
    let npcs = NpcLookup::new();
    let (skill_json, npc_json) = match language {
        "en" => (
            include_str!("../data/i18n/skills/en.json"),
            include_str!("../data/i18n/npcs/en.json"),
        ),
        _ => (
            include_str!("../data/i18n/skills/de.json"),
            include_str!("../data/i18n/npcs/de.json"),
        ),
    };
    skills.load_from_json(skill_json);
    npcs.load_from_json(npc_json);
    (skills, npcs)
}

impl Engine {
    pub fn new(db: Db, language: &str, capture_dir: PathBuf) -> Arc<Self> {
        let (skills, npcs) = load_lookups(language);
        let skills = Arc::new(skills);
        let npcs = Arc::new(npcs);
        let storage = Arc::new(DataStorage::new());
        let ping = Arc::new(PingTracker::new());
        let calc = Arc::new(Mutex::new(DpsCalculator::new(
            storage.clone(),
            skills.clone(),
            npcs.clone(),
            ping.clone(),
        )));
        let dot_skill_ids: HashSet<i32> =
            serde_json::from_str::<Vec<i32>>(include_str!("../data/dot_skill_ids.json"))
                .unwrap_or_default()
                .into_iter()
                .collect();
        if let Err(e) = db.close_dangling_runs() {
            tracing::warn!("Could not close old runs: {e}");
        }

        let engine = Arc::new(Self {
            storage,
            skills,
            npcs,
            ping,
            dot_skill_ids,
            calc,
            db: Arc::new(db),
            live: RwLock::new(Live::default()),
            status: RwLock::new(CaptureStatus { game_running: false, ..Default::default() }),
            overlay: RwLock::new(OverlaySettings::default()),
            run: Mutex::new(RunState { run_id: None, dungeon_id: 0, started_at: 0, members_written: HashSet::new() }),
            reset_requested: AtomicBool::new(false),
            target_mode: RwLock::new("bossTargets".into()),
            buffs: BuffTracker::default(),
            record_wanted: AtomicBool::new(false),
            capture_dir,
        });

        // Combat data is cleared on zone changes and when a party ends; save
        // what is there first, or leaving right after a kill loses it.
        let weak = Arc::downgrade(&engine);
        engine.storage.set_before_reset(move || {
            if let Some(engine) = weak.upgrade() {
                engine.save_fights(true);
            }
        });
        engine
    }

    pub fn live(&self) -> Live {
        self.live.read().clone()
    }

    pub fn set_permission(&self, ok: bool) {
        self.status.write().permission = ok;
    }

    pub fn set_capture_error(&self, error: Option<String>) {
        self.status.write().error = error;
    }

    pub fn set_game_running(&self, running: bool) {
        let mut s = self.status.write();
        s.game_running = running;
        if !running {
            s.locked_port = None;
            s.device = None;
        }
    }

    pub fn set_locked(&self, lock: Option<(u16, String)>) {
        let mut s = self.status.write();
        s.game_running = s.game_running || lock.is_some();
        s.locked_port = lock.as_ref().map(|l| l.0);
        s.device = lock.map(|l| l.1);
    }

    pub fn capture_dir(&self) -> PathBuf {
        self.capture_dir.clone()
    }

    pub fn recording_wanted(&self) -> bool {
        self.record_wanted.load(Ordering::SeqCst)
    }

    /// Start or stop recording the game connection. Returns whether it records now.
    pub fn toggle_recording(&self) -> bool {
        let on = !self.record_wanted.fetch_xor(true, Ordering::SeqCst);
        let mut s = self.status.write();
        s.recording_error = None;
        s.recording_requested = on;
        if !on {
            s.recording = None;
        }
        on
    }

    pub fn set_recording(&self, on: bool) {
        if self.recording_wanted() != on {
            self.toggle_recording();
        }
    }

    pub fn recording_started(&self, path: &Path) {
        self.status.write().recording = Some(path.display().to_string());
    }

    pub fn stop_recording(&self, error: Option<String>) {
        self.record_wanted.store(false, Ordering::SeqCst);
        let mut s = self.status.write();
        s.recording = None;
        s.recording_requested = false;
        s.recording_error = error;
    }

    fn effect_name(&self, code: u32) -> String {
        buffs::name_keys(code)
            .into_iter()
            .map(|k| self.skills.get_skill_name(k))
            .find(|n| !n.is_empty())
            .unwrap_or_else(|| format!("#{code}"))
    }

    /// Buff uptimes of every fighter and debuff uptimes on the boss, per fight.
    fn fight_effects(&self, records: &[FightRecord]) -> Vec<EffectRow> {
        let mut rows = Vec::new();
        for record in records {
            let from = record.start_time_ms;
            let to = from + record.duration_ms.max(0);
            let targets = record
                .actors
                .iter()
                .map(|a| (a.actor_id, false))
                .chain(std::iter::once((record.target_id, true)));
            for (entity, on_boss) in targets {
                if entity <= 0 {
                    continue;
                }
                for u in self
                    .buffs
                    .uptimes(entity as u32, from, to)
                    .into_iter()
                    .filter(|u| u.percent >= MIN_UPTIME_PERCENT)
                    .take(MAX_EFFECTS_PER_ENTITY)
                {
                    rows.push(EffectRow {
                        fight_id: record.id.clone(),
                        entity_id: entity,
                        code: u.code,
                        name: self.effect_name(u.code),
                        caster_id: u.caster as i64,
                        uptime: u.percent,
                        on_boss,
                    });
                }
            }
        }
        rows
    }

    /// Start the meter over: the live numbers go, saved fights stay.
    pub fn request_reset(&self) {
        self.reset_requested.store(true, Ordering::SeqCst);
    }

    pub fn set_target_mode(&self, mode: &str) {
        *self.target_mode.write() = mode.to_string();
        self.calc.lock().set_target_selection_mode(mode);
    }

    fn self_names(&self) -> Vec<String> {
        let profile = self.storage.local_profile();
        profile
            .name
            .into_iter()
            .chain(self.storage.local_character_name())
            .collect()
    }

    fn save_fights(&self, force: bool) {
        if self.storage.damage_generation() <= 0 {
            return;
        }
        let (records, local) = {
            let mut calc = if force {
                self.calc.lock()
            } else {
                match self.calc.try_lock() {
                    Some(c) => c,
                    None => return,
                }
            };
            let records = if force { calc.snapshot_boss_fights_force() } else { calc.snapshot_boss_fights() };
            (records, self.live.read().rows.iter().find(|r| r.is_self).map(|r| r.id as i64))
        };
        let dungeon = self.storage.current_dungeon_id();
        let dead = self.storage.get_dead_entities();
        match self.db.save_fights(&records, dungeon, &self.self_names(), local, &dead) {
            Ok(0) => return,
            Ok(n) => tracing::info!("Saved {n} fight(s)"),
            Err(e) => {
                tracing::error!("Saving fights failed: {e:#}");
                return;
            }
        }
        let effects = self.fight_effects(&records);
        if let Err(e) = self.db.save_effects(&records, &effects) {
            tracing::error!("Saving buffs failed: {e:#}");
        }
    }

    /// Follow instance entries and exits, and record who was in the party.
    fn track_run(&self, now: i64) {
        let dungeon = self.storage.current_dungeon_id();
        let profile = self.storage.local_profile();
        let me = profile.name.clone().or_else(|| self.storage.local_character_name());
        if let Some(name) = &me {
            let job = profile.class.map(|c| c.class_name().to_string()).unwrap_or_default();
            let _ = self.db.note_my_character(name, profile.server_id, &job, profile.level.unwrap_or(0), now);
        }

        let mut run = self.run.lock();
        if dungeon != run.dungeon_id {
            if let Some(id) = run.run_id.take() {
                drop(run);
                self.save_fights(true);
                if let Err(e) = self.db.end_run(id, now) {
                    tracing::error!("Ending run failed: {e:#}");
                }
                tracing::info!("Left {}", names::dungeon_label(self.run.lock().dungeon_id));
                run = self.run.lock();
            }
            run.dungeon_id = dungeon;
            run.members_written.clear();
            if dungeon > 0 {
                match self.db.start_run(dungeon, now, me.as_deref(), profile.server_id) {
                    Ok(id) => {
                        tracing::info!("Entered {} (run {id})", names::dungeon_label(dungeon));
                        run.run_id = Some(id);
                        run.started_at = now;
                    }
                    Err(e) => tracing::error!("Starting run failed: {e:#}"),
                }
            }
        }
        let Some(run_id) = run.run_id else { return };
        if let Some(name) = &me {
            let _ = self.db.set_run_character(run_id, name, profile.server_id);
        }

        let party = self.storage.get_party_members();
        let mut members: Vec<Member> = party
            .into_iter()
            .map(|(name, m)| Member {
                is_self: me.as_deref() == Some(name.as_str()),
                job: m.job.map(|j| j.class_name().to_string()).unwrap_or_default(),
                server_id: m.server_id,
                level: m.level,
                gear_score: m.gear_score,
                combat_power: m.combat_power,
                dbid: m.dbid,
                name,
            })
            .collect();
        if let Some(name) = &me {
            if !members.iter().any(|m| &m.name == name) {
                members.push(Member {
                    name: name.clone(),
                    job: profile.class.map(|c| c.class_name().to_string()).unwrap_or_default(),
                    server_id: profile.server_id,
                    level: profile.level.unwrap_or(0) as i32,
                    gear_score: 0,
                    combat_power: 0,
                    dbid: 0,
                    is_self: true,
                });
            }
        }
        // Only touch the database when the party changed.
        let fresh: Vec<Member> = members
            .into_iter()
            .filter(|m| run.members_written.insert(format!("{}|{}|{}", m.name, m.job, m.combat_power)))
            .collect();
        if !fresh.is_empty() {
            if let Err(e) = self.db.upsert_members(run_id, &fresh) {
                tracing::error!("Saving party failed: {e:#}");
            }
        }
    }

    fn build_live(&self, dps: &DpsData) -> Live {
        let profile = self.storage.local_profile();
        let me = profile.name.clone().or_else(|| self.storage.local_character_name());
        let dead = self.storage.get_dead_entities();
        let mut rows: Vec<LiveRow> = dps
            .map
            .iter()
            .map(|(&id, p)| {
                let info = names::class_info(&p.job);
                LiveRow {
                    id,
                    name: if p.nickname.is_empty() { format!("#{id}") } else { p.nickname.clone() },
                    class_key: info.key,
                    class_name: info.name,
                    color: info.color,
                    damage: p.amount,
                    dps: p.dps,
                    share: p.damage_contribution,
                    combat_power: p.combat_power,
                    dead: dead.contains(&id),
                    is_self: dps.local_player_id == Some(id as i64)
                        || (me.is_some() && me.as_deref() == Some(p.nickname.as_str())),
                }
            })
            .collect();
        rows.sort_by(|a, b| b.damage.total_cmp(&a.damage));
        let total: f64 = rows.iter().fold(0.0, |acc, r| acc + r.damage);
        if total > 0.0 && rows.iter().all(|r| r.share <= 0.0) {
            for r in &mut rows {
                r.share = r.damage * 100.0 / total;
            }
        }

        let target_hp = if dps.target_max_hp > 0 {
            let current = if dps.target_current_hp >= 0 {
                dps.target_current_hp
            } else {
                (dps.target_max_hp - dps.target_total_damage).max(0)
            };
            Some((current as f64 / dps.target_max_hp as f64).clamp(0.0, 1.0))
        } else {
            None
        };
        let dungeon_id = if dps.dungeon_id > 0 { dps.dungeon_id } else { self.storage.current_dungeon_id() };
        let run = self.run.lock();
        Live {
            rows,
            target_name: dps.target_name.clone(),
            target_id: dps.target_id,
            target_mode: dps.target_mode.clone(),
            target_hp,
            battle_time_ms: dps.battle_time,
            total_damage: total,
            ping_ms: self.ping.current_ping_ms(),
            dungeon_id,
            dungeon: names::dungeon_label(dungeon_id),
            run_id: run.run_id,
            run_started_at: run.run_id.map(|_| run.started_at),
            character: me,
            capture: self.status.read().clone(),
            overlay: self.overlay.read().clone(),
        }
    }

    /// The meter's heartbeat. Runs on its own thread forever.
    pub fn run_ticks(self: Arc<Self>) {
        let mut tick: u64 = 0;
        loop {
            std::thread::sleep(TICK);
            tick += 1;

            if self.reset_requested.swap(false, Ordering::SeqCst) {
                self.save_fights(true);
                let mut calc = self.calc.lock();
                calc.restart_target_selection(true);
                self.storage.hide_party_placeholders();
            }

            let dps = self.calc.lock().get_dps();
            let live = self.build_live(&dps);
            *self.live.write() = live;

            if tick % 4 == 0 {
                self.track_run(now_ms());
            }
            if tick % SAVE_EVERY_TICKS == 0 {
                self.save_fights(false);
            }
            if tick % 7_200 == 0 {
                self.buffs.prune_old(now_ms());
            }
        }
    }
}
