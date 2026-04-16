//! Deterministic stat calculation engine.
//! Computes final stats from gear, runes, sigils, traits, infusions, and buffs.
//! Must match in-game values within ±1 (rounding).
//!
//! Stat formula for gear: `attribute_adjustment * multiplier + value`
//! where attribute_adjustment comes from the item's rarity/type and
//! multiplier/value come from the itemstat definition.

use std::collections::HashMap;
use std::ops::AddAssign;

use gw2_api::models::{EquipmentPiece, EquipmentTab, Fact, InfixUpgrade, Item, ItemStat, Trait};

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

    /// Returns true if all stats are zero (default).
    pub fn is_zero(&self) -> bool {
        self.power == 0.0
            && self.precision == 0.0
            && self.toughness == 0.0
            && self.vitality == 0.0
            && self.condition_damage == 0.0
            && self.expertise == 0.0
            && self.concentration == 0.0
            && self.ferocity == 0.0
            && self.healing_power == 0.0
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

impl AddAssign for StatBlock {
    fn add_assign(&mut self, rhs: Self) {
        self.power += rhs.power;
        self.precision += rhs.precision;
        self.toughness += rhs.toughness;
        self.vitality += rhs.vitality;
        self.condition_damage += rhs.condition_damage;
        self.expertise += rhs.expertise;
        self.concentration += rhs.concentration;
        self.ferocity += rhs.ferocity;
        self.healing_power += rhs.healing_power;
    }
}

impl AddAssign<&StatBlock> for StatBlock {
    fn add_assign(&mut self, rhs: &StatBlock) {
        self.power += rhs.power;
        self.precision += rhs.precision;
        self.toughness += rhs.toughness;
        self.vitality += rhs.vitality;
        self.condition_damage += rhs.condition_damage;
        self.expertise += rhs.expertise;
        self.concentration += rhs.concentration;
        self.ferocity += rhs.ferocity;
        self.healing_power += rhs.healing_power;
    }
}

/// Derived combat stats computed from primary stats.
#[derive(Debug, Clone, Default)]
pub struct DerivedStats {
    pub crit_chance: f64, // percentage (0-100)
    pub crit_damage: f64, // percentage (e.g., 202.6)
    pub effective_power: f64,
    pub health: f64,
    pub armor: f64,
}

/// Level 80 base stats per profession.
/// All professions get base_primary_attribute (1000) in Power, Precision, Toughness,
/// Vitality at level 80. Health pool varies by profession.
/// Source: https://wiki.guildwars2.com/wiki/Attribute
pub fn base_stats() -> StatBlock {
    let base = crate::data::universal_formulas::formulas().base_primary_attribute;
    StatBlock {
        power: base,
        precision: base,
        toughness: base,
        vitality: base,
        ..Default::default()
    }
}

/// Base health by profession (at level 80, NOT including vitality).
/// Vitality is added separately in compute_derived: health = base + vitality * 10.
/// Values loaded from data/profession_profiles.json via the data module.
/// Source: https://wiki.guildwars2.com/wiki/Health
pub fn base_health(profession: &str) -> f64 {
    crate::data::profession_profiles::profiles()
        .base_health(profession)
        .unwrap_or(5922.0) // default to medium for unknown professions
}

/// Base defense from a full set of Ascended armor, by armor weight class.
/// Values loaded from data/profession_profiles.json via the data module.
/// Source: https://wiki.guildwars2.com/wiki/Armor
pub fn base_defense(profession: &str) -> f64 {
    crate::data::profession_profiles::profiles()
        .base_defense(profession)
        .unwrap_or(1118.0) // default to medium for unknown professions
}

/// Armor weight class for a profession.
/// Values loaded from data/profession_profiles.json via the data module.
pub fn armor_weight(profession: &str) -> &'static str {
    // OnceLock profiles live for 'static, so we can return &'static str
    crate::data::profession_profiles::profiles()
        .armor_weight(profession)
        .unwrap_or("Medium")
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
        if matches!(
            slot,
            "Relic" | "HelmAquatic" | "WeaponAquaticA" | "WeaponAquaticB"
        ) {
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
        let stat_id = piece.stats.as_ref().map(|s| s.id).or_else(|| {
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
            if let Some(infix) = item.details.as_ref().and_then(|d| d.infix_upgrade.as_ref()) {
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
/// Rune stat bonuses are in the `bonuses` array as strings like "+25 Power".
/// We sum all 6 tiers (assuming full 6-piece set).
pub fn calculate_rune_stats(rune_id: Option<u32>, items_cache: &HashMap<u32, Item>) -> StatBlock {
    let mut stats = StatBlock::default();

    let Some(id) = rune_id else {
        return stats;
    };
    let Some(rune) = items_cache.get(&id) else {
        return stats;
    };

    if let Some(ref details) = rune.details {
        // Parse stat bonuses from bonuses strings like "+25 Power", "+100 Condition Damage"
        for bonus_str in &details.bonuses {
            if let Some((value, attr)) = parse_bonus_string(bonus_str) {
                stats.add(&attr, value);
            }
        }

        // Also check infix_upgrade as fallback (some items may use it)
        if let Some(ref infix) = details.infix_upgrade {
            apply_infix_upgrade(&mut stats, infix);
        }
    }

    stats
}

/// Parse a rune bonus string like "+25 Power" or "+100 Condition Damage".
/// Returns (value, normalized_attribute_name) or None if not a stat bonus.
fn parse_bonus_string(s: &str) -> Option<(f64, String)> {
    let s = s.trim();
    if !s.starts_with('+') {
        return None;
    }

    // Split at first space after the number: "+25 Power" -> ("25", "Power")
    let without_plus = &s[1..];
    let space_idx = without_plus.find(' ')?;
    let num_str = &without_plus[..space_idx];
    let attr_str = without_plus[space_idx + 1..].trim();

    let value: f64 = num_str.parse().ok()?;

    // Normalize attribute names from display format to API format
    let attr = match attr_str {
        "Power" => "Power",
        "Precision" => "Precision",
        "Toughness" => "Toughness",
        "Vitality" => "Vitality",
        "Ferocity" => "CritDamage",
        "Condition Damage" => "ConditionDamage",
        "Expertise" => "Expertise",
        "Concentration" => "Concentration",
        "Healing Power" => "Healing",
        _ => return None, // Non-stat bonuses (e.g., "5% damage increase") are ignored
    };

    Some((value, attr.to_string()))
}

/// Calculate permanent sigil stat bonuses.
/// Only sigils with permanent bonuses contribute to the stat sheet.
pub fn calculate_sigil_stats(sigil_ids: &[u32], items_cache: &HashMap<u32, Item>) -> StatBlock {
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
/// Also processes `traited_facts` — conditional bonuses that activate when
/// the required trait is also equipped (overriding or supplementing base facts).
pub fn calculate_trait_stats(
    equipped_trait_ids: &[u32],
    traits_cache: &HashMap<u32, Trait>,
) -> StatBlock {
    let mut stats = StatBlock::default();

    for &trait_id in equipped_trait_ids {
        let Some(t) = traits_cache.get(&trait_id) else {
            continue;
        };

        // Collect indices of base facts overridden by active traited_facts
        let overridden: Vec<u32> = t
            .traited_facts
            .iter()
            .filter(|tf| equipped_trait_ids.contains(&tf.requires_trait))
            .filter_map(|tf| tf.overrides)
            .collect();

        // Process base facts (skip overridden ones)
        for (idx, fact) in t.facts.iter().enumerate() {
            if overridden.contains(&(idx as u32)) {
                continue;
            }
            apply_attribute_adjust(&mut stats, fact);
        }

        // Process active traited_facts
        for tf in &t.traited_facts {
            if equipped_trait_ids.contains(&tf.requires_trait) {
                apply_attribute_adjust(&mut stats, &tf.fact);
            }
        }
    }

    stats
}

/// Apply an AttributeAdjust fact to a stat block.
fn apply_attribute_adjust(stats: &mut StatBlock, fact: &Fact) {
    if let Fact::AttributeAdjust {
        value: Some(val),
        target: Some(ref target),
        ..
    } = fact
    {
        stats.add(target, *val as f64);
    }
}

/// Calculate stat conversions from traits (BuffConversion facts).
/// E.g., "10% of Precision becomes Ferocity".
/// Uses a snapshot of stats before any conversions so all conversions
/// read the same base values regardless of trait order.
/// Also processes traited_facts that contain BuffConversion.
pub fn apply_trait_conversions(
    stats: &mut StatBlock,
    equipped_trait_ids: &[u32],
    traits_cache: &HashMap<u32, Trait>,
) {
    let snapshot = stats.clone();

    for &trait_id in equipped_trait_ids {
        let Some(t) = traits_cache.get(&trait_id) else {
            continue;
        };

        // Collect indices of base facts overridden by active traited_facts
        let overridden: Vec<u32> = t
            .traited_facts
            .iter()
            .filter(|tf| equipped_trait_ids.contains(&tf.requires_trait))
            .filter_map(|tf| tf.overrides)
            .collect();

        // Process base facts (skip overridden ones)
        for (idx, fact) in t.facts.iter().enumerate() {
            if overridden.contains(&(idx as u32)) {
                continue;
            }
            apply_buff_conversion(&mut *stats, &snapshot, fact);
        }

        // Process active traited_facts
        for tf in &t.traited_facts {
            if equipped_trait_ids.contains(&tf.requires_trait) {
                apply_buff_conversion(&mut *stats, &snapshot, &tf.fact);
            }
        }
    }
}

/// Apply a BuffConversion fact using the pre-conversion snapshot.
fn apply_buff_conversion(stats: &mut StatBlock, snapshot: &StatBlock, fact: &Fact) {
    if let Fact::BuffConversion {
        source: Some(ref src),
        percent: Some(pct),
        target: Some(ref tgt),
        ..
    } = fact
    {
        let source_val = snapshot.get(src);
        let bonus = source_val * pct / 100.0;
        stats.add(tgt, bonus.round());
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
/// Formula constants loaded from data/formulas/universal.json.
/// Source: https://wiki.guildwars2.com/wiki/Critical_Chance (crit chance)
/// Source: https://wiki.guildwars2.com/wiki/Ferocity (crit damage)
/// Source: https://wiki.guildwars2.com/wiki/Health (health)
pub fn compute_derived(stats: &StatBlock, profession: &str) -> DerivedStats {
    let f = crate::data::universal_formulas::formulas();
    let crit_chance = f.crit_chance(stats.precision).clamp(0.0, 100.0);
    let crit_damage = f.crit_damage(stats.ferocity);
    let effective_power = stats.power * (1.0 + (crit_chance / 100.0) * (crit_damage / 100.0 - 1.0));
    let health = base_health(profession) + stats.vitality * f.vitality_to_health;
    let armor = stats.toughness + base_defense(profession);

    DerivedStats {
        crit_chance,
        crit_damage,
        effective_power,
        health,
        armor,
    }
}

/// Full stat calculation pipeline for PvE/WvW.
/// NOTE: rune_id and sigil_ids are passed separately from equipment to avoid
/// double-counting — calculate_gear_stats does NOT process piece.upgrades.
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
        // Source: https://wiki.guildwars2.com/wiki/Health
        // High HP (9212): Warrior, Necromancer
        assert_eq!(base_health("Warrior"), 9212.0);
        assert_eq!(base_health("Necromancer"), 9212.0);
        // Medium HP (5922): Revenant, Engineer, Ranger, Mesmer
        assert_eq!(base_health("Ranger"), 5922.0);
        assert_eq!(base_health("Revenant"), 5922.0);
        assert_eq!(base_health("Engineer"), 5922.0);
        assert_eq!(base_health("Mesmer"), 5922.0);
        // Low HP (1645): Guardian, Thief, Elementalist
        assert_eq!(base_health("Guardian"), 1645.0);
        assert_eq!(base_health("Thief"), 1645.0);
        assert_eq!(base_health("Elementalist"), 1645.0);
    }

    #[test]
    fn test_base_defense() {
        assert_eq!(base_defense("Warrior"), 1271.0);
        assert_eq!(base_defense("Guardian"), 1271.0);
        assert_eq!(base_defense("Revenant"), 1271.0);
        assert_eq!(base_defense("Engineer"), 1118.0);
        assert_eq!(base_defense("Ranger"), 1118.0);
        assert_eq!(base_defense("Thief"), 1118.0);
        assert_eq!(base_defense("Elementalist"), 967.0);
        assert_eq!(base_defense("Mesmer"), 967.0);
        assert_eq!(base_defense("Necromancer"), 967.0);
    }

    #[test]
    fn test_armor_weight() {
        assert_eq!(armor_weight("Warrior"), "Heavy");
        assert_eq!(armor_weight("Thief"), "Medium");
        assert_eq!(armor_weight("Elementalist"), "Light");
    }

    #[test]
    fn test_derived_stats_no_gear() {
        let stats = base_stats();
        let derived = compute_derived(&stats, "Warrior");
        // Source: https://wiki.guildwars2.com/wiki/Critical_Chance
        // Precision 1000: crit chance = (1000 - 895) / 21 = 5.0%
        assert!((derived.crit_chance - 5.0).abs() < 0.1);
        // Source: https://wiki.guildwars2.com/wiki/Ferocity
        // Ferocity 0: crit damage = 150.0 + 0/15 = 150%
        assert!((derived.crit_damage - 150.0).abs() < 0.1);
        // Source: https://wiki.guildwars2.com/wiki/Health
        // Health: 9212 (profession base) + 1000 (base vitality) * 10 = 19212
        assert!((derived.health - 19212.0).abs() < 1.0);
        // Armor: 1000 (base toughness) + 1271 (heavy armor defense) = 2271
        assert!((derived.armor - 2271.0).abs() < 1.0);
    }

    #[test]
    fn test_parse_rune_bonus_strings() {
        assert_eq!(
            parse_bonus_string("+25 Power"),
            Some((25.0, "Power".into()))
        );
        assert_eq!(
            parse_bonus_string("+100 Condition Damage"),
            Some((100.0, "ConditionDamage".into()))
        );
        assert_eq!(
            parse_bonus_string("+35 Ferocity"),
            Some((35.0, "CritDamage".into()))
        );
        // Non-stat bonuses return None
        assert_eq!(parse_bonus_string("+5% damage increase"), None);
        assert_eq!(parse_bonus_string("Some text"), None);
    }

    #[test]
    fn test_rune_scholar_stats() {
        // Scholar rune bonuses: +25 Power, +35 Ferocity, +50 Power, +65 Ferocity, +100 Power, +125 Ferocity
        // Total: Power = 175, Ferocity = 225
        let mut items = HashMap::new();
        items.insert(
            24836,
            Item {
                id: 24836,
                name: "Superior Rune of the Scholar".into(),
                item_type: "UpgradeComponent".into(),
                rarity: "Exotic".into(),
                level: 60,
                details: Some(gw2_api::models::ItemDetails {
                    detail_type: Some("Rune".into()),
                    bonuses: vec![
                        "+25 Power".into(),
                        "+35 Ferocity".into(),
                        "+50 Power".into(),
                        "+65 Ferocity".into(),
                        "+100 Power".into(),
                        "+125 Ferocity".into(),
                    ],
                    ..default_details()
                }),
                ..default_item()
            },
        );

        let stats = calculate_rune_stats(Some(24836), &items);
        assert_eq!(stats.power, 175.0);
        assert_eq!(stats.ferocity, 225.0);
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
    fn test_stat_block_add_assign() {
        let mut base = StatBlock {
            power: 1000.0,
            precision: 900.0,
            ..Default::default()
        };
        let bonus = StatBlock {
            power: 200.0,
            ferocity: 300.0,
            ..Default::default()
        };
        base += &bonus;
        assert_eq!(base.power, 1200.0);
        assert_eq!(base.precision, 900.0);
        assert_eq!(base.ferocity, 300.0);
    }

    #[test]
    fn test_stat_block_is_zero() {
        assert!(StatBlock::default().is_zero());
        let mut s = StatBlock::default();
        s.power = 1.0;
        assert!(!s.is_zero());
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


    #[test]
    fn test_stat_add_all_alias_pairs() {
        // CLAUDE.md calls out the ConditionDuration/Expertise rename as the
        // primary API compatibility concern. The GW2 API still emits the old
        // spellings alongside the new ones depending on the endpoint, so every
        // alias pair must normalize symmetrically through `add` and round-trip
        // through `get`. Missing any pair silently drops stats on either path.
        let pairs = [
            ("ConditionDuration", "Expertise"),
            ("BoonDuration", "Concentration"),
            ("CritDamage", "Ferocity"),
            ("Healing", "HealingPower"),
        ];
        for (old, new) in pairs {
            let mut stats = StatBlock::default();
            stats.add(old, 10.0);
            stats.add(new, 5.0);

            let via_old = stats.get(old);
            let via_new = stats.get(new);
            assert_eq!(
                via_old, via_new,
                "alias pair ({old}, {new}) must read the same value — got {via_old} vs {via_new}",
            );
            assert_eq!(
                via_new, 15.0,
                "alias pair ({old}, {new}) must accumulate additions — got {via_new}",
            );
        }
    }

    #[test]
    fn test_stat_get_unknown_attr_returns_zero() {
        // `get` returns 0.0 for unrecognized attributes (e.g. AgonyResistance)
        // rather than panicking. This mirrors `add`'s silent-skip behavior
        // and lets callers iterate API responses without per-attr filtering.
        let stats = StatBlock::default();
        assert_eq!(stats.get("AgonyResistance"), 0.0);
        assert_eq!(stats.get("TotallyFakeAttribute"), 0.0);
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
