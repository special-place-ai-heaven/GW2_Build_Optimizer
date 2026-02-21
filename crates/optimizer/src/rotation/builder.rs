//! Builds a rotation skill set from a build's skill IDs and GameDb data.
//! Extracts timing, cooldown, and effect information from API skill facts.

use gw2_api::models::facts::Fact;
use gw2_api::models::Skill;

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
        matches!(f, Fact::StunBreak { value: Some(true), .. })
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
        "Bleeding"
            | "Burning"
            | "Poison"
            | "Torment"
            | "Confusion"
            | "Vulnerability"
            | "Weakness"
            | "Blind"
            | "Blinded"
            | "Chill"
            | "Chilled"
            | "Cripple"
            | "Crippled"
            | "Fear"
            | "Immobilize"
            | "Immobilized"
            | "Slow"
            | "Taunt"
    )
}

/// Build rotation skills for an entire weapon set + utility bar from a profession's skills.
/// Filters by profession name and collects weapon skills + heal/utility/elite.
pub fn build_profession_rotation(
    profession: &str,
    skill_ids: &[u32],
    db: &GameDb,
) -> Vec<RotationSkill> {
    let mut rotation = Vec::new();

    // First add weapon skills (from skill_ids that are weapon-slot skills)
    for &id in skill_ids {
        if let Some(skill) = db.skills.get(&id) {
            if skill.professions.contains(&profession.to_string()) || skill.professions.is_empty() {
                let rs = skill_to_rotation(skill);
                rotation.push(rs);
            }
        }
    }

    rotation
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
}
