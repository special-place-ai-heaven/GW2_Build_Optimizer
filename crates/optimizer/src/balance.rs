//! BalanceContext: carries game-mode and patch identity through all mode-sensitive calculations.
//!
//! Constructed once at the top level (addon entry points) and threaded by reference
//! (`&BalanceContext`) through every function that reads a mode-split coefficient.
//!
//! `patch_id` is temporary — sourced from a snapshot constant until P3-08 adds
//! manifest-backed authoritative sourcing.

use gw2_core::types::GameMode;

/// Snapshot constant for patch_id until P3-08 adds manifest-backed sourcing.
const SNAPSHOT_PATCH_ID: &str = "snapshot-2026-03-06";

/// Context that travels through every mode-sensitive calculation.
///
/// - `game_mode`: determines which coefficient table to use (PvE vs PvP vs WvW).
/// - `patch_id`: identifies the balance snapshot; temporary sourcing until P3-08.
///
/// Passed by reference (`&BalanceContext`). Constructed once at addon entry points.
#[derive(Debug, Clone)]
pub struct BalanceContext {
    /// Which game mode we are optimizing for.
    pub game_mode: GameMode,
    /// Balance snapshot identifier (temporary — authoritative sourcing deferred to P3-08).
    pub patch_id: String,
}

impl BalanceContext {
    /// Create a new BalanceContext with a snapshot patch_id.
    pub fn new(game_mode: GameMode) -> Self {
        Self {
            game_mode,
            patch_id: SNAPSHOT_PATCH_ID.to_string(),
        }
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
