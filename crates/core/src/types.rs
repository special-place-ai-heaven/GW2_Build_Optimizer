//! Core domain types for resolved builds.
//! A "resolved" build has all IDs expanded to full data from the cache.

use serde::{Deserialize, Serialize};

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
    /// `profession` determines base health: Warrior/Necro=9212, medium=5922, light=1645.
    pub fn compute_derived(&mut self, profession: &str) {
        self.crit_chance = ((self.precision - 895) as f64 / 21.0).clamp(0.0, 100.0);
        self.crit_damage = 150.0 + self.ferocity as f64 / 15.0;
        let base_hp = match profession {
            "Warrior" | "Necromancer" => 9212,
            "Revenant" | "Engineer" | "Ranger" | "Mesmer" => 5922,
            "Guardian" | "Thief" | "Elementalist" => 1645,
            _ => 5922, // default to medium
        };
        self.health = self.vitality * 10 + base_hp;
        self.armor = self.toughness + 1000; // approximate, defense from gear adds too
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

/// A saved optimizer build for persistence (Save/Load tab).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedBuild {
    pub name: String,
    pub timestamp: u64, // Unix epoch seconds
    pub character_name: String,
    pub game_mode: GameMode,
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
    pub changes_made: Vec<String>,
    pub estimated_stats: Option<StatBlock>,
}
