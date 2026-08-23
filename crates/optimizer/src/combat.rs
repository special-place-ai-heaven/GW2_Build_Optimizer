//! Combat performance model.
//! Calculates real combat metrics (strike DPS, condition DPS, healing, survivability)
//! using GW2's published formulas. Replaces hardcoded buff assumptions with proper
//! buff profiles and percentage-based damage modifiers from traits/runes/sigils.

use std::collections::HashMap;

use gw2_api::models::{Fact, Item, Trait};

use crate::balance::BalanceContext;
use crate::stats::{self, DerivedStats, StatBlock};
use crate::text_util::{capitalize, stack_multiplier, strip_gw2_markup};

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
    /// Additive crit chance bonus (percentage points, e.g. 7.0 for +7%).
    pub crit_chance_pct: Vec<f64>,
    /// Bonus strings / facts that had a `%` but matched no known category.
    pub unparsed: Vec<String>,
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

    /// Total additive crit chance bonus (percentage points).
    pub fn total_crit_chance_bonus(&self) -> f64 {
        self.crit_chance_pct.iter().sum()
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
/// Convert an attribute (Expertise / Concentration) into a duration-bonus ratio.
/// `per_pct` is "points per 1%" from `universal.json` (15 → 1500 divisor).
fn duration_ratio_from_attribute(attr: f64, per_pct: f64) -> f64 {
    if per_pct <= 0.0 {
        0.0
    } else {
        attr / 100.0 / per_pct
    }
}

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
    let f = crate::data::universal_formulas::formulas();
    let cap = cap.min(f.condition_duration_cap);
    (duration_ratio_from_attribute(expertise, f.expertise_per_condition_duration_pct)
        + global_condi_bonus
        + specific_condi_bonus)
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
    let f = crate::data::universal_formulas::formulas();
    let cap = cap.min(f.boon_duration_cap);
    (duration_ratio_from_attribute(concentration, f.concentration_per_boon_duration_pct)
        + global_boon_bonus)
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
    let crit_chance =
        (f.crit_chance(total_precision) + fury_crit + modifiers.total_crit_chance_bonus())
            .clamp(0.0, 100.0);
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
    let total_condi_bonus = condition_duration_bonus(
        stats.expertise,
        global_condi_ratio,
        0.0,
        f.condition_duration_cap,
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
            f.condition_duration_cap,
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
        f.boon_duration_cap,
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

pub fn extract_damage_modifiers(
    equipped_trait_ids: &[u32],
    rune_id: Option<u32>,
    sigil_ids: &[u32],
    relic_id: Option<u32>,
    traits_cache: &HashMap<u32, Trait>,
    items_cache: &HashMap<u32, Item>,
    ctx: &BalanceContext,
) -> DamageModifiers {
    fn absorb_pair(dst: &mut Vec<f64>, src: Vec<f64>, competitive: bool) {
        match src.as_slice() {
            [] => {}
            [a, b] => dst.push(if competitive { a.min(*b) } else { a.max(*b) }),
            _ => dst.extend(src),
        }
    }
    fn absorb_map(
        dst: &mut HashMap<String, Vec<f64>>,
        src: HashMap<String, Vec<f64>>,
        competitive: bool,
    ) {
        for (key, vals) in src {
            absorb_pair(dst.entry(key).or_default(), vals, competitive);
        }
    }
    fn absorb_mode_pairs(dst: &mut DamageModifiers, src: DamageModifiers, competitive: bool) {
        absorb_pair(&mut dst.strike_pct, src.strike_pct, competitive);
        absorb_pair(&mut dst.condition_pct, src.condition_pct, competitive);
        absorb_pair(&mut dst.crit_damage_pct, src.crit_damage_pct, competitive);
        absorb_pair(&mut dst.condi_duration_pct, src.condi_duration_pct, competitive);
        absorb_pair(&mut dst.boon_duration_pct, src.boon_duration_pct, competitive);
        absorb_pair(&mut dst.healing_pct, src.healing_pct, competitive);
        absorb_pair(&mut dst.crit_chance_pct, src.crit_chance_pct, competitive);
        absorb_map(&mut dst.specific_condi, src.specific_condi, competitive);
        absorb_map(
            &mut dst.specific_condi_duration,
            src.specific_condi_duration,
            competitive,
        );
        dst.unparsed.extend(src.unparsed);
    }

    let mut mods = DamageModifiers::default();
    let equipped_set: std::collections::HashSet<u32> = equipped_trait_ids.iter().copied().collect();
    let competitive = matches!(
        ctx.game_mode,
        gw2_core::types::GameMode::PvP | gw2_core::types::GameMode::WvW
    );

    for &trait_id in equipped_trait_ids {
        let Some(t) = traits_cache.get(&trait_id) else {
            continue;
        };

        let overridden: std::collections::HashSet<u32> = t
            .traited_facts
            .iter()
            .filter(|tf| equipped_set.contains(&tf.requires_trait))
            .filter_map(|tf| tf.overrides)
            .collect();

        let mut trait_mods = DamageModifiers::default();
        for (idx, fact) in t.facts.iter().enumerate() {
            if overridden.contains(&(idx as u32)) {
                continue;
            }
            extract_modifier_from_fact(&mut trait_mods, fact);
        }
        for tf in &t.traited_facts {
            if equipped_set.contains(&tf.requires_trait) {
                extract_modifier_from_fact(&mut trait_mods, &tf.fact);
            }
        }
        // ponytail: pair = API PvE/competitive split; 3+ values stay summed
        absorb_mode_pairs(&mut mods, trait_mods, competitive);
    }

    if let Some(id) = rune_id {
        if let Some(rune) = items_cache.get(&id) {
            if let Some(ref details) = rune.details {
                for bonus_str in &details.bonuses {
                    parse_rune_modifier(&mut mods, bonus_str);
                }
            }
        }
    }

    for &id in sigil_ids {
        if let Some(sigil) = items_cache.get(&id) {
            parse_sigil_modifier(&mut mods, sigil, ctx);
        }
    }

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
            if percent_text_is_conditional(text) && !(text_lower_has_90hp(text)) {
                return;
            }
            let text_lower = text.to_lowercase();
            let uptime = if text_lower_has_90hp(text) { 0.9 } else { 1.0 };
            let decimal = *pct / 100.0 * uptime;
            let applied = apply_percent_category(mods, &text_lower, *pct * uptime, decimal);
            if !applied {
                mods.unparsed.push(text.clone());
            }
        }
        Fact::Buff {
            text: Some(ref text),
            status: Some(ref status),
            ..
        } => {
            // Self-applied damage buffs (e.g. traits that grant Fury, Might)
            // are uptime-dependent, not permanent modifiers.
            let _ = (text, status);
        }
        _ => {}
    }
}

fn text_lower_has_90hp(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("90") && t.contains("health")
}

pub(crate) fn percent_text_is_conditional(text: &str) -> bool {
    let t = text.to_lowercase();
    // Scholar-style "while above 90% health" is high-uptime, not a skip.
    if t.contains("90") && t.contains("health") {
        return false;
    }
    [
        "while ",
        "when ",
        "after ",
        "if ",
        "below",
        "above",
        "on critical",
        "on crit",
        "chance to",
        "for each",
        "per ally",
        "per foe",
        "disabled",
        "downed",
    ]
    .iter()
    .any(|k| t.contains(k))
}

/// Shared standing-percent classification used by combat and synergy.
/// `Ignore` is a known non-outgoing tooltip (consumed, not unparsed).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PercentClass {
    CritChancePts,
    CritDamagePts,
    Condition,
    Healing,
    CondiDuration,
    BoonDuration,
    SpecificCondiDuration(String),
    Strike,
    Ignore,
}

/// Map tooltip text + raw percent into one standing-combat bucket.
/// Conditional gating stays in the callers (`percent_text_is_conditional`).
pub(crate) fn classify_percent_text(text: &str, percent: f64) -> Option<PercentClass> {
    let hay = text.to_lowercase();
    let is_crit_chance = hay.contains("critical chance")
        || hay.contains("crit chance")
        || hay.contains("critical-strike chance")
        || hay.contains("critical-hit chance")
        || hay.contains("critical strike chance");
    if is_crit_chance {
        // Precise Strike / Hidden Killer / Burst Precision: 100% is a guaranteed
        // first hit, not +100 standing crit-chance points.
        return Some(if percent >= 100.0 {
            PercentClass::Ignore
        } else {
            PercentClass::CritChancePts
        });
    }
    if hay.contains("critical damage") || hay.contains("crit damage") {
        return Some(PercentClass::CritDamagePts);
    }
    if hay.contains("recharge")
        || hay.contains("reduced")
        || hay.contains("reduction")
        || hay.contains("incoming")
    {
        return Some(PercentClass::Ignore);
    }
    if hay.trim() == "percent" {
        return Some(PercentClass::Ignore);
    }
    if hay.contains("condition damage") {
        return Some(PercentClass::Condition);
    }
    if hay.contains("outgoing healing") || hay.contains("healing effectiveness") {
        return Some(PercentClass::Healing);
    }
    if hay.contains("condition duration") {
        return Some(PercentClass::CondiDuration);
    }
    if hay.contains("boon duration") {
        return Some(PercentClass::BoonDuration);
    }
    for boon in &[
        "might",
        "fury",
        "swiftness",
        "protection",
        "quickness",
        "alacrity",
        "regeneration",
        "vigor",
        "resistance",
        "resolution",
        "aegis",
        "stability",
    ] {
        if hay.contains(boon) && hay.contains("duration") {
            return Some(PercentClass::BoonDuration);
        }
    }
    for condi in &["bleeding", "burning", "poison", "torment", "confusion"] {
        if hay.contains(condi) && hay.contains("duration") {
            let condi_cap = capitalize(condi);
            let canonical =
                crate::data::boon_condition_formulas::canonical_condition_name(&condi_cap);
            return Some(PercentClass::SpecificCondiDuration(canonical.to_string()));
        }
    }
    if hay.contains("strike damage") || (hay.contains("damage") && !hay.contains("condition")) {
        return Some(PercentClass::Strike);
    }
    None
}

fn apply_percent_category(
    mods: &mut DamageModifiers,
    hay: &str,
    points: f64,
    decimal: f64,
) -> bool {
    match classify_percent_text(hay, points) {
        Some(PercentClass::Ignore) => true,
        Some(PercentClass::CritChancePts) => {
            mods.crit_chance_pct.push(points);
            true
        }
        Some(PercentClass::CritDamagePts) => {
            mods.crit_damage_pct.push(points);
            true
        }
        Some(PercentClass::Condition) => {
            if decimal.abs() > 0.001 {
                mods.condition_pct.push(decimal);
            }
            true
        }
        Some(PercentClass::Healing) => {
            mods.healing_pct.push(decimal);
            true
        }
        Some(PercentClass::CondiDuration) => {
            mods.condi_duration_pct.push(decimal);
            true
        }
        Some(PercentClass::BoonDuration) => {
            mods.boon_duration_pct.push(decimal);
            true
        }
        Some(PercentClass::SpecificCondiDuration(key)) => {
            mods.specific_condi_duration
                .entry(key)
                .or_default()
                .push(decimal);
            true
        }
        Some(PercentClass::Strike) => {
            if decimal.abs() > 0.001 {
                mods.strike_pct.push(decimal);
            }
            true
        }
        None => false,
    }
}

fn percent_is_vs_target(rest_after_percent: &str) -> bool {
    let head: String = rest_after_percent.chars().take(48).collect();
    let t = head.to_lowercase();
    if foe_cc_trigger(&t) {
        return false;
    }
    t.contains("vs.") || t.contains("versus") || t.contains(" against ")
}

fn percent_text_is_ignored(hay: &str) -> bool {
    hay.contains("movement")
        || hay.contains("speed")
        || hay.contains("endurance")
        || hay.contains("experience")
        || hay.contains("incoming")
        || hay.contains("stun duration")
        || hay.contains("daze duration")
        || hay.contains("chill duration")
        || hay.contains("cripple duration")
        || hay.contains("weakness duration")
}

/// Test-only shim for the cross-parser consistency suite.
///
/// Runs [`extract_modifier_from_fact`] against a fresh [`DamageModifiers`] and
/// collapses the result into a comparable [`FactClass`] so it can be asserted
/// equal to `synergy::tests_consistency_shim::classify_fact`. Both parsers must
/// agree on what a given modifier `Fact` *means*; this shim exposes the private
/// combat parser to the consistency test module without widening its public API.
#[cfg(test)]
pub(crate) mod tests_consistency_shim {
    use super::{extract_modifier_from_fact, DamageModifiers};
    use crate::parser_consistency_tests::FactClass;
    use gw2_api::models::Fact;

    pub(crate) fn classify_fact(fact: &Fact) -> Vec<FactClass> {
        let mut mods = DamageModifiers::default();
        extract_modifier_from_fact(&mut mods, fact);
        classify_mods(&mods)
    }

    /// Map a populated `DamageModifiers` to the set of classifications it
    /// implies. Each non-empty field contributes one `FactClass`; ordering is
    /// normalized by the caller before comparison.
    fn classify_mods(mods: &DamageModifiers) -> Vec<FactClass> {
        let mut out = Vec::new();
        if !mods.strike_pct.is_empty() {
            out.push(FactClass::Strike);
        }
        if !mods.condition_pct.is_empty() {
            out.push(FactClass::ConditionDamage);
        }
        if !mods.crit_damage_pct.is_empty() {
            out.push(FactClass::Crit);
        }
        if !mods.healing_pct.is_empty() {
            out.push(FactClass::Healing);
        }
        if !mods.condi_duration_pct.is_empty() {
            out.push(FactClass::AllConditionDuration);
        }
        if !mods.boon_duration_pct.is_empty() {
            out.push(FactClass::AllBoonDuration);
        }
        for key in mods.specific_condi_duration.keys() {
            out.push(FactClass::SpecificConditionDuration(key.clone()));
        }
        for key in mods.specific_condi.keys() {
            out.push(FactClass::SpecificConditionDamage(key.clone()));
        }
        out
    }
}

/// Parse a rune bonus string for percentage modifiers.
/// Accepts `+N%`, `-N%`, or `N%` (API strings are not always prefixed).
fn parse_rune_modifier(mods: &mut DamageModifiers, bonus: &str) {
    parse_percent_clauses(mods, bonus);
}

/// Walk every `N%` in `text` and map it to a modifier category.
pub(crate) fn parse_percent_clauses(mods: &mut DamageModifiers, text: &str) -> bool {
    let s = strip_gw2_markup(text).trim().to_lowercase();
    let stacks = stack_multiplier(&s);
    let mut from = 0;
    let mut any = false;
    while let Some(rel) = s[from..].find('%') {
        let pct_idx = from + rel;
        let is_num_part = |c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == '−';
        let num_start = s[..pct_idx]
            .char_indices()
            .rev()
            .find(|(_, c)| !is_num_part(*c))
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        if num_start >= pct_idx {
            from = pct_idx + 1;
            continue;
        }
        let num = s[num_start..pct_idx].replace('−', "-");
        let Ok(value) = num.parse::<f64>() else {
            from = pct_idx + 1;
            continue;
        };
        let rest = s[pct_idx + 1..].trim();
        let before = s[..num_start].trim();
        let hay = format!("{before} {rest}");
        if percent_text_is_ignored(&hay) {
            from = pct_idx + 1;
            continue;
        }
        if percent_is_vs_target(rest) {
            from = pct_idx + 1;
            continue;
        }
        if upgrade_unreliable(&s) || upgrade_unreliable(&hay) {
            from = pct_idx + 1;
            continue;
        }
        if percent_text_is_conditional(&hay)
            && stacks <= 1.0
            && !text_lower_has_90hp(&hay)
            && !easy_rotation_trigger(&hay)
        {
            from = pct_idx + 1;
            continue;
        }
        let stacks = if upgrade_unreliable(&s) { 0.0 } else { stacks };
        let uptime = upgrade_uptime(&s);
        let scaled = value * stacks * uptime;
        let decimal = scaled / 100.0;
        if apply_percent_category(mods, &hay, scaled, decimal) {
            any = true;
        } else {
            mods.unparsed.push(text.to_string());
        }
        from = pct_idx + 1;
    }
    any
}

/// Parse API upgrade prose (rune bonus, sigil buff, relic description) into mods.
/// Percents first; numberless "increased strike damage" text uses expected-value uptime.
pub(crate) fn apply_upgrade_text(mods: &mut DamageModifiers, text: &str) {
    let stripped = strip_gw2_markup(text);
    if parse_percent_clauses(mods, &stripped) {
        return;
    }
    apply_upgrade_prose(mods, &stripped);
}

/// On-kill and death-reset stacks cannot carry a build. Do not match `kill`
/// inside `skill`.
pub(crate) fn upgrade_unreliable(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("on kill")
        || t.contains("when you kill")
        || t.contains("kill a foe")
        || t.contains("kill an enemy")
        || t.contains("killing a")
        || t.contains("each kill")
        || t.contains("per kill")
        || t.contains("until you die")
        || t.contains("when you die")
        || t.contains("lost on")
        || t.contains("downed")
}

fn upgrade_is_triggered(t: &str) -> bool {
    t.contains("after ")
        || t.contains("when ")
        || t.contains("upon ")
        || t.contains("on using")
        || t.contains("on crit")
        || t.contains("on dodge")
        || t.contains("on evade")
        || t.contains("on weapon swap")
        || t.contains("weapon swap")
        || t.contains("while ")
}

/// Dodge, weapon swap, elite, hitting a CC'd foe, and weapon skills are rotation-normal.
fn easy_rotation_trigger(t: &str) -> bool {
    t.contains("evade")
        || t.contains("dodge")
        || t.contains("elite")
        || t.contains("weapon swap")
        || t.contains("swap weapon")
        || (t.contains("swap") && (t.contains("after") || t.contains("when") || t.contains("on ")))
        || foe_cc_trigger(t)
        || t.contains("weapon skill")
        || t.contains("resource cost")
        || t.contains("critical")
        || t.contains("on crit")
        || t.contains("when you hit")
        || t.contains("after hitting")
        || t.contains("when you strike")
        || t.contains("grant a boon")
        || t.contains("apply a boon")
}

pub(crate) fn foe_cc_trigger(t: &str) -> bool {
    t.contains("stun")
        || t.contains("daze")
        || t.contains("knock")
        || t.contains("launch")
        || t.contains("float")
        || t.contains("sink")
        || t.contains("fear")
        || t.contains("taunt")
        || t.contains("immobil")
        || t.contains("disable")
}

const UPGRADE_BUFF_S: f64 = 6.0;

/// Seconds printed next to "recharge" (Fireworks: weapon skill recharge ≥20s).
fn recharge_seconds(t: &str) -> Option<f64> {
    let idx = t.find("recharge")?;
    let start = idx.saturating_sub(48);
    let end = (idx + 48).min(t.len());
    let window = &t[start..end];
    let mut best: Option<f64> = None;
    let mut i = 0;
    let bytes = window.as_bytes();
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let rest = &window[i..];
            let num: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(n) = num.parse::<f64>() {
                if (8.0..=180.0).contains(&n) {
                    best = Some(best.map_or(n, |b| b.max(n)));
                }
            }
            i += num.len().max(1);
        } else {
            i += 1;
        }
    }
    best
}

/// How much a build can rely on this upgrade: passive / rotation / long CD / kill luck.
pub(crate) fn upgrade_rely_label(text: &str) -> &'static str {
    if upgrade_unreliable(text) {
        return "unreliable";
    }
    let t = text.to_lowercase();
    if t.contains("elite") {
        return "elite_cd";
    }
    if recharge_seconds(&t).is_some_and(|n| n >= 15.0) {
        return "long_recharge";
    }
    if upgrade_is_triggered(&t) {
        return "rotation";
    }
    "passive"
}

fn buff_duration_s(t: &str) -> f64 {
    let mut from = 0;
    while let Some(rel) = t[from..].find("for ") {
        let i = from + rel + 4;
        let rest = &t[i..];
        let num: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Ok(n) = num.parse::<f64>() {
            let after = rest[num.len()..].trim_start();
            if after.starts_with("second")
                || after.starts_with("sec")
                || (after.starts_with('s') && !after.starts_with("stack"))
            {
                return n.clamp(1.0, 30.0);
            }
        }
        from = i.max(from + 1);
    }
    UPGRADE_BUFF_S
}

fn upgrade_uptime(text: &str) -> f64 {
    let t = text.to_lowercase();
    if text_lower_has_90hp(&t) {
        return 0.9;
    }
    if !upgrade_is_triggered(&t) {
        return 1.0;
    }
    if stack_multiplier(&t) > 1.0 && !upgrade_unreliable(&t) {
        return 1.0;
    }
    let dur = buff_duration_s(&t);
    // Elite is press-anytime but a long CD (30–90s). 40s ≈ on-CD with some Alacrity.
    // Long weapon-skill recharge (Fireworks live text) must beat the generic 5s skill period.
    let period = if t.contains("elite") {
        40.0
    } else if let Some(cd) = recharge_seconds(&t).filter(|n| *n >= 15.0) {
        cd
    } else if t.contains("evade") || t.contains("dodge") {
        8.0
    } else if t.contains("weapon swap") || t.contains("swap weapon") || t.contains("swap") {
        9.0
    } else if foe_cc_trigger(&t) {
        15.0
    } else if t.contains("weapon skill") || t.contains("resource cost") {
        5.0
    } else if t.contains("critical") || t.contains("on crit") {
        4.0
    } else if t.contains("when you hit")
        || t.contains("after hitting")
        || t.contains("when you strike")
    {
        5.0
    } else {
        8.0
    };
    (dur / period).clamp(0.0, 1.0)
}

fn apply_upgrade_prose(mods: &mut DamageModifiers, text: &str) {
    let t = text.to_lowercase();
    if t.contains('%') {
        return;
    }
    if upgrade_unreliable(&t) {
        return;
    }
    let uptime = upgrade_uptime(&t);
    if t.contains("increased strike")
        || t.contains("deal increased strike")
        || (t.contains("strike damage") && t.contains("increased"))
    {
        mods.strike_pct.push(0.07 * uptime);
        return;
    }
    if t.contains("increased condition duration") {
        mods.condi_duration_pct.push(0.10 * uptime);
        return;
    }
    if t.contains("healing effectiveness") || t.contains("increase healing") {
        mods.healing_pct.push(0.10 * uptime);
        return;
    }
    if t.contains("critical-strike chance")
        || t.contains("critical-hit chance")
        || t.contains("guaranteed critical")
        || (t.contains("critical") && t.contains("chance") && t.contains("increased"))
    {
        mods.crit_chance_pct.push(7.0 * uptime);
        return;
    }
    if t.contains("increased damage") && !t.contains("condition") {
        mods.strike_pct.push(0.05 * uptime);
        mods.condition_pct.push(0.05 * uptime);
    }
}

pub(crate) fn item_buff_description(item: &Item) -> Option<&str> {
    item.details
        .as_ref()
        .and_then(|d| d.infix_upgrade.as_ref())
        .and_then(|iu| iu.buff.as_ref())
        .and_then(|b| b.description.as_deref())
        .filter(|s| !s.is_empty())
}

/// Parse known sigil damage modifiers from item data.
fn parse_sigil_modifier(mods: &mut DamageModifiers, sigil: &Item, ctx: &BalanceContext) {
    if let Some(buff) = item_buff_description(sigil) {
        apply_upgrade_text(mods, buff);
        if !mods.strike_pct.is_empty()
            || !mods.condition_pct.is_empty()
            || !mods.condi_duration_pct.is_empty()
            || !mods.boon_duration_pct.is_empty()
            || !mods.healing_pct.is_empty()
            || !mods.crit_chance_pct.is_empty()
            || !mods.crit_damage_pct.is_empty()
            || !mods.specific_condi.is_empty()
            || !mods.specific_condi_duration.is_empty()
        {
            return;
        }
    }

    let name_lower = sigil.name.to_lowercase();
    let competitive = matches!(
        ctx.game_mode,
        gw2_core::types::GameMode::PvP | gw2_core::types::GameMode::WvW
    );

    if name_lower.contains("sigil of force") {
        mods.strike_pct.push(if competitive { 0.03 } else { 0.05 });
    } else if name_lower.contains("sigil of bursting") {
        mods.condition_pct
            .push(if competitive { 0.04 } else { 0.06 });
    } else if let Some(desc) = sigil.description.as_deref() {
        apply_upgrade_text(mods, desc);
    }
}

/// Parse relic modifiers from the API description (always present on live relics).
fn parse_relic_modifier(mods: &mut DamageModifiers, relic: &Item) {
    if let Some(desc) = relic.description.as_deref() {
        if !desc.is_empty() {
            apply_upgrade_text(mods, desc);
            return;
        }
    }
    if let Some(buff) = item_buff_description(relic) {
        apply_upgrade_text(mods, buff);
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
    fn test_parse_rune_modifier_outgoing_healing() {
        // Rune of the Monk delivers "+10% Outgoing Healing" as a bonus string.
        // Runes have no facts, so this is the only path — it must not be dropped.
        let mut mods = DamageModifiers::default();
        parse_rune_modifier(&mut mods, "+10% Outgoing Healing");
        assert_eq!(mods.healing_pct.len(), 1);
        assert!((mods.healing_pct[0] - 0.10).abs() < 0.001);
        assert!(mods.strike_pct.is_empty());
    }

    #[test]
    fn test_parse_rune_modifier_condition_damage() {
        // Flat "+X% Condition Damage" must land in condition_pct, not be dropped
        // by the generic-damage branch (which excludes "condition") and not be
        // mis-routed to strike_pct.
        let mut mods = DamageModifiers::default();
        parse_rune_modifier(&mut mods, "+5% Condition Damage");
        assert_eq!(mods.condition_pct.len(), 1);
        assert!((mods.condition_pct[0] - 0.05).abs() < 0.001);
        assert!(mods.strike_pct.is_empty());
    }

    #[test]
    fn test_parse_rune_modifier_condition_duration_not_treated_as_damage() {
        // Guard the branch ordering: "Condition Duration" contains neither a
        // damage keyword we want here; it must route to condi_duration_pct only.
        let mut mods = DamageModifiers::default();
        parse_rune_modifier(&mut mods, "+15% Condition Duration");
        assert_eq!(mods.condi_duration_pct.len(), 1);
        assert!(mods.condition_pct.is_empty());
        assert!(mods.strike_pct.is_empty());
    }

    #[test]
    fn test_extract_damage_modifiers_multi_bonus_rune_credits_all() {
        // Integration-style guard: feed a single REALISTIC multi-bonus rune item
        // through the top-level entry point `extract_damage_modifiers` (rune path
        // only — no traits/sigils/relic) and assert that EVERY modifier kind on
        // it is credited SIMULTANEOUSLY. Single-modifier rune tests pass even if a
        // *different* bonus on the same item is silently dropped during the loop
        // over `details.bonuses`; this asserts the whole multi-tier set survives
        // one pass. Noise (flat stats / non-modifier flavor) must be ignored.
        //
        // Modeled on a condi multi-stat rune (Nightmare/Trapper-style): a 6-tier
        // bonus list mixing flat condition damage, a specific-condition duration,
        // a global condition duration, an outgoing-healing %, and two non-modifier
        // strings that the parser must skip without corrupting results.
        let rune_id: u32 = 24818;
        let rune = Item {
            id: rune_id,
            name: "Superior Rune of the Nightmare".into(),
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
            details: Some(gw2_api::models::ItemDetails {
                detail_type: Some("Rune".into()),
                weight_class: None,
                defense: None,
                damage_type: None,
                min_power: None,
                max_power: None,
                suffix: Some("of the Nightmare".into()),
                bonuses: vec![
                    // Tier 1: flat stat — no '%', must be ignored as noise.
                    "+25 Power".into(),
                    // Tier 2: flat condition-damage % -> condition_pct (0.05).
                    "+5% Condition Damage".into(),
                    // Tier 3: non-modifier flavor text — must be ignored as noise.
                    "Gain might on heal".into(),
                    // Tier 4: specific-condition duration ->
                    // specific_condi_duration["Burning"] (0.10).
                    "+10% Burning Duration".into(),
                    // Tier 5: global condition duration -> condi_duration_pct (0.15).
                    "+15% Condition Duration".into(),
                    // Tier 6: outgoing healing % -> healing_pct (0.10).
                    "+10% Outgoing Healing".into(),
                ],
                infusion_upgrade_flags: vec![],
                infusion_slots: vec![],
                attribute_adjustment: None,
                infix_upgrade: None,
                suffix_item_id: None,
                secondary_suffix_item_id: None,
                stat_choices: vec![],
            }),
        };

        let mut items_cache: HashMap<u32, Item> = HashMap::new();
        items_cache.insert(rune_id, rune);
        let empty_traits: HashMap<u32, Trait> = HashMap::new();
        let ctx = BalanceContext::pve();

        let mods = extract_damage_modifiers(
            &[],           // no equipped traits
            Some(rune_id), // rune under test
            &[],           // no sigils
            None,          // no relic
            &empty_traits,
            &items_cache,
            &ctx,
        );

        // All four modifier kinds present at once from the single item.
        assert_eq!(
            mods.condition_pct.len(),
            1,
            "flat condition-damage % dropped from multi-bonus rune"
        );
        assert!((mods.condition_pct[0] - 0.05).abs() < 0.001);

        let burning = mods
            .specific_condi_duration
            .get("Burning")
            .expect("Burning-duration bonus dropped from multi-bonus rune");
        assert_eq!(burning.len(), 1);
        assert!((burning[0] - 0.10).abs() < 0.001);

        assert_eq!(
            mods.condi_duration_pct.len(),
            1,
            "global condition-duration % dropped from multi-bonus rune"
        );
        assert!((mods.condi_duration_pct[0] - 0.15).abs() < 0.001);

        assert_eq!(
            mods.healing_pct.len(),
            1,
            "outgoing-healing % dropped from multi-bonus rune"
        );
        assert!((mods.healing_pct[0] - 0.10).abs() < 0.001);

        // Noise must not leak into any axis: the flat "+25 Power" and the flavor
        // "Gain might on heal" credit nothing, and nothing spilled into strike.
        assert!(
            mods.strike_pct.is_empty(),
            "noise/flat-stat bonuses leaked into strike_pct"
        );
    }

    fn percent_fact(text: &str, percent: f64) -> gw2_api::models::Fact {
        gw2_api::models::Fact::Percent {
            text: Some(text.to_string()),
            icon: None,
            percent: Some(percent),
        }
    }

    #[test]
    fn test_extract_modifier_condition_damage_fact_without_increase_keyword() {
        // A structured Percent fact phrased as "Condition Damage: +X%" (no literal
        // word "increase") must still credit condition_pct — previously dropped.
        let mut mods = DamageModifiers::default();
        extract_modifier_from_fact(&mut mods, &percent_fact("Condition Damage: +10%", 10.0));
        assert_eq!(mods.condition_pct.len(), 1);
        assert!((mods.condition_pct[0] - 0.10).abs() < 0.001);
        assert!(mods.strike_pct.is_empty());
    }

    #[test]
    fn test_extract_modifier_generic_damage_fact_still_routes_to_strike() {
        // Guard: a generic-damage fact must NOT leak into the condition branch.
        let mut mods = DamageModifiers::default();
        extract_modifier_from_fact(&mut mods, &percent_fact("Damage increased by 7%", 7.0));
        assert_eq!(mods.strike_pct.len(), 1);
        assert!(mods.condition_pct.is_empty());
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
        parse_sigil_modifier(&mut mods, &sigil, &BalanceContext::pve());
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

    fn item_with_desc(name: &str, item_type: &str, desc: &str) -> Item {
        Item {
            id: 99999,
            name: name.into(),
            item_type: item_type.into(),
            rarity: "Exotic".into(),
            level: 80,
            description: Some(desc.into()),
            icon: None,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec![],
            restrictions: vec![],
            details: None,
        }
    }

    #[test]
    fn test_parse_sigil_description_condition_damage() {
        // A description-fallback sigil with "+N% condition damage" must credit
        // condition_pct, not be swallowed by the generic-damage branch.
        let mut mods = DamageModifiers::default();
        let sigil = item_with_desc(
            "Unknown Sigil of Testing",
            "UpgradeComponent",
            "Grants +6% condition damage.",
        );
        parse_sigil_modifier(&mut mods, &sigil, &BalanceContext::pve());
        assert_eq!(mods.condition_pct.len(), 1);
        assert!((mods.condition_pct[0] - 0.06).abs() < 0.001);
        assert!(mods.strike_pct.is_empty());
    }

    #[test]
    fn test_parse_relic_description_condition_damage() {
        let mut mods = DamageModifiers::default();
        let relic = item_with_desc(
            "Unknown Relic",
            "Relic",
            "Increases condition damage by 10%.",
        );
        parse_relic_modifier(&mut mods, &relic);
        assert_eq!(mods.condition_pct.len(), 1);
        assert!((mods.condition_pct[0] - 0.10).abs() < 0.001);
        assert!(mods.strike_pct.is_empty());
    }

    #[test]
    fn test_parse_sigil_smoldering() {
        let mut mods = DamageModifiers::default();
        let sigil = item_with_desc(
            "Superior Sigil of Smoldering",
            "UpgradeComponent",
            "Increase Inflicted Burning Duration: 20%.",
        );
        parse_sigil_modifier(&mut mods, &sigil, &BalanceContext::pve());
        assert_eq!(
            mods.specific_condi_duration.get("Burning").unwrap().len(),
            1
        );
        assert!((mods.specific_condi_duration["Burning"][0] - 0.20).abs() < 0.001);
    }

    #[test]
    fn test_parse_sigil_transference() {
        let mut mods = DamageModifiers::default();
        let sigil = item_with_desc(
            "Superior Sigil of Transference",
            "UpgradeComponent",
            "Outgoing healing is increased by 10%.",
        );
        parse_sigil_modifier(&mut mods, &sigil, &BalanceContext::pve());
        assert_eq!(mods.healing_pct.len(), 1);
        assert!((mods.healing_pct[0] - 0.10).abs() < 0.001);
    }

    #[test]
    fn test_parse_relic_isgarren() {
        let mut mods = DamageModifiers::default();
        let relic = item_with_desc("Relic of Isgarren", "Relic", "Gain 10% critical damage.");
        parse_relic_modifier(&mut mods, &relic);
        assert_eq!(mods.crit_damage_pct.len(), 1);
        assert!((mods.crit_damage_pct[0] - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_relic_nightmare() {
        let mut mods = DamageModifiers::default();
        let relic = item_with_desc(
            "Relic of the Nightmare",
            "Relic",
            "Your elite skill inflicts fear and pulses poison around you.",
        );
        parse_relic_modifier(&mut mods, &relic);
        assert!(
            mods.condi_duration_pct.is_empty(),
            "current Nightmare is not +10% condition duration"
        );
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
        let result = condition_duration_multiplied(3.0, 450.0, 0.0, 0.20, 1.0, &ctx);
        assert!((result - 4.5).abs() < 0.001, "Expected 4.5, got {result}",);
    }

    #[test]
    fn test_boon_duration_basic() {
        // 600 Concentration: bonus = 600/1500 = 0.40
        // result = 5.0 * (1 + 0.40) = 5.0 * 1.40 = 7.0s
        // Source: https://wiki.guildwars2.com/wiki/Boon_Duration
        let ctx = BalanceContext::pve();
        let result = boon_duration_multiplied(5.0, 600.0, 0.0, 1.0, &ctx);
        assert!((result - 7.0).abs() < 0.001, "Expected 7.0, got {result}",);
    }

    #[test]
    fn test_condition_duration_cap() {
        // 1800 Expertise + 30% modifier: raw = 1800/1500 + 0.30 = 1.50, capped at 1.0
        // result = 3.0 * (1 + 1.0) = 6.0s
        // Source: https://wiki.guildwars2.com/wiki/Condition_Duration ("maximum 100%")
        let ctx = BalanceContext::pve();
        let result = condition_duration_multiplied(3.0, 1800.0, 0.0, 0.30, 1.0, &ctx);
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
        let result = boon_duration_multiplied(5.0, 2000.0, 0.0, 1.0, &ctx);
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
        let result = condition_duration_multiplied(4.0, 300.0, 0.10, 0.20, 1.0, &ctx);
        assert!((result - 6.0).abs() < 0.001, "Expected 6.0, got {result}",);
    }

    #[test]
    fn test_zero_expertise_zero_modifiers() {
        // 0 Expertise, no modifiers: bonus = 0.0, multiplier = 1.0
        // result = base * 1.0 = base
        // Source: https://wiki.guildwars2.com/wiki/Expertise
        let ctx = BalanceContext::pve();
        let result = condition_duration_multiplied(5.0, 0.0, 0.0, 0.0, 1.0, &ctx);
        assert!((result - 5.0).abs() < 0.001, "Expected 5.0, got {result}",);

        let boon_result = boon_duration_multiplied(5.0, 0.0, 0.0, 1.0, &ctx);
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
        let condi = condition_duration_bonus(750.0, 0.05, 0.10, 1.0, &ctx);
        assert!((condi - 0.65).abs() < 0.001, "Expected 0.65, got {condi}",);

        // 900 Concentration, 10% global: 900/1500 + 0.10 = 0.60 + 0.10 = 0.70
        let boon = boon_duration_bonus(900.0, 0.10, 1.0, &ctx);
        assert!((boon - 0.70).abs() < 0.001, "Expected 0.70, got {boon}",);

        // Capped: 1500 Expertise, 20% global, 10% specific:
        // 1500/1500 + 0.20 + 0.10 = 1.30 -> capped at 1.0
        let capped = condition_duration_bonus(1500.0, 0.20, 0.10, 1.0, &ctx);
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

        let condi_pve = condition_duration_bonus(600.0, 0.10, 0.05, 1.0, &pve);
        let condi_pvp = condition_duration_bonus(600.0, 0.10, 0.05, 1.0, &pvp);
        let condi_wvw = condition_duration_bonus(600.0, 0.10, 0.05, 1.0, &wvw);
        assert!(
            (condi_pve - condi_pvp).abs() < 0.001,
            "PvE and PvP should match (currently mode-invariant)",
        );
        assert!(
            (condi_pve - condi_wvw).abs() < 0.001,
            "PvE and WvW should match (currently mode-invariant)",
        );

        let boon_pve = boon_duration_bonus(600.0, 0.10, 1.0, &pve);
        let boon_pvp = boon_duration_bonus(600.0, 0.10, 1.0, &pvp);
        let boon_wvw = boon_duration_bonus(600.0, 0.10, 1.0, &wvw);
        assert!(
            (boon_pve - boon_pvp).abs() < 0.001,
            "PvE and PvP boon should match (currently mode-invariant)",
        );
        assert!(
            (boon_pve - boon_wvw).abs() < 0.001,
            "PvE and WvW boon should match (currently mode-invariant)",
        );
    }


    fn percent_trait(id: u32, name: &str, pairs: &[(&str, f64)]) -> Trait {
        Trait {
            id,
            name: name.to_string(),
            icon: None,
            description: None,
            specialization: 0,
            tier: 0,
            order: 0,
            slot: "Minor".into(),
            facts: pairs
                .iter()
                .map(|(text, pct)| Fact::Percent {
                    text: Some((*text).into()),
                    icon: None,
                    percent: Some(*pct),
                })
                .collect(),
            traited_facts: vec![],
            skills: vec![],
        }
    }

    #[test]
    fn screenshot_percent_facts_are_legal_in_wvw() {
        let traits = [
            percent_trait(1011, "Precise Strike", &[("Critical Chance Increase", 100.0)]),
            percent_trait(
                1001,
                "Wolfsong",
                &[("Damage Increase", 5.0), ("Damage Increase", 10.0)],
            ),
            percent_trait(
                2156,
                "Furious Strength",
                &[("Damage Increase", 15.0), ("Damage Increase", 7.0)],
            ),
            percent_trait(
                2119,
                "Second Skin",
                &[("Damage Reduced", 25.0), ("Damage Reduced", 33.0)],
            ),
            percent_trait(1015, "Remorseless", &[("Damage Increase", 25.0)]),
            percent_trait(1698, "Lead the Wind", &[("Recharge Reduced", 20.0)]),
        ];
        let cache: HashMap<u32, Trait> = traits.into_iter().map(|t| (t.id, t)).collect();
        let ids: Vec<u32> = cache.keys().copied().collect();
        let items = HashMap::new();

        let wvw = extract_damage_modifiers(
            &ids,
            None,
            &[],
            None,
            &cache,
            &items,
            &BalanceContext::wvw(),
        );
        assert!(
            wvw.crit_chance_pct.is_empty(),
            "Precise Strike must not add standing crit: {:?}",
            wvw.crit_chance_pct
        );
        let mut strike = wvw.strike_pct.clone();
        strike.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(strike, vec![0.05, 0.07, 0.25]);
        let expected_mult = 1.05 * 1.07 * 1.25;
        assert!((wvw.total_strike_mult() - expected_mult).abs() < 1e-9);

        let pve = extract_damage_modifiers(
            &ids,
            None,
            &[],
            None,
            &cache,
            &items,
            &BalanceContext::pve(),
        );
        let mut pve_strike = pve.strike_pct.clone();
        pve_strike.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(pve_strike, vec![0.10, 0.15, 0.25]);
    }

}
