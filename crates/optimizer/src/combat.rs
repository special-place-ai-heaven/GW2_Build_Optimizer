//! Combat performance model.
//! Calculates real combat metrics (strike DPS, condition DPS, healing, survivability)
//! using GW2's published formulas. Replaces hardcoded buff assumptions with proper
//! buff profiles and percentage-based damage modifiers from traits/runes/sigils.

use std::collections::HashMap;

use gw2_api::models::{Fact, Item, Trait};

use crate::balance::BalanceContext;
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
        self.condition_pct
            .iter()
            .fold(1.0, |acc, &m| acc * (1.0 + m))
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
/// Boon effect values loaded from data/formulas/boons.json.
///
/// Now constructed from rotation profile data via `buff_profiles_for_profession()`.
pub type BuffProfile = crate::data::BuffProfileFromScenario;

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
    /// Critical hit chance percentage (0-100, clamped; includes Fury).
    pub crit_chance: f64,
    /// Effective health: health * armor / reference.
    pub effective_health: f64,
    /// Damage reduction percentage from Protection boon (0% or 33%).
    pub damage_reduction_pct: f64,
}

// ─── Condition Tick Formulas (Level 80) ───
// Formulas loaded from data/formulas/conditions.json via data::conditions().
// Source: https://wiki.guildwars2.com/wiki/Bleeding, Burning, Poisoned, Torment, Confusion

/// Calculate all condition tick damage values.
///
/// Simple conditions (Bleeding, Burning, Poison) are mode-aware but currently
/// same across all modes. Torment uses stationary as the conservative baseline
/// (movement_fraction weighting is P3-14 scope). Confusion uses on_skill_use
/// as the default (trigger frequency is P3-14 scope).
pub fn calculate_condition_ticks(
    condition_damage: f64,
    modifiers: &DamageModifiers,
    ctx: &BalanceContext,
) -> ConditionTicks {
    let conds = crate::data::conditions();
    let mode = ctx.game_mode.clone();
    ConditionTicks {
        bleeding: conds.tick_damage("Bleeding", condition_damage, mode.clone())
            * modifiers.total_condi_mult_for("Bleeding"),
        burning: conds.tick_damage("Burning", condition_damage, mode.clone())
            * modifiers.total_condi_mult_for("Burning"),
        poison: conds.tick_damage("Poisoned", condition_damage, mode.clone())
            * modifiers.total_condi_mult_for("Poisoned"),
        // Torment: stationary baseline (conservative). Movement weighting is P3-14 scope.
        torment: conds.torment_tick(condition_damage, mode.clone(), false)
            * modifiers.total_condi_mult_for("Torment"),
        // Confusion: on_skill_use baseline (matches historical behavior).
        // Trigger frequency weighting is P3-14 scope.
        confusion: conds.confusion_tick(condition_damage, mode, true)
            * modifiers.total_condi_mult_for("Confusion"),
    }
}

// ─── Condition Stack Weights ───

/// Per-condition stack-count weights for a typical rotation.
/// Now constructed from rotation profile data via `condition_weights_for_profession()`.
pub type ConditionWeights = crate::data::ConditionWeightsFromProfile;

/// Look up condition weights for a profession from rotation profile data.
///
/// Accepts both base profession names (as returned by the GW2 API `profession.name` field,
/// e.g., `"Necromancer"`, `"Guardian"`) and elite specialization names (e.g., `"Scourge"`,
/// `"Firebrand"`, `"Harbinger"`) for forward-compatibility.
///
/// Falls back to generic profile if no profession-specific profile exists.
pub fn condition_weights_for_profession(
    profession: &str,
    ctx: &BalanceContext,
) -> ConditionWeights {
    let data = crate::data::rotation_profiles::rotation_profiles();
    let profile = data.lookup(profession, None, &ctx.game_mode);
    match profile {
        Some(p) => ConditionWeights::from_profile(p),
        None => {
            // Should never happen since we always have a Generic fallback,
            // but provide safe defaults.
            ConditionWeights {
                bleeding: 3.0,
                burning: 2.0,
                poison: 1.0,
                torment: 1.5,
                confusion: 0.5,
            }
        }
    }
}

// ─── Duration Formulas ───
//
// Sources:
// - https://wiki.guildwars2.com/wiki/Expertise (condition duration)
// - https://wiki.guildwars2.com/wiki/Concentration (boon duration)
// - https://wiki.guildwars2.com/wiki/Boon_Duration (cap at 100%)
// - https://wiki.guildwars2.com/wiki/Condition_Duration (cap at 100%)
//
// The divisor 1500 derives from: 15 Expertise = 1 percentage point = 0.01 ratio,
// so 1 Expertise = 1/1500 ratio.
// TODO: read divisor from loaded universal formulas (P3-03) once available

/// Divisor for converting Expertise to condition duration bonus ratio.
/// 15 Expertise = 1% = 0.01, so 1 Expertise = 1/1500.
/// Source: wiki/Expertise — "15 points of Expertise = 1% Condition Duration"
// TODO: derive from loaded `expertise_per_condition_duration_pct` (P3-03)
const EXPERTISE_DIVISOR: f64 = 1500.0;

/// Divisor for converting Concentration to boon duration bonus ratio.
/// 15 Concentration = 1% = 0.01, so 1 Concentration = 1/1500.
/// Source: wiki/Concentration — "15 points of Concentration = 1% Boon Duration"
// TODO: derive from loaded `concentration_per_boon_duration_pct` (P3-03)
const CONCENTRATION_DIVISOR: f64 = 1500.0;

/// Maximum condition duration bonus as a ratio (1.0 = 100% bonus = double duration).
/// Source: wiki/Condition_Duration — "maximum 100%"
// TODO: read from loaded `condition_duration_cap` (P3-03)
const CONDITION_DURATION_CAP: f64 = 1.0;

/// Maximum boon duration bonus as a ratio (1.0 = 100% bonus = double duration).
/// Source: wiki/Boon_Duration — "maximum 100%"
// TODO: read from loaded `boon_duration_cap` (P3-03)
const BOON_DURATION_CAP: f64 = 1.0;

/// Compute the capped condition duration bonus as a ratio (0.0–1.0).
///
/// Formula: `((expertise / 1500) + global_condi_bonus + specific_condi_bonus).min(cap)`
///
/// All modifier inputs are ratios (e.g. 0.10 = 10%). The cap is also a ratio
/// (1.0 = 100% bonus = double base duration).
///
/// Source: https://wiki.guildwars2.com/wiki/Expertise
pub fn condition_duration_bonus(
    expertise: f64,
    global_condi_bonus: f64,
    specific_condi_bonus: f64,
    cap: f64,
    _ctx: &BalanceContext,
) -> f64 {
    ((expertise / EXPERTISE_DIVISOR) + global_condi_bonus + specific_condi_bonus)
        .max(0.0)
        .min(cap)
}

/// Compute the outgoing condition duration after applying the duration bonus.
///
/// Formula: `base_duration * (1.0 + capped_bonus)`
///
/// Source: https://wiki.guildwars2.com/wiki/Expertise
pub fn condition_duration_multiplied(
    base_duration: f64,
    expertise: f64,
    global_condi_bonus: f64,
    specific_condi_bonus: f64,
    cap: f64,
    ctx: &BalanceContext,
) -> f64 {
    let bonus = condition_duration_bonus(
        expertise,
        global_condi_bonus,
        specific_condi_bonus,
        cap,
        ctx,
    );
    base_duration * (1.0 + bonus)
}

/// Compute the capped boon duration bonus as a ratio (0.0–1.0).
///
/// Formula: `((concentration / 1500) + global_boon_bonus).min(cap)`
///
/// All modifier inputs are ratios (e.g. 0.10 = 10%). The cap is also a ratio
/// (1.0 = 100% bonus = double base duration).
///
/// Source: https://wiki.guildwars2.com/wiki/Boon_Duration
pub fn boon_duration_bonus(
    concentration: f64,
    global_boon_bonus: f64,
    cap: f64,
    _ctx: &BalanceContext,
) -> f64 {
    ((concentration / CONCENTRATION_DIVISOR) + global_boon_bonus)
        .max(0.0)
        .min(cap)
}

/// Compute the outgoing boon duration after applying the duration bonus.
///
/// Formula: `base_duration * (1.0 + capped_bonus)`
///
/// Source: https://wiki.guildwars2.com/wiki/Boon_Duration
pub fn boon_duration_multiplied(
    base_duration: f64,
    concentration: f64,
    global_boon_bonus: f64,
    cap: f64,
    ctx: &BalanceContext,
) -> f64 {
    let bonus = boon_duration_bonus(concentration, global_boon_bonus, cap, ctx);
    base_duration * (1.0 + bonus)
}

// ─── Combat Performance Calculation ───

/// Reference weapon strength (Ascended greatsword average).
/// This is an empirical reference baseline, NOT a wiki formula constant.
const REFERENCE_WEAPON_STRENGTH: f64 = 1100.0;

/// Calculate full combat performance metrics for a build.
pub fn calculate_combat_performance(
    stats: &StatBlock,
    _derived: &DerivedStats,
    modifiers: &DamageModifiers,
    buffs: &BuffProfile,
    condition_weights: &ConditionWeights,
    profession: &str,
    ctx: &BalanceContext,
) -> CombatPerformance {
    // Apply buff stats — values loaded from data/formulas/boons.json
    let b = crate::data::boons();
    let might_power = buffs.might_stacks as f64 * b.might_power_per_stack();
    let might_condi = buffs.might_stacks as f64 * b.might_condi_per_stack();
    // Fury crit bonus: mode-dependent (PvE=25%, PvP/WvW=20%).
    // Source: https://wiki.guildwars2.com/wiki/Fury
    // Data stores ratio (0.25/0.20), multiply by 100 for percentage points.
    let fury_crit_bonus = b.fury_crit_bonus(ctx.game_mode.clone()) * 100.0;
    let fury_crit = if buffs.fury { fury_crit_bonus } else { 0.0 };

    let total_power = stats.power + might_power;
    let total_precision = stats.precision;
    let total_condition_damage = stats.condition_damage + might_condi;

    // Crit chance: base from precision + fury
    // Source: https://wiki.guildwars2.com/wiki/Critical_Chance
    let f = crate::data::universal_formulas::formulas();
    let crit_chance = (f.crit_chance(total_precision) + fury_crit).clamp(0.0, 100.0);
    // Crit damage: base + ferocity component + trait bonuses
    // Source: https://wiki.guildwars2.com/wiki/Ferocity
    let crit_damage = f.crit_damage(stats.ferocity) + modifiers.total_crit_damage_bonus();

    // Effective power with strike modifiers
    let crit_factor = 1.0 + (crit_chance / 100.0) * (crit_damage / 100.0 - 1.0);
    let effective_power = total_power * crit_factor * modifiers.total_strike_mult();

    // Vulnerability multiplier on target — loaded from data
    let vuln_mult = 1.0 + buffs.vulnerability_stacks as f64 * b.vulnerability_pct_per_stack();

    // Strike DPS index (normalized)
    let strike_dps_index =
        effective_power * vuln_mult * REFERENCE_WEAPON_STRENGTH / f.tooltip_reference_armor;

    // Condition ticks (with modifiers applied)
    let condition_ticks = calculate_condition_ticks(total_condition_damage, modifiers, ctx);

    // Condition duration from Expertise + modifiers
    // Global condi duration bonus as ratio (e.g. 0.10 for 10%)
    let global_condi_ratio: f64 = modifiers.condi_duration_pct.iter().sum();
    // Overall condition duration bonus (no per-condition specifics) for UI display
    let total_condi_bonus = condition_duration_bonus(
        stats.expertise,
        global_condi_ratio,
        0.0,
        CONDITION_DURATION_CAP,
        ctx,
    );
    // CombatPerformance.condi_duration_pct is percentage points (0-100)
    let total_condi_duration = total_condi_bonus * 100.0;

    // Condition DPS index: weighted sum of ticks * per-condition duration multiplier * vuln
    // Weights represent typical condition application rates in a rotation
    // Per-condition duration multiplier: 1 + capped(expertise_bonus + global + specific)
    let condi_dur_for = |condition: &str| -> f64 {
        let specific: f64 = modifiers
            .specific_condi_duration
            .get(condition)
            .map(|v| v.iter().sum())
            .unwrap_or(0.0);
        1.0 + condition_duration_bonus(
            stats.expertise,
            global_condi_ratio,
            specific,
            CONDITION_DURATION_CAP,
            ctx,
        )
    };
    let bleed_dur = condi_dur_for("Bleeding");
    let burn_dur = condi_dur_for("Burning");
    let poison_dur = condi_dur_for("Poisoned");
    let torment_dur = condi_dur_for("Torment");
    let confuse_dur = condi_dur_for("Confusion");

    let condition_dps_index = (condition_ticks.bleeding * condition_weights.bleeding * bleed_dur
        + condition_ticks.burning * condition_weights.burning * burn_dur
        + condition_ticks.poison * condition_weights.poison * poison_dur
        + condition_ticks.torment * condition_weights.torment * torment_dur
        + condition_ticks.confusion * condition_weights.confusion * confuse_dur)
        * vuln_mult;

    let total_dps_index = strike_dps_index + condition_dps_index;

    // Healing power index
    let healing_power_index = stats.healing_power * modifiers.total_healing_mult();

    // Boon duration from Concentration + modifiers
    let global_boon_ratio: f64 = modifiers.boon_duration_pct.iter().sum();
    let boon_duration_pct = boon_duration_bonus(
        stats.concentration,
        global_boon_ratio,
        BOON_DURATION_CAP,
        ctx,
    ) * 100.0;

    // Survivability
    // Source: https://wiki.guildwars2.com/wiki/Health
    let health = stats::base_health(profession) + stats.vitality * f.vitality_to_health;
    let armor = stats.toughness + stats::base_defense(profession);

    // GW2 damage formula: Damage = Power * coeff * weapon_strength / Armor
    // Armor is a linear divisor — NOT a diminishing-returns DR percentage.
    // Strike EHP = Health * (Armor / Reference_Armor) / (1 - Protection_DR)
    // Condition EHP = Health / (1 - Resolution_DR) — conditions bypass armor entirely.
    // Blended EHP: 65% strike / 35% condition weighting (typical PvE encounter mix).
    // Protection/Resolution: data stores multiplier (0.67), DR = 1 - multiplier
    let protection_dr = if buffs.protection {
        1.0 - b.protection_multiplier()
    } else {
        0.0
    };
    let resolution_dr = if buffs.resolution {
        1.0 - b.resolution_multiplier()
    } else {
        0.0
    };
    let strike_ehp = health * armor / f.tooltip_reference_armor / (1.0 - protection_dr);
    let condition_ehp = health / (1.0 - resolution_dr);
    let effective_health = strike_ehp * 0.65 + condition_ehp * 0.35;
    let blended_dr = protection_dr * 0.65 + resolution_dr * 0.35;

    CombatPerformance {
        effective_power,
        strike_dps_index,
        condition_ticks,
        condition_dps_index,
        total_dps_index,
        healing_power_index,
        boon_duration_pct,
        condi_duration_pct: total_condi_duration,
        crit_chance,
        effective_health,
        damage_reduction_pct: blended_dr * 100.0,
    }
}

// ─── Buff Profiles ───

/// Returns the three standard buff profiles: Solo, Party, Full Squad.
///
/// Now data-driven from rotation profiles. Looks up the profession's rotation profile
/// and converts each scenario into a BuffProfile. Falls back to generic profile.
pub fn default_buff_profiles(ctx: &BalanceContext) -> Vec<BuffProfile> {
    buff_profiles_for_profession("Generic", ctx)
}

/// Returns buff profiles for a specific profession from rotation profile data.
pub fn buff_profiles_for_profession(profession: &str, ctx: &BalanceContext) -> Vec<BuffProfile> {
    let data = crate::data::rotation_profiles::rotation_profiles();
    let profile = data.lookup(profession, None, &ctx.game_mode);
    match profile {
        Some(p) => p
            .scenarios
            .iter()
            .map(|s| BuffProfile::from_scenario(p, s))
            .collect(),
        None => {
            // Should never happen since we always have a Generic fallback.
            // Provide hardcoded safe defaults matching the old behavior.
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
                    protection: true,
                    resolution: false,
                    vulnerability_stacks: 0,
                    label: "Party".into(),
                },
                BuffProfile {
                    might_stacks: 25,
                    fury: true,
                    protection: true,
                    resolution: true,
                    vulnerability_stacks: 25,
                    label: "Full Squad".into(),
                },
            ]
        }
    }
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
    _ctx: &BalanceContext,
) -> DamageModifiers {
    let mut mods = DamageModifiers::default();
    // Hoist into a HashSet once — `equipped_trait_ids` is scanned twice per
    // traited_fact (overridden filter + activation gate) across every trait;
    // O(n) linear scans add up across the ~36-trait hot path.
    let equipped_set: std::collections::HashSet<u32> = equipped_trait_ids.iter().copied().collect();

    // 1. Traits — look for Percent facts with damage-related text
    for &trait_id in equipped_trait_ids {
        let Some(t) = traits_cache.get(&trait_id) else {
            continue;
        };

        // Collect overridden indices from active traited_facts
        let overridden: std::collections::HashSet<u32> = t
            .traited_facts
            .iter()
            .filter(|tf| equipped_set.contains(&tf.requires_trait))
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
            if equipped_set.contains(&tf.requires_trait) {
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

            // Specific condition duration patterns.
            // Search terms stay verb-form (GW2 tooltip text uses "Poison Duration",
            // not "Poisoned Duration"); the map key is canonicalized so reads via
            // `total_condi_duration_for("Poisoned")` and `condi_dur_for("Poisoned")`
            // hit the entry inserted here.
            for condi in &["Bleeding", "Burning", "Poison", "Torment", "Confusion"] {
                if text_lower.contains(&condi.to_lowercase()) && text_lower.contains("duration") {
                    let canonical =
                        crate::data::boon_condition_formulas::canonical_condition_name(condi);
                    mods.specific_condi_duration
                        .entry(canonical.to_string())
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

    // Specific condition duration: "+7% Burning Duration".
    // Canonicalize the key (e.g. "poison" → "Poisoned") so downstream lookups
    // via `total_condi_duration_for("Poisoned")` / `condi_dur_for("Poisoned")`
    // actually hit this entry. Storing the verb-form key dropped rune-sourced
    // specific duration bonuses on read.
    for condi in &["bleeding", "burning", "poison", "torment", "confusion"] {
        if rest.contains(condi) && rest.contains("duration") {
            let condi_cap = capitalize(condi);
            let canonical =
                crate::data::boon_condition_formulas::canonical_condition_name(&condi_cap);
            mods.specific_condi_duration
                .entry(canonical.to_string())
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

    // Known permanent/high-uptime damage sigils
    if name_lower.contains("sigil of force") {
        // Superior Sigil of Force: +5% damage
        mods.strike_pct.push(0.05);
    } else if name_lower.contains("sigil of impact") {
        // Superior Sigil of Impact: +3% damage vs stunned/knocked down
        mods.strike_pct.push(0.015); // ~50% uptime estimate
    } else if name_lower.contains("sigil of the night") {
        // Superior Sigil of the Night: +10% damage at night
        mods.strike_pct.push(0.05); // ~50% uptime estimate
    } else if name_lower.contains("sigil of bursting") {
        // Superior Sigil of Bursting: +6% condition damage
        mods.condition_pct.push(0.06);
    } else if name_lower.contains("sigil of malice") {
        // Superior Sigil of Malice: +10% condition duration
        mods.condi_duration_pct.push(0.10);
    } else if name_lower.contains("sigil of concentration") {
        // Superior Sigil of Concentration: +10% boon duration on weapon swap (33% uptime)
        mods.boon_duration_pct.push(0.033);
    } else if name_lower.contains("sigil of smoldering") {
        // Superior Sigil of Smoldering: +10% Burning duration
        mods.specific_condi_duration
            .entry("Burning".into())
            .or_default()
            .push(0.10);
    } else if name_lower.contains("sigil of earth") {
        // Superior Sigil of Earth: 60% chance to cause Bleeding on crit (proc-based, skip)
    } else if name_lower.contains("sigil of agony") {
        // Superior Sigil of Agony: +10% Torment duration
        mods.specific_condi_duration
            .entry("Torment".into())
            .or_default()
            .push(0.10);
    } else if name_lower.contains("sigil of venom") {
        // Superior Sigil of Venom: +10% Poison duration.
        // Map key uses canonical form to match the read paths.
        mods.specific_condi_duration
            .entry("Poisoned".into())
            .or_default()
            .push(0.10);
    } else if name_lower.contains("sigil of doom") {
        // Superior Sigil of Doom: apply Poison on weapon swap (proc, skip)
    } else if name_lower.contains("sigil of geomancy") {
        // Superior Sigil of Geomancy: Bleeding on weapon swap (proc, skip)
    } else if name_lower.contains("sigil of absorption") {
        // Superior Sigil of Absorption: steal a boon on hit (utility, skip)
    } else if name_lower.contains("sigil of transference") {
        // Superior Sigil of Transference: +10% outgoing healing
        mods.healing_pct.push(0.10);
    } else if name_lower.contains("sigil of benevolence") {
        // Superior Sigil of Benevolence: stacking +1% outgoing healing per kill (estimate ~3%)
        mods.healing_pct.push(0.03);
    } else {
        // Fallback: try parsing from description
        parse_sigil_from_description(mods, sigil);
    }
}

/// Try to extract modifiers from a sigil's description text.
fn parse_sigil_from_description(mods: &mut DamageModifiers, sigil: &Item) {
    let desc = match sigil.description {
        Some(ref d) => d.to_lowercase(),
        None => return,
    };

    // Look for "+N% <condition> duration" patterns. Canonicalize the key so
    // downstream lookups via "Poisoned" hit "poison"-sourced entries.
    for condi in &["bleeding", "burning", "poison", "torment", "confusion"] {
        if let Some(pct) = extract_percent_before(&desc, &format!("{} duration", condi)) {
            let condi_cap = capitalize(condi);
            let canonical =
                crate::data::boon_condition_formulas::canonical_condition_name(&condi_cap);
            mods.specific_condi_duration
                .entry(canonical.to_string())
                .or_default()
                .push(pct / 100.0);
        }
    }

    // "+N% condition duration"
    if let Some(pct) = extract_percent_before(&desc, "condition duration") {
        mods.condi_duration_pct.push(pct / 100.0);
    }
    // "+N% boon duration"
    if let Some(pct) = extract_percent_before(&desc, "boon duration") {
        mods.boon_duration_pct.push(pct / 100.0);
    }
    // "+N% damage" (strike)
    if desc.contains("damage") && !desc.contains("condition damage") {
        if let Some(pct) = extract_percent_before(&desc, "damage") {
            mods.strike_pct.push(pct / 100.0);
        }
    }
}

/// Extract a percentage number associated with `keyword` from text.
/// Picks the `N%` occurrence closest (by char distance) to the keyword,
/// in either direction.
///
/// Examples:
/// - `"10% burning duration"` + `"burning duration"` → `Some(10.0)`
/// - `"increases outgoing healing by 15%"` + `"healing"` → `Some(15.0)`
/// - `"+10% condition duration. +5% boon duration."` + `"boon duration"`
///   → `Some(5.0)` (the closer percent, not the first one).
///
/// Uses char-level iteration to avoid UTF-8 boundary panics.
fn extract_percent_before(text: &str, keyword: &str) -> Option<f64> {
    let chars: Vec<char> = text.chars().collect();
    let keyword_chars: Vec<char> = keyword.chars().collect();
    if keyword_chars.is_empty() || keyword_chars.len() > chars.len() {
        return None;
    }
    // Find the first occurrence of keyword as a char-window in chars.
    let kw_start = (0..=chars.len() - keyword_chars.len())
        .find(|&i| chars[i..i + keyword_chars.len()] == keyword_chars[..])?;
    let kw_end = kw_start + keyword_chars.len();
    // Find the `%` whose distance to the keyword span is minimal.
    let pct_pos = chars
        .iter()
        .enumerate()
        .filter(|(_, &c)| c == '%')
        .map(|(i, _)| {
            let dist = if i < kw_start {
                kw_start - i
            } else if i >= kw_end {
                i - (kw_end - 1)
            } else {
                0
            };
            (dist, i)
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, i)| i)?;
    // Walk backwards from `%` to find the start of the number.
    let start = chars[..pct_pos]
        .iter()
        .rposition(|c| !c.is_ascii_digit() && *c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= pct_pos {
        return None;
    }
    let num: String = chars[start..pct_pos].iter().collect();
    num.parse::<f64>().ok()
}

/// Parse known relic damage modifiers.
fn parse_relic_modifier(mods: &mut DamageModifiers, relic: &Item) {
    let name_lower = relic.name.to_lowercase();

    // Known relics with permanent or high-uptime damage modifiers
    if name_lower.contains("relic of the thief") {
        // +1% damage per boon on target (estimate ~5 boons avg in group)
        mods.strike_pct.push(0.05);
    } else if name_lower.contains("relic of fireworks") {
        // +10% damage for 6s after dodge (estimate ~40% uptime)
        mods.strike_pct.push(0.04);
    } else if name_lower.contains("relic of isgarren") {
        // +10% crit damage while above health threshold (high uptime for DPS)
        mods.crit_damage_pct.push(10.0);
    } else if name_lower.contains("relic of the aristocracy") {
        // +5% damage per ally affected by your boons (support builds, estimate ~15%)
        // Only relevant for boon support
        mods.strike_pct.push(0.05);
    } else if name_lower.contains("relic of cerus") {
        // +1% condition damage per condition on target (estimate ~5 conditions)
        mods.condition_pct.push(0.05);
    } else if name_lower.contains("relic of the nightmare") {
        // +10% condition duration
        mods.condi_duration_pct.push(0.10);
    } else if name_lower.contains("relic of the krait") {
        // Bleeding on skill use (proc-based, skip)
    } else if name_lower.contains("relic of the monk") {
        // +10% outgoing healing
        mods.healing_pct.push(0.10);
    } else if name_lower.contains("relic of karakosa") {
        // +10% outgoing healing while above health threshold
        mods.healing_pct.push(0.08); // ~80% uptime estimate
    } else if name_lower.contains("relic of nourys") {
        // +10% boon duration for 10s after weapon swap (~33% uptime)
        mods.boon_duration_pct.push(0.033);
    } else if name_lower.contains("relic of the fractal") {
        // +15% damage in fractals (content-specific, skip)
    } else {
        // Fallback: try parsing from description
        parse_relic_from_description(mods, relic);
    }
}

/// Try to extract modifiers from a relic's description text.
fn parse_relic_from_description(mods: &mut DamageModifiers, relic: &Item) {
    let desc = match relic.description {
        Some(ref d) => d.to_lowercase(),
        None => return,
    };

    // Check for common patterns
    if desc.contains("outgoing healing") {
        if let Some(pct) = extract_percent_before(&desc, "outgoing healing") {
            mods.healing_pct.push(pct / 100.0);
            return;
        }
    }
    if desc.contains("condition duration") {
        if let Some(pct) = extract_percent_before(&desc, "condition duration") {
            mods.condi_duration_pct.push(pct / 100.0);
            return;
        }
    }
    if desc.contains("boon duration") {
        if let Some(pct) = extract_percent_before(&desc, "boon duration") {
            mods.boon_duration_pct.push(pct / 100.0);
        }
    }
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
    use crate::balance::BalanceContext;

    #[test]
    fn test_condition_tick_formulas() {
        // With 0 condition damage — base values only
        // Formulas from data/formulas/conditions.json (source-of-truth verified)
        let mods = DamageModifiers::default();
        let ctx = BalanceContext::pve();
        let ticks = calculate_condition_ticks(0.0, &mods, &ctx);
        assert!((ticks.bleeding - 22.0).abs() < 0.1);
        // Burning base: 131.0 (L1 verified against wiki)
        assert!((ticks.burning - 131.0).abs() < 0.1);
        assert!((ticks.poison - 33.5).abs() < 0.1);
        // Torment PvE stationary: 0.09*0 + 31.8 = 31.8 (L2 verified)
        assert!((ticks.torment - 31.8).abs() < 0.1);
        // Confusion PvE on-skill-use: 0.0325*0 + 16.24 = 16.24 (L3 verified)
        assert!((ticks.confusion - 16.24).abs() < 0.1);
    }

    #[test]
    fn test_condition_tick_with_stats() {
        // With 2000 condition damage (typical Viper build)
        // Formulas from data/formulas/conditions.json (source-of-truth verified)
        let mods = DamageModifiers::default();
        let ctx = BalanceContext::pve();
        let ticks = calculate_condition_ticks(2000.0, &mods, &ctx);
        // Bleeding: 0.06 * 2000 + 22 = 142
        assert!((ticks.bleeding - 142.0).abs() < 0.1);
        // Burning: 0.155 * 2000 + 131.0 = 441.0 (L1: base=131.0)
        assert!((ticks.burning - 441.0).abs() < 0.1);
        // Poison: 0.06 * 2000 + 33.5 = 153.5
        assert!((ticks.poison - 153.5).abs() < 0.1);
        // Torment PvE stationary: 0.09 * 2000 + 31.8 = 211.8 (L2 verified)
        assert!((ticks.torment - 211.8).abs() < 0.1);
        // Confusion PvE on-skill-use: 0.0325 * 2000 + 16.24 = 81.24 (L3 verified)
        assert!((ticks.confusion - 81.24).abs() < 0.1);
    }

    #[test]
    fn test_condition_ticks_with_modifiers() {
        let mut mods = DamageModifiers::default();
        mods.condition_pct.push(0.10); // +10% global condition damage
        mods.specific_condi
            .entry("Burning".into())
            .or_default()
            .push(0.20); // +20% burning damage specifically

        let ctx = BalanceContext::pve();
        let ticks = calculate_condition_ticks(1000.0, &mods, &ctx);
        // Bleeding: (0.06 * 1000 + 22) * 1.10 = 82 * 1.10 = 90.2
        assert!((ticks.bleeding - 90.2).abs() < 0.1);
        // Burning: (0.155 * 1000 + 131.0) * 1.10 * 1.20 = 286.0 * 1.32 = 377.52
        assert!((ticks.burning - 377.52).abs() < 0.1);
    }

    // ─── Mode dispatch integration tests ───

    #[test]
    fn test_torment_mode_dispatch_in_combat() {
        // PvE vs PvP Torment should differ
        // Source: https://wiki.guildwars2.com/wiki/Torment
        let mods = DamageModifiers::default();
        let ctx_pve = BalanceContext::pve();
        let ctx_pvp = BalanceContext::pvp();
        let ticks_pve = calculate_condition_ticks(1000.0, &mods, &ctx_pve);
        let ticks_pvp = calculate_condition_ticks(1000.0, &mods, &ctx_pvp);
        // PvE stationary: 0.09 * 1000 + 31.8 = 121.8
        assert!((ticks_pve.torment - 121.8).abs() < 0.1);
        // PvP stationary: 0.07 * 1000 + 26.0 = 96.0
        assert!((ticks_pvp.torment - 96.0).abs() < 0.1);
        assert!(
            (ticks_pve.torment - ticks_pvp.torment).abs() > 1.0,
            "Torment should differ between PvE and PvP"
        );
    }

    #[test]
    fn test_confusion_mode_dispatch_in_combat() {
        // PvE vs PvP Confusion on-skill-use should differ
        // Source: https://wiki.guildwars2.com/wiki/Confusion
        let mods = DamageModifiers::default();
        let ctx_pve = BalanceContext::pve();
        let ctx_pvp = BalanceContext::pvp();
        let ticks_pve = calculate_condition_ticks(1000.0, &mods, &ctx_pve);
        let ticks_pvp = calculate_condition_ticks(1000.0, &mods, &ctx_pvp);
        // PvE on-skill-use: 0.0325 * 1000 + 16.24 = 48.74
        assert!((ticks_pve.confusion - 48.74).abs() < 0.1);
        // PvP on-skill-use: 0.0975 * 1000 + 49.5 = 147.0
        assert!((ticks_pvp.confusion - 147.0).abs() < 0.1);
        assert!(
            (ticks_pve.confusion - ticks_pvp.confusion).abs() > 1.0,
            "Confusion should differ between PvE and PvP"
        );
    }

    #[test]
    fn test_combat_performance_uses_loaded_boon_values() {
        // Verify calculate_combat_performance results match loaded data values
        let ctx = BalanceContext::pve();
        let stats = StatBlock {
            power: 2000.0,
            precision: 1500.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Warrior");
        let mods = DamageModifiers::default();
        let buffs = BuffProfile {
            might_stacks: 10,
            fury: true,
            protection: true,
            resolution: true,
            vulnerability_stacks: 10,
            label: "Test".to_string(),
        };
        let perf = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &buffs,
            &ConditionWeights::default_pve(),
            "Warrior",
            &ctx,
        );
        // Fury PvE = 25% crit chance bonus (from data)
        // Might 10 stacks * 30 Power = +300 Power (from data)
        // Vulnerability 10 stacks * 0.01 = +10% (from data)
        // Protection DR = 1.0 - 0.67 = 0.33 (from data)
        assert!(
            perf.effective_power > 2000.0,
            "Fury+Might should boost power"
        );
        assert!(perf.damage_reduction_pct > 0.0, "Protection should give DR");
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
        let ctx = BalanceContext::pve();
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
        let solo = default_buff_profiles(&ctx)
            .into_iter()
            .find(|b| b.label == "Solo")
            .unwrap();

        let perf = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &ConditionWeights::default_pve(),
            "Warrior",
            &ctx,
        );

        // Effective power should be > raw power due to crits
        assert!(perf.effective_power > 2800.0);
        // Strike DPS index should be substantial
        assert!(perf.strike_dps_index > 1000.0);
        // Condition DPS should be low (no condition damage)
        assert!(perf.condition_dps_index < perf.strike_dps_index);
        // Total = strike + condition
        assert!(
            (perf.total_dps_index - perf.strike_dps_index - perf.condition_dps_index).abs() < 1.0
        );
    }

    #[test]
    fn test_squad_buffs_increase_performance() {
        let ctx = BalanceContext::pve();
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
        let profiles = default_buff_profiles(&ctx);
        let solo = profiles.iter().find(|b| b.label == "Solo").unwrap();
        let squad = profiles.iter().find(|b| b.label == "Full Squad").unwrap();

        let perf_solo = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            solo,
            &ConditionWeights::default_pve(),
            "Warrior",
            &ctx,
        );
        let perf_squad = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            squad,
            &ConditionWeights::default_pve(),
            "Warrior",
            &ctx,
        );

        // Squad should have higher DPS (Might + Fury + Vulnerability)
        assert!(perf_squad.strike_dps_index > perf_solo.strike_dps_index);
        assert!(perf_squad.total_dps_index > perf_solo.total_dps_index);
    }

    #[test]
    fn test_condi_build_has_high_condi_dps() {
        let ctx = BalanceContext::pve();
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
        let solo = default_buff_profiles(&ctx)
            .into_iter()
            .find(|b| b.label == "Solo")
            .unwrap();

        let perf = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &ConditionWeights::default_pve(),
            "Necromancer",
            &ctx,
        );

        // Condition DPS index should be significant
        assert!(perf.condition_dps_index > 500.0);
        // Condi duration should be 40% (600 / 15)
        assert!((perf.condi_duration_pct - 40.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_rune_modifier_burning_duration() {
        let mut mods = DamageModifiers::default();
        parse_rune_modifier(&mut mods, "+7% Burning Duration");
        assert_eq!(
            mods.specific_condi_duration.get("Burning").unwrap().len(),
            1
        );
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
    fn test_extract_percent_before_basic() {
        assert_eq!(
            extract_percent_before("10% burning duration", "burning duration"),
            Some(10.0)
        );
        assert_eq!(
            extract_percent_before("+10% condition duration", "condition duration"),
            Some(10.0)
        );
        assert_eq!(
            extract_percent_before("grants 5% damage bonus", "damage"),
            Some(5.0)
        );
        // No match
        assert_eq!(extract_percent_before("no number here", "damage"), None);
        // Keyword not found
        assert_eq!(
            extract_percent_before("10% burning duration", "poison duration"),
            None
        );
    }

    #[test]
    fn test_extract_percent_before_unicode_safe() {
        // Should not panic on non-ASCII characters
        assert_eq!(extract_percent_before("—5% damage", "damage"), Some(5.0));
        assert_eq!(
            extract_percent_before("résumé 10% condition duration", "condition duration"),
            Some(10.0)
        );
    }

    #[test]
    fn test_parse_sigil_description_fallback() {
        let mut mods = DamageModifiers::default();
        let sigil = Item {
            id: 99999,
            name: "Unknown Sigil of Testing".into(),
            item_type: "UpgradeComponent".into(),
            rarity: "Exotic".into(),
            level: 60,
            description: Some("Grants +10% bleeding duration.".into()),
            icon: None,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec![],
            restrictions: vec![],
            details: None,
        };
        parse_sigil_modifier(&mut mods, &sigil);
        assert_eq!(
            mods.specific_condi_duration.get("Bleeding").unwrap().len(),
            1
        );
        assert!((mods.specific_condi_duration["Bleeding"][0] - 0.10).abs() < 0.001);
    }

    #[test]
    fn test_parse_relic_description_fallback() {
        let mut mods = DamageModifiers::default();
        let relic = Item {
            id: 99999,
            name: "Unknown Relic".into(),
            item_type: "Relic".into(),
            rarity: "Legendary".into(),
            level: 80,
            description: Some("Increases outgoing healing by 15%.".into()),
            icon: None,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec![],
            restrictions: vec![],
            details: None,
        };
        parse_relic_modifier(&mut mods, &relic);
        assert_eq!(mods.healing_pct.len(), 1);
        assert!((mods.healing_pct[0] - 0.15).abs() < 0.001);
    }

    #[test]
    fn test_parse_sigil_smoldering() {
        let mut mods = DamageModifiers::default();
        let sigil = Item {
            id: 1,
            name: "Superior Sigil of Smoldering".into(),
            item_type: "UpgradeComponent".into(),
            rarity: "Exotic".into(),
            level: 60,
            description: None,
            icon: None,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec![],
            restrictions: vec![],
            details: None,
        };
        parse_sigil_modifier(&mut mods, &sigil);
        assert_eq!(
            mods.specific_condi_duration.get("Burning").unwrap().len(),
            1
        );
        assert!((mods.specific_condi_duration["Burning"][0] - 0.10).abs() < 0.001);
    }

    #[test]
    fn test_parse_sigil_transference() {
        let mut mods = DamageModifiers::default();
        let sigil = Item {
            id: 1,
            name: "Superior Sigil of Transference".into(),
            item_type: "UpgradeComponent".into(),
            rarity: "Exotic".into(),
            level: 60,
            description: None,
            icon: None,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec![],
            restrictions: vec![],
            details: None,
        };
        parse_sigil_modifier(&mut mods, &sigil);
        assert_eq!(mods.healing_pct.len(), 1);
        assert!((mods.healing_pct[0] - 0.10).abs() < 0.001);
    }

    #[test]
    fn test_parse_relic_isgarren() {
        let mut mods = DamageModifiers::default();
        let relic = Item {
            id: 1,
            name: "Relic of Isgarren".into(),
            item_type: "Relic".into(),
            rarity: "Legendary".into(),
            level: 80,
            description: None,
            icon: None,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec![],
            restrictions: vec![],
            details: None,
        };
        parse_relic_modifier(&mut mods, &relic);
        assert_eq!(mods.crit_damage_pct.len(), 1);
        assert!((mods.crit_damage_pct[0] - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_relic_nightmare() {
        let mut mods = DamageModifiers::default();
        let relic = Item {
            id: 1,
            name: "Relic of the Nightmare".into(),
            item_type: "Relic".into(),
            rarity: "Legendary".into(),
            level: 80,
            description: None,
            icon: None,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec![],
            restrictions: vec![],
            details: None,
        };
        parse_relic_modifier(&mut mods, &relic);
        assert_eq!(mods.condi_duration_pct.len(), 1);
        assert!((mods.condi_duration_pct[0] - 0.10).abs() < 0.001);
    }

    #[test]
    fn test_default_buff_profiles() {
        let ctx = BalanceContext::pve();
        let profiles = default_buff_profiles(&ctx);
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
        let ctx = BalanceContext::pve();
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

        let perf_without = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &without,
            &ConditionWeights::default_pve(),
            "Guardian",
            &ctx,
        );
        let perf_with = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &with,
            &ConditionWeights::default_pve(),
            "Guardian",
            &ctx,
        );

        assert!(perf_with.damage_reduction_pct > perf_without.damage_reduction_pct);
        assert!(perf_with.effective_health > perf_without.effective_health);
    }

    // ─── Profession-Aware Condition Weight Tests (P2-01) ───

    #[test]
    fn test_firebrand_weights_amplify_burning_score() {
        let ctx = BalanceContext::pve();
        // Same stat block: firebrand preset should produce higher condition_dps_index
        // than default_pve when burning tick is dominant (high condition_damage → large burn tick).
        let stats = StatBlock {
            condition_damage: 2000.0,
            expertise: 400.0,
            power: 1000.0,
            precision: 1000.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Guardian");
        let mods = DamageModifiers::default();
        let solo = default_buff_profiles(&ctx)
            .into_iter()
            .find(|b| b.label == "Solo")
            .unwrap();

        let perf_default = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &ConditionWeights::default_pve(),
            "Guardian",
            &ctx,
        );
        let perf_firebrand = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &ConditionWeights::firebrand_group(),
            "Guardian",
            &ctx,
        );

        // Firebrand has burning=8.0 vs default burning=2.0; burning tick (0.155*2000+131.75=441.75)
        // is the largest per-tick value, so firebrand preset should score higher.
        assert!(
            perf_firebrand.condition_dps_index > perf_default.condition_dps_index,
            "firebrand preset (condi={:.1}) should exceed default_pve (condi={:.1}) \
             when burning tick dominates",
            perf_firebrand.condition_dps_index,
            perf_default.condition_dps_index,
        );
        // strike and effective_health are unaffected
        assert!((perf_firebrand.strike_dps_index - perf_default.strike_dps_index).abs() < 0.01);
        assert!((perf_firebrand.effective_health - perf_default.effective_health).abs() < 0.01);
    }

    #[test]
    fn test_necro_weights_amplify_bleeding_torment_score() {
        let ctx = BalanceContext::pve();
        // Same stat block: necro preset should produce higher condition_dps_index
        // than default_pve when bleeding+torment ticks are dominant.
        let stats = StatBlock {
            condition_damage: 2000.0,
            expertise: 400.0,
            power: 1000.0,
            precision: 1000.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Necromancer");
        let mods = DamageModifiers::default();
        let solo = default_buff_profiles(&ctx)
            .into_iter()
            .find(|b| b.label == "Solo")
            .unwrap();

        let perf_default = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &ConditionWeights::default_pve(),
            "Necromancer",
            &ctx,
        );
        let perf_necro = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &ConditionWeights::necro_group(),
            "Necromancer",
            &ctx,
        );

        // Necro: bleeding=8.0 (vs 3.0), torment=6.0 (vs 1.5); these two combined dominate.
        // Tick values: bleeding=142, torment=106.875 vs burning=441.75 (which necro weights at 1.0).
        // 8*142 + 6*106.875 = 1136 + 641.25 = 1777.25 vs default 3*142 + 2*441.75 + 1.5*106.875
        // = 426 + 883.5 + 160.3 = 1469.8 → necro wins.
        assert!(
            perf_necro.condition_dps_index > perf_default.condition_dps_index,
            "necro_group preset (condi={:.1}) should exceed default_pve (condi={:.1}) \
             when bleeding+torment ticks dominate",
            perf_necro.condition_dps_index,
            perf_default.condition_dps_index,
        );
        assert!((perf_necro.strike_dps_index - perf_default.strike_dps_index).abs() < 0.01);
        assert!((perf_necro.effective_health - perf_default.effective_health).abs() < 0.01);
    }

    #[test]
    fn test_condition_weights_for_profession_dispatch() {
        let ctx = BalanceContext::pve();
        // Necromancer: heavy Bleeding + Torment from rotation profile
        let w = condition_weights_for_profession("Necromancer", &ctx);
        assert!(
            w.bleeding > 2.0,
            "Necro should have high Bleeding: {}",
            w.bleeding
        );
        assert!(
            w.torment > 1.0,
            "Necro should have high Torment: {}",
            w.torment
        );

        // Guardian: heavy Burning from rotation profile
        let g = condition_weights_for_profession("Guardian", &ctx);
        assert!(
            g.burning > 2.0,
            "Guardian should have high Burning: {}",
            g.burning
        );
        assert!(
            g.burning > g.bleeding,
            "Guardian Burning should exceed Bleeding"
        );

        // Warrior: moderate condition application from rotation profile
        let dw = condition_weights_for_profession("Warrior", &ctx);
        assert!(
            dw.bleeding > 0.0,
            "Warrior should have some Bleeding: {}",
            dw.bleeding
        );
        assert!(
            dw.burning > 0.0,
            "Warrior should have some Burning: {}",
            dw.burning
        );

        // Unknown profession → generic fallback
        let unk = condition_weights_for_profession("ElementalistVariant", &ctx);
        assert!(
            unk.bleeding > 0.0,
            "Generic should have some Bleeding: {}",
            unk.bleeding
        );
        assert!(
            unk.burning > 0.0,
            "Generic should have some Burning: {}",
            unk.burning
        );

        // All professions return non-empty weights
        for prof in &[
            "Warrior",
            "Guardian",
            "Revenant",
            "Engineer",
            "Ranger",
            "Thief",
            "Elementalist",
            "Mesmer",
            "Necromancer",
        ] {
            let w = condition_weights_for_profession(prof, &ctx);
            let total = w.bleeding + w.burning + w.poison + w.torment + w.confusion;
            assert!(
                total > 0.0,
                "{} should have non-zero total condition weights",
                prof
            );
        }
    }

    #[test]
    fn test_profession_specific_weights_affect_combat() {
        let ctx = BalanceContext::pve();
        // Different professions should produce different condition_dps_index values
        // given identical condition-heavy stats, because their rotation profiles differ.
        let stats = StatBlock {
            condition_damage: 2000.0,
            expertise: 400.0,
            power: 1000.0,
            precision: 1000.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Necromancer");
        let mods = DamageModifiers::default();
        let solo = default_buff_profiles(&ctx)
            .into_iter()
            .find(|b| b.label == "Solo")
            .unwrap();

        let necro_weights = condition_weights_for_profession("Necromancer", &ctx);
        let guardian_weights = condition_weights_for_profession("Guardian", &ctx);

        let necro_result = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &necro_weights,
            "Necromancer",
            &ctx,
        );
        let guardian_result = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &guardian_weights,
            "Guardian",
            &ctx,
        );

        // Necromancer and Guardian have different condition profiles (Bleeding/Torment vs Burning),
        // so their condition_dps_index values should differ.
        assert!(
            (necro_result.condition_dps_index - guardian_result.condition_dps_index).abs() > 0.01,
            "Necromancer (condi={:.1}) and Guardian (condi={:.1}) should produce \
             different condition_dps_index values",
            necro_result.condition_dps_index,
            guardian_result.condition_dps_index,
        );
    }

    #[test]
    fn test_profession_dispatch_affects_condi_score() {
        let ctx = BalanceContext::pve();
        // End-to-end integration: condition_weights_for_profession("Necromancer")
        // dispatches to necro_group() which should produce a higher condition_dps_index
        // than condition_weights_for_profession("Warrior") (-> default_pve()),
        // given a condition-heavy profile (bleeding + torment dominant).
        //
        // This catches any accidental reversion of the dispatch function to always
        // return default_pve(), because the scoring difference is too large for
        // floating-point rounding to close.
        let stats = StatBlock {
            condition_damage: 2000.0,
            expertise: 400.0,
            power: 1000.0,
            precision: 1000.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Necromancer");
        let mods = DamageModifiers::default();
        let solo = default_buff_profiles(&ctx)
            .into_iter()
            .find(|b| b.label == "Solo")
            .unwrap();

        // Dispatch: Necromancer -> necro_group (bleeding=8.0, torment=6.0)
        let necro_weights = condition_weights_for_profession("Necromancer", &ctx);
        let necro_result = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &necro_weights,
            "Necromancer",
            &ctx,
        );
        // Dispatch: Warrior -> default_pve (bleeding=3.0, torment=1.5)
        let warrior_weights = condition_weights_for_profession("Warrior", &ctx);
        let default_result = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &warrior_weights,
            "Warrior",
            &ctx,
        );

        // necro_group scores ~116 weighted units vs default_pve ~39 — gap is definitive.
        assert!(
            necro_result.condition_dps_index > default_result.condition_dps_index,
            "Necromancer dispatch (condi_dps={:.1}) should exceed Warrior dispatch (condi_dps={:.1}) \
             when bleeding+torment ticks are dominant",
            necro_result.condition_dps_index,
            default_result.condition_dps_index,
        );
        // Strike DPS should be identical — condition weights don't affect power damage.
        assert!(
            (necro_result.strike_dps_index - default_result.strike_dps_index).abs() < 0.01,
            "strike_dps_index should be unaffected by condition weight dispatch",
        );
    }

    // ─── Mode-Differentiation Test (P3-02 AC 6) ───

    #[test]
    fn test_fury_crit_bonus_pve_vs_pvp() {
        // PvE Fury grants 25% crit chance, PvP/WvW Fury grants 20%.
        // With Fury active, the same stats should produce higher effective_power
        // in PvE than in PvP due to the 5% crit chance difference.
        let ctx_pve = BalanceContext::pve();
        let ctx_pvp = BalanceContext::pvp();

        let stats = StatBlock {
            power: 2500.0,
            precision: 1500.0,
            ferocity: 1000.0,
            toughness: 1000.0,
            vitality: 1000.0,
            ..Default::default()
        };
        let derived = stats::compute_derived(&stats, "Warrior");
        let mods = DamageModifiers::default();

        // Buff profile with Fury active
        let fury_profile = BuffProfile {
            might_stacks: 0,
            fury: true,
            protection: false,
            resolution: false,
            vulnerability_stacks: 0,
            label: "Fury Only".into(),
        };

        let perf_pve = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &fury_profile,
            &ConditionWeights::default_pve(),
            "Warrior",
            &ctx_pve,
        );
        let perf_pvp = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &fury_profile,
            &ConditionWeights::default_pve(),
            "Warrior",
            &ctx_pvp,
        );

        // PvE should have higher effective power (25% Fury crit vs 20% PvP Fury crit)
        assert!(
            perf_pve.effective_power > perf_pvp.effective_power,
            "PvE effective_power ({:.1}) should exceed PvP ({:.1}) due to Fury crit bonus \
             (25% PvE vs 20% PvP)",
            perf_pve.effective_power,
            perf_pvp.effective_power,
        );

        // Strike DPS index should also be higher in PvE
        assert!(
            perf_pve.strike_dps_index > perf_pvp.strike_dps_index,
            "PvE strike_dps ({:.1}) should exceed PvP ({:.1})",
            perf_pve.strike_dps_index,
            perf_pvp.strike_dps_index,
        );

        // Without Fury, results should be identical across modes
        let no_fury = BuffProfile {
            might_stacks: 0,
            fury: false,
            protection: false,
            resolution: false,
            vulnerability_stacks: 0,
            label: "No Buffs".into(),
        };

        let perf_pve_no = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &no_fury,
            &ConditionWeights::default_pve(),
            "Warrior",
            &ctx_pve,
        );
        let perf_pvp_no = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &no_fury,
            &ConditionWeights::default_pve(),
            "Warrior",
            &ctx_pvp,
        );

        assert!(
            (perf_pve_no.effective_power - perf_pvp_no.effective_power).abs() < 0.01,
            "Without Fury, PvE ({:.1}) and PvP ({:.1}) effective_power should be identical",
            perf_pve_no.effective_power,
            perf_pvp_no.effective_power,
        );
    }

    // ─── Duration Formula Tests (P3-05) ───

    #[test]
    fn test_condition_duration_basic() {
        // 450 Expertise + 20% Burning Duration modifier.
        // bonus = 450/1500 + 0.20 = 0.30 + 0.20 = 0.50
        // result = 3.0 * (1 + 0.50) = 3.0 * 1.50 = 4.5s
        // Source: https://wiki.guildwars2.com/wiki/Expertise
        let ctx = BalanceContext::pve();
        let result =
            condition_duration_multiplied(3.0, 450.0, 0.0, 0.20, CONDITION_DURATION_CAP, &ctx);
        assert!((result - 4.5).abs() < 0.001, "Expected 4.5, got {result}",);
    }

    #[test]
    fn test_boon_duration_basic() {
        // 600 Concentration: bonus = 600/1500 = 0.40
        // result = 5.0 * (1 + 0.40) = 5.0 * 1.40 = 7.0s
        // Source: https://wiki.guildwars2.com/wiki/Boon_Duration
        let ctx = BalanceContext::pve();
        let result = boon_duration_multiplied(5.0, 600.0, 0.0, BOON_DURATION_CAP, &ctx);
        assert!((result - 7.0).abs() < 0.001, "Expected 7.0, got {result}",);
    }

    #[test]
    fn test_condition_duration_cap() {
        // 1800 Expertise + 30% modifier: raw = 1800/1500 + 0.30 = 1.50, capped at 1.0
        // result = 3.0 * (1 + 1.0) = 6.0s
        // Source: https://wiki.guildwars2.com/wiki/Condition_Duration ("maximum 100%")
        let ctx = BalanceContext::pve();
        let result =
            condition_duration_multiplied(3.0, 1800.0, 0.0, 0.30, CONDITION_DURATION_CAP, &ctx);
        assert!(
            (result - 6.0).abs() < 0.001,
            "Expected 6.0 (capped), got {result}",
        );
    }

    #[test]
    fn test_boon_duration_cap() {
        // 2000 Concentration: raw = 2000/1500 = 1.333, capped at 1.0
        // result = 5.0 * (1 + 1.0) = 10.0s
        // Source: https://wiki.guildwars2.com/wiki/Boon_Duration ("maximum 100%")
        let ctx = BalanceContext::pve();
        let result = boon_duration_multiplied(5.0, 2000.0, 0.0, BOON_DURATION_CAP, &ctx);
        assert!(
            (result - 10.0).abs() < 0.001,
            "Expected 10.0 (capped), got {result}",
        );
    }

    #[test]
    fn test_condition_duration_additive_stacking() {
        // global 10% + specific Burning 20% + Expertise 300
        // bonus = 300/1500 + 0.10 + 0.20 = 0.20 + 0.10 + 0.20 = 0.50
        // result = 4.0 * (1 + 0.50) = 4.0 * 1.50 = 6.0s
        // Source: https://wiki.guildwars2.com/wiki/Expertise
        let ctx = BalanceContext::pve();
        let result =
            condition_duration_multiplied(4.0, 300.0, 0.10, 0.20, CONDITION_DURATION_CAP, &ctx);
        assert!((result - 6.0).abs() < 0.001, "Expected 6.0, got {result}",);
    }

    #[test]
    fn test_zero_expertise_zero_modifiers() {
        // 0 Expertise, no modifiers: bonus = 0.0, multiplier = 1.0
        // result = base * 1.0 = base
        // Source: https://wiki.guildwars2.com/wiki/Expertise
        let ctx = BalanceContext::pve();
        let result =
            condition_duration_multiplied(5.0, 0.0, 0.0, 0.0, CONDITION_DURATION_CAP, &ctx);
        assert!((result - 5.0).abs() < 0.001, "Expected 5.0, got {result}",);

        let boon_result = boon_duration_multiplied(5.0, 0.0, 0.0, BOON_DURATION_CAP, &ctx);
        assert!(
            (boon_result - 5.0).abs() < 0.001,
            "Expected 5.0, got {boon_result}",
        );
    }

    #[test]
    fn test_duration_bonus_ratio_values() {
        // Verify condition_duration_bonus and boon_duration_bonus return correct ratios.
        // Source: https://wiki.guildwars2.com/wiki/Expertise, wiki/Concentration
        let ctx = BalanceContext::pve();

        // 750 Expertise, 5% global, 10% specific: 750/1500 + 0.05 + 0.10 = 0.65
        let condi = condition_duration_bonus(750.0, 0.05, 0.10, CONDITION_DURATION_CAP, &ctx);
        assert!((condi - 0.65).abs() < 0.001, "Expected 0.65, got {condi}",);

        // 900 Concentration, 10% global: 900/1500 + 0.10 = 0.60 + 0.10 = 0.70
        let boon = boon_duration_bonus(900.0, 0.10, BOON_DURATION_CAP, &ctx);
        assert!((boon - 0.70).abs() < 0.001, "Expected 0.70, got {boon}",);

        // Capped: 1500 Expertise, 20% global, 10% specific:
        // 1500/1500 + 0.20 + 0.10 = 1.30 -> capped at 1.0
        let capped = condition_duration_bonus(1500.0, 0.20, 0.10, CONDITION_DURATION_CAP, &ctx);
        assert!(
            (capped - 1.0).abs() < 0.001,
            "Expected 1.0 (capped), got {capped}",
        );
    }

    #[test]
    fn test_combat_performance_condi_duration_matches() {
        // Existing test: 600 Expertise, no modifiers → condi_duration_pct ~40.0
        // 600/1500 = 0.40 ratio = 40.0 percentage points
        // Source: https://wiki.guildwars2.com/wiki/Expertise
        let ctx = BalanceContext::pve();
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
        let solo = default_buff_profiles(&ctx)
            .into_iter()
            .find(|b| b.label == "Solo")
            .unwrap();

        let perf = calculate_combat_performance(
            &stats,
            &derived,
            &mods,
            &solo,
            &ConditionWeights::default_pve(),
            "Necromancer",
            &ctx,
        );

        assert!(
            (perf.condi_duration_pct - 40.0).abs() < 0.1,
            "Expected ~40.0 condi_duration_pct, got {}",
            perf.condi_duration_pct,
        );
    }

    #[test]
    fn test_balance_context_accepted() {
        // Verify duration functions compile and run with all BalanceContext variants.
        // This is a signature future-proofing test — the formulas are currently
        // mode-invariant, so results should be identical across modes.
        let pve = BalanceContext::pve();
        let pvp = BalanceContext::pvp();
        let wvw = BalanceContext::wvw();

        let condi_pve = condition_duration_bonus(600.0, 0.10, 0.05, CONDITION_DURATION_CAP, &pve);
        let condi_pvp = condition_duration_bonus(600.0, 0.10, 0.05, CONDITION_DURATION_CAP, &pvp);
        let condi_wvw = condition_duration_bonus(600.0, 0.10, 0.05, CONDITION_DURATION_CAP, &wvw);
        assert!(
            (condi_pve - condi_pvp).abs() < 0.001,
            "PvE and PvP should match (currently mode-invariant)",
        );
        assert!(
            (condi_pve - condi_wvw).abs() < 0.001,
            "PvE and WvW should match (currently mode-invariant)",
        );

        let boon_pve = boon_duration_bonus(600.0, 0.10, BOON_DURATION_CAP, &pve);
        let boon_pvp = boon_duration_bonus(600.0, 0.10, BOON_DURATION_CAP, &pvp);
        let boon_wvw = boon_duration_bonus(600.0, 0.10, BOON_DURATION_CAP, &wvw);
        assert!(
            (boon_pve - boon_pvp).abs() < 0.001,
            "PvE and PvP boon should match (currently mode-invariant)",
        );
        assert!(
            (boon_pve - boon_wvw).abs() < 0.001,
            "PvE and WvW boon should match (currently mode-invariant)",
        );
    }

    // ─── extract_percent_before regression tests ───

    #[test]
    fn extract_percent_before_simple() {
        assert_eq!(
            extract_percent_before("10% burning duration", "burning duration"),
            Some(10.0)
        );
        assert_eq!(extract_percent_before("+7% damage", "damage"), Some(7.0));
    }

    #[test]
    fn extract_percent_before_picks_closest_when_multiple_percents() {
        // Bug regression: previously returned the FIRST `%` in the text, so
        // this case would return 10.0 for `"boon duration"` instead of 5.0.
        let text = "+10% condition duration. +5% boon duration.";
        assert_eq!(extract_percent_before(text, "boon duration"), Some(5.0));
        assert_eq!(
            extract_percent_before(text, "condition duration"),
            Some(10.0)
        );
    }

    #[test]
    fn extract_percent_before_missing_keyword() {
        assert_eq!(extract_percent_before("10% damage", "boon duration"), None);
    }

    #[test]
    fn extract_percent_before_percent_after_keyword() {
        // Real GW2 description form: "increases outgoing healing by 15%" —
        // percent appears AFTER the keyword. The picker is direction-agnostic
        // and returns the closest percent in either direction.
        assert_eq!(
            extract_percent_before("increases outgoing healing by 15%", "healing"),
            Some(15.0),
        );
    }

    #[test]
    fn extract_percent_before_handles_decimals() {
        assert_eq!(
            extract_percent_before("+0.5% burning damage", "burning damage"),
            Some(0.5)
        );
    }
}
