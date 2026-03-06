use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use thiserror::Error;

use super::{DataLoadError, EvidenceLevel};

/// Canonical JSON embedded at compile time from data/profession_profiles.json.
const PROFESSION_PROFILES_JSON: &str =
    include_str!("../../../../data/profession_profiles.json");

static PROFILES: OnceLock<ProfessionProfiles> = OnceLock::new();

/// Returns the globally loaded profession profiles, parsing on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data, should never happen).
pub fn profiles() -> &'static ProfessionProfiles {
    PROFILES.get_or_init(|| {
        load_profession_profiles(PROFESSION_PROFILES_JSON)
            .expect("embedded profession_profiles.json is invalid")
    })
}

/// Try to load profession profiles from the embedded JSON, returning typed errors
/// on failure. Does NOT store in OnceLock — used for health-check validation.
pub fn try_load_profession_profiles() -> Result<ProfessionProfiles, Vec<DataLoadError>> {
    load_profession_profiles(PROFESSION_PROFILES_JSON).map_err(|e| {
        vec![match e {
            ProfessionProfileError::ParseError(pe) => DataLoadError::ParseError {
                source: "profession_profiles".into(),
                detail: pe.to_string(),
            },
            ProfessionProfileError::ValidationError(msg) => DataLoadError::ValidationError {
                source: "profession_profiles".into(),
                field: String::new(),
                reason: msg,
            },
        }]
    })
}

#[derive(Debug, Error)]
pub enum ProfessionProfileError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum ArmorWeightClass {
    Heavy,
    Medium,
    Light,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum HealthClass {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfessionProfile {
    pub profession: String,
    pub armor_weight: ArmorWeightClass,
    pub health_class: HealthClass,
    pub base_health_level_80: i32,
    pub base_defense_level_80: i32,
    pub evidence_level: EvidenceLevel,
    pub sources: Vec<String>,
}

/// O(1) lookup wrapper for loaded profession profiles.
#[derive(Debug)]
pub struct ProfessionProfiles {
    map: HashMap<String, ProfessionProfile>,
}

impl ProfessionProfiles {
    /// Base health at level 80 for a profession (not including vitality).
    pub fn base_health(&self, profession: &str) -> Option<f64> {
        self.map
            .get(profession)
            .map(|p| p.base_health_level_80 as f64)
    }

    /// Base defense at level 80 from armor weight class.
    pub fn base_defense(&self, profession: &str) -> Option<f64> {
        self.map
            .get(profession)
            .map(|p| p.base_defense_level_80 as f64)
    }

    /// Armor weight class name for a profession.
    pub fn armor_weight(&self, profession: &str) -> Option<&str> {
        self.map.get(profession).map(|p| match p.armor_weight {
            ArmorWeightClass::Heavy => "Heavy",
            ArmorWeightClass::Medium => "Medium",
            ArmorWeightClass::Light => "Light",
        })
    }

    /// Get the full profile for a profession.
    pub fn get(&self, profession: &str) -> Option<&ProfessionProfile> {
        self.map.get(profession)
    }

    /// Number of loaded profiles.
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Parse and validate profession profiles from JSON text.
pub fn load_profession_profiles(
    json: &str,
) -> Result<ProfessionProfiles, ProfessionProfileError> {
    let entries: Vec<ProfessionProfile> = serde_json::from_str(json)?;
    validate_profiles(&entries)?;
    let map: HashMap<String, ProfessionProfile> = entries
        .into_iter()
        .map(|p| (p.profession.clone(), p))
        .collect();
    Ok(ProfessionProfiles { map })
}

fn validate_profiles(entries: &[ProfessionProfile]) -> Result<(), ProfessionProfileError> {
    // Exactly 9 professions
    if entries.len() != 9 {
        return Err(ProfessionProfileError::ValidationError(format!(
            "expected 9 professions, got {}",
            entries.len()
        )));
    }

    // No duplicates
    let mut seen = std::collections::HashSet::new();
    for entry in entries {
        if !seen.insert(&entry.profession) {
            return Err(ProfessionProfileError::ValidationError(format!(
                "duplicate profession: {}",
                entry.profession
            )));
        }
    }

    // Defense must match armor class
    for entry in entries {
        let expected_defense = match entry.armor_weight {
            ArmorWeightClass::Heavy => 1271,
            ArmorWeightClass::Medium => 1118,
            ArmorWeightClass::Light => 967,
        };
        if entry.base_defense_level_80 != expected_defense {
            return Err(ProfessionProfileError::ValidationError(format!(
                "{}: base_defense {} does not match {:?} armor (expected {})",
                entry.profession,
                entry.base_defense_level_80,
                entry.armor_weight,
                expected_defense
            )));
        }
    }

    // Health must match health class
    for entry in entries {
        let expected_health = match entry.health_class {
            HealthClass::High => 9212,
            HealthClass::Medium => 5922,
            HealthClass::Low => 1645,
        };
        if entry.base_health_level_80 != expected_health {
            return Err(ProfessionProfileError::ValidationError(format!(
                "{}: base_health {} does not match {:?} health class (expected {})",
                entry.profession,
                entry.base_health_level_80,
                entry.health_class,
                expected_health
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_profiles_load_successfully() {
        let p = profiles();
        assert_eq!(p.len(), 9);
    }

    // Source: https://wiki.guildwars2.com/wiki/Health
    // Guardian is Heavy armor but Low health class (1645 base HP at level 80)
    #[test]
    fn test_guardian_health_class_is_low() {
        let p = profiles();
        let guardian = p.get("Guardian").expect("Guardian not found");
        assert_eq!(guardian.health_class, HealthClass::Low);
        // Wiki: https://wiki.guildwars2.com/wiki/Health — Guardian base HP = 1645
        assert_eq!(guardian.base_health_level_80, 1645);
        assert_eq!(p.base_health("Guardian"), Some(1645.0));
    }

    // Source: https://wiki.guildwars2.com/wiki/Health
    // Necromancer is Light armor but High health class (9212 base HP at level 80)
    #[test]
    fn test_necromancer_health_class_is_high() {
        let p = profiles();
        let necro = p.get("Necromancer").expect("Necromancer not found");
        assert_eq!(necro.health_class, HealthClass::High);
        // Wiki: https://wiki.guildwars2.com/wiki/Health — Necromancer base HP = 9212
        assert_eq!(necro.base_health_level_80, 9212);
        assert_eq!(p.base_health("Necromancer"), Some(9212.0));
    }

    // Source: https://wiki.guildwars2.com/wiki/Health
    // All 9 professions must have correct base health values
    #[test]
    fn test_base_health_values_match_source_of_truth() {
        let p = profiles();
        // High health class (9212): Warrior, Necromancer
        // Wiki: https://wiki.guildwars2.com/wiki/Health
        assert_eq!(p.base_health("Warrior"), Some(9212.0));
        assert_eq!(p.base_health("Necromancer"), Some(9212.0));
        // Medium health class (5922): Revenant, Engineer, Ranger, Mesmer
        assert_eq!(p.base_health("Revenant"), Some(5922.0));
        assert_eq!(p.base_health("Engineer"), Some(5922.0));
        assert_eq!(p.base_health("Ranger"), Some(5922.0));
        assert_eq!(p.base_health("Mesmer"), Some(5922.0));
        // Low health class (1645): Guardian, Thief, Elementalist
        assert_eq!(p.base_health("Guardian"), Some(1645.0));
        assert_eq!(p.base_health("Thief"), Some(1645.0));
        assert_eq!(p.base_health("Elementalist"), Some(1645.0));
    }

    // Source: https://wiki.guildwars2.com/wiki/Armor
    // All 9 professions must have correct base defense values
    #[test]
    fn test_base_defense_values_match_source_of_truth() {
        let p = profiles();
        // Heavy (1271): Warrior, Guardian, Revenant
        // Wiki: https://wiki.guildwars2.com/wiki/Armor
        assert_eq!(p.base_defense("Warrior"), Some(1271.0));
        assert_eq!(p.base_defense("Guardian"), Some(1271.0));
        assert_eq!(p.base_defense("Revenant"), Some(1271.0));
        // Medium (1118): Engineer, Ranger, Thief
        assert_eq!(p.base_defense("Engineer"), Some(1118.0));
        assert_eq!(p.base_defense("Ranger"), Some(1118.0));
        assert_eq!(p.base_defense("Thief"), Some(1118.0));
        // Light (967): Elementalist, Mesmer, Necromancer
        assert_eq!(p.base_defense("Elementalist"), Some(967.0));
        assert_eq!(p.base_defense("Mesmer"), Some(967.0));
        assert_eq!(p.base_defense("Necromancer"), Some(967.0));
    }

    #[test]
    fn test_all_9_professions_present() {
        let p = profiles();
        let expected = [
            "Warrior", "Guardian", "Revenant", "Engineer", "Ranger",
            "Thief", "Elementalist", "Mesmer", "Necromancer",
        ];
        for name in &expected {
            assert!(p.get(name).is_some(), "missing profession: {}", name);
        }
        assert_eq!(p.len(), 9);
    }

    #[test]
    fn test_unknown_profession_returns_none() {
        let p = profiles();
        assert_eq!(p.base_health("FakeProfession"), None);
        assert_eq!(p.base_defense("FakeProfession"), None);
        assert_eq!(p.armor_weight("FakeProfession"), None);
    }

    #[test]
    fn test_duplicate_profession_rejected() {
        // 9 entries total but Warrior appears twice (Necromancer missing) —
        // passes the count check so the HashSet duplicate detection is exercised.
        let json = r#"[
            {"profession":"Warrior","armor_weight":"Heavy","health_class":"High","base_health_level_80":9212,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]},
            {"profession":"Guardian","armor_weight":"Heavy","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]},
            {"profession":"Revenant","armor_weight":"Heavy","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]},
            {"profession":"Engineer","armor_weight":"Medium","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Ranger","armor_weight":"Medium","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Thief","armor_weight":"Medium","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Elementalist","armor_weight":"Light","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]},
            {"profession":"Mesmer","armor_weight":"Light","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]},
            {"profession":"Warrior","armor_weight":"Heavy","health_class":"High","base_health_level_80":9212,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]}
        ]"#;
        let err = load_profession_profiles(json).unwrap_err();
        assert!(
            err.to_string().contains("duplicate profession: Warrior"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_fewer_than_9_rejected() {
        let json = r#"[
            {"profession":"Warrior","armor_weight":"Heavy","health_class":"High","base_health_level_80":9212,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]}
        ]"#;
        let err = load_profession_profiles(json).unwrap_err();
        assert!(err.to_string().contains("expected 9 professions, got 1"));
    }

    #[test]
    fn test_malformed_enum_rejected() {
        let json = r#"[
            {"profession":"Warrior","armor_weight":"SuperHeavy","health_class":"High","base_health_level_80":9212,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]}
        ]"#;
        let err = load_profession_profiles(json).unwrap_err();
        assert!(matches!(err, ProfessionProfileError::ParseError(_)));
    }

    #[test]
    fn test_health_class_mismatch_rejected() {
        // High health class should have 9212, not 5922
        let json = r#"[
            {"profession":"Warrior","armor_weight":"Heavy","health_class":"High","base_health_level_80":5922,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]},
            {"profession":"Guardian","armor_weight":"Heavy","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]},
            {"profession":"Revenant","armor_weight":"Heavy","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]},
            {"profession":"Engineer","armor_weight":"Medium","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Ranger","armor_weight":"Medium","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Thief","armor_weight":"Medium","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Elementalist","armor_weight":"Light","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]},
            {"profession":"Mesmer","armor_weight":"Light","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]},
            {"profession":"Necromancer","armor_weight":"Light","health_class":"High","base_health_level_80":9212,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]}
        ]"#;
        let err = load_profession_profiles(json).unwrap_err();
        assert!(err.to_string().contains("does not match"));
    }

    #[test]
    fn test_defense_armor_class_mismatch_rejected() {
        // Heavy armor should have defense 1271, not 967
        let json = r#"[
            {"profession":"Warrior","armor_weight":"Heavy","health_class":"High","base_health_level_80":9212,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]},
            {"profession":"Guardian","armor_weight":"Heavy","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]},
            {"profession":"Revenant","armor_weight":"Heavy","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1271,"evidence_level":"Factual","sources":[]},
            {"profession":"Engineer","armor_weight":"Medium","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Ranger","armor_weight":"Medium","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Thief","armor_weight":"Medium","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":1118,"evidence_level":"Factual","sources":[]},
            {"profession":"Elementalist","armor_weight":"Light","health_class":"Low","base_health_level_80":1645,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]},
            {"profession":"Mesmer","armor_weight":"Light","health_class":"Medium","base_health_level_80":5922,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]},
            {"profession":"Necromancer","armor_weight":"Light","health_class":"High","base_health_level_80":9212,"base_defense_level_80":967,"evidence_level":"Factual","sources":[]}
        ]"#;
        let err = load_profession_profiles(json).unwrap_err();
        assert!(err.to_string().contains("does not match"));
    }

    #[test]
    fn test_armor_weight_values() {
        let p = profiles();
        // Wiki: https://wiki.guildwars2.com/wiki/Armor
        assert_eq!(p.armor_weight("Warrior"), Some("Heavy"));
        assert_eq!(p.armor_weight("Guardian"), Some("Heavy"));
        assert_eq!(p.armor_weight("Revenant"), Some("Heavy"));
        assert_eq!(p.armor_weight("Engineer"), Some("Medium"));
        assert_eq!(p.armor_weight("Ranger"), Some("Medium"));
        assert_eq!(p.armor_weight("Thief"), Some("Medium"));
        assert_eq!(p.armor_weight("Elementalist"), Some("Light"));
        assert_eq!(p.armor_weight("Mesmer"), Some("Light"));
        assert_eq!(p.armor_weight("Necromancer"), Some("Light"));
    }
}
