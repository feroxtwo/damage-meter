//! Damage curves are observations at the 500 ms heartbeat, never synthetic hits.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
pub const MAX_POINTS: usize = 14_400;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub ms: i64,
    pub damage: HashMap<i32, i64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Series {
    pub points: VecDeque<Point>,
    pub partial: bool,
}
impl Series {
    pub fn observe(&mut self, ms: i64, damage: HashMap<i32, i64>) {
        if self
            .points
            .back()
            .is_some_and(|p| p.ms == ms && p.damage == damage)
        {
            return;
        }
        if self.points.is_empty() && ms > 1000 {
            self.partial = true;
        }
        if let Some(last) = self.points.back() {
            if ms < last.ms {
                self.partial = true;
                return;
            }
            if last
                .damage
                .iter()
                .any(|(id, d)| damage.get(id).copied().unwrap_or(0) < *d)
            {
                self.partial = true;
            }
        }
        self.points.push_back(Point {
            ms: ms.max(0),
            damage,
        });
        if self.points.len() > MAX_POINTS {
            self.points.pop_front();
            self.partial = true;
        }
    }
    /// Fixed five-second denominator, also during the first five seconds.
    pub fn burst(&self, actor: i32, now: i64) -> f64 {
        let total = self
            .points
            .back()
            .and_then(|p| p.damage.get(&actor))
            .copied()
            .unwrap_or(0);
        let baseline = self
            .points
            .iter()
            .rev()
            .find(|p| p.ms <= now - 5000)
            .and_then(|p| p.damage.get(&actor))
            .copied();
        // A trimmed history is not evidence that all lifetime damage occurred
        // in the current five seconds. Use the first retained observation.
        let baseline = baseline.unwrap_or_else(|| {
            if self.partial && now > 5000 {
                self.points
                    .front()
                    .and_then(|p| p.damage.get(&actor))
                    .copied()
                    .unwrap_or(total)
            } else {
                0
            }
        });
        (total - baseline).max(0) as f64 / 5.0
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"resolution_ms":500,"partial":self.partial,"points":self.points,"method":"sampled_cumulative_damage"})
    }
}

/// Shared history budget across all targets, also on encounters with many adds.
pub fn bound_series(series: &mut HashMap<(i32, i64), Series>) {
    let total: usize = series.values().map(|s| s.points.len()).sum();
    let mut excess = total.saturating_sub(MAX_POINTS);
    if excess == 0 {
        return;
    }
    let mut keys: Vec<_> = series.keys().copied().collect();
    keys.sort_by_key(|k| k.1);
    for key in keys {
        let s = series.get_mut(&key).unwrap();
        // Keep an endpoint for old targets, so their partial status survives.
        let remove = excess.min(s.points.len().saturating_sub(1));
        if remove > 0 {
            s.points.drain(..remove);
            s.partial = true;
            excess -= remove;
        }
        if excess == 0 {
            break;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn burst_expires_and_does_not_use_average_damage_per_hit() {
        let mut s = Series::default();
        s.observe(0, HashMap::from([(1, 100)]));
        s.observe(1000, HashMap::from([(1, 200)]));
        s.observe(5000, HashMap::from([(1, 1000)]));
        assert_eq!(s.burst(1, 5000), 180.0);
        assert_eq!(s.burst(1, 10000), 0.0);
        s.observe(6000, HashMap::from([(1, 900)]));
        assert!(s.partial);
    }
    #[test]
    fn many_targets_share_a_bounded_history_without_inventing_burst() {
        let mut series = HashMap::new();
        for target in 0..32 {
            let s = series
                .entry((target, target as i64))
                .or_insert_with(Series::default);
            for i in 0..1000 {
                s.observe(i * 500, HashMap::from([(1, i * 100)]));
            }
        }
        bound_series(&mut series);
        assert_eq!(
            series.values().map(|s| s.points.len()).sum::<usize>(),
            MAX_POINTS
        );
        let trimmed = &series[&(0, 0)];
        assert!(trimmed.partial);
        assert_eq!(trimmed.points.len(), 1);
        assert_eq!(trimmed.burst(1, 500_000), 0.0);
        assert_eq!(trimmed.json()["points"][0]["damage"]["1"], 99900);
    }
}
