//! BalanceContext: carries game-mode and patch identity through all mode-sensitive calculations.
//!
//! Constructed once at the top level (addon entry points) and threaded by reference
//! (`&BalanceContext`) through every function that reads a mode-split coefficient.
//!
//! `patch_id` comes from the active embedded manifest (`latest_manifest`).
//! Use [`BalanceContext::for_patch`] for an explicit historical snapshot.

use gw2_core::types::GameMode;

/// Active balance snapshot from the embedded manifest set.
pub fn active_patch_id() -> &'static str {
    crate::data::manifests::latest_manifest().patch_id.as_str()
}

/// Context that travels through every mode-sensitive calculation.
///
/// - `game_mode`: determines which coefficient table to use (PvE vs PvP vs WvW).
/// - `patch_id`: the balance snapshot (`latest_manifest`, or an explicit historical id).
///
/// Passed by reference (`&BalanceContext`). Constructed once at addon entry points.
#[derive(Debug, Clone)]
pub struct BalanceContext {
    /// Which game mode we are optimizing for.
    pub game_mode: GameMode,
    /// Balance snapshot identifier (active manifest, or an explicit historical id).
    pub patch_id: String,
}

impl BalanceContext {
    /// Create a context for the active manifest snapshot.
    pub fn new(game_mode: GameMode) -> Self {
        Self::for_patch(game_mode, active_patch_id())
    }

    /// Explicit snapshot (historical lookup or tests). Does not rewrite `patch_id`.
    pub fn for_patch(game_mode: GameMode, patch_id: impl Into<String>) -> Self {
        Self {
            game_mode,
            patch_id: patch_id.into(),
        }
    }

    /// Whether this `patch_id` exists in the embedded manifest set.
    pub fn patch_is_known(&self) -> bool {
        crate::data::manifests::manifests()
            .iter()
            .any(|m| m.patch_id == self.patch_id)
    }

    /// Convenience: PvE context for tests and default paths.
    pub fn pve() -> Self {
        Self::new(GameMode::PvE)
    }

    /// Convenience: PvP context.
    pub fn pvp() -> Self {
        Self::new(GameMode::PvP)
    }

    /// Convenience: WvW context.
    pub fn wvw() -> Self {
        Self::new(GameMode::WvW)
    }

    /// Mode label string (e.g. "PvE", "PvP", "WvW").
    pub fn mode_label(&self) -> &str {
        self.game_mode.label()
    }
}

/// `Some` when `live_build_id` is not the active manifest's `game_build_id`.
pub fn live_build_mismatch(live_build_id: u64) -> Option<String> {
    crate::data::manifests::check_staleness(live_build_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_uses_the_active_manifest_not_a_hand_edited_literal() {
        let ctx = BalanceContext::pve();
        let active = crate::data::manifests::latest_manifest();
        assert_eq!(ctx.patch_id, active.patch_id);
        assert_eq!(active_patch_id(), active.patch_id.as_str());
        assert!(ctx.patch_is_known());
    }

    #[test]
    fn for_patch_keeps_historical_ids_and_flags_unknown() {
        let old = BalanceContext::for_patch(GameMode::WvW, "2026-01-13");
        assert_eq!(old.patch_id, "2026-01-13");
        assert!(
            old.patch_is_known(),
            "historical manifest must stay reachable"
        );
        let unknown = BalanceContext::for_patch(GameMode::PvE, "9999-99-99");
        assert!(!unknown.patch_is_known());
    }

    #[test]
    fn live_build_mismatch_is_observable() {
        let active = crate::data::manifests::latest_manifest();
        assert!(live_build_mismatch(active.game_build_id).is_none());
        let warn = live_build_mismatch(active.game_build_id.wrapping_add(1))
            .expect("a different live build must be visible");
        assert!(warn.contains(&active.game_build_id.to_string()));
    }
}
