//! Deterministic synergy engine: effect taxonomy, extractors, and scoring.
//!
//! This module provides a unified representation for ALL synergy-relevant effects
//! from any build component (traits, runes, sigils, relics, skills). Effects are
//! extracted into `NormalizedEffect` values, then scored against `OptimizationWeights`
//! with interaction bonuses for synergistic combinations.

use gw2_api::models::{Fact, Item, Skill, Trait as GW2Trait};

use crate::scoring::OptimizationWeights;

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
    DamageModifier { category: DamageCategory, percent: f64 },
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
    StatConversion { source: StatType, target: StatType, percent: f64 },
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
pub fn extract_trait_effects(
    t: &GW2Trait,
    equipped_traits: &[u32],
) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();

    // Collect overridden indices from active traited_facts
    let overridden: Vec<u32> = t
        .traited_facts
        .iter()
        .filter(|tf| equipped_traits.contains(&tf.requires_trait))
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
        if equipped_traits.contains(&tf.requires_trait) {
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
pub fn extract_sigil_effects(sigil: &Item) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();
    let name_lower = sigil.name.to_lowercase();

    // Known permanent/high-uptime sigils
    if name_lower.contains("sigil of force") {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Strike,
            percent: 5.0,
        });
    } else if name_lower.contains("sigil of impact") {
        // +3% damage while foe is stunned/knocked down; actual uptime 5-15% in PvE
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnHit,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Strike,
                percent: 3.0,
            }),
            estimated_uptime: 0.1,
        });
    } else if name_lower.contains("sigil of the night") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::Passive,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Strike,
                percent: 10.0,
            }),
            estimated_uptime: 0.5,
        });
    } else if name_lower.contains("sigil of bursting") {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Condition,
            percent: 6.0,
        });
    } else if name_lower.contains("sigil of malice") {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::AllCondition,
            percent: 10.0,
        });
    } else if name_lower.contains("sigil of concentration") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnWeaponSwap,
            effect: Box::new(NormalizedEffect::DurationBonus {
                kind: DurationKind::AllBoon,
                percent: 10.0,
            }),
            estimated_uptime: 0.33,
        });
    } else if name_lower.contains("sigil of smoldering") {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::SpecificCondition("Burning".into()),
            percent: 10.0,
        });
    } else if name_lower.contains("sigil of agony") {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::SpecificCondition("Torment".into()),
            percent: 10.0,
        });
    } else if name_lower.contains("sigil of venom") {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::SpecificCondition("Poison".into()),
            percent: 10.0,
        });
    } else if name_lower.contains("sigil of transference") {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Healing,
            percent: 10.0,
        });
    } else if name_lower.contains("sigil of benevolence") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnKill,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Healing,
                percent: 10.0,
            }),
            estimated_uptime: 0.3,
        });
    } else if name_lower.contains("sigil of earth") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnCrit,
            effect: Box::new(NormalizedEffect::AppliesStatus {
                status: "Bleeding".into(),
                is_condition: true,
                duration_s: 5,
                stacks: 1,
            }),
            estimated_uptime: 0.6,
        });
    } else if name_lower.contains("sigil of doom") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnWeaponSwap,
            effect: Box::new(NormalizedEffect::AppliesStatus {
                status: "Poison".into(),
                is_condition: true,
                duration_s: 5,
                stacks: 1,
            }),
            estimated_uptime: 0.33,
        });
    } else if name_lower.contains("sigil of geomancy") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnWeaponSwap,
            effect: Box::new(NormalizedEffect::AppliesStatus {
                status: "Bleeding".into(),
                is_condition: true,
                duration_s: 5,
                stacks: 3,
            }),
            estimated_uptime: 0.33,
        });
    } else {
        // Try parsing from description
        if let Some(ref desc) = sigil.description {
            effects.extend(parse_description_to_effects(desc));
        }
    }

    effects
}

/// Extract normalized effects from a relic item.
pub fn extract_relic_effects(relic: &Item) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();
    let name_lower = relic.name.to_lowercase();

    if name_lower.contains("relic of the thief") {
        // +1% per boon up to 10%; with good boon coverage ~85% effective uptime at 8-10 stacks
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::Passive,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Strike,
                percent: 10.0,
            }),
            estimated_uptime: 0.85,
        });
    } else if name_lower.contains("relic of fireworks") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnDodge,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Strike,
                percent: 10.0,
            }),
            estimated_uptime: 0.4,
        });
    } else if name_lower.contains("relic of isgarren") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnHealthThreshold,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Crit,
                percent: 10.0,
            }),
            estimated_uptime: 0.85,
        });
    } else if name_lower.contains("relic of the aristocracy") {
        // +1% per boon up to 5%; with good boon coverage ~80% effective uptime at 4-5 stacks
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::Passive,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Strike,
                percent: 5.0,
            }),
            estimated_uptime: 0.8,
        });
    } else if name_lower.contains("relic of cerus") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::Passive,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Condition,
                percent: 5.0,
            }),
            estimated_uptime: 0.5,
        });
    } else if name_lower.contains("relic of the nightmare") {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::AllCondition,
            percent: 10.0,
        });
    } else if name_lower.contains("relic of the krait") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnHit,
            effect: Box::new(NormalizedEffect::AppliesStatus {
                status: "Bleeding".into(),
                is_condition: true,
                duration_s: 4,
                stacks: 1,
            }),
            estimated_uptime: 0.5,
        });
    } else if name_lower.contains("relic of the monk") {
        effects.push(NormalizedEffect::DamageModifier {
            category: DamageCategory::Healing,
            percent: 10.0,
        });
    } else if name_lower.contains("relic of karakosa") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnHealthThreshold,
            effect: Box::new(NormalizedEffect::DamageModifier {
                category: DamageCategory::Healing,
                percent: 10.0,
            }),
            estimated_uptime: 0.8,
        });
    } else if name_lower.contains("relic of nourys") {
        effects.push(NormalizedEffect::ProcEffect {
            trigger: ProcTrigger::OnWeaponSwap,
            effect: Box::new(NormalizedEffect::DurationBonus {
                kind: DurationKind::AllBoon,
                percent: 10.0,
            }),
            estimated_uptime: 0.33,
        });
    } else {
        // Try parsing from description
        if let Some(ref desc) = relic.description {
            effects.extend(parse_description_to_effects(desc));
        }
    }

    effects
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
            let text_lower = text.to_lowercase();
            if text_lower.contains("damage") && !text_lower.contains("condition damage") {
                effects.push(NormalizedEffect::DamageModifier {
                    category: DamageCategory::Strike,
                    percent: *pct,
                });
            } else if text_lower.contains("condition damage") && text_lower.contains("increase") {
                effects.push(NormalizedEffect::DamageModifier {
                    category: DamageCategory::Condition,
                    percent: *pct,
                });
            } else if text_lower.contains("critical damage") || text_lower.contains("crit damage") {
                effects.push(NormalizedEffect::DamageModifier {
                    category: DamageCategory::Crit,
                    percent: *pct,
                });
            } else if text_lower.contains("outgoing healing") {
                effects.push(NormalizedEffect::DamageModifier {
                    category: DamageCategory::Healing,
                    percent: *pct,
                });
            } else if text_lower.contains("boon duration") {
                effects.push(NormalizedEffect::DurationBonus {
                    kind: DurationKind::AllBoon,
                    percent: *pct,
                });
            } else if text_lower.contains("condition duration") {
                effects.push(NormalizedEffect::DurationBonus {
                    kind: DurationKind::AllCondition,
                    percent: *pct,
                });
            }

            // Specific condition duration patterns
            for condi in &["Bleeding", "Burning", "Poison", "Torment", "Confusion"] {
                if text_lower.contains(&condi.to_lowercase()) && text_lower.contains("duration") {
                    effects.push(NormalizedEffect::DurationBonus {
                        kind: DurationKind::SpecificCondition(condi.to_string()),
                        percent: *pct,
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
            let is_cond = is_condition(status);
            effects.push(NormalizedEffect::AppliesStatus {
                status: status.clone(),
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
            let is_cond = is_condition(status);
            effects.push(NormalizedEffect::AppliesStatus {
                status: status.clone(),
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
/// Examples: "+7% Burning Duration", "+5% damage", "+175 Power"
fn parse_rune_bonus_to_effects(bonus: &str) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();
    let s = bonus.trim().to_lowercase();

    // Match "+N <Stat>" patterns (flat stat bonuses)
    if s.starts_with('+') {
        let without_plus = &s[1..];

        // Check for percentage patterns first
        if let Some(pct_idx) = without_plus.find('%') {
            let num_str = &without_plus[..pct_idx];
            let rest = without_plus[pct_idx + 1..].trim();

            if let Ok(value) = num_str.trim().parse::<f64>() {
                // Specific condition duration
                for condi in &["bleeding", "burning", "poison", "torment", "confusion"] {
                    if rest.contains(condi) && rest.contains("duration") {
                        effects.push(NormalizedEffect::DurationBonus {
                            kind: DurationKind::SpecificCondition(capitalize(condi)),
                            percent: value,
                        });
                        return effects;
                    }
                }

                if rest.contains("condition duration") {
                    effects.push(NormalizedEffect::DurationBonus {
                        kind: DurationKind::AllCondition,
                        percent: value,
                    });
                } else if rest.contains("boon duration") {
                    effects.push(NormalizedEffect::DurationBonus {
                        kind: DurationKind::AllBoon,
                        percent: value,
                    });
                } else if rest.contains("condition") && rest.contains("damage") {
                    // "+X% Condition Damage" — percentage bonus to all condition damage output.
                    effects.push(NormalizedEffect::DamageModifier {
                        category: DamageCategory::Condition,
                        percent: value,
                    });
                } else if rest.contains("damage") {
                    effects.push(NormalizedEffect::DamageModifier {
                        category: DamageCategory::Strike,
                        percent: value,
                    });
                } else if rest.contains("healing") {
                    effects.push(NormalizedEffect::DamageModifier {
                        category: DamageCategory::Healing,
                        percent: value,
                    });
                }
            }
        } else {
            // Flat stat bonus: "+175 Power"
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
    }

    effects
}

/// Parse item description text into effects (fallback).
fn parse_description_to_effects(desc: &str) -> Vec<NormalizedEffect> {
    let mut effects = Vec::new();
    let desc_lower = desc.to_lowercase();

    // "+N% <condition> duration"
    for condi in &["bleeding", "burning", "poison", "torment", "confusion"] {
        if let Some(pct) = extract_percent_before(&desc_lower, &format!("{} duration", condi)) {
            effects.push(NormalizedEffect::DurationBonus {
                kind: DurationKind::SpecificCondition(capitalize(condi)),
                percent: pct,
            });
        }
    }

    if let Some(pct) = extract_percent_before(&desc_lower, "condition duration") {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::AllCondition,
            percent: pct,
        });
    }
    if let Some(pct) = extract_percent_before(&desc_lower, "boon duration") {
        effects.push(NormalizedEffect::DurationBonus {
            kind: DurationKind::AllBoon,
            percent: pct,
        });
    }
    if desc_lower.contains("outgoing healing") {
        if let Some(pct) = extract_percent_before(&desc_lower, "outgoing healing") {
            effects.push(NormalizedEffect::DamageModifier {
                category: DamageCategory::Healing,
                percent: pct,
            });
        }
    }
    if desc_lower.contains("damage") && !desc_lower.contains("condition damage") {
        if let Some(pct) = extract_percent_before(&desc_lower, "damage") {
            effects.push(NormalizedEffect::DamageModifier {
                category: DamageCategory::Strike,
                percent: pct,
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
        NormalizedEffect::AppliesStatus { status, is_condition, stacks, .. } => {
            if *is_condition {
                let w = weights.condition;
                // Conditions are valuable for condition builds
                (*stacks as f64).min(5.0) * 0.02 * w
                    + condition_importance(status) * 0.03 * w
            } else {
                // Boons: value depends on boon type
                boon_weight(status, weights) * 0.05
            }
        }
        NormalizedEffect::BenefitsFromStatus { effect, .. } => {
            // Base value of the inner effect, discounted
            score_normalized_effect(effect, weights) * 0.3
        }
        NormalizedEffect::StatConversion { source, target, percent } => {
            let src_w = weight_for_stat(source, weights);
            let tgt_w = weight_for_stat(target, weights);
            // Conversion is good when source stat is high (from gear) and target is useful
            percent / 100.0 * src_w * tgt_w
        }
        NormalizedEffect::DurationBonus { kind, percent } => {
            let w = match kind {
                DurationKind::AllCondition => weights.condition * 0.8 + weights.control * 0.3,
                DurationKind::AllBoon => weights.boon_support * 0.4 + weights.control * 0.2 + weights.healing * 0.3,
                DurationKind::SpecificCondition(_) => weights.condition * 0.5,
            };
            percent / 100.0 * w
        }
        NormalizedEffect::Conditional { effect, .. } => {
            // Conditional effects get partial credit (may or may not be activated)
            score_normalized_effect(effect, weights) * 0.4
        }
        NormalizedEffect::ProcEffect { effect, estimated_uptime, .. } => {
            score_normalized_effect(effect, weights) * estimated_uptime
        }
    }
}

/// Compute the marginal synergy score of adding new_effects to the existing build effects.
/// Detects interaction chains and rewards builds with complementary components.
pub fn compute_marginal_synergy(
    new_effects: &[NormalizedEffect],
    existing_effects: &[(ComponentId, Vec<NormalizedEffect>)],
    weights: &OptimizationWeights,
) -> (f64, Vec<SynergyLink>) {
    let mut synergy = 0.0;
    let mut links = Vec::new();

    // Flatten existing effects for quick scanning
    let all_existing: Vec<(&ComponentId, &NormalizedEffect)> = existing_effects
        .iter()
        .flat_map(|(id, effs)| effs.iter().map(move |e| (id, e)))
        .collect();

    for new_eff in new_effects {
        for &(existing_id, existing_eff) in &all_existing {
            // 1. TraitedFact activation: Conditional requires a trait that is in the build
            if let NormalizedEffect::Conditional { requires_trait_id, effect, .. } = new_eff {
                if let ComponentId::Trait(tid) = existing_id {
                    if *tid == *requires_trait_id {
                        let bonus = score_normalized_effect(effect, weights) * 0.6;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("Trait {}", tid),
                            target: existing_id.clone(),
                            target_name: "Conditional effect".into(),
                            link_type: SynergyLinkType::TraitedFact,
                            score: bonus,
                            description: "Equipped trait activates a conditional effect upgrade.".into(),
                        });
                    }
                }
            }

            // 2. Enabler→Payoff: AppliesStatus meets BenefitsFromStatus
            if let NormalizedEffect::AppliesStatus { status: applied, .. } = new_eff {
                if let NormalizedEffect::BenefitsFromStatus { status: needed, effect } = existing_eff {
                    if applied == needed {
                        let bonus = score_normalized_effect(effect, weights) * 0.5;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("Benefits from {}", needed),
                            target: existing_id.clone(),
                            target_name: format!("Applies {}", applied),
                            link_type: SynergyLinkType::EnablerPayoff,
                            score: bonus,
                            description: format!("{} application enables a bonus effect.", applied),
                        });
                    }
                }
            }
            if let NormalizedEffect::BenefitsFromStatus { status: needed, effect } = new_eff {
                if let NormalizedEffect::AppliesStatus { status: applied, .. } = existing_eff {
                    if applied == needed {
                        let bonus = score_normalized_effect(effect, weights) * 0.5;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("Applies {}", applied),
                            target: existing_id.clone(),
                            target_name: format!("Benefits from {}", needed),
                            link_type: SynergyLinkType::EnablerPayoff,
                            score: bonus,
                            description: format!("Existing {} application feeds into a bonus effect.", applied),
                        });
                    }
                }
            }

            // 3. Condition stacking: multiple sources of same condition
            if let NormalizedEffect::AppliesStatus { status: s1, is_condition: true, .. } = new_eff {
                if let NormalizedEffect::AppliesStatus { status: s2, is_condition: true, .. } = existing_eff {
                    if s1 == s2 {
                        let bonus = condition_importance(s1) * 0.03 * weights.condition;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("{} source", s2),
                            target: existing_id.clone(),
                            target_name: format!("{} source", s1),
                            link_type: SynergyLinkType::ConditionStacking,
                            score: bonus,
                            description: format!("Multiple sources of {} stack for higher sustained damage.", s1),
                        });
                    }
                }
            }

            // 4. Modifier stacking: multiple modifiers in same category interact
            if let NormalizedEffect::DamageModifier { category: c1, percent: p1 } = new_eff {
                if let NormalizedEffect::DamageModifier { category: c2, percent: p2 } = existing_eff {
                    if c1 == c2 {
                        let interaction = (p1 / 100.0) * (p2 / 100.0);
                        let w = weight_for_damage_category(c1, weights);
                        let bonus = interaction * w * 0.5;
                        synergy += bonus;
                        if bonus > 0.001 {
                            links.push(SynergyLink {
                                source: existing_id.clone(),
                                source_name: format!("{:?} +{}%", c2, p2),
                                target: existing_id.clone(),
                                target_name: format!("{:?} +{}%", c1, p1),
                                link_type: SynergyLinkType::ModifierStacking,
                                score: bonus,
                                description: format!("Stacking {:?} damage modifiers multiply for greater effect.", c1),
                            });
                        }
                    }
                }
            }

            // 5. Duration alignment: condition applied + matching duration bonus
            if let NormalizedEffect::AppliesStatus { status, is_condition: true, .. } = new_eff {
                if let NormalizedEffect::DurationBonus { kind, .. } = existing_eff {
                    if duration_matches_condition(kind, status) {
                        let bonus = 0.03 * weights.condition;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("{:?} duration", kind),
                            target: existing_id.clone(),
                            target_name: format!("{} application", status),
                            link_type: SynergyLinkType::DurationAlignment,
                            score: bonus,
                            description: format!("{} duration bonus extends {} ticks for more damage.", status, status),
                        });
                    }
                }
            }
            if let NormalizedEffect::DurationBonus { kind, .. } = new_eff {
                if let NormalizedEffect::AppliesStatus { status, is_condition: true, .. } = existing_eff {
                    if duration_matches_condition(kind, status) {
                        let bonus = 0.03 * weights.condition;
                        synergy += bonus;
                        links.push(SynergyLink {
                            source: existing_id.clone(),
                            source_name: format!("{} application", status),
                            target: existing_id.clone(),
                            target_name: format!("{:?} duration", kind),
                            link_type: SynergyLinkType::DurationAlignment,
                            score: bonus,
                            description: format!("Existing {} application benefits from added duration bonus.", status),
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
    if synergy_links.is_empty() {
        return format!(
            "This {} build uses {} gear for optimal stat distribution. \
             Traits, rune, sigils, and relic were selected to maximize synergy with the chosen weight priorities.",
            profession, gear_prefix,
        );
    }

    let mut parts = Vec::new();
    parts.push(format!(
        "This {} build uses {} gear.", profession, gear_prefix,
    ));

    for link in synergy_links.iter().take(5) {
        parts.push(link.description.clone());
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
        StatType::Concentration => weights.boon_support * 0.5 + weights.control * 0.2 + weights.healing * 0.3,
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

fn condition_importance(status: &str) -> f64 {
    match status {
        "Burning" => 1.0,       // Highest tick damage: 0.155*CD + 131.75
        "Bleeding" => 0.7,      // Stacks to 25: 0.06*CD + 22
        "Torment" => 0.6,       // Stationary: 0.0375*CD + 31.875; moving: 2×
        "Poison" => 0.5,        // 0.06*CD + 33.5, also -33% healing
        "Confusion" => 0.1,     // On-use only: 0.0175*CD + 11 (~10% of Burning DPS)
        "Vulnerability" => 0.8, // Force multiplier (+1% all damage per stack)
        _ => 0.2,               // CC conditions (Immobilize, Chill, etc.)
    }
}

fn boon_weight(status: &str, weights: &OptimizationWeights) -> f64 {
    match status {
        "Might" => weights.power * 0.5 + weights.condition * 0.5,
        "Fury" => weights.power * 0.7,
        "Quickness" => weights.power * 0.5 + weights.condition * 0.3,
        "Alacrity" => weights.boon_support * 0.4 + weights.power * 0.2,
        "Protection" => weights.sustain * 0.6,
        "Resolution" => weights.sustain * 0.4,
        "Regeneration" => weights.healing * 0.4,
        "Vigor" => weights.sustain * 0.3,
        "Stability" => weights.control * 0.3 + weights.sustain * 0.3 + weights.boon_support * 0.2,
        "Swiftness" => 0.05,
        "Resistance" => weights.sustain * 0.4,
        "Aegis" => weights.sustain * 0.5,
        _ => 0.05,
    }
}

fn duration_matches_condition(kind: &DurationKind, condition: &str) -> bool {
    match kind {
        DurationKind::AllCondition => true,
        DurationKind::SpecificCondition(ref c) => c.eq_ignore_ascii_case(condition),
        DurationKind::AllBoon => false,
    }
}

fn is_condition(status: &str) -> bool {
    matches!(
        status,
        "Bleeding" | "Burning" | "Poison" | "Torment" | "Confusion"
            | "Vulnerability" | "Weakness" | "Blind" | "Blinded"
            | "Chill" | "Chilled" | "Cripple" | "Crippled"
            | "Fear" | "Immobilize" | "Immobilized"
            | "Slow" | "Taunt"
    )
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

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// Extract a percentage from text (reused from combat.rs pattern).
fn extract_percent_before(text: &str, keyword: &str) -> Option<f64> {
    if !text.contains(keyword) {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let pct_pos = chars.iter().position(|&c| c == '%')?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stat_type_from_api() {
        assert_eq!(StatType::from_api("Power"), Some(StatType::Power));
        assert_eq!(StatType::from_api("ConditionDuration"), Some(StatType::Expertise));
        assert_eq!(StatType::from_api("CritDamage"), Some(StatType::Ferocity));
        assert_eq!(StatType::from_api("AgonyResistance"), None);
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
            NormalizedEffect::AppliesStatus { status, is_condition, stacks, duration_s } => {
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
            NormalizedEffect::StatConversion { source, target, percent } => {
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
        let eff = NormalizedEffect::StatBonus { stat: StatType::Power, value: 150.0 };
        let score = score_normalized_effect(&eff, &w);
        assert!(score > 0.0, "Power stat should have positive score with PowerDPS weights");

        let eff_hp = NormalizedEffect::StatBonus { stat: StatType::HealingPower, value: 150.0 };
        let score_hp = score_normalized_effect(&eff_hp, &w);
        assert!(score > score_hp, "Power should score higher than Healing with PowerDPS weights");
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
        assert!(score > score_power, "Condi modifier should score higher with condition weights");
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
        let (score, _links) = compute_marginal_synergy(&new_effects, &existing, &w);
        assert!(score > 0.0, "Should get synergy bonus for stacking conditions");
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
        let (score, _links) = compute_marginal_synergy(&new_effects, &existing, &w);
        assert!(score > 0.0, "Duration bonus should synergize with matching condition");
    }

    #[test]
    fn test_extract_sigil_force() {
        let sigil = Item {
            id: 1, name: "Superior Sigil of Force".into(),
            item_type: "UpgradeComponent".into(), rarity: "Exotic".into(),
            level: 60, description: None, icon: None, vendor_value: None,
            chat_link: None, default_skin: None, flags: vec![], game_types: vec![],
            restrictions: vec![], details: None,
        };
        let effects = extract_sigil_effects(&sigil);
        assert!(!effects.is_empty());
        assert!(matches!(&effects[0], NormalizedEffect::DamageModifier {
            category: DamageCategory::Strike, percent
        } if (*percent - 5.0).abs() < 0.01));
    }

    #[test]
    fn test_extract_relic_nightmare() {
        let relic = Item {
            id: 1, name: "Relic of the Nightmare".into(),
            item_type: "Relic".into(), rarity: "Legendary".into(),
            level: 80, description: None, icon: None, vendor_value: None,
            chat_link: None, default_skin: None, flags: vec![], game_types: vec![],
            restrictions: vec![], details: None,
        };
        let effects = extract_relic_effects(&relic);
        assert_eq!(effects.len(), 1);
        assert!(matches!(&effects[0], NormalizedEffect::DurationBonus {
            kind: DurationKind::AllCondition, percent
        } if (*percent - 10.0).abs() < 0.01));
    }
}
