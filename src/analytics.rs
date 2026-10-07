//! Damage curves are observations at the 500 ms heartbeat, never synthetic hits.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub ms: i64,
    pub damage: HashMap<i32, i64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Series {
    pub points: Vec<Point>,
    pub partial: bool,
}
impl Series {
    pub fn observe(&mut self, ms: i64, damage: HashMap<i32, i64>) {
        if self
            .points
            .last()
            .is_some_and(|p| p.ms == ms && p.damage == damage)
        {
            return;
        }
        if self.points.is_empty() && ms > 1000 {
            self.partial = true;
        }
        if let Some(last) = self.points.last() {
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
        self.points.push(Point {
            ms: ms.max(0),
            damage,
        });
        if self.points.len() > 14_400 {
            self.points.remove(0);
            self.partial = true;
        }
    }
    /// Fixed five-second denominator, also during the first five seconds.
    pub fn burst(&self, actor: i32, now: i64) -> f64 {
        let total = self
            .points
            .last()
            .and_then(|p| p.damage.get(&actor))
            .copied()
            .unwrap_or(0);
        let baseline = self
            .points
            .iter()
            .rev()
            .find(|p| p.ms <= now - 5000)
            .and_then(|p| p.damage.get(&actor))
            .copied()
            .unwrap_or(0);
        (total - baseline).max(0) as f64 / 5.0
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"resolution_ms":500,"partial":self.partial,"points":self.points,"method":"sampled_cumulative_damage"})
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
}
