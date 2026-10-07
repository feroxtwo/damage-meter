//! Buffs and debuffs: who had which effect up, and for how long.
//!
//! The A2Tools parser does not read the game's effect packets, so this module
//! does. The packet layout comes from the open meters NOIA2
//! (github.com/ZDYoung0519/NOIA2, `parser/buff.rs`) and Kuroukihime's
//! AIon2-Dps-Meter (`BuffPacketProcessor.cs`), both GPL-3.0:
//!
//! ```text
//! 2A 38 | 2B 38               opcode (2B: refresh)
//! <varint target>
//! 2 bytes                     unknown
//! <varint>                    unknown
//! u32 LE effect code          110 000 000..=200 000 000 or 20 000 000..30 000 000
//! u32 LE duration ms, 4 bytes unknown
//! u64 LE server time
//! <varint caster>
//! ```
//!
//! An effect counts from the moment its packet arrives for its duration; a
//! refresh before it runs out extends the same interval.

use std::collections::{HashMap, VecDeque};

use parking_lot::Mutex;

/// A permanent effect (auras, passives) says nothing about play.
const PERMANENT: u32 = u32::MAX;
/// Effects shorter or longer than this are noise or out-of-combat food.
const MIN_DURATION_MS: u32 = 100;
const MAX_DURATION_MS: u32 = 3_600_000;
/// A refresh this soon after the end still continues the same interval.
const MERGE_TOLERANCE_MS: i64 = 100;
/// Intervals older than this are dropped.
const KEEP_MS: i64 = 2 * 3_600_000;
const INTERVALS_PER_EFFECT: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuffEvent {
    pub target: u32,
    pub caster: u32,
    pub code: u32,
    pub duration_ms: u32,
}

fn varint(data: &[u8], offset: usize) -> Option<(u32, usize)> {
    let mut value = 0u32;
    let mut shift = 0u32;
    for (i, &b) in data.get(offset..)?.iter().enumerate() {
        value |= u32::from(b & 0x7F) << shift;
        if b & 0x80 == 0 {
            return Some((value, i + 1));
        }
        shift += 7;
        if shift >= 32 {
            return None;
        }
    }
    None
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn is_effect_code(code: u32) -> bool {
    (110_000_000..=200_000_000).contains(&code) || (20_000_000..30_000_000).contains(&code)
}

/// Read an effect packet. `payload` starts at the opcode (after the frame's
/// length prefix).
pub fn parse(payload: &[u8]) -> Option<BuffEvent> {
    // Some frames carry one flag byte between length and opcode.
    let payload = match payload.first() {
        Some(0xF0..=0xFE) => &payload[1..],
        _ => payload,
    };
    if payload.len() < 2 || !matches!(payload[0], 0x2A | 0x2B) || payload[1] != 0x38 {
        return None;
    }
    let refresh = payload[0] == 0x2B;
    let (target, len) = varint(payload, 2)?;
    let after_target = 2 + len;

    let mut offset = after_target + 2;
    let (_, len) = varint(payload, offset)?;
    offset += len;
    let mut code = u32_at(payload, offset)?;
    offset += 4;

    if refresh && !is_effect_code(code) {
        let retry = after_target + 1;
        if let Some((_, len)) = varint(payload, retry)
            && let Some(c) = u32_at(payload, retry + len).filter(|&c| is_effect_code(c))
        {
            code = c;
            offset = retry + len + 4;
        }
    }
    if !is_effect_code(code) || offset + 16 > payload.len() {
        return None;
    }
    let duration_ms = u32_at(payload, offset)?;
    offset += 16; // duration, 4 unknown bytes, server time
    let (caster, _) = varint(payload, offset)?;
    if caster <= 1
        || duration_ms == PERMANENT
        || !(MIN_DURATION_MS..=MAX_DURATION_MS).contains(&duration_ms)
    {
        return None;
    }
    Some(BuffEvent {
        target,
        caster,
        code,
        duration_ms,
    })
}

/// Candidate keys for an effect's name in the skill table, best first: effect
/// codes are a skill code with one more digit.
pub fn name_keys(code: u32) -> [i32; 4] {
    let base = if code >= 100_000_000 { code / 10 } else { code } as i32;
    [base, base / 100 * 100, base / 10_000 * 10_000, code as i32]
}

#[derive(Debug, Clone, Copy)]
struct Interval {
    start: i64,
    end: i64,
}

/// Uptime of one effect on one entity within a window.
#[derive(Debug, Clone, PartialEq)]
pub struct Uptime {
    pub code: u32,
    /// The caster who kept it up longest.
    pub caster: u32,
    /// 0..=100
    pub percent: f64,
}

type EffectIntervals = HashMap<u32, HashMap<(u32, u32), VecDeque<Interval>>>;
type EffectCoverage = (Vec<(i64, i64)>, u32, i64);

/// Every effect interval seen, by target, then (effect, caster).
#[derive(Default)]
pub struct BuffTracker {
    inner: Mutex<EffectIntervals>,
}

fn covered(intervals: &[(i64, i64)], from: i64, to: i64) -> i64 {
    let mut clipped: Vec<(i64, i64)> = intervals
        .iter()
        .map(|&(s, e)| (s.max(from), e.min(to)))
        .filter(|(s, e)| e > s)
        .collect();
    clipped.sort_unstable();
    let mut total = 0;
    let mut cur: Option<(i64, i64)> = None;
    for (s, e) in clipped {
        match &mut cur {
            Some((_, ce)) if s <= *ce => *ce = (*ce).max(e),
            _ => {
                if let Some((cs, ce)) = cur {
                    total += ce - cs;
                }
                cur = Some((s, e));
            }
        }
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    total
}

impl BuffTracker {
    pub fn record(&self, event: BuffEvent, now_ms: i64) {
        let end = now_ms + event.duration_ms as i64;
        let mut inner = self.inner.lock();
        let list = inner
            .entry(event.target)
            .or_default()
            .entry((event.code, event.caster))
            .or_default();
        match list.back_mut() {
            Some(last) if now_ms <= last.end + MERGE_TOLERANCE_MS => last.end = last.end.max(end),
            _ => list.push_back(Interval { start: now_ms, end }),
        }
        while list.len() > INTERVALS_PER_EFFECT {
            list.pop_front();
        }
        if inner.len() > 4096 {
            Self::prune(&mut inner, now_ms);
        }
    }

    fn prune(inner: &mut EffectIntervals, now_ms: i64) {
        for effects in inner.values_mut() {
            for list in effects.values_mut() {
                while list.front().is_some_and(|i| i.end < now_ms - KEEP_MS) {
                    list.pop_front();
                }
            }
            effects.retain(|_, l| !l.is_empty());
        }
        inner.retain(|_, e| !e.is_empty());
    }

    pub fn timeline(&self, from: i64, to: i64) -> serde_json::Value {
        let mut rows = Vec::new();
        for (&target, effects) in self.inner.lock().iter() {
            for (&(code, caster), list) in effects {
                for i in list {
                    let start = i.start.max(from);
                    let end = i.end.min(to);
                    if end > start {
                        rows.push(serde_json::json!({"target":target,"code":code,"caster":caster,"start_ms":start-from,"end_ms":end-from}));
                    }
                }
            }
        }
        serde_json::json!(rows)
    }
    pub fn prune_old(&self, now_ms: i64) {
        Self::prune(&mut self.inner.lock(), now_ms);
    }

    /// Effects on `target` during `[from, to)`, highest uptime first. An
    /// effect up from several casters counts once.
    pub fn uptimes(&self, target: u32, from: i64, to: i64) -> Vec<Uptime> {
        if to <= from {
            return Vec::new();
        }
        let inner = self.inner.lock();
        let Some(effects) = inner.get(&target) else {
            return Vec::new();
        };
        let mut by_code: HashMap<u32, EffectCoverage> = HashMap::new();
        for (&(code, caster), list) in effects {
            let spans: Vec<(i64, i64)> = list.iter().map(|i| (i.start, i.end)).collect();
            let mine = covered(&spans, from, to);
            if mine == 0 {
                continue;
            }
            let entry = by_code.entry(code).or_insert((Vec::new(), caster, 0));
            entry.0.extend(spans);
            if mine > entry.2 {
                entry.1 = caster;
                entry.2 = mine;
            }
        }
        let window = (to - from) as f64;
        let mut out: Vec<Uptime> = by_code
            .into_iter()
            .map(|(code, (spans, caster, _))| Uptime {
                code,
                caster,
                percent: (covered(&spans, from, to) as f64 * 100.0 / window).min(100.0),
            })
            .collect();
        out.sort_by(|a, b| b.percent.total_cmp(&a.percent).then(a.code.cmp(&b.code)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_varint(mut v: u32, out: &mut Vec<u8>) {
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }

    pub fn packet(opcode: u8, target: u32, code: u32, duration: u32, caster: u32) -> Vec<u8> {
        let mut p = vec![opcode, 0x38];
        enc_varint(target, &mut p);
        p.extend([0x01, 0x00]);
        enc_varint(5, &mut p);
        p.extend(code.to_le_bytes());
        p.extend(duration.to_le_bytes());
        p.extend([0; 4]);
        p.extend(1_700_000_000_000u64.to_le_bytes());
        enc_varint(caster, &mut p);
        p
    }

    #[test]
    fn reads_an_effect_packet() {
        let p = packet(0x2A, 300, 120_340_010, 8_000, 4711);
        assert_eq!(
            parse(&p),
            Some(BuffEvent {
                target: 300,
                caster: 4711,
                code: 120_340_010,
                duration_ms: 8_000
            })
        );
        // With the flag byte in front.
        let mut flagged = vec![0xF3];
        flagged.extend(&p);
        assert!(parse(&flagged).is_some());
    }

    #[test]
    fn skips_permanent_unknown_and_other_packets() {
        assert_eq!(parse(&packet(0x2A, 300, 120_340_010, u32::MAX, 4711)), None);
        assert_eq!(parse(&packet(0x2A, 300, 5, 8_000, 4711)), None);
        assert_eq!(parse(&packet(0x2A, 300, 120_340_010, 8_000, 1)), None);
        assert_eq!(parse(&packet(0x04, 300, 120_340_010, 8_000, 4711)), None);
        assert_eq!(parse(&[0x2A, 0x38, 0x80]), None);
    }

    #[test]
    fn refresh_with_shifted_layout() {
        // 2B 38 where the effect code sits one byte after the target.
        let mut p = vec![0x2B, 0x38];
        enc_varint(300, &mut p);
        p.push(0x00);
        enc_varint(7, &mut p);
        p.extend(150_000_000u32.to_le_bytes());
        p.extend(5_000u32.to_le_bytes());
        p.extend([0; 12]);
        enc_varint(99, &mut p);
        assert_eq!(
            parse(&p).map(|e| (e.code, e.caster)),
            Some((150_000_000, 99))
        );
    }

    #[test]
    fn uptime_merges_refreshes_and_casters() {
        let t = BuffTracker::default();
        let ev = |caster, d| BuffEvent {
            target: 1,
            caster,
            code: 120_000_000,
            duration_ms: d,
        };
        t.record(ev(10, 10_000), 0); // 0..10 s
        t.record(ev(10, 10_000), 5_000); // refresh: 0..15 s
        t.record(ev(20, 10_000), 30_000); // another caster: 30..40 s
        t.record(
            BuffEvent {
                code: 130_000_000,
                ..ev(10, 5_000)
            },
            0,
        );
        let u = t.uptimes(1, 0, 60_000);
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].code, 120_000_000);
        assert!((u[0].percent - 25.0 * 100.0 / 60.0).abs() < 1e-9);
        assert_eq!(u[0].caster, 10);
        assert!(t.uptimes(2, 0, 60_000).is_empty());
        assert!(t.uptimes(1, 100_000, 200_000).is_empty());
    }

    #[test]
    fn names_try_the_skill_code() {
        assert_eq!(name_keys(120_340_015)[0], 12_034_001);
        assert_eq!(name_keys(120_340_015)[2], 12_030_000);
        assert_eq!(name_keys(25_000_123)[0], 25_000_123);
    }
}
