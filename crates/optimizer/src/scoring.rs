//! 5-axis optimization weights and scoring functions.
//! Replaces the old Archetype + AggressionLevel system with a unified
//! radar-chart-driven scoring model.

use serde::{Deserialize, Serialize};

use crate::combat::CombatPerformance;

/// Normalization divisors for cross-axis score comparison.
/// These map realistic Solo-profile maximums to ~1.0 so per-axis scores
/// occupy the same 0.0–1.0 range and no single axis can dominate.
///
/// Calibrated against typical fully-geared Ascended builds:
/// - Berserker Warrior Solo: strike_dps_index ≈ 2200
/// - Viper Necro Solo: condition_dps_index ≈ 3000
/// - Minstrel Guardian Solo: effective_health ≈ 45000
/// - Minstrel/Harrier Druid: healing_power_index ≈ 1500
///
/// All per-axis scores are capped at 1.0 in score_with_weights() so
/// even extreme builds cannot exceed the normalized range.
pub const STRIKE_DPS_NORM: f64 = 3000.0;
pub const CONDI_DPS_NORM: f64 = 3500.0;
pub const EFFECTIVE_HEALTH_NORM: f64 = 50000.0;
pub const HEALING_NORM: f64 = 1500.0;

/// 5-axis radar chart labels, in render order (clockwise from top).
pub const AXIS_LABELS: [&str; 5] = ["Power", "Disable", "Condition", "Heal", "Sustain"];

/// Total weight budget across all 5 axes.
///
/// Models the real GW2 gear stat budget: each gear piece has a fixed stat pool
/// (attribute_adjustment * multiplier + value) distributed across 3–9 attributes.
/// You can't max everything — choosing Berserker's (Power/Prec/Ferocity) means
/// zero Condition Damage/Toughness/Healing. This budget forces the same trade-off
/// on the radar chart.
///
/// Budget 2.0 means:
/// - Focused build: 1 axis at 1.0 + scraps (like Berserker's 3-stat)
/// - Dual focus: 2 axes at ~0.7 each + scraps (like Viper's 4-stat)
/// - Hybrid: 5 axes at ~0.4 each (like Celestial 9-stat)
pub const WEIGHT_BUDGET: f64 = 2.0;

/// 5-axis optimization weights. Each axis 0.0–1.0, total constrained to WEIGHT_BUDGET.
/// Drives gear search, trait selection, and build scoring.
///
/// Axes (clockwise from top, matching radar chart layout):
/// 1. Power — strike/direct damage (strike_dps_index)
/// 2. Disable — CC, stability, interrupts (boon_duration + condi_duration proxy)
/// 3. Condition — condition/DoT damage (condition_dps_index)
/// 4. Heal — healing output (healing_power_index)
/// 5. Sustain — survivability (effective_health + damage_reduction)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationWeights {
    pub power: f64,
    pub disable: f64,
    pub condition: f64,
    pub healing: f64,
    pub sustain: f64,
}

impl Default for OptimizationWeights {
    fn default() -> Self {
        Self::preset_balanced()
    }
}

impl OptimizationWeights {
    /// Clamp all axes to [0.0, 1.0].
    pub fn clamped(&self) -> Self {
        Self {
            power: self.power.clamp(0.0, 1.0),
            disable: self.disable.clamp(0.0, 1.0),
            condition: self.condition.clamp(0.0, 1.0),
            healing: self.healing.clamp(0.0, 1.0),
            sustain: self.sustain.clamp(0.0, 1.0),
        }
    }

    /// Sum of all weights.
    pub fn total(&self) -> f64 {
        self.power + self.disable + self.condition + self.healing + self.sustain
    }

    /// Get weight by axis index (0=Power, 1=Disable, 2=Condition, 3=Heal, 4=Sustain).
    pub fn get(&self, index: usize) -> f64 {
        match index {
            0 => self.power,
            1 => self.disable,
            2 => self.condition,
            3 => self.healing,
            4 => self.sustain,
            _ => 0.0,
        }
    }

    /// Set weight by axis index (unconstrained — does not enforce budget).
    pub fn set(&mut self, index: usize, value: f64) {
        let v = value.clamp(0.0, 1.0);
        match index {
            0 => self.power = v,
            1 => self.disable = v,
            2 => self.condition = v,
            3 => self.healing = v,
            4 => self.sustain = v,
            _ => {}
        }
    }

    /// Set weight by axis index, enforcing the WEIGHT_BUDGET constraint.
    /// When the new value would push the total over budget, other axes are
    /// proportionally scaled down — modeling real GW2 gear stat trade-offs
    /// where investing in one stat means less budget for others.
    pub fn set_constrained(&mut self, index: usize, new_val: f64) {
        let new_val = new_val.clamp(0.0, 1.0);
        self.set(index, new_val);

        let total = self.total();
        if total > WEIGHT_BUDGET {
            let excess = total - WEIGHT_BUDGET;
            // Sum of all other axes
            let other_total: f64 = (0..5)
                .filter(|&i| i != index)
                .map(|i| self.get(i))
                .sum();

            if other_total > 0.001 {
                // Scale down other axes proportionally
                let scale = ((other_total - excess) / other_total).max(0.0);
                for i in 0..5 {
                    if i != index {
                        let v = self.get(i);
                        self.set(i, v * scale);
                    }
                }
            } else {
                // All others are ~0, cap the dragged axis at budget
                self.set(index, WEIGHT_BUDGET.min(1.0));
            }
        }
    }

    /// Remaining budget available for distribution.
    pub fn budget_remaining(&self) -> f64 {
        (WEIGHT_BUDGET - self.total()).max(0.0)
    }

    /// Return weights as an array in axis order.
    pub fn as_array(&self) -> [f64; 5] {
        [self.power, self.disable, self.condition, self.healing, self.sustain]
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

    /// Default weights for a game mode.
    /// Default weights for a game mode (all within WEIGHT_BUDGET).
    pub fn default_for_mode(mode: &str) -> Self {
        match mode {
            "PvE" => Self { power: 0.8, disable: 0.1, condition: 0.2, healing: 0.0, sustain: 0.1 },
            // total = 1.2 (PvE favors offense)
            "WvW" => Self { power: 0.4, disable: 0.3, condition: 0.2, healing: 0.2, sustain: 0.5 },
            // total = 1.6 (WvW needs sustain + utility)
            "PvP" => Self { power: 0.4, disable: 0.4, condition: 0.2, healing: 0.2, sustain: 0.5 },
            // total = 1.7 (PvP needs CC + sustain)
            _ => Self::preset_balanced(),
        }
    }

    /// Convert to per-stat weights for trait scoring.
    pub fn to_stat_weights(&self) -> StatWeights {
        StatWeights {
            power: self.power * 0.8,
            precision: self.power * 0.6 + self.condition * 0.2,
            ferocity: self.power * 0.5,
            condition_damage: self.condition * 0.8 + self.disable * 0.2,
            expertise: self.condition * 0.6 + self.disable * 0.4,
            concentration: self.disable * 0.5 + self.healing * 0.3,
            healing_power: self.healing * 0.9,
            toughness: self.sustain * 0.8,
            vitality: self.sustain * 0.7,
        }
    }

    // --- Presets ---

    // --- Presets (all respect WEIGHT_BUDGET = 2.0) ---
    // Model real GW2 gear stat distributions:
    // - 3-stat focus (Berserker's): 1 major axis high, 2 minor scraps → total ~1.2
    // - 4-stat hybrid (Viper's): 2 axes moderate-high → total ~1.5
    // - Celestial: all axes equal → total = 2.0

    pub fn preset_power_dps() -> Self {
        // Berserker's/Assassin's: max strike DPS, minimal elsewhere
        Self { power: 1.0, disable: 0.0, condition: 0.0, healing: 0.0, sustain: 0.1 }
        // total = 1.1
    }

    pub fn preset_condi_dps() -> Self {
        // Viper's: high condi + some power + expertise for disable
        Self { power: 0.2, disable: 0.2, condition: 1.0, healing: 0.0, sustain: 0.1 }
        // total = 1.5
    }

    pub fn preset_tank() -> Self {
        // Minstrel's/Nomad's: max sustain + some healing + some disable
        Self { power: 0.1, disable: 0.2, condition: 0.0, healing: 0.3, sustain: 1.0 }
        // total = 1.6
    }

    pub fn preset_healer() -> Self {
        // Harrier's/Magi's: max healing + moderate sustain + boon duration
        Self { power: 0.0, disable: 0.2, condition: 0.0, healing: 1.0, sustain: 0.4 }
        // total = 1.6
    }

    pub fn preset_balanced() -> Self {
        // Mixed build: even spread across offensive + defensive
        Self { power: 0.5, disable: 0.3, condition: 0.3, healing: 0.2, sustain: 0.5 }
        // total = 1.8
    }

    pub fn preset_celestial() -> Self {
        // Celestial: equal investment in everything
        Self { power: 0.4, disable: 0.4, condition: 0.4, healing: 0.4, sustain: 0.4 }
        // total = 2.0
    }

    /// Named presets for UI buttons.
    pub const PRESETS: [(&'static str, fn() -> Self); 6] = [
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

/// Score a build against user-specified 5-axis weights.
/// Each CombatPerformance metric is normalized to [0, 1] using realistic
/// Solo-profile maximums, then multiplied by the corresponding weight.
/// All per-axis scores are hard-capped at 1.0 so no single axis can
/// dominate regardless of how extreme the raw metric values are.
///
/// Includes a misalignment penalty: if the user weighted any axis ≥ 0.4
/// but the build scores below 0.15 on that axis, the total score is
/// reduced by 70%. This catches edge cases where a set sneaks through
/// tier selection via a secondary axis but is fundamentally wrong for
/// the user's primary intent.
pub fn score_with_weights(perf: &CombatPerformance, weights: &OptimizationWeights) -> f64 {
    let w = weights.clamped();
    let total_w = w.total().max(0.01);

    let power_score = (perf.strike_dps_index / STRIKE_DPS_NORM).min(1.0);
    let condition_score = (perf.condition_dps_index / CONDI_DPS_NORM
        + (perf.condi_duration_pct / 100.0).min(1.0) * 0.15)
        .min(1.0);
    let sustain_score = (perf.effective_health / EFFECTIVE_HEALTH_NORM
        + perf.damage_reduction_pct / 100.0)
        .min(1.0);
    let healing_score = (perf.healing_power_index / HEALING_NORM).min(1.0);
    // CC/Disable: proxy via boon duration (stability uptime) + condi duration (CC conditions)
    let disable_score = (perf.boon_duration_pct / 100.0 * 0.5
        + perf.condi_duration_pct / 100.0 * 0.5)
        .min(1.0);

    let raw = (w.power * power_score
        + w.condition * condition_score
        + w.sustain * sustain_score
        + w.healing * healing_score
        + w.disable * disable_score)
        / total_w;

    // Graduated penalty: if any axis the user weighted ≥ 0.4 scores below 0.15,
    // penalize proportionally to that axis's weight. Neglecting a higher-weighted
    // axis costs more — e.g., zero condition at weight 0.67 hurts more than zero
    // healing at weight 0.52. Multiplicative so multiple neglected axes compound.
    let axis_weights = [w.power, w.disable, w.condition, w.healing, w.sustain];
    let axis_scores = [power_score, disable_score, condition_score, healing_score, sustain_score];
    let mut penalty = 1.0;
    for i in 0..5 {
        if axis_weights[i] >= 0.4 && axis_scores[i] < 0.15 {
            penalty *= 1.0 - axis_weights[i] * 0.7;
        }
    }

    raw * penalty
}

// ─── Hierarchical Tier Tables ───
//
// Each axis has 5 tiers of stat prefixes. Higher tier = more focused set,
// only appropriate at higher weight levels.
//
// Tier 5 (0.8-1.0): Pure focused sets — only 1-2 options
// Tier 4 (0.6-0.8): Focused + one secondary stat
// Tier 3 (0.4-0.6): Moderate presence of this axis's stats
// Tier 2 (0.2-0.4): Minor presence, not the set's primary purpose
// Tier 1 (0.0-0.2): Minimal contribution
//
// Selection includes the requested tier AND all tiers above (more focused
// sets are always appropriate). E.g., tier 3 gets tier 3+4+5 sets.
//
// When multiple axes are active, unions of their tier selections form the
// candidate pool. Overlapping sets (e.g., Trailblazer's in both Condition
// tier 4 and Sustain tier 4) naturally emerge as strong candidates.

/// Power axis tiers: strike DPS (Power / Precision / Ferocity)
const POWER_TIERS: [&[&str]; 5] = [
    &["Cleric's"],                                           // Tier 1: HP/Pow/Tou — minor Power
    &["Soldier's", "Knight's", "Cavalier's", "Harrier's"],   // Tier 2: some Power, not primary
    &["Valkyrie", "Diviner's", "Viper's", "Sinister"],       // Tier 3: moderate Power presence
    &["Marauder", "Dragon's", "Grieving"],                    // Tier 4: Power-major with 1 extra
    &["Berserker's", "Assassin's"],                           // Tier 5: pure Pow/Prec/Fer
];

/// Condition axis tiers: DoT damage (ConditionDamage / Expertise)
const CONDITION_TIERS: [&[&str]; 5] = [
    &[],                                                      // Tier 1: very few sets carry minor CD
    &["Apothecary's"],                                        // Tier 2: HP/CD/Tou — CD as secondary
    &["Dire", "Ritualist's", "Plaguedoctor's"],               // Tier 3: moderate CD presence
    &["Trailblazer's", "Grieving"],                           // Tier 4: CD-major + secondary
    &["Viper's", "Sinister"],                                 // Tier 5: pure CD offensive
];

/// Sustain axis tiers: survivability (Toughness / Vitality)
const SUSTAIN_TIERS: [&[&str]; 5] = [
    &["Magi's", "Apothecary's"],                              // Tier 1: Vit as minor stat
    &["Ritualist's", "Plaguedoctor's", "Cleric's"],           // Tier 2: some survivability
    &["Knight's", "Cavalier's", "Valkyrie", "Marauder", "Dragon's"], // Tier 3: Vit or Tou secondary
    &["Trailblazer's", "Dire", "Soldier's"],                  // Tier 4: Tou+Vit as major component
    &["Nomad's", "Minstrel's"],                               // Tier 5: max Toughness + Vitality
];

/// Heal axis tiers: healing output (HealingPower)
const HEAL_TIERS: [&[&str]; 5] = [
    &[],                                                      // Tier 1: very few sets carry minor HP
    &[],                                                      // Tier 2: (Celestial handles this)
    &["Plaguedoctor's", "Apothecary's", "Nomad's"],           // Tier 3: moderate HP
    &["Harrier's", "Cleric's"],                               // Tier 4: HP prominent
    &["Magi's", "Minstrel's"],                                // Tier 5: HP as primary stat
];

/// Disable axis tiers: CC/boon/condi duration (Concentration / Expertise)
/// Note: Minstrel's is excluded here — its Concentration is a bonus on top of
/// its Heal+Tank identity. It appears in SUSTAIN_TIERS and HEAL_TIERS instead.
const DISABLE_TIERS: [&[&str]; 5] = [
    &[],                                                      // Tier 1: very few sets carry minor duration
    &[],                                                      // Tier 2: (Celestial handles this)
    &["Viper's", "Trailblazer's", "Plaguedoctor's"],          // Tier 3: Exp or Conc as secondary
    &["Harrier's"],                                           // Tier 4: Concentration + Power + HP
    &["Diviner's", "Ritualist's"],                            // Tier 5: high Conc or Exp
];

/// Convert a weight value (0.0-1.0) to a tier level (1-5).
fn weight_to_tier(w: f64) -> usize {
    if w >= 0.8 { 5 }
    else if w >= 0.6 { 4 }
    else if w >= 0.4 { 3 }
    else if w >= 0.2 { 2 }
    else { 1 }
}

/// Select stat prefixes using hierarchical tier tables.
///
/// Each radar chart axis maps to a tier table. The weight level determines
/// which tier to start from — higher weights select fewer, more focused sets.
/// All tiers from the selected level UP to tier 5 are included (focused sets
/// are always appropriate at lower weight levels too).
///
/// When multiple axes are active, their selections are unioned — overlapping
/// sets that serve multiple axes naturally become strong candidates.
///
/// Celestial (all 9 stats) is always included as the universal hybrid.
pub fn select_prefixes_by_tiers(weights: &OptimizationWeights) -> Vec<&'static str> {
    let w = weights.clamped();
    let mut prefixes = Vec::new();

    let axes: [(f64, &[&[&str]; 5]); 5] = [
        (w.power, &POWER_TIERS),
        (w.condition, &CONDITION_TIERS),
        (w.sustain, &SUSTAIN_TIERS),
        (w.healing, &HEAL_TIERS),
        (w.disable, &DISABLE_TIERS),
    ];

    for (weight, tiers) in &axes {
        if *weight < 0.15 { continue; } // Skip negligible axes (below tier 1 threshold)
        let tier = weight_to_tier(*weight); // 1-5
        // Include this tier and all above (more focused sets are always appropriate)
        for t in (tier - 1)..5 {
            prefixes.extend_from_slice(tiers[t]);
        }
    }

    // Universal hybrid — always included
    prefixes.push("Celestial");

    prefixes.sort();
    prefixes.dedup();
    prefixes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weights_clamped() {
        let w = OptimizationWeights {
            power: 1.5, disable: -0.5, condition: 0.5, healing: 0.0, sustain: 1.0,
        };
        let c = w.clamped();
        assert_eq!(c.power, 1.0);
        assert_eq!(c.disable, 0.0);
        assert_eq!(c.condition, 0.5);
    }

    #[test]
    fn test_weights_total() {
        let w = OptimizationWeights::preset_power_dps();
        // Power DPS: power=1.0, sustain=0.1 → total = 1.1
        assert!((w.total() - 1.1).abs() < 0.001);
    }

    #[test]
    fn test_weights_summary_label() {
        let w = OptimizationWeights::preset_power_dps();
        let label = w.summary_label();
        assert!(label.contains("Power"), "Power DPS should show Power, got: {}", label);
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
    fn test_tier_power_max_focused() {
        // Power=1.0 only → tier 5 → just Berserker's + Assassin's + Celestial
        let w = OptimizationWeights::preset_power_dps();
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(prefixes.contains(&"Berserker's"), "Power max should include Berserker's");
        assert!(prefixes.contains(&"Assassin's"), "Power max should include Assassin's");
        assert!(prefixes.contains(&"Celestial"), "Should always include Celestial");
        assert!(!prefixes.contains(&"Minstrel's"), "Power max should not include Minstrel's");
        assert!(!prefixes.contains(&"Magi's"), "Power max should not include Magi's");
        assert!(!prefixes.contains(&"Nomad's"), "Power max should not include Nomad's");
    }

    #[test]
    fn test_tier_condi_dps_focused() {
        let w = OptimizationWeights::preset_condi_dps();
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(prefixes.contains(&"Viper's"), "Condi DPS should include Viper's");
        assert!(prefixes.contains(&"Sinister"), "Condi DPS should include Sinister");
        assert!(!prefixes.contains(&"Minstrel's"), "Condi DPS should not include Minstrel's");
        assert!(!prefixes.contains(&"Magi's"), "Condi DPS should not include Magi's");
    }

    #[test]
    fn test_tier_condi_sustain_heal_no_berserker() {
        // User scenario: Condition=0.67, Sustain=0.67, Heal=0.52
        // Power=0.07 (skipped), so NO pure power sets should appear
        let w = OptimizationWeights {
            power: 0.07, disable: 0.07, condition: 0.67, healing: 0.52, sustain: 0.67,
        };
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(prefixes.contains(&"Viper's"), "Should include Viper's (condition tier 5)");
        assert!(prefixes.contains(&"Trailblazer's"), "Should include Trailblazer's (condition+sustain tier 4)");
        assert!(prefixes.contains(&"Dire"), "Should include Dire (condition tier 3)");
        assert!(prefixes.contains(&"Magi's"), "Should include Magi's (heal tier 5)");
        assert!(!prefixes.contains(&"Berserker's"), "Power axis is negligible — no Berserker's");
        assert!(!prefixes.contains(&"Assassin's"), "Power axis is negligible — no Assassin's");
    }

    #[test]
    fn test_tier_healer_includes_minstrels() {
        let w = OptimizationWeights::preset_healer();
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(prefixes.contains(&"Minstrel's"), "Healer preset should include Minstrel's");
        assert!(prefixes.contains(&"Magi's"), "Healer preset should include Magi's");
        assert!(prefixes.contains(&"Harrier's"), "Healer preset should include Harrier's");
    }

    #[test]
    fn test_tier_always_has_celestial() {
        let w = OptimizationWeights { power: 0.0, disable: 0.0, condition: 0.0, healing: 0.0, sustain: 0.0 };
        let prefixes = select_prefixes_by_tiers(&w);
        assert!(prefixes.contains(&"Celestial"), "Should always include Celestial");
    }

    #[test]
    fn test_tier_power_condi_dual_focus() {
        // Power=0.7, Condition=0.7 → both at tier 4
        let w = OptimizationWeights {
            power: 0.7, disable: 0.0, condition: 0.7, healing: 0.0, sustain: 0.0,
        };
        let prefixes = select_prefixes_by_tiers(&w);
        // Power tier 4+5: Berserker's, Assassin's, Marauder, Dragon's, Grieving
        assert!(prefixes.contains(&"Berserker's"));
        assert!(prefixes.contains(&"Grieving"), "Grieving is Power tier 4 + bridges condition");
        // Condition tier 4+5: Viper's, Sinister, Trailblazer's, Grieving
        assert!(prefixes.contains(&"Viper's"));
        assert!(prefixes.contains(&"Sinister"));
        assert!(prefixes.contains(&"Trailblazer's"));
        // Should NOT have pure heal/tank sets
        assert!(!prefixes.contains(&"Minstrel's"));
        assert!(!prefixes.contains(&"Magi's"));
        assert!(!prefixes.contains(&"Nomad's"));
    }

    #[test]
    fn test_tier_balanced_broad_selection() {
        // Balanced preset: P=0.5, D=0.3, C=0.3, H=0.2, S=0.5
        let w = OptimizationWeights::preset_balanced();
        let prefixes = select_prefixes_by_tiers(&w);
        // Power at 0.5 → tier 3: gets Valkyrie, Diviner's, Viper's, Sinister + tier 4+5
        assert!(prefixes.contains(&"Berserker's"));
        assert!(prefixes.contains(&"Valkyrie"));
        // Sustain at 0.5 → tier 3: gets Knight's, Cavalier's, etc.
        assert!(prefixes.contains(&"Knight's"));
        // Should have a decent variety
        assert!(prefixes.len() >= 8, "Balanced should have broad selection, got {}", prefixes.len());
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
        use crate::combat::{self, DamageModifiers, default_buff_profiles};
        use crate::stats;

        let berserker = crate::stats::StatBlock {
            power: 2800.0, precision: 2100.0, ferocity: 1400.0,
            toughness: 1000.0, vitality: 1000.0, ..Default::default()
        };
        let derived = stats::compute_derived(&berserker, "Warrior");
        let mods = DamageModifiers::default();
        let solo = &default_buff_profiles()[0];
        let perf = combat::calculate_combat_performance(
            &berserker, &derived, &mods, solo, "Warrior",
        );

        let tanky = crate::stats::StatBlock {
            power: 1200.0, precision: 1000.0, toughness: 2500.0,
            vitality: 2200.0, ..Default::default()
        };
        let derived_t = stats::compute_derived(&tanky, "Warrior");
        let perf_t = combat::calculate_combat_performance(
            &tanky, &derived_t, &mods, solo, "Warrior",
        );

        // Power weights → berserker wins
        let power_w = OptimizationWeights::preset_power_dps();
        assert!(
            score_with_weights(&perf, &power_w) > score_with_weights(&perf_t, &power_w),
            "Berserker should score higher with power weights"
        );

        // Tank weights → tanky wins
        let tank_w = OptimizationWeights::preset_tank();
        assert!(
            score_with_weights(&perf_t, &tank_w) > score_with_weights(&perf, &tank_w),
            "Tanky should score higher with sustain weights"
        );
    }

    #[test]
    fn test_score_condi_build_higher_with_condi_weights() {
        use crate::combat::{self, DamageModifiers, default_buff_profiles};
        use crate::stats;

        let viper = crate::stats::StatBlock {
            power: 1800.0, precision: 1600.0, condition_damage: 2200.0,
            expertise: 600.0, toughness: 1000.0, vitality: 1000.0,
            ..Default::default()
        };
        let derived_v = stats::compute_derived(&viper, "Necromancer");
        let mods = DamageModifiers::default();
        let solo = &default_buff_profiles()[0];
        let perf_v = combat::calculate_combat_performance(
            &viper, &derived_v, &mods, solo, "Necromancer",
        );

        let berserker = crate::stats::StatBlock {
            power: 2800.0, precision: 2100.0, ferocity: 1400.0,
            toughness: 1000.0, vitality: 1000.0, ..Default::default()
        };
        let derived_b = stats::compute_derived(&berserker, "Necromancer");
        let perf_b = combat::calculate_combat_performance(
            &berserker, &derived_b, &mods, solo, "Necromancer",
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
        for i in 0..5 {
            w.set(i, 0.42);
            assert!((w.get(i) - 0.42).abs() < 0.001);
        }
    }

    #[test]
    fn test_set_constrained_respects_budget() {
        let mut w = OptimizationWeights::preset_power_dps();
        // Drag condition to max — should reduce power to stay within budget
        w.set_constrained(2, 1.0); // axis 2 = Condition
        assert!(
            w.total() <= WEIGHT_BUDGET + 0.001,
            "Total {} should be <= budget {}",
            w.total(),
            WEIGHT_BUDGET
        );
        assert!((w.condition - 1.0).abs() < 0.001, "Dragged axis should be at 1.0");
        assert!(
            w.power < 1.0,
            "Power should have decreased from 1.0, got {}",
            w.power
        );
    }

    #[test]
    fn test_set_constrained_from_zero_others() {
        let mut w = OptimizationWeights {
            power: 0.0, disable: 0.0, condition: 0.0, healing: 0.0, sustain: 0.0,
        };
        w.set_constrained(0, 1.0);
        assert!((w.power - 1.0).abs() < 0.001);
        assert!(w.total() <= WEIGHT_BUDGET + 0.001);
    }

    #[test]
    fn test_score_trailblazer_beats_minstrel_condi_weights() {
        // Trailblazer's (CondDmg) must score higher than Minstrel's (zero CondDmg)
        // with condition-focused weights. Now ensured by tier selection excluding
        // Minstrel's, but scoring should also reflect this correctly.
        use crate::combat::{self, DamageModifiers, default_buff_profiles};
        use crate::stats;

        let trailblazer = stats::StatBlock {
            power: 1000.0, precision: 1000.0,
            condition_damage: 2000.0, expertise: 900.0,
            toughness: 1500.0, vitality: 1500.0,
            ..Default::default()
        };
        let derived_t = stats::compute_derived(&trailblazer, "Ranger");
        let mods = DamageModifiers::default();
        let solo = &default_buff_profiles()[0];
        let perf_t = combat::calculate_combat_performance(
            &trailblazer, &derived_t, &mods, solo, "Ranger",
        );

        let minstrel = stats::StatBlock {
            power: 1200.0, precision: 1000.0,
            toughness: 2000.0, vitality: 2000.0,
            healing_power: 1500.0, concentration: 600.0,
            ..Default::default()
        };
        let derived_m = stats::compute_derived(&minstrel, "Ranger");
        let perf_m = combat::calculate_combat_performance(
            &minstrel, &derived_m, &mods, solo, "Ranger",
        );

        let w = OptimizationWeights {
            power: 0.07, disable: 0.07, condition: 0.67, healing: 0.52, sustain: 0.67,
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
}
