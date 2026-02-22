//! Build archetypes and scoring functions.
//! Archetypes define what the optimizer should maximize.

use serde::{Deserialize, Serialize};

use crate::combat::CombatPerformance;
use crate::stats::{DerivedStats, StatBlock};

/// Normalization divisors for cross-archetype score comparison.
/// These map typical high-end values to ~1.0 so scores across different
/// archetypes occupy a comparable range. Empirically tuned — do not
/// adjust without validating cross-archetype ordering.
const STRIKE_DPS_NORM: f64 = 50000.0;
const CONDI_DPS_NORM: f64 = 50000.0;
const TOTAL_DPS_NORM: f64 = 100000.0;
const EFFECTIVE_HEALTH_NORM: f64 = 200000.0;

/// 5-stage aggression level slider.
/// Determines the balance between damage output and survivability/control.
/// Stage 1 = pure survivability, Stage 5 = pure damage, Stage 3 = balanced.
///
/// Design principle: "Pure damage is NOT the goal. The ability to DELIVER
/// damage is the goal." The optimal position depends on game mode and build:
/// - PvE: lean aggressive (mobs are predictable, healer supports you)
/// - WvW/PvP: lean balanced or defensive (CC, stability, stunbreaks matter)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggressionLevel {
    /// Stage 1: Maximum survivability — tank, healer, bunker
    FullDefense = 1,
    /// Stage 2: Defensive lean — survivable damage dealer
    Defensive = 2,
    /// Stage 3: Balanced — damage + survive in equal measure
    Balanced = 3,
    /// Stage 4: Aggressive — damage-focused with minimal survival
    Aggressive = 4,
    /// Stage 5: Full glass cannon — maximum damage output
    FullOffense = 5,
}

impl AggressionLevel {
    pub const ALL: [AggressionLevel; 5] = [
        AggressionLevel::FullDefense,
        AggressionLevel::Defensive,
        AggressionLevel::Balanced,
        AggressionLevel::Aggressive,
        AggressionLevel::FullOffense,
    ];

    pub fn label(&self) -> &str {
        match self {
            AggressionLevel::FullDefense => "Full Defense",
            AggressionLevel::Defensive => "Defensive",
            AggressionLevel::Balanced => "Balanced",
            AggressionLevel::Aggressive => "Aggressive",
            AggressionLevel::FullOffense => "Full Offense",
        }
    }

    /// Default aggression level for a game mode.
    pub fn default_for_mode(game_mode: &str) -> Self {
        match game_mode {
            "PvE" => AggressionLevel::Aggressive,
            "WvW" => AggressionLevel::Balanced,
            "PvP" => AggressionLevel::Balanced,
            _ => AggressionLevel::Balanced,
        }
    }

    /// Damage weight (0.0–1.0). Higher = more weight on DPS metrics.
    pub fn damage_weight(&self) -> f64 {
        match self {
            AggressionLevel::FullDefense => 0.1,
            AggressionLevel::Defensive => 0.3,
            AggressionLevel::Balanced => 0.5,
            AggressionLevel::Aggressive => 0.75,
            AggressionLevel::FullOffense => 0.95,
        }
    }

    /// Survivability weight (0.0–1.0). Higher = more weight on EHP/control.
    pub fn survival_weight(&self) -> f64 {
        1.0 - self.damage_weight()
    }

    /// Convert from slider index (0-4) to AggressionLevel.
    pub fn from_index(idx: usize) -> Self {
        Self::ALL[idx.min(4)]
    }

    /// Convert to slider index (0-4).
    pub fn to_index(&self) -> usize {
        (*self as usize).saturating_sub(1)
    }
}

/// Build archetypes matching the UI's archetype selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Archetype {
    PowerDPS,
    ConditionDPS,
    SustainHybrid,
    Tank,
    BoonSupport,
    HealSupport,
    CelestialHybrid,
}

impl Archetype {
    pub const ALL: [Archetype; 7] = [
        Archetype::PowerDPS,
        Archetype::ConditionDPS,
        Archetype::SustainHybrid,
        Archetype::Tank,
        Archetype::BoonSupport,
        Archetype::HealSupport,
        Archetype::CelestialHybrid,
    ];

    pub fn label(&self) -> &str {
        match self {
            Archetype::PowerDPS => "Power DPS",
            Archetype::ConditionDPS => "Condition DPS",
            Archetype::SustainHybrid => "Sustain Hybrid",
            Archetype::Tank => "Tank",
            Archetype::BoonSupport => "Boon Support",
            Archetype::HealSupport => "Heal Support",
            Archetype::CelestialHybrid => "Celestial Hybrid",
        }
    }

    /// Stat weights for this archetype. Higher = more important.
    pub fn weights(&self) -> StatWeights {
        match self {
            Archetype::PowerDPS => StatWeights {
                power: 1.0,
                precision: 0.8,
                ferocity: 0.7,
                ..StatWeights::default()
            },
            Archetype::ConditionDPS => StatWeights {
                condition_damage: 1.0,
                expertise: 0.8,
                precision: 0.3,
                ..StatWeights::default()
            },
            Archetype::SustainHybrid => StatWeights {
                power: 0.6,
                precision: 0.5,
                ferocity: 0.4,
                vitality: 0.4,
                toughness: 0.3,
                ..StatWeights::default()
            },
            Archetype::Tank => StatWeights {
                toughness: 1.0,
                vitality: 1.0,
                healing_power: 0.3,
                ..StatWeights::default()
            },
            Archetype::BoonSupport => StatWeights {
                concentration: 1.0,
                healing_power: 0.6,
                expertise: 0.2,
                vitality: 0.3,
                toughness: 0.3,
                ..StatWeights::default()
            },
            Archetype::HealSupport => StatWeights {
                healing_power: 1.0,
                concentration: 0.5,
                expertise: 0.15,
                vitality: 0.3,
                toughness: 0.3,
                ..StatWeights::default()
            },
            Archetype::CelestialHybrid => StatWeights {
                power: 0.4,
                precision: 0.3,
                ferocity: 0.3,
                condition_damage: 0.4,
                expertise: 0.3,
                concentration: 0.3,
                healing_power: 0.3,
                toughness: 0.3,
                vitality: 0.3,
            },
        }
    }

    /// Relevant stat prefixes to consider for gear search (prunes search space).
    pub fn relevant_prefixes(&self) -> &[&str] {
        match self {
            Archetype::PowerDPS => &["Berserker's", "Assassin's", "Dragon's"],
            Archetype::ConditionDPS => &[
                "Viper's", "Sinister", "Grieving", "Trailblazer's", "Plaguedoctor's",
            ],
            Archetype::SustainHybrid => &[
                "Marauder", "Valkyrie", "Berserker's", "Dragon's", "Cavalier's",
            ],
            Archetype::Tank => &["Minstrel's", "Nomad's", "Trailblazer's", "Dire"],
            Archetype::BoonSupport => &[
                "Harrier's", "Minstrel's", "Diviner's", "Plaguedoctor's",
            ],
            Archetype::HealSupport => &[
                "Harrier's", "Minstrel's", "Cleric's", "Magi's",
            ],
            Archetype::CelestialHybrid => &[
                "Celestial", "Diviner's", "Trailblazer's", "Minstrel's",
            ],
        }
    }
}

/// Stat weights for scoring builds.
#[derive(Debug, Clone, Default)]
pub struct StatWeights {
    pub power: f64,
    pub precision: f64,
    pub toughness: f64,
    pub vitality: f64,
    pub condition_damage: f64,
    pub expertise: f64,
    pub concentration: f64,
    pub ferocity: f64,
    pub healing_power: f64,
}

/// Score a stat block against an archetype's weights.
/// Higher score = better fit for the archetype.
pub fn score_stats(stats: &StatBlock, derived: &DerivedStats, archetype: &Archetype) -> f64 {
    let w = archetype.weights();

    // Weighted sum of normalized stats (divide by typical max to normalize)
    let raw = stats.power / 3000.0 * w.power
        + stats.precision / 3000.0 * w.precision
        + stats.toughness / 3000.0 * w.toughness
        + stats.vitality / 2000.0 * w.vitality
        + stats.condition_damage / 2500.0 * w.condition_damage
        + stats.expertise / 1000.0 * w.expertise
        + stats.concentration / 1000.0 * w.concentration
        + stats.ferocity / 1500.0 * w.ferocity
        + stats.healing_power / 1800.0 * w.healing_power;

    // Bonus for effective power in DPS archetypes
    let ep_bonus = match archetype {
        Archetype::PowerDPS => derived.effective_power / 50000.0,
        _ => 0.0,
    };

    raw + ep_bonus
}

/// Score a build based on actual combat performance metrics.
/// Higher score = better fit for the archetype.
/// Uses real DPS/healing/survivability numbers instead of raw stat weights.
pub fn score_combat(perf: &CombatPerformance, archetype: &Archetype) -> f64 {
    match archetype {
        Archetype::PowerDPS => {
            // Primary: strike DPS. Minor: crit-scaling reward.
            perf.strike_dps_index / STRIKE_DPS_NORM
        }
        Archetype::ConditionDPS => {
            // Primary: condition DPS. Duration multiplier rewards Expertise investment.
            let dur_bonus = (perf.condi_duration_pct / 100.0).min(1.0) * 0.15;
            perf.condition_dps_index / CONDI_DPS_NORM + dur_bonus
        }
        Archetype::SustainHybrid => {
            // Balanced DPS + survivability, strike-leaning
            perf.strike_dps_index / 60000.0
                + perf.condition_dps_index / 120000.0
                + perf.effective_health / 400000.0
        }
        Archetype::Tank => {
            // Maximize effective health and damage reduction
            perf.effective_health / EFFECTIVE_HEALTH_NORM
                + perf.damage_reduction_pct / 100.0
                + perf.healing_power_index / 10000.0
        }
        Archetype::BoonSupport => {
            // Primary: boon duration. Secondary: healing. Tertiary: some DPS contribution.
            perf.boon_duration_pct / 100.0
                + perf.healing_power_index / 5000.0
                + perf.condi_duration_pct / 500.0
                + perf.total_dps_index / 300000.0
        }
        Archetype::HealSupport => {
            // Primary: healing output. Secondary: boon duration for heal-boon hybrids.
            perf.healing_power_index / 3000.0
                + perf.boon_duration_pct / 200.0
                + perf.condi_duration_pct / 500.0
                + perf.effective_health / 800000.0
        }
        Archetype::CelestialHybrid => {
            // Well-rounded: DPS + support + survivability, all weighted evenly
            perf.total_dps_index / TOTAL_DPS_NORM
                + perf.boon_duration_pct / 200.0
                + perf.condi_duration_pct / 300.0
                + perf.healing_power_index / 8000.0
                + perf.effective_health / 500000.0
        }
    }
}

/// Score a build using both archetype weights AND aggression level.
/// The archetype determines WHAT to optimize (power vs condi vs heal),
/// while aggression level determines the DAMAGE vs SURVIVABILITY balance.
///
/// Formula: damage_component * damage_weight + survival_component * survival_weight
pub fn score_combat_weighted(
    perf: &CombatPerformance,
    archetype: &Archetype,
    aggression: &AggressionLevel,
) -> f64 {
    let dw = aggression.damage_weight();
    let sw = aggression.survival_weight();

    // Damage component — what the archetype cares about offensively
    let damage_score = match archetype {
        Archetype::PowerDPS => perf.strike_dps_index / STRIKE_DPS_NORM,
        Archetype::ConditionDPS => {
            let dur_bonus = (perf.condi_duration_pct / 100.0).min(1.0) * 0.15;
            perf.condition_dps_index / CONDI_DPS_NORM + dur_bonus
        }
        Archetype::SustainHybrid => {
            perf.strike_dps_index / 60000.0 + perf.condition_dps_index / 120000.0
        }
        Archetype::Tank => perf.healing_power_index / 10000.0,
        Archetype::BoonSupport => {
            perf.boon_duration_pct / 100.0 + perf.healing_power_index / 5000.0
        }
        Archetype::HealSupport => {
            perf.healing_power_index / 3000.0 + perf.boon_duration_pct / 200.0
        }
        Archetype::CelestialHybrid => {
            perf.total_dps_index / TOTAL_DPS_NORM
                + perf.boon_duration_pct / 200.0
                + perf.healing_power_index / 8000.0
        }
    };

    // Survivability component — EHP and damage reduction
    let survival_score = perf.effective_health / EFFECTIVE_HEALTH_NORM
        + perf.damage_reduction_pct / 100.0;

    damage_score * dw + survival_score * sw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_dps_prefers_berserker_stats() {
        let berserker = StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            ..Default::default()
        };
        let celestial = StatBlock {
            power: 1600.0,
            precision: 1600.0,
            toughness: 1600.0,
            vitality: 1600.0,
            condition_damage: 1600.0,
            expertise: 600.0,
            concentration: 600.0,
            ferocity: 600.0,
            healing_power: 600.0,
        };

        let derived_b = DerivedStats { effective_power: 40000.0, ..Default::default() };
        let derived_c = DerivedStats { effective_power: 20000.0, ..Default::default() };

        let score_b = score_stats(&berserker, &derived_b, &Archetype::PowerDPS);
        let score_c = score_stats(&celestial, &derived_c, &Archetype::PowerDPS);

        assert!(score_b > score_c, "Berserker should score higher for Power DPS");
    }

    #[test]
    fn test_score_combat_power_dps() {
        use crate::combat::{self, DamageModifiers, default_buff_profiles};
        use crate::stats;

        // Berserker build: high power, precision, ferocity
        let berserker = StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived_b = stats::compute_derived(&berserker, "Warrior");
        let mods_b = DamageModifiers::default();
        let solo = &default_buff_profiles()[0];
        let perf_b = combat::calculate_combat_performance(
            &berserker, &derived_b, &mods_b, solo, "Warrior",
        );

        // Celestial build: all stats moderate
        let celestial = StatBlock {
            power: 1600.0,
            precision: 1600.0,
            toughness: 1600.0,
            vitality: 1600.0,
            condition_damage: 1600.0,
            expertise: 600.0,
            concentration: 600.0,
            ferocity: 600.0,
            healing_power: 600.0,
        };
        let derived_c = stats::compute_derived(&celestial, "Warrior");
        let mods_c = DamageModifiers::default();
        let perf_c = combat::calculate_combat_performance(
            &celestial, &derived_c, &mods_c, solo, "Warrior",
        );

        let score_b = score_combat(&perf_b, &Archetype::PowerDPS);
        let score_c = score_combat(&perf_c, &Archetype::PowerDPS);
        assert!(
            score_b > score_c,
            "Berserker ({}) should score higher than Celestial ({}) for PowerDPS",
            score_b, score_c,
        );
    }

    #[test]
    fn test_score_combat_condi_dps() {
        use crate::combat::{self, DamageModifiers, default_buff_profiles};
        use crate::stats;

        // Viper build: high condi + expertise
        let viper = StatBlock {
            power: 1800.0,
            precision: 1600.0,
            condition_damage: 2200.0,
            expertise: 600.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived_v = stats::compute_derived(&viper, "Necromancer");
        let mods_v = DamageModifiers::default();
        let solo = &default_buff_profiles()[0];
        let perf_v = combat::calculate_combat_performance(
            &viper, &derived_v, &mods_v, solo, "Necromancer",
        );

        // Berserker build: no condi damage
        let berserker = StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived_b = stats::compute_derived(&berserker, "Necromancer");
        let mods_b = DamageModifiers::default();
        let perf_b = combat::calculate_combat_performance(
            &berserker, &derived_b, &mods_b, solo, "Necromancer",
        );

        let score_v = score_combat(&perf_v, &Archetype::ConditionDPS);
        let score_b = score_combat(&perf_b, &Archetype::ConditionDPS);
        assert!(
            score_v > score_b,
            "Viper ({}) should score higher than Berserker ({}) for ConditionDPS",
            score_v, score_b,
        );
    }

    #[test]
    fn test_tank_prefers_toughness_vitality() {
        let tanky = StatBlock {
            toughness: 2800.0,
            vitality: 2200.0,
            ..Default::default()
        };
        let glass = StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            ..Default::default()
        };

        let d = DerivedStats::default();
        let score_t = score_stats(&tanky, &d, &Archetype::Tank);
        let score_g = score_stats(&glass, &d, &Archetype::Tank);

        assert!(score_t > score_g, "Tanky stats should score higher for Tank archetype");
    }

    #[test]
    fn test_aggression_level_weights() {
        // Full offense = nearly all damage weight
        assert!(AggressionLevel::FullOffense.damage_weight() > 0.9);
        assert!(AggressionLevel::FullOffense.survival_weight() < 0.1);

        // Full defense = nearly all survival weight
        assert!(AggressionLevel::FullDefense.damage_weight() < 0.15);
        assert!(AggressionLevel::FullDefense.survival_weight() > 0.85);

        // Balanced = roughly equal
        assert!((AggressionLevel::Balanced.damage_weight() - 0.5).abs() < 0.01);

        // Weights always sum to 1.0
        for level in &AggressionLevel::ALL {
            let total = level.damage_weight() + level.survival_weight();
            assert!((total - 1.0).abs() < 0.01, "Weights must sum to 1.0 for {:?}", level);
        }
    }

    #[test]
    fn test_aggression_level_index_roundtrip() {
        for (i, level) in AggressionLevel::ALL.iter().enumerate() {
            assert_eq!(level.to_index(), i);
            assert_eq!(AggressionLevel::from_index(i), *level);
        }
    }

    #[test]
    fn test_aggression_default_for_mode() {
        assert_eq!(AggressionLevel::default_for_mode("PvE"), AggressionLevel::Aggressive);
        assert_eq!(AggressionLevel::default_for_mode("WvW"), AggressionLevel::Balanced);
        assert_eq!(AggressionLevel::default_for_mode("PvP"), AggressionLevel::Balanced);
    }

    #[test]
    fn test_score_combat_weighted_glass_vs_tanky() {
        use crate::combat::{self, DamageModifiers, default_buff_profiles};
        use crate::stats;

        // Glass cannon: high damage, low survivability
        let glass = StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived_g = stats::compute_derived(&glass, "Warrior");
        let mods = DamageModifiers::default();
        let solo = &default_buff_profiles()[0];
        let perf_g = combat::calculate_combat_performance(
            &glass, &derived_g, &mods, solo, "Warrior",
        );

        // Tanky build: low damage, high survivability
        let tanky = StatBlock {
            power: 1200.0,
            precision: 1000.0,
            toughness: 2500.0,
            vitality: 2200.0,
            ..Default::default()
        };
        let derived_t = stats::compute_derived(&tanky, "Warrior");
        let perf_t = combat::calculate_combat_performance(
            &tanky, &derived_t, &mods, solo, "Warrior",
        );

        // At Full Offense, glass should win
        let score_g_offense = score_combat_weighted(&perf_g, &Archetype::PowerDPS, &AggressionLevel::FullOffense);
        let score_t_offense = score_combat_weighted(&perf_t, &Archetype::PowerDPS, &AggressionLevel::FullOffense);
        assert!(
            score_g_offense > score_t_offense,
            "Glass ({}) should beat tanky ({}) at FullOffense",
            score_g_offense, score_t_offense
        );

        // At Full Defense, tanky should win
        let score_g_defense = score_combat_weighted(&perf_g, &Archetype::PowerDPS, &AggressionLevel::FullDefense);
        let score_t_defense = score_combat_weighted(&perf_t, &Archetype::PowerDPS, &AggressionLevel::FullDefense);
        assert!(
            score_t_defense > score_g_defense,
            "Tanky ({}) should beat glass ({}) at FullDefense",
            score_t_defense, score_g_defense
        );
    }
}
