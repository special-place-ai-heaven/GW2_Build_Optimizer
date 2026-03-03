//! Specialization data from /v2/specializations.
//! Links professions to trait lines (5 core + elite specs per profession).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Specialization {
    pub id: u32,
    pub name: String,
    pub profession: String,
    pub elite: bool,
    pub minor_traits: Vec<u32>,
    pub major_traits: Vec<u32>,
    pub weapon_trait: Option<u32>,
    pub icon: Option<String>,
    pub background: Option<String>,
    pub profession_icon: Option<String>,
    pub profession_icon_big: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_dragonhunter() {
        let json = r#"{
            "id": 27,
            "name": "Dragonhunter",
            "profession": "Guardian",
            "elite": true,
            "minor_traits": [1848, 1896, 1926],
            "major_traits": [1898, 1983, 1911, 2037, 1835, 1943, 1908, 1963, 1955],
            "weapon_trait": 1826,
            "icon": "https://example.com/icon.png",
            "background": "https://example.com/bg.png",
            "profession_icon_big": "https://example.com/big.png",
            "profession_icon": "https://example.com/small.png"
        }"#;
        let spec: Specialization = serde_json::from_str(json).unwrap();
        assert_eq!(spec.id, 27);
        assert_eq!(spec.name, "Dragonhunter");
        assert!(spec.elite);
        assert_eq!(spec.profession, "Guardian");
        assert_eq!(spec.minor_traits.len(), 3);
        assert_eq!(spec.major_traits.len(), 9); // 3 columns x 3 choices
    }
}
