//! Deterministic stat calculation engine.
//! Computes final stats from gear, runes, sigils, traits, infusions, and buffs.
//! Must match in-game values within ±1 (rounding).
//!
//! Stat formula for gear: `attribute_adjustment * multiplier + value`
//! where attribute_adjustment comes from the item's rarity/type and
//! multiplier/value come from the itemstat definition.

use std::collections::HashMap;

use gw2_api::models::{
    EquipmentPiece, EquipmentTab, Fact, InfixUpgrade, Item, ItemStat, Trait,
};

/// All nine primary stats.
#[derive(Debug, Clone, Default)]
pub struct StatBlock {
    pub power: f64,
    pub precision: f64,
    pub toughness: f64,
    pub vitality: f64,
    pub condition_damage: f64,
    pub expertise: f64,
    pub concentration: f64,
    pub ferocity: f64,
    pub healing_power: f64,
}

impl StatBlock {
    pub fn add(&mut self, attr: &str, value: f64) {
        match attr {
            "Power" => self.power += value,
            "Precision" => self.precision += value,
            "Toughness" => self.toughness += value,
            "Vitality" => self.vitality += value,
            "ConditionDamage" => self.condition_damage += value,
            "ConditionDuration" | "Expertise" => self.expertise += value,
            "BoonDuration" | "Concentration" => self.concentration += value,
            "CritDamage" | "Ferocity" => self.ferocity += value,
            "Healing" | "HealingPower" => self.healing_power += value,
            _ => {} // AgonyResistance, etc. — ignored for build optimization
        }
    }

    pub fn get(&self, attr: &str) -> f64 {
        match attr {
            "Power" => self.power,
            "Precision" => self.precision,
            "Toughness" => self.toughness,
            "Vitality" => self.vitality,
            "ConditionDamage" => self.condition_damage,
            "ConditionDuration" | "Expertise" => self.expertise,
            "BoonDuration" | "Concentration" => self.concentration,
            "CritDamage" | "Ferocity" => self.ferocity,
            "Healing" | "HealingPower" => self.healing_power,
            _ => 0.0,
        }
    }
}

/// Derived combat stats computed from primary stats.
#[derive(Debug, Clone, Default)]
pub struct DerivedStats {
    pub crit_chance: f64,  // percentage (0-100)
    pub crit_damage: f64,  // percentage (e.g., 202.6)
    pub effective_power: f64,
    pub health: f64,
    pub armor: f64,
}

/// Level 80 base stats per profession.
/// All professions get 1000 in Power, Precision, Toughness, Vitality at level 80.
/// Health pool varies by profession.
pub fn base_stats() -> StatBlock {
    StatBlock {
        power: 1000.0,
        precision: 1000.0,
        toughness: 1000.0,
        vitality: 1000.0,
        ..Default::default()
    }
}

/// Base health by profession (at level 80, before vitality).
pub fn base_health(profession: &str) -> f64 {
    match profession {
        "Warrior" | "Necromancer" => 19212.0,
        "Revenant" | "Engineer" | "Ranger" | "Mesmer" => 15922.0,
        "Guardian" | "Thief" | "Elementalist" => 11645.0,
        _ => 15922.0, // default to medium
    }
}

/// Calculate stats from equipped gear using the itemstat formula.
/// For each equipment piece: look up its attribute_adjustment and the stat prefix,
/// then apply `attribute_adjustment * multiplier + value` for each stat.
pub fn calculate_gear_stats(
    equipment: &[EquipmentPiece],
    items_cache: &HashMap<u32, Item>,
    itemstats_cache: &HashMap<u32, ItemStat>,
) -> StatBlock {
    let mut stats = StatBlock::default();

    for piece in equipment {
        let slot = piece.slot.as_str();
        // Skip non-stat slots
        if matches!(slot, "Relic" | "HelmAquatic" | "WeaponAquaticA" | "WeaponAquaticB") {
            continue;
        }

        let item = match items_cache.get(&piece.id) {
            Some(i) => i,
            None => continue,
        };

        let attribute_adjustment = item
            .details
            .as_ref()
            .and_then(|d| d.attribute_adjustment)
            .unwrap_or(0.0);

        // Get the stat prefix either from equipped stats or from the item's infix_upgrade
        let stat_id = piece
            .stats
            .as_ref()
            .map(|s| s.id)
            .or_else(|| {
                item.details
                    .as_ref()
                    .and_then(|d| d.infix_upgrade.as_ref())
                    .and_then(|iu| iu.id)
            });

        if let Some(sid) = stat_id {
            if let Some(itemstat) = itemstats_cache.get(&sid) {
                for attr in &itemstat.attributes {
                    let value = attribute_adjustment * attr.multiplier + attr.value as f64;
                    stats.add(&attr.attribute, value.round());
                }
            }
        } else {
            // Fallback: use infix_upgrade attributes directly (for items with fixed stats)
            if let Some(infix) = item
                .details
                .as_ref()
                .and_then(|d| d.infix_upgrade.as_ref())
            {
                apply_infix_upgrade(&mut stats, infix);
            }
        }
    }

    stats
}

/// Apply stat bonuses from an InfixUpgrade (fixed stat items).
fn apply_infix_upgrade(stats: &mut StatBlock, infix: &InfixUpgrade) {
    for attr in &infix.attributes {
        stats.add(&attr.attribute, attr.modifier as f64);
    }
}

/// Calculate rune set bonus stats.
/// Most runes give stat bonuses at various stack counts (1-6 pieces).
/// We assume 6 of the same rune for full set bonus.
pub fn calculate_rune_stats(
    rune_id: Option<u32>,
    items_cache: &HashMap<u32, Item>,
) -> StatBlock {
    let mut stats = StatBlock::default();

    let Some(id) = rune_id else {
        return stats;
    };
    let Some(rune) = items_cache.get(&id) else {
        return stats;
    };

    // Rune stats come from the infix_upgrade on the upgrade component
    if let Some(ref details) = rune.details {
        if let Some(ref infix) = details.infix_upgrade {
            apply_infix_upgrade(&mut stats, infix);
        }
    }

    stats
}

/// Calculate permanent sigil stat bonuses.
/// Only sigils with permanent bonuses contribute to the stat sheet.
pub fn calculate_sigil_stats(
    sigil_ids: &[u32],
    items_cache: &HashMap<u32, Item>,
) -> StatBlock {
    let mut stats = StatBlock::default();

    for &id in sigil_ids {
        let Some(sigil) = items_cache.get(&id) else {
            continue;
        };
        if let Some(ref details) = sigil.details {
            if let Some(ref infix) = details.infix_upgrade {
                apply_infix_upgrade(&mut stats, infix);
            }
        }
    }

    stats
}

/// Calculate stat bonuses from infusions.
/// Infusions typically give +5 or +9 to a single stat.
pub fn calculate_infusion_stats(
    equipment: &[EquipmentPiece],
    items_cache: &HashMap<u32, Item>,
) -> StatBlock {
    let mut stats = StatBlock::default();

    for piece in equipment {
        for &infusion_id in &piece.infusions {
            let Some(infusion) = items_cache.get(&infusion_id) else {
                continue;
            };
            if let Some(ref details) = infusion.details {
                if let Some(ref infix) = details.infix_upgrade {
                    apply_infix_upgrade(&mut stats, infix);
                }
            }
        }
    }

    stats
}

/// Calculate stat modifiers from equipped traits.
/// Looks for `AttributeAdjust` facts which give flat stat bonuses.
pub fn calculate_trait_stats(
    equipped_trait_ids: &[u32],
    traits_cache: &HashMap<u32, Trait>,
) -> StatBlock {
    let mut stats = StatBlock::default();

    for &trait_id in equipped_trait_ids {
        let Some(t) = traits_cache.get(&trait_id) else {
            continue;
        };

        for fact in &t.facts {
            if let Fact::AttributeAdjust {
                value: Some(val),
                target: Some(ref target),
                ..
            } = fact
            {
                stats.add(target, *val as f64);
            }
        }
    }

    stats
}

/// Calculate stat conversions from traits (BuffConversion facts).
/// E.g., "10% of Precision becomes Ferocity".
pub fn apply_trait_conversions(
    stats: &mut StatBlock,
    equipped_trait_ids: &[u32],
    traits_cache: &HashMap<u32, Trait>,
) {
    for &trait_id in equipped_trait_ids {
        let Some(t) = traits_cache.get(&trait_id) else {
            continue;
        };

        for fact in &t.facts {
            if let Fact::BuffConversion {
                source: Some(ref src),
                percent: Some(pct),
                target: Some(ref tgt),
                ..
            } = fact
            {
                let source_val = stats.get(src);
                let bonus = source_val * pct / 100.0;
                stats.add(tgt, bonus.round());
            }
        }
    }
}

/// Calculate PvP stats from amulet (flat values, no formula).
pub fn calculate_pvp_stats(amulet_attributes: &HashMap<String, i32>) -> StatBlock {
    let mut stats = base_stats();

    for (attr, &val) in amulet_attributes {
        stats.add(attr, val as f64);
    }

    stats
}

/// Compute derived combat stats from primary stats.
pub fn compute_derived(stats: &StatBlock, profession: &str) -> DerivedStats {
    let crit_chance = ((stats.precision - 895.0) / 21.0).clamp(0.0, 100.0);
    let crit_damage = 150.0 + stats.ferocity / 15.0;
    let effective_power =
        stats.power * (1.0 + (crit_chance / 100.0) * (crit_damage / 100.0 - 1.0));
    let health = base_health(profession) + stats.vitality * 10.0;
    let armor = stats.toughness + 1000.0; // base defense varies by gear, approximate

    DerivedStats {
        crit_chance,
        crit_damage,
        effective_power,
        health,
        armor,
    }
}

/// Full stat calculation pipeline for PvE/WvW.
pub fn calculate_full_stats(
    equipment: &EquipmentTab,
    equipped_trait_ids: &[u32],
    rune_id: Option<u32>,
    sigil_ids: &[u32],
    profession: &str,
    items_cache: &HashMap<u32, Item>,
    itemstats_cache: &HashMap<u32, ItemStat>,
    traits_cache: &HashMap<u32, Trait>,
) -> (StatBlock, DerivedStats) {
    let mut stats = base_stats();

    // Gear stats
    let gear = calculate_gear_stats(&equipment.equipment, items_cache, itemstats_cache);
    stats.power += gear.power;
    stats.precision += gear.precision;
    stats.toughness += gear.toughness;
    stats.vitality += gear.vitality;
    stats.condition_damage += gear.condition_damage;
    stats.expertise += gear.expertise;
    stats.concentration += gear.concentration;
    stats.ferocity += gear.ferocity;
    stats.healing_power += gear.healing_power;

    // Rune bonuses
    let rune_stats = calculate_rune_stats(rune_id, items_cache);
    add_block(&mut stats, &rune_stats);

    // Sigil bonuses (permanent only)
    let sigil_stats = calculate_sigil_stats(sigil_ids, items_cache);
    add_block(&mut stats, &sigil_stats);

    // Infusion bonuses
    let infusion_stats = calculate_infusion_stats(&equipment.equipment, items_cache);
    add_block(&mut stats, &infusion_stats);

    // Trait flat bonuses
    let trait_stats = calculate_trait_stats(equipped_trait_ids, traits_cache);
    add_block(&mut stats, &trait_stats);

    // Trait conversions (applied after all flat bonuses)
    apply_trait_conversions(&mut stats, equipped_trait_ids, traits_cache);

    let derived = compute_derived(&stats, profession);
    (stats, derived)
}

fn add_block(target: &mut StatBlock, source: &StatBlock) {
    target.power += source.power;
    target.precision += source.precision;
    target.toughness += source.toughness;
    target.vitality += source.vitality;
    target.condition_damage += source.condition_damage;
    target.expertise += source.expertise;
    target.concentration += source.concentration;
    target.ferocity += source.ferocity;
    target.healing_power += source.healing_power;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_stats() {
        let stats = base_stats();
        assert_eq!(stats.power, 1000.0);
        assert_eq!(stats.precision, 1000.0);
        assert_eq!(stats.toughness, 1000.0);
        assert_eq!(stats.vitality, 1000.0);
    }

    #[test]
    fn test_base_health() {
        assert_eq!(base_health("Warrior"), 19212.0);
        assert_eq!(base_health("Guardian"), 11645.0);
        assert_eq!(base_health("Ranger"), 15922.0);
    }

    #[test]
    fn test_derived_stats_no_gear() {
        let stats = base_stats();
        let derived = compute_derived(&stats, "Warrior");
        // Precision 1000: crit chance = (1000 - 895) / 21 = 5.0%
        assert!((derived.crit_chance - 5.0).abs() < 0.1);
        // Ferocity 0: crit damage = 150%
        assert!((derived.crit_damage - 150.0).abs() < 0.1);
        // Health: 19212 + 1000 * 10 = 29212
        assert!((derived.health - 29212.0).abs() < 1.0);
    }

    #[test]
    fn test_berserker_stat_calculation() {
        // Berserker's Ascended Helm: attribute_adjustment = 141
        // Berserker's: Power 0.35 * 141 + 32 = 81.35 -> 81
        //              Precision 0.25 * 141 + 18 = 53.25 -> 53
        //              Ferocity 0.25 * 141 + 18 = 53.25 -> 53
        let mut items = HashMap::new();
        items.insert(
            1,
            Item {
                id: 1,
                name: "Test Helm".into(),
                item_type: "Armor".into(),
                rarity: "Ascended".into(),
                level: 80,
                details: Some(gw2_api::models::ItemDetails {
                    detail_type: Some("Helm".into()),
                    attribute_adjustment: Some(141.0),
                    ..default_details()
                }),
                ..default_item()
            },
        );

        let mut itemstats = HashMap::new();
        itemstats.insert(
            584,
            ItemStat {
                id: 584,
                name: "Berserker's".into(),
                attributes: vec![
                    gw2_api::models::StatAttribute {
                        attribute: "Power".into(),
                        multiplier: 0.35,
                        value: 32,
                    },
                    gw2_api::models::StatAttribute {
                        attribute: "Precision".into(),
                        multiplier: 0.25,
                        value: 18,
                    },
                    gw2_api::models::StatAttribute {
                        attribute: "CritDamage".into(),
                        multiplier: 0.25,
                        value: 18,
                    },
                ],
            },
        );

        let equipment = vec![EquipmentPiece {
            id: 1,
            slot: "Helm".into(),
            stats: Some(gw2_api::models::EquipmentStats {
                id: 584,
                attributes: None,
            }),
            ..default_equipment_piece()
        }];

        let stats = calculate_gear_stats(&equipment, &items, &itemstats);

        // 0.35 * 141 + 32 = 81.35, rounded = 81
        assert!((stats.power - 81.0).abs() < 1.0);
        // 0.25 * 141 + 18 = 53.25, rounded = 53
        assert!((stats.precision - 53.0).abs() < 1.0);
        assert!((stats.ferocity - 53.0).abs() < 1.0);
    }

    #[test]
    fn test_pvp_stats() {
        let mut amulet = HashMap::new();
        amulet.insert("Power".into(), 900);
        amulet.insert("Precision".into(), 1200);
        amulet.insert("CritDamage".into(), 900);

        let stats = calculate_pvp_stats(&amulet);
        // Base 1000 + amulet
        assert_eq!(stats.power, 1900.0);
        assert_eq!(stats.precision, 2200.0);
        assert_eq!(stats.ferocity, 900.0); // CritDamage maps to ferocity
    }

    #[test]
    fn test_stat_add_aliases() {
        let mut stats = StatBlock::default();
        stats.add("CritDamage", 100.0);
        stats.add("Ferocity", 50.0);
        assert_eq!(stats.ferocity, 150.0); // Both map to ferocity

        stats.add("BoonDuration", 30.0);
        stats.add("Concentration", 20.0);
        assert_eq!(stats.concentration, 50.0);
    }

    // Helper constructors for test data
    fn default_item() -> Item {
        Item {
            id: 0,
            name: String::new(),
            description: None,
            icon: None,
            item_type: String::new(),
            rarity: String::new(),
            level: 0,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: Vec::new(),
            game_types: Vec::new(),
            restrictions: Vec::new(),
            details: None,
        }
    }

    fn default_details() -> gw2_api::models::ItemDetails {
        gw2_api::models::ItemDetails {
            detail_type: None,
            weight_class: None,
            defense: None,
            damage_type: None,
            min_power: None,
            max_power: None,
            suffix: None,
            bonuses: Vec::new(),
            infusion_upgrade_flags: Vec::new(),
            infusion_slots: Vec::new(),
            attribute_adjustment: None,
            infix_upgrade: None,
            suffix_item_id: None,
            secondary_suffix_item_id: None,
            stat_choices: Vec::new(),
        }
    }

    fn default_equipment_piece() -> EquipmentPiece {
        EquipmentPiece {
            id: 0,
            slot: String::new(),
            location: None,
            skin: None,
            upgrades: Vec::new(),
            infusions: Vec::new(),
            binding: None,
            bound_to: None,
            dyes: Vec::new(),
            stats: None,
        }
    }
}
