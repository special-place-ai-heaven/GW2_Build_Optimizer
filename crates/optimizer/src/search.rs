//! Gear combination search with pruning.
//! Finds the best stat prefix combinations for each gear slot.

use std::collections::HashMap;

use gw2_api::models::{ItemStat, Specialization};

use crate::scoring::OptimizationWeights;

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
    "Helm",
    "Shoulders",
    "Coat",
    "Gloves",
    "Leggings",
    "Boots",
    "WeaponA1",
    "WeaponA2",
    "WeaponB1",
    "WeaponB2",
    "Backpack",
    "Accessory1",
    "Accessory2",
    "Amulet",
    "Ring1",
    "Ring2",
];

/// Slot group definitions for mix strategies.
const TRINKET_SLOTS: &[&str] = &[
    "Backpack",
    "Accessory1",
    "Accessory2",
    "Amulet",
    "Ring1",
    "Ring2",
];
const WEAPON_SLOTS: &[&str] = &["WeaponA1", "WeaponA2", "WeaponB1", "WeaponB2"];
const ARMOR_SLOTS: &[&str] = &["Helm", "Shoulders", "Coat", "Gloves", "Leggings", "Boots"];

/// Find gear prefix combinations using hierarchical tier selection.
///
/// The tier system pre-selects only appropriate prefixes for the user's
/// weight levels, so fewer candidates are generated — each one is a
/// sensible GW2 build rather than a combinatorial guess.
///
/// Strategies:
/// 1. Full single-prefix set (e.g., all Berserker's)
/// 2. Primary + secondary on trinkets (classic armor+weapon / trinket split)
/// 3. Primary + secondary on weapons (weapon stat swap)
/// 4. Primary + secondary on armor (armor stat swap)
pub fn search_gear_prefixes(
    weights: &OptimizationWeights,
    itemstats: &HashMap<u32, ItemStat>,
) -> Vec<GearCandidate> {
    let relevant = crate::scoring::select_prefixes_by_tiers(weights);
    let mut candidates = Vec::new();

    // Find itemstat IDs matching the relevant prefix names.
    // Sort by id so candidate-generation order — and therefore the tie-break
    // when downstream scoring produces equal scores — is stable across runs.
    // `HashMap::values()` order is unspecified.
    let mut relevant_stats: Vec<&ItemStat> = itemstats
        .values()
        .filter(|is| relevant.iter().any(|r| is.name.contains(r)))
        .collect();
    relevant_stats.sort_by_key(|is| is.id);

    if relevant_stats.is_empty() {
        return candidates;
    }

    // Strategy 1: Full set of single prefix
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

    // Mix strategies: 3 split patterns per prefix pair
    if relevant_stats.len() >= 2 {
        for (i, primary) in relevant_stats.iter().enumerate() {
            for (j, secondary) in relevant_stats.iter().enumerate() {
                if i == j {
                    continue;
                }

                // Strategy 2: Secondary on trinkets (classic armor+weapon / trinket split)
                candidates.push(build_mixed_candidate(
                    primary.id,
                    secondary.id,
                    TRINKET_SLOTS,
                    &format!("{} / {}", primary.name, secondary.name),
                ));

                // Strategy 3: Secondary on weapons
                candidates.push(build_mixed_candidate(
                    primary.id,
                    secondary.id,
                    WEAPON_SLOTS,
                    &format!("{} (wep: {})", primary.name, secondary.name),
                ));

                // Strategy 4: Secondary on armor
                candidates.push(build_mixed_candidate(
                    primary.id,
                    secondary.id,
                    ARMOR_SLOTS,
                    &format!("{} (armor: {})", primary.name, secondary.name),
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
    locks: &gw2_core::types::BuildLocks,
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

    let locked_elite = locks.locked_elite_id();
    // Check if slot 0 and slot 1 are locked to specific core specs
    let locked_slot0 = locks.specs[0];
    let locked_slot1 = locks.specs[1];

    // With each elite spec + 2 core specs (elite fills slot 3)
    for &elite in &elite_specs {
        // If elite spec is locked, skip all others
        if let Some(locked) = locked_elite {
            if elite != locked {
                continue;
            }
        }
        // Generate core spec pairs for slots 0 and 1
        let slot0_candidates: Vec<u32> = if let Some(locked0) = locked_slot0 {
            // Slot 0 is locked — only this spec
            if core_specs.contains(&locked0) {
                vec![locked0]
            } else {
                continue;
            }
        } else {
            core_specs.clone()
        };
        let slot1_candidates: Vec<u32> = if let Some(locked1) = locked_slot1 {
            if core_specs.contains(&locked1) {
                vec![locked1]
            } else {
                continue;
            }
        } else {
            core_specs.clone()
        };

        for &s0 in &slot0_candidates {
            for &s1 in &slot1_candidates {
                if s0 == s1 {
                    continue;
                } // Can't pick same spec twice
                  // Normalize order for dedup (smaller first) unless locked
                let pair = if locked_slot0.is_some() || locked_slot1.is_some() || s0 < s1 {
                    (s0, s1)
                } else {
                    continue; // skip reversed pair to avoid duplicates
                };
                combos.push((Some(elite), vec![pair.0, pair.1]));
            }
        }
    }

    // Without elite spec: 3 core specs (only when no elite is locked)
    if locked_elite.is_none() {
        let slot0_candidates: Vec<u32> = if let Some(locked0) = locked_slot0 {
            if core_specs.contains(&locked0) {
                vec![locked0]
            } else {
                vec![]
            }
        } else {
            core_specs.clone()
        };
        let slot1_candidates: Vec<u32> = if let Some(locked1) = locked_slot1 {
            if core_specs.contains(&locked1) {
                vec![locked1]
            } else {
                vec![]
            }
        } else {
            core_specs.clone()
        };
        // Slot 2 (normally elite) can be a core spec if no elite is locked
        let slot2_candidates: Vec<u32> = if let Some(locked2) = locks.specs[2] {
            if core_specs.contains(&locked2) {
                vec![locked2]
            } else {
                vec![]
            }
        } else {
            core_specs.clone()
        };

        for &s0 in &slot0_candidates {
            for &s1 in &slot1_candidates {
                if s1 == s0 {
                    continue;
                }
                for &s2 in &slot2_candidates {
                    if s2 == s0 || s2 == s1 {
                        continue;
                    }
                    // Deduplicate: only keep combos where ids are in ascending order
                    // unless specific slots are locked
                    let any_locked = locked_slot0.is_some()
                        || locked_slot1.is_some()
                        || locks.specs[2].is_some();
                    if !(any_locked || s0 < s1 && s1 < s2) {
                        continue;
                    }
                    combos.push((None, vec![s0, s1, s2]));
                }
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
        itemstats.insert(
            584,
            ItemStat {
                id: 584,
                name: "Berserker's".into(),
                attributes: vec![],
            },
        );
        itemstats.insert(
            656,
            ItemStat {
                id: 656,
                name: "Assassin's".into(),
                attributes: vec![],
            },
        );

        let weights = OptimizationWeights::preset_power_dps();
        let candidates = search_gear_prefixes(&weights, &itemstats);
        // 2 single-prefix + 2*1*3 mixed strategies = 8 total
        assert!(
            candidates.len() >= 8,
            "Expected >=8 candidates, got {}",
            candidates.len()
        );
        // All should have 16 slots
        for c in &candidates {
            assert_eq!(c.slot_stats.len(), STAT_SLOTS.len());
        }
    }

    #[test]
    fn test_mixed_candidate_rings_only() {
        let c = build_mixed_candidate(1, 2, &["Ring1", "Ring2"], "Primary (rings: Secondary)");
        // Rings should use secondary stat
        assert_eq!(c.slot_stats["Ring1"], 2);
        assert_eq!(c.slot_stats["Ring2"], 2);
        // Everything else should use primary
        assert_eq!(c.slot_stats["Coat"], 1);
        assert_eq!(c.slot_stats["WeaponA1"], 1);
        assert_eq!(c.slot_stats["Amulet"], 1);
    }

    #[test]
    fn test_celestial_weights_has_mixing_options() {
        let mut itemstats = HashMap::new();
        itemstats.insert(
            1,
            ItemStat {
                id: 1,
                name: "Celestial".into(),
                attributes: vec![],
            },
        );
        itemstats.insert(
            2,
            ItemStat {
                id: 2,
                name: "Diviner's".into(),
                attributes: vec![],
            },
        );
        itemstats.insert(
            3,
            ItemStat {
                id: 3,
                name: "Trailblazer's".into(),
                attributes: vec![],
            },
        );

        let weights = OptimizationWeights::preset_celestial();
        let candidates = search_gear_prefixes(&weights, &itemstats);
        assert!(
            candidates.len() > 3,
            "Celestial weights should generate mixed gear candidates, got {}",
            candidates.len()
        );
    }
}
