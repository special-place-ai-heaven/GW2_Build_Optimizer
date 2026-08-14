//! Profession data from /v2/professions.
//! Defines which weapons and specializations are available per profession,
//! and which weapons are unlocked by elite specializations.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profession {
    pub id: String,
    pub name: String,
    pub code: Option<u32>,
    pub specializations: Vec<u32>,
    pub weapons: HashMap<String, WeaponInfo>,
    #[serde(default)]
    pub training: Vec<Training>,
    /// Palette ID to skill ID mapping. Each entry is [palette_id, skill_id].
    /// Requires API schema version 2019-12-19T00:00:00.000Z or later.
    #[serde(default)]
    pub skills_by_palette: Vec<Vec<u32>>,
    pub icon: Option<String>,
    pub icon_big: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponInfo {
    /// If present, this weapon is only available when this elite spec is equipped.
    pub specialization: Option<u32>,
    pub flags: Vec<String>, // "Mainhand", "Offhand", "TwoHand", "Aquatic"
    #[serde(default)]
    pub skills: Vec<WeaponSkillRef>,
}

impl WeaponInfo {
    /// Underwater-only (Trident, Harpoon Gun, aquatic Spear). Not a land weapon set.
    pub fn is_aquatic(&self) -> bool {
        self.flags.iter().any(|f| f.eq_ignore_ascii_case("Aquatic"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponSkillRef {
    pub id: u32,
    pub slot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Training {
    pub id: u32,
    pub category: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub track: Vec<TrainingTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingTrack {
    pub cost: Option<u32>,
    #[serde(rename = "type")]
    pub track_type: Option<String>,
    pub skill_id: Option<u32>,
    pub trait_id: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_profession_weapons() {
        let json = r#"{
            "id": "Guardian",
            "name": "Guardian",
            "specializations": [42, 16, 13, 49, 46, 27, 62, 65, 81],
            "weapons": {
                "Axe": {
                    "specialization": 62,
                    "flags": ["Mainhand"],
                    "skills": [{"id": 45047, "slot": "Weapon_1"}]
                },
                "Greatsword": {
                    "flags": ["TwoHand"],
                    "skills": [{"id": 9137, "slot": "Weapon_1"}]
                }
            }
        }"#;
        let prof: Profession = serde_json::from_str(json).unwrap();
        assert_eq!(prof.id, "Guardian");
        assert_eq!(prof.specializations.len(), 9); // 5 core + 4 elite
        let axe = &prof.weapons["Axe"];
        assert_eq!(axe.specialization, Some(62)); // Requires Firebrand
        let gs = &prof.weapons["Greatsword"];
        assert_eq!(gs.specialization, None); // Available to all Guardians
    }
}
