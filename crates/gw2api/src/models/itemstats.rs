//! Item stat combinations (e.g., Berserker's, Viper's).
//! Defines how gear stats are calculated: `attribute_adjustment * multiplier + value`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStat {
    pub id: u32,
    pub name: String,
    pub attributes: Vec<StatAttribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatAttribute {
    pub attribute: String,
    pub multiplier: f64,
    pub value: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_berserkers() {
        let json = r#"{
            "id": 584,
            "name": "Berserker's",
            "attributes": [
                {"attribute": "Power", "multiplier": 0.35, "value": 32},
                {"attribute": "Precision", "multiplier": 0.25, "value": 18},
                {"attribute": "CritDamage", "multiplier": 0.25, "value": 18}
            ]
        }"#;
        let stat: ItemStat = serde_json::from_str(json).unwrap();
        assert_eq!(stat.id, 584);
        assert_eq!(stat.name, "Berserker's");
        assert_eq!(stat.attributes.len(), 3);
        assert_eq!(stat.attributes[0].attribute, "Power");
        assert!((stat.attributes[0].multiplier - 0.35).abs() < f64::EPSILON);
        assert_eq!(stat.attributes[0].value, 32);
    }
}
