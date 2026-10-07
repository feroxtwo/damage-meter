//! The meter's state: the parser's combat store, the live numbers the overlay
//! and web app show, and the run tracking that fills the database.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::analytics::Series;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;

use a2tools_dps_meter_lib::combat::data_storage::DataStorage;
use a2tools_dps_meter_lib::combat::dps_calculator::DpsCalculator;
use a2tools_dps_meter_lib::combat::ping_tracker::PingTracker;
use a2tools_dps_meter_lib::entity::details_context::DetailsContext;
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
const RECORD_SETTING: &str = "record_packets";

pub fn now_ms() -> i64 {
    a2tools_dps_meter_lib::clock::now_ms()
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LiveRow {
    pub id: i32,
    pub name: String,
    pub class_key: &'static str,
    pub class_name: &'static str,
    pub color: [u8; 3],
    pub heal: f64,
    pub hps: f64,
    pub damage_received: f64,
    pub burst_dps: f64,
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
    pub stream_gaps: u64,
    pub packets: u64,
    pub last_packet_ms: Option<i64>,
    /// The running executable while capture permission is missing, so the
    /// help can name the exact `setcap` target. Diagnostics never include it.
    pub binary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Live {
    pub rows: Vec<LiveRow>,
    pub target_name: String,
    pub target_id: i32,
    pub target_started_at: Option<i64>,
    pub target_mode: String,
    pub reset_notice: Option<String>,
    /// 0..=1, or `None` when the target's HP is unknown.
    pub target_hp: Option<f64>,
    pub hp_estimated: bool,
    pub training: Value,
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
    pub numeric_limited: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(default)]
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
    pub pin_self: bool,
    pub metric: String,
    pub position: Option<[f32; 2]>,
    pub theme: String,
    pub compact: bool,
    /// Zero disables idle reset. Stored with profiles.
    pub idle_reset_seconds: u64,
    pub wipe_reset: bool,
    pub skill_language: String,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            locked: false,
            visible: true,
            opacity: 0.72,
            scale: 1.0,
            max_rows: 8,
            show_dps: true,
            hide_names: false,
            pin_self: true,
            metric: "damage".into(),
            position: None,
            theme: "midnight".into(),
            compact: false,
            idle_reset_seconds: 0,
            wipe_reset: false,
            skill_language: "de".into(),
        }
    }
}

impl OverlaySettings {
    /// These settings with every field of `patch` that has the right type.
    /// Unknown keys and values of the wrong type are skipped one by one, so a
    /// damaged or older settings record keeps its valid fields, and a partial
    /// dashboard update cannot reset the overlay's position or lock state.
    pub fn merged(&self, patch: &Value) -> Self {
        let mut fields = match serde_json::to_value(self) {
            Ok(Value::Object(fields)) => fields,
            _ => return self.clone(),
        };
        if let Some(patch) = patch.as_object() {
            for (key, value) in patch {
                if !fields.contains_key(key) {
                    continue;
                }
                let previous = fields.insert(key.clone(), value.clone());
                if serde_json::from_value::<Self>(Value::Object(fields.clone())).is_err()
                    && let Some(previous) = previous
                {
                    fields.insert(key.clone(), previous);
                }
            }
        }
        let mut merged: Self =
            serde_json::from_value(Value::Object(fields)).unwrap_or_else(|_| self.clone());
        merged.normalize();
        merged
    }

    fn normalize(&mut self) {
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0.0, 1.0)
        } else {
            0.72
        };
        self.scale = if self.scale.is_finite() {
            self.scale.clamp(0.6, 2.5)
        } else {
            1.0
        };
        self.max_rows = self.max_rows.clamp(1, 24);
        if !["de", "en"].contains(&self.skill_language.as_str()) {
            self.skill_language = "de".into();
        }
        if !["midnight", "aether", "ember"].contains(&self.theme.as_str()) {
            self.theme = "midnight".into();
        }
        if self.idle_reset_seconds != 0 {
            self.idle_reset_seconds = self.idle_reset_seconds.clamp(15, 900);
        }
        if !["damage", "heal", "damage_received"].contains(&self.metric.as_str()) {
            self.metric = "damage".into();
        }
        if self
            .position
            .is_some_and(|p| p.iter().any(|v| !v.is_finite() || v.abs() > 100_000.0))
        {
            self.position = None;
        }
    }
}

/// Name, server, class, level and when that row was written.
type NotedCharacter = (String, u16, String, u32, i64);

struct RunState {
    run_id: Option<i64>,
    dungeon_id: i32,
    started_at: i64,
    members_written: HashSet<String>,
}

pub struct Engine {
    pub storage: Arc<DataStorage>,
    pub combat_gate: Mutex<()>,
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
    series: Mutex<HashMap<(i32, i64), Series>>,
    training: Mutex<Value>,
    encounter: Mutex<crate::encounters::Monitor>,
    reset_notice: RwLock<Option<String>>,
    stopping: AtomicBool,
    shutdown_saved: AtomicBool,
    /// The character row last written and when, so the heartbeat does not
    /// rewrite an unchanged row every two seconds.
    noted_character: Mutex<Option<NotedCharacter>>,
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
        let named = db.fill_missing_boss_names(|code| npcs.get_npc_name(code));
        if let Ok(n @ 1..) = named {
            tracing::info!("Named {n} saved fight(s)");
        }
        if let Err(e) = db.close_dangling_runs() {
            tracing::warn!("Could not close old runs: {e}");
        }

        let initial_overlay = OverlaySettings {
            skill_language: if language == "en" { "en" } else { "de" }.into(),
            ..Default::default()
        };
        let overlay = db
            .meta("overlay")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .map(|saved| initial_overlay.merged(&saved))
            .unwrap_or(initial_overlay);
        let target_mode = db
            .meta("target_mode")
            .ok()
            .flatten()
            .filter(|m| valid_mode(m))
            .unwrap_or_else(|| "bossTargets".into());
        calc.lock().set_target_selection_mode(&target_mode);
        let engine = Arc::new(Self {
            storage,
            combat_gate: Mutex::new(()),
            skills,
            npcs,
            ping,
            dot_skill_ids,
            calc,
            db: Arc::new(db),
            live: RwLock::new(Live::default()),
            status: RwLock::new(CaptureStatus {
                game_running: false,
                ..Default::default()
            }),
            overlay: RwLock::new(overlay),
            run: Mutex::new(RunState {
                run_id: None,
                dungeon_id: 0,
                started_at: 0,
                members_written: HashSet::new(),
            }),
            reset_requested: AtomicBool::new(false),
            target_mode: RwLock::new(target_mode),
            buffs: BuffTracker::default(),
            record_wanted: AtomicBool::new(false),
            capture_dir,
            series: Mutex::new(HashMap::new()),
            training: Mutex::new(json!({"state":"idle"})),
            encounter: Mutex::new(crate::encounters::Monitor::default()),
            reset_notice: RwLock::new(None),
            stopping: AtomicBool::new(false),
            shutdown_saved: AtomicBool::new(false),
            noted_character: Mutex::new(None),
        });

        if engine.db.meta(RECORD_SETTING).ok().flatten().as_deref() == Some("1") {
            engine.set_recording(true);
        }

        // Combat data is cleared on zone changes and when a party ends; save
        // what is there first, or leaving right after a kill loses it.
        let weak = Arc::downgrade(&engine);
        engine.storage.set_before_reset(move || {
            if let Some(engine) = weak.upgrade()
                && !engine.save_fights(true)
            {
                *engine.reset_notice.write() = Some("Speicherfehler beim Zonenwechsel: Kampfdaten können unvollständig sein. Freien Speicher und Terminalmeldung prüfen.".into());
            }
        });
        engine
    }

    pub fn live(&self) -> Live {
        self.live.read().clone()
    }

    pub fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    /// Serialize the final snapshot against packet processing and preserve short attempts.
    /// SIGKILL and power loss cannot run this path; periodic snapshots remain the fallback.
    pub fn shutdown(&self) -> bool {
        if self.stopping.swap(true, Ordering::SeqCst) {
            return self.shutdown_saved.load(Ordering::SeqCst);
        }
        let _gate = self.combat_gate.lock();
        let saved = self.save_fights(true);
        if saved {
            let run = self.run.lock();
            if let Some(id) = run.run_id
                && let Err(e) = self.db.end_run(id, now_ms())
            {
                tracing::error!("Saving run at shutdown: {e:#}");
                return false;
            }
        }
        self.shutdown_saved.store(saved, Ordering::SeqCst);
        saved
    }

    pub fn received_game_packet(&self) {
        let mut status = self.status.write();
        status.packets = status.packets.saturating_add(1);
        status.last_packet_ms = Some(now_ms());
    }

    pub fn set_permission(&self, ok: bool) {
        let mut status = self.status.write();
        status.permission = ok;
        status.binary = (!ok)
            .then(std::env::current_exe)
            .and_then(Result::ok)
            .map(|p| p.display().to_string());
    }

    pub fn capture_gap(&self) {
        self.status.write().stream_gaps += 1;
        for s in self.series.lock().values_mut() {
            s.partial = true;
        }
    }
    pub fn replay_tick(&self) {
        self.process_reset();
        let (dps, context) = self.snapshot();
        self.observe(&context);
        let live = self.build_live(&dps, &context);
        self.automatic_reset(&live, &context);
        *self.live.write() = live;
    }
    /// One consistent parser view per heartbeat, shared by the curve, the live
    /// numbers and the reset monitor instead of three separate snapshots.
    fn snapshot(&self) -> (DpsData, DetailsContext) {
        let mut calc = self.calc.lock();
        (calc.get_dps(), calc.get_details_context())
    }
    pub fn replay_report(&self) -> Value {
        self.save_fights(true);
        let c = self.calc.lock().get_details_context();
        let targets:Vec<_>=c.targets.iter().map(|t| {
            let d=self.calc.lock().get_target_details(t.target_id,None);
            json!({"target":t,"details":d,"analytics":self.series.lock().get(&(t.target_id,d.start_time)).map(|s|s.json())})
        }).collect();
        json!({"live":self.live(),"context":c,"targets":targets,"parser_rev":crate::updates::PARSER_REV})
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
            s.last_packet_ms = None;
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
        drop(s);
        // Remembered, so recording stays on across restarts until turned off.
        if let Err(e) = self.db.set_meta(RECORD_SETTING, if on { "1" } else { "0" }) {
            tracing::warn!("Could not save the recording setting: {e:#}");
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

    /// A requested recording waits for a new connection without reporting a
    /// file as active after its connection has gone away.
    pub fn recording_paused(&self) {
        self.status.write().recording = None;
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
        if let Err(e) = self.db.set_meta("target_mode", mode) {
            tracing::error!("Saving target mode: {e}");
        }
    }

    pub fn update_overlay(&self, mut body: OverlaySettings) -> anyhow::Result<OverlaySettings> {
        body.normalize();
        // Lock before persistence so toggles and dashboard writes cannot interleave.
        let mut settings = self.overlay.write();
        self.db
            .set_meta("overlay", &serde_json::to_string(&body)?)?;
        *settings = body.clone();
        Ok(body)
    }
    /// Apply only the fields a client sent, against the current settings.
    pub fn patch_overlay(&self, patch: &Value) -> anyhow::Result<OverlaySettings> {
        self.modify_overlay(|s| *s = s.merged(patch))
    }
    pub fn modify_overlay(
        &self,
        f: impl FnOnce(&mut OverlaySettings),
    ) -> anyhow::Result<OverlaySettings> {
        let mut settings = self.overlay.write();
        let mut next = settings.clone();
        f(&mut next);
        next.normalize();
        self.db
            .set_meta("overlay", &serde_json::to_string(&next)?)?;
        *settings = next.clone();
        Ok(next)
    }
    pub fn profile(&self, key: &str, save: bool) -> anyhow::Result<OverlaySettings> {
        let key = format!("overlay_profile:{key}");
        if save {
            let s = self.overlay.read().clone();
            self.db.set_meta(&key, &serde_json::to_string(&s)?)?;
            Ok(s)
        } else {
            let s = self
                .db
                .meta(&key)?
                .ok_or_else(|| anyhow::anyhow!("Profil nicht vorhanden"))?;
            let defaults = OverlaySettings {
                skill_language: self.overlay.read().skill_language.clone(),
                ..Default::default()
            };
            self.update_overlay(defaults.merged(&serde_json::from_str(&s)?))
        }
    }
    pub fn player_details(&self, id: i32) -> Value {
        let calc = self.calc.lock();
        let target = self.live.read().target_id;
        let details = calc.get_target_details(target, Some(&[id]));
        let context = calc.get_details_context();
        let actor = context.actors.iter().find(|a| a.actor_id == id);
        json!({"id":id,"target_id":target,"start_time":details.start_time,"duration_ms":details.battle_time,
          "skills":crate::db::skill_rows(&details.skills,id,details.battle_time),"heal_skills":crate::db::skill_rows(&details.heal_skills,id,details.battle_time),"actor":actor,"analytics":self.series.lock().get(&(target,details.start_time)).map(|s|s.json()),"data_quality":"observed_hits_unknown_quality_coverage"})
    }
    pub fn start_training(&self, seconds: i64) {
        self.set_target_mode("trainTargets");
        self.request_reset();
        *self.training.lock() = json!({"state":"armed","seconds":seconds,"started_at":null});
    }
    pub fn cancel_training(&self) {
        *self.training.lock() = json!({"state":"idle"});
    }
    fn observe(&self, context: &DetailsContext) {
        let now = now_ms();
        let mut series = self.series.lock();
        for target in &context.targets {
            if now - target.last_damage_time > 10_000 {
                continue;
            }
            let start = target.last_damage_time - target.battle_time;
            let entry = series.entry((target.target_id, start)).or_default();
            entry.observe(
                now - start,
                target
                    .actor_damage
                    .iter()
                    .map(|(&id, &d)| (id, i64::from(d)))
                    .collect(),
            );
        }
        if series.len() > 256 {
            let mut keys: Vec<_> = series.keys().copied().collect();
            keys.sort_by_key(|k| k.1);
            for k in keys.into_iter().take(series.len() - 256) {
                series.remove(&k);
            }
        }
        crate::analytics::bound_series(&mut series);
    }
    fn tick_training(&self, live: &Live) {
        let mut t = self.training.lock();
        let state = t["state"].as_str().unwrap_or("idle").to_string();
        if state == "armed" && live.total_damage > 0.0 {
            t["state"] = json!("running");
            t["started_at"] = json!(now_ms() - live.battle_time_ms);
            t["target_id"] = json!(live.target_id);
        }
        if t["state"] == "running" {
            let elapsed = now_ms() - t["started_at"].as_i64().unwrap_or(now_ms());
            t["elapsed_ms"] = json!(elapsed);
            if live.target_id != t["target_id"].as_i64().unwrap_or(0) as i32
                || live.target_mode != "trainTargets"
                || !live.capture.game_running
                || live.capture.error.is_some()
            {
                t["state"] = json!("interrupted");
                return;
            }
            if elapsed >= t["seconds"].as_i64().unwrap_or(60) * 1000 {
                let seconds = t["seconds"].as_i64().unwrap_or(60);
                let rows:Vec<_>=live.rows.iter().map(|r|json!({"name":r.name,"is_self":r.is_self,"damage":r.damage,"dps":r.damage/(elapsed as f64/1000.0)})).collect();
                let key = format!(
                    "training_best:{}:{}:{}",
                    live.character.as_deref().unwrap_or(""),
                    live.target_name,
                    seconds
                );
                let my = rows
                    .iter()
                    .find(|r| r["is_self"] == true)
                    .and_then(|r| r["dps"].as_f64())
                    .unwrap_or(0.0);
                let best = self
                    .db
                    .meta(&key)
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                t["state"] = json!("finished");
                t["rows"] = json!(rows);
                t["personal_best"] = json!(my > best);
                t["best_dps"] = json!(my.max(best));
                t["target"] = json!(live.target_name);
                t["character"] = json!(live.character);
                if my > best {
                    let _ = self.db.set_meta(&key, &my.to_string());
                }
                let _ = self.db.set_meta("last_training", &t.to_string());
            }
        }
    }

    fn self_names(&self) -> Vec<String> {
        let profile = self.storage.local_profile();
        profile
            .name
            .into_iter()
            .chain(self.storage.local_character_name())
            .collect()
    }

    /// A monster's name comes from its spawn packet. When that was missed at
    /// the fight's first snapshot, it may have arrived since.
    fn name_fights(&self, records: &mut [FightRecord]) {
        if records
            .iter()
            .all(|r| r.mob_code > 0 && !r.boss_name.is_empty())
        {
            return;
        }
        let mobs = self.storage.get_mob_data();
        for r in records {
            if r.mob_code <= 0
                && let Some(&code) = mobs.get(&r.target_id)
            {
                r.mob_code = code;
                r.is_train = r.is_train || self.npcs.is_training_dummy(code);
            }
            if r.boss_name.is_empty() && r.mob_code > 0 {
                r.boss_name = self.npcs.get_npc_name(r.mob_code);
            }
        }
    }

    /// The upstream boss snapshot masks every name but the local player's
    /// ("Mo****t") because its records are meant for uploading. This history is
    /// local and joins partners by name, so put back the name the parser knows,
    /// but only where it is exactly the masked form of that name.
    fn unmask_actor_names(&self, records: &mut [FightRecord]) {
        use a2tools_dps_meter_lib::entity::fight_record::obscure_nickname;
        for record in records {
            for actor in &mut record.actors {
                let Some(name) = self
                    .storage
                    .get_nickname(actor.actor_id)
                    .filter(|n| !n.trim().is_empty())
                else {
                    continue;
                };
                if actor.nickname != name
                    && (actor.nickname.is_empty()
                        || actor.nickname == format!("#{}", actor.actor_id)
                        || obscure_nickname(&name) == actor.nickname)
                {
                    actor.nickname = name;
                }
            }
        }
    }

    /// The live target's name, or a placeholder when its spawn was missed.
    fn target_name(&self, dps: &DpsData) -> String {
        if !dps.target_name.is_empty() || dps.target_id <= 0 {
            return dps.target_name.clone();
        }
        self.storage
            .get_mob_data()
            .get(&dps.target_id)
            .map(|&code| self.npcs.get_npc_name(code))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Unbekannter Gegner".into())
    }

    fn save_fights(&self, force: bool) -> bool {
        if self.storage.damage_generation() <= 0 {
            return true;
        }
        let (mut records, local) = {
            let mut calc = if force {
                self.calc.lock()
            } else {
                match self.calc.try_lock() {
                    Some(c) => c,
                    None => return false,
                }
            };
            let mut records = if force {
                calc.snapshot_boss_fights_force()
            } else {
                calc.snapshot_boss_fights()
            };
            if force {
                // The upstream periodic saver omits short attempts and already-idle targets.
                // Before a reset retain every target with observed damage by our player.
                let context = calc.get_details_context();
                if let Some(local_id) = self.storage.local_player_id() {
                    for target in &context.targets {
                        if !target.actor_damage.contains_key(&(local_id as i32)) {
                            continue;
                        }
                        let details = calc.get_target_details(target.target_id, None);
                        let start = target.last_damage_time - target.battle_time;
                        let id = format!("auto_{}_{}", target.target_id, start);
                        if records.iter().any(|r| r.id == id) {
                            continue;
                        }
                        let mob_code = self
                            .storage
                            .get_mob_data()
                            .get(&target.target_id)
                            .copied()
                            .unwrap_or(0);
                        let actors: Vec<_> = context
                            .actors
                            .iter()
                            .filter(|a| {
                                details
                                    .skills
                                    .iter()
                                    .any(|skill| skill.actor_id == a.actor_id)
                            })
                            .cloned()
                            .collect();
                        let record = serde_json::from_value(
                            json!({"id":id,"bossName":target.target_name,
                            "targetId":target.target_id,"startTimeMs":start,"durationMs":target.battle_time,
                            "totalDamage":target.total_damage,"jobs":[],"dungeonId":self.storage.current_dungeon_id(),
                            "mobCode":mob_code,"isTrain":self.npcs.is_training_dummy(mob_code),
                            "details":details,"actors":actors}),
                        );
                        match record {
                            Ok(r) => records.push(r),
                            Err(e) => {
                                tracing::error!("Attempt snapshot: {e}");
                                return false;
                            }
                        }
                    }
                }
            }
            (
                records,
                self.live
                    .read()
                    .rows
                    .iter()
                    .find(|r| r.is_self)
                    .map(|r| r.id as i64),
            )
        };
        self.name_fights(&mut records);
        self.unmask_actor_names(&mut records);
        for record in &mut records {
            for skill in record
                .details
                .skills
                .iter()
                .chain(&record.details.heal_skills)
            {
                if record.actors.iter().all(|a| a.actor_id != skill.actor_id)
                    && let Ok(actor) = serde_json::from_value(json!({"actorId":skill.actor_id,
                        "nickname":self.storage.get_nickname(skill.actor_id).unwrap_or_else(||format!("#{}",skill.actor_id)),"job":skill.job}))
                {
                    record.actors.push(actor);
                }
            }
        }
        let dungeon = self.storage.current_dungeon_id();
        let dead = self.storage.get_dead_entities();
        match self
            .db
            .save_fights(&records, dungeon, &self.self_names(), local, &dead)
        {
            Ok(0) => return true,
            Ok(n) => tracing::info!("Saved {n} fight(s)"),
            Err(e) => {
                tracing::error!("Saving fights failed: {e:#}");
                return false;
            }
        }
        for record in &records {
            if let Some(series) = self
                .series
                .lock()
                .get(&(record.target_id, record.start_time_ms))
            {
                let mut data = series.json();
                if let Some(points) = data["points"].as_array_mut() {
                    let mut end = None;
                    for point in points.iter() {
                        if point["ms"].as_i64().unwrap_or(0) > record.duration_ms {
                            end = Some(point.clone());
                            break;
                        }
                    }
                    points.retain(|p| p["ms"].as_i64().unwrap_or(0) <= record.duration_ms);
                    if let Some(mut point) = end {
                        point["ms"] = json!(record.duration_ms);
                        points.push(point);
                    }
                }
                if let Some(last) = data["points"].as_array_mut().and_then(|p| p.last_mut()) {
                    let mut totals: HashMap<i32, i64> = HashMap::new();
                    for skill in &record.details.skills {
                        *totals.entry(skill.actor_id).or_default() += i64::from(skill.dmg);
                    }
                    last["damage"] = json!(totals);
                }
                data["effects"] = self.buffs.timeline(
                    record.start_time_ms,
                    record.start_time_ms + record.duration_ms,
                );
                data["effects_partial"] = json!(self.buffs.partial_between(
                    record.start_time_ms,
                    record.start_time_ms + record.duration_ms.max(1),
                ));
                data["parser_rev"] = json!(crate::updates::PARSER_REV);
                data["outcome"] = json!(if self.storage.is_entity_dead(record.target_id) {
                    "kill"
                } else {
                    "unknown"
                });
                if let Err(e) = self.db.save_analytics(&record.id, &data) {
                    tracing::error!("Saving analytics: {e}");
                    return false;
                }
            }
        }
        let effects = self.fight_effects(&records);
        if let Err(e) = self.db.save_effects(&records, &effects) {
            tracing::error!("Saving buffs failed: {e:#}");
            return false;
        }
        true
    }

    /// Follow instance entries and exits, and record who was in the party.
    fn track_run(&self, now: i64) {
        let dungeon = self.storage.current_dungeon_id();
        let profile = self.storage.local_profile();
        let me = profile
            .name
            .clone()
            .or_else(|| self.storage.local_character_name());
        if let Some(name) = &me {
            let job = profile
                .class
                .map(|c| c.class_name().to_string())
                .unwrap_or_default();
            let level = profile.level.unwrap_or(0);
            let mut noted = self.noted_character.lock();
            let unchanged = noted.as_ref().is_some_and(|(n, server, j, l, at)| {
                n == name
                    && *server == profile.server_id
                    && *j == job
                    && *l == level
                    && now - at < 60_000
            });
            if !unchanged
                && self
                    .db
                    .note_my_character(name, profile.server_id, &job, level, now)
                    .is_ok()
            {
                *noted = Some((name.clone(), profile.server_id, job, level, now));
            }
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
                match self
                    .db
                    .start_run(dungeon, now, me.as_deref(), profile.server_id)
                {
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
                job: m
                    .job
                    .map(|j| j.class_name().to_string())
                    .unwrap_or_default(),
                server_id: m.server_id,
                level: m.level,
                gear_score: m.gear_score,
                combat_power: m.combat_power,
                dbid: m.dbid,
                name,
            })
            .collect();
        if let Some(name) = &me
            && !members.iter().any(|m| &m.name == name)
        {
            members.push(Member {
                name: name.clone(),
                job: profile
                    .class
                    .map(|c| c.class_name().to_string())
                    .unwrap_or_default(),
                server_id: profile.server_id,
                level: profile.level.unwrap_or(0) as i32,
                gear_score: 0,
                combat_power: 0,
                dbid: 0,
                is_self: true,
            });
        }
        // Only touch the database when the party changed.
        let fresh: Vec<Member> = members
            .into_iter()
            .filter(|m| {
                run.members_written
                    .insert(format!("{}|{}|{}", m.name, m.job, m.combat_power))
            })
            .collect();
        if !fresh.is_empty()
            && let Err(e) = self.db.upsert_members(run_id, &fresh)
        {
            tracing::error!("Saving party failed: {e:#}");
        }
    }

    fn build_live(&self, dps: &DpsData, context: &DetailsContext) -> Live {
        let profile = self.storage.local_profile();
        let me = profile
            .name
            .clone()
            .or_else(|| self.storage.local_character_name());
        let dead = self.storage.get_dead_entities();

        let start = context
            .targets
            .iter()
            .find(|t| t.target_id == dps.target_id)
            .map(|t| t.last_damage_time - t.battle_time)
            .unwrap_or(0);
        let mut heals: HashMap<i32, i64> = HashMap::new();
        let mut heal_jobs = HashMap::new();
        let summons = self.storage.get_summon_data();
        for (raw_id, skills) in self.storage.get_heal_snapshot() {
            let resolved =
                a2tools_dps_meter_lib::entity::summon_resolver::resolve(raw_id, &summons);
            let name = self.storage.get_nickname(resolved);
            let id = context
                .actors
                .iter()
                .find(|a| name.as_deref() == Some(a.nickname.as_str()))
                .map(|a| a.actor_id)
                .unwrap_or(resolved);
            for ((code, _), h) in skills {
                *heals.entry(id).or_default() += h.total_heal;
                if let Some(job) =
                    a2tools_dps_meter_lib::entity::job_class::JobClass::convert_from_skill(code)
                {
                    heal_jobs
                        .entry(id)
                        .or_insert_with(|| job.class_name().to_string());
                }
            }
        }
        let mut rows: Vec<LiveRow> = dps
            .map
            .iter()
            .map(|(&id, p)| {
                let info = names::class_info(&p.job);
                LiveRow {
                    id,
                    name: if p.nickname.is_empty() {
                        format!("#{id}")
                    } else {
                        p.nickname.clone()
                    },
                    class_key: info.key,
                    class_name: info.name,
                    color: info.color,
                    heal: heals.get(&id).copied().unwrap_or(0) as f64,
                    hps: heals.get(&id).copied().unwrap_or(0) as f64 * 1000.0
                        / dps.battle_time.max(1000) as f64,
                    damage_received: context
                        .actors
                        .iter()
                        .find(|a| a.actor_id == id)
                        .map(|a| a.damage_received)
                        .unwrap_or(0) as f64,
                    burst_dps: self
                        .series
                        .lock()
                        .get(&(dps.target_id, start))
                        .map(|s| s.burst(id, now_ms() - start))
                        .unwrap_or(0.0),
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
        for (&id, &amount) in &heals {
            if rows.iter().any(|r| r.id == id) {
                continue;
            }
            let actor = context.actors.iter().find(|a| a.actor_id == id);
            let job = actor
                .map(|a| a.job.as_str())
                .filter(|j| !j.is_empty())
                .or_else(|| heal_jobs.get(&id).map(String::as_str))
                .unwrap_or("");
            let name = actor
                .map(|a| a.nickname.clone())
                .or_else(|| self.storage.get_nickname(id))
                .unwrap_or_else(|| format!("#{id}"));
            let info = names::class_info(job);
            let heal = amount as f64;
            rows.push(LiveRow {
                id,
                name: name.clone(),
                class_key: info.key,
                class_name: info.name,
                color: info.color,
                heal,
                hps: heal * 1000.0 / dps.battle_time.max(1000) as f64,
                damage_received: actor.map(|a| a.damage_received).unwrap_or(0) as f64,
                is_self: self.storage.local_player_id() == Some(id as i64)
                    || me.as_deref() == Some(name.as_str()),
                dead: dead.contains(&id),
                ..Default::default()
            });
        }
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
        let dungeon_id = if dps.dungeon_id > 0 {
            dps.dungeon_id
        } else {
            self.storage.current_dungeon_id()
        };
        let run = self.run.lock();
        Live {
            rows,
            target_name: self.target_name(dps),
            target_id: dps.target_id,
            target_started_at: (total > 0.0).then_some(start),
            target_mode: dps.target_mode.clone(),
            reset_notice: self.reset_notice.read().clone(),
            target_hp,
            hp_estimated: dps.target_current_hp < 0,
            training: self.training.lock().clone(),
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
            numeric_limited: self.storage.get_combat_snapshot_light().values().any(|t| {
                t.total_damage > i32::MAX as i64
                    || t.actors
                        .values()
                        .any(|a| a.skills.values().any(|s| s.total_damage == i32::MAX))
            }),
        }
    }

    fn automatic_reset(&self, live: &Live, context: &DetailsContext) {
        if live.total_damage <= 0.0
            || live.target_id <= 0
            || live.target_mode == "trainTargets"
            || ["armed", "running"].contains(&self.training.lock()["state"].as_str().unwrap_or(""))
        {
            return;
        }

        let Some(target) = context
            .targets
            .iter()
            .find(|t| t.target_id == live.target_id)
        else {
            return;
        };
        let settings = self.overlay.read().clone();
        let reason = self.encounter.lock().update(
            crate::encounters::Observation {
                key: (
                    target.target_id,
                    target.last_damage_time - target.battle_time,
                ),
                now: now_ms(),
                last_damage: context
                    .targets
                    .iter()
                    .map(|t| t.last_damage_time)
                    .max()
                    .unwrap_or(target.last_damage_time),
                hp: if live.hp_estimated {
                    None
                } else {
                    live.target_hp
                },
                all_dead: live.rows.len() >= 2
                    && live.rows.iter().any(|r| r.is_self)
                    && live.rows.iter().all(|r| r.dead),
                target_dead: self.storage.is_entity_dead(live.target_id),
            },
            settings.idle_reset_seconds,
            settings.wipe_reset && live.target_mode == "bossTargets",
        );
        if let Some(reason) = reason {
            self.reset_combat(reason);
        }
    }
    fn process_reset(&self) {
        if self.reset_requested.swap(false, Ordering::SeqCst) {
            self.reset_combat("manual");
        }
    }
    fn reset_combat(&self, reason: &str) -> bool {
        let _gate = self.combat_gate.lock();
        if reason != "manual" {
            let context = self.calc.lock().get_details_context();
            let newest = context
                .targets
                .iter()
                .map(|t| t.last_damage_time)
                .max()
                .unwrap_or(now_ms());
            let quiet = if reason == "wipe" {
                5_000
            } else {
                self.overlay.read().idle_reset_seconds as i64 * 1000
            };
            if quiet <= 0 || now_ms() - newest < quiet {
                return false;
            }
        }
        if !self.save_fights(true) {
            *self.reset_notice.write() =
                Some("Reset abgebrochen: Kampf konnte nicht gespeichert werden.".into());
            return false;
        }
        let ids: Vec<_> = self
            .storage
            .get_combat_snapshot_light()
            .values()
            .map(|t| format!("auto_{}_{}", t.target_id, t.first_damage_time))
            .collect();
        if let Err(e) = self.db.finish_attempts(&ids, reason) {
            tracing::error!("Finishing attempts: {e}");
            *self.reset_notice.write() =
                Some("Reset abgebrochen: Abschluss konnte nicht gespeichert werden.".into());
            return false;
        }
        if self.training.lock()["state"] == "running" {
            self.training.lock()["state"] = json!("interrupted");
        }
        self.calc.lock().restart_target_selection(true);
        self.storage.hide_party_placeholders();
        *self.encounter.lock() = crate::encounters::Monitor::default();
        *self.reset_notice.write() = Some(
            match reason {
                "wipe" => "Wipe erkannt: Versuch gespeichert und Meter zurückgesetzt.",
                "idle" => "Leerlauf: Versuch gespeichert und Meter zurückgesetzt.",
                _ => "Versuch gespeichert und Meter zurückgesetzt.",
            }
            .into(),
        );
        true
    }

    /// The meter's heartbeat. Runs on its own thread forever.
    pub fn run_ticks(self: Arc<Self>) {
        let mut tick: u64 = 0;
        while !self.is_stopping() {
            std::thread::sleep(TICK);
            if self.is_stopping() {
                break;
            }
            tick += 1;

            self.process_reset();

            let (dps, context) = self.snapshot();
            self.observe(&context);
            let live = self.build_live(&dps, &context);
            self.automatic_reset(&live, &context);
            self.tick_training(&live);
            *self.live.write() = live;

            if tick.is_multiple_of(4) {
                let _gate = self.combat_gate.lock();
                if self.is_stopping() {
                    break;
                }
                self.track_run(now_ms());
            }
            if tick.is_multiple_of(SAVE_EVERY_TICKS) {
                let _gate = self.combat_gate.lock();
                if self.is_stopping() {
                    break;
                }
                self.save_fights(false);
            }
            if tick.is_multiple_of(7_200) {
                self.buffs.prune_old(now_ms());
            }
        }
    }
}

pub fn valid_mode(mode: &str) -> bool {
    [
        "bossTargets",
        "mostDamage",
        "lastHitByMe",
        "allTargets",
        "trainTargets",
    ]
    .contains(&mode)
}
/// Rank first, then replace the last visible row with the local player if needed.
pub fn overlay_rows(live: &Live, settings: &OverlaySettings) -> Vec<(usize, LiveRow)> {
    let mut ranked: Vec<_> = live.rows.to_vec();
    ranked.sort_by(|a, b| {
        metric_value(b, &settings.metric).total_cmp(&metric_value(a, &settings.metric))
    });
    let mut rows: Vec<_> = ranked
        .iter()
        .cloned()
        .enumerate()
        .take(settings.max_rows)
        .collect();
    if settings.pin_self
        && !rows.iter().any(|(_, r)| r.is_self)
        && let Some((rank, me)) = ranked.into_iter().enumerate().find(|(_, r)| r.is_self)
    {
        if rows.len() >= settings.max_rows {
            rows.pop();
        }
        rows.push((rank, me));
    }
    rows
}
pub fn metric_value(row: &LiveRow, metric: &str) -> f64 {
    match metric {
        "heal" => row.heal,
        "damage_received" => row.damage_received,
        _ => row.damage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_and_position_survive_engine_restart() {
        let path = std::env::temp_dir().join(format!("a2m-settings-{}.db", std::process::id()));
        {
            let e = Engine::new(Db::open(&path).unwrap(), "de", std::env::temp_dir());
            e.modify_overlay(|s| {
                s.position = Some([123.0, 456.0]);
                s.max_rows = 2;
                s.hide_names = true;
            })
            .unwrap();
            e.profile("Main", true).unwrap();
            e.modify_overlay(|s| s.hide_names = false).unwrap();
        }
        {
            let e = Engine::new(Db::open(&path).unwrap(), "de", std::env::temp_dir());
            assert_eq!(e.overlay.read().position, Some([123.0, 456.0]));
            assert!(!e.overlay.read().hide_names);
            e.profile("Main", false).unwrap();
            assert!(e.overlay.read().hide_names);
            assert!(e.profile("missing", false).is_err());
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn pinned_player_keeps_actual_rank_and_healing_sorts_independently() {
        let live = Live {
            rows: vec![
                LiveRow {
                    name: "A".into(),
                    damage: 100.0,
                    heal: 1.0,
                    ..Default::default()
                },
                LiveRow {
                    name: "B".into(),
                    damage: 50.0,
                    heal: 500.0,
                    ..Default::default()
                },
                LiveRow {
                    name: "Me".into(),
                    damage: 10.0,
                    is_self: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut s = OverlaySettings {
            max_rows: 2,
            ..Default::default()
        };
        let rows = overlay_rows(&live, &s);
        assert_eq!(rows[1].0, 2);
        assert!(rows[1].1.is_self);
        s.metric = "heal".into();
        assert_eq!(overlay_rows(&live, &s)[0].1.name, "B");
    }
    #[test]
    fn training_waits_for_damage_finishes_once_and_remembers_best() {
        let e = Engine::new(Db::in_memory().unwrap(), "de", std::env::temp_dir());
        a2tools_dps_meter_lib::clock::set_override(Some(1000));
        e.start_training(60);
        e.tick_training(&Live::default());
        assert_eq!(e.training.lock()["state"], "armed");
        let mut live = Live {
            target_id: 9,
            target_name: "Dummy".into(),
            target_mode: "trainTargets".into(),
            capture: CaptureStatus {
                game_running: true,
                ..Default::default()
            },
            character: Some("Me".into()),
            total_damage: 100.0,
            rows: vec![LiveRow {
                name: "Me".into(),
                damage: 100.0,
                is_self: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        e.tick_training(&live);
        assert_eq!(e.training.lock()["state"], "running");
        a2tools_dps_meter_lib::clock::set_override(Some(61_000));
        live.rows[0].damage = 6000.0;
        e.tick_training(&live);
        assert_eq!(e.training.lock()["state"], "finished");
        assert_eq!(e.training.lock()["best_dps"], 100.0);
        live.rows[0].damage = 12000.0;
        e.tick_training(&live);
        assert_eq!(e.training.lock()["best_dps"], 100.0);
        assert!(e.db.meta("last_training").unwrap().is_some());
        a2tools_dps_meter_lib::clock::set_override(None);
    }
    fn short_attempt() -> Arc<Engine> {
        use a2tools_dps_meter_lib::entity::damage_packet::ParsedDamagePacket;
        let e = Engine::new(Db::in_memory().unwrap(), "de", std::env::temp_dir());
        e.set_target_mode("mostDamage");
        e.storage.set_local_player_id(Some(2259));
        for (time, damage) in [(1000, 100), (2000, 200)] {
            a2tools_dps_meter_lib::clock::set_override(Some(time));
            let mut p = ParsedDamagePacket::new();
            p.set_actor_id(2259);
            p.set_target_id(50_000);
            p.set_skill_code(11010000);
            p.set_damage(damage);
            e.storage.append_damage(p);
            e.storage.append_nickname_authoritative(2259, "Me");
            e.replay_tick();
        }
        e
    }
    #[test]
    fn reset_persists_short_attempt_before_clearing_and_does_not_overwrite_it() {
        let e = short_attempt();
        e.request_reset();
        e.replay_tick();
        let saved = e.db.fight_detail("auto_50000_1000").unwrap().unwrap();
        assert_eq!(saved["total_damage"], 300);
        assert_eq!(saved["duration_ms"], 1000);
        assert_eq!(saved["analytics"]["end_reason"], "manual");
        assert_eq!(e.live().total_damage, 0.0);
        e.request_reset();
        e.replay_tick();
        assert_eq!(
            e.db.fight_detail("auto_50000_1000").unwrap().unwrap()["total_damage"],
            300
        );
        a2tools_dps_meter_lib::clock::set_override(None);
    }
    #[test]
    fn idle_reset_saves_attempt_and_training_is_exempt() {
        let e = short_attempt();
        e.modify_overlay(|s| s.idle_reset_seconds = 15).unwrap();
        *e.training.lock() = json!({"state":"armed","seconds":60});
        a2tools_dps_meter_lib::clock::set_override(Some(18_000));
        e.replay_tick();
        assert_eq!(e.live().total_damage, 300.0);
        e.cancel_training();
        e.replay_tick();
        e.replay_tick();
        assert_eq!(e.live().total_damage, 0.0);
        assert_eq!(
            e.db.fight_detail("auto_50000_1000").unwrap().unwrap()["analytics"]["end_reason"],
            "idle"
        );
        a2tools_dps_meter_lib::clock::set_override(None);
    }
    #[test]
    fn storage_failure_prevents_reset_and_retry_retains_the_attempt() {
        let e = short_attempt();
        e.db.fail_fight_writes(true);
        assert!(!e.reset_combat("manual"));
        assert_eq!(
            e.storage.get_combat_snapshot_light()[&50000].total_damage,
            300
        );
        e.db.fail_fight_writes(false);
        assert!(e.reset_combat("manual"));
        assert_eq!(
            e.db.fight_detail("auto_50000_1000").unwrap().unwrap()["total_damage"],
            300
        );
        a2tools_dps_meter_lib::clock::set_override(None);
    }
    #[test]
    fn independent_math_agrees_in_live_details_history_and_replay() {
        use a2tools_dps_meter_lib::entity::damage_packet::ParsedDamagePacket;
        use a2tools_dps_meter_lib::entity::special_damage::SpecialDamage;
        let e = Engine::new(Db::in_memory().unwrap(), "de", std::env::temp_dir());
        e.set_target_mode("mostDamage");
        e.storage.set_local_player_id(Some(2259));
        // Independent oracle: 100+300+600=1000, common 10 s, Me=40 DPS/40%, Other=60 DPS/60%.
        for (ts, actor, damage, crit) in [
            (1000, 2259, 100, true),
            (3000, 2259, 300, false),
            (11000, 2260, 600, false),
        ] {
            a2tools_dps_meter_lib::clock::set_override(Some(ts));
            let mut p = ParsedDamagePacket::new();
            p.set_actor_id(actor);
            p.set_target_id(50000);
            p.set_skill_code(11010000);
            p.set_damage(damage);
            if crit {
                p.set_specials(vec![SpecialDamage::Critical]);
            }
            e.storage.append_damage(p);
            e.storage
                .append_nickname_authoritative(actor, if actor == 2259 { "Me" } else { "Other" });
            if actor == 2259 {
                e.storage.append_heal(actor, 17010000, 100, false);
            }
            e.replay_tick();
        }
        let live = e.live();
        assert_eq!(live.battle_time_ms, 10000);
        assert_eq!(live.total_damage, 1000.0);
        let me = live.rows.iter().find(|r| r.is_self).unwrap();
        assert_eq!(me.damage, 400.0);
        assert_eq!(me.dps, 40.0);
        assert_eq!(me.share, 40.0);
        assert_eq!(me.heal, 200.0);
        assert_eq!(me.hps, 20.0);
        let detail = e.player_details(2259);
        let skill = &detail["skills"][0];
        assert_eq!(skill["damage"], 400);
        assert_eq!(skill["hits"], 2);
        assert_eq!(skill["average"], 200.0);
        assert_eq!(skill["min"], 100);
        assert_eq!(skill["max"], 300);
        assert_eq!(skill["crit_rate"], 50.0);
        assert_eq!(skill["dps"], 40.0);
        let report = e.replay_report();
        assert_eq!(report["live"]["total_damage"], 1000.0);
        let saved = e.db.fight_detail("auto_50000_1000").unwrap().unwrap();
        assert_eq!(saved["total_damage"], 1000);
        let saved_me = saved["players"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["is_self"] == 1)
            .unwrap();
        assert_eq!(saved_me["damage"], 400);
        assert_eq!(saved_me["dps"], 40.0);
        assert_eq!(saved_me["skills"][0]["crit_rate"], 50.0);
        // Idle must not enlarge the combat window; burst expires independently.
        a2tools_dps_meter_lib::clock::set_override(Some(21000));
        e.replay_tick();
        assert_eq!(e.live().battle_time_ms, 10000);
        a2tools_dps_meter_lib::clock::set_override(None);
    }
    #[test]
    fn shutdown_keeps_short_attempt_and_reports_storage_failure() {
        let e = short_attempt();
        assert!(e.shutdown());
        assert!(e.is_stopping());
        assert_eq!(
            e.db.fight_detail("auto_50000_1000").unwrap().unwrap()["total_damage"],
            300
        );
        assert!(e.shutdown());
        let failed = short_attempt();
        failed.db.fail_fight_writes(true);
        assert!(!failed.shutdown());
        assert!(!failed.shutdown());
        assert_eq!(
            failed.storage.get_combat_snapshot_light()[&50000].total_damage,
            300
        );
        a2tools_dps_meter_lib::clock::set_override(None);
    }
    #[test]
    fn corrupt_saved_settings_are_normalized_without_losing_valid_fields() {
        let db = Db::in_memory().unwrap();
        db.set_meta("overlay", r#"{"max_rows":0,"scale":999,"opacity":-1,"theme":"invalid","metric":"invalid","compact":true}"#).unwrap();
        let e = Engine::new(db, "de", std::env::temp_dir());
        let s = e.overlay.read();
        assert_eq!(s.max_rows, 1);
        assert_eq!(s.scale, 2.5);
        assert_eq!(s.opacity, 0.0);
        assert_eq!(s.theme, "midnight");
        assert_eq!(s.metric, "damage");
        assert!(s.compact);
        drop(s);
        e.modify_overlay(|s| {
            s.scale = f32::NAN;
            s.opacity = f32::INFINITY;
        })
        .unwrap();
        assert_eq!(e.overlay.read().scale, 1.0);
        assert_eq!(e.overlay.read().opacity, 0.72);
    }
    #[test]
    fn saved_settings_with_one_damaged_field_keep_position_and_profiles() {
        let db = Db::in_memory().unwrap();
        db.set_meta(
            "overlay",
            r#"{"scale":"oops","position":[640,360],"theme":"ember","locked":true,"unknown_old_key":1}"#,
        )
        .unwrap();
        let e = Engine::new(db, "de", std::env::temp_dir());
        let s = e.overlay.read().clone();
        assert_eq!(s.position, Some([640.0, 360.0]));
        assert_eq!(s.theme, "ember");
        assert!(s.locked);
        assert_eq!(s.scale, 1.0);
    }
    #[test]
    fn live_healing_and_burst_follow_real_aggregates_and_decay_when_idle() {
        use a2tools_dps_meter_lib::entity::damage_packet::ParsedDamagePacket;
        let e = Engine::new(Db::in_memory().unwrap(), "de", std::env::temp_dir());
        e.set_target_mode("mostDamage");
        e.storage.set_local_player_id(Some(2259));
        for (ts, damage) in [(1000, 100), (2000, 900)] {
            a2tools_dps_meter_lib::clock::set_override(Some(ts));
            let mut p = ParsedDamagePacket::new();
            p.set_actor_id(2259);
            p.set_target_id(50_000);
            p.set_skill_code(11010000);
            p.set_damage(damage);
            e.storage.append_damage(p);
            e.storage.append_nickname_authoritative(2259, "Me");
            e.storage.append_heal(2259, 17010000, 50, false);
            e.replay_tick();
        }
        let live = e.live();
        let me = live.rows.iter().find(|r| r.is_self).unwrap();
        assert_eq!(me.damage, 1000.0);
        assert_eq!(me.heal, 100.0);
        assert_eq!(me.burst_dps, 200.0);
        a2tools_dps_meter_lib::clock::set_override(Some(8000));
        e.replay_tick();
        assert_eq!(
            e.live().rows.iter().find(|r| r.is_self).unwrap().burst_dps,
            0.0
        );
        a2tools_dps_meter_lib::clock::set_override(None);
    }

    #[test]
    fn saved_boss_fights_keep_full_partner_names_for_history_and_partners() {
        use a2tools_dps_meter_lib::combat::data_storage::PartyMember;
        use a2tools_dps_meter_lib::entity::damage_packet::ParsedDamagePacket;
        let e = Engine::new(Db::in_memory().unwrap(), "de", std::env::temp_dir());
        e.storage.set_local_player_id(Some(2259));
        e.storage.append_mob(50_000, 2000412); // a boss in the NPC table
        e.storage.set_party_roster(
            vec![
                (
                    "Me".into(),
                    PartyMember {
                        slot: 1,
                        dbid: 11,
                        ..Default::default()
                    },
                ),
                (
                    "Moonlight".into(),
                    PartyMember {
                        slot: 2,
                        dbid: 22,
                        ..Default::default()
                    },
                ),
            ],
            true,
        );
        for ts in (1000..=21000).step_by(1000) {
            a2tools_dps_meter_lib::clock::set_override(Some(ts));
            for (actor, name) in [(2259, "Me"), (2260, "Moonlight")] {
                let mut p = ParsedDamagePacket::new();
                p.set_actor_id(actor);
                p.set_target_id(50_000);
                p.set_skill_code(11010000);
                p.set_damage(100);
                e.storage.append_damage(p);
                e.storage.append_nickname_authoritative(actor, name);
            }
            e.replay_tick();
        }
        // The periodic upstream snapshot path, which masks names for uploads.
        assert!(e.save_fights(false));
        let f = e.db.fight_detail("auto_50000_1000").unwrap().unwrap();
        let mut names: Vec<_> = f["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["Me", "Moonlight"]);
        a2tools_dps_meter_lib::clock::set_override(None);
    }
}
