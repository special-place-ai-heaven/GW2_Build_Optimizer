//! Optimization orchestration — runs the full pipeline.
//! Combines deterministic gear search with LLM reasoning (S08).

use std::collections::HashMap;

use gw2_api::models::{
    EquipmentTab, Item, ItemStat, Profession, Specialization, Trait as GW2Trait,
};
use gw2_core::types::GameMode;

use crate::combat::{self, CombatPerformance, DamageModifiers};
use crate::scoring::{score_combat, Archetype};
use crate::search::{search_gear_prefixes, search_spec_combos, GearCandidate};
use crate::stats;

/// A complete build candidate ready for comparison or LLM evaluation.
#[derive(Debug, Clone)]
pub struct BuildCandidate {
    pub gear: GearCandidate,
    pub elite_spec: Option<u32>,
    pub core_specs: Vec<u32>,
    pub stats: stats::StatBlock,
    pub derived: stats::DerivedStats,
    pub score: f64,
    /// Combat performance metrics (Solo profile) for display and scoring.
    pub combat: CombatPerformance,
    /// Extracted damage modifiers (for recalculating with different buff profiles).
    pub modifiers: DamageModifiers,
}

/// Progress update during optimization.
#[derive(Debug, Clone)]
pub struct OptimizeProgress {
    pub stage: String,
    pub done: bool,
}

/// Run the optimization pipeline for a given profession and archetype.
/// Returns top N candidates ranked by score.
/// For PvP, skips gear search (stats come from amulet) and only evaluates spec/trait combos.
/// For PvE/WvW, runs full gear + spec search.
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
    game_mode: &GameMode,
) -> Vec<BuildCandidate> {
    if *game_mode == GameMode::PvP {
        return optimize_pvp(profession, archetype, specs_cache, traits_cache, &mut on_progress, top_n);
    }

    on_progress(OptimizeProgress {
        stage: "Searching gear combinations...".into(),
        done: false,
    });

    // 1. Find best gear prefix combinations
    let mut gear_candidates = search_gear_prefixes(archetype, itemstats_cache);

    // Score each gear candidate (preliminary — no traits/modifiers yet)
    let empty_mods = DamageModifiers::default();
    let solo_profile = &combat::default_buff_profiles()[0];
    for candidate in &mut gear_candidates {
        let mock_stats = calculate_candidate_stats(candidate, itemstats_cache);
        let mut full_stats = stats::base_stats();
        stats_add(&mut full_stats, &mock_stats);
        let derived = stats::compute_derived(&full_stats, &profession.name);
        let perf = combat::calculate_combat_performance(
            &full_stats, &derived, &empty_mods, solo_profile, &profession.name,
        );
        candidate.score = score_combat(&perf, archetype);
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
        for (elite, cores) in &spec_combos {
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

            // Extract damage modifiers from traits (no rune/sigil/relic in search phase)
            let modifiers = combat::extract_damage_modifiers(
                &trait_ids, None, &[], None, traits_cache, _items_cache,
            );

            // Calculate combat performance with Solo profile
            let combat_perf = combat::calculate_combat_performance(
                &full_stats, &derived, &modifiers, solo_profile, &profession.name,
            );
            let score = score_combat(&combat_perf, archetype);

            all_candidates.push(BuildCandidate {
                gear: gear.clone(),
                elite_spec: *elite,
                core_specs: cores.clone(),
                stats: full_stats,
                derived,
                score,
                combat: combat_perf,
                modifiers,
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

/// PvP optimization: specs + traits only (gear is replaced by amulet system).
fn optimize_pvp(
    profession: &Profession,
    archetype: &Archetype,
    specs_cache: &HashMap<u32, Specialization>,
    traits_cache: &HashMap<u32, GW2Trait>,
    on_progress: &mut impl FnMut(OptimizeProgress),
    top_n: usize,
) -> Vec<BuildCandidate> {
    on_progress(OptimizeProgress {
        stage: "Evaluating PvP specialization combinations...".into(),
        done: false,
    });

    let spec_combos = search_spec_combos(&profession.specializations, specs_cache);
    let mut all_candidates: Vec<BuildCandidate> = Vec::new();

    // PvP: no gear search, use empty gear candidate
    let empty_gear = GearCandidate {
        slot_stats: HashMap::new(),
        stat_prefix_name: "(PvP Amulet)".into(),
        score: 0.0,
    };

    let solo_profile = &combat::default_buff_profiles()[0];

    for (elite, cores) in &spec_combos {
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

        // PvP stats come from amulet (not gear), so only calculate trait bonuses
        let trait_stats = stats::calculate_trait_stats(&trait_ids, traits_cache);
        let mut full_stats = stats::base_stats();
        stats_add(&mut full_stats, &trait_stats);
        stats::apply_trait_conversions(&mut full_stats, &trait_ids, traits_cache);

        let derived = stats::compute_derived(&full_stats, &profession.name);

        // Extract modifiers from traits only (PvP has no gear modifiers)
        let modifiers = combat::extract_damage_modifiers(
            &trait_ids, None, &[], None, traits_cache, &HashMap::new(),
        );
        let combat_perf = combat::calculate_combat_performance(
            &full_stats, &derived, &modifiers, solo_profile, &profession.name,
        );
        let score = score_combat(&combat_perf, archetype);

        all_candidates.push(BuildCandidate {
            gear: empty_gear.clone(),
            elite_spec: *elite,
            core_specs: cores.clone(),
            stats: full_stats,
            derived,
            score,
            combat: combat_perf,
            modifiers,
        });
    }

    on_progress(OptimizeProgress {
        stage: "Ranking PvP candidates...".into(),
        done: false,
    });

    all_candidates.sort_by(|a, b| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
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
        "Coat" => 225.0,  // varies by weight class, using Medium average
        "Leggings" => 171.0, // varies by weight class, using Medium average
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
        assert_eq!(attribute_adjustment_for_slot("Coat"), 225.0);
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
            skills_by_palette: Vec::new(),
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
            &GameMode::PvE,
        );

        assert!(!candidates.is_empty());
        // Should be sorted by score descending
        for i in 1..candidates.len() {
            assert!(candidates[i - 1].score >= candidates[i].score);
        }
    }
}
