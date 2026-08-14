//! Revenant legend data from /v2/legends.
//! Each legend defines swap skill, heal, elite, and utility skills.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Legend {
    pub id: String,
    /// Build-template legend byte (`/v2/legends` `code`). Schema 2019-12-19+.
    #[serde(default)]
    pub code: Option<u32>,
    pub swap: u32,
    pub heal: u32,
    pub elite: u32,
    pub utilities: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_legend() {
        let json = r#"{
            "id": "Legend1",
            "code": 1,
            "swap": 28085,
            "heal": 27220,
            "elite": 27760,
            "utilities": [28379, 27014, 26644]
        }"#;
        let legend: Legend = serde_json::from_str(json).unwrap();
        assert_eq!(legend.id, "Legend1");
        assert_eq!(legend.code, Some(1));
        assert_eq!(legend.utilities.len(), 3);
    }
}
