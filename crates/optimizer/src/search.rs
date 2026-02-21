//! Gear combination search with pruning.
//! Finds the best stat prefix combinations for each gear slot given an archetype.

use std::collections::HashMap;

use gw2_api::models::{ItemStat, Specialization};

use crate::scoring::Archetype;

/// A candidate gear configuration: stat prefix per slot.
#[derive(Debug, Clone)]
pub struct GearCandidate {
    /// Maps slot name to itemstat ID.
    pub slot_stats: HashMap<String, u32>,
    /// The itemstat name used (for display, e.g., "Berserker's").
    pub stat_prefix_name: String,
    /// Score from the scoring function.
    pub score: f64,
}

/// PvE/WvW equipment slots that carry stats.
pub const STAT_SLOTS: &[&str] = &[
    "Helm", "Shoulders", "Coat", "Gloves", "Leggings", "Boots",
    "WeaponA1", "WeaponA2", "WeaponB1", "WeaponB2",
    "Backpack", "Accessory1", "Accessory2", "Amulet", "Ring1", "Ring2",
];

/// Slot group definitions for mix strategies.
const TRINKET_SLOTS: &[&str] = &["Backpack", "Accessory1", "Accessory2", "Amulet", "Ring1", "Ring2"];
const RING_SLOTS: &[&str] = &["Ring1", "Ring2"];
const ACCESSORY_SLOTS: &[&str] = &["Accessory1", "Accessory2"];
const WEAPON_SLOTS: &[&str] = &["WeaponA1", "WeaponA2", "WeaponB1", "WeaponB2"];
const ARMOR_SLOTS: &[&str] = &["Helm", "Shoulders", "Coat", "Gloves", "Leggings", "Boots"];

/// Find the best gear prefix combinations for an archetype.
/// Uses multiple mix strategies to capture common GW2 build patterns:
/// 1. Full set of single prefix (e.g., all Berserker's)
/// 2. Primary on armor+weapons, secondary on trinkets (classic split)
/// 3. Primary everywhere, secondary on rings only (precision breakpoints)
/// 4. Primary everywhere, secondary on accessories only
/// 5. Primary on armor+trinkets, secondary on weapons
/// 6. Primary on armor, secondary on weapons+trinkets
/// 7. Primary on weapons, secondary on armor+trinkets
pub fn search_gear_prefixes(
    archetype: &Archetype,
    itemstats: &HashMap<u32, ItemStat>,
) -> Vec<GearCandidate> {
    let relevant = archetype.relevant_prefixes();
    let mut candidates = Vec::new();

    // Find itemstat IDs matching the relevant prefix names
    let relevant_stats: Vec<&ItemStat> = itemstats
        .values()
        .filter(|is| relevant.iter().any(|r| is.name.contains(r)))
        .collect();

    if relevant_stats.is_empty() {
        return candidates;
    }

    // Strategy 1: Full set of single prefix (e.g., all Berserker's)
    for stat in &relevant_stats {
        let mut slot_stats = HashMap::new();
        for slot in STAT_SLOTS {
            slot_stats.insert(slot.to_string(), stat.id);
        }
        candidates.push(GearCandidate {
            slot_stats,
            stat_prefix_name: stat.name.clone(),
            score: 0.0,
        });
    }

    // Mix strategies: for each pair of relevant prefixes, generate several split patterns
    if relevant_stats.len() >= 2 {
        for (i, primary) in relevant_stats.iter().enumerate() {
            for (j, secondary) in relevant_stats.iter().enumerate() {
                if i == j {
                    continue;
                }

                let label = format!("{} / {}", primary.name, secondary.name);

                // Strategy 2: Secondary on all trinkets (classic armor+weapon / trinket split)
                candidates.push(build_mixed_candidate(
                    primary.id, secondary.id, TRINKET_SLOTS, &label,
                ));

                // Strategy 3: Secondary on rings only (common for crit-cap fine-tuning)
                candidates.push(build_mixed_candidate(
                    primary.id, secondary.id, RING_SLOTS,
                    &format!("{} (rings: {})", primary.name, secondary.name),
                ));

                // Strategy 4: Secondary on accessories only
                candidates.push(build_mixed_candidate(
                    primary.id, secondary.id, ACCESSORY_SLOTS,
                    &format!("{} (accs: {})", primary.name, secondary.name),
                ));

                // Strategy 5: Secondary on weapons only
                candidates.push(build_mixed_candidate(
                    primary.id, secondary.id, WEAPON_SLOTS,
                    &format!("{} (wep: {})", primary.name, secondary.name),
                ));

                // Strategy 6: Secondary on weapons + trinkets (armor-only primary)
                let wep_and_trinket: Vec<&str> = WEAPON_SLOTS.iter()
                    .chain(TRINKET_SLOTS.iter())
                    .copied()
                    .collect();
                candidates.push(build_mixed_candidate(
                    primary.id, secondary.id, &wep_and_trinket,
                    &format!("{} armor / {} rest", primary.name, secondary.name),
                ));

                // Strategy 7: Secondary on armor + trinkets (weapon-only primary)
                let armor_and_trinket: Vec<&str> = ARMOR_SLOTS.iter()
                    .chain(TRINKET_SLOTS.iter())
                    .copied()
                    .collect();
                candidates.push(build_mixed_candidate(
                    primary.id, secondary.id, &armor_and_trinket,
                    &format!("{} wep / {} rest", primary.name, secondary.name),
                ));
            }
        }
    }

    candidates
}

/// Build a gear candidate with primary stat on most slots, secondary on specified slots.
fn build_mixed_candidate(
    primary_id: u32,
    secondary_id: u32,
    secondary_slots: &[&str],
    label: &str,
) -> GearCandidate {
    let mut slot_stats = HashMap::new();
    for slot in STAT_SLOTS {
        let stat_id = if secondary_slots.contains(slot) {
            secondary_id
        } else {
            primary_id
        };
        slot_stats.insert(slot.to_string(), stat_id);
    }
    GearCandidate {
        slot_stats,
        stat_prefix_name: label.to_string(),
        score: 0.0,
    }
}

/// Find valid specialization combinations for a profession.
/// Returns (elite_spec_id_or_none, core_specs).
/// With elite: 2 core specs (elite fills slot 3).
/// Without elite: 3 core specs (all 3 slots are core).
pub fn search_spec_combos(
    profession_specs: &[u32],
    all_specs: &HashMap<u32, Specialization>,
) -> Vec<(Option<u32>, Vec<u32>)> {
    let mut combos = Vec::new();

    let core_specs: Vec<u32> = profession_specs
        .iter()
        .filter(|id| all_specs.get(id).is_some_and(|s| !s.elite))
        .copied()
        .collect();

    let elite_specs: Vec<u32> = profession_specs
        .iter()
        .filter(|id| all_specs.get(id).is_some_and(|s| s.elite))
        .copied()
        .collect();

    // With each elite spec + 2 core specs (elite fills slot 3)
    for &elite in &elite_specs {
        for i in 0..core_specs.len() {
            for j in (i + 1)..core_specs.len() {
                combos.push((Some(elite), vec![core_specs[i], core_specs[j]]));
            }
        }
    }

    // Without elite spec: 3 core specs (all 3 slots are core, no repeats)
    for i in 0..core_specs.len() {
        for j in (i + 1)..core_specs.len() {
            for k in (j + 1)..core_specs.len() {
                combos.push((None, vec![core_specs[i], core_specs[j], core_specs[k]]));
            }
        }
    }

    combos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stat_slots_count() {
        // 6 armor + 4 weapons + 6 trinkets = 16
        assert_eq!(STAT_SLOTS.len(), 16);
    }

    #[test]
    fn test_search_gear_prefixes_generates_candidates() {
        let mut itemstats = HashMap::new();
        itemstats.insert(584, ItemStat {
            id: 584,
            name: "Berserker's".into(),
            attributes: vec![],
        });
        itemstats.insert(656, ItemStat {
            id: 656,
            name: "Assassin's".into(),
            attributes: vec![],
        });

        let candidates = search_gear_prefixes(&Archetype::PowerDPS, &itemstats);
        // 2 single-prefix + 2*6 mixed strategies = 14 total
        assert!(candidates.len() >= 14, "Expected >=14 candidates, got {}", candidates.len());
        // All should have 16 slots
        for c in &candidates {
            assert_eq!(c.slot_stats.len(), STAT_SLOTS.len());
        }
    }

    #[test]
    fn test_mixed_candidate_rings_only() {
        let c = build_mixed_candidate(1, 2, RING_SLOTS, "Primary (rings: Secondary)");
        // Rings should use secondary stat
        assert_eq!(c.slot_stats["Ring1"], 2);
        assert_eq!(c.slot_stats["Ring2"], 2);
        // Everything else should use primary
        assert_eq!(c.slot_stats["Coat"], 1);
        assert_eq!(c.slot_stats["WeaponA1"], 1);
        assert_eq!(c.slot_stats["Amulet"], 1);
    }

    #[test]
    fn test_celestial_hybrid_has_mixing_options() {
        let mut itemstats = HashMap::new();
        itemstats.insert(1, ItemStat { id: 1, name: "Celestial".into(), attributes: vec![] });
        itemstats.insert(2, ItemStat { id: 2, name: "Diviner's".into(), attributes: vec![] });
        itemstats.insert(3, ItemStat { id: 3, name: "Trailblazer's".into(), attributes: vec![] });

        let candidates = search_gear_prefixes(&Archetype::CelestialHybrid, &itemstats);
        // 3 single + 6*5 mixed = 33
        assert!(candidates.len() > 3, "CelestialHybrid should generate mixed gear candidates, got {}", candidates.len());
    }
}
