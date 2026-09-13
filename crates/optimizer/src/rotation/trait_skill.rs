//! NeedsMechanic Engine E2: trait-skill cast scheduler.
//!
//! Trait-skill is NOT a bus event. When a trait record with `cast_skill_id`
//! fires on an existing TriggerRule (OnElite / OnSkillUse / …), call
//! [`resolve_trait_skill`] and apply the returned [`SkillEffect`]s on the
//! existing apply path. Lesser resolve does not emit OnElite / OnDisableFoe
//! unless those events actually land via CrowdControl / elite cast.

use std::collections::HashMap;

use crate::data::normalized_effects::{OperationType, StatusOperation, TargetSide};
use crate::data::quality::FactualValue;
use crate::rotation::{CoverKind, RotationSkill, SkillEffect};

/// Look up SkillEffects for a trait-cast lesser skill id.
/// Shared by wvw_timeline and simulator. Returns None when the catalog has
/// no entry — callers must not invent hardcoded trait→skill maps.
pub fn resolve_trait_skill(
    skill_id: u32,
    catalog: &HashMap<u32, Vec<SkillEffect>>,
) -> Option<&[SkillEffect]> {
    catalog.get(&skill_id).map(Vec::as_slice)
}

/// Seed a cast catalog from bar / injected RotationSkills.
pub fn catalog_from_skills(skills: &[RotationSkill]) -> HashMap<u32, Vec<SkillEffect>> {
    let mut map = HashMap::new();
    for skill in skills {
        map.insert(skill.skill_id, skill.effects.clone());
    }
    map
}

/// Convert a status-operation payload (lesser skill outcome on the trait
/// record) into SkillEffects for the cast catalog.
pub fn skill_effects_from_status_operation(op: &StatusOperation) -> Vec<SkillEffect> {
    let stacks = match &op.amount_value {
        FactualValue::Resolved(v) => (*v).max(1.0).round() as u32,
        FactualValue::Unknown => 1,
    };
    let duration_ms = match &op.base_duration_ms {
        Some(FactualValue::Resolved(ms)) => *ms,
        _ => 1_000,
    };
    match (&op.operation_type, &op.target_side) {
        (OperationType::AppliesBoon, TargetSide::Self_ | TargetSide::Ally) => {
            if let Some((kind, strippable)) = cover_kind_for_status(&op.status_kind) {
                let mut out = vec![SkillEffect::Cover {
                    kind,
                    duration_ms,
                    strippable,
                }];
                // Keep the named buff for causal / uptime reads (Arcane Shield,
                // Enduring Pain) alongside cover when both matter.
                if !op.status_kind.eq_ignore_ascii_case("Protection")
                    && !op.status_kind.eq_ignore_ascii_case("Aegis")
                    && !op.status_kind.eq_ignore_ascii_case("Stability")
                    && !op.status_kind.eq_ignore_ascii_case("Resistance")
                {
                    out.push(SkillEffect::ApplyBuff {
                        buff: op.status_kind.clone(),
                        stacks,
                        duration_ms,
                    });
                }
                out
            } else {
                vec![SkillEffect::ApplyBuff {
                    buff: op.status_kind.clone(),
                    stacks,
                    duration_ms,
                }]
            }
        }
        (OperationType::AppliesCondition, TargetSide::Enemy) => {
            vec![SkillEffect::ApplyCondition {
                condition: op.status_kind.clone(),
                stacks,
                duration_ms,
            }]
        }
        (OperationType::RemovesCondition, TargetSide::Self_ | TargetSide::Ally) => {
            vec![SkillEffect::RemovesCondition {
                conditions_removed: stacks,
            }]
        }
        (OperationType::ConvertsConditionToBoon, TargetSide::Self_ | TargetSide::Ally) => {
            vec![SkillEffect::ConvertConditions]
        }
        (OperationType::RemovesBoon | OperationType::CorruptsBoon, TargetSide::Enemy) => {
            vec![SkillEffect::CorruptBoons]
        }
        _ => Vec::new(),
    }
}

fn cover_kind_for_status(status: &str) -> Option<(CoverKind, bool)> {
    if status.eq_ignore_ascii_case("Distortion")
        || status.eq_ignore_ascii_case("Invulnerability")
        || status.eq_ignore_ascii_case("Determined")
        || status.eq_ignore_ascii_case("Enduring Pain")
    {
        Some((CoverKind::Invulnerability, false))
    } else if status.eq_ignore_ascii_case("Arcane Shield") {
        Some((CoverKind::Block, false))
    } else if status.eq_ignore_ascii_case("Aegis") {
        Some((CoverKind::Aegis, true))
    } else if status.eq_ignore_ascii_case("Stability") {
        Some((CoverKind::Stability, true))
    } else if status.eq_ignore_ascii_case("Resistance") {
        Some((CoverKind::Resistance, true))
    } else if status.eq_ignore_ascii_case("Protection") {
        Some((CoverKind::Protection, true))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::normalized_effects::{AmountMode, TargetScope};

    fn op_arcane() -> StatusOperation {
        StatusOperation {
            operation_type: OperationType::AppliesBoon,
            target_side: TargetSide::Self_,
            status_kind: "Arcane Shield".into(),
            amount_mode: AmountMode::Stacks,
            amount_value: FactualValue::Resolved(3.0),
            base_duration_ms: Some(FactualValue::Resolved(5_000)),
            target_scope: TargetScope::Self_,
            target_count: None,
            internal_cooldown_ms: None,
            source_duration_multiplier: None,
        }
    }

    #[test]
    fn resolve_trait_skill_returns_catalog_entry() {
        let effects = skill_effects_from_status_operation(&op_arcane());
        let mut catalog = HashMap::new();
        catalog.insert(25_579, effects.clone());
        let got = resolve_trait_skill(25_579, &catalog).expect("catalog hit");
        assert_eq!(got, effects.as_slice());
        assert!(resolve_trait_skill(1, &catalog).is_none());
    }

    #[test]
    fn arcane_shield_maps_to_block_cover_and_buff() {
        let effects = skill_effects_from_status_operation(&op_arcane());
        assert!(
            effects.iter().any(|e| matches!(
                e,
                SkillEffect::Cover {
                    kind: CoverKind::Block,
                    ..
                }
            )),
            "{effects:?}"
        );
        assert!(
            effects.iter().any(|e| matches!(
                e,
                SkillEffect::ApplyBuff { buff, .. } if buff == "Arcane Shield"
            )),
            "{effects:?}"
        );
    }
}
