//! Core domain types for resolved builds.
//! A "resolved" build has all IDs expanded to full data from the cache.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A fully resolved character build with all data expanded.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedBuild {
    pub character_name: String,
    pub profession: String,
    pub game_mode: GameMode,
    pub specializations: Vec<ResolvedSpec>,
    pub skills: ResolvedSkills,
    pub weapons: Vec<ResolvedWeaponSet>,
    pub armor: Vec<ResolvedGearPiece>,
    pub trinkets: Vec<ResolvedGearPiece>,
    pub relic: Option<ResolvedRelic>,
    pub rune: Option<ResolvedUpgrade>,
    pub pvp_amulet: Option<ResolvedPvpAmulet>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMode {
    #[default]
    PvE,
    PvP,
    WvW,
}

impl GameMode {
    /// Canonical iteration order for `GameMode`. The slice `[PvE, PvP, WvW]`
    /// is part of the contract — downstream consumers iterate this constant
    /// to build tab order and default-lookup sequences. Reordering silently
    /// shifts UI/lookup behavior across the workspace. The
    /// `game_mode_all_order_is_pinned` test below guards the order.
    pub const ALL: [GameMode; 3] = [GameMode::PvE, GameMode::PvP, GameMode::WvW];

    pub fn label(&self) -> &str {
        match self {
            GameMode::PvE => "PvE",
            GameMode::PvP => "PvP",
            GameMode::WvW => "WvW",
        }
    }
}

/// Granular lock constraints for the optimizer.
/// Controls which specializations and trait choices are preserved vs. free to change.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildLocks {
    /// Spec locks by slot (0, 1, 2). None = optimizer decides, Some(id) = must use this spec.
    ///
    /// Slot 2 is semantically the elite-eligible slot: `locked_elite_id()` and the
    /// "Locked to: <elite>" badge in `render_improve_tab` both read `specs[2]` directly.
    /// Reordering slots is therefore not supported by the UI — it would silently break
    /// that invariant for every consumer that assumes `specs[2]` is the elite slot.
    pub specs: [Option<u32>; 3],
    /// Trait locks: spec_id → [Adept, Master, Grandmaster]. None = free, Some(id) = locked trait.
    #[serde(default)]
    pub trait_locks: HashMap<u32, [Option<u32>; 3]>,
}

impl BuildLocks {
    /// Get the locked elite spec ID (slot 2), for backward-compatible optimizer paths.
    pub fn locked_elite_id(&self) -> Option<u32> {
        self.specs[2]
    }

    /// Check if any locks are set at all.
    pub fn has_any_locks(&self) -> bool {
        self.specs.iter().any(|s| s.is_some())
            || self
                .trait_locks
                .values()
                .any(|cols| cols.iter().any(|c| c.is_some()))
    }

    /// Get locked trait for a specific spec and column (0=Adept, 1=Master, 2=Grandmaster).
    pub fn locked_trait(&self, spec_id: u32, column: usize) -> Option<u32> {
        self.trait_locks
            .get(&spec_id)
            .and_then(|cols| cols.get(column).copied().flatten())
    }

    /// Build a text description of all active locks for prompt generation.
    pub fn describe_constraints(&self) -> String {
        let mut parts = Vec::new();
        for (slot, spec) in self.specs.iter().enumerate() {
            if let Some(id) = spec {
                parts.push(format!("Slot {} spec locked to ID {}", slot + 1, id));
            }
        }
        // Sort by spec_id so the output is deterministic regardless of
        // HashMap iteration order. The string is embedded verbatim in LLM
        // prompts; nondeterministic ordering breaks prompt cache hits and
        // makes responses drift between identical user requests.
        let mut trait_entries: Vec<(&u32, &[Option<u32>; 3])> = self.trait_locks.iter().collect();
        trait_entries.sort_by_key(|(spec_id, _)| **spec_id);
        for (spec_id, cols) in trait_entries {
            for (col, trait_id) in cols.iter().enumerate() {
                if let Some(tid) = trait_id {
                    let tier = match col {
                        0 => "Adept",
                        1 => "Master",
                        2 => "Grandmaster",
                        _ => "Unknown",
                    };
                    parts.push(format!(
                        "Spec {} {} trait locked to ID {}",
                        spec_id, tier, tid
                    ));
                }
            }
        }
        if parts.is_empty() {
            String::new()
        } else {
            parts.join("; ")
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedSpec {
    pub id: u32,
    pub name: String,
    pub elite: bool,
    pub traits_selected: Vec<ResolvedTrait>,
    pub traits_available: Vec<Vec<TraitOption>>, // 3 columns, 3 options each
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedTrait {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub column: usize, // 0, 1, 2
    pub selected: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraitOption {
    pub id: u32,
    pub name: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedSkills {
    pub heal: Option<SkillInfo>,
    pub utilities: Vec<Option<SkillInfo>>,
    pub elite: Option<SkillInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillInfo {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedWeaponSet {
    pub label: String, // "Set 1", "Set 2"
    pub main_hand: Option<WeaponInfo>,
    pub off_hand: Option<WeaponInfo>,
    pub sigils: Vec<UpgradeInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeaponInfo {
    pub name: String,
    pub weapon_type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpgradeInfo {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedGearPiece {
    pub slot: String,
    pub name: String,
    pub stat_prefix: String,
    pub infusions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedUpgrade {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedRelic {
    pub id: u32,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedPvpAmulet {
    pub id: u32,
    pub name: String,
    pub stats: std::collections::HashMap<String, i32>,
}

/// Calculated stat block for a build.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatBlock {
    pub power: i32,
    pub precision: i32,
    pub toughness: i32,
    pub vitality: i32,
    pub condition_damage: i32,
    pub expertise: i32,
    pub concentration: i32,
    pub ferocity: i32,
    pub healing_power: i32,
    // Derived
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub health: i32,
    pub armor: i32,
}

impl StatBlock {
    /// Compute derived stats from base stats.
    /// `base_health` and `base_defense` are profession-specific values from loaded
    /// profession profiles (data/profession_profiles.json). Callers get these values
    /// from the optimizer crate's stats::base_health() / stats::base_defense().
    ///
    /// NOTE: GW2 HP classes and armor classes do NOT align!
    /// HP: High (Warrior, Necromancer), Medium (Rev, Engi, Ranger, Mesmer), Low (Guardian, Thief, Ele)
    /// Armor: Heavy (Warrior, Guardian, Revenant), Medium (Ranger, Engi, Thief), Light (Ele, Mes, Necro)
    ///
    /// NOTE: The canonical source for formula constants (895, 21, 150, 15, 10) is
    /// `data/formulas/universal.json`. The active runtime paths in
    /// `crates/optimizer/src/{stats,combat}.rs` use loaded values from that file.
    /// This method retains hardcoded values because `core` cannot depend on `optimizer`.
    /// If callers are added, inject constants as parameters or move this to `optimizer`.
    pub fn compute_derived(&mut self, base_health: i32, base_defense: i32) {
        self.crit_chance = ((self.precision - 895) as f64 / 21.0).clamp(0.0, 100.0);
        self.crit_damage = 150.0 + self.ferocity as f64 / 15.0;
        self.health = self.vitality * 10 + base_health;
        self.armor = self.toughness + base_defense;
    }
}

/// Combat performance metrics for UI display.
/// Calculated from the optimizer's CombatPerformance struct,
/// rounded to display-friendly values.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CombatMetrics {
    pub effective_power: i32,
    pub strike_dps_index: i32,
    pub condition_dps_index: i32,
    pub total_dps_index: i32,
    pub healing_index: i32,
    pub crit_chance: f64,
    pub boon_duration_pct: f64,
    pub condi_duration_pct: f64,
    pub effective_health: i32,
    pub damage_reduction_pct: f64,
    // Condition breakdown
    pub bleeding_tick: i32,
    pub burning_tick: i32,
    pub poison_tick: i32,
    pub torment_tick: i32,
    pub confusion_tick: i32,
}

/// Rotation simulation breakdown for UI display.
/// Shows simulated DPS, condition uptimes, buff uptimes, and skill usage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RotationBreakdown {
    pub simulated_dps: i32,
    pub strike_dps: i32,
    pub condition_dps: i32,
    /// Average condition stacks (condition_name → avg_stacks).
    pub condition_uptime: Vec<(String, f64)>,
    /// Buff uptime percentages (buff_name → pct 0-100).
    pub buff_uptime: Vec<(String, f64)>,
    /// Skill usage in the rotation (name, cast_count, dps_contribution).
    pub skill_usage: Vec<(String, u32, i32)>,
    /// Number of stunbreaks in the skill bar.
    pub stunbreak_count: u32,
    /// Whether the build has stability access.
    pub has_stability: bool,
    /// Stability uptime percentage (0.0-1.0).
    pub stability_uptime: f64,
    /// Number of equipped skills that have at least one cleanse effect.
    pub cleanse_count: u32,
    /// Estimated conditions removed per 20 seconds.
    pub cleanse_rate_per_20s: f64,
}

/// A saved optimizer build for persistence (Save/Load tab).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedBuild {
    pub name: String,
    pub timestamp: u64, // Unix epoch seconds
    pub character_name: String,
    pub game_mode: GameMode,
    /// Profession name (e.g. "Necromancer"). Empty for pre-P3-16 saves;
    /// treated as "Warrior" fallback at load time (backward-compat shim).
    #[serde(default)]
    pub profession: String,
    /// Engine version that created this save (informational, e.g. "1.0.0").
    #[serde(default)]
    pub engine_version: String,
    /// Balance manifest version used when this save was created (P3-08, future).
    #[serde(default)]
    pub balance_manifest_version: Option<String>,
    // Build suggestion data (mirrors BuildSuggestion fields)
    pub label: String,
    pub stat_prefix: String,
    pub specializations: Vec<(String, Vec<String>)>,
    pub weapons: Vec<String>,
    pub skills: Vec<String>,
    pub rune: String,
    pub sigils: Vec<String>,
    pub relic: String,
    pub explanation: String,
    #[serde(default)]
    pub synergy_explanation: String,
    pub changes_made: Vec<String>,
    pub estimated_stats: Option<StatBlock>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These tests snapshot the exact string format produced by
    /// `BuildLocks::describe_constraints`. The output is consumed verbatim by the
    /// LLM prompts in `crates/optimizer/src/prompts.rs` (e.g.
    /// `new_build_prompt_with_tools`, `synergy_build_prompt`). If you change the
    /// format intentionally, update both the prompt callers and these snapshots
    /// in the same change.
    #[test]
    fn describe_constraints_no_locks() {
        let locks = BuildLocks::default();
        assert_eq!(locks.describe_constraints(), "");
    }

    #[test]
    fn describe_constraints_only_spec_locked() {
        let mut locks = BuildLocks::default();
        locks.specs[0] = Some(5);
        locks.specs[2] = Some(34);
        assert_eq!(
            locks.describe_constraints(),
            "Slot 1 spec locked to ID 5; Slot 3 spec locked to ID 34",
        );
    }

    #[test]
    fn describe_constraints_only_traits_locked() {
        // Use a single spec_id so HashMap iteration order can't make the
        // snapshot flaky.
        let mut locks = BuildLocks::default();
        locks
            .trait_locks
            .insert(34, [Some(1111), Some(2222), Some(3333)]);
        assert_eq!(
            locks.describe_constraints(),
            "Spec 34 Adept trait locked to ID 1111; Spec 34 Master trait locked to ID 2222; Spec 34 Grandmaster trait locked to ID 3333",
        );
    }

    #[test]
    fn describe_constraints_both_spec_and_traits_locked() {
        // Single spec_id again to avoid HashMap iteration nondeterminism.
        let mut locks = BuildLocks::default();
        locks.specs[2] = Some(34);
        locks
            .trait_locks
            .insert(34, [Some(1111), None, Some(3333)]);
        assert_eq!(
            locks.describe_constraints(),
            "Slot 3 spec locked to ID 34; Spec 34 Adept trait locked to ID 1111; Spec 34 Grandmaster trait locked to ID 3333",
        );
    }

    /// `trait_locks` is a `HashMap<u32, _>`, so its iteration order is
    /// nondeterministic. `describe_constraints` must sort by `spec_id` so the
    /// emitted string is stable across runs (these snapshots assert that).
    #[test]
    fn describe_constraints_two_specs_with_traits_sorted_by_spec_id() {
        // Insert in the reverse of the expected order to expose any reliance
        // on insertion order.
        let mut locks = BuildLocks::default();
        locks.trait_locks.insert(48, [Some(2001), None, None]);
        locks.trait_locks.insert(34, [Some(1111), Some(2222), None]);
        assert_eq!(
            locks.describe_constraints(),
            "Spec 34 Adept trait locked to ID 1111; Spec 34 Master trait locked to ID 2222; Spec 48 Adept trait locked to ID 2001",
        );
    }

    #[test]
    fn describe_constraints_three_specs_with_traits_sorted_by_spec_id() {
        // Insert out of order; expected output is sorted ascending by spec_id.
        let mut locks = BuildLocks::default();
        locks.trait_locks.insert(72, [None, None, Some(7000)]);
        locks.trait_locks.insert(5, [Some(500), None, Some(502)]);
        locks.trait_locks.insert(34, [None, Some(3400), None]);
        assert_eq!(
            locks.describe_constraints(),
            "Spec 5 Adept trait locked to ID 500; Spec 5 Grandmaster trait locked to ID 502; Spec 34 Master trait locked to ID 3400; Spec 72 Grandmaster trait locked to ID 7000",
        );
    }

    #[test]
    fn describe_constraints_mixed_spec_and_multi_spec_traits() {
        // Spec locks come first (in slot order), followed by trait locks
        // grouped by spec_id ascending. One of the trait-locked specs has no
        // matching spec lock, exercising the independence of the two maps.
        let mut locks = BuildLocks::default();
        locks.specs[0] = Some(5);
        locks.specs[2] = Some(34);
        locks.trait_locks.insert(34, [Some(3401), None, None]);
        locks.trait_locks.insert(5, [None, Some(502), Some(503)]);
        assert_eq!(
            locks.describe_constraints(),
            "Slot 1 spec locked to ID 5; Slot 3 spec locked to ID 34; Spec 5 Master trait locked to ID 502; Spec 5 Grandmaster trait locked to ID 503; Spec 34 Adept trait locked to ID 3401",
        );
    }


    #[test]
    fn game_mode_all_order_is_pinned() {
        // `GameMode::ALL` is iterated by tab rendering and default-lookup code
        // across the workspace. The order [PvE, PvP, WvW] is load-bearing — if
        // it silently flips, UI tabs and default profiles reorder with it.
        assert_eq!(
            GameMode::ALL,
            [GameMode::PvE, GameMode::PvP, GameMode::WvW],
        );
    }

    #[test]
    fn game_mode_default_is_pve() {
        // Pinned alongside ALL's order because several consumers rely on the
        // default game mode matching the first element of ALL.
        assert_eq!(GameMode::default(), GameMode::PvE);
        assert_eq!(GameMode::default(), GameMode::ALL[0]);
    }
}
