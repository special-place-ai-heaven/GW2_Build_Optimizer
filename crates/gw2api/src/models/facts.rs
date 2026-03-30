//! Shared fact types used by both traits and skills.
//! Facts describe the mechanical effects (damage, buffs, conditions, etc.)
//! that are critical for the LLM to reason about synergies and rotations.

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

/// A mechanical fact describing an effect.
/// The `type` field in the API determines which variant this is.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Fact {
    AttributeAdjust {
        text: Option<String>,
        icon: Option<String>,
        value: Option<i32>,
        target: Option<String>,
    },
    Buff {
        text: Option<String>,
        icon: Option<String>,
        duration: Option<u32>,
        status: Option<String>,
        description: Option<String>,
        apply_count: Option<u32>,
    },
    BuffConversion {
        text: Option<String>,
        icon: Option<String>,
        source: Option<String>,
        percent: Option<f64>,
        target: Option<String>,
    },
    ComboField {
        text: Option<String>,
        icon: Option<String>,
        field_type: Option<String>,
    },
    ComboFinisher {
        text: Option<String>,
        icon: Option<String>,
        finisher_type: Option<String>,
        percent: Option<u32>,
    },
    Damage {
        text: Option<String>,
        icon: Option<String>,
        hit_count: Option<u32>,
        dmg_multiplier: Option<f64>,
    },
    Distance {
        text: Option<String>,
        icon: Option<String>,
        distance: Option<u32>,
    },
    Duration {
        text: Option<String>,
        icon: Option<String>,
        duration: Option<u32>,
    },
    Heal {
        text: Option<String>,
        icon: Option<String>,
        hit_count: Option<u32>,
    },
    HealingAdjust {
        text: Option<String>,
        icon: Option<String>,
        hit_count: Option<u32>,
    },
    NoData {
        text: Option<String>,
        icon: Option<String>,
    },
    Number {
        text: Option<String>,
        icon: Option<String>,
        value: Option<i32>,
    },
    Percent {
        text: Option<String>,
        icon: Option<String>,
        percent: Option<f64>,
    },
    PrefixedBuff {
        text: Option<String>,
        icon: Option<String>,
        duration: Option<u32>,
        status: Option<String>,
        description: Option<String>,
        apply_count: Option<u32>,
        prefix: Option<BuffPrefix>,
    },
    Radius {
        text: Option<String>,
        icon: Option<String>,
        distance: Option<u32>,
    },
    Range {
        text: Option<String>,
        icon: Option<String>,
        value: Option<u32>,
    },
    Recharge {
        text: Option<String>,
        icon: Option<String>,
        value: Option<f64>,
    },
    StunBreak {
        text: Option<String>,
        icon: Option<String>,
        value: Option<bool>,
    },
    Time {
        text: Option<String>,
        icon: Option<String>,
        duration: Option<u32>,
    },
    Unblockable {
        text: Option<String>,
        icon: Option<String>,
        value: Option<bool>,
    },
    /// Fallback for unknown fact types the API may add in the future.
    #[serde(other)]
    Unknown,
}

/// Prefix for PrefixedBuff facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuffPrefix {
    pub text: Option<String>,
    pub icon: Option<String>,
    pub status: Option<String>,
    pub description: Option<String>,
}

/// A conditional fact that activates when a specific trait is selected.
/// `requires_trait` is the trait ID that must be equipped.
/// `overrides` is the index in the parent facts array to replace (or append if absent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitedFact {
    pub requires_trait: u32,
    pub overrides: Option<u32>,
    #[serde(flatten)]
    pub fact: Fact,
}

/// Lenient deserializer for `Vec<Fact>` — skips facts that fail to parse
/// (e.g. missing `type` field). The GW2 API occasionally returns fact objects
/// without a `type` discriminator.
pub fn deserialize_facts<'de, D>(deserializer: D) -> Result<Vec<Fact>, D::Error>
where
    D: Deserializer<'de>,
{
    let values: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// Lenient deserializer for `Vec<TraitedFact>` — skips entries that fail to parse.
pub fn deserialize_traited_facts<'de, D>(deserializer: D) -> Result<Vec<TraitedFact>, D::Error>
where
    D: Deserializer<'de>,
{
    let values: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_attribute_adjust() {
        let json = r#"{
            "type": "AttributeAdjust",
            "icon": "https://example.com/icon.png",
            "value": 180,
            "target": "CritDamage"
        }"#;
        let fact: Fact = serde_json::from_str(json).unwrap();
        match fact {
            Fact::AttributeAdjust { value, target, .. } => {
                assert_eq!(value, Some(180));
                assert_eq!(target.as_deref(), Some("CritDamage"));
            }
            _ => panic!("Expected AttributeAdjust"),
        }
    }

    #[test]
    fn test_deserialize_buff() {
        let json = r#"{
            "text": "Apply Buff/Condition",
            "type": "Buff",
            "icon": "https://example.com/icon.png",
            "duration": 4,
            "status": "Fury",
            "description": "Critical chance increased; stacks duration.",
            "apply_count": 1
        }"#;
        let fact: Fact = serde_json::from_str(json).unwrap();
        match fact {
            Fact::Buff {
                duration,
                status,
                apply_count,
                ..
            } => {
                assert_eq!(duration, Some(4));
                assert_eq!(status.as_deref(), Some("Fury"));
                assert_eq!(apply_count, Some(1));
            }
            _ => panic!("Expected Buff"),
        }
    }

    #[test]
    fn test_deserialize_recharge() {
        let json = r#"{"text": "Recharge", "type": "Recharge", "icon": "https://example.com/icon.png", "value": 8}"#;
        let fact: Fact = serde_json::from_str(json).unwrap();
        match fact {
            Fact::Recharge { value, .. } => assert_eq!(value, Some(8.0)),
            _ => panic!("Expected Recharge"),
        }
    }

    #[test]
    fn test_lenient_facts_skips_missing_type() {
        // Simulates GW2 API returning a fact without a "type" field
        let json = r#"[
            {"text": "Recharge", "type": "Recharge", "icon": "i.png", "value": 8},
            {"text": "Some effect", "icon": "i.png", "value": 5},
            {"text": "Range", "type": "Range", "icon": "i.png", "value": 300}
        ]"#;
        let values: Vec<serde_json::Value> = serde_json::from_str(json).unwrap();
        let facts: Vec<Fact> = values
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        // The middle fact (missing type) should be skipped
        assert_eq!(facts.len(), 2);
    }
}
