//! Optimization orchestration — runs the full pipeline.
//! Combines deterministic gear search with LLM reasoning (S08).

use std::collections::HashMap;

use gw2_api::models::{
    EquipmentTab, Item, ItemStat, Profession, Specialization, Trait as GW2Trait,
};
use gw2_core::types::GameMode;

use gw2_api::models::Fact;

use crate::combat::{self, CombatPerformance, DamageModifiers};
use crate::scoring::{score_combat, score_combat_weighted, AggressionLevel, Archetype};
use crate::search::{search_gear_prefixes, search_spec_combos, GearCandidate};
use crate::stats;

/// A complete build candidate ready for comparison or LLM evaluation.
#[derive(Debug, Clone)]
pub struct BuildCandidate {
    pub gear: GearCandidate,
    pub elite_spec: Option<u32>,
    pub core_specs: Vec<u32>,
    /// All equipped trait IDs (minor + selected major, 3 per spec column).
    pub equipped_traits: Vec<u32>,
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
    aggression: Option<&AggressionLevel>,
) -> Vec<BuildCandidate> {
    // Default to FullOffense for backward compatibility (matches old score_combat behavior)
    let default_aggression = AggressionLevel::FullOffense;
    let aggression = aggression.unwrap_or(&default_aggression);

    if *game_mode == GameMode::PvP {
        return optimize_pvp(profession, archetype, specs_cache, traits_cache, &mut on_progress, top_n, aggression);
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
        full_stats += &mock_stats;
        let derived = stats::compute_derived(&full_stats, &profession.name);
        let perf = combat::calculate_combat_performance(
            &full_stats, &derived, &empty_mods, solo_profile, &profession.name,
        );
        candidate.score = score_combat_weighted(&perf, archetype, aggression);
    }

    // Sort by score descending
    gear_candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    gear_candidates.truncate(top_n * 3); // keep extra — traits can shift rankings significantly

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
            let spec_ids: Vec<u32> = cores
                .iter()
                .copied()
                .chain(elite.iter().copied())
                .collect();

            // Collect minor traits (always active) + best major trait per column
            let mut trait_ids = Vec::new();
            for &spec_id in &spec_ids {
                if let Some(spec) = specs_cache.get(&spec_id) {
                    trait_ids.extend(&spec.minor_traits);
                    // Pick 1 best major trait per column (Adept/Master/Grandmaster)
                    let best = select_best_major_traits(
                        &spec.major_traits, archetype, traits_cache,
                    );
                    trait_ids.extend(best);
                }
            }

            // Calculate stats with gear + traits
            let gear_stats = calculate_candidate_stats(gear, itemstats_cache);
            let trait_stats = stats::calculate_trait_stats(&trait_ids, traits_cache);

            let mut full_stats = stats::base_stats();
            full_stats += &gear_stats;
            full_stats += &trait_stats;
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
            let score = score_combat_weighted(&combat_perf, archetype, aggression);

            all_candidates.push(BuildCandidate {
                gear: gear.clone(),
                elite_spec: *elite,
                core_specs: cores.clone(),
                equipped_traits: trait_ids,
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
    aggression: &AggressionLevel,
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
        let spec_ids: Vec<u32> = cores
            .iter()
            .copied()
            .chain(elite.iter().copied())
            .collect();

        let mut trait_ids = Vec::new();
        for &spec_id in &spec_ids {
            if let Some(spec) = specs_cache.get(&spec_id) {
                trait_ids.extend(&spec.minor_traits);
                let best = select_best_major_traits(
                    &spec.major_traits, archetype, traits_cache,
                );
                trait_ids.extend(best);
            }
        }

        // PvP stats come from amulet (not gear), so only calculate trait bonuses
        let trait_stats = stats::calculate_trait_stats(&trait_ids, traits_cache);
        let mut full_stats = stats::base_stats();
        full_stats += &trait_stats;
        stats::apply_trait_conversions(&mut full_stats, &trait_ids, traits_cache);

        let derived = stats::compute_derived(&full_stats, &profession.name);

        // Extract modifiers from traits only (PvP has no gear modifiers)
        let modifiers = combat::extract_damage_modifiers(
            &trait_ids, None, &[], None, traits_cache, &HashMap::new(),
        );
        let combat_perf = combat::calculate_combat_performance(
            &full_stats, &derived, &modifiers, solo_profile, &profession.name,
        );
        let score = score_combat_weighted(&combat_perf, archetype, aggression);

        all_candidates.push(BuildCandidate {
            gear: empty_gear.clone(),
            elite_spec: *elite,
            core_specs: cores.clone(),
            equipped_traits: trait_ids,
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

        let adj = attribute_adjustment_for_slot(slot);

        for attr in &itemstat.attributes {
            let value = adj * attr.multiplier + attr.value as f64;
            stats.add(&attr.attribute, value.round());
        }
    }

    stats
}

/// Typical Ascended attribute_adjustment values by equipment slot.
/// In GW2, attribute_adjustment is the same across armor weight classes —
/// only the defense rating differs (handled by base_defense in stats.rs).
fn attribute_adjustment_for_slot(slot: &str) -> f64 {
    match slot {
        // Armor (Ascended) — same attribute_adjustment regardless of weight class
        "Helm" | "Shoulders" | "Gloves" | "Boots" => 141.0,
        "Coat" => 225.0,
        "Leggings" => 171.0,
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

/// Select the best major trait from each column (Adept/Master/Grandmaster) for an archetype.
/// GW2 specialization major_traits layout: [A1, A2, A3, M1, M2, M3, G1, G2, G3]
/// Each column has 3 choices; the player picks 1 per column = 3 total.
/// This heuristic scores each trait's stat contributions + damage modifier relevance
/// against the archetype weights and picks the best per column.
fn select_best_major_traits(
    major_traits: &[u32],
    archetype: &Archetype,
    traits_cache: &HashMap<u32, GW2Trait>,
) -> Vec<u32> {
    if major_traits.len() != 9 {
        // Unexpected layout — return all as fallback (some specs may have fewer)
        return major_traits.to_vec();
    }

    let weights = archetype.weights();
    let mut selected = Vec::with_capacity(3);

    // Process 3 columns: [0..3], [3..6], [6..9]
    for col_start in (0..9).step_by(3) {
        let column = &major_traits[col_start..col_start + 3];
        let mut best_id = column[0];
        let mut best_score = f64::NEG_INFINITY;

        for &trait_id in column {
            let score = score_trait_for_archetype(trait_id, &weights, traits_cache);
            if score > best_score {
                best_score = score;
                best_id = trait_id;
            }
        }
        selected.push(best_id);
    }

    selected
}

/// Score a single trait's relevance for an archetype by examining its facts.
/// Looks at AttributeAdjust (flat stat bonuses) and Percent (damage modifiers).
fn score_trait_for_archetype(
    trait_id: u32,
    weights: &crate::scoring::StatWeights,
    traits_cache: &HashMap<u32, GW2Trait>,
) -> f64 {
    let Some(t) = traits_cache.get(&trait_id) else {
        return 0.0;
    };

    let mut score = 0.0;

    for fact in &t.facts {
        score += score_fact(fact, weights);
    }

    // Score traited_facts — these activate when a specific other trait is equipped.
    // If the requiring trait is from the same spec, it's likely co-selected (80% credit).
    // If from a different spec, it's uncertain (30% credit).
    for tf in &t.traited_facts {
        let same_spec = traits_cache.get(&tf.requires_trait)
            .map(|rt| rt.specialization == t.specialization)
            .unwrap_or(false);
        let credit = if same_spec { 0.8 } else { 0.3 };
        score += score_fact(&tf.fact, weights) * credit;
    }

    score
}

/// Score a single fact's contribution to an archetype.
fn score_fact(fact: &Fact, weights: &crate::scoring::StatWeights) -> f64 {
    match fact {
        Fact::AttributeAdjust {
            value: Some(val),
            target: Some(ref target),
            ..
        } => {
            let w = match target.as_str() {
                "Power" => weights.power,
                "Precision" => weights.precision,
                "Toughness" => weights.toughness,
                "Vitality" => weights.vitality,
                "ConditionDamage" => weights.condition_damage,
                "ConditionDuration" | "Expertise" => weights.expertise,
                "BoonDuration" | "Concentration" => weights.concentration,
                "CritDamage" | "Ferocity" => weights.ferocity,
                "Healing" | "HealingPower" => weights.healing_power,
                _ => 0.0,
            };
            // Normalize: +100 stat with weight 1.0 → 0.033 (similar to stat scoring)
            (*val as f64) / 3000.0 * w
        }
        Fact::Percent {
            text: Some(ref text),
            percent: Some(pct),
            ..
        } => {
            let text_lower = text.to_lowercase();
            // Damage-related percent modifiers are highly valuable for DPS archetypes
            if text_lower.contains("damage") {
                let dps_weight = (weights.power + weights.condition_damage) / 2.0;
                pct / 100.0 * dps_weight
            } else if text_lower.contains("critical") {
                pct / 100.0 * weights.ferocity.max(weights.precision)
            } else if text_lower.contains("healing") {
                pct / 100.0 * weights.healing_power
            } else if text_lower.contains("boon duration") {
                pct / 100.0 * weights.concentration
            } else if text_lower.contains("condition duration") {
                pct / 100.0 * weights.expertise
            } else {
                0.0
            }
        }
        Fact::BuffConversion {
            percent: Some(pct),
            source: Some(ref src),
            target: Some(ref tgt),
            ..
        } => {
            // Conversion is valuable if source stat is high for this archetype
            // and target stat is also weighted
            let src_w = match src.as_str() {
                "Power" => weights.power,
                "Precision" => weights.precision,
                "Toughness" => weights.toughness,
                "Vitality" => weights.vitality,
                "ConditionDamage" => weights.condition_damage,
                "Ferocity" => weights.ferocity,
                _ => 0.0,
            };
            let tgt_w = match tgt.as_str() {
                "Power" => weights.power,
                "Precision" => weights.precision,
                "Toughness" => weights.toughness,
                "Vitality" => weights.vitality,
                "ConditionDamage" => weights.condition_damage,
                "Ferocity" => weights.ferocity,
                "Healing" | "HealingPower" => weights.healing_power,
                _ => 0.0,
            };
            pct / 100.0 * src_w * tgt_w
        }
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_adjustment_slots() {
        assert_eq!(attribute_adjustment_for_slot("Coat"), 225.0);
        assert_eq!(attribute_adjustment_for_slot("Helm"), 141.0);
        assert_eq!(attribute_adjustment_for_slot("Amulet"), 157.0);
        assert_eq!(attribute_adjustment_for_slot("Leggings"), 171.0);
        assert_eq!(attribute_adjustment_for_slot("WeaponA1"), 251.0);
    }

    #[test]
    fn test_select_best_major_traits_picks_one_per_column() {
        // Create 9 traits: 3 columns × 3 rows
        // Column 0 (Adept): traits 100, 101, 102
        // Column 1 (Master): traits 200, 201, 202
        // Column 2 (Grandmaster): traits 300, 301, 302
        let major_traits = vec![100, 101, 102, 200, 201, 202, 300, 301, 302];

        // With no traits in cache, should still return 3 traits (one per column)
        let traits_cache = HashMap::new();
        let selected = select_best_major_traits(&major_traits, &Archetype::PowerDPS, &traits_cache);
        assert_eq!(selected.len(), 3);
        // Each should come from a different column
        assert!(major_traits[0..3].contains(&selected[0]));
        assert!(major_traits[3..6].contains(&selected[1]));
        assert!(major_traits[6..9].contains(&selected[2]));
    }

    #[test]
    fn test_select_best_major_traits_prefers_power_for_power_dps() {
        use gw2_api::models::Fact;

        let major_traits = vec![100, 101, 102, 200, 201, 202, 300, 301, 302];
        let mut traits_cache = HashMap::new();

        // Trait 100: gives +150 Power (good for PowerDPS)
        traits_cache.insert(100, GW2Trait {
            id: 100, name: "Power Trait".into(), tier: 1, order: 0,
            description: None, slot: "Major".into(), icon: None,
            specialization: 1, skills: vec![],
            facts: vec![Fact::AttributeAdjust {
                text: Some("Power".into()), icon: None,
                value: Some(150), target: Some("Power".into()),
            }],
            traited_facts: vec![],
        });
        // Trait 101: gives +150 Vitality (bad for PowerDPS)
        traits_cache.insert(101, GW2Trait {
            id: 101, name: "Vitality Trait".into(), tier: 1, order: 1,
            description: None, slot: "Major".into(), icon: None,
            specialization: 1, skills: vec![],
            facts: vec![Fact::AttributeAdjust {
                text: Some("Vitality".into()), icon: None,
                value: Some(150), target: Some("Vitality".into()),
            }],
            traited_facts: vec![],
        });
        // Trait 102: nothing
        traits_cache.insert(102, GW2Trait {
            id: 102, name: "Empty Trait".into(), tier: 1, order: 2,
            description: None, slot: "Major".into(), icon: None,
            specialization: 1, skills: vec![],
            facts: vec![],
            traited_facts: vec![],
        });

        let selected = select_best_major_traits(&major_traits, &Archetype::PowerDPS, &traits_cache);
        // First column should select trait 100 (Power bonus)
        assert_eq!(selected[0], 100, "PowerDPS should prefer Power trait over Vitality");
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
            None,
        );

        assert!(!candidates.is_empty());
        // Should be sorted by score descending
        for i in 1..candidates.len() {
            assert!(candidates[i - 1].score >= candidates[i].score);
        }
    }
}
