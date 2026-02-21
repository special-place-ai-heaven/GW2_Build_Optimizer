//! Combat performance model.
//! Calculates real combat metrics (strike DPS, condition DPS, healing, survivability)
//! using GW2's published formulas. Replaces hardcoded buff assumptions with proper
//! buff profiles and percentage-based damage modifiers from traits/runes/sigils.

use std::collections::HashMap;

use gw2_api::models::{Fact, Item, Trait};

use crate::stats::{self, DerivedStats, StatBlock};

/// Percentage-based damage modifiers from traits, runes, sigils, relics.
/// Stacks multiplicatively: total = product(1 + modifier) for each source.
#[derive(Debug, Clone, Default)]
pub struct DamageModifiers {
    /// Strike damage increase percentages (e.g. 0.05 for +5%)
    pub strike_pct: Vec<f64>,
    /// Global condition damage increase percentages
    pub condition_pct: Vec<f64>,
    /// Per-condition type damage increase (e.g. "Burning" => [0.20])
    pub specific_condi: HashMap<String, Vec<f64>>,
    /// Additive crit damage bonus percentages (added to base 150% + ferocity)
    pub crit_damage_pct: Vec<f64>,
    /// Global condition duration increase percentages (added to Expertise-based)
    pub condi_duration_pct: Vec<f64>,
    /// Per-condition type duration increase
    pub specific_condi_duration: HashMap<String, Vec<f64>>,
    /// Global boon duration increase percentages (added to Concentration-based)
    pub boon_duration_pct: Vec<f64>,
    /// Outgoing healing increase percentages
    pub healing_pct: Vec<f64>,
}

impl DamageModifiers {
    /// Total multiplicative strike damage modifier.
    pub fn total_strike_mult(&self) -> f64 {
        self.strike_pct.iter().fold(1.0, |acc, &m| acc * (1.0 + m))
    }

    /// Total multiplicative condition damage modifier (global).
    pub fn total_condi_mult(&self) -> f64 {
        self.condition_pct.iter().fold(1.0, |acc, &m| acc * (1.0 + m))
    }

    /// Total multiplicative condition damage modifier for a specific condition.
    pub fn total_condi_mult_for(&self, condition: &str) -> f64 {
        let global = self.total_condi_mult();
        let specific = self
            .specific_condi
            .get(condition)
            .map(|v| v.iter().fold(1.0, |acc, &m| acc * (1.0 + m)))
            .unwrap_or(1.0);
        global * specific
    }

    /// Total additive crit damage bonus (percentage points).
    pub fn total_crit_damage_bonus(&self) -> f64 {
        self.crit_damage_pct.iter().sum()
    }

    /// Total additive condition duration bonus from modifiers (percentage points).
    pub fn total_condi_duration_bonus(&self) -> f64 {
        self.condi_duration_pct.iter().sum::<f64>() * 100.0
    }

    /// Total condition duration bonus for a specific condition (percentage points).
    pub fn total_condi_duration_for(&self, condition: &str) -> f64 {
        let global = self.total_condi_duration_bonus();
        let specific = self
            .specific_condi_duration
            .get(condition)
            .map(|v| v.iter().sum::<f64>() * 100.0)
            .unwrap_or(0.0);
        global + specific
    }

    /// Total additive boon duration bonus from modifiers (percentage points).
    pub fn total_boon_duration_bonus(&self) -> f64 {
        self.boon_duration_pct.iter().sum::<f64>() * 100.0
    }

    /// Total multiplicative healing modifier.
    pub fn total_healing_mult(&self) -> f64 {
        self.healing_pct.iter().fold(1.0, |acc, &m| acc * (1.0 + m))
    }
}

/// Per-condition tick damage at level 80.
#[derive(Debug, Clone, Default)]
pub struct ConditionTicks {
    pub bleeding: f64,
    pub burning: f64,
    pub poison: f64,
    pub torment: f64,
    pub confusion: f64,
}

/// Buff scenario for active-tier calculations.
#[derive(Debug, Clone)]
pub struct BuffProfile {
    /// Might stacks (0-25). Each stack = +30 Power, +30 Condition Damage.
    pub might_stacks: u32,
    /// Fury: +25% critical chance (additive).
    pub fury: bool,
    /// Protection: -33% incoming strike damage.
    pub protection: bool,
    /// Resolution: -33% incoming condition damage.
    pub resolution: bool,
    /// Vulnerability stacks on target (0-25). Each stack = +1% damage dealt.
    pub vulnerability_stacks: u32,
    /// Display label for UI.
    pub label: String,
}

/// Complete combat performance metrics for a build under a given buff profile.
#[derive(Debug, Clone, Default)]
pub struct CombatPerformance {
    /// Effective power: Power * crit factor * strike modifiers.
    pub effective_power: f64,
    /// Strike DPS index: effective_power * vuln_mult / target_armor.
    /// Normalized to a reference value for comparison.
    pub strike_dps_index: f64,
    /// Per-condition tick damage values.
    pub condition_ticks: ConditionTicks,
    /// Condition DPS index: weighted sum of condition ticks with duration scaling.
    pub condition_dps_index: f64,
    /// Combined DPS index.
    pub total_dps_index: f64,
    /// Healing power index: healing_power * healing_mult.
    pub healing_power_index: f64,
    /// Total boon duration (capped 100%).
    pub boon_duration_pct: f64,
    /// Total condition duration (capped 100%).
    pub condi_duration_pct: f64,
    /// Effective health: health * armor / reference.
    pub effective_health: f64,
    /// Damage reduction percentage from armor (+ protection if active).
    pub damage_reduction_pct: f64,
}

// ─── Condition Tick Formulas (Level 80) ───

/// Calculate bleeding tick damage: 0.06 * CondDmg + 22
fn bleeding_tick(condition_damage: f64) -> f64 {
    0.06 * condition_damage + 22.0
}

/// Calculate burning tick damage: 0.155 * CondDmg + 131
fn burning_tick(condition_damage: f64) -> f64 {
    0.155 * condition_damage + 131.0
}

/// Calculate poison tick damage: 0.06 * CondDmg + 33.5
fn poison_tick(condition_damage: f64) -> f64 {
    0.06 * condition_damage + 33.5
}

/// Calculate torment tick damage (stationary): 0.06 * CondDmg + 22
fn torment_tick(condition_damage: f64) -> f64 {
    0.06 * condition_damage + 22.0
}

/// Calculate confusion tick damage (on skill use): 0.195 * CondDmg + 95.5
fn confusion_tick(condition_damage: f64) -> f64 {
    0.195 * condition_damage + 95.5
}

/// Calculate all condition tick damage values.
pub fn calculate_condition_ticks(condition_damage: f64, modifiers: &DamageModifiers) -> ConditionTicks {
    ConditionTicks {
        bleeding: bleeding_tick(condition_damage) * modifiers.total_condi_mult_for("Bleeding"),
        burning: burning_tick(condition_damage) * modifiers.total_condi_mult_for("Burning"),
        poison: poison_tick(condition_damage) * modifiers.total_condi_mult_for("Poison"),
        torment: torment_tick(condition_damage) * modifiers.total_condi_mult_for("Torment"),
        confusion: confusion_tick(condition_damage) * modifiers.total_condi_mult_for("Confusion"),
    }
}

// ─── Combat Performance Calculation ───

/// Reference target armor for DPS index calculations (typical raid boss).
const REFERENCE_ARMOR: f64 = 2597.0;
/// Reference weapon strength (Ascended greatsword average).
const REFERENCE_WEAPON_STRENGTH: f64 = 1100.0;

/// Calculate full combat performance metrics for a build.
pub fn calculate_combat_performance(
    stats: &StatBlock,
    _derived: &DerivedStats,
    modifiers: &DamageModifiers,
    buffs: &BuffProfile,
    profession: &str,
) -> CombatPerformance {
    // Apply buff stats
    let might_power = buffs.might_stacks as f64 * 30.0;
    let might_condi = buffs.might_stacks as f64 * 30.0;
    let fury_crit = if buffs.fury { 25.0 } else { 0.0 };

    let total_power = stats.power + might_power;
    let total_precision = stats.precision;
    let total_condition_damage = stats.condition_damage + might_condi;

    // Crit chance: base from precision + fury
    let crit_chance = (((total_precision - 895.0) / 21.0) + fury_crit).clamp(0.0, 100.0);
    // Crit damage: 150% + ferocity/15 + trait bonuses
    let crit_damage = 150.0 + stats.ferocity / 15.0 + modifiers.total_crit_damage_bonus();

    // Effective power with strike modifiers
    let crit_factor = 1.0 + (crit_chance / 100.0) * (crit_damage / 100.0 - 1.0);
    let effective_power = total_power * crit_factor * modifiers.total_strike_mult();

    // Vulnerability multiplier on target
    let vuln_mult = 1.0 + buffs.vulnerability_stacks as f64 * 0.01;

    // Strike DPS index (normalized)
    let strike_dps_index =
        effective_power * vuln_mult * REFERENCE_WEAPON_STRENGTH / REFERENCE_ARMOR;

    // Condition ticks (with modifiers applied)
    let condition_ticks = calculate_condition_ticks(total_condition_damage, modifiers);

    // Condition duration from expertise + modifiers
    let base_condi_duration = (stats.expertise / 15.0).clamp(0.0, 100.0);
    let total_condi_duration = (base_condi_duration + modifiers.total_condi_duration_bonus()).clamp(0.0, 100.0);

    // Condition DPS index: weighted sum of ticks * (1 + duration_bonus) * vuln
    // Weights represent typical condition application rates in a rotation
    let condi_duration_mult = 1.0 + total_condi_duration / 100.0;
    let condition_dps_index = (condition_ticks.bleeding * 3.0  // ~3 stacks average
        + condition_ticks.burning * 2.0   // ~2 stacks average
        + condition_ticks.poison * 1.0
        + condition_ticks.torment * 1.5
        + condition_ticks.confusion * 0.5)
        * condi_duration_mult
        * vuln_mult;

    let total_dps_index = strike_dps_index + condition_dps_index;

    // Healing power index
    let healing_power_index = stats.healing_power * modifiers.total_healing_mult();

    // Boon duration from concentration + modifiers
    let base_boon_duration = (stats.concentration / 15.0).clamp(0.0, 100.0);
    let boon_duration_pct = (base_boon_duration + modifiers.total_boon_duration_bonus()).clamp(0.0, 100.0);

    // Survivability
    let health = stats::base_health(profession) + stats.vitality * 10.0;
    let armor = stats.toughness + 1000.0; // base defense approximation

    // Damage reduction from armor
    let armor_dr = armor / (armor + 2600.0);
    let protection_dr = if buffs.protection { 0.33 } else { 0.0 };
    // Combined: 1 - (1 - armor_dr) * (1 - protection_dr)
    let total_dr = 1.0 - (1.0 - armor_dr) * (1.0 - protection_dr);

    let effective_health = health * armor / REFERENCE_ARMOR * (1.0 / (1.0 - total_dr));

    CombatPerformance {
        effective_power,
        strike_dps_index,
        condition_ticks,
        condition_dps_index,
        total_dps_index,
        healing_power_index,
        boon_duration_pct,
        condi_duration_pct: total_condi_duration,
        effective_health,
        damage_reduction_pct: total_dr * 100.0,
    }
}

// ─── Buff Profiles ───

/// Returns the three standard buff profiles: Solo, Party, Full Squad.
pub fn default_buff_profiles() -> Vec<BuffProfile> {
    vec![
        BuffProfile {
            might_stacks: 0,
            fury: false,
            protection: false,
            resolution: false,
            vulnerability_stacks: 0,
            label: "Solo".into(),
        },
        BuffProfile {
            might_stacks: 15,
            fury: true,
            protection: false,
            resolution: false,
            vulnerability_stacks: 0,
            label: "Party".into(),
        },
        BuffProfile {
            might_stacks: 25,
            fury: true,
            protection: false,
            resolution: false,
            vulnerability_stacks: 25,
            label: "Full Squad".into(),
        },
    ]
}

// ─── Damage Modifier Extraction ───

/// Extract percentage-based damage modifiers from equipped traits, rune, and sigils.
pub fn extract_damage_modifiers(
    equipped_trait_ids: &[u32],
    rune_id: Option<u32>,
    sigil_ids: &[u32],
    relic_id: Option<u32>,
    traits_cache: &HashMap<u32, Trait>,
    items_cache: &HashMap<u32, Item>,
) -> DamageModifiers {
    let mut mods = DamageModifiers::default();

    // 1. Traits — look for Percent facts with damage-related text
    for &trait_id in equipped_trait_ids {
        let Some(t) = traits_cache.get(&trait_id) else {
            continue;
        };

        // Collect overridden indices from active traited_facts
        let overridden: Vec<u32> = t
            .traited_facts
            .iter()
            .filter(|tf| equipped_trait_ids.contains(&tf.requires_trait))
            .filter_map(|tf| tf.overrides)
            .collect();

        // Process base facts
        for (idx, fact) in t.facts.iter().enumerate() {
            if overridden.contains(&(idx as u32)) {
                continue;
            }
            extract_modifier_from_fact(&mut mods, fact);
        }

        // Process active traited_facts
        for tf in &t.traited_facts {
            if equipped_trait_ids.contains(&tf.requires_trait) {
                extract_modifier_from_fact(&mut mods, &tf.fact);
            }
        }
    }

    // 2. Rune bonuses — parse strings like "+7% Burning Duration", "+5% damage"
    if let Some(id) = rune_id {
        if let Some(rune) = items_cache.get(&id) {
            if let Some(ref details) = rune.details {
                for bonus_str in &details.bonuses {
                    parse_rune_modifier(&mut mods, bonus_str);
                }
            }
        }
    }

    // 3. Sigils — known permanent damage sigils
    for &id in sigil_ids {
        if let Some(sigil) = items_cache.get(&id) {
            parse_sigil_modifier(&mut mods, sigil);
        }
    }

    // 4. Relics — parse known relic effects
    if let Some(id) = relic_id {
        if let Some(relic) = items_cache.get(&id) {
            parse_relic_modifier(&mut mods, relic);
        }
    }

    mods
}

/// Extract a damage modifier from a single Fact.
fn extract_modifier_from_fact(mods: &mut DamageModifiers, fact: &Fact) {
    match fact {
        Fact::Percent {
            text: Some(ref text),
            percent: Some(pct),
            ..
        } => {
            let text_lower = text.to_lowercase();
            let decimal = *pct / 100.0;

            if text_lower.contains("damage") && !text_lower.contains("condition damage") {
                // Generic damage increase (applies to strike)
                if decimal.abs() > 0.001 {
                    mods.strike_pct.push(decimal);
                }
            } else if text_lower.contains("condition damage") && text_lower.contains("increase") {
                if decimal.abs() > 0.001 {
                    mods.condition_pct.push(decimal);
                }
            } else if text_lower.contains("critical damage") || text_lower.contains("crit damage") {
                mods.crit_damage_pct.push(*pct); // already in percentage points
            } else if text_lower.contains("condition duration") {
                mods.condi_duration_pct.push(decimal);
            } else if text_lower.contains("boon duration") {
                mods.boon_duration_pct.push(decimal);
            } else if text_lower.contains("outgoing healing") {
                mods.healing_pct.push(decimal);
            }

            // Specific condition duration patterns
            for condi in &["Bleeding", "Burning", "Poison", "Torment", "Confusion"] {
                if text_lower.contains(&condi.to_lowercase())
                    && text_lower.contains("duration")
                {
                    mods.specific_condi_duration
                        .entry(condi.to_string())
                        .or_default()
                        .push(decimal);
                }
            }
        }
        Fact::Buff {
            text: Some(ref text),
            status: Some(ref status),
            ..
        } => {
            // Self-applied damage buffs (e.g. traits that grant Fury, Might)
            // These are transient/conditional, not permanent modifiers.
            // We don't count self-applied boons as permanent modifiers here
            // since they depend on skill usage/uptime.
            let _ = (text, status); // Acknowledge but skip
        }
        _ => {}
    }
}

/// Parse a rune bonus string for percentage modifiers.
/// Examples: "+7% Burning Duration", "+5% damage", "+10% Condition Duration"
fn parse_rune_modifier(mods: &mut DamageModifiers, bonus: &str) {
    let s = bonus.trim().to_lowercase();

    // Match patterns like "+N% <thing>"
    if !s.starts_with('+') {
        return;
    }
    let without_plus = &s[1..];
    let pct_idx = match without_plus.find('%') {
        Some(i) => i,
        None => return,
    };
    let num_str = &without_plus[..pct_idx];
    let rest = without_plus[pct_idx + 1..].trim();

    let Ok(value) = num_str.trim().parse::<f64>() else {
        return;
    };
    let decimal = value / 100.0;

    // Specific condition duration: "+7% Burning Duration"
    for condi in &["bleeding", "burning", "poison", "torment", "confusion"] {
        if rest.contains(condi) && rest.contains("duration") {
            let condi_cap = capitalize(condi);
            mods.specific_condi_duration
                .entry(condi_cap)
                .or_default()
                .push(decimal);
            return;
        }
    }

    // Global condition duration
    if rest.contains("condition duration") {
        mods.condi_duration_pct.push(decimal);
        return;
    }

    // Boon duration
    if rest.contains("boon duration") {
        mods.boon_duration_pct.push(decimal);
        return;
    }

    // Generic damage
    if rest.contains("damage") && !rest.contains("condition") {
        mods.strike_pct.push(decimal);
    }
}

/// Parse known sigil damage modifiers from item data.
fn parse_sigil_modifier(mods: &mut DamageModifiers, sigil: &Item) {
    let name_lower = sigil.name.to_lowercase();

    // Known permanent damage sigils
    if name_lower.contains("sigil of force") {
        // Superior Sigil of Force: +5% damage
        mods.strike_pct.push(0.05);
    } else if name_lower.contains("sigil of impact") {
        // Superior Sigil of Impact: +3% damage on stun (conditional, count at reduced value)
        mods.strike_pct.push(0.015); // ~50% uptime estimate
    } else if name_lower.contains("sigil of the night") {
        // Superior Sigil of the Night: +10% damage at night (conditional)
        mods.strike_pct.push(0.05); // ~50% uptime estimate
    } else if name_lower.contains("sigil of bursting") {
        // Superior Sigil of Bursting: +6% condition damage
        mods.condition_pct.push(0.06);
    } else if name_lower.contains("sigil of malice") {
        // Superior Sigil of Malice: +10% condition duration
        mods.condi_duration_pct.push(0.10);
    } else if name_lower.contains("sigil of concentration") {
        // Superior Sigil of Concentration: +10% boon duration on weapon swap
        mods.boon_duration_pct.push(0.05); // ~50% uptime estimate
    }
    // Sigils with on-crit/on-hit procs are too conditional to model statically
}

/// Parse known relic damage modifiers.
fn parse_relic_modifier(mods: &mut DamageModifiers, relic: &Item) {
    let name_lower = relic.name.to_lowercase();

    // Known relics with permanent or high-uptime damage modifiers
    if name_lower.contains("relic of the thief") {
        // +1% damage per boon on target (estimate ~5 boons avg)
        mods.strike_pct.push(0.05);
    } else if name_lower.contains("relic of fireworks") {
        // +10% damage for 6s after dodge (estimate ~50% uptime)
        mods.strike_pct.push(0.05);
    } else if name_lower.contains("relic of the fractal") {
        // +15% damage in fractals (conditional on content)
        // Skip — too content-specific
    }
    // Most relics have proc-based effects, skip for static model
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_tick_formulas() {
        // With 0 condition damage — base values only
        let mods = DamageModifiers::default();
        let ticks = calculate_condition_ticks(0.0, &mods);
        assert!((ticks.bleeding - 22.0).abs() < 0.1);
        assert!((ticks.burning - 131.0).abs() < 0.1);
        assert!((ticks.poison - 33.5).abs() < 0.1);
        assert!((ticks.torment - 22.0).abs() < 0.1);
        assert!((ticks.confusion - 95.5).abs() < 0.1);
    }

    #[test]
    fn test_condition_tick_with_stats() {
        // With 2000 condition damage (typical Viper build)
        let mods = DamageModifiers::default();
        let ticks = calculate_condition_ticks(2000.0, &mods);
        // Bleeding: 0.06 * 2000 + 22 = 142
        assert!((ticks.bleeding - 142.0).abs() < 0.1);
        // Burning: 0.155 * 2000 + 131 = 441
        assert!((ticks.burning - 441.0).abs() < 0.1);
        // Poison: 0.06 * 2000 + 33.5 = 153.5
        assert!((ticks.poison - 153.5).abs() < 0.1);
        // Torment: 0.06 * 2000 + 22 = 142
        assert!((ticks.torment - 142.0).abs() < 0.1);
        // Confusion: 0.195 * 2000 + 95.5 = 485.5
        assert!((ticks.confusion - 485.5).abs() < 0.1);
    }

    #[test]
    fn test_condition_ticks_with_modifiers() {
        let mut mods = DamageModifiers::default();
        mods.condition_pct.push(0.10); // +10% global condition damage
        mods.specific_condi
            .entry("Burning".into())
            .or_default()
            .push(0.20); // +20% burning damage specifically

        let ticks = calculate_condition_ticks(1000.0, &mods);
        // Bleeding: (0.06 * 1000 + 22) * 1.10 = 82 * 1.10 = 90.2
        assert!((ticks.bleeding - 90.2).abs() < 0.1);
        // Burning: (0.155 * 1000 + 131) * 1.10 * 1.20 = 286 * 1.32 = 377.52
        assert!((ticks.burning - 377.52).abs() < 0.1);
    }

    #[test]
    fn test_damage_modifiers_multiplicative() {
        let mut mods = DamageModifiers::default();
        mods.strike_pct.push(0.05); // +5%
        mods.strike_pct.push(0.10); // +10%
        // Multiplicative: 1.05 * 1.10 = 1.155
        assert!((mods.total_strike_mult() - 1.155).abs() < 0.001);
    }

    #[test]
    fn test_solo_combat_performance() {
        // Berserker build: ~2800 Power, ~2100 Precision, ~1400 Ferocity
        let stats = StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Warrior");
        let mods = DamageModifiers::default();
        let solo = default_buff_profiles().into_iter().find(|b| b.label == "Solo").unwrap();

        let perf = calculate_combat_performance(&stats, &derived, &mods, &solo, "Warrior");

        // Effective power should be > raw power due to crits
        assert!(perf.effective_power > 2800.0);
        // Strike DPS index should be substantial
        assert!(perf.strike_dps_index > 1000.0);
        // Condition DPS should be low (no condition damage)
        assert!(perf.condition_dps_index < perf.strike_dps_index);
        // Total = strike + condition
        assert!((perf.total_dps_index - perf.strike_dps_index - perf.condition_dps_index).abs() < 1.0);
    }

    #[test]
    fn test_squad_buffs_increase_performance() {
        let stats = StatBlock {
            power: 2800.0,
            precision: 2100.0,
            ferocity: 1400.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Warrior");
        let mods = DamageModifiers::default();
        let profiles = default_buff_profiles();
        let solo = profiles.iter().find(|b| b.label == "Solo").unwrap();
        let squad = profiles.iter().find(|b| b.label == "Full Squad").unwrap();

        let perf_solo = calculate_combat_performance(&stats, &derived, &mods, solo, "Warrior");
        let perf_squad = calculate_combat_performance(&stats, &derived, &mods, squad, "Warrior");

        // Squad should have higher DPS (Might + Fury + Vulnerability)
        assert!(perf_squad.strike_dps_index > perf_solo.strike_dps_index);
        assert!(perf_squad.total_dps_index > perf_solo.total_dps_index);
    }

    #[test]
    fn test_condi_build_has_high_condi_dps() {
        // Viper build: high condi damage + expertise
        let stats = StatBlock {
            power: 1800.0,
            precision: 1600.0,
            condition_damage: 2200.0,
            expertise: 600.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Necromancer");
        let mods = DamageModifiers::default();
        let solo = default_buff_profiles().into_iter().find(|b| b.label == "Solo").unwrap();

        let perf = calculate_combat_performance(&stats, &derived, &mods, &solo, "Necromancer");

        // Condition DPS index should be significant
        assert!(perf.condition_dps_index > 500.0);
        // Condi duration should be 40% (600 / 15)
        assert!((perf.condi_duration_pct - 40.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_rune_modifier_burning_duration() {
        let mut mods = DamageModifiers::default();
        parse_rune_modifier(&mut mods, "+7% Burning Duration");
        assert_eq!(mods.specific_condi_duration.get("Burning").unwrap().len(), 1);
        assert!((mods.specific_condi_duration["Burning"][0] - 0.07).abs() < 0.001);
    }

    #[test]
    fn test_parse_rune_modifier_damage() {
        let mut mods = DamageModifiers::default();
        parse_rune_modifier(&mut mods, "+5% damage");
        assert_eq!(mods.strike_pct.len(), 1);
        assert!((mods.strike_pct[0] - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_parse_rune_modifier_condition_duration() {
        let mut mods = DamageModifiers::default();
        parse_rune_modifier(&mut mods, "+10% Condition Duration");
        assert_eq!(mods.condi_duration_pct.len(), 1);
        assert!((mods.condi_duration_pct[0] - 0.10).abs() < 0.001);
    }

    #[test]
    fn test_default_buff_profiles() {
        let profiles = default_buff_profiles();
        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[0].label, "Solo");
        assert_eq!(profiles[0].might_stacks, 0);
        assert_eq!(profiles[1].label, "Party");
        assert_eq!(profiles[1].might_stacks, 15);
        assert!(profiles[1].fury);
        assert_eq!(profiles[2].label, "Full Squad");
        assert_eq!(profiles[2].might_stacks, 25);
        assert_eq!(profiles[2].vulnerability_stacks, 25);
    }

    #[test]
    fn test_protection_increases_survivability() {
        let stats = StatBlock {
            power: 1000.0,
            precision: 1000.0,
            toughness: 1500.0,
            vitality: 1500.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Guardian");
        let mods = DamageModifiers::default();

        let without = BuffProfile {
            might_stacks: 0,
            fury: false,
            protection: false,
            resolution: false,
            vulnerability_stacks: 0,
            label: "No Prot".into(),
        };
        let with = BuffProfile {
            might_stacks: 0,
            fury: false,
            protection: true,
            resolution: false,
            vulnerability_stacks: 0,
            label: "With Prot".into(),
        };

        let perf_without = calculate_combat_performance(&stats, &derived, &mods, &without, "Guardian");
        let perf_with = calculate_combat_performance(&stats, &derived, &mods, &with, "Guardian");

        assert!(perf_with.damage_reduction_pct > perf_without.damage_reduction_pct);
        assert!(perf_with.effective_health > perf_without.effective_health);
    }
}
