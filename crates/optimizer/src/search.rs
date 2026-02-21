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

/// Find the best gear prefix combinations for an archetype.
/// Uses a greedy approach: try each relevant prefix as the primary,
/// then mix in secondary prefixes for optimization.
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
            score: 0.0, // scored later
        });
    }

    // Strategy 2: Mix two prefixes (primary on most slots, secondary on trinkets)
    if relevant_stats.len() >= 2 {
        for (i, primary) in relevant_stats.iter().enumerate() {
            for (j, secondary) in relevant_stats.iter().enumerate() {
                if i == j {
                    continue;
                }
                let mut slot_stats = HashMap::new();
                for slot in STAT_SLOTS {
                    let is_trinket = matches!(
                        *slot,
                        "Backpack" | "Accessory1" | "Accessory2" | "Amulet" | "Ring1" | "Ring2"
                    );
                    let stat_id = if is_trinket { secondary.id } else { primary.id };
                    slot_stats.insert(slot.to_string(), stat_id);
                }
                candidates.push(GearCandidate {
                    slot_stats,
                    stat_prefix_name: format!("{} / {}", primary.name, secondary.name),
                    score: 0.0,
                });
            }
        }
    }

    candidates
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
        // Should have: 2 single-prefix + 2 mixed = 4
        assert!(candidates.len() >= 4);
    }
}
