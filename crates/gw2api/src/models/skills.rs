//! Skill data from /v2/skills.
//! Skills define abilities — their damage multipliers, cooldowns, costs,
//! buff/condition applications, and how they change with traits (traited_facts).

use serde::{Deserialize, Serialize};

use super::facts::{Fact, TraitedFact};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub chat_link: Option<String>,
    #[serde(rename = "type")]
    pub skill_type: Option<String>,
    pub weapon_type: Option<String>,
    #[serde(default)]
    pub professions: Vec<String>,
    pub slot: Option<String>,
    #[serde(default)]
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub traited_facts: Vec<TraitedFact>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub attunement: Option<String>,
    pub cost: Option<u32>,
    pub dual_wield: Option<String>,
    pub flip_skill: Option<u32>,
    pub initiative: Option<u32>,
    pub next_chain: Option<u32>,
    pub prev_chain: Option<u32>,
    #[serde(default)]
    pub transform_skills: Vec<u32>,
    #[serde(default)]
    pub bundle_skills: Vec<u32>,
    pub toolbelt_skill: Option<u32>,
    #[serde(default)]
    pub flags: Vec<String>,
    pub specialization: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_skill() {
        let json = r#"{
            "name": "Arcing Slice",
            "id": 14375,
            "description": "Burst. Deliver a circular attack.",
            "icon": "https://example.com/icon.png",
            "chat_link": "[&Byc4AAA=]",
            "type": "Profession",
            "weapon_type": "None",
            "professions": ["Warrior"],
            "slot": "Profession_1",
            "cost": 30,
            "flip_skill": 14545,
            "categories": ["Burst"],
            "facts": [
                {"text": "Range", "type": "Range", "value": 150},
                {"text": "Recharge", "type": "Recharge", "value": 8}
            ]
        }"#;
        let skill: Skill = serde_json::from_str(json).unwrap();
        assert_eq!(skill.id, 14375);
        assert_eq!(skill.name, "Arcing Slice");
        assert_eq!(skill.cost, Some(30));
        assert_eq!(skill.professions, vec!["Warrior"]);
        assert_eq!(skill.facts.len(), 2);
    }
}
