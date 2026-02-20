//! Optimization orchestration — runs the full pipeline.
//! Combines deterministic gear search with LLM reasoning (S08).

use std::collections::HashMap;

use gw2_api::models::{
    EquipmentTab, Item, ItemStat, Profession, Specialization, Trait as GW2Trait,
};

use crate::scoring::{score_stats, Archetype};
use crate::search::{search_gear_prefixes, search_spec_combos, GearCandidate};
use crate::stats;

/// A complete build candidate ready for comparison or LLM evaluation.
#[derive(Debug, Clone)]
pub struct BuildCandidate {
    pub gear: GearCandidate,
    pub elite_spec: Option<u32>,
    pub core_specs: [u32; 2],
    pub stats: stats::StatBlock,
    pub derived: stats::DerivedStats,
    pub score: f64,
}

/// Progress update during optimization.
#[derive(Debug, Clone)]
pub struct OptimizeProgress {
    pub stage: String,
    pub done: bool,
}

/// Run the optimization pipeline for a given profession and archetype.
/// Returns top N candidates ranked by score.
/// This handles the deterministic parts; LLM reasoning is added in S08.
pub fn optimize(
    profession: &Profession,
    archetype: &Archetype,
    _current_equipment: Option<&EquipmentTab>,
    _items_cache: &HashMap<u32, Item>,
    itemstats_cache: &HashMap<u32, ItemStat>,
    specs_cache: &HashMap<u32, Specialization>,
    traits_cache: &HashMap<u32, GW2Trait>,
    mut on_progress: impl FnMut(OptimizeProgress),
    top_n: usize,
) -> Vec<BuildCandidate> {
    on_progress(OptimizeProgress {
        stage: "Searching gear combinations...".into(),
        done: false,
    });

    // 1. Find best gear prefix combinations
    let mut gear_candidates = search_gear_prefixes(archetype, itemstats_cache);

    // Score each gear candidate
    for candidate in &mut gear_candidates {
        // Build a mock equipment from the candidate's slot_stats
        let mock_stats = calculate_candidate_stats(candidate, itemstats_cache);
        let mut full_stats = stats::base_stats();
        stats_add(&mut full_stats, &mock_stats);
        let derived = stats::compute_derived(&full_stats, &profession.name);
        candidate.score = score_stats(&full_stats, &derived, archetype);
    }

    // Sort by score descending
    gear_candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    gear_candidates.truncate(top_n * 2); // keep extra for spec combinations

    on_progress(OptimizeProgress {
        stage: "Evaluating specialization combinations...".into(),
        done: false,
    });

    // 2. Find valid spec combinations
    let spec_combos = search_spec_combos(&profession.specializations, specs_cache);

    // 3. Combine gear + specs into full candidates
    let mut all_candidates: Vec<BuildCandidate> = Vec::new();

    for gear in &gear_candidates {
        // For efficiency, only test top spec combos with top gear
        for (elite, cores) in spec_combos.iter().take(5) {
            // Collect trait IDs for stat calculation (minor traits auto-selected)
            let mut trait_ids = Vec::new();
            let spec_ids: Vec<u32> = cores
                .iter()
                .copied()
                .chain(elite.iter().copied())
                .collect();

            for &spec_id in &spec_ids {
                if let Some(spec) = specs_cache.get(&spec_id) {
                    trait_ids.extend(&spec.minor_traits);
                }
            }

            // Calculate stats with gear + traits
            let gear_stats = calculate_candidate_stats(gear, itemstats_cache);
            let trait_stats = stats::calculate_trait_stats(&trait_ids, traits_cache);

            let mut full_stats = stats::base_stats();
            stats_add(&mut full_stats, &gear_stats);
            stats_add(&mut full_stats, &trait_stats);
            stats::apply_trait_conversions(&mut full_stats, &trait_ids, traits_cache);

            let derived = stats::compute_derived(&full_stats, &profession.name);
            let score = score_stats(&full_stats, &derived, archetype);

            all_candidates.push(BuildCandidate {
                gear: gear.clone(),
                elite_spec: *elite,
                core_specs: *cores,
                stats: full_stats,
                derived,
                score,
            });
        }
    }

    on_progress(OptimizeProgress {
        stage: "Ranking candidates...".into(),
        done: false,
    });

    // Sort and return top N
    all_candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all_candidates.truncate(top_n);

    on_progress(OptimizeProgress {
        stage: "Done".into(),
        done: true,
    });

    all_candidates
}

/// Calculate approximate stats for a gear candidate using itemstat formulas.
/// Uses a typical Ascended attribute_adjustment per slot type.
fn calculate_candidate_stats(
    candidate: &GearCandidate,
    itemstats_cache: &HashMap<u32, ItemStat>,
) -> stats::StatBlock {
    let mut stats = stats::StatBlock::default();

    for (slot, &stat_id) in &candidate.slot_stats {
        let Some(itemstat) = itemstats_cache.get(&stat_id) else {
            continue;
        };

        // Typical Ascended attribute_adjustment by slot type
        let adj = attribute_adjustment_for_slot(slot);

        for attr in &itemstat.attributes {
            let value = adj * attr.multiplier + attr.value as f64;
            stats.add(&attr.attribute, value.round());
        }
    }

    stats
}

/// Typical Ascended attribute_adjustment values by equipment slot.
fn attribute_adjustment_for_slot(slot: &str) -> f64 {
    match slot {
        // Armor (Ascended)
        "Helm" | "Shoulders" | "Gloves" | "Boots" => 141.0,
        "Coat" => 315.0,
        "Leggings" => 191.0,
        // Weapons (Ascended)
        "WeaponA1" | "WeaponB1" => 251.0, // main-hand / two-handed
        "WeaponA2" | "WeaponB2" => 125.0, // off-hand
        // Trinkets (Ascended)
        "Backpack" => 63.0,
        "Accessory1" | "Accessory2" => 110.0,
        "Amulet" => 157.0,
        "Ring1" | "Ring2" => 126.0,
        _ => 100.0,
    }
}

fn stats_add(target: &mut stats::StatBlock, source: &stats::StatBlock) {
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
    fn test_attribute_adjustment_slots() {
        assert_eq!(attribute_adjustment_for_slot("Coat"), 315.0);
        assert_eq!(attribute_adjustment_for_slot("Helm"), 141.0);
        assert_eq!(attribute_adjustment_for_slot("Amulet"), 157.0);
    }

    #[test]
    fn test_optimize_returns_candidates() {
        let mut itemstats = HashMap::new();
        itemstats.insert(584, ItemStat {
            id: 584,
            name: "Berserker's".into(),
            attributes: vec![
                gw2_api::models::StatAttribute { attribute: "Power".into(), multiplier: 0.35, value: 32 },
                gw2_api::models::StatAttribute { attribute: "Precision".into(), multiplier: 0.25, value: 18 },
                gw2_api::models::StatAttribute { attribute: "CritDamage".into(), multiplier: 0.25, value: 18 },
            ],
        });

        let profession = Profession {
            id: "Warrior".into(),
            name: "Warrior".into(),
            code: None,
            specializations: vec![1, 2, 3, 4, 5],
            weapons: HashMap::new(),
            training: Vec::new(),
            icon: None,
            icon_big: None,
        };

        let mut specs = HashMap::new();
        for id in 1..=5u32 {
            specs.insert(id, Specialization {
                id,
                name: format!("Spec{}", id),
                profession: "Warrior".into(),
                elite: false,
                minor_traits: Vec::new(),
                major_traits: Vec::new(),
                weapon_trait: None,
                icon: None,
                background: None,
                profession_icon: None,
                profession_icon_big: None,
            });
        }

        let candidates = optimize(
            &profession,
            &Archetype::PowerDPS,
            None,
            &HashMap::new(),
            &itemstats,
            &specs,
            &HashMap::new(),
            |_| {},
            3,
        );

        assert!(!candidates.is_empty());
        // Should be sorted by score descending
        for i in 1..candidates.len() {
            assert!(candidates[i - 1].score >= candidates[i].score);
        }
    }
}
