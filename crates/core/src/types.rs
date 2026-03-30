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
        for (spec_id, cols) in &self.trait_locks {
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
