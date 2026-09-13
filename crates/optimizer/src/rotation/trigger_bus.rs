//! NeedsMechanic Engine E0: shared TriggerBus + Endurance/Dodge family.
//!
//! Bus events are OnDodge, OnDisableFoe, OnElite, OnThreshold only.
//! EndurancePool and DodgeAction are one family (not a second dodge path).
//! Trait-skill registry rides the bus as consumers — no one-off casts.

use std::collections::VecDeque;

/// E0 bus events. Shared by `simulator` and `wvw_timeline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BusEvent {
    OnDodge,
    OnDisableFoe,
    OnElite,
    /// Health (or similar) threshold crossed — maps to TriggerRule::OnThreshold.
    OnThreshold,
}

/// One recorded emission for causal proofs / traces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusEmission {
    pub event: BusEvent,
    pub at_ms: u32,
}

/// Shared trigger bus: emit sites push; consumers drain or peek.
#[derive(Debug, Default, Clone)]
pub struct TriggerBus {
    queue: VecDeque<BusEmission>,
    /// Cumulative emit counts (never cleared by drain).
    pub totals: [u32; 4],
}

impl TriggerBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&mut self, event: BusEvent, at_ms: u32) {
        let idx = match event {
            BusEvent::OnDodge => 0,
            BusEvent::OnDisableFoe => 1,
            BusEvent::OnElite => 2,
            BusEvent::OnThreshold => 3,
        };
        self.totals[idx] = self.totals[idx].saturating_add(1);
        self.queue.push_back(BusEmission { event, at_ms });
    }

    pub fn drain(&mut self) -> Vec<BusEmission> {
        self.queue.drain(..).collect()
    }

    pub fn pending(&self) -> impl Iterator<Item = &BusEmission> {
        self.queue.iter()
    }

    pub fn count(&self, event: BusEvent) -> u32 {
        match event {
            BusEvent::OnDodge => self.totals[0],
            BusEvent::OnDisableFoe => self.totals[1],
            BusEvent::OnElite => self.totals[2],
            BusEvent::OnThreshold => self.totals[3],
        }
    }
}

/// Player endurance pool. Cap 100; one dodge costs 50; base regen 5/s.
#[derive(Debug, Clone)]
pub struct EndurancePool {
    pub current: f64,
    pub max: f64,
    pub regen_per_sec: f64,
    /// Endurance spent this fight (causal accounting).
    pub spent: f64,
}

impl Default for EndurancePool {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
            regen_per_sec: 5.0,
            spent: 0.0,
        }
    }
}

impl EndurancePool {
    pub fn new_full() -> Self {
        Self::default()
    }

    pub fn tick(&mut self, dt_ms: u32) {
        if self.current >= self.max {
            self.current = self.max;
            return;
        }
        let gain = self.regen_per_sec * (dt_ms as f64) / 1_000.0;
        self.current = (self.current + gain).min(self.max);
    }

    pub fn can_dodge(&self, cost: f64) -> bool {
        self.current + 1e-9 >= cost
    }

    /// Spend `cost` if available. Returns false when short.
    pub fn try_spend(&mut self, cost: f64) -> bool {
        if !self.can_dodge(cost) {
            return false;
        }
        self.current -= cost;
        self.spent += cost;
        true
    }
}

/// Dodge cost in endurance points (half the pool).
pub const DODGE_COST: f64 = 50.0;

/// Same-family dodge action: spends EndurancePool and emits OnDodge on the bus.
#[derive(Debug, Clone, Default)]
pub struct DodgeAction {
    pub dodges: u32,
}

impl DodgeAction {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt a dodge. On success: spends endurance, emits OnDodge, returns true.
    pub fn try_dodge(
        &mut self,
        pool: &mut EndurancePool,
        bus: &mut TriggerBus,
        at_ms: u32,
    ) -> bool {
        if !pool.try_spend(DODGE_COST) {
            return false;
        }
        self.dodges = self.dodges.saturating_add(1);
        bus.emit(BusEvent::OnDodge, at_ms);
        true
    }
}

/// Map a bus event to the matching TriggerRule spelling used by records.
pub fn bus_to_trigger_rule(event: BusEvent) -> crate::data::normalized_effects::TriggerRule {
    use crate::data::normalized_effects::TriggerRule;
    match event {
        BusEvent::OnDodge => TriggerRule::OnDodge,
        BusEvent::OnDisableFoe => TriggerRule::OnDisableFoe,
        BusEvent::OnElite => TriggerRule::OnElite,
        BusEvent::OnThreshold => TriggerRule::OnThreshold,
    }
}

#[cfg(test)]
mod kent_tests {
    use super::*;

    /// Kent causal dodge micro-proof: endurance spend → DodgeAction → bus OnDodge.
    #[test]
    fn kent_causal_dodge_endurance_to_bus_on_dodge() {
        let mut pool = EndurancePool::new_full();
        let mut bus = TriggerBus::new();
        let mut dodge = DodgeAction::new();

        assert!(pool.can_dodge(DODGE_COST));
        assert!(dodge.try_dodge(&mut pool, &mut bus, 1_000));
        assert_eq!(dodge.dodges, 1);
        assert!((pool.current - 50.0).abs() < 1e-9);
        assert!((pool.spent - 50.0).abs() < 1e-9);
        assert_eq!(bus.count(BusEvent::OnDodge), 1);

        let drained = bus.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].event, BusEvent::OnDodge);
        assert_eq!(drained[0].at_ms, 1_000);

        // Second dodge spends the remaining 50 -> pool empty.
        assert!(dodge.try_dodge(&mut pool, &mut bus, 1_050));
        assert_eq!(dodge.dodges, 2);
        assert!(pool.current.abs() < 1e-9);
        assert_eq!(bus.count(BusEvent::OnDodge), 2);

        // Third dodge refuses until regen restores a full cost.
        assert!(!dodge.try_dodge(&mut pool, &mut bus, 1_100));
        assert_eq!(dodge.dodges, 2);
        assert_eq!(bus.count(BusEvent::OnDodge), 2);

        // 10 s regen restores 50 endurance -> third dodge fires.
        pool.tick(10_000);
        assert!(dodge.try_dodge(&mut pool, &mut bus, 11_100));
        assert_eq!(dodge.dodges, 3);
        assert_eq!(bus.count(BusEvent::OnDodge), 3);
    }

    #[test]
    fn bus_events_are_e0_four_only() {
        let mut bus = TriggerBus::new();
        bus.emit(BusEvent::OnDodge, 0);
        bus.emit(BusEvent::OnDisableFoe, 1);
        bus.emit(BusEvent::OnElite, 2);
        bus.emit(BusEvent::OnThreshold, 3);
        assert_eq!(bus.count(BusEvent::OnDodge), 1);
        assert_eq!(bus.count(BusEvent::OnDisableFoe), 1);
        assert_eq!(bus.count(BusEvent::OnElite), 1);
        assert_eq!(bus.count(BusEvent::OnThreshold), 1);
    }
}
