use crate::validation::ValidatedBuild;

/// Canonical full-build state for deterministic search and evaluation.
///
/// This is the state the optimizer should search over. It is intentionally
/// complete: every build-affecting choice is represented explicitly so the
/// deterministic referee can evaluate whole builds rather than partial stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildGenome {
    pub profession_name: String,
    pub spec_ids: Vec<u32>,
    pub elite_spec_id: Option<u32>,
    pub major_trait_ids: Vec<u32>,
    pub all_trait_ids: Vec<u32>,
    pub gear_prefix_id: Option<u32>,
    pub gear_prefix_name: Option<String>,
    pub rune_id: Option<u32>,
    pub sigil_ids: Vec<u32>,
    pub relic_id: Option<u32>,
    pub weapon_set1: WeaponSetGenome,
    pub weapon_set2: WeaponSetGenome,
    pub skills: SkillLoadoutGenome,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WeaponSetGenome {
    pub main_hand: Option<String>,
    pub off_hand: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillLoadoutGenome {
    pub heal_id: Option<u32>,
    pub utility_ids: Vec<u32>,
    pub elite_id: Option<u32>,
}

impl BuildGenome {
    pub fn from_validated(profession_name: impl Into<String>, validated: &ValidatedBuild) -> Self {
        let profession_name = profession_name.into();
        let spec_ids: Vec<u32> = validated
            .specializations
            .iter()
            .map(|spec| spec.spec_id)
            .collect();
        let elite_spec_id = validated
            .specializations
            .iter()
            .find(|spec| spec.elite)
            .map(|spec| spec.spec_id);
        let major_trait_ids: Vec<u32> = validated
            .specializations
            .iter()
            .flat_map(|spec| spec.trait_ids.iter().copied())
            .collect();
        let all_trait_ids: Vec<u32> = validated
            .specializations
            .iter()
            .flat_map(|spec| spec.all_trait_ids.iter().copied())
            .collect();

        Self {
            profession_name,
            spec_ids,
            elite_spec_id,
            major_trait_ids,
            all_trait_ids,
            gear_prefix_id: validated
                .gear_prefix
                .as_ref()
                .map(|prefix| prefix.itemstat_id),
            gear_prefix_name: validated
                .gear_prefix
                .as_ref()
                .map(|prefix| prefix.name.clone()),
            rune_id: validated.rune.as_ref().map(|rune| rune.id),
            sigil_ids: validated.sigils.iter().map(|sigil| sigil.id).collect(),
            relic_id: validated.relic.as_ref().map(|relic| relic.id),
            weapon_set1: WeaponSetGenome {
                main_hand: validated.weapons.set1.main_hand.clone(),
                off_hand: validated.weapons.set1.off_hand.clone(),
            },
            weapon_set2: WeaponSetGenome {
                main_hand: validated.weapons.set2.main_hand.clone(),
                off_hand: validated.weapons.set2.off_hand.clone(),
            },
            skills: SkillLoadoutGenome {
                heal_id: validated.skills.heal.as_ref().map(|(id, _)| *id),
                utility_ids: validated
                    .skills
                    .utilities
                    .iter()
                    .filter_map(|skill| skill.as_ref().map(|(id, _)| *id))
                    .collect(),
                elite_id: validated.skills.elite.as_ref().map(|(id, _)| *id),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BuildGenome;
    use crate::validation::{
        ValidatedBuild, ValidatedGearPrefix, ValidatedItem, ValidatedSkills, ValidatedSpec,
        ValidatedWeaponSet, ValidatedWeapons,
    };

    #[test]
    fn build_genome_from_validated_extracts_complete_state() {
        let validated = ValidatedBuild {
            specializations: vec![
                ValidatedSpec {
                    spec_id: 1,
                    name: "Core".into(),
                    elite: false,
                    trait_ids: vec![11, 12, 13],
                    trait_names: vec![],
                    all_trait_ids: vec![101, 102, 11, 12, 13],
                },
                ValidatedSpec {
                    spec_id: 2,
                    name: "Elite".into(),
                    elite: true,
                    trait_ids: vec![21, 22, 23],
                    trait_names: vec![],
                    all_trait_ids: vec![201, 202, 21, 22, 23],
                },
            ],
            weapons: ValidatedWeapons {
                set1: ValidatedWeaponSet {
                    main_hand: Some("Sword".into()),
                    off_hand: Some("Focus".into()),
                },
                set2: ValidatedWeaponSet {
                    main_hand: Some("Staff".into()),
                    off_hand: None,
                },
            },
            skills: ValidatedSkills {
                heal: Some((1001, "Heal".into())),
                utilities: vec![
                    Some((2001, "Utility A".into())),
                    None,
                    Some((2003, "Utility C".into())),
                ],
                elite: Some((3001, "Elite".into())),
            },
            rune: Some(ValidatedItem {
                id: 4001,
                name: "Rune".into(),
            }),
            sigils: vec![
                ValidatedItem {
                    id: 5001,
                    name: "Sigil A".into(),
                },
                ValidatedItem {
                    id: 5002,
                    name: "Sigil B".into(),
                },
            ],
            relic: Some(ValidatedItem {
                id: 6001,
                name: "Relic".into(),
            }),
            gear_prefix: Some(ValidatedGearPrefix {
                itemstat_id: 7001,
                name: "Berserker".into(),
            }),
            explanation: String::new(),
            synergy_explanation: String::new(),
            changes: vec![],
            warnings: vec![],
            errors: vec![],
        };

        let genome = BuildGenome::from_validated("Guardian", &validated);
        assert_eq!(genome.profession_name, "Guardian");
        assert_eq!(genome.spec_ids, vec![1, 2]);
        assert_eq!(genome.elite_spec_id, Some(2));
        assert_eq!(genome.major_trait_ids, vec![11, 12, 13, 21, 22, 23]);
        assert_eq!(
            genome.all_trait_ids,
            vec![101, 102, 11, 12, 13, 201, 202, 21, 22, 23]
        );
        assert_eq!(genome.gear_prefix_id, Some(7001));
        assert_eq!(genome.rune_id, Some(4001));
        assert_eq!(genome.sigil_ids, vec![5001, 5002]);
        assert_eq!(genome.relic_id, Some(6001));
        assert_eq!(genome.weapon_set1.main_hand.as_deref(), Some("Sword"));
        assert_eq!(genome.weapon_set1.off_hand.as_deref(), Some("Focus"));
        assert_eq!(genome.weapon_set2.main_hand.as_deref(), Some("Staff"));
        assert_eq!(genome.skills.heal_id, Some(1001));
        assert_eq!(genome.skills.utility_ids, vec![2001, 2003]);
        assert_eq!(genome.skills.elite_id, Some(3001));
    }
}
