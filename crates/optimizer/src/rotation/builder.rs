//! Builds a rotation skill set from a build's skill IDs and GameDb data.
//! Extracts timing, cooldown, and effect information from API skill facts.

use gw2_api::models::facts::Fact;
use gw2_api::models::Skill;

use crate::data::normalized_effects::{EffectCategory, NormalizedEffect};
use crate::gamedb::GameDb;

use super::skill_timings::default_timing;
use super::{RotationSkill, SkillEffect, SkillSlot};

/// Build a list of RotationSkills from skill IDs, looking up data from GameDb.
pub fn build_rotation_skills(skill_ids: &[u32], db: &GameDb) -> Vec<RotationSkill> {
    skill_ids
        .iter()
        .filter_map(|&id| {
            let skill = db.skills.get(&id)?;
            Some(skill_to_rotation(skill))
        })
        .collect()
}

/// Enrich rotation skills with `RemovesCondition` effects, using NormalizedEffects data
/// as the primary source and description text as a fallback heuristic.
///
/// Primary path: scan `ne_effects` for entries with `category == RemovesCondition`
/// whose `source_id` matches a skill's `skill_id`. When found, add a
/// `RemovesCondition` effect carrying the `conditions_removed` count from
/// `status_operation.amount_value` (floored to u32, minimum 1).
///
/// Fallback path: if no NormalizedEffects entry is found for a skill, check
/// the skill's API description (from `db.skills`) for condition-cleanse language:
/// ("remov" AND "condit") OR ("cure" AND "condit"). If matched, add
/// `RemovesCondition { conditions_removed: 1 }` as a heuristic estimate.
///
/// Idempotent per-call: skips any skill that already carries a `RemovesCondition` effect.
pub fn enrich_with_cleanse(
    skills: &mut [RotationSkill],
    ne_effects: &[NormalizedEffect],
    db: &GameDb,
) {
    use crate::data::quality::FactualValue;
    use std::collections::HashMap;

    // Build a fast lookup: source_id → max conditions_removed from NormalizedEffects.
    // A skill may appear as multiple RemovesCondition entries; take the largest amount.
    let mut ne_cleanse: HashMap<u32, u32> = HashMap::new();
    for effect in ne_effects {
        if effect.category == EffectCategory::RemovesCondition {
            let count = match &effect.status_operation {
                Some(op) => match op.amount_value {
                    FactualValue::Resolved(v) => (v.floor() as u32).max(1),
                    FactualValue::Unknown => 1,
                },
                None => 1,
            };
            let entry = ne_cleanse.entry(effect.source_id).or_insert(0);
            *entry = (*entry).max(count);
        }
    }

    for skill in skills.iter_mut() {
        // Skip if already carries a RemovesCondition effect (idempotency guard).
        if skill
            .effects
            .iter()
            .any(|e| matches!(e, SkillEffect::RemovesCondition { .. }))
        {
            continue;
        }

        if let Some(&count) = ne_cleanse.get(&skill.skill_id) {
            // Primary: NormalizedEffects data matched by source_id.
            skill.effects.push(SkillEffect::RemovesCondition {
                conditions_removed: count,
            });
        } else {
            // Fallback: description text heuristic.
            let description = db
                .skills
                .get(&skill.skill_id)
                .and_then(|s| s.description.as_deref())
                .unwrap_or("")
                .to_lowercase();

            if (description.contains("remov") && description.contains("condit"))
                || (description.contains("cure") && description.contains("condit"))
            {
                skill.effects.push(SkillEffect::RemovesCondition {
                    conditions_removed: 1, // HEURISTIC: assume 1 condition removed
                });
            }
        }
    }
}

/// Convert a GW2 API Skill into a RotationSkill with extracted timing and effects.
fn skill_to_rotation(skill: &Skill) -> RotationSkill {
    let slot = skill
        .slot
        .as_deref()
        .and_then(SkillSlot::from_api)
        .unwrap_or(SkillSlot::Utility);

    let timing = default_timing(slot);
    let cooldown_ms = extract_cooldown(&skill.facts);
    let effects = extract_effects(&skill.facts);
    let is_stunbreak = skill.facts.iter().any(|f| {
        matches!(
            f,
            Fact::StunBreak {
                value: Some(true),
                ..
            }
        )
    });

    RotationSkill {
        skill_id: skill.id,
        name: skill.name.clone(),
        slot,
        cast_time_ms: timing.total_ms(),
        cooldown_ms,
        effects,
        next_chain: skill.next_chain,
        is_stunbreak,
        weapon_set: 0, // default; caller can tag with set 1/2 via tag_weapon_set()
    }
}

/// Tag weapon skills in a rotation with their weapon set number.
/// Non-weapon skills (heal/utility/elite/profession) are left at set 0.
pub fn tag_weapon_set(skills: &mut [RotationSkill], weapon_set: u8) {
    for skill in skills.iter_mut() {
        if skill.slot.is_weapon() {
            skill.weapon_set = weapon_set;
        }
    }
}

/// Extract cooldown from Fact::Recharge (seconds → milliseconds).
fn extract_cooldown(facts: &[Fact]) -> u32 {
    for fact in facts {
        if let Fact::Recharge { value: Some(v), .. } = fact {
            return (*v * 1000.0) as u32;
        }
    }
    0 // no cooldown = auto-attack or instant
}

/// Extract all combat-relevant effects from skill facts.
fn extract_effects(facts: &[Fact]) -> Vec<SkillEffect> {
    let mut effects = Vec::new();

    for fact in facts {
        match fact {
            Fact::Damage {
                hit_count,
                dmg_multiplier,
                ..
            } => {
                effects.push(SkillEffect::StrikeDamage {
                    hit_count: hit_count.unwrap_or(1),
                    dmg_multiplier: dmg_multiplier.unwrap_or(1.0),
                });
            }
            Fact::Buff {
                status: Some(status),
                duration,
                apply_count,
                ..
            } => {
                let stacks = apply_count.unwrap_or(1);
                let duration_ms = duration.unwrap_or(0) * 1000;

                if is_damaging_condition(status) {
                    effects.push(SkillEffect::ApplyCondition {
                        condition: status.clone(),
                        stacks,
                        duration_ms,
                    });
                } else {
                    effects.push(SkillEffect::ApplyBuff {
                        buff: status.clone(),
                        stacks,
                        duration_ms,
                    });
                }
            }
            // PrefixedBuff has the same combat-relevant fields as Buff (status, duration,
            // apply_count) but also carries a textual prefix describing the application
            // context (e.g. "To nearby enemies", "On hit"). The effects are identical
            // for simulation purposes — handle them the same way.
            Fact::PrefixedBuff {
                status: Some(status),
                duration,
                apply_count,
                ..
            } => {
                let stacks = apply_count.unwrap_or(1);
                let duration_ms = duration.unwrap_or(0) * 1000;

                if is_damaging_condition(status) {
                    effects.push(SkillEffect::ApplyCondition {
                        condition: status.clone(),
                        stacks,
                        duration_ms,
                    });
                } else {
                    effects.push(SkillEffect::ApplyBuff {
                        buff: status.clone(),
                        stacks,
                        duration_ms,
                    });
                }
            }
            Fact::ComboField {
                field_type: Some(ft),
                ..
            } => {
                effects.push(SkillEffect::ComboField {
                    field_type: ft.clone(),
                });
            }
            _ => {}
        }
    }

    effects
}

/// GW2 damaging conditions (same list as gamedb.rs is_condition).
fn is_damaging_condition(status: &str) -> bool {
    matches!(
        status,
        "Bleeding" | "Burning" | "Poison" | "Torment" | "Confusion"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_skill(id: u32, name: &str, slot: &str, facts: Vec<Fact>) -> Skill {
        Skill {
            id,
            name: name.to_string(),
            description: None,
            icon: None,
            chat_link: None,
            skill_type: None,
            weapon_type: None,
            professions: vec!["Warrior".to_string()],
            slot: Some(slot.to_string()),
            facts,
            traited_facts: vec![],
            categories: vec![],
            attunement: None,
            cost: None,
            dual_wield: None,
            flip_skill: None,
            initiative: None,
            next_chain: None,
            prev_chain: None,
            transform_skills: vec![],
            bundle_skills: vec![],
            toolbelt_skill: None,
            flags: vec![],
            specialization: None,
        }
    }

    #[test]
    fn test_extract_cooldown() {
        let facts = vec![Fact::Recharge {
            text: Some("Recharge".into()),
            icon: None,
            value: Some(8.0),
        }];
        assert_eq!(extract_cooldown(&facts), 8000);
    }

    #[test]
    fn test_extract_cooldown_missing() {
        let facts = vec![Fact::Damage {
            text: None,
            icon: None,
            hit_count: Some(1),
            dmg_multiplier: Some(1.0),
        }];
        assert_eq!(extract_cooldown(&facts), 0);
    }

    #[test]
    fn test_extract_effects_damage() {
        let facts = vec![Fact::Damage {
            text: None,
            icon: None,
            hit_count: Some(3),
            dmg_multiplier: Some(1.5),
        }];
        let effects = extract_effects(&facts);
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            SkillEffect::StrikeDamage {
                hit_count,
                dmg_multiplier,
            } => {
                assert_eq!(*hit_count, 3);
                assert!((dmg_multiplier - 1.5).abs() < 0.01);
            }
            _ => panic!("Expected StrikeDamage"),
        }
    }

    #[test]
    fn test_extract_effects_condition() {
        let facts = vec![Fact::Buff {
            text: None,
            icon: None,
            status: Some("Bleeding".into()),
            duration: Some(6),
            apply_count: Some(2),
            description: None,
        }];
        let effects = extract_effects(&facts);
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            SkillEffect::ApplyCondition {
                condition,
                stacks,
                duration_ms,
            } => {
                assert_eq!(condition, "Bleeding");
                assert_eq!(*stacks, 2);
                assert_eq!(*duration_ms, 6000);
            }
            _ => panic!("Expected ApplyCondition"),
        }
    }

    #[test]
    fn test_extract_effects_buff() {
        let facts = vec![Fact::Buff {
            text: None,
            icon: None,
            status: Some("Might".into()),
            duration: Some(10),
            apply_count: Some(3),
            description: None,
        }];
        let effects = extract_effects(&facts);
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            SkillEffect::ApplyBuff {
                buff,
                stacks,
                duration_ms,
            } => {
                assert_eq!(buff, "Might");
                assert_eq!(*stacks, 3);
                assert_eq!(*duration_ms, 10000);
            }
            _ => panic!("Expected ApplyBuff"),
        }
    }

    #[test]
    fn test_extract_effects_prefixed_buff_condition() {
        // PrefixedBuff is used by AoE and on-hit effects; must be handled same as Buff.
        use gw2_api::models::facts::BuffPrefix;
        let facts = vec![Fact::PrefixedBuff {
            text: None,
            icon: None,
            status: Some("Bleeding".into()),
            duration: Some(5),
            apply_count: Some(3),
            description: None,
            prefix: Some(BuffPrefix {
                text: Some("To nearby enemies".into()),
                icon: None,
                status: None,
                description: None,
            }),
        }];
        let effects = extract_effects(&facts);
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            SkillEffect::ApplyCondition {
                condition,
                stacks,
                duration_ms,
            } => {
                assert_eq!(condition, "Bleeding");
                assert_eq!(*stacks, 3);
                assert_eq!(*duration_ms, 5000);
            }
            _ => panic!("Expected ApplyCondition from PrefixedBuff"),
        }
    }

    #[test]
    fn test_skill_to_rotation() {
        let skill = make_test_skill(
            100,
            "Chop",
            "Weapon_1",
            vec![
                Fact::Damage {
                    text: None,
                    icon: None,
                    hit_count: Some(1),
                    dmg_multiplier: Some(0.8),
                },
                Fact::Recharge {
                    text: None,
                    icon: None,
                    value: Some(0.0),
                },
            ],
        );
        let rs = skill_to_rotation(&skill);
        assert_eq!(rs.skill_id, 100);
        assert_eq!(rs.name, "Chop");
        assert_eq!(rs.slot, SkillSlot::Weapon1);
        assert_eq!(rs.cooldown_ms, 0);
        assert_eq!(rs.effects.len(), 1);
    }

    #[test]
    fn test_stunbreak_detection() {
        let skill = make_test_skill(
            200,
            "Shake It Off!",
            "Utility",
            vec![Fact::StunBreak {
                text: Some("Stun Break".into()),
                icon: None,
                value: Some(true),
            }],
        );
        let rs = skill_to_rotation(&skill);
        assert!(rs.is_stunbreak);
    }

    // ─── Tests for enrich_with_cleanse ───

    use crate::data::normalized_effects::{
        AmountMode, EffectCategory, NormalizedEffect, OperationType,
        SourceType, StackingRule, StatusOperation, TargetScope, TargetSide, TriggerRule,
        UptimeModel, UptimeModelKind,
    };
    use crate::data::quality::FactualValue;
    use crate::data::EvidenceLevel;
    use crate::gamedb::GameDb;
    use std::collections::HashMap;

    /// Build a minimal empty GameDb for test purposes.
    fn empty_db() -> GameDb {
        GameDb {
            items: HashMap::new(),
            itemstats: HashMap::new(),
            skills: HashMap::new(),
            traits: HashMap::new(),
            specializations: HashMap::new(),
            professions: HashMap::new(),
            legends: HashMap::new(),
            pvp_amulets: HashMap::new(),
            skills_by_profession: HashMap::new(),
            traits_by_spec: HashMap::new(),
            items_by_type: HashMap::new(),
            runes: vec![],
            sigils: vec![],
            relics: vec![],
            skill_to_palette: HashMap::new(),
            palette_to_skill: HashMap::new(),
            traits_by_condition: HashMap::new(),
            skills_by_condition: HashMap::new(),
            traits_by_buff: HashMap::new(),
            skills_by_buff: HashMap::new(),
        }
    }

    /// Build a minimal `NormalizedEffect` with `RemovesCondition` for a given skill ID and count.
    fn cleanse_ne(source_id: u32, count: f64) -> NormalizedEffect {
        NormalizedEffect {
            effect_id: format!("skill:{}:cleanse", source_id),
            source_type: SourceType::Skill,
            source_id,
            source_name: format!("Skill {}", source_id),
            category: EffectCategory::RemovesCondition,
            value: FactualValue::Resolved(count),
            stacking_rule: StackingRule::NonStacking,
            trigger_rule: TriggerRule::OnSkillUse,
            uptime_model: UptimeModel {
                kind: UptimeModelKind::Unknown,
                uptime: None,
            },
            evidence_level: EvidenceLevel::Factual,
            source: None,
            effect_duration: None,
            internal_cooldown: None,
            max_stacks: None,
            status_operation: Some(StatusOperation {
                operation_type: OperationType::RemovesCondition,
                target_side: TargetSide::Self_,
                status_kind: "Any".to_string(),
                amount_mode: AmountMode::Count,
                amount_value: FactualValue::Resolved(count),
                base_duration_ms: None,
                target_scope: TargetScope::Self_,
                target_count: None,
                internal_cooldown_ms: None,
                source_duration_multiplier: None,
            }),
            inner_category: None,
        }
    }

    /// Build a minimal rotation skill for cleanse tests.
    fn cleanse_test_skill(id: u32) -> RotationSkill {
        RotationSkill {
            skill_id: id,
            name: format!("Skill {}", id),
            slot: SkillSlot::Utility,
            cast_time_ms: 500,
            cooldown_ms: 20000,
            effects: vec![],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        }
    }

    #[test]
    fn test_enrich_with_cleanse_ne_primary() {
        // NormalizedEffects has a RemovesCondition entry → should add effect.
        let mut skills = vec![cleanse_test_skill(9158)];
        let ne = vec![cleanse_ne(9158, 3.0)];
        let db = empty_db();

        enrich_with_cleanse(&mut skills, &ne, &db);

        let cleanse_effects: Vec<_> = skills[0]
            .effects
            .iter()
            .filter_map(|e| {
                if let SkillEffect::RemovesCondition { conditions_removed } = e {
                    Some(*conditions_removed)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(cleanse_effects, vec![3], "should detect 3 conditions removed from NE data");
    }

    #[test]
    fn test_enrich_with_cleanse_description_fallback() {
        // No NE entry, but skill description contains "removes condition" phrasing.
        let mut skill_entry = make_test_skill(500, "Mending", "Heal", vec![]);
        skill_entry.description = Some("Cure conditions affecting you.".to_string());

        let mut db = empty_db();
        db.skills.insert(500, skill_entry);

        let mut skills = vec![cleanse_test_skill(500)];
        enrich_with_cleanse(&mut skills, &[], &db);

        let has_cleanse = skills[0]
            .effects
            .iter()
            .any(|e| matches!(e, SkillEffect::RemovesCondition { conditions_removed: 1 }));
        assert!(has_cleanse, "description heuristic should detect cleanse from 'cure...condition'");
    }

    #[test]
    fn test_enrich_with_cleanse_no_match() {
        // No NE entry, no matching description → no cleanse effect added.
        let mut db = empty_db();
        let mut skill_entry = make_test_skill(999, "Fireball", "Utility", vec![]);
        skill_entry.description = Some("Deal fire damage.".to_string());
        db.skills.insert(999, skill_entry);

        let mut skills = vec![cleanse_test_skill(999)];
        enrich_with_cleanse(&mut skills, &[], &db);

        let has_cleanse = skills[0]
            .effects
            .iter()
            .any(|e| matches!(e, SkillEffect::RemovesCondition { .. }));
        assert!(!has_cleanse, "no cleanse effect for non-cleanse skill");
    }

    #[test]
    fn test_enrich_with_cleanse_idempotent() {
        // Calling enrich twice should not add duplicate cleanse effects.
        let mut skills = vec![cleanse_test_skill(9158)];
        let ne = vec![cleanse_ne(9158, 3.0)];
        let db = empty_db();

        enrich_with_cleanse(&mut skills, &ne, &db);
        enrich_with_cleanse(&mut skills, &ne, &db);

        let cleanse_count = skills[0]
            .effects
            .iter()
            .filter(|e| matches!(e, SkillEffect::RemovesCondition { .. }))
            .count();
        assert_eq!(cleanse_count, 1, "idempotency: only one RemovesCondition effect");
    }

    #[test]
    fn test_enrich_with_cleanse_max_across_multiple_ne_entries() {
        // Multiple NE entries for same source_id → take maximum conditions_removed.
        let mut skills = vec![cleanse_test_skill(100)];
        let ne = vec![cleanse_ne(100, 1.0), cleanse_ne(100, 5.0), cleanse_ne(100, 2.0)];
        let db = empty_db();

        enrich_with_cleanse(&mut skills, &ne, &db);

        let max_count = skills[0].effects.iter().find_map(|e| {
            if let SkillEffect::RemovesCondition { conditions_removed } = e {
                Some(*conditions_removed)
            } else {
                None
            }
        });
        assert_eq!(max_count, Some(5), "should take maximum conditions_removed across entries");
    }
}
