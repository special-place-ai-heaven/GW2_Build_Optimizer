//! Character data from /v2/characters (authenticated).
//! Build tabs contain trait/skill selections.
//! Equipment tabs contain gear with stats, runes, sigils, and PvP equipment.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Basic character info from /v2/characters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub profession: String,
    pub level: u32,
    pub race: Option<String>,
    pub gender: Option<String>,
}

/// A build tab from /v2/characters/:id/buildtabs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildTab {
    pub tab: u32,
    pub is_active: bool,
    pub build: Build,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    pub name: Option<String>,
    pub profession: Option<String>,
    pub specializations: Vec<SpecSelection>,
    pub skills: Option<SkillSelection>,
    pub aquatic_skills: Option<SkillSelection>,
    // Revenant-specific
    #[serde(default)]
    pub legends: Vec<Option<String>>,
    #[serde(default)]
    pub aquatic_legends: Vec<Option<String>>,
    // Ranger-specific
    pub pets: Option<PetSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecSelection {
    pub id: Option<u32>,
    pub traits: Vec<Option<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSelection {
    pub heal: Option<u32>,
    pub utilities: Vec<Option<u32>>,
    pub elite: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetSelection {
    #[serde(default)]
    pub terrestrial: Vec<Option<u32>>,
    #[serde(default)]
    pub aquatic: Vec<Option<u32>>,
}

/// An equipment tab from /v2/characters/:id/equipmenttabs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentTab {
    pub tab: u32,
    pub name: Option<String>,
    pub is_active: bool,
    #[serde(default)]
    pub equipment: Vec<EquipmentPiece>,
    pub equipment_pvp: Option<EquipmentPvp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentPiece {
    pub id: u32,
    pub slot: String,
    pub location: Option<String>, // "Equipped" or "Armory"
    pub skin: Option<u32>,
    #[serde(default)]
    pub upgrades: Vec<u32>, // Rune/sigil item IDs
    #[serde(default)]
    pub infusions: Vec<u32>,
    pub binding: Option<String>,
    pub bound_to: Option<String>,
    #[serde(default)]
    pub dyes: Vec<Option<u32>>,
    pub stats: Option<EquipmentStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentStats {
    pub id: u32, // Resolves to /v2/itemstats
    pub attributes: Option<HashMap<String, i32>>,
}

/// PvP-specific equipment (amulet system).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentPvp {
    pub amulet: Option<u32>, // Resolves to /v2/pvp/amulets
    pub rune: Option<u32>,   // Resolves to /v2/items
    #[serde(default)]
    pub sigils: Vec<Option<u32>>, // 4 sigils (2 per weapon set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_build_tab() {
        let json = r#"{
            "tab": 1,
            "is_active": true,
            "build": {
                "name": "Power DPS",
                "profession": "Guardian",
                "specializations": [
                    {"id": 42, "traits": [574, 565, 1686]},
                    {"id": 16, "traits": [566, 567, 604]},
                    {"id": 27, "traits": [1898, 1835, 1955]}
                ],
                "skills": {
                    "heal": 21664,
                    "utilities": [9168, 9093, 9153],
                    "elite": 29965
                },
                "aquatic_skills": {
                    "heal": 21664,
                    "utilities": [9168, 9093, 9153],
                    "elite": 29965
                }
            }
        }"#;
        let tab: BuildTab = serde_json::from_str(json).unwrap();
        assert!(tab.is_active);
        assert_eq!(tab.build.specializations.len(), 3);
        assert_eq!(tab.build.specializations[2].id, Some(27)); // Dragonhunter
    }
}
