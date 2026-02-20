//! Item data from /v2/items.
//! Covers armor, weapons, trinkets, upgrade components (runes, sigils),
//! back items, and relics — all equipment relevant to build optimization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    #[serde(rename = "type")]
    pub item_type: String,
    pub rarity: String,
    pub level: u32,
    pub vendor_value: Option<u32>,
    pub chat_link: Option<String>,
    pub default_skin: Option<u32>,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub game_types: Vec<String>,
    #[serde(default)]
    pub restrictions: Vec<String>,
    pub details: Option<ItemDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ItemDetails {
    Armor(ArmorDetails),
    Weapon(WeaponDetails),
    Trinket(TrinketDetails),
    Back(BackDetails),
    UpgradeComponent(UpgradeDetails),
    // Relic items use the base Item fields; details may be minimal
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmorDetails {
    #[serde(rename = "type")]
    pub armor_type: Option<String>, // Helm, Shoulders, Coat, Gloves, Leggings, Boots
    pub weight_class: Option<String>, // Heavy, Medium, Light
    pub defense: Option<u32>,
    #[serde(default)]
    pub infusion_slots: Vec<InfusionSlot>,
    pub attribute_adjustment: Option<f64>,
    pub infix_upgrade: Option<InfixUpgrade>,
    pub suffix_item_id: Option<u32>, // Rune ID
    pub secondary_suffix_item_id: Option<String>,
    #[serde(default)]
    pub stat_choices: Vec<u32>, // Selectable stat IDs (for Ascended/Legendary)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponDetails {
    #[serde(rename = "type")]
    pub weapon_type: Option<String>,
    pub damage_type: Option<String>,
    pub min_power: Option<u32>,
    pub max_power: Option<u32>,
    pub defense: Option<u32>,
    #[serde(default)]
    pub infusion_slots: Vec<InfusionSlot>,
    pub attribute_adjustment: Option<f64>,
    pub infix_upgrade: Option<InfixUpgrade>,
    pub suffix_item_id: Option<u32>, // Sigil ID
    pub secondary_suffix_item_id: Option<String>,
    #[serde(default)]
    pub stat_choices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrinketDetails {
    #[serde(rename = "type")]
    pub trinket_type: Option<String>, // Accessory, Amulet, Ring
    #[serde(default)]
    pub infusion_slots: Vec<InfusionSlot>,
    pub attribute_adjustment: Option<f64>,
    pub infix_upgrade: Option<InfixUpgrade>,
    pub suffix_item_id: Option<u32>,
    pub secondary_suffix_item_id: Option<String>,
    #[serde(default)]
    pub stat_choices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackDetails {
    #[serde(default)]
    pub infusion_slots: Vec<InfusionSlot>,
    pub attribute_adjustment: Option<f64>,
    pub infix_upgrade: Option<InfixUpgrade>,
    pub suffix_item_id: Option<u32>,
    pub secondary_suffix_item_id: Option<String>,
    #[serde(default)]
    pub stat_choices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeDetails {
    #[serde(rename = "type")]
    pub upgrade_type: Option<String>, // Default, Gem, Rune, Sigil
    #[serde(default)]
    pub flags: Vec<String>, // Compatible item types
    #[serde(default)]
    pub infusion_upgrade_flags: Vec<String>,
    pub suffix: Option<String>,
    pub infix_upgrade: Option<InfixUpgrade>,
    #[serde(default)]
    pub bonuses: Vec<String>, // Rune set bonus descriptions
}

/// Stat bonuses built into an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfixUpgrade {
    pub id: Option<u32>, // ItemStat ID
    #[serde(default)]
    pub attributes: Vec<InfixAttribute>,
    pub buff: Option<InfixBuff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfixAttribute {
    pub attribute: String,
    pub modifier: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfixBuff {
    pub skill_id: Option<u32>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfusionSlot {
    #[serde(default)]
    pub flags: Vec<String>, // "Enrichment" or "Infusion"
    pub item_id: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_item_base() {
        let json = r#"{
            "id": 12345,
            "name": "Berserker's Greatsword",
            "type": "Weapon",
            "rarity": "Ascended",
            "level": 80,
            "vendor_value": 100,
            "flags": ["SoulbindOnAcquire"],
            "game_types": ["PvE", "WvW"],
            "restrictions": []
        }"#;
        let item: Item = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, 12345);
        assert_eq!(item.item_type, "Weapon");
        assert_eq!(item.rarity, "Ascended");
    }
}
