//! Deterministic synergy engine: effect taxonomy, extractors, and scoring.
//!
//! This module provides a unified representation for ALL synergy-relevant effects
//! from any build component (traits, runes, sigils, relics, skills). Effects are
//! extracted into `NormalizedEffect` values, then scored against `OptimizationWeights`
//! with interaction bonuses for synergistic combinations.

use gw2_api::models::{Fact, Item, Skill, Trait as GW2Trait};

use crate::data::boon_condition_formulas::{boon_weight, condition_importance};
use crate::scoring::OptimizationWeights;
use crate::text_util::{strip_gw2_markup, text_describes_condition_cleanse};

// ─── Supporting Enums ───

/// GW2 primary attribute identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatType {
    Power,
    Precision,
    Toughness,
    Vitality,
    ConditionDamage,
    Expertise,
    Concentration,
    Ferocity,
    HealingPower,
}

impl StatType {
    pub fn from_api(s: &str) -> Option<Self> {
        match s {
            "Power" => Some(Self::Power),
            "Precision" => Some(Self::Precision),
            "Toughness" => Some(Self::Toughness),
            "Vitality" => Some(Self::Vitality),
            "ConditionDamage" => Some(Self::ConditionDamage),
            "ConditionDuration" | "Expertise" => Some(Self::Expertise),
            "BoonDuration" | "Concentration" => Some(Self::Concentration),
            "CritDamage" | "Ferocity" => Some(Self::Ferocity),
            "Healing" | "HealingPower" => Some(Self::HealingPower),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Power => "Power",
            Self::Precision => "Precision",
            Self::Toughness => "Toughness",
            Self::Vitality => "Vitality",
            Self::ConditionDamage => "Condition Damage",
            Self::Expertise => "Expertise",
            Self::Concentration => "Concentration",
            Self::Ferocity => "Ferocity",
            Self::HealingPower => "Healing Power",
        }
    }
}

/// Category of damage modifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DamageCategory {
    Strike,
    Condition,
    SpecificCondition(String),
    Crit,
    Healing,
}

/// Duration bonus kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DurationKind {
    AllCondition,
    AllBoon,
    SpecificCondition(String),
}

/// Proc trigger kind (for estimated uptime).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProcTrigger {
    OnCrit,
    OnHit,
    OnDodge,
    OnWeaponSwap,
    OnKill,
    OnHealthThreshold,
    Passive,
}

// ─── NormalizedEffect ───

/// Unified representation for all synergy-relevant effects from any build component.
#[derive(Debug, Clone)]
pub enum NormalizedEffect {
    /// Flat stat bonus (e.g. +150 Power from a trait).
    StatBonus { stat: StatType, value: f64 },
    /// Percentage damage modifier (e.g. +5% strike damage).
    DamageModifier {
        category: DamageCategory,
        percent: f64,
    },
    /// Applies a condition or boon.
    AppliesStatus {
        status: String,
        is_condition: bool,
        duration_s: u32,
        stacks: u32,
    },
    /// Benefits when a status is present (e.g. +X% while Fury is active).
    BenefitsFromStatus {
        status: String,
        effect: Box<NormalizedEffect>,
    },
    /// Converts one stat to another (e.g. 7% of Toughness to Power).
    StatConversion {
        source: StatType,
        target: StatType,
        percent: f64,
    },
    /// Duration bonus for conditions/boons (e.g. +10% Burning Duration).
    DurationBonus { kind: DurationKind, percent: f64 },
    /// Conditional effect requiring a specific trait to be equipped (TraitedFact).
    Conditional {
        requires_trait_id: u32,
        overrides_index: Option<u32>,
        effect: Box<NormalizedEffect>,
    },
    /// Proc-based effect with estimated uptime (0.0-1.0).
    ProcEffect {
        trigger: ProcTrigger,
        effect: Box<NormalizedEffect>,
        estimated_uptime: f64,
    },
}

/// Identifies the source of effects.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComponentId {
    Trait(u32),
    Rune(u32),
    Sigil(u32),
    Relic(u32),
    Skill(u32),
}

/// A scored synergy link between two components.
#[derive(Debug, Clone)]
pub struct SynergyLink {
    pub source: ComponentId,
    pub source_name: String,
    pub target: ComponentId,
    pub target_name: String,
    pub link_type: SynergyLinkType,
    pub score: f64,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum SynergyLinkType {
    TraitedFact,
    EnablerPayoff,
    ConditionStacking,
    ModifierStacking,
    DurationAlignment,
}

// ─── Effect Extractors ───

/// Extract normalized effects from a trait, considering equipped traits for TraitedFact resolution.
///
/// `equipped_traits` is scanned twice per traited_fact under typical inputs (~36 trait
/// ids), so we hoist it into a `HashSet` once. Reduces O(facts × traits) linear scans
/// to O(facts) hash lookups — meaningful since `select_specs_and_traits` calls this
/// for every trait in every cross-product combo (hundreds of invocations per build).
pub fn extract_trait_effects(t: &GW2Trait, equipped_traits: &[u32]) -> Vec<NormalizedEffect> {
    let equipped_set: std::collections::HashSet<u32> = equipped_traits.iter().copied().collect();
    let mut effects = Vec::new();

    // Collect overridden indices from active traited_facts
    let overridden: std::collections::HashSet<u32> = t
        .traited_facts
        .iter()
        .filter(|tf| equipped_set.contains(&tf.requires_trait))
        .filter_map(|tf| tf.overrides)
        .collect();

    // Process base facts (skip overridden ones)
    for (idx, fact) in t.facts.iter().enumerate() {
        if overridden.contains(&(idx as u32)) {
            continue;
        }
        effects.extend(extract_effects_from_fact(fact));
    }

    // Process traited_facts — active ones as direct effects, inactive as Conditional
    for tf in &t.traited_facts {
        let inner_effects = extract_effects_from_fact(&tf.fact);
        if equipped_set.contains(&tf.requires_trait) {
            effects.extend(inner_effects);
        } else {
            for eff in inner_effects {
                effects.push(NormalizedEffect::Conditional {
                    requires_trait_id: tf.requires_trait,
                    overrides_index: tf.overrides,
                    effect: Box::new(eff),
                });
            }
        }
    }

    effects
}

/// Extract normalized effects from a rune item.
pub fn extract_rune_effects(rune: &Item) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();

    if let Some(ref details) = rune.details {
        for bonus_str in &details.bonuses {
            effects.extend(parse_rune_bonus_to_effects(bonus_str));
        }
    }

    // Also check for stat bonuses in infix_upgrade
    if let Some(ref details) = rune.details {
        if let Some(ref infix) = details.infix_upgrade {
            for attr in &infix.attributes {
                if let Some(stat) = StatType::from_api(&attr.attribute) {
                    effects.push(NormalizedEffect::StatBonus {
                        stat,
                        value: attr.modifier as f64,
                    });
                }
            }
        }
    }

    effects
}

/// Extract normalized effects from a sigil item.
pub fn extract_sigil_effects(
    sigil: &Item,
    ctx: &crate::balance::BalanceContext,
) -> Vec<NormalizedEffect> {
    if let Some(buff) = crate::combat::item_buff_description(sigil) {
        let effects = effects_from_upgrade_text(buff);
        if !effects.is_empty() {
            return effects;
        }
    }
    if let Some(ref desc) = sigil.description {
        let effects = effects_from_upgrade_text(desc);
        if !effects.is_empty() {
            return effects;
        }
    }

    let name_lower = sigil.name.to_lowercase();
    let competitive = matches!(
        ctx.game_mode,
        gw2_core::types::GameMode::PvP | gw2_core::types::GameMode::WvW
    );
    if name_lower.contains("sigil of force") {
        vec![NormalizedEffect::DamageModifier {
            category: DamageCategory::Strike,
            percent: if competitive { 3.0 } else { 5.0 },
        }]
    } else if name_lower.contains("sigil of bursting") {
        vec![NormalizedEffect::DamageModifier {
            category: DamageCategory::Condition,
            percent: if competitive { 4.0 } else { 6.0 },
        }]
    } else {
        Vec::new()
    }
}

/// Extract normalized effects from a relic item.
pub fn extract_relic_effects(relic: &Item) -> Vec<NormalizedEffect> {
    if let Some(ref desc) = relic.description {
        if !desc.is_empty() {
            return effects_from_upgrade_text(desc);
        }
    }
    if let Some(buff) = crate::combat::item_buff_description(relic) {
        return effects_from_upgrade_text(buff);
    }
    Vec::new()
}

/// Extract normalized effects from a skill.
pub fn extract_skill_effects(skill: &Skill) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();
    for fact in &skill.facts {
        effects.extend(extract_effects_from_fact(fact));
    }
    effects
}

// ─── Internal fact-to-effect conversion ───

/// Convert a single GW2 API Fact into zero or more NormalizedEffects.
fn extract_effects_from_fact(fact: &Fact) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();

    match fact {
        Fact::AttributeAdjust {
            value: Some(val),
            target: Some(ref target),
            ..
        } => {
            if let Some(stat) = StatType::from_api(target) {
                effects.push(NormalizedEffect::StatBonus {
                    stat,
                    value: *val as f64,
                });
            }
        }
        Fact::Percent {
            text: Some(ref text),
            percent: Some(pct),
            ..
        } => {
            if crate::combat::percent_text_is_conditional(text) {
                return effects;
            }
            let text_lower = text.to_lowercase();
            let pct = if text_lower.contains("90") && text_lower.contains("health") {
                *pct * 0.9
            } else {
                *pct
            };
            // Crit damage MUST be checked before the generic "damage" catch-all:
            // "Critical Damage" contains the substring "damage" but is neither
            // strike nor condition damage. Ordering it first prevents it from
            // being misclassified as a strike modifier.
            if text_lower.contains("critical damage") || text_lower.contains("crit damage") {
                effects.push(NormalizedEffect::DamageModifier {
                    category: DamageCategory::Crit,
                    percent: pct,
                });
            } else if text_lower.contains("condition damage") {
                // Mirror the strike branch below: trust the structured Percent fact.
                // Requiring the literal word "increase" silently dropped condition
                // damage facts phrased as "Condition Damage: +X%".
                effects.push(NormalizedEffect::DamageModifier {
                    category: DamageCategory::Condition,
                    percent: pct,
                });
            } else if text_lower.contains("damage") {
                effects.push(NormalizedEffect::DamageModifier {
                    category: DamageCategory::Strike,
                    percent: pct,
                });
            } else if text_lower.contains("outgoing healing") {
                effects.push(NormalizedEffect::DamageModifier {
                    category: DamageCategory::Healing,
                    percent: pct,
                });
            } else if text_lower.contains("boon duration") {
                effects.push(NormalizedEffect::DurationBonus {
                    kind: DurationKind::AllBoon,
                    percent: pct,
                });
            } else if text_lower.contains("condition duration") {
                effects.push(NormalizedEffect::DurationBonus {
                    kind: DurationKind::AllCondition,
                    percent: pct,
                });
            }

            // Specific condition duration patterns. Search terms stay verb-form
            // (tooltip text says "Poison Duration"); the stored key is
            // canonicalized so it matches the "Poisoned" form every other path
            // (and `duration_matches`) uses. Without this, a "+X% Poison Duration"
            // bonus keyed as "Poison" would never match a "Poisoned" lookup.
            for condi in &["Bleeding", "Burning", "Poison", "Torment", "Confusion"] {
                if text_lower.contains(&condi.to_lowercase()) && text_lower.contains("duration") {
                    let canonical =
                        crate::data::boon_condition_formulas::canonical_condition_name(condi);
                    effects.push(NormalizedEffect::DurationBonus {
                        kind: DurationKind::SpecificCondition(canonical.to_string()),
                        percent: pct,
                    });
                }
            }
        }
        Fact::Buff {
            status: Some(ref status),
            duration: Some(dur),
            apply_count,
            ..
        } => {
            let is_cond = crate::data::boon_condition_formulas::is_condition(status);
            // Store the canonical name (e.g. "Poisoned" not "Poison") so the
            // synergy matchers in `compute_marginal_synergy` compare apples to
            // apples — two emitters (one trait, one sigil) referring to the
            // same condition under different aliases would otherwise miss.
            let canonical = crate::data::boon_condition_formulas::canonical_condition_name(status);
            effects.push(NormalizedEffect::AppliesStatus {
                status: canonical.to_string(),
                is_condition: is_cond,
                duration_s: *dur,
                stacks: apply_count.unwrap_or(1),
            });
        }
        // PrefixedBuff has the same combat-relevant fields as Buff (status, duration,
        // apply_count) but also carries a textual prefix describing the application
        // context (e.g. "To nearby enemies", "On hit"). The effects are identical
        // for synergy scoring purposes — handle them the same way.
        Fact::PrefixedBuff {
            status: Some(ref status),
            duration: Some(dur),
            apply_count,
            ..
        } => {
            let is_cond = crate::data::boon_condition_formulas::is_condition(status);
            let canonical = crate::data::boon_condition_formulas::canonical_condition_name(status);
            effects.push(NormalizedEffect::AppliesStatus {
                status: canonical.to_string(),
                is_condition: is_cond,
                duration_s: *dur,
                stacks: apply_count.unwrap_or(1),
            });
        }
        Fact::BuffConversion {
            percent: Some(pct),
            source: Some(ref src),
            target: Some(ref tgt),
            ..
        } => {
            if let (Some(s), Some(t)) = (StatType::from_api(src), StatType::from_api(tgt)) {
                effects.push(NormalizedEffect::StatConversion {
                    source: s,
                    target: t,
                    percent: *pct,
                });
            }
        }
        Fact::Damage { .. } => {
            // Damage facts represent direct hit damage, not percentage modifiers.
            // Treating them as DamageModifier inflates modifier stacking calculations.
            // Direct damage is handled by the rotation simulator, not the synergy engine.
        }
        _ => {}
    }

    effects
}

/// Parse a rune bonus string into NormalizedEffects.
/// Examples: "+7% Burning Duration", "7% Burning Duration", "+5% damage", "+175 Power"
fn parse_rune_bonus_to_effects(bonus: &str) -> Vec<NormalizedEffect> {
    if let Some(all) = parse_all_stats_bonus(bonus) {
        return all;
    }
    let mut effects = effects_from_upgrade_text(bonus);
    if !effects.is_empty() {
        return effects;
    }
    let s = bonus.trim().to_lowercase();
    if let Some(without_plus) = s.strip_prefix('+') {
        let parts: Vec<&str> = without_plus.splitn(2, ' ').collect();
        if parts.len() == 2 {
            if let Ok(value) = parts[0].trim().parse::<f64>() {
                let stat_name = parts[1].trim();
                if let Some(stat) = stat_type_from_display_name(stat_name) {
                    effects.push(NormalizedEffect::StatBonus { stat, value });
                }
            }
        }
    }
    effects
}

const ALL_STATS: [StatType; 9] = [
    StatType::Power,
    StatType::Precision,
    StatType::Toughness,
    StatType::Vitality,
    StatType::ConditionDamage,
    StatType::Expertise,
    StatType::Concentration,
    StatType::Ferocity,
    StatType::HealingPower,
];

fn parse_all_stats_bonus(bonus: &str) -> Option<Vec<NormalizedEffect>> {
    let s = bonus.trim().to_lowercase();
    if !s.contains("all stats") {
        return None;
    }
    let rest = s.strip_prefix('+').unwrap_or(s.as_str());
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = num.parse().ok()?;
    if value <= 0.0 {
        return None;
    }
    Some(
        ALL_STATS
            .iter()
            .map(|&stat| NormalizedEffect::StatBonus { stat, value })
            .collect(),
    )
}

fn effects_from_upgrade_text(text: &str) -> Vec<NormalizedEffect> {
    let mut mods = crate::combat::DamageModifiers::default();
    crate::combat::apply_upgrade_text(&mut mods, text);
    let mut effects = normalized_from_modifiers(&mods);
    effects.extend(prose_status_effects(text));
    effects
}

fn prose_status_effects(text: &str) -> Vec<NormalizedEffect> {
    let t = strip_gw2_markup(text).to_lowercase();
    let mut effects = Vec::new();
    let duration_mod = t.contains("increase") && t.contains("duration");
    if !duration_mod {
        let applying = t.contains("inflict") || t.contains("applies") || t.contains("apply ");
        if applying {
            for (needle, status) in [
                ("bleeding", "Bleeding"),
                ("burning", "Burning"),
                ("poison", "Poisoned"),
                ("torment", "Torment"),
                ("confusion", "Confusion"),
                ("vulnerability", "Vulnerability"),
            ] {
                if t.contains(needle) {
                    effects.push(NormalizedEffect::AppliesStatus {
                        status: status.into(),
                        is_condition: true,
                        duration_s: 5,
                        stacks: 1,
                    });
                    break;
                }
            }
        }
        for (needle, status) in [
            ("might", "Might"),
            ("fury", "Fury"),
            ("quickness", "Quickness"),
            ("alacrity", "Alacrity"),
            ("protection", "Protection"),
        ] {
            if (t.contains("grant") || t.contains("gain ")) && t.contains(needle) {
                effects.push(NormalizedEffect::AppliesStatus {
                    status: status.into(),
                    is_condition: false,
                    duration_s: 5,
                    stacks: 1,
                });
                break;
            }
        }
    }
    if text_describes_condition_cleanse(&t) {
        effects.push(NormalizedEffect::AppliesStatus {
            status: "Cleanse".into(),
            is_condition: false,
            duration_s: 0,
            stacks: 1,
        });
    }
    effects
}

fn normalized_from_modifiers(mods: &crate::combat::DamageModifiers) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();
    for &d in &mods.strike_pct {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Strike,
            percent: d * 100.0,
        });
    }
    for &d in &mods.condition_pct {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Condition,
            percent: d * 100.0,
        });
    }
    for &d in &mods.crit_damage_pct {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Crit,
            percent: d,
        });
    }
    for &d in &mods.crit_chance_pct {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Crit,
            percent: d,
        });
    }
    for &d in &mods.healing_pct {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Healing,
            percent: d * 100.0,
        });
    }
    for &d in &mods.condi_duration_pct {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::AllCondition,
            percent: d * 100.0,
        });
    }
    for &d in &mods.boon_duration_pct {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::AllBoon,
            percent: d * 100.0,
        });
    }
    for (cond, vals) in &mods.specific_condi_duration {
        for &d in vals {
            effects.push(NormalizedEffect::DurationBonus {
                kind: DurationKind::SpecificCondition(cond.clone()),
                percent: d * 100.0,
            });
        }
    }
    for (cond, vals) in &mods.specific_condi {
        for &d in vals {
            effects.push(NormalizedEffect::DamageModifier {
                category: DamageCategory::SpecificCondition(cond.clone()),
                percent: d * 100.0,
            });
        }
    }
    effects
}

// ─── Scoring Functions ───

/// Score a single NormalizedEffect against optimization weights.
/// Returns a value roughly on the 0.0-1.0 scale.
pub fn score_normalized_effect(effect: &NormalizedEffect, weights: &OptimizationWeights) -> f64 {
    match effect {
        NormalizedEffect::StatBonus { stat, value } => {
            let w = weight_for_stat(stat, weights);
            // +100 stat with weight 1.0 → ~0.033 (matches existing score_fact normalization)
            value / 3000.0 * w
        }
        NormalizedEffect::DamageModifier { category, percent } => {
            let w = weight_for_damage_category(category, weights);
            // +5% damage modifier with weight 1.0 → 0.05
            percent / 100.0 * w
        }
        NormalizedEffect::AppliesStatus {
            status,
            is_condition,
            stacks,
            ..
        } => {
            if *is_condition {
                let w = weights.condition;
                // Conditions are valuable for condition builds
                (*stacks as f64).min(5.0) * 0.02 * w + condition_importance(status) * 0.03 * w
            } else {
                // Boons: value depends on boon type
                boon_weight(status, weights) * 0.05
            }
        }
        NormalizedEffect::BenefitsFromStatus { effect, .. } => {
            // Base value of the inner effect, discounted
            score_normalized_effect(effect, weights) * 0.3
        }
        NormalizedEffect::StatConversion {
            source,
            target,
            percent,
        } => {
            let src_w = weight_for_stat(source, weights);
            let tgt_w = weight_for_stat(target, weights);
            // Conversion is good when source stat is high (from gear) and target is useful
            percent / 100.0 * src_w * tgt_w
        }
        NormalizedEffect::DurationBonus { kind, percent } => {
            let w = match kind {
                DurationKind::AllCondition => weights.condition * 0.8 + weights.control * 0.3,
                DurationKind::AllBoon => {
                    weights.boon_support * 0.4 + weights.control * 0.2 + weights.healing * 0.3
                }
                DurationKind::SpecificCondition(_) => weights.condition * 0.5,
            };
            percent / 100.0 * w
        }
        NormalizedEffect::Conditional { effect, .. } => {
            // Conditional effects get partial credit (may or may not be activated)
            score_normalized_effect(effect, weights) * 0.4
        }
        NormalizedEffect::ProcEffect {
            effect,
            estimated_uptime,
            ..
        } => score_normalized_effect(effect, weights) * estimated_uptime,
    }
}

/// Compute the marginal synergy score of adding new_effects to the existing build effects.
/// Detects interaction chains and rewards builds with complementary components.
///
/// `new_id` identifies the component whose effects are being added (rune, sigil,
/// relic, trait, weapon-skill, utility-skill). It populates the `target` field of
/// each emitted `SynergyLink` so UI can render which component completes a chain.
/// Callers without a meaningful new id (legacy/test cases) may pass `None`; the
/// target then falls back to the existing component, preserving prior behavior.
pub fn compute_marginal_synergy(
    new_effects: &[NormalizedEffect],
    existing_effects: &[(ComponentId, Vec<NormalizedEffect>)],
    weights: &OptimizationWeights,
    new_id: Option<&ComponentId>,
) -> (f64, Vec<SynergyLink>) {
    let mut synergy = 0.0;
    let mut links = Vec::new();
    let new_target = |existing: &ComponentId| new_id.cloned().unwrap_or_else(|| existing.clone());

    // Flatten existing effects for quick scanning
    let all_existing: Vec<(&ComponentId, &NormalizedEffect)> = existing_effects
        .iter()
        .flat_map(|(id, effs)| effs.iter().map(move |e| (id, e)))
        .collect();

    for new_eff in new_effects {
        for &(existing_id, existing_eff) in &all_existing {
            // 1. TraitedFact activation: Conditional requires a trait that is in the build
            if let NormalizedEffect::Conditional {
                requires_trait_id,
                effect,
                ..
            } = new_eff
            {
                if let ComponentId::Trait(tid) = existing_id {
                    if *tid == *requires_trait_id {
                        let bonus = score_normalized_effect(effect, weights) * 0.6;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("Trait {}", tid),
                            target: new_target(existing_id),
                            target_name: "Conditional effect".into(),
                            link_type: SynergyLinkType::TraitedFact,
                            score: bonus,
                            description: gw2_core::i18n::t("explain.traited_fact"),
                        });
                    }
                }
            }

            // 2. Enabler→Payoff: AppliesStatus meets BenefitsFromStatus
            if let NormalizedEffect::AppliesStatus {
                status: applied, ..
            } = new_eff
            {
                if let NormalizedEffect::BenefitsFromStatus {
                    status: needed,
                    effect,
                } = existing_eff
                {
                    if applied == needed {
                        let bonus = score_normalized_effect(effect, weights) * 0.5;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("Benefits from {}", needed),
                            target: new_target(existing_id),
                            target_name: format!("Applies {}", applied),
                            link_type: SynergyLinkType::EnablerPayoff,
                            score: bonus,
                            description: gw2_core::i18n::tf(
                                "explain.applies_enables",
                                &[("status", &applied.to_string())],
                            ),
                        });
                    }
                }
            }
            if let NormalizedEffect::BenefitsFromStatus {
                status: needed,
                effect,
            } = new_eff
            {
                if let NormalizedEffect::AppliesStatus {
                    status: applied, ..
                } = existing_eff
                {
                    if applied == needed {
                        let bonus = score_normalized_effect(effect, weights) * 0.5;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("Applies {}", applied),
                            target: new_target(existing_id),
                            target_name: format!("Benefits from {}", needed),
                            link_type: SynergyLinkType::EnablerPayoff,
                            score: bonus,
                            description: gw2_core::i18n::tf(
                                "explain.existing_feeds",
                                &[("status", &applied.to_string())],
                            ),
                        });
                    }
                }
            }

            // 3. Condition stacking: multiple sources of same condition
            if let NormalizedEffect::AppliesStatus {
                status: s1,
                is_condition: true,
                ..
            } = new_eff
            {
                if let NormalizedEffect::AppliesStatus {
                    status: s2,
                    is_condition: true,
                    ..
                } = existing_eff
                {
                    if s1 == s2 {
                        let bonus = condition_importance(s1) * 0.03 * weights.condition;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("{} source", s2),
                            target: new_target(existing_id),
                            target_name: format!("{} source", s1),
                            link_type: SynergyLinkType::ConditionStacking,
                            score: bonus,
                            description: gw2_core::i18n::tf(
                                "explain.condition_stack",
                                &[("status", &s1.to_string())],
                            ),
                        });
                    }
                }
            }

            // 4. Modifier stacking: multiple modifiers in same category interact
            if let NormalizedEffect::DamageModifier {
                category: c1,
                percent: p1,
            } = new_eff
            {
                if let NormalizedEffect::DamageModifier {
                    category: c2,
                    percent: p2,
                } = existing_eff
                {
                    if c1 == c2 {
                        let interaction = (p1 / 100.0) * (p2 / 100.0);
                        let w = weight_for_damage_category(c1, weights);
                        let bonus = interaction * w * 0.5;
                        synergy += bonus;
                        if bonus > 0.001 {
                            links.push(SynergyLink {
                                source: existing_id.clone(),
                                source_name: format!("{:?} +{}%", c2, p2),
                                target: new_target(existing_id),
                                target_name: format!("{:?} +{}%", c1, p1),
                                link_type: SynergyLinkType::ModifierStacking,
                                score: bonus,
                                description: gw2_core::i18n::tf(
                                    "explain.modifier_stack",
                                    &[("category", &format!("{c1:?}"))],
                                ),
                            });
                        }
                    }
                }
            }

            // 5. Duration alignment: condition applied + matching duration bonus
            if let NormalizedEffect::AppliesStatus {
                status,
                is_condition: true,
                ..
            } = new_eff
            {
                if let NormalizedEffect::DurationBonus { kind, .. } = existing_eff {
                    if duration_matches_condition(kind, status) {
                        let bonus = 0.03 * weights.condition;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("{:?} duration", kind),
                            target: new_target(existing_id),
                            target_name: format!("{} application", status),
                            link_type: SynergyLinkType::DurationAlignment,
                            score: bonus,
                            description: gw2_core::i18n::tf(
                                "explain.duration_extends",
                                &[("status", &status.to_string())],
                            ),
                        });
                    }
                }
            }
            if let NormalizedEffect::DurationBonus { kind, .. } = new_eff {
                if let NormalizedEffect::AppliesStatus {
                    status,
                    is_condition: true,
                    ..
                } = existing_eff
                {
                    if duration_matches_condition(kind, status) {
                        let bonus = 0.03 * weights.condition;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("{} application", status),
                            target: new_target(existing_id),
                            target_name: format!("{:?} duration", kind),
                            link_type: SynergyLinkType::DurationAlignment,
                            score: bonus,
                            description: gw2_core::i18n::tf(
                                "explain.duration_benefits",
                                &[("status", &status.to_string())],
                            ),
                        });
                    }
                }
            }
        }
    }

    (synergy, links)
}

// ─── Template Explanation ───

/// Generate a template-based explanation from synergy links (no LLM needed).
pub fn template_explanation(
    synergy_links: &[SynergyLink],
    gear_prefix: &str,
    profession: &str,
) -> String {
    use gw2_core::i18n::tf;
    if synergy_links.is_empty() {
        return tf(
            "explain.uses_gear_empty",
            &[("profession", profession), ("gear", gear_prefix)],
        );
    }

    let mut parts = Vec::new();
    parts.push(tf(
        "explain.uses_gear",
        &[("profession", profession), ("gear", gear_prefix)],
    ));
    let mut seen = std::collections::HashSet::new();
    for link in synergy_links {
        if seen.insert(link.description.as_str()) {
            parts.push(link.description.clone());
        }
        if parts.len() >= 6 {
            break;
        }
    }
    parts.join(" ")
}

// ─── Helper Functions ───

fn weight_for_stat(stat: &StatType, weights: &OptimizationWeights) -> f64 {
    match stat {
        StatType::Power => weights.power * 0.8,
        StatType::Precision => weights.power * 0.6 + weights.condition * 0.2,
        StatType::Ferocity => weights.power * 0.5,
        StatType::ConditionDamage => weights.condition * 0.8 + weights.control * 0.2,
        StatType::Expertise => weights.condition * 0.6 + weights.control * 0.4,
        StatType::Concentration => {
            weights.boon_support * 0.5 + weights.control * 0.2 + weights.healing * 0.3
        }
        StatType::HealingPower => weights.healing * 0.9,
        StatType::Toughness => weights.sustain * 0.8,
        StatType::Vitality => weights.sustain * 0.7,
    }
}

fn weight_for_damage_category(cat: &DamageCategory, weights: &OptimizationWeights) -> f64 {
    match cat {
        DamageCategory::Strike => weights.power,
        DamageCategory::Condition => weights.condition,
        DamageCategory::SpecificCondition(_) => weights.condition * 0.8,
        DamageCategory::Crit => weights.power * 0.8,
        DamageCategory::Healing => weights.healing,
    }
}

fn duration_matches_condition(kind: &DurationKind, condition: &str) -> bool {
    match kind {
        DurationKind::AllCondition => true,
        DurationKind::SpecificCondition(ref c) => c.eq_ignore_ascii_case(condition),
        DurationKind::AllBoon => false,
    }
}

/// Test-only thin wrappers so the alias-routing regression suite can fuzz the
/// shared `is_condition` helper (through the path this module consumes it by)
/// and the private `condition_importance` helper without changing visibility.
#[cfg(test)]
pub(crate) mod tests_alias_helpers {
    pub(crate) fn is_condition(status: &str) -> bool {
        crate::data::boon_condition_formulas::is_condition(status)
    }
    pub(crate) fn condition_importance(status: &str) -> f64 {
        crate::data::boon_condition_formulas::condition_importance(status)
    }
}

/// Test-only shim for the cross-parser consistency suite.
///
/// Runs the private [`extract_effects_from_fact`] and collapses its
/// `Vec<NormalizedEffect>` into the same comparable [`FactClass`] set the
/// combat parser's shim produces, so the consistency test can assert the two
/// parsers classify each modifier `Fact` identically without exposing either
/// private parser as a public API.
#[cfg(test)]
pub(crate) mod tests_consistency_shim {
    use super::{extract_effects_from_fact, DamageCategory, DurationKind, NormalizedEffect};
    use crate::parser_consistency_tests::FactClass;
    use gw2_api::models::Fact;

    pub(crate) fn classify_fact(fact: &Fact) -> Vec<FactClass> {
        let effects = extract_effects_from_fact(fact);
        let mut out = Vec::new();
        for effect in &effects {
            match effect {
                NormalizedEffect::DamageModifier { category, .. } => match category {
                    DamageCategory::Strike => out.push(FactClass::Strike),
                    DamageCategory::Condition => out.push(FactClass::ConditionDamage),
                    DamageCategory::Crit => out.push(FactClass::Crit),
                    DamageCategory::Healing => out.push(FactClass::Healing),
                    DamageCategory::SpecificCondition(c) => {
                        out.push(FactClass::SpecificConditionDamage(c.clone()))
                    }
                },
                NormalizedEffect::DurationBonus { kind, .. } => match kind {
                    DurationKind::AllCondition => out.push(FactClass::AllConditionDuration),
                    DurationKind::AllBoon => out.push(FactClass::AllBoonDuration),
                    DurationKind::SpecificCondition(c) => {
                        out.push(FactClass::SpecificConditionDuration(c.clone()))
                    }
                },
                // Other effect kinds (StatBonus, AppliesStatus, etc.) are not
                // damage/duration modifiers and are out of scope for this
                // consistency table — the combat parser deliberately ignores
                // them, so emitting nothing here keeps the comparison honest.
                _ => {}
            }
        }
        out
    }
}

fn stat_type_from_display_name(name: &str) -> Option<StatType> {
    match name.to_lowercase().as_str() {
        "power" => Some(StatType::Power),
        "precision" => Some(StatType::Precision),
        "toughness" => Some(StatType::Toughness),
        "vitality" => Some(StatType::Vitality),
        "condition damage" | "conditiondamage" => Some(StatType::ConditionDamage),
        "expertise" | "condition duration" => Some(StatType::Expertise),
        "concentration" | "boon duration" => Some(StatType::Concentration),
        "ferocity" | "crit damage" => Some(StatType::Ferocity),
        "healing power" | "healingpower" | "healing" => Some(StatType::HealingPower),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stat_type_from_api() {
        assert_eq!(StatType::from_api("Power"), Some(StatType::Power));
        assert_eq!(
            StatType::from_api("ConditionDuration"),
            Some(StatType::Expertise)
        );
        assert_eq!(StatType::from_api("CritDamage"), Some(StatType::Ferocity));
        assert_eq!(StatType::from_api("AgonyResistance"), None);
    }

    #[test]
    fn template_explanation_dedupes_repeated_blurbs() {
        gw2_core::i18n::set_language("en");
        let link = SynergyLink {
            source: ComponentId::Trait(1),
            source_name: "A".into(),
            target: ComponentId::Trait(2),
            target_name: "B".into(),
            link_type: SynergyLinkType::ModifierStacking,
            score: 1.0,
            description: "Stacking Strike damage modifiers multiply for greater effect."
                .into(),
        };
        let text = template_explanation(&[link.clone(), link.clone(), link], "Valkyrie", "Thief");
        assert!(text.starts_with("This Thief build uses Valkyrie gear."));
        assert_eq!(
            text.matches("Stacking Strike damage modifiers multiply for greater effect.")
                .count(),
            1
        );
    }


    #[test]
    fn all_stats_bonus_emits_nine_attributes() {
        let fx = parse_rune_bonus_to_effects("+8 to All Stats");
        assert_eq!(fx.len(), 9);
        assert!(fx.iter().any(|e| matches!(
            e,
            NormalizedEffect::StatBonus {
                stat: StatType::HealingPower,
                value
            } if (*value - 8.0).abs() < 1e-9
        )));
        assert!(fx.iter().any(|e| matches!(
            e,
            NormalizedEffect::StatBonus {
                stat: StatType::ConditionDamage,
                value
            } if (*value - 8.0).abs() < 1e-9
        )));
    }

    #[test]
    fn test_extract_effects_from_attribute_adjust() {
        let fact = Fact::AttributeAdjust {
            text: Some("Power".into()),
            icon: None,
            value: Some(150),
            target: Some("Power".into()),
        };
        let effects = extract_effects_from_fact(&fact);
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            NormalizedEffect::StatBonus { stat, value } => {
                assert_eq!(*stat, StatType::Power);
                assert!((value - 150.0).abs() < 0.01);
            }
            _ => panic!("Expected StatBonus"),
        }
    }

    #[test]
    fn test_extract_effects_from_percent_damage() {
        let fact = Fact::Percent {
            text: Some("Damage increased: 5%".into()),
            icon: None,
            percent: Some(5.0),
        };
        let effects = extract_effects_from_fact(&fact);
        assert!(!effects.is_empty());
        assert!(matches!(&effects[0], NormalizedEffect::DamageModifier {
            category: DamageCategory::Strike, percent
        } if (*percent - 5.0).abs() < 0.01));
    }

    #[test]
    fn test_extract_effects_condition_damage_without_increase_keyword() {
        // "Condition Damage: +X%" (no literal "increase") must map to the
        // Condition damage category, not be dropped.
        let fact = Fact::Percent {
            text: Some("Condition Damage: +8%".into()),
            icon: None,
            percent: Some(8.0),
        };
        let effects = extract_effects_from_fact(&fact);
        assert!(matches!(&effects[0], NormalizedEffect::DamageModifier {
            category: DamageCategory::Condition, percent
        } if (*percent - 8.0).abs() < 0.01));
    }

    #[test]
    fn test_extract_effects_poison_duration_key_is_canonical() {
        // Regression: a fact "+10% Poison Duration" must store the SpecificCondition
        // key in canonical "Poisoned" form so it matches duration_matches_condition's
        // "Poisoned" lookup. Previously this path stored the raw "Poison" verb form
        // (unlike its sibling rune/description parsers) and the bonus never applied.
        let fact = Fact::Percent {
            text: Some("+10% Poison Duration".into()),
            icon: None,
            percent: Some(10.0),
        };
        let effects = extract_effects_from_fact(&fact);
        let key = effects.iter().find_map(|e| match e {
            NormalizedEffect::DurationBonus {
                kind: DurationKind::SpecificCondition(c),
                ..
            } => Some(c.as_str()),
            _ => None,
        });
        assert_eq!(key, Some("Poisoned"));
    }

    #[test]
    fn test_extract_effects_from_buff() {
        let fact = Fact::Buff {
            text: Some("Bleeding".into()),
            icon: None,
            duration: Some(5),
            status: Some("Bleeding".into()),
            description: None,
            apply_count: Some(2),
        };
        let effects = extract_effects_from_fact(&fact);
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            NormalizedEffect::AppliesStatus {
                status,
                is_condition,
                stacks,
                duration_s,
            } => {
                assert_eq!(status, "Bleeding");
                assert!(*is_condition);
                assert_eq!(*stacks, 2);
                assert_eq!(*duration_s, 5);
            }
            _ => panic!("Expected AppliesStatus"),
        }
    }

    #[test]
    fn test_extract_buff_conversion() {
        let fact = Fact::BuffConversion {
            text: None,
            icon: None,
            source: Some("Toughness".into()),
            percent: Some(7.0),
            target: Some("Power".into()),
        };
        let effects = extract_effects_from_fact(&fact);
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            NormalizedEffect::StatConversion {
                source,
                target,
                percent,
            } => {
                assert_eq!(*source, StatType::Toughness);
                assert_eq!(*target, StatType::Power);
                assert!((percent - 7.0).abs() < 0.01);
            }
            _ => panic!("Expected StatConversion"),
        }
    }

    #[test]
    fn test_parse_rune_bonus_flat_stat() {
        let effects = parse_rune_bonus_to_effects("+175 Power");
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            NormalizedEffect::StatBonus { stat, value } => {
                assert_eq!(*stat, StatType::Power);
                assert!((value - 175.0).abs() < 0.01);
            }
            _ => panic!("Expected StatBonus"),
        }
    }

    #[test]
    fn test_parse_rune_bonus_percent_damage() {
        let effects = parse_rune_bonus_to_effects("+5% damage");
        assert_eq!(effects.len(), 1);
        assert!(matches!(&effects[0], NormalizedEffect::DamageModifier {
            category: DamageCategory::Strike, percent
        } if (*percent - 5.0).abs() < 0.01));
    }

    #[test]
    fn test_parse_rune_bonus_condi_duration() {
        let effects = parse_rune_bonus_to_effects("+7% Burning Duration");
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            NormalizedEffect::DurationBonus { kind, percent } => {
                assert!(matches!(kind, DurationKind::SpecificCondition(c) if c == "Burning"));
                assert!((percent - 7.0).abs() < 0.01);
            }
            _ => panic!("Expected DurationBonus"),
        }
    }

    #[test]
    fn test_score_stat_bonus_power_dps() {
        let w = OptimizationWeights::preset_power_dps();
        let eff = NormalizedEffect::StatBonus {
            stat: StatType::Power,
            value: 150.0,
        };
        let score = score_normalized_effect(&eff, &w);
        assert!(
            score > 0.0,
            "Power stat should have positive score with PowerDPS weights"
        );

        let eff_hp = NormalizedEffect::StatBonus {
            stat: StatType::HealingPower,
            value: 150.0,
        };
        let score_hp = score_normalized_effect(&eff_hp, &w);
        assert!(
            score > score_hp,
            "Power should score higher than Healing with PowerDPS weights"
        );
    }

    #[test]
    fn test_score_damage_modifier_condition() {
        let w = OptimizationWeights::preset_condi_dps();
        let eff = NormalizedEffect::DamageModifier {
            category: DamageCategory::Condition,
            percent: 10.0,
        };
        let score = score_normalized_effect(&eff, &w);
        assert!(score > 0.0);

        let w_power = OptimizationWeights::preset_power_dps();
        let score_power = score_normalized_effect(&eff, &w_power);
        assert!(
            score > score_power,
            "Condi modifier should score higher with condition weights"
        );
    }

    #[test]
    fn test_marginal_synergy_condition_stacking() {
        let w = OptimizationWeights::preset_condi_dps();
        let new_effects = vec![NormalizedEffect::AppliesStatus {
            status: "Burning".into(),
            is_condition: true,
            duration_s: 5,
            stacks: 1,
        }];
        let existing = vec![(
            ComponentId::Trait(100),
            vec![NormalizedEffect::AppliesStatus {
                status: "Burning".into(),
                is_condition: true,
                duration_s: 3,
                stacks: 2,
            }],
        )];
        let new_id = ComponentId::Rune(42);
        let (score, links) = compute_marginal_synergy(&new_effects, &existing, &w, Some(&new_id));
        assert!(
            score > 0.0,
            "Should get synergy bonus for stacking conditions"
        );
        assert!(
            links
                .iter()
                .any(|l| l.target == new_id && l.source == ComponentId::Trait(100)),
            "Link should attribute new effect to its component (Rune 42) targeting trait source"
        );
    }

    #[test]
    fn test_marginal_synergy_duration_alignment() {
        let w = OptimizationWeights::preset_condi_dps();
        let new_effects = vec![NormalizedEffect::DurationBonus {
            kind: DurationKind::SpecificCondition("Burning".into()),
            percent: 10.0,
        }];
        let existing = vec![(
            ComponentId::Trait(100),
            vec![NormalizedEffect::AppliesStatus {
                status: "Burning".into(),
                is_condition: true,
                duration_s: 5,
                stacks: 2,
            }],
        )];
        let new_id = ComponentId::Sigil(7);
        let (score, links) = compute_marginal_synergy(&new_effects, &existing, &w, Some(&new_id));
        assert!(
            score > 0.0,
            "Duration bonus should synergize with matching condition"
        );
        assert!(
            links.iter().any(|l| l.target == new_id),
            "Link target should point to new component (Sigil 7), not existing"
        );
    }

    #[test]
    fn test_marginal_synergy_canonicalized_poisoned() {
        // Regression: rune/sigil extractors used to emit "Poison" while trait
        // extractors emitted canonical "Poisoned". `duration_matches_condition`
        // uses eq_ignore_ascii_case which does NOT equate them — the synergy
        // chain was silently dropped. Both sides now canonicalize on emit.
        let w = OptimizationWeights::preset_condi_dps();
        // Caller hand-builds "Poison" form to simulate the pre-fix bug shape;
        // the canonicalized matchers in compute_marginal_synergy still match
        // because trait extractors now produce "Poisoned" consistently.
        let new_effects = vec![NormalizedEffect::DurationBonus {
            kind: DurationKind::SpecificCondition("Poisoned".into()),
            percent: 10.0,
        }];
        let existing = vec![(
            ComponentId::Trait(200),
            vec![NormalizedEffect::AppliesStatus {
                status: "Poisoned".into(),
                is_condition: true,
                duration_s: 5,
                stacks: 1,
            }],
        )];
        let new_id = ComponentId::Sigil(99);
        let (score, _links) = compute_marginal_synergy(&new_effects, &existing, &w, Some(&new_id));
        assert!(
            score > 0.0,
            "Poisoned-applying trait should synergize with Poison-duration sigil"
        );
    }

    #[test]
    fn test_extract_sigil_force() {
        let sigil = Item {
            id: 1,
            name: "Superior Sigil of Force".into(),
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
        let effects = extract_sigil_effects(&sigil, &crate::balance::BalanceContext::pve());
        assert!(!effects.is_empty());
        assert!(matches!(&effects[0], NormalizedEffect::DamageModifier {
            category: DamageCategory::Strike, percent
        } if (*percent - 5.0).abs() < 0.01));
    }

    #[test]
    fn test_extract_relic_nightmare() {
        let relic = Item {
            id: 1,
            name: "Relic of the Nightmare".into(),
            item_type: "Relic".into(),
            rarity: "Legendary".into(),
            level: 80,
            description: Some(
                "Your elite skill inflicts fear and pulses poison around you.".into(),
            ),
            icon: None,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec![],
            restrictions: vec![],
            details: None,
        };
        let effects = extract_relic_effects(&relic);
        assert!(!effects.iter().any(|e| matches!(
            e,
            NormalizedEffect::DurationBonus {
                kind: DurationKind::AllCondition,
                ..
            }
        )));
        assert!(effects.iter().any(|e| matches!(
            e,
            NormalizedEffect::AppliesStatus {
                status,
                is_condition: true,
                ..
            } if status == "Poisoned"
        )));
    }
}
