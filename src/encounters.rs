//! Conservative attempt boundaries. Never infer a wipe from estimated HP alone.
#[derive(Default)]
pub struct Monitor {
    key: Option<(i32, i64)>,
    low_hp: Option<f64>,
    all_dead_at: Option<i64>,
    death_invalidated: bool,
}
pub struct Observation {
    pub key: (i32, i64),
    pub now: i64,
    pub last_damage: i64,
    pub hp: Option<f64>,
    pub all_dead: bool,
    pub target_dead: bool,
}
impl Monitor {
    pub fn update(
        &mut self,
        o: Observation,
        idle_seconds: u64,
        wipe: bool,
    ) -> Option<&'static str> {
        if self.key != Some(o.key) {
            *self = Self {
                key: Some(o.key),
                ..Self::default()
            };
        }
        let idle = o.now.saturating_sub(o.last_damage).max(0);
        // The parser retains death markers until reset. Damage after the all-dead
        // observation means a revive/late packet: never reuse those stale deaths.
        if self.all_dead_at.is_some_and(|t| o.last_damage > t) {
            self.death_invalidated = true;
        }
        if o.all_dead && !o.target_dead {
            self.all_dead_at.get_or_insert(o.now);
        } else {
            self.all_dead_at = None;
        }
        if let Some(hp) = o.hp.filter(|hp| hp.is_finite() && (0.0..=1.0).contains(hp)) {
            let reset = self
                .low_hp
                .is_some_and(|low| low <= 0.8 && hp >= 0.98 && hp - low >= 0.2);
            if wipe
                && !self.death_invalidated
                && !o.target_dead
                && idle >= 5_000
                && reset
                && self.all_dead_at.is_some_and(|t| o.now - t >= 2_000)
            {
                return Some("wipe");
            }
            self.low_hp = Some(self.low_hp.unwrap_or(hp).min(hp));
        }
        if idle_seconds > 0 && idle >= (idle_seconds * 1000) as i64 {
            return Some("idle");
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample(now: i64, hp: Option<f64>, dead: bool) -> Observation {
        Observation {
            key: (9, 1000),
            now,
            last_damage: 1000,
            hp,
            all_dead: dead,
            target_dead: false,
        }
    }
    #[test]
    fn wipe_requires_deaths_real_hp_reset_and_quiet_period() {
        let mut m = Monitor::default();
        assert_eq!(m.update(sample(2000, Some(0.6), false), 0, true), None);
        assert_eq!(m.update(sample(4000, Some(0.6), true), 0, true), None);
        assert_eq!(m.update(sample(7000, None, true), 0, true), None);
        assert_eq!(m.update(sample(8000, Some(1.0), false), 0, true), None);
        assert_eq!(m.update(sample(9000, Some(1.0), true), 0, true), None);
        assert_eq!(
            m.update(sample(11500, Some(1.0), true), 0, true),
            Some("wipe")
        );
    }
    #[test]
    fn target_death_and_target_switch_are_not_wipes() {
        let mut m = Monitor::default();
        m.update(sample(2000, Some(0.5), true), 0, true);
        let mut killed = sample(8000, Some(1.0), true);
        killed.target_dead = true;
        assert_eq!(m.update(killed, 0, true), None);
        let mut switched = sample(11000, Some(1.0), true);
        switched.key = (10, 9000);
        assert_eq!(m.update(switched, 0, true), None);
    }
    #[test]
    fn damage_after_death_invalidates_stale_death_markers() {
        let mut m = Monitor::default();
        m.update(sample(2000, Some(0.5), true), 0, true);
        let mut revived = sample(9000, Some(0.5), true);
        revived.last_damage = 8000;
        assert_eq!(m.update(revived, 0, true), None);
        let mut reset = sample(15000, Some(1.0), true);
        reset.last_damage = 8000;
        assert_eq!(m.update(reset, 0, true), None);
    }
    #[test]
    fn idle_is_optional_and_uses_last_damage_not_start() {
        let mut m = Monitor::default();
        assert_eq!(m.update(sample(16000, None, false), 0, false), None);
        assert_eq!(m.update(sample(15999, None, false), 15, false), None);
        assert_eq!(
            m.update(sample(16000, None, false), 15, false),
            Some("idle")
        );
        let mut active = sample(17000, None, false);
        active.last_damage = 16900;
        assert_eq!(m.update(active, 15, false), None);
    }
}
