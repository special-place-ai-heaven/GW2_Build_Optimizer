//! PvP amulet data from /v2/pvp/amulets.
//! In PvP, stats come from the amulet rather than gear.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PvpAmulet {
    pub id: u32,
    pub name: String,
    pub icon: Option<String>,
    pub attributes: HashMap<String, i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_pvp_amulet() {
        let json = r#"{
            "id": 4,
            "name": "Assassin Amulet",
            "icon": "https://example.com/icon.png",
            "attributes": {
                "Precision": 1200,
                "Power": 900,
                "CritDamage": 900
            }
        }"#;
        let amulet: PvpAmulet = serde_json::from_str(json).unwrap();
        assert_eq!(amulet.id, 4);
        assert_eq!(amulet.name, "Assassin Amulet");
        assert_eq!(amulet.attributes["Power"], 900);
        assert_eq!(amulet.attributes["Precision"], 1200);
    }
}
