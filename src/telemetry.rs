//! Bounded, local combat evidence. No casts are inferred from damage ticks.
//! Frame layouts checked against SkeeveAN/Aion-DPS-Meter fafba135 (MIT).
//! See docs/UPSTREAM_INSPIRATION.md. Protocol coverage still needs live EU acceptance.
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::Instant;

const LIMIT: usize = 50_000;
const KEEP_MS: i64 = 7_200_000;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Event {
    pub kind: String,
    pub at_ms: i64,
    pub actor: i32,
    pub target: i32,
    pub skill: i32,
    pub amount: i64,
    pub duration_ms: Option<i64>,
    pub position: Option<[f32; 2]>,
    pub facing: Option<f32>,
    pub outcome: Option<String>,
}
#[derive(Default)]
struct Family {
    count: u64,
    previous: u64,
    established: u32,
    silent: u32,
    last_ms: Option<i64>,
}
struct Cooldown {
    family: i32,
    sequence: u64,
    at: i64,
    raw: u32,
    observed: Instant,
}
#[derive(Default)]
struct State {
    events: VecDeque<Event>,
    lost: VecDeque<(i64, i64)>,
    families: BTreeMap<&'static str, Family>,
    window: Option<i64>,
    unknown: BTreeMap<u16, u64>,
    rejected: u64,
    frames: u64,
    last_frame: Option<i64>,
    cooldowns: HashMap<i32, Cooldown>,
    casts: HashMap<i32, (Event, Instant)>,
    replay: bool,
    cooldown_sequence: u64,
    identity: Option<i32>,
}
#[derive(Default)]
pub struct Telemetry(Mutex<State>);
impl Telemetry {
    pub fn record(&self, event: Event) {
        let mut s = self.0.lock();
        // A repeated HP sample is harmless, but never coalesce skill uses by time:
        // two skills can legitimately be used in the same network timestamp.
        if matches!(event.kind.as_str(), "windup" | "skill_use") {
            if s.casts.len() >= 256 && !s.casts.contains_key(&event.actor) {
                if let Some(old) = s
                    .casts
                    .iter()
                    .min_by_key(|(_, (e, _))| e.at_ms)
                    .map(|(id, _)| *id)
                {
                    s.casts.remove(&old);
                }
            }
            s.casts.insert(event.actor, (event.clone(), Instant::now()));
        }
        s.events.push_back(event);
        let now = s.events.back().map_or(0, |e| e.at_ms);
        while s.events.len() > LIMIT || s.events.front().is_some_and(|e| now - e.at_ms > KEEP_MS) {
            if let Some(e) = s.events.pop_front() {
                note_loss(&mut s, e.at_ms, e.at_ms.saturating_add(1));
            }
        }
    }
    pub fn set_replay(&self) {
        self.0.lock().replay = true;
    }
    pub fn boundary(&self, at: i64) {
        self.0.lock().casts.clear();
        self.record(Event {
            kind: "boundary".into(),
            at_ms: at,
            ..Default::default()
        });
    }
    pub fn gap(&self, at: i64) {
        note_loss(
            &mut self.0.lock(),
            at.saturating_sub(2000),
            at.saturating_add(1),
        );
    }
    pub fn identity(&self, id: Option<i32>) {
        let mut s = self.0.lock();
        if s.identity != id {
            s.cooldowns.clear();
            s.identity = id;
        }
    }
    pub fn frame(&self, payload: &[u8], at: i64) {
        let p = strip_flag(payload);
        let Some(op) = opcode(p) else { return };
        let family = match op {
            0x0438 => "damage",
            0x0538 => "dot_heal",
            0x008d => "hp",
            0x0238 => "skill_use",
            0x0938 => "windup",
            0x2a38 | 0x2b38 | 0x2c38 => "effects",
            0x4738 => "cooldowns",
            0x3336 | 0x5936 => "identity",
            _ => "other",
        };
        {
            let mut s = self.0.lock();
            if s.window.is_some_and(|w| at - w >= 30_000) {
                let windows = ((at - s.window.unwrap()) / 30_000).min(100) as u32;
                for f in s.families.values_mut() {
                    if f.count >= 5 {
                        f.established = (f.established + 1).min(100);
                        f.silent = 0;
                    } else if f.count == 0 {
                        f.silent = f.silent.saturating_add(windows);
                    }
                    f.previous = f.count;
                    f.count = 0;
                }
                s.window = Some(at);
            }
            s.window.get_or_insert(at);
            s.frames += 1;
            s.last_frame = Some(at);
            let f = s.families.entry(family).or_default();
            f.count += 1;
            f.last_ms = Some(at);
            if family == "other" && (s.unknown.len() < 128 || s.unknown.contains_key(&op)) {
                *s.unknown.entry(op).or_default() += 1;
            }
        }
        let parsed = match op {
            0x0238 | 0x0938 => parse_aim(p, at),
            0x008d => parse_hp(p, at),
            0x3c37 => parse_position(p, at, Some(3)),
            0x1a37 | 0x1b37 | 0x1c37 => parse_position(p, at, None),
            0x4236 => parse_death(p, at),
            0x05e0 => parse_stagger(p, at),
            0x2a38 | 0x2b38 => {
                let identity = self.0.lock().identity;
                if let Some(event) = parse_effect_position(p, at, identity) {
                    self.record(event);
                }
                return;
            }
            0x4738 => {
                match parse_cooldowns(p) {
                    Some(entries) => {
                        let mut s = self.0.lock();
                        for (skill, value) in entries {
                            if s.cooldowns.len() < 128 || s.cooldowns.contains_key(&skill) {
                                s.cooldown_sequence += 1;
                                let sequence = s.cooldown_sequence;
                                s.cooldowns.insert(
                                    skill,
                                    Cooldown {
                                        family: skill_family(skill),
                                        sequence,
                                        at,
                                        raw: value,
                                        observed: Instant::now(),
                                    },
                                );
                            }
                        }
                    }
                    None => self.0.lock().rejected += 1,
                };
                return;
            }
            _ => return,
        };
        match parsed {
            Some(e) => self.record(e),
            None => self.0.lock().rejected += 1,
        };
    }
    pub fn health(&self, now: i64) -> Value {
        let s = self.0.lock();
        let rows:Vec<_>=s.families.iter().map(|(name,f)|json!({"family":name,"count":f.count,"previous_count":f.previous,"last_ms":f.last_ms,"established_windows":f.established})).collect();
        // Silence alone is not a combat failure. Demand current cast activity,
        // a previously established damage stream and several complete silent windows.
        let casts = s
            .families
            .get("skill_use")
            .is_some_and(|f| f.count >= 5 && f.last_ms.is_some_and(|t| now - t < 30_000));
        let suspicious = casts
            && s.families
                .get("damage")
                .is_some_and(|f| f.established >= 3 && f.silent >= 4 && f.count == 0);
        json!({"state":if suspicious{"suspected_break"}else if s.frames==0{"unknown"}else{"observing"},"families":rows,"frames":s.frames,"rejected_layouts":s.rejected,"last_frame_ms":s.last_frame,"unknown_opcodes":s.unknown.iter().map(|(op,n)|json!({"opcode":format!("{op:04x}"),"count":n})).collect::<Vec<_>>(),"message":if suspicious{"Spielverkehr und Skill-Nutzungen erkannt, etablierte Schadenspakete bleiben aus. Parser prüfen."}else{"Beobachtete Paketfamilien. Empfang allein bestätigt keine Messgenauigkeit."}})
    }
    pub fn cooldowns(&self, now: i64, skills: &[i32], factor: Option<f64>) -> Value {
        let s = self.0.lock();
        let factor = factor.filter(|f| f.is_finite() && (0.1..=10.0).contains(f));
        json!(skills.iter().take(8).map(|id|{
            let elapsed=|c:&Cooldown|if s.replay{now.saturating_sub(c.at).max(0) as f64}else{c.observed.elapsed().as_secs_f64()*1000.0};
            let family=skill_family(*id);
            let reading=s.cooldowns.values().filter(|c|c.family==family && elapsed(c)<120_000.0).max_by_key(|c|c.sequence);
            let seconds=reading.and_then(|c|if c.raw==0{Some(0.0)}else{factor.map(|f|(c.raw as f64/f/1000.0-elapsed(c)/1000.0).max(0.0))});
            json!({"skill":id,"raw":reading.map(|c|c.raw),"seconds":seconds,"estimated":reading.is_some_and(|c|c.raw>0),"available":reading.is_some(),"unit_confirmed":false})
        }).collect::<Vec<_>>())
    }
    pub fn report(&self, from: i64, to: i64, boss: i32, actors: &[i32]) -> Value {
        let s = self.0.lock();
        let allowed = |id: i32| id == boss || actors.contains(&id);
        let lower = s
            .events
            .iter()
            .filter(|e| e.kind == "boundary" && e.at_ms <= from)
            .map(|e| e.at_ms)
            .max()
            .unwrap_or(i64::MIN)
            .max(from.saturating_sub(2000));
        let upper = s
            .events
            .iter()
            .filter(|e| e.kind == "boundary" && e.at_ms > from)
            .map(|e| e.at_ms)
            .min()
            .unwrap_or(i64::MAX)
            .min(to.saturating_add(3000));
        let mut events: Vec<_> = s
            .events
            .iter()
            .filter(|e| {
                e.at_ms >= lower && e.at_ms <= upper && (allowed(e.actor) || allowed(e.target))
            })
            .filter(|e| e.kind != "position" || allowed(e.actor))
            .map(|e| {
                let mut e = e.clone();
                e.at_ms -= from;
                e
            })
            .collect();
        events.sort_by_key(|e| e.at_ms);
        let partial = s.lost.iter().any(|(a, b)| *a <= to && *b >= from);
        let mut skills: BTreeMap<(i32, i32), Vec<i64>> = BTreeMap::new();
        for e in &events {
            if e.kind == "skill_use" && actors.contains(&e.actor) {
                skills
                    .entry((e.actor, skill_family(e.skill)))
                    .or_default()
                    .push(e.at_ms);
            }
        }
        let duration = (to - from).max(1000) as f64 / 1000.0;
        let rotations:Vec<_>=skills.iter().map(|((actor,skill),times)|json!({"actor":actor,"skill":skill,"uses":times.len(),"per_minute":times.len() as f64*60.0/duration,"first_ms":times[0],"mean_gap_ms":if times.len()>1{Some((times.last().unwrap()-times[0]) as f64/(times.len()-1) as f64)}else{None}})).collect();
        let abilities = boss_abilities(&events, boss, actors);
        let deaths:Vec<_>=events.iter().filter(|e|e.kind=="death" && actors.contains(&e.target)).map(|d|{
            let hits:Vec<_>=events.iter().filter(|h|h.kind=="hit" && h.target==d.target && h.at_ms<=d.at_ms && h.at_ms>=d.at_ms-5000).collect();
            let revived=events.iter().find(|h|h.kind=="hp" && h.actor==d.target && h.amount>0 && h.at_ms>d.at_ms).map(|h|h.at_ms);
            json!({"actor":d.target,"at_ms":d.at_ms,"revived_ms":revived,"last_hits":hits,"cause_confirmed":false})
        }).collect();
        json!({"version":1,"partial":partial,"coverage":"observed_events","events":events,"rotations":rotations,"deaths":deaths,"boss_abilities":abilities,"hp_check":hp_check(&events,boss,partial),"position_method":"sparse_server_readings","aoe_geometry_available":false,"pre_window_ms":2000,"post_window_ms":3000,"damage_duration_ms":to-from})
    }
    pub fn live_casts(&self, now: i64, boss: i32) -> Value {
        let s = self.0.lock();
        let latest = s.casts.get(&boss);
        json!(latest.and_then(|(e,observed)|{
            let elapsed=if s.replay{now.saturating_sub(e.at_ms).max(0)}else{observed.elapsed().as_millis() as i64};
            e.duration_ms.filter(|d|e.kind=="windup" && elapsed<*d).map(|duration|json!({"skill":e.skill,"target":e.target,"remaining_ms":duration-elapsed,"duration_ms":duration,"estimated":false,"completion_confirmed":false}))
        }))
    }
}
fn skill_family(id: i32) -> i32 {
    crate::skills::metadata(i64::from(id))
        .and_then(|m| m["skill_id"].as_i64())
        .map(|v| v as i32)
        .unwrap_or(id)
}
fn note_loss(s: &mut State, a: i64, b: i64) {
    if let Some(last) = s.lost.back_mut()
        && a <= last.1
    {
        last.0 = last.0.min(a);
        last.1 = last.1.max(b);
    } else {
        s.lost.push_back((a, b));
    }
    while s.lost.len() > 64 {
        let old = s.lost.pop_front().unwrap();
        s.lost.front_mut().unwrap().0 = old.0;
    }
}
#[derive(Default, Serialize)]
struct Ability {
    skill: i32,
    casts: usize,
    hits: usize,
    damage: i64,
    targets: Vec<i32>,
    blocks: usize,
    parries: usize,
    avoids: usize,
}
fn boss_abilities(events: &[Event], boss: i32, actors: &[i32]) -> Value {
    let mut map: BTreeMap<i32, Ability> = BTreeMap::new();
    for e in events.iter().filter(|e| e.actor == boss) {
        if e.kind == "skill_use"
            || (matches!(e.kind.as_str(), "hit" | "defense") && actors.contains(&e.target))
        {
            let row = map.entry(e.skill).or_insert_with(|| Ability {
                skill: e.skill,
                ..Default::default()
            });
            if e.kind == "skill_use" {
                row.casts += 1;
            } else {
                if e.kind == "hit" {
                    row.hits += 1;
                    row.damage = row.damage.saturating_add(e.amount);
                }
                let outcome = e.outcome.as_deref().unwrap_or("");
                if outcome.contains("ShieldBlock") || outcome.contains("PerfectBlock") {
                    row.blocks += 1;
                }
                if outcome.contains("Parry") {
                    row.parries += 1;
                }
                if e.kind == "defense" && matches!(outcome, "Miss" | "Resist") {
                    row.avoids += 1;
                }
                if !row.targets.contains(&e.target) {
                    row.targets.push(e.target);
                }
            }
        }
    }
    json!(map.into_values().collect::<Vec<_>>())
}

fn hp_check(events: &[Event], boss: i32, partial: bool) -> Value {
    let hp: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "hp" && e.actor == boss)
        .collect();
    let reason = if partial {
        Some("Datenlücke im Vergleichsfenster")
    } else if hp.len() < 2 {
        Some("Zu wenige HP-Messungen")
    } else {
        None
    };
    if let Some(reason) = reason {
        return json!({"state":"unavailable","reason":reason});
    }
    let first = hp[0];
    let last = hp[hp.len() - 1];
    // HP increases and long missing intervals are ambiguous. Never label them shields.
    if hp
        .windows(2)
        .any(|p| p[1].amount > p[0].amount || p[1].at_ms - p[0].at_ms > 3000)
        || last.amount == 0
    {
        return json!({"state":"unavailable","reason":"Heilung, HP-Lücke oder Overkill im Vergleichsfenster"});
    }
    let lost = first.amount - last.amount;
    let counted: i64 = events
        .iter()
        .filter(|e| {
            e.kind == "hit" && e.target == boss && e.at_ms > first.at_ms && e.at_ms <= last.at_ms
        })
        .map(|e| e.amount)
        .sum();
    if lost <= 0 || counted <= 0 {
        return json!({"state":"unavailable","reason":"Kein auswertbarer HP-Verlust"});
    }
    let deviation = (counted - lost) as f64 / lost as f64 * 100.0;
    json!({"state":if deviation.abs()<=1.0{"plausible"}else{"deviation"},"lost_hp":lost,"counted_damage":counted,"deviation_percent":deviation,"from_ms":first.at_ms,"to_ms":last.at_ms,"scope":"all_observed_hits_on_target","accuracy_proven":false})
}
pub fn strip_flag(p: &[u8]) -> &[u8] {
    if p.first().is_some_and(|b| (0xf0..=0xfe).contains(b)) {
        &p[1..]
    } else {
        p
    }
}
fn opcode(p: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes(p.get(..2)?.try_into().ok()?))
}
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 2 }
    }
    fn byte(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.pos = self.pos.checked_add(n)?;
        (self.pos <= self.data.len()).then_some(())
    }
    fn var(&mut self) -> Option<u32> {
        let mut n = 0u32;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            if shift == 28 && b > 15 {
                return None;
            }
            n |= u32::from(b & 127) << shift;
            if b & 128 == 0 {
                return Some(n);
            }
        }
        None
    }
    fn id(&mut self) -> Option<i32> {
        let v = self.var()?;
        (v > 0 && v <= 10_000_000).then_some(v as i32)
    }
    fn u32(&mut self) -> Option<u32> {
        let v = u32::from_le_bytes(self.data.get(self.pos..self.pos + 4)?.try_into().ok()?);
        self.pos += 4;
        Some(v)
    }
    fn i64(&mut self) -> Option<i64> {
        let v = i64::from_le_bytes(self.data.get(self.pos..self.pos + 8)?.try_into().ok()?);
        self.pos += 8;
        Some(v)
    }
    fn float(&mut self) -> Option<f32> {
        let v = f32::from_bits(self.u32()?);
        (v.is_finite() && v.abs() < 10_000_000.0).then_some(v)
    }
    fn end(&self) -> bool {
        self.pos == self.data.len()
    }
}
fn parse_aim(p: &[u8], at: i64) -> Option<Event> {
    let mut r = Reader::new(p);
    let actor = r.id()?;
    r.skip(1)?;
    let skill = r.u32()?;
    if !(1..=300_000_000).contains(&skill) {
        return None;
    }
    r.skip(1)?;
    let flags = r.byte()?;
    let target = r.var()?;
    if target > 10_000_000 {
        return None;
    }
    let facing = r.float()?;
    if facing.abs() > 720.0 {
        return None;
    }
    if flags & 2 != 0 {
        r.float()?;
        r.float()?;
        r.float()?;
    } // aim vectors, never world coordinates
    let duration = if p[0] == 9 {
        let d = r.var()?;
        if !(50..=120_000).contains(&d) {
            return None;
        }
        Some(i64::from(d))
    } else {
        None
    };
    Some(Event {
        kind: if duration.is_some() {
            "windup"
        } else {
            "skill_use"
        }
        .into(),
        at_ms: at,
        actor,
        target: target as i32,
        skill: skill as i32,
        duration_ms: duration,
        facing: Some(facing),
        ..Event::default()
    })
}
fn parse_hp(p: &[u8], at: i64) -> Option<Event> {
    let mut r = Reader::new(p);
    let actor = r.id()?;
    let flags = r.byte()?;
    if flags & !3 != 0 {
        return None;
    }
    if flags & 1 != 0 {
        let n = r.byte()? as usize;
        r.skip(n * 5)?;
    }
    let mut hp = None;
    if flags & 2 != 0 {
        let n = r.byte()?;
        for _ in 0..n {
            let kind = r.byte()?;
            let value = r.i64()?;
            if kind == 0 {
                hp = Some(value);
            }
        }
    }
    let amount = hp?;
    if !r.end() || amount < 0 {
        return None;
    }
    Some(Event {
        kind: "hp".into(),
        at_ms: at,
        actor,
        amount,
        ..Event::default()
    })
}
fn parse_death(p: &[u8], at: i64) -> Option<Event> {
    let mut r = Reader::new(p);
    let target = r.id()?;
    if r.var()? != 0 || r.var()? != 3 || !r.end() {
        return None;
    }
    Some(Event {
        kind: "death".into(),
        at_ms: at,
        target,
        ..Event::default()
    })
}

fn parse_effect_position(p: &[u8], at: i64, local: Option<i32>) -> Option<Event> {
    let buff = crate::buffs::parse(p)?;
    if Some(buff.caster as i32) != local {
        return None;
    }
    let mut header = Reader::new(p);
    header.id()?;
    header.skip(if p[0] == 0x2a { 2 } else { 1 })?;
    header.var()?;
    if header.u32()? != buff.code {
        return None;
    }
    header.skip(16)?;
    if header.id()? != buff.caster as i32 || p.len() < header.pos + 13 {
        return None;
    }
    let mut r = Reader {
        data: p,
        pos: p.len() - 12,
    };
    let x = r.float()?;
    let y = r.float()?;
    r.float()?;
    if x.abs() > 500_000.0 || y.abs() > 500_000.0 {
        return None;
    }
    Some(Event {
        kind: "position".into(),
        at_ms: at,
        actor: buff.caster as i32,
        position: Some([x, y]),
        ..Event::default()
    })
}
fn parse_position(p: &[u8], at: i64, local: Option<u8>) -> Option<Event> {
    let mut r = Reader::new(p);
    let actor = r.id()?;
    if let Some(flag) = local {
        if r.byte()? != flag {
            return None;
        }
    } else {
        r.skip(2)?;
    }
    let x = r.float()?;
    let y = r.float()?;
    if x.abs() > 500_000.0 || y.abs() > 500_000.0 {
        return None;
    }
    Some(Event {
        kind: "position".into(),
        at_ms: at,
        actor,
        position: Some([x, y]),
        ..Event::default()
    })
}
fn parse_stagger(p: &[u8], at: i64) -> Option<Event> {
    let mut r = Reader::new(p);
    let actor = r.id()?;
    if r.byte()? != 0 || r.byte()? != 3 || !r.end() {
        return None;
    }
    Some(Event {
        kind: "stagger".into(),
        at_ms: at,
        actor,
        ..Event::default()
    })
}
fn parse_cooldowns(p: &[u8]) -> Option<Vec<(i32, u32)>> {
    let mut r = Reader::new(p);
    let count = r.byte()?;
    if count > 128 {
        return None;
    }
    let mut entries = Vec::new();
    for _ in 0..count {
        let id = r.u32()?;
        let value = r.var()?;
        if value > 864_000_000 {
            return None;
        }
        if (10_000_000..=300_000_000).contains(&id) {
            entries.push((id as i32, value));
        }
    }
    r.end().then_some(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(kind: &str, at: i64, actor: i32, target: i32, amount: i64) -> Event {
        Event {
            kind: kind.into(),
            at_ms: at,
            actor,
            target,
            amount,
            skill: 11010000,
            ..Event::default()
        }
    }
    #[test]
    fn rotation_is_not_hit_count() {
        let t = Telemetry::default();
        for i in 0..10 {
            t.record(e("hit", 1000 + i, 1, 50, 100));
        }
        t.record(e("skill_use", 1000, 1, 50, 0));
        assert_eq!(t.report(1000, 2000, 50, &[1])["rotations"][0]["uses"], 1);
    }
    #[test]
    fn hp_requires_matching_window_and_rejects_gaps() {
        let t = Telemetry::default();
        t.record(e("hp", 0, 50, 0, 1000));
        t.record(e("hit", 1000, 1, 50, 100));
        t.record(e("hp", 2000, 50, 0, 900));
        assert_eq!(
            t.report(0, 2000, 50, &[1])["hp_check"]["state"],
            "plausible"
        );
        t.gap(1000);
        assert_eq!(
            t.report(0, 2000, 50, &[1])["hp_check"]["state"],
            "unavailable"
        );
    }
    #[test]
    fn packets_are_bounded_and_unknown_units_stay_unknown() {
        let t = Telemetry::default();
        let mut p = vec![0x47, 0x38, 1];
        p.extend(11010000u32.to_le_bytes());
        p.push(100);
        t.frame(&p, 1000);
        assert!(t.cooldowns(1001, &[11010000], None)[0]["seconds"].is_null());
        assert!(
            t.cooldowns(1001, &[11010000], Some(2.4))[0]["estimated"]
                .as_bool()
                .unwrap()
        );
        p.pop();
        t.frame(&p, 2000);
        assert_eq!(t.health(2000)["rejected_layouts"], 1);
    }
    #[test]
    fn idle_is_not_parser_failure() {
        let t = Telemetry::default();
        for i in 0..20 {
            t.frame(&[0, 0x36], i * 30000);
        }
        assert_eq!(t.health(600000)["state"], "observing");
    }
    #[test]
    fn death_is_not_guessed_from_missing_hp() {
        let t = Telemetry::default();
        t.record(e("hp", 0, 1, 0, 0));
        assert_eq!(
            t.report(0, 1000, 50, &[1])["deaths"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn malformed_frames_never_panic() {
        for op in [
            [2, 0x38],
            [9, 0x38],
            [0, 0x8d],
            [0x47, 0x38],
            [4, 0x8d],
            [0x3c, 0x37],
        ] {
            for len in 0..100 {
                let mut p = op.to_vec();
                p.extend(vec![255; len]);
                Telemetry::default().frame(&p, 0);
            }
        }
    }
    #[test]
    fn ownership_is_not_death_and_post_hit_deaths_are_retained() {
        let t = Telemetry::default();
        t.frame(&[4, 0x8d, 42], 1000);
        assert!(
            t.report(0, 1000, 50, &[42])["deaths"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        t.frame(&[0x42, 0x36, 42, 0, 3], 1100);
        assert_eq!(t.report(0, 1000, 50, &[42])["deaths"][0]["at_ms"], 1100);
    }
    #[test]
    fn reset_boundary_prevents_next_attempt_events_leaking_back() {
        let t = Telemetry::default();
        t.record(e("skill_use", 1000, 1, 50, 0));
        t.boundary(2000);
        t.record(e("skill_use", 2100, 1, 50, 0));
        assert_eq!(t.report(1000, 1500, 50, &[1])["rotations"][0]["uses"], 1);
    }
    #[test]
    fn established_damage_silence_needs_current_skill_activity() {
        let t = Telemetry::default();
        let mut cast = vec![2, 0x38, 42, 0];
        cast.extend(11010000u32.to_le_bytes());
        cast.extend([0, 0, 50]);
        cast.extend(0f32.to_le_bytes());
        for window in 0..3 {
            for i in 0..6 {
                t.frame(&[4, 0x38], window * 30000 + i);
            }
        }
        for window in 3..=7 {
            for i in 0..6 {
                t.frame(&cast, window * 30000 + i);
            }
        }
        assert_eq!(t.health(210005)["state"], "suspected_break");
        assert_eq!(t.health(300000)["state"], "observing");
    }
    #[test]
    fn live_countdowns_ignore_wall_clock_jumps_and_replay_uses_capture_time() {
        let mut packet = vec![0x47, 0x38, 1];
        packet.extend(11010000u32.to_le_bytes());
        packet.extend([0x90, 0x4e]);
        let t = Telemetry::default();
        t.frame(&packet, 1000);
        assert!(
            t.cooldowns(600000, &[11010000], Some(1.0))[0]["seconds"]
                .as_f64()
                .unwrap()
                > 9.0
        );
        let replay = Telemetry::default();
        replay.set_replay();
        replay.frame(&packet, 1000);
        assert_eq!(
            replay.cooldowns(6000, &[11010000], Some(1.0))[0]["seconds"],
            5.0
        );
        t.record(Event {
            kind: "windup".into(),
            at_ms: 1000,
            actor: 50,
            duration_ms: Some(5000),
            ..Event::default()
        });
        assert!(t.live_casts(600000, 50)["remaining_ms"].as_i64().unwrap() > 4000);
        t.boundary(2000);
        assert!(t.live_casts(600000, 50).is_null());
    }
    #[test]
    fn newest_skill_variant_reset_wins_for_selected_family() {
        let t = Telemetry::default();
        t.set_replay();
        let mut packet = vec![0x47, 0x38, 1];
        packet.extend(11170000u32.to_le_bytes());
        packet.push(100);
        t.frame(&packet, 1000);
        let mut reset = vec![0x47, 0x38, 1];
        reset.extend(11170010u32.to_le_bytes());
        reset.push(0);
        t.frame(&reset, 1001);
        assert_eq!(t.cooldowns(1001, &[11170000], Some(1.0))[0]["seconds"], 0.0);
    }
}
