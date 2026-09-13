//! NeedsMechanic Engine E4: Mesmer IllusionState (clones).
//!
//! One state owns clone count (cap 3). `spawn_clone` / `consume` are the sole
//! writers; successful spawns emit TriggerBus::OnCloneCreated. Shared by
//! `simulator` and `wvw_timeline`. Not phantasms, shatter, blades, or Mirage.

use super::trigger_bus::{BusEvent, TriggerBus};

/// Maximum living clones (Mesmer base). Virtuoso blades are not clones.
pub const CLONE_CAP: u32 = 3;

/// Sole clone-count authority. Default count = 0. Later phantasm/shatter may
/// extend this struct — do not add those fields in E4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllusionState {
    pub count: u32,
    pub cap: u32,
}

impl Default for IllusionState {
    fn default() -> Self {
        Self {
            count: 0,
            cap: CLONE_CAP,
        }
    }
}

impl IllusionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment iff `count < cap`. Returns true when count rose.
    pub fn spawn(&mut self) -> bool {
        if self.count >= self.cap {
            return false;
        }
        self.count += 1;
        true
    }

    /// Remove `min(n, count)` clones; return how many removed (shatter later).
    pub fn consume(&mut self, n: u32) -> u32 {
        let removed = n.min(self.count);
        self.count -= removed;
        removed
    }
}

/// Spawn one clone on the shared IllusionState and emit OnCloneCreated iff
/// the count actually rose. Cap no-op does not emit (723 must not stack on
/// failed create). Returns true only when count rose.
pub fn spawn_clone(state: &mut IllusionState, bus: &mut TriggerBus, at_ms: u32) -> bool {
    if !state.spawn() {
        return false;
    }
    bus.emit(BusEvent::OnCloneCreated, at_ms);
    true
}

#[cfg(test)]
mod kent_tests {
    use super::*;
    use crate::rotation::trigger_bus::TriggerBus;

    #[test]
    fn kent_e4_spawn_0_to_1_emits() {
        let mut state = IllusionState::new();
        let mut bus = TriggerBus::new();
        assert_eq!(state.count, 0);
        assert!(spawn_clone(&mut state, &mut bus, 100));
        assert_eq!(state.count, 1);
        assert_eq!(bus.count(BusEvent::OnCloneCreated), 1);
    }

    #[test]
    fn kent_e4_fourth_spawn_at_cap_is_noop_no_emit() {
        let mut state = IllusionState::new();
        let mut bus = TriggerBus::new();
        for i in 0..3 {
            assert!(spawn_clone(&mut state, &mut bus, i * 10));
        }
        assert_eq!(state.count, 3);
        assert_eq!(bus.count(BusEvent::OnCloneCreated), 3);
        assert!(!spawn_clone(&mut state, &mut bus, 40));
        assert_eq!(state.count, 3);
        assert_eq!(bus.count(BusEvent::OnCloneCreated), 3);
    }

    #[test]
    fn kent_e4_consume_returns_removed() {
        let mut state = IllusionState::new();
        let mut bus = TriggerBus::new();
        spawn_clone(&mut state, &mut bus, 0);
        spawn_clone(&mut state, &mut bus, 1);
        assert_eq!(state.consume(5), 2);
        assert_eq!(state.count, 0);
        assert_eq!(state.consume(1), 0);
    }
}
