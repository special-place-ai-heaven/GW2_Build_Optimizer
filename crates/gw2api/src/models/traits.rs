//! Trait data from /v2/traits.
//! Traits define passive and triggered effects within specialization lines.
//! The `facts` and `traited_facts` fields are critical for understanding
//! synergies, proc conditions, and stat modifications.

use serde::{Deserialize, Serialize};

use super::facts::{deserialize_facts, deserialize_traited_facts, Fact, TraitedFact};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trait {
    pub id: u32,
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub specialization: u32,
    pub tier: u32,
    pub order: u32,
    pub slot: String, // "Major" or "Minor"
    #[serde(default, deserialize_with = "deserialize_facts")]
    pub facts: Vec<Fact>,
    #[serde(default, deserialize_with = "deserialize_traited_facts")]
    pub traited_facts: Vec<TraitedFact>,
    #[serde(default)]
    pub skills: Vec<TraitSkill>,
}

/// A skill triggered by a trait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitSkill {
    pub id: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    #[serde(default, deserialize_with = "deserialize_facts")]
    pub facts: Vec<Fact>,
    #[serde(default, deserialize_with = "deserialize_traited_facts")]
    pub traited_facts: Vec<TraitedFact>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_trait_raging_storm() {
        let json = r#"{
            "id": 214,
            "tier": 2,
            "order": 1,
            "name": "Raging Storm",
            "description": "Critically striking a foe grants fury.",
            "slot": "Major",
            "facts": [
                {"text": "Recharge", "type": "Recharge", "icon": "https://example.com/i.png", "value": 8},
                {"type": "AttributeAdjust", "icon": "https://example.com/i.png", "value": 180, "target": "CritDamage"},
                {"text": "Apply Buff/Condition", "type": "Buff", "icon": "https://example.com/i.png", "duration": 4, "status": "Fury", "description": "Crit chance increased.", "apply_count": 1},
                {"text": "Radius", "type": "Distance", "icon": "https://example.com/i.png", "distance": 360},
                {"text": "Number of Targets", "type": "Number", "icon": "https://example.com/i.png", "value": 5}
            ],
            "specialization": 41,
            "icon": "https://example.com/icon.png"
        }"#;
        let t: Trait = serde_json::from_str(json).unwrap();
        assert_eq!(t.id, 214);
        assert_eq!(t.name, "Raging Storm");
        assert_eq!(t.slot, "Major");
        assert_eq!(t.tier, 2);
        assert_eq!(t.facts.len(), 5);
    }
}
