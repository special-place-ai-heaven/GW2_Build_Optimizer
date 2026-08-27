//! 6-axis optimization weights and scoring functions.
//! Replaces the old 5-axis system with a 6-axis radar-chart-driven scoring model
//! driven by objective profile data files.
//!
//! Axes: power, condition, boon_support, healing, sustain, control.

use serde::{Deserialize, Serialize};

use crate::combat::CombatPerformance;
use crate::data::objective_profiles;

/// Default normalization divisors for cross-axis score comparison.
/// These are used as fallbacks when no objective profile is loaded.
/// They are calibrated ceiling values (not typical build averages), chosen so
/// well-optimized builds approach 1.0 without hitting the cap from gear alone.
pub const STRIKE_DPS_NORM: f64 = 3000.0;
pub const CONDI_DPS_NORM: f64 = 3500.0;
pub const EFFECTIVE_HEALTH_NORM: f64 = 50000.0;
pub const HEALING_NORM: f64 = 1500.0;
/// Default normalization for boon support scoring axis.
pub const BOON_SUPPORT_NORM: f64 = 1.0;
/// Default normalization for control scoring axis.
pub const CONTROL_NORM: f64 = 1.0;

/// 6-axis radar chart labels, in render order (clockwise from top).
pub const AXIS_LABELS: [&str; 6] = [
    "Power",
    "Condition",
    "Boon Spt",
    "Heal",
    "Sustain",
    "Control",
];

/// Machine keys for the same 6 axes (`OptimizationWeights` field names).
pub const AXIS_KEYS: [&str; 6] = [
    "power",
    "condition",
    "boon_support",
    "healing",
    "sustain",
    "control",
];

/// Default total weight budget. Now loaded from objective profile data at runtime.
/// This constant is kept for backward compatibility with code that doesn't have
/// a profile available yet.
pub const DEFAULT_WEIGHT_BUDGET: f64 = 2.0;

/// Legacy alias for backward compatibility. Code that referenced `WEIGHT_BUDGET`
/// directly can still compile but should migrate to using the profile's weight_budget.
pub const WEIGHT_BUDGET: f64 = DEFAULT_WEIGHT_BUDGET;

/// A named UI preset: display label paired with a constructor for the weights.
type WeightPreset = (&'static str, fn() -> OptimizationWeights);

/// 6-axis optimization weights. Each axis 0.0-1.0, total constrained to weight budget.
/// Drives gear search, trait selection, and build scoring.
///
/// Axes (clockwise from top, matching radar chart layout):
/// 1. Power -- strike/direct damage (strike_dps_index)
/// 2. Condition -- condition/DoT damage (condition_dps_index)
/// 3. Boon Support -- boon generation, uptime, and sharing
/// 4. Heal -- healing output (healing_power_index)
/// 5. Sustain -- survivability (effective_health + damage_reduction)
/// 6. Control -- CC, suppression, boon denial, mobility denial
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationWeights {
    pub power: f64,
    pub condition: f64,
    #[serde(default)]
    pub boon_support: f64,
    pub healing: f64,
    pub sustain: f64,
    /// Control axis (replaces old "disable" axis).
    /// Backward-compatible: deserializes from "disable" in old saved data.
    #[serde(alias = "disable")]
    pub control: f64,
}

impl Default for OptimizationWeights {
    fn default() -> Self {
        Self::preset_balanced()
    }
}

impl OptimizationWeights {
    /// Number of axes.
    pub const NUM_AXES: usize = 6;

    /// Clamp all axes to [0.0, 1.0].
    pub fn clamped(&self) -> Self {
        Self {
            power: self.power.clamp(0.0, 1.0),
            condition: self.condition.clamp(0.0, 1.0),
            boon_support: self.boon_support.clamp(0.0, 1.0),
            healing: self.healing.clamp(0.0, 1.0),
            sustain: self.sustain.clamp(0.0, 1.0),
            control: self.control.clamp(0.0, 1.0),
        }
    }

    /// Sum of all weights.
    pub fn total(&self) -> f64 {
        self.power + self.condition + self.boon_support + self.healing + self.sustain + self.control
    }

    /// Get weight by axis index (0=Power, 1=Condition, 2=BoonSupport, 3=Heal, 4=Sustain, 5=Control).
    pub fn get(&self, index: usize) -> f64 {
        match index {
            0 => self.power,
            1 => self.condition,
            2 => self.boon_support,
            3 => self.healing,
            4 => self.sustain,
            5 => self.control,
            _ => 0.0,
        }
    }

    /// Set weight by axis index (unconstrained -- does not enforce budget).
    pub fn set(&mut self, index: usize, value: f64) {
        let v = value.clamp(0.0, 1.0);
        match index {
            0 => self.power = v,
            1 => self.condition = v,
            2 => self.boon_support = v,
            3 => self.healing = v,
            4 => self.sustain = v,
            5 => self.control = v,
            _ => {}
        }
    }

    /// Set weight by axis index, enforcing a weight budget constraint.
    /// When the new value would push the total over budget, other axes are
    /// proportionally scaled down -- modeling real GW2 gear stat trade-offs
    /// where investing in one stat means less budget for others.
    pub fn set_constrained(&mut self, index: usize, new_val: f64) {
        self.set_constrained_with_budget(index, new_val, DEFAULT_WEIGHT_BUDGET);
    }

    /// Set weight by axis index, enforcing a specific weight budget.
    pub fn set_constrained_with_budget(&mut self, index: usize, new_val: f64, budget: f64) {
        let new_val = new_val.clamp(0.0, 1.0);
        self.set(index, new_val);

        let total = self.total();
        if total > budget {
            let excess = total - budget;
            // Sum of all other axes
            let other_total: f64 = (0..Self::NUM_AXES)
                .filter(|&i| i != index)
                .map(|i| self.get(i))
                .sum();

            if other_total > 0.001 {
                // Scale down other axes proportionally
                let scale = ((other_total - excess) / other_total).max(0.0);
                for i in 0..Self::NUM_AXES {
                    if i != index {
                        let v = self.get(i);
                        self.set(i, v * scale);
                    }
                }
            } else {
                // All others are ~0, cap the dragged axis at budget
                self.set(index, budget.min(1.0));
            }
        }
    }

    /// Remaining budget available for distribution (using default budget).
    pub fn budget_remaining(&self) -> f64 {
        (DEFAULT_WEIGHT_BUDGET - self.total()).max(0.0)
    }

    /// Remaining budget with a specific budget value.
    pub fn budget_remaining_with(&self, budget: f64) -> f64 {
        (budget - self.total()).max(0.0)
    }

    /// Return weights as an array in axis order.
    pub fn as_array(&self) -> [f64; 6] {
        [
            self.power,
            self.condition,
            self.boon_support,
            self.healing,
            self.sustain,
            self.control,
        ]
    }

    /// Human-readable summary of the dominant priorities.
    pub fn summary_label(&self) -> String {
        let mut axes: Vec<(&str, f64)> = AXIS_LABELS
            .iter()
            .zip(self.as_array().iter())
            .map(|(label, &val)| (*label, val))
            .filter(|(_, v)| *v > 0.3)
            .collect();
        axes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if axes.is_empty() {
            return "Balanced".to_string();
        }
        axes.iter()
            .take(3)
            .map(|(label, _)| *label)
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// Default weights for a game mode. Resolved from objective profile data.
    pub fn default_for_mode(mode: &str) -> Self {
        let profiles = objective_profiles::objective_profiles();
        if let Some(profile) = profiles.default_for_mode(mode) {
            let aw = &profile.axis_weights;
            Self {
                power: aw.power,
                condition: aw.condition,
                boon_support: aw.boon_support,
                healing: aw.healing,
                sustain: aw.sustain,
                control: aw.control,
            }
        } else {
            // Fallback to hardcoded defaults if profiles not loaded
            Self::preset_balanced()
        }
    }

    /// Convert to per-stat weights for trait scoring.
    pub fn to_stat_weights(&self) -> StatWeights {
        StatWeights {
            power: self.power * 0.8,
            precision: self.power * 0.6 + self.condition * 0.2,
            ferocity: self.power * 0.5,
            condition_damage: self.condition * 0.8 + self.control * 0.2,
            expertise: self.condition * 0.6 + self.control * 0.4,
            concentration: self.boon_support * 0.7 + self.control * 0.2 + self.healing * 0.3,
            healing_power: self.healing * 0.9,
            toughness: self.sustain * 0.8,
            vitality: self.sustain * 0.7,
        }
    }

    // --- Presets ---
    // Now loaded from objective profile data. These methods provide backward
    // compatibility and quick access without needing a profile reference.

    pub fn preset_power_dps() -> Self {
        Self {
            power: 1.0,
            condition: 0.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.1,
            control: 0.0,
        }
        // total = 1.1
    }

    pub fn preset_condi_dps() -> Self {
        Self {
            power: 0.2,
            condition: 1.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.1,
            control: 0.2,
        }
        // total = 1.5
    }

    pub fn preset_tank() -> Self {
        Self {
            power: 0.1,
            condition: 0.0,
            boon_support: 0.0,
            healing: 0.3,
            sustain: 1.0,
            control: 0.2,
        }
        // total = 1.6
    }

    pub fn preset_healer() -> Self {
        // Boon support is weighted high (0.6) because meta GW2 PvE healers are
        // defined by quickness/alacrity uptime, not raw sustain -- this is what
        // makes Harrier's (Pow/Heal/Concentration) the canonical healer prefix
        // over pure-sustain Magi's (Heal/Tou/Vit). Sustain stays nonzero so the
        // tier selector still includes Minstrel's for heal-tank fallback builds.
        Self {
            power: 0.0,
            condition: 0.0,
            boon_support: 0.6,
            healing: 1.0,
            sustain: 0.2,
            control: 0.0,
        }
        // total = 1.8
    }

    pub fn preset_balanced() -> Self {
        Self {
            power: 0.4,
            condition: 0.3,
            boon_support: 0.2,
            healing: 0.2,
            sustain: 0.4,
            control: 0.3,
        }
        // total = 1.8
    }

    pub fn preset_disrupt() -> Self {
        Self {
            power: 0.2,
            condition: 0.3,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.2,
            control: 1.0,
        }
        // total = 1.7
    }

    pub fn preset_celestial() -> Self {
        Self {
            power: 0.33,
            condition: 0.33,
            boon_support: 0.33,
            healing: 0.33,
            sustain: 0.34,
            control: 0.34,
        }
        // total = 2.0
    }

    /// Named presets for UI buttons. These serve as quick-access shortcuts;
    /// the full named profile system is available through objective profile data.
    pub const PRESETS: [WeightPreset; 6] = [
        ("Power DPS", Self::preset_power_dps),
        ("Condi DPS", Self::preset_condi_dps),
        ("Tank", Self::preset_tank),
        ("Healer", Self::preset_healer),
        ("Balanced", Self::preset_balanced),
        ("Celestial", Self::preset_celestial),
    ];
}

/// Stat weights for trait scoring. Maps per-stat importance.
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

// ─── ObjectiveScorer ───

/// Wraps OptimizationWeights + objective profile data to provide a single
/// entry point for scoring. Contains normalization constants, boon/condition
/// priorities, interaction priorities, and weight budget from the profile.
#[derive(Debug, Clone)]
pub struct ObjectiveScorer {
    /// User-editable weight vector.
    pub weights: OptimizationWeights,
    /// Weight budget from the objective profile.
    pub weight_budget: f64,
    /// Per-axis normalization constants from the profile.
    pub strike_dps_norm: f64,
    pub condi_dps_norm: f64,
    pub boon_support_norm: f64,
    pub healing_power_norm: f64,
    pub effective_health_norm: f64,
    pub control_norm: f64,
    /// Boon type -> priority (0.0-1.0).
    pub boon_priorities: std::collections::HashMap<String, f64>,
    /// Condition type -> priority (0.0-1.0).
    pub condition_priorities: std::collections::HashMap<String, f64>,
    /// Interaction operation type -> priority (0.0-1.0).
    pub interaction_priorities: std::collections::HashMap<String, f64>,
}

impl ObjectiveScorer {
    /// Create a scorer from explicit weights using the default objective profile
    /// for the given mode.
    pub fn from_mode(weights: OptimizationWeights, mode: &str) -> Self {
        let profiles = objective_profiles::objective_profiles();
        if let Some(profile) = profiles.default_for_mode(mode) {
            Self::from_profile(weights, profile)
        } else {
            Self::fallback(weights)
        }
    }

    /// Create a scorer from an explicit objective profile.
    pub fn from_profile(
        weights: OptimizationWeights,
        profile: &objective_profiles::ObjectiveProfile,
    ) -> Self {
        let nc = &profile.normalization_constants;
        Self {
            weights,
            weight_budget: profile.weight_budget,
            strike_dps_norm: nc.strike_dps_norm,
            condi_dps_norm: nc.condi_dps_norm,
            boon_support_norm: nc.boon_support_norm,
            healing_power_norm: nc.healing_power_norm,
            effective_health_norm: nc.effective_health_norm,
            control_norm: nc.control_norm,
            boon_priorities: profile.boon_priorities.clone(),
            condition_priorities: profile.condition_priorities.clone(),
            interaction_priorities: profile.interaction_priorities.clone(),
        }
    }

    /// Create a fallback scorer with default normalization constants and empty priorities.
    pub fn fallback(weights: OptimizationWeights) -> Self {
        Self {
            weights,
            weight_budget: DEFAULT_WEIGHT_BUDGET,
            strike_dps_norm: STRIKE_DPS_NORM,
            condi_dps_norm: CONDI_DPS_NORM,
            boon_support_norm: BOON_SUPPORT_NORM,
            healing_power_norm: HEALING_NORM,
            effective_health_norm: EFFECTIVE_HEALTH_NORM,
            control_norm: CONTROL_NORM,
            boon_priorities: std::collections::HashMap::new(),
            condition_priorities: std::collections::HashMap::new(),
            interaction_priorities: std::collections::HashMap::new(),
        }
    }

    /// Score a build using this scorer's weights and profile-loaded normalization constants.
    /// Distinct from `score_with_weights` which uses module-level defaults.
    pub fn score(&self, perf: &CombatPerformance) -> f64 {
        score_with_norms(
            perf,
            &self.weights,
            self.strike_dps_norm,
            self.condi_dps_norm,
            self.effective_health_norm,
            self.healing_power_norm,
        )
    }

    /// Get boon priority, defaulting to 0.5 if not specified.
    pub fn boon_priority(&self, boon_name: &str) -> f64 {
        self.boon_priorities.get(boon_name).copied().unwrap_or(0.5)
    }

    /// Get condition priority, defaulting to 0.5 if not specified.
    pub fn condition_priority(&self, condition_name: &str) -> f64 {
        self.condition_priorities
            .get(condition_name)
            .copied()
            .unwrap_or(0.5)
    }

    /// Get interaction operation priority, defaulting to 0.5 if not specified.
    pub fn interaction_priority(&self, operation: &str) -> f64 {
        self.interaction_priorities
            .get(operation)
            .copied()
            .unwrap_or(0.5)
    }
}

/// Score a build against user-specified 6-axis weights.
/// Each CombatPerformance metric is normalized to [0, 1] using realistic
/// Solo-profile maximums, then multiplied by the corresponding weight.
/// All per-axis scores are hard-capped at 1.0 so no single axis can
/// dominate regardless of how extreme the raw metric values are.
///
/// Includes a misalignment penalty: if the user weighted any axis >= 0.4
/// but the build scores below 0.15 on that axis, the total score is
/// reduced proportionally. This catches edge cases where a set sneaks
/// through tier selection via a secondary axis but is fundamentally wrong
/// for the user's primary intent.
/// Uncapped weighted direction score: same axes as `score_with_weights`
/// but WITHOUT the per-axis saturation caps. Radar weights are a direction
/// indicator, not a goal — once a capped axis is satisfied, surplus piece
/// swaps that keep it at cap and raise other wished stats must be visible.
/// Used as the final tie-break level in `referee::search_rank`.
pub fn raw_direction_score(perf: &CombatPerformance, weights: &OptimizationWeights) -> f64 {
    let w = weights.clamped();
    let total_w = w.total().max(0.01);
    let power_score = perf.strike_dps_index / STRIKE_DPS_NORM;
    let condition_score =
        perf.condition_dps_index / CONDI_DPS_NORM + perf.condi_duration_pct / 100.0 * 0.15;
    let sustain_score =
        perf.effective_health / EFFECTIVE_HEALTH_NORM + perf.damage_reduction_pct / 100.0;
    let healing_score = perf.healing_power_index / HEALING_NORM;
    let boon_support_score = perf.boon_duration_pct / 100.0;
    let control_score =
        perf.condi_duration_pct / 100.0 * 0.6 + perf.boon_duration_pct / 100.0 * 0.4;

    (w.power * power_score
        + w.condition * condition_score
        + w.boon_support * boon_support_score
        + w.healing * healing_score
        + w.sustain * sustain_score
        + w.control * control_score)
        / total_w
}

pub fn score_with_weights(perf: &CombatPerformance, weights: &OptimizationWeights) -> f64 {
    score_with_norms(
        perf,
        weights,
        STRIKE_DPS_NORM,
        CONDI_DPS_NORM,
        EFFECTIVE_HEALTH_NORM,
        HEALING_NORM,
    )
}

/// Inner scoring function with explicit normalization divisors.
/// Called by both `score_with_weights` (module-level defaults) and
/// `ObjectiveScorer::score()` (profile-loaded norms).
fn score_with_norms(
    perf: &CombatPerformance,
    weights: &OptimizationWeights,
    strike_dps_norm: f64,
    condi_dps_norm: f64,
    effective_health_norm: f64,
    healing_norm: f64,
) -> f64 {
    let w = weights.clamped();
    let total_w = w.total().max(0.01);

    let power_score = (perf.strike_dps_index / strike_dps_norm).min(1.0);
    let condition_score = (perf.condition_dps_index / condi_dps_norm
        + (perf.condi_duration_pct / 100.0).min(1.0) * 0.15)
        .min(1.0);
    let sustain_score = (perf.effective_health / effective_health_norm
        + perf.damage_reduction_pct / 100.0)
        .min(1.0);
    let healing_score = (perf.healing_power_index / healing_norm).min(1.0);
    // Boon Support: proxy via boon duration (boon uptime contribution)
    let boon_support_score = (perf.boon_duration_pct / 100.0).min(1.0);
    // Control: proxy via condi duration (CC condition duration) + some boon duration (stability etc.)
    let control_score =
        (perf.condi_duration_pct / 100.0 * 0.6 + perf.boon_duration_pct / 100.0 * 0.4).min(1.0);

    let raw = (w.power * power_score
        + w.condition * condition_score
        + w.boon_support * boon_support_score
        + w.healing * healing_score
        + w.sustain * sustain_score
        + w.control * control_score)
        / total_w;

    // Graduated penalty: if any axis the user weighted >= 0.4 scores below 0.15,
    // penalize proportionally to that axis's weight. Multiplicative so multiple
    // neglected axes compound.
    let axis_weights = w.as_array();
    let axis_scores = [
        power_score,
        condition_score,
        boon_support_score,
        healing_score,
        sustain_score,
        control_score,
    ];
    let mut penalty = 1.0;
    for i in 0..6 {
        if axis_weights[i] >= 0.4 && axis_scores[i] < 0.15 {
            penalty *= 1.0 - axis_weights[i] * 0.7;
        }
    }

    raw * penalty
}

// ─── Hierarchical Tier Tables ───
//
// Each axis has 5 tiers of stat prefixes. The tier tables use 5 axes for gear
// selection (power, condition, sustain, heal, control/boon_support combined as
// "duration" since gear prefixes map Concentration/Expertise to both).
//
// The boon_support axis doesn't have its own tier table because GW2 gear doesn't
// have a "boon support" stat -- Concentration (boon duration) is the closest,
// and it's already in the control/duration tiers. The boon_support axis is
// used by the scorer through boon_priorities, not gear selection.

/// Power axis tiers: strike DPS (Power / Precision / Ferocity)
const POWER_TIERS: [&[&str]; 5] = [
    &["Cleric's"], // Tier 1: HP/Pow/Tou -- minor Power
    &["Soldier's", "Knight's", "Cavalier's", "Harrier's"], // Tier 2: some Power, not primary
    &["Valkyrie", "Diviner's", "Viper's", "Sinister"], // Tier 3: moderate Power presence
    &["Marauder", "Dragon's", "Grieving"], // Tier 4: Power-major with 1 extra
    &["Berserker's", "Assassin's"], // Tier 5: pure Pow/Prec/Fer
];

/// Condition axis tiers: DoT damage (ConditionDamage / Expertise)
const CONDITION_TIERS: [&[&str]; 5] = [
    &[],                                        // Tier 1: very few sets carry minor CD
    &["Apothecary's"],                          // Tier 2: HP/CD/Tou -- CD as secondary
    &["Dire", "Ritualist's", "Plaguedoctor's"], // Tier 3: moderate CD presence
    &["Trailblazer's", "Grieving"],             // Tier 4: CD-major + secondary
    &["Viper's", "Sinister"],                   // Tier 5: pure CD offensive
];

/// Sustain axis tiers: survivability (Toughness / Vitality)
const SUSTAIN_TIERS: [&[&str]; 5] = [
    &["Magi's", "Apothecary's"], // Tier 1: Vit as minor stat
    &["Ritualist's", "Plaguedoctor's", "Cleric's"], // Tier 2: some survivability
    &["Knight's", "Cavalier's", "Valkyrie", "Marauder", "Dragon's"], // Tier 3: Vit or Tou secondary
    &["Trailblazer's", "Dire", "Soldier's"], // Tier 4: Tou+Vit as major component
    &["Nomad's", "Minstrel's"],  // Tier 5: max Toughness + Vitality
];

/// Heal axis tiers: healing output (HealingPower)
const HEAL_TIERS: [&[&str]; 5] = [
    &[],                                            // Tier 1: very few sets carry minor HP
    &[],                                            // Tier 2: (Celestial handles this)
    &["Plaguedoctor's", "Apothecary's", "Nomad's"], // Tier 3: moderate HP
    &["Harrier's", "Cleric's"],                     // Tier 4: HP prominent
    &["Magi's", "Minstrel's"],                      // Tier 5: HP as primary stat
];

/// Control/Boon Support axis tiers: CC/boon/condi duration (Concentration / Expertise)
/// Combined tier table for both control and boon_support axes -- both care about
/// duration stats (Concentration, Expertise) in gear.
const CONTROL_TIERS: [&[&str]; 5] = [
    &[],                                             // Tier 1: very few sets carry minor duration
    &[],                                             // Tier 2: (Celestial handles this)
    &["Viper's", "Trailblazer's", "Plaguedoctor's"], // Tier 3: Exp or Conc as secondary
    &["Harrier's"],                                  // Tier 4: Concentration + Power + HP
    &["Diviner's", "Ritualist's"],                   // Tier 5: high Conc or Exp
];

/// Convert a weight value (0.0-1.0) to a tier level (1-5).
fn weight_to_tier(w: f64) -> usize {
    if w >= 0.8 {
        5
    } else if w >= 0.6 {
        4
    } else if w >= 0.4 {
        3
    } else if w >= 0.2 {
        2
    } else {
        1
    }
}

/// Select stat prefixes using hierarchical tier tables.
///
/// Each radar chart axis maps to a tier table. The weight level determines
/// which tier to start from. All tiers from the selected level UP to tier 5
/// are included.
///
/// Note: boon_support and control both map to the CONTROL_TIERS table since
/// GW2 gear duration stats serve both boon and CC purposes.
///
/// Celestial (all 9 stats) is always included as the universal hybrid.
pub fn select_prefixes_by_tiers(weights: &OptimizationWeights) -> Vec<&'static str> {
    let w = weights.clamped();
    let mut prefixes = Vec::new();

    // Use the max of boon_support and control for the duration tier lookup
    let duration_weight = w.boon_support.max(w.control);

    // Detect if there's a dominant axis (>= 0.8)
    let max_weight = [w.power, w.condition, w.sustain, w.healing, duration_weight]
        .iter()
        .fold(0.0_f64, |a, &b| a.max(b));
    let has_dominant = max_weight >= 0.8;

    let axes: [(f64, &[&[&str]; 5]); 5] = [
        (w.power, &POWER_TIERS),
        (w.condition, &CONDITION_TIERS),
        (w.sustain, &SUSTAIN_TIERS),
        (w.healing, &HEAL_TIERS),
        (duration_weight, &CONTROL_TIERS),
    ];

    for (weight, tiers) in &axes {
        if *weight < 0.15 {
            continue;
        }

        let mut tier = weight_to_tier(*weight);

        if has_dominant && *weight < 0.6 {
            tier = tier.max(4);
        }

        for t in (tier - 1)..5 {
            prefixes.extend_from_slice(tiers[t]);
        }
    }

    // Universal hybrid -- always included
    prefixes.push("Celestial");

    prefixes.sort();
    prefixes.dedup();
    prefixes
}

// ─── Deterministic Gear Prefix Selection ───
//
// Maps radar chart weights to the closest GW2 gear prefix using cosine similarity.
// Each prefix has a 6-axis "purpose profile" representing what kind of build uses it.
// The profile with highest cosine similarity to the user's weights wins.

/// Result of deterministic gear prefix selection.
#[derive(Debug, Clone)]
pub struct GearPrefixMatch {
    /// Best matching gear prefix name.
    pub primary: &'static str,
    /// Runner-up prefix (for potential mixing or context).
    pub secondary: Option<&'static str>,
    /// Cosine similarity score (0.0-1.0) -- how well the primary matches.
    pub similarity: f64,
}

/// Purpose profiles for all GW2 gear prefixes.
/// Format: (name, [power, condition, boon_support, heal, sustain, control])
/// Values represent "this prefix is intended for builds with these priorities."
const GEAR_PROFILES: &[(&str, [f64; 6])] = &[
    // -- Pure Power DPS --
    ("Berserker's", [1.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("Assassin's", [0.95, 0.0, 0.0, 0.0, 0.0, 0.0]),
    // -- Power + Survivability --
    ("Marauder", [0.75, 0.0, 0.0, 0.0, 0.25, 0.0]),
    ("Dragon's", [0.75, 0.0, 0.0, 0.0, 0.25, 0.0]),
    ("Valkyrie", [0.65, 0.0, 0.0, 0.0, 0.35, 0.0]),
    // -- Power + Condition hybrid --
    ("Grieving", [0.5, 0.5, 0.0, 0.0, 0.0, 0.0]),
    // -- Condition DPS --
    ("Viper's", [0.2, 0.65, 0.0, 0.0, 0.0, 0.15]),
    ("Sinister", [0.15, 0.85, 0.0, 0.0, 0.0, 0.0]),
    // -- Condition + Sustain (tanky condi) --
    ("Trailblazer's", [0.0, 0.45, 0.0, 0.0, 0.45, 0.1]),
    ("Dire", [0.0, 0.4, 0.0, 0.0, 0.6, 0.0]),
    // -- Condition + Boon/Condi Duration --
    ("Ritualist's", [0.0, 0.45, 0.2, 0.0, 0.1, 0.25]),
    // -- Condition + Healing --
    ("Plaguedoctor's", [0.0, 0.35, 0.0, 0.35, 0.15, 0.15]),
    // -- Pure Healer --
    ("Magi's", [0.0, 0.0, 0.0, 0.85, 0.15, 0.0]),
    // -- Healing + Boon Support --
    ("Harrier's", [0.1, 0.0, 0.4, 0.5, 0.0, 0.0]),
    ("Cleric's", [0.2, 0.0, 0.0, 0.55, 0.25, 0.0]),
    // -- Heal Tank --
    ("Minstrel's", [0.0, 0.0, 0.25, 0.35, 0.4, 0.0]),
    // -- Pure Tank --
    ("Nomad's", [0.0, 0.0, 0.0, 0.1, 0.9, 0.0]),
    // -- Power Tank --
    ("Soldier's", [0.35, 0.0, 0.0, 0.0, 0.65, 0.0]),
    ("Knight's", [0.35, 0.0, 0.0, 0.0, 0.65, 0.0]),
    ("Cavalier's", [0.3, 0.0, 0.0, 0.0, 0.7, 0.0]),
    // -- Power + Boon Duration --
    ("Diviner's", [0.45, 0.0, 0.35, 0.0, 0.0, 0.2]),
    // -- Condition/Heal/Tank hybrid --
    ("Apothecary's", [0.0, 0.25, 0.0, 0.4, 0.35, 0.0]),
    // -- Universal Hybrid --
    ("Celestial", [0.25, 0.25, 0.1, 0.1, 0.1, 0.2]),
];

/// Deterministically select the best gear prefix for the given radar chart weights.
///
/// Uses cosine similarity between the user's 6-axis weight vector and each
/// gear prefix's purpose profile. Returns the best match and a runner-up.
///
/// This is the AUTHORITATIVE gear selection -- Gemini does NOT get to override it.
pub fn select_gear_prefix(weights: &OptimizationWeights) -> GearPrefixMatch {
    let w = weights.clamped();
    let user = w.as_array();

    let user_mag = magnitude_6(&user);
    if user_mag < 0.001 {
        return GearPrefixMatch {
            primary: "Celestial",
            secondary: None,
            similarity: 0.0,
        };
    }

    let mut best = ("Celestial", 0.0_f64);
    let mut second = ("Celestial", -1.0_f64);

    for &(name, ref profile) in GEAR_PROFILES {
        let prof_mag = magnitude_6(profile);
        if prof_mag < 0.001 {
            continue;
        }
        let dot: f64 = user.iter().zip(profile.iter()).map(|(a, b)| a * b).sum();
        let cos = dot / (user_mag * prof_mag);

        if cos > best.1 {
            second = best;
            best = (name, cos);
        } else if cos > second.1 {
            second = (name, cos);
        }
    }

    GearPrefixMatch {
        primary: best.0,
        secondary: if second.1 > 0.0 { Some(second.0) } else { None },
        similarity: best.1,
    }
}

/// Longest gear-prefix name mentioned in free text (`celestial` → `Celestial`).
/// Skips a name the player negated (`not minstrel`, `without harrier`).
pub fn prefix_named_in_text(text: &str) -> Option<&'static str> {
    let hay = format!(
        " {} ",
        text.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    ' '
                }
            })
            .collect::<String>()
    );
    let mut best: Option<&'static str> = None;
    for &(name, _) in GEAR_PROFILES {
        let stem = name.trim_end_matches("'s").to_ascii_lowercase();
        let pats = [format!(" {stem} "), format!(" {stem}s ")];
        if !pats.iter().any(|p| hay.contains(p.as_str())) {
            continue;
        }
        if stem_negated(&hay, &stem) {
            continue;
        }
        if best.map(|b| b.len()).unwrap_or(0) < name.len() {
            best = Some(name);
        }
    }
    best
}

fn stem_negated(hay: &str, stem: &str) -> bool {
    let needles = [format!(" {stem} "), format!(" {stem}s ")];
    for n in &needles {
        let mut rest = hay;
        while let Some(idx) = rest.find(n.as_str()) {
            let before = &rest[..idx];
            if before.ends_with(" not")
                || before.ends_with(" no")
                || before.ends_with(" without")
                || before.ends_with(" instead of")
            {
                return true;
            }
            rest = &rest[idx + 1..];
        }
    }
    false
}

/// Euclidean magnitude of a 6-element vector.
fn magnitude_6(v: &[f64; 6]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weights_clamped() {
        let w = OptimizationWeights {
            power: 1.5,
            condition: 0.5,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 1.0,
            control: -0.5,
        };
        let c = w.clamped();
        assert_eq!(c.power, 1.0);
        assert_eq!(c.control, 0.0);
        assert_eq!(c.condition, 0.5);
    }

    #[test]
    fn prefix_named_in_text_finds_celestial() {
        assert_eq!(
            prefix_named_in_text("celestial gear tempest support"),
            Some("Celestial")
        );
        assert_eq!(prefix_named_in_text("Optimize a power DPS build"), None);
        assert_eq!(
            prefix_named_in_text("I said CELESTIAL support, not minstrel"),
            Some("Celestial")
        );
        assert_eq!(
            prefix_named_in_text("not celestial, use minstrel"),
            Some("Minstrel's")
        );
    }

    #[test]
    fn test_weights_total() {
        let w = OptimizationWeights::preset_power_dps();
        // Power DPS: power=1.0, sustain=0.1 -> total = 1.1
        assert!((w.total() - 1.1).abs() < 0.001);
    }

    #[test]
    fn test_weights_6_axes() {
        let w = OptimizationWeights::preset_balanced();
        assert_eq!(w.as_array().len(), 6);
        // Verify all 6 axes are included in total
        let arr = w.as_array();
        let sum: f64 = arr.iter().sum();
        assert!((sum - w.total()).abs() < 0.001);
    }

    #[test]
    fn test_weights_summary_label() {
        let w = OptimizationWeights::preset_power_dps();
        let label = w.summary_label();
        assert!(
            label.contains("Power"),
            "Power DPS should show Power, got: {}",
            label
        );
    }

    #[test]
    fn test_preset_roundtrip() {
        for (name, preset_fn) in &OptimizationWeights::PRESETS {
            let w = preset_fn();
            assert!(w.total() > 0.0, "Preset {} has zero total", name);
            let c = w.clamped();
            assert_eq!(w, c, "Preset {} should already be clamped", name);
        }
    }

    #[test]
    fn test_serde_backward_compat_disable_alias() {
        // Old serialized data with "disable" field should deserialize into "control"
        let json = r#"{"power":0.5,"disable":0.3,"condition":0.2,"healing":0.1,"sustain":0.4}"#;
        let w: OptimizationWeights = serde_json::from_str(json).expect("should deserialize");
        assert!(
            (w.control - 0.3).abs() < 0.001,
            "disable should alias to control"
        );
        assert!(
            (w.boon_support - 0.0).abs() < 0.001,
            "boon_support should default to 0.0"
        );
    }

    #[test]
    fn test_serde_new_format() {
        let w = OptimizationWeights::preset_balanced();
        let json = serde_json::to_string(&w).expect("should serialize");
        let w2: OptimizationWeights = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(w, w2);
    }

    #[test]
    fn test_tier_power_max_focused() {
        let w = OptimizationWeights::preset_power_dps();
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(
            prefixes.contains(&"Berserker's"),
            "Power max should include Berserker's"
        );
        assert!(
            prefixes.contains(&"Assassin's"),
            "Power max should include Assassin's"
        );
        assert!(
            prefixes.contains(&"Celestial"),
            "Should always include Celestial"
        );
        assert!(
            !prefixes.contains(&"Minstrel's"),
            "Power max should not include Minstrel's"
        );
        assert!(
            !prefixes.contains(&"Magi's"),
            "Power max should not include Magi's"
        );
        assert!(
            !prefixes.contains(&"Nomad's"),
            "Power max should not include Nomad's"
        );
    }

    #[test]
    fn test_tier_condi_dps_focused() {
        let w = OptimizationWeights::preset_condi_dps();
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(
            prefixes.contains(&"Viper's"),
            "Condi DPS should include Viper's"
        );
        assert!(
            prefixes.contains(&"Sinister"),
            "Condi DPS should include Sinister"
        );
        assert!(
            !prefixes.contains(&"Minstrel's"),
            "Condi DPS should not include Minstrel's"
        );
        assert!(
            !prefixes.contains(&"Magi's"),
            "Condi DPS should not include Magi's"
        );
    }

    #[test]
    fn test_tier_condi_sustain_heal_no_berserker() {
        let w = OptimizationWeights {
            power: 0.07,
            condition: 0.67,
            boon_support: 0.0,
            healing: 0.52,
            sustain: 0.67,
            control: 0.07,
        };
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(
            prefixes.contains(&"Viper's"),
            "Should include Viper's (condition tier 5)"
        );
        assert!(
            prefixes.contains(&"Trailblazer's"),
            "Should include Trailblazer's (condition+sustain tier 4)"
        );
        assert!(
            prefixes.contains(&"Dire"),
            "Should include Dire (condition tier 3)"
        );
        assert!(
            prefixes.contains(&"Magi's"),
            "Should include Magi's (heal tier 5)"
        );
        assert!(
            !prefixes.contains(&"Berserker's"),
            "Power axis is negligible -- no Berserker's"
        );
        assert!(
            !prefixes.contains(&"Assassin's"),
            "Power axis is negligible -- no Assassin's"
        );
    }

    #[test]
    fn test_tier_healer_includes_minstrels() {
        let w = OptimizationWeights::preset_healer();
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(
            prefixes.contains(&"Minstrel's"),
            "Healer preset should include Minstrel's"
        );
        assert!(
            prefixes.contains(&"Magi's"),
            "Healer preset should include Magi's"
        );
        assert!(
            prefixes.contains(&"Harrier's"),
            "Healer preset should include Harrier's"
        );
    }

    #[test]
    fn test_tier_always_has_celestial() {
        let w = OptimizationWeights {
            power: 0.0,
            condition: 0.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.0,
            control: 0.0,
        };
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(
            prefixes.contains(&"Celestial"),
            "Should always include Celestial"
        );
    }

    #[test]
    fn test_tier_power_condi_dual_focus() {
        let w = OptimizationWeights {
            power: 0.7,
            condition: 0.7,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.0,
            control: 0.0,
        };
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(prefixes.contains(&"Berserker's"));
        assert!(prefixes.contains(&"Grieving"));
        assert!(prefixes.contains(&"Viper's"));
        assert!(prefixes.contains(&"Sinister"));
        assert!(prefixes.contains(&"Trailblazer's"));
        assert!(!prefixes.contains(&"Minstrel's"));
        assert!(!prefixes.contains(&"Magi's"));
        assert!(!prefixes.contains(&"Nomad's"));
    }

    #[test]
    fn test_tier_balanced_broad_selection() {
        let w = OptimizationWeights::preset_balanced();
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(prefixes.contains(&"Berserker's"));
        assert!(
            prefixes.len() >= 6,
            "Balanced should have broad selection, got {}",
            prefixes.len()
        );
    }

    #[test]
    fn test_weight_to_tier_boundaries() {
        assert_eq!(weight_to_tier(0.0), 1);
        assert_eq!(weight_to_tier(0.19), 1);
        assert_eq!(weight_to_tier(0.2), 2);
        assert_eq!(weight_to_tier(0.39), 2);
        assert_eq!(weight_to_tier(0.4), 3);
        assert_eq!(weight_to_tier(0.59), 3);
        assert_eq!(weight_to_tier(0.6), 4);
        assert_eq!(weight_to_tier(0.79), 4);
        assert_eq!(weight_to_tier(0.8), 5);
        assert_eq!(weight_to_tier(1.0), 5);
    }

    #[test]
    fn test_score_power_build_higher_with_power_weights() {
        use crate::balance::BalanceContext;
        use crate::combat::{self, default_buff_profiles, DamageModifiers};
        use crate::stats;

        let ctx = BalanceContext::pve();
        let berserker = crate::stats::StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&berserker, "Warrior");
        let mods = DamageModifiers::default();
        let solo = &default_buff_profiles(&ctx)[0];
        let perf = combat::calculate_combat_performance(
            &berserker,
            &derived,
            &mods,
            solo,
            &combat::ConditionWeights::default_pve(),
            "Warrior",
            &ctx,
        );

        let tanky = crate::stats::StatBlock {
            power: 1200.0,
            precision: 1000.0,
            toughness: 2500.0,
            vitality: 2200.0,
            ..Default::default()
        };
        let derived_t = stats::compute_derived(&tanky, "Warrior");
        let perf_t = combat::calculate_combat_performance(
            &tanky,
            &derived_t,
            &mods,
            solo,
            &combat::ConditionWeights::default_pve(),
            "Warrior",
            &ctx,
        );

        let power_w = OptimizationWeights::preset_power_dps();
        assert!(
            score_with_weights(&perf, &power_w) > score_with_weights(&perf_t, &power_w),
            "Berserker should score higher with power weights"
        );

        let tank_w = OptimizationWeights::preset_tank();
        assert!(
            score_with_weights(&perf_t, &tank_w) > score_with_weights(&perf, &tank_w),
            "Tanky should score higher with sustain weights"
        );
    }

    #[test]
    fn test_score_condi_build_higher_with_condi_weights() {
        use crate::balance::BalanceContext;
        use crate::combat::{self, default_buff_profiles, DamageModifiers};
        use crate::stats;

        let ctx = BalanceContext::pve();
        let viper = crate::stats::StatBlock {
            power: 1800.0,
            precision: 1600.0,
            condition_damage: 2200.0,
            expertise: 600.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived_v = stats::compute_derived(&viper, "Necromancer");
        let mods = DamageModifiers::default();
        let solo = &default_buff_profiles(&ctx)[0];
        let perf_v = combat::calculate_combat_performance(
            &viper,
            &derived_v,
            &mods,
            solo,
            &combat::ConditionWeights::default_pve(),
            "Necromancer",
            &ctx,
        );

        let berserker = crate::stats::StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived_b = stats::compute_derived(&berserker, "Necromancer");
        let perf_b = combat::calculate_combat_performance(
            &berserker,
            &derived_b,
            &mods,
            solo,
            &combat::ConditionWeights::default_pve(),
            "Necromancer",
            &ctx,
        );

        let condi_w = OptimizationWeights::preset_condi_dps();
        assert!(
            score_with_weights(&perf_v, &condi_w) > score_with_weights(&perf_b, &condi_w),
            "Viper should score higher than Berserker with condition weights"
        );
    }

    #[test]
    fn test_get_set_roundtrip() {
        let mut w = OptimizationWeights::default();
        for i in 0..6 {
            w.set(i, 0.42);
            assert!((w.get(i) - 0.42).abs() < 0.001);
        }
    }

    #[test]
    fn test_set_constrained_respects_budget() {
        let mut w = OptimizationWeights::preset_power_dps();
        // Drag condition to max -- should reduce power to stay within budget
        w.set_constrained(1, 1.0); // axis 1 = Condition
        assert!(
            w.total() <= WEIGHT_BUDGET + 0.001,
            "Total {} should be <= budget {}",
            w.total(),
            WEIGHT_BUDGET
        );
        assert!(
            (w.condition - 1.0).abs() < 0.001,
            "Dragged axis should be at 1.0"
        );
        assert!(
            w.power < 1.0,
            "Power should have decreased from 1.0, got {}",
            w.power
        );
    }

    #[test]
    fn test_set_constrained_from_zero_others() {
        let mut w = OptimizationWeights {
            power: 0.0,
            condition: 0.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.0,
            control: 0.0,
        };
        w.set_constrained(0, 1.0);
        assert!((w.power - 1.0).abs() < 0.001);
        assert!(w.total() <= WEIGHT_BUDGET + 0.001);
    }

    #[test]
    fn test_score_trailblazer_beats_minstrel_condi_weights() {
        use crate::balance::BalanceContext;
        use crate::combat::{self, condition_weights_for_profession, DamageModifiers};
        use crate::stats;

        let ctx = BalanceContext::pve();
        let trailblazer = stats::StatBlock {
            power: 1000.0,
            precision: 1000.0,
            condition_damage: 2000.0,
            expertise: 900.0,
            toughness: 1500.0,
            vitality: 1500.0,
            ..Default::default()
        };
        let derived_t = stats::compute_derived(&trailblazer, "Ranger");
        let mods = DamageModifiers::default();
        let cw = condition_weights_for_profession("Ranger", &ctx);
        let solo = &combat::buff_profiles_for_profession("Ranger", &ctx)[0];
        let perf_t = combat::calculate_combat_performance(
            &trailblazer,
            &derived_t,
            &mods,
            solo,
            &cw,
            "Ranger",
            &ctx,
        );

        let minstrel = stats::StatBlock {
            power: 1200.0,
            precision: 1000.0,
            toughness: 2000.0,
            vitality: 2000.0,
            healing_power: 1500.0,
            concentration: 600.0,
            ..Default::default()
        };
        let derived_m = stats::compute_derived(&minstrel, "Ranger");
        let perf_m = combat::calculate_combat_performance(
            &minstrel, &derived_m, &mods, solo, &cw, "Ranger", &ctx,
        );

        let w = OptimizationWeights {
            power: 0.07,
            condition: 0.93,
            boon_support: 0.0,
            healing: 0.33,
            sustain: 0.67,
            control: 0.0,
        };

        let score_trail = score_with_weights(&perf_t, &w);
        let score_minst = score_with_weights(&perf_m, &w);
        assert!(
            score_trail > score_minst,
            "Trailblazer (score={:.4}) must beat Minstrel (score={:.4}) with condition-focused weights",
            score_trail, score_minst
        );
    }

    #[test]
    fn test_all_presets_within_budget() {
        for (name, preset_fn) in &OptimizationWeights::PRESETS {
            let w = preset_fn();
            assert!(
                w.total() <= WEIGHT_BUDGET + 0.001,
                "Preset {} total {} exceeds budget {}",
                name,
                w.total(),
                WEIGHT_BUDGET
            );
        }
    }

    // --- Gear Prefix Selection Tests ---

    #[test]
    fn test_gear_prefix_power_max() {
        let w = OptimizationWeights::preset_power_dps();
        let m = select_gear_prefix(&w);
        assert_eq!(
            m.primary, "Berserker's",
            "Power max should select Berserker's, got {}",
            m.primary
        );
    }

    #[test]
    fn test_gear_prefix_condi_dps() {
        let w = OptimizationWeights::preset_condi_dps();
        let m = select_gear_prefix(&w);
        assert!(
            m.primary == "Viper's" || m.primary == "Sinister",
            "Condi DPS should select Viper's or Sinister, got {}",
            m.primary
        );
    }

    #[test]
    fn test_gear_prefix_condi_with_sustain_is_trailblazer() {
        let w = OptimizationWeights {
            power: 0.0,
            condition: 1.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.5,
            control: 0.0,
        };
        let m = select_gear_prefix(&w);
        assert_eq!(
            m.primary, "Trailblazer's",
            "Condition + Sustain should select Trailblazer's, got {} (sim={:.3})",
            m.primary, m.similarity
        );
    }

    #[test]
    fn test_gear_prefix_condi_low_power_sustain_is_trailblazer() {
        let w = OptimizationWeights {
            power: 0.2,
            condition: 1.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.5,
            control: 0.0,
        };
        let m = select_gear_prefix(&w);
        assert!(
            m.primary == "Trailblazer's" || m.primary == "Viper's",
            "Condi max + low power + sustain should select Trailblazer's or Viper's, got {}",
            m.primary
        );
    }

    #[test]
    fn test_gear_prefix_screenshot_condi_disabler_weights() {
        let w = OptimizationWeights {
            power: 0.10,
            condition: 0.55,
            boon_support: 0.19,
            healing: 0.32,
            sustain: 0.42,
            control: 0.42,
        };

        let m = select_gear_prefix(&w);

        assert_eq!(
            m.primary, "Plaguedoctor's",
            "The screenshot's condition/control profile must not seed a power prefix"
        );
    }

    #[test]
    fn test_gear_prefix_healer() {
        let w = OptimizationWeights::preset_healer();
        let m = select_gear_prefix(&w);
        assert!(
            m.primary == "Magi's" || m.primary == "Harrier's" || m.primary == "Minstrel's",
            "Healer should select Magi's/Harrier's/Minstrel's, got {}",
            m.primary
        );
    }

    #[test]
    fn test_gear_prefix_tank() {
        let w = OptimizationWeights::preset_tank();
        let m = select_gear_prefix(&w);
        assert!(
            m.primary == "Nomad's"
                || m.primary == "Minstrel's"
                || m.primary == "Dire"
                || m.primary == "Cavalier's"
                || m.primary == "Knight's"
                || m.primary == "Soldier's",
            "Tank should select a defensive set, got {}",
            m.primary
        );
    }

    #[test]
    fn test_gear_prefix_celestial() {
        let w = OptimizationWeights::preset_celestial();
        let m = select_gear_prefix(&w);
        assert!(
            m.similarity > 0.5,
            "Celestial preset should have decent similarity, got {:.3}",
            m.similarity
        );
    }

    #[test]
    fn test_gear_prefix_zero_weights_is_celestial() {
        let w = OptimizationWeights {
            power: 0.0,
            condition: 0.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.0,
            control: 0.0,
        };
        let m = select_gear_prefix(&w);
        assert_eq!(
            m.primary, "Celestial",
            "Zero weights should default to Celestial"
        );
    }

    #[test]
    fn test_gear_prefix_power_condi_hybrid_is_grieving() {
        let w = OptimizationWeights {
            power: 0.7,
            condition: 0.7,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.0,
            control: 0.0,
        };
        let m = select_gear_prefix(&w);
        assert_eq!(
            m.primary, "Grieving",
            "Equal Power+Condition should select Grieving, got {}",
            m.primary
        );
    }

    #[test]
    fn test_gear_prefix_control_focused_is_diviners() {
        let w = OptimizationWeights {
            power: 0.3,
            condition: 0.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.0,
            control: 1.0,
        };
        let m = select_gear_prefix(&w);
        // Diviner's or Ritualist's -- both have high control/duration component
        assert!(
            m.primary == "Diviner's" || m.primary == "Ritualist's" || m.primary == "Celestial",
            "Control focused + some Power should select Diviner's/Ritualist's, got {}",
            m.primary
        );
    }

    #[test]
    fn test_gear_prefix_never_returns_healing_for_condi() {
        let cases = [
            OptimizationWeights {
                power: 0.0,
                condition: 1.0,
                boon_support: 0.0,
                healing: 0.0,
                sustain: 0.0,
                control: 0.0,
            },
            OptimizationWeights {
                power: 0.2,
                condition: 1.0,
                boon_support: 0.0,
                healing: 0.0,
                sustain: 0.0,
                control: 0.2,
            },
            OptimizationWeights {
                power: 0.0,
                condition: 1.0,
                boon_support: 0.0,
                healing: 0.0,
                sustain: 0.5,
                control: 0.0,
            },
            OptimizationWeights {
                power: 0.5,
                condition: 0.8,
                boon_support: 0.0,
                healing: 0.0,
                sustain: 0.0,
                control: 0.0,
            },
        ];
        let healing_prefixes = ["Magi's", "Harrier's", "Minstrel's", "Cleric's"];
        for w in &cases {
            let m = select_gear_prefix(w);
            assert!(
                !healing_prefixes.contains(&m.primary),
                "Condition-focused weights should NOT return healing prefix, got {}",
                m.primary
            );
        }
    }

    #[test]
    fn test_gear_prefix_never_returns_healing_for_power() {
        let cases = [
            OptimizationWeights::preset_power_dps(),
            OptimizationWeights {
                power: 1.0,
                condition: 0.0,
                boon_support: 0.0,
                healing: 0.0,
                sustain: 0.3,
                control: 0.0,
            },
            OptimizationWeights {
                power: 0.8,
                condition: 0.0,
                boon_support: 0.0,
                healing: 0.0,
                sustain: 0.0,
                control: 0.2,
            },
        ];
        let healing_prefixes = ["Magi's", "Harrier's", "Minstrel's", "Cleric's"];
        for w in &cases {
            let m = select_gear_prefix(w);
            assert!(
                !healing_prefixes.contains(&m.primary),
                "Power-focused weights should NOT return healing prefix, got {}",
                m.primary
            );
        }
    }

    // --- ObjectiveScorer Tests ---

    #[test]
    fn test_objective_scorer_from_mode() {
        let w = OptimizationWeights::preset_power_dps();
        let scorer = ObjectiveScorer::from_mode(w.clone(), "PvE");
        assert!((scorer.weight_budget - 2.0).abs() < 0.001);
        assert!(!scorer.boon_priorities.is_empty());
        assert!(!scorer.condition_priorities.is_empty());
    }

    #[test]
    fn test_objective_scorer_fallback() {
        let w = OptimizationWeights::preset_power_dps();
        let scorer = ObjectiveScorer::fallback(w.clone());
        assert!((scorer.weight_budget - 2.0).abs() < 0.001);
        assert!(scorer.boon_priorities.is_empty());
        assert_eq!(scorer.weights, w);
    }

    #[test]
    fn test_objective_scorer_boon_priority() {
        let w = OptimizationWeights::preset_power_dps();
        let scorer = ObjectiveScorer::from_mode(w, "PvE");
        let might_prio = scorer.boon_priority("Might");
        assert!(
            might_prio > 0.0,
            "Might should have positive priority in PvE"
        );
        // Unknown boon returns 0.5 default
        let unknown = scorer.boon_priority("Unknown");
        assert!((unknown - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_objective_scorer_condition_priority() {
        let w = OptimizationWeights::preset_condi_dps();
        let scorer = ObjectiveScorer::from_mode(w, "PvE");
        let burning_prio = scorer.condition_priority("Burning");
        assert!(
            burning_prio > 0.0,
            "Burning should have positive priority in PvE Condi"
        );
    }

    #[test]
    fn test_default_for_mode_uses_profile_data() {
        let pve = OptimizationWeights::default_for_mode("PvE");
        let pvp = OptimizationWeights::default_for_mode("PvP");
        let wvw = OptimizationWeights::default_for_mode("WvW");
        // They should be different
        assert_ne!(pve, pvp, "PvE and PvP defaults should differ");
        assert_ne!(pve, wvw, "PvE and WvW defaults should differ");
    }

    /// ObjectiveScorer::score() uses profile-loaded norms, not module-level defaults.
    /// Two scorers with different profiles produce different scores for the same build.
    #[test]
    fn test_objective_scorer_uses_profile_norms() {
        use crate::balance::BalanceContext;
        use crate::combat::{
            self, buff_profiles_for_profession, condition_weights_for_profession, DamageModifiers,
        };
        use crate::stats;

        let ctx = BalanceContext::pve();
        let stats = stats::StatBlock {
            power: 2000.0,
            precision: 1500.0,
            ferocity: 800.0,
            toughness: 1200.0,
            vitality: 1400.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Guardian");
        let mods = DamageModifiers::default();
        let cw = condition_weights_for_profession("Guardian", &ctx);
        let profile = &buff_profiles_for_profession("Guardian", &ctx)[0];
        let perf = combat::calculate_combat_performance(
            &stats, &derived, &mods, profile, &cw, "Guardian", &ctx,
        );

        // PvE Power DPS scorer
        let pve_scorer = ObjectiveScorer::from_mode(OptimizationWeights::preset_power_dps(), "PvE");
        // WvW Roamer scorer (has different normalization constants + weights)
        let wvw_scorer =
            ObjectiveScorer::from_mode(OptimizationWeights::default_for_mode("WvW"), "WvW");

        let score_pve = pve_scorer.score(&perf);
        let score_wvw = wvw_scorer.score(&perf);

        // Scores should differ because profiles have different weights
        assert_ne!(
            score_pve, score_wvw,
            "PvE Power DPS scorer and WvW scorer should produce different scores for the same build"
        );
    }

    /// ObjectiveScorer::score() and score_with_weights() agree when using same weights and
    /// the default normalization constants (fallback scorer == module-level constants).
    #[test]
    fn test_objective_scorer_fallback_matches_score_with_weights() {
        use crate::balance::BalanceContext;
        use crate::combat::{
            self, buff_profiles_for_profession, condition_weights_for_profession, DamageModifiers,
        };
        use crate::stats;

        let ctx = BalanceContext::pve();
        let stats = stats::StatBlock {
            power: 2000.0,
            precision: 1500.0,
            ferocity: 800.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Warrior");
        let mods = DamageModifiers::default();
        let cw = condition_weights_for_profession("Warrior", &ctx);
        let profile = &buff_profiles_for_profession("Warrior", &ctx)[0];
        let perf = combat::calculate_combat_performance(
            &stats, &derived, &mods, profile, &cw, "Warrior", &ctx,
        );

        let weights = OptimizationWeights::preset_power_dps();
        let fallback_scorer = ObjectiveScorer::fallback(weights.clone());

        let score_scorer = fallback_scorer.score(&perf);
        let score_fn = score_with_weights(&perf, &weights);

        assert!(
            (score_scorer - score_fn).abs() < 0.0001,
            "Fallback scorer (score={}) should match score_with_weights (score={})",
            score_scorer,
            score_fn
        );
    }
}
