//! Optimization orchestration — runs the full pipeline.
//! Combines deterministic gear search with LLM reasoning (S08).

use std::collections::HashMap;

use gw2_api::models::{
    EquipmentTab, Item, ItemStat, Profession, PvpAmulet, Specialization, Trait as GW2Trait,
};
use gw2_core::types::GameMode;

use gw2_api::models::Fact;

use crate::balance::BalanceContext;
use crate::combat::{self, CombatPerformance, DamageModifiers};
use crate::context::{self, ContextConfig};
use crate::data;
use crate::gamedb::GameDb;
use crate::gemini_tools::{self, ToolContext};
use crate::llm::LlmClient;
use crate::prompts;
use crate::rotation;
use crate::scoring::{self, score_with_weights, OptimizationWeights, StatWeights};
use crate::search::{search_gear_prefixes, search_spec_combos, GearCandidate};
use crate::stats;
use crate::validation::{self, ValidatedBuild};

/// Selected PvP amulet in a build candidate (PvP mode only).
#[derive(Debug, Clone)]
pub struct PvpAmuletCandidate {
    pub id: u32,
    pub name: String,
    pub stats: HashMap<String, i32>,
}

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
    /// Selected PvP amulet (only set in PvP mode; None for PvE/WvW).
    pub pvp_amulet: Option<PvpAmuletCandidate>,
    /// Overall data quality assessment for this candidate's inputs.
    pub data_quality: data::DataQuality,
    /// Reasons for any data quality degradation.
    pub quality_reasons: Vec<data::DataQualityReason>,
}

/// Progress update during optimization.
#[derive(Debug, Clone)]
pub struct OptimizeProgress {
    pub stage: String,
    pub done: bool,
}

/// Build a human-readable description of lock constraints using GameDb names.
/// Used for LLM prompt generation so Gemini knows what to preserve.
fn describe_lock_constraints(locks: &gw2_core::types::BuildLocks, db: &GameDb) -> String {
    if !locks.has_any_locks() {
        return String::new();
    }
    let mut parts = Vec::new();
    for (slot, spec_id) in locks.specs.iter().enumerate() {
        if let Some(id) = spec_id {
            let name = db
                .specializations
                .get(id)
                .map(|s| s.name.as_str())
                .unwrap_or("Unknown");
            let elite = db.specializations.get(id).is_some_and(|s| s.elite);
            let elite_tag = if elite { " (Elite)" } else { "" };
            parts.push(format!(
                "Slot {} LOCKED to \"{}\"{}",
                slot + 1,
                name,
                elite_tag
            ));

            // Trait locks for this spec
            if let Some(trait_cols) = locks.trait_locks.get(id) {
                for (col, trait_id) in trait_cols.iter().enumerate() {
                    if let Some(tid) = trait_id {
                        let tier = match col {
                            0 => "Adept",
                            1 => "Master",
                            _ => "Grandmaster",
                        };
                        let tname = db
                            .traits
                            .get(tid)
                            .map(|t| t.name.as_str())
                            .unwrap_or("Unknown");
                        parts.push(format!("  {} trait LOCKED to \"{}\"", tier, tname));
                    }
                }
            }
        }
    }
    parts.join("\n")
}

/// Run the optimization pipeline for a given profession and archetype.
/// Returns top N candidates ranked by score, or an error describing why none were found.
/// For PvP, skips gear search (stats come from amulet) and only evaluates spec/trait combos.
/// For PvE/WvW, runs full gear + spec search.
// Core optimization entry point; the caches, weights, progress callback, and
// top-N are distinct concerns — bundling them into a params struct adds
// indirection without clarifying the call site.
#[allow(clippy::too_many_arguments)]
pub fn optimize(
    profession: &Profession,
    weights: &OptimizationWeights,
    _current_equipment: Option<&EquipmentTab>,
    _items_cache: &HashMap<u32, Item>,
    itemstats_cache: &HashMap<u32, ItemStat>,
    specs_cache: &HashMap<u32, Specialization>,
    traits_cache: &HashMap<u32, GW2Trait>,
    mut on_progress: impl FnMut(OptimizeProgress),
    top_n: usize,
    ctx: &BalanceContext,
    locks: &gw2_core::types::BuildLocks,
    pvp_amulets: &HashMap<u32, PvpAmulet>,
) -> Result<Vec<BuildCandidate>, String> {
    if ctx.game_mode == GameMode::PvP {
        return optimize_pvp(
            profession,
            weights,
            specs_cache,
            traits_cache,
            &mut on_progress,
            top_n,
            locks,
            ctx,
            pvp_amulets,
        )
        .and_then(|v| {
            if v.is_empty() {
                Err(format!(
                    "No PvP candidates found for {} / {}",
                    profession.name,
                    weights.summary_label()
                ))
            } else {
                Ok(v)
            }
        });
    }

    on_progress(OptimizeProgress {
        stage: "Searching gear combinations...".into(),
        done: false,
    });

    // 1. Find best gear prefix combinations
    let mut gear_candidates = search_gear_prefixes(weights, itemstats_cache);
    if gear_candidates.is_empty() {
        return Err(format!(
            "No gear stat prefixes found for {}. GameDb has {} itemstats loaded.",
            weights.summary_label(),
            itemstats_cache.len()
        ));
    }

    // Score each gear candidate (preliminary — no traits/modifiers yet)
    let empty_mods = DamageModifiers::default();
    let solo_profile = &combat::buff_profiles_for_profession(&profession.name, ctx)[0];
    let cw = combat::condition_weights_for_profession(&profession.name, ctx);
    for candidate in &mut gear_candidates {
        let mock_stats = calculate_candidate_stats(candidate, itemstats_cache);
        let mut full_stats = stats::base_stats();
        full_stats += &mock_stats;
        let derived = stats::compute_derived(&full_stats, &profession.name);
        let perf = combat::calculate_combat_performance(
            &full_stats,
            &derived,
            &empty_mods,
            solo_profile,
            &cw,
            &profession.name,
            ctx,
        );
        candidate.score = score_with_weights(&perf, weights);
    }

    // Sort by score descending
    gear_candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    gear_candidates.truncate(top_n * 3); // keep extra — traits can shift rankings significantly

    on_progress(OptimizeProgress {
        stage: "Evaluating specialization combinations...".into(),
        done: false,
    });

    // 2. Find valid spec combinations
    let spec_combos = search_spec_combos(&profession.specializations, specs_cache, locks);
    if spec_combos.is_empty() {
        let core_count = profession
            .specializations
            .iter()
            .filter(|id| specs_cache.get(id).is_some_and(|s| !s.elite))
            .count();
        let elite_count = profession
            .specializations
            .iter()
            .filter(|id| specs_cache.get(id).is_some_and(|s| s.elite))
            .count();
        return Err(format!(
            "No valid spec combinations for {}. Has {} core specs (need ≥3) and {} elite specs. \
             {} of {} spec IDs found in GameDb.",
            profession.name,
            core_count,
            elite_count,
            profession
                .specializations
                .iter()
                .filter(|id| specs_cache.contains_key(id))
                .count(),
            profession.specializations.len()
        ));
    }

    let stat_weights = weights.to_stat_weights();

    // 3. Combine gear + specs into full candidates
    let mut all_candidates: Vec<BuildCandidate> = Vec::new();

    // Pre-compute spec-combo invariants (trait_ids, trait stats, trait modifiers).
    // These are gear-independent — recomputing them inside the gear loop was
    // ~5x wasted work for a typical 5-gear search.
    struct PrecomputedSpec {
        elite: Option<u32>,
        cores: Vec<u32>,
        trait_ids: Vec<u32>,
        trait_stats: stats::StatBlock,
        modifiers: combat::DamageModifiers,
    }
    let precomputed_specs: Vec<PrecomputedSpec> = spec_combos
        .iter()
        .map(|(elite, cores)| {
            let spec_ids: Vec<u32> = cores.iter().copied().chain(elite.iter().copied()).collect();

            let mut trait_ids = Vec::new();
            for &spec_id in &spec_ids {
                if let Some(spec) = specs_cache.get(&spec_id) {
                    trait_ids.extend(&spec.minor_traits);
                    let best = select_best_major_traits(
                        &spec.major_traits,
                        &stat_weights,
                        traits_cache,
                        locks,
                        spec_id,
                    );
                    trait_ids.extend(best);
                }
            }

            let trait_stats = stats::calculate_trait_stats(&trait_ids, traits_cache);
            let modifiers = combat::extract_damage_modifiers(
                &trait_ids,
                None,
                &[],
                None,
                traits_cache,
                _items_cache,
                ctx,
            );
            PrecomputedSpec {
                elite: *elite,
                cores: cores.clone(),
                trait_ids,
                trait_stats,
                modifiers,
            }
        })
        .collect();

    for gear in &gear_candidates {
        // gear_stats is spec-invariant — compute once per gear.
        let gear_stats = calculate_candidate_stats(gear, itemstats_cache);

        for spec in &precomputed_specs {
            let mut full_stats = stats::base_stats();
            full_stats += &gear_stats;
            full_stats += &spec.trait_stats;
            stats::apply_trait_conversions(&mut full_stats, &spec.trait_ids, traits_cache);

            let derived = stats::compute_derived(&full_stats, &profession.name);

            // Calculate combat performance with Solo profile
            let combat_perf = combat::calculate_combat_performance(
                &full_stats,
                &derived,
                &spec.modifiers,
                solo_profile,
                &cw,
                &profession.name,
                ctx,
            );
            let score = score_with_weights(&combat_perf, weights);
            let (data_quality, quality_reasons) =
                quality_from_modifiers(&spec.modifiers, &[], false, ctx.game_mode.label());

            all_candidates.push(BuildCandidate {
                gear: gear.clone(),
                elite_spec: spec.elite,
                core_specs: spec.cores.clone(),
                equipped_traits: spec.trait_ids.clone(),
                stats: full_stats,
                derived,
                score,
                combat: combat_perf,
                modifiers: spec.modifiers.clone(),
                pvp_amulet: None,
                data_quality,
                quality_reasons,
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

    if all_candidates.is_empty() {
        return Err(format!(
            "Optimization produced 0 candidates from {} gear × {} spec combos for {} / {}",
            gear_candidates.len(),
            spec_combos.len(),
            profession.name,
            weights.summary_label()
        ));
    }

    Ok(all_candidates)
}

/// PvP optimization: iterates PvP amulets × spec/trait combos (gear is replaced by amulet system).
/// PvP amulet stats REPLACE gear stats — the stat block is: base_stats + amulet + traits.
/// Slot-budget data is NOT loaded during PvP optimization.
/// Returns an error if no PvP amulet data is available (no silent zero-stat fallback).
// PvP search variant mirroring `optimize`'s caches/weights/callback shape; a
// params struct would not improve the single internal call site.
#[allow(clippy::too_many_arguments)]
fn optimize_pvp(
    profession: &Profession,
    weights: &OptimizationWeights,
    specs_cache: &HashMap<u32, Specialization>,
    traits_cache: &HashMap<u32, GW2Trait>,
    on_progress: &mut impl FnMut(OptimizeProgress),
    top_n: usize,
    locks: &gw2_core::types::BuildLocks,
    ctx: &BalanceContext,
    pvp_amulets: &HashMap<u32, PvpAmulet>,
) -> Result<Vec<BuildCandidate>, String> {
    if pvp_amulets.is_empty() {
        return Err("No PvP amulet data available. Download game data first.".to_string());
    }

    on_progress(OptimizeProgress {
        stage: "Evaluating PvP amulet × specialization combinations...".into(),
        done: false,
    });

    let spec_combos = search_spec_combos(&profession.specializations, specs_cache, locks);
    let mut all_candidates: Vec<BuildCandidate> = Vec::new();

    // PvP: no gear search, use empty gear candidate
    let empty_gear = GearCandidate {
        slot_stats: HashMap::new(),
        stat_prefix_name: "(PvP Amulet)".into(),
        score: 0.0,
    };

    let solo_profile = &combat::buff_profiles_for_profession(&profession.name, ctx)[0];
    let stat_weights = weights.to_stat_weights();
    let cw = combat::condition_weights_for_profession(&profession.name, ctx);

    // Iterate amulets by id so candidates with identical scores break ties
    // deterministically. `pvp_amulets.values()` iteration order is unspecified;
    // the downstream `sort_by(...)` is stable but a stable sort preserves whatever
    // input order it got, so the "best" amulet could vary across runs on ties.
    let mut amulets_sorted: Vec<&PvpAmulet> = pvp_amulets.values().collect();
    amulets_sorted.sort_by_key(|a| a.id);

    // Pre-compute spec-combo invariants. trait_ids, trait_stats, and modifiers
    // do not depend on the chosen amulet — recomputing them per amulet was
    // ~N_amulets wasted work (often >10 amulets per profession in GW2).
    let empty_items_cache: HashMap<u32, gw2_api::models::Item> = HashMap::new();
    struct PvpPrecomputedSpec {
        elite: Option<u32>,
        cores: Vec<u32>,
        trait_ids: Vec<u32>,
        trait_stats: stats::StatBlock,
        modifiers: combat::DamageModifiers,
    }
    let precomputed_specs: Vec<PvpPrecomputedSpec> = spec_combos
        .iter()
        .map(|(elite, cores)| {
            let spec_ids: Vec<u32> = cores.iter().copied().chain(elite.iter().copied()).collect();
            let mut trait_ids = Vec::new();
            for &spec_id in &spec_ids {
                if let Some(spec) = specs_cache.get(&spec_id) {
                    trait_ids.extend(&spec.minor_traits);
                    let best = select_best_major_traits(
                        &spec.major_traits,
                        &stat_weights,
                        traits_cache,
                        locks,
                        spec_id,
                    );
                    trait_ids.extend(best);
                }
            }
            let trait_stats = stats::calculate_trait_stats(&trait_ids, traits_cache);
            let modifiers = combat::extract_damage_modifiers(
                &trait_ids,
                None,
                &[],
                None,
                traits_cache,
                &empty_items_cache,
                ctx,
            );
            PvpPrecomputedSpec {
                elite: *elite,
                cores: cores.clone(),
                trait_ids,
                trait_stats,
                modifiers,
            }
        })
        .collect();

    for amulet in amulets_sorted {
        for spec in &precomputed_specs {
            // PvP stat block: base_stats + amulet stats + trait stats (no gear)
            let mut full_stats = stats::base_stats();

            // Apply amulet stats (replaces gear stats)
            for (attr, &value) in &amulet.attributes {
                full_stats.add(attr, value as f64);
            }

            // Apply trait stats (precomputed)
            full_stats += &spec.trait_stats;
            stats::apply_trait_conversions(&mut full_stats, &spec.trait_ids, traits_cache);

            let derived = stats::compute_derived(&full_stats, &profession.name);

            let combat_perf = combat::calculate_combat_performance(
                &full_stats,
                &derived,
                &spec.modifiers,
                solo_profile,
                &cw,
                &profession.name,
                ctx,
            );
            let score = score_with_weights(&combat_perf, weights);
            let (data_quality, quality_reasons) =
                quality_from_modifiers(&spec.modifiers, &[], false, ctx.game_mode.label());

            all_candidates.push(BuildCandidate {
                gear: empty_gear.clone(),
                elite_spec: spec.elite,
                core_specs: spec.cores.clone(),
                equipped_traits: spec.trait_ids.clone(),
                stats: full_stats,
                derived,
                score,
                combat: combat_perf,
                modifiers: spec.modifiers.clone(),
                pvp_amulet: Some(PvpAmuletCandidate {
                    id: amulet.id,
                    name: amulet.name.clone(),
                    stats: amulet.attributes.clone(),
                }),
                data_quality,
                quality_reasons,
            });
        }
    }

    on_progress(OptimizeProgress {
        stage: "Ranking PvP candidates...".into(),
        done: false,
    });

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

    Ok(all_candidates)
}

/// Calculate approximate stats for a gear candidate using slot budget data.
/// Looks up per-slot budget values from loaded `SlotBudgets` data.
fn calculate_candidate_stats(
    candidate: &GearCandidate,
    itemstats_cache: &HashMap<u32, ItemStat>,
) -> stats::StatBlock {
    let mut stats = stats::StatBlock::default();
    let budgets = data::slot_budgets::slot_budgets();

    for (slot, &stat_id) in &candidate.slot_stats {
        let Some(itemstat) = itemstats_cache.get(&stat_id) else {
            continue;
        };

        let Some(slot_type) = data::SlotType::from_api_slot(slot) else {
            continue;
        };
        let shape = data::stat_shape_from_attr_count(itemstat.attributes.len());
        let Some(budget) = budgets.get(slot_type, shape) else {
            continue;
        };

        add_budget_stats_for_itemstat(&mut stats, itemstat, budget);
    }

    stats
}

/// PvE/WvW: 16 slot budgets from the prefix. PvP: matching amulet attributes only.
pub fn apply_optimized_gear_stats(
    stats: &mut stats::StatBlock,
    db: &GameDb,
    prefix_id: Option<u32>,
    ctx: &BalanceContext,
) {
    let Some(id) = prefix_id else {
        return;
    };
    let Some(itemstat) = db.itemstats.get(&id) else {
        return;
    };
    if ctx.game_mode == GameMode::PvP {
        if let Some(amulet) = match_pvp_amulet(db, &itemstat.name) {
            for (attr, &value) in &amulet.attributes {
                stats.add(attr, value as f64);
            }
            return;
        }
    }
    let budgets = data::slot_budgets::slot_budgets();
    let shape = data::stat_shape_from_attr_count(itemstat.attributes.len());
    for &(slot_type, _) in data::EQUIPMENT_SLOTS {
        if let Some(budget) = budgets.get(slot_type, shape) {
            add_budget_stats_for_itemstat(stats, itemstat, budget);
        }
    }
}

/// Match a PvE prefix name (e.g. "Berserker's") to a PvP amulet ("Berserker Amulet").
pub fn match_pvp_amulet<'a>(db: &'a GameDb, prefix_name: &str) -> Option<&'a PvpAmulet> {
    let needle = prefix_name.trim_end_matches("'s").trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let mut best: Option<(&PvpAmulet, usize)> = None;
    for a in db.pvp_amulets.values() {
        let n = a.name.to_lowercase();
        let stem = n.replace(" amulet", "");
        if !(n.contains(&needle) || stem.contains(&needle) || needle.contains(&stem)) {
            continue;
        }
        let dist = n.len().abs_diff(needle.len());
        match best {
            None => best = Some((a, dist)),
            Some((_, d)) if dist < d => best = Some((a, dist)),
            Some((prev, d)) if dist == d && a.id < prev.id => best = Some((a, dist)),
            _ => {}
        }
    }
    best.map(|(a, _)| a)
}

pub fn quality_from_modifiers(
    modifiers: &DamageModifiers,
    warnings: &[String],
    has_errors: bool,
    mode: &str,
) -> (data::DataQuality, Vec<data::DataQualityReason>) {
    let mut quality = data::DataQuality::Verified;
    let mut reasons = Vec::new();
    if !warnings.is_empty() {
        quality = quality.merge(&data::DataQuality::Provisional);
        for w in warnings {
            reasons.push(data::DataQualityReason {
                field: "validated_build.warning".into(),
                entity: mode.into(),
                modes: vec![mode.to_string()],
                explanation: w.clone(),
            });
        }
    }
    if has_errors {
        quality = quality.merge(&data::DataQuality::Blocked);
        reasons.push(data::DataQualityReason {
            field: "validated_build.error".into(),
            entity: mode.into(),
            modes: vec![mode.to_string()],
            explanation: "Validation errors present".into(),
        });
    }
    if !modifiers.unparsed.is_empty() {
        quality = quality.merge(&data::DataQuality::Provisional);
        reasons.push(data::DataQualityReason {
            field: "modifiers.unparsed".into(),
            entity: mode.into(),
            modes: vec![mode.to_string()],
            explanation: format!(
                "{} bonus string(s) had % but no known category: {}",
                modifiers.unparsed.len(),
                modifiers
                    .unparsed
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        });
    }
    (quality, reasons)
}

/// Add stat values from a slot budget entry, classifying each itemstat
/// attribute as major or minor based on its multiplier relative to the
/// highest multiplier in the set.
pub fn add_budget_stats_for_itemstat(
    stats: &mut stats::StatBlock,
    itemstat: &ItemStat,
    budget: &data::slot_budgets::SlotBudgetEntry,
) {
    if itemstat.attributes.is_empty() {
        return;
    }
    let max_mult = itemstat
        .attributes
        .iter()
        .map(|a| a.multiplier)
        .fold(f64::NEG_INFINITY, f64::max);
    for attr in &itemstat.attributes {
        // An attribute is "major" if its multiplier is the highest (or within
        // a small tolerance to handle floating-point). For CelestialLike,
        // all multipliers are equal, and major == minor in the budget.
        let value = if (attr.multiplier - max_mult).abs() < 0.001 {
            budget.major as f64
        } else {
            budget.minor as f64
        };
        stats.add(&attr.attribute, value);
    }
}

/// Select the best major trait from each column (Adept/Master/Grandmaster) for an archetype.
/// GW2 specialization major_traits layout: [A1, A2, A3, M1, M2, M3, G1, G2, G3]
/// Each column has 3 choices; the player picks 1 per column = 3 total.
/// This heuristic scores each trait's stat contributions + damage modifier relevance
/// against the archetype weights and picks the best per column.
fn select_best_major_traits(
    major_traits: &[u32],
    stat_weights: &StatWeights,
    traits_cache: &HashMap<u32, GW2Trait>,
    locks: &gw2_core::types::BuildLocks,
    spec_id: u32,
) -> Vec<u32> {
    if major_traits.len() != 9 {
        // Unexpected layout — return all as fallback (some specs may have fewer)
        return major_traits.to_vec();
    }

    let weights = stat_weights;
    let mut selected = Vec::with_capacity(3);
    let trait_lock = locks.trait_locks.get(&spec_id);

    // Process 3 columns: [0..3], [3..6], [6..9]
    for (col_idx, col_start) in (0..9).step_by(3).enumerate() {
        let column = &major_traits[col_start..col_start + 3];

        // Check if this column is locked
        if let Some(locked_id) = trait_lock.and_then(|t| t[col_idx]) {
            if column.contains(&locked_id) {
                selected.push(locked_id);
                continue;
            }
        }

        let mut best_id = column[0];
        let mut best_score = f64::NEG_INFINITY;

        for &trait_id in column {
            let score = score_trait_for_archetype(trait_id, weights, traits_cache);
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
pub fn score_trait_for_archetype(
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
        let same_spec = traits_cache
            .get(&tf.requires_trait)
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

/// Result of the synergy-driven optimization pipeline.
/// Contains a fully validated build with pre-computed combat metrics at 3 buff tiers.
#[derive(Debug, Clone)]
pub struct SynergyResult {
    pub validated: ValidatedBuild,
    pub stats: stats::StatBlock,
    pub combat_solo: CombatPerformance,
    pub combat_party: CombatPerformance,
    pub combat_squad: CombatPerformance,
    pub modifiers: DamageModifiers,
    pub rotation: Option<rotation::SimulationResult>,
    /// Overall data quality assessment for this result's inputs.
    pub data_quality: data::DataQuality,
    /// Reasons for any data quality degradation.
    pub quality_reasons: Vec<data::DataQualityReason>,
}

/// Stage 1 of the Gemini pipeline: deterministic gear-prefix selection.
/// Returns the authoritative primary prefix (which overrides any Gemini choice)
/// and the tier-based pool of candidates shown to Gemini as context.
fn select_gemini_gear_prefixes(weights: &OptimizationWeights) -> (&'static str, Vec<&'static str>) {
    let gear_match = scoring::select_gear_prefix(weights);
    let tier_prefixes = scoring::select_prefixes_by_tiers(weights);
    let gear_prefixes: Vec<&str> = tier_prefixes.to_vec();
    (gear_match.primary, gear_prefixes)
}

/// Stage 2 of the Gemini pipeline: build the pre-computed profession context
/// string fed into the prompt.
fn build_pre_computed_gemini_context<'a>(
    db: &'a GameDb,
    profession_name: &'a str,
    weights: &'a OptimizationWeights,
    mode_str: &'a str,
    gear_prefixes: Vec<&'a str>,
    determined_prefix: &'a str,
    current_build_summary: Option<&'a str>,
) -> String {
    let context_config = ContextConfig {
        db,
        profession_name,
        weights,
        game_mode: mode_str,
        gear_prefixes,
        current_build_summary,
        determined_prefix: Some(determined_prefix),
    };
    context::build_gemini_context(&context_config)
}

/// Stage 3 of the Gemini pipeline: assemble the final synergy prompt
/// (applies user-imposed spec/trait lock constraints).
// Prompt-assembly stage; each argument is independent prompt input, grouping
// them into a struct would only rename fields, not reduce coupling.
#[allow(clippy::too_many_arguments)]
fn build_gemini_synergy_prompt(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    mode_str: &str,
    pre_computed_context: &str,
    current_build_summary: Option<&str>,
    determined_prefix: &str,
    locks: &gw2_core::types::BuildLocks,
) -> String {
    let lock_constraints = describe_lock_constraints(locks, db);
    let lock_constraint_ref = if lock_constraints.is_empty() {
        None
    } else {
        Some(lock_constraints.as_str())
    };
    prompts::synergy_build_prompt(
        profession_name,
        weights,
        mode_str,
        pre_computed_context,
        current_build_summary,
        Some(determined_prefix),
        lock_constraint_ref,
    )
}

/// Stage 4 of the Gemini pipeline: call the LLM with tool definitions and
/// multi-turn progress reporting. Tool candidates are empty — the LLM is
/// choosing the build, not ranking candidates.
// LLM-call stage; the client, context, db, and progress callback are distinct
// dependencies passed straight through — a params struct adds no clarity.
#[allow(clippy::too_many_arguments)]
fn call_gemini_with_progress(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    llm_client: &dyn LlmClient,
    prompt: &str,
    current_build_summary: Option<&str>,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<String, String> {
    let tools = crate::llm::tools::tool_definitions();
    let tool_ctx = ToolContext {
        db,
        profession_name,
        candidates: &[],
        current_build_summary,
        weights: weights.clone(),
        balance_ctx: ctx,
    };
    let provider_name = llm_client.provider_name().to_string();
    llm_client
        .generate_with_tools_progress(
            prompt,
            &tools,
            &mut |name: &str, args: &serde_json::Value| {
                gemini_tools::execute_tool(name, args, &tool_ctx)
            },
            5,
            &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                let tool_list = if tool_names.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", tool_names.join(", "))
                };
                on_progress(OptimizeProgress {
                    stage: format!(
                        "{} reasoning [{}/{}]{}...",
                        provider_name,
                        turn + 1,
                        max_turns,
                        tool_list
                    ),
                    done: false,
                });
            },
        )
        .map_err(|e| format!("LLM call failed: {}", e))
}

/// Stage 5 of the Gemini pipeline: parse the LLM response and apply the
/// deterministic gear-prefix override. Gemini is unreliable at following gear
/// constraints, so the cosine-similarity selection from Stage 1 is authoritative.
fn parse_and_override_gear_prefix(
    llm_response: &str,
    determined_prefix: &str,
) -> Result<prompts::GeminiBuildResponse, String> {
    let mut parsed = prompts::parse_gemini_build(llm_response)
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;
    parsed.stat_prefix = determined_prefix.to_string();
    Ok(parsed)
}

/// Stage 6 of the Gemini pipeline: validate the parsed response against the
/// GameDb and reject builds with no specializations or any hard error.
fn validate_gemini_response(
    parsed: &prompts::GeminiBuildResponse,
    db: &GameDb,
    profession_name: &str,
) -> Result<ValidatedBuild, String> {
    let validated = validation::validate_gemini_build(parsed, db, profession_name);
    let joined_errors = || {
        validated
            .errors
            .iter()
            .map(|e| e.detail.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    };
    if validated.specializations.is_empty() {
        return Err(format!(
            "Validation failed — no specializations resolved. Errors: {}",
            joined_errors()
        ));
    }
    if !validated.errors.is_empty() {
        return Err(format!(
            "Validation failed — build has hard errors: {}",
            joined_errors()
        ));
    }
    Ok(validated)
}

/// Stage 8 of the Gemini pipeline: compute combat performance at all three
/// buff tiers (solo / party / squad) for the validated build.
fn compute_three_tier_combat(
    full_stats: &stats::StatBlock,
    derived: &stats::DerivedStats,
    modifiers: &DamageModifiers,
    profession_name: &str,
    ctx: &BalanceContext,
) -> (CombatPerformance, CombatPerformance, CombatPerformance) {
    let buff_profiles = combat::buff_profiles_for_profession(profession_name, ctx);
    let cw = combat::condition_weights_for_profession(profession_name, ctx);
    let solo = combat::calculate_combat_performance(
        full_stats,
        derived,
        modifiers,
        &buff_profiles[0],
        &cw,
        profession_name,
        ctx,
    );
    let party = combat::calculate_combat_performance(
        full_stats,
        derived,
        modifiers,
        &buff_profiles[1],
        &cw,
        profession_name,
        ctx,
    );
    let squad = combat::calculate_combat_performance(
        full_stats,
        derived,
        modifiers,
        &buff_profiles[2],
        &cw,
        profession_name,
        ctx,
    );
    (solo, party, squad)
}
/// Run the synergy-driven optimization pipeline.
/// Sends ALL profession data to Gemini in a single prompt for holistic synergy reasoning.
/// Returns a fully validated build with combat metrics at 3 buff tiers.
// Gemini pipeline entry point; arguments are the db, weights, balance context,
// LLM client, and callbacks — grouping them adds indirection without clarity.
#[allow(clippy::too_many_arguments)]
pub fn optimize_with_gemini(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    llm_client: &dyn LlmClient,
    current_build_summary: Option<&str>,
    locks: &gw2_core::types::BuildLocks,
    scenario: Option<&crate::scenario::ScenarioSpec>,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<SynergyResult, String> {
    // 1. Authoritative gear-prefix selection (Gemini cannot override this).
    on_progress(OptimizeProgress {
        stage: "Selecting gear prefix...".into(),
        done: false,
    });
    let (determined_prefix, gear_prefixes) = select_gemini_gear_prefixes(weights);

    // 2. Pre-computed profession context.
    on_progress(OptimizeProgress {
        stage: "Building profession context...".into(),
        done: false,
    });
    let mode_str = match ctx.game_mode {
        GameMode::PvE => "PvE",
        GameMode::PvP => "PvP",
        GameMode::WvW => "WvW",
    };
    let pre_computed_context = build_pre_computed_gemini_context(
        db,
        profession_name,
        weights,
        mode_str,
        gear_prefixes,
        determined_prefix,
        current_build_summary,
    );

    // 3. Synergy prompt with lock constraints.
    on_progress(OptimizeProgress {
        stage: "Preparing Gemini prompt...".into(),
        done: false,
    });
    let prompt = build_gemini_synergy_prompt(
        db,
        profession_name,
        weights,
        mode_str,
        &pre_computed_context,
        current_build_summary,
        determined_prefix,
        locks,
    );

    // 4. LLM call (multi-turn tool-use progress emitted inside the helper).
    on_progress(OptimizeProgress {
        stage: format!(
            "{} reasoning about synergies...",
            llm_client.provider_name()
        ),
        done: false,
    });
    let llm_response = call_gemini_with_progress(
        db,
        profession_name,
        weights,
        ctx,
        llm_client,
        &prompt,
        current_build_summary,
        on_progress,
    )?;

    // 5. Parse + deterministic gear-prefix override.
    on_progress(OptimizeProgress {
        stage: "Parsing Gemini build...".into(),
        done: false,
    });
    let parsed = parse_and_override_gear_prefix(&llm_response, determined_prefix)?;

    // 6. Validate against GameDb.
    on_progress(OptimizeProgress {
        stage: "Validating build...".into(),
        done: false,
    });
    let validated = validate_gemini_response(&parsed, db, profession_name)?;

    // 7. Stats from validated gear prefix + trait modifiers.
    on_progress(OptimizeProgress {
        stage: "Calculating stats...".into(),
        done: false,
    });
    let (full_stats, modifiers) = calculate_validated_stats(&validated, db, profession_name, ctx);
    let derived = stats::compute_derived(&full_stats, profession_name);

    // 8. 3-tier combat performance.
    on_progress(OptimizeProgress {
        stage: "Computing combat performance...".into(),
        done: false,
    });
    let (combat_solo, combat_party, combat_squad) =
        compute_three_tier_combat(&full_stats, &derived, &modifiers, profession_name, ctx);

    // 9. Rotation simulation from validated skills.
    on_progress(OptimizeProgress {
        stage: "Simulating rotation...".into(),
        done: false,
    });
    let rotation_result = simulate_validated_rotation(&validated, db, &full_stats, scenario);

    on_progress(OptimizeProgress {
        stage: "Done".into(),
        done: true,
    });

    let (data_quality, quality_reasons) = quality_from_modifiers(
        &modifiers,
        &validated.warnings,
        !validated.errors.is_empty(),
        ctx.game_mode.label(),
    );
    Ok(SynergyResult {
        validated,
        stats: full_stats,
        combat_solo,
        combat_party,
        combat_squad,
        modifiers,
        rotation: rotation_result,
        data_quality,
        quality_reasons,
    })
}

/// Calculate stats from a validated build: gear prefix + trait bonuses + conversions.
pub fn calculate_validated_stats(
    validated: &ValidatedBuild,
    db: &GameDb,
    _profession_name: &str,
    ctx: &BalanceContext,
) -> (stats::StatBlock, DamageModifiers) {
    let mut full_stats = stats::base_stats();

    apply_optimized_gear_stats(
        &mut full_stats,
        db,
        validated.gear_prefix.as_ref().map(|p| p.itemstat_id),
        ctx,
    );

    // Rune and sigil flat stat bonuses (permanent stats only).
    let rune_id = validated.rune.as_ref().map(|r| r.id);
    let sigil_ids: Vec<u32> = validated.sigils.iter().map(|s| s.id).collect();
    let rune_stats = stats::calculate_rune_stats(rune_id, &db.items);
    full_stats += &rune_stats;
    let sigil_stats = stats::calculate_sigil_stats(&sigil_ids, &db.items);
    full_stats += &sigil_stats;

    // Collect all trait IDs from validated specializations
    let all_trait_ids: Vec<u32> = validated
        .specializations
        .iter()
        .flat_map(|s| s.all_trait_ids.iter().copied())
        .collect();

    // Trait stats
    let trait_stats = stats::calculate_trait_stats(&all_trait_ids, &db.traits);
    full_stats += &trait_stats;
    stats::apply_trait_conversions(&mut full_stats, &all_trait_ids, &db.traits);

    // Extract damage modifiers from traits + rune + sigils + relic
    let relic_id = validated.relic.as_ref().map(|r| r.id);
    let modifiers = combat::extract_damage_modifiers(
        &all_trait_ids,
        rune_id,
        &sigil_ids,
        relic_id,
        &db.traits,
        &db.items,
        ctx,
    );

    (full_stats, modifiers)
}

/// Simulate a rotation from validated skill IDs.
pub fn simulate_validated_rotation(
    validated: &ValidatedBuild,
    db: &GameDb,
    stats: &stats::StatBlock,
    scenario: Option<&crate::scenario::ScenarioSpec>,
) -> Option<rotation::SimulationResult> {
    // Heal/utility/elite stay at weapon_set 0 (always available); weapon skills
    // get tagged with their actual set 1 or 2 so the simulator's weapon-swap
    // logic in `is_skill_available` and `should_weapon_swap` can decide when to
    // swap. Previously all skills defaulted to set 0, making the simulator
    // treat both weapon sets as simultaneously available — no swap, set 2
    // skills usable while set 1 active.
    let mut non_weapon_ids: Vec<u32> = Vec::new();

    if let Some((id, _)) = &validated.skills.heal {
        non_weapon_ids.push(*id);
    }
    for (id, _) in validated.skills.utilities.iter().flatten() {
        non_weapon_ids.push(*id);
    }
    if let Some((id, _)) = &validated.skills.elite {
        non_weapon_ids.push(*id);
    }

    // Resolve weapon skills from validated weapon types
    let profession_name = if let Some(spec) = validated.specializations.first() {
        // Use the profession from the spec
        db.specializations
            .get(&spec.spec_id)
            .map(|s| s.profession.as_str())
            .unwrap_or("")
    } else {
        ""
    };

    let mut set1_ids: Vec<u32> = Vec::new();
    let mut set2_ids: Vec<u32> = Vec::new();

    // Find weapon skills for each weapon set. `db.profession(name)` is an O(1)
    // HashMap lookup keyed on id (which equals the name for GW2 professions).
    if let Some(profession) = db.profession(profession_name) {
        if let Some(ref main) = validated.weapons.set1.main_hand {
            add_weapon_skill_ids(&mut set1_ids, profession, main, db, 1);
        }
        if let Some(ref off) = validated.weapons.set1.off_hand {
            add_weapon_skill_ids(&mut set1_ids, profession, off, db, 1);
        }
        if let Some(ref main) = validated.weapons.set2.main_hand {
            add_weapon_skill_ids(&mut set2_ids, profession, main, db, 2);
        }
        if let Some(ref off) = validated.weapons.set2.off_hand {
            add_weapon_skill_ids(&mut set2_ids, profession, off, db, 2);
        }
    }

    if non_weapon_ids.is_empty() && set1_ids.is_empty() && set2_ids.is_empty() {
        return None;
    }

    let mut rotation_skills = rotation::builder::build_rotation_skills(&non_weapon_ids, db);
    let mut set1_skills = rotation::builder::build_rotation_skills(&set1_ids, db);
    rotation::builder::tag_weapon_set(&mut set1_skills, 1);
    let mut set2_skills = rotation::builder::build_rotation_skills(&set2_ids, db);
    rotation::builder::tag_weapon_set(&mut set2_skills, 2);
    rotation_skills.extend(set1_skills);
    rotation_skills.extend(set2_skills);
    let mode = scenario.map(|s| s.game_mode.label()).unwrap_or("PvE");
    let ne = crate::data::normalized_effects::effects().effects_for_mode(mode);
    rotation::builder::enrich_with_cleanse(&mut rotation_skills, ne, db);

    if rotation_skills.is_empty() {
        return None;
    }

    let power = stats.get("Power");
    let condition_damage = stats.get("ConditionDamage");
    let precision = stats.get("Precision");
    let ferocity = stats.get("Ferocity");
    let weapon_strength = 1100.0;

    let duration_ms = scenario
        .map(|s| crate::rotation::combat_model::simulation_window_ms(s.combat_tier, s.combat_kind))
        .unwrap_or(0);

    let enemy = scenario
        .map(|s| {
            crate::rotation::combat_model::EnemyDummy::for_scenario(s.combat_tier, s.combat_kind)
        })
        .unwrap_or_default();

    let mode = scenario
        .map(|s| s.game_mode.clone())
        .unwrap_or(GameMode::PvE);
    let sim_ctx = BalanceContext::new(mode.clone());
    let (_, mods) = calculate_validated_stats(validated, db, profession_name, &sim_ctx);
    let params = rotation::simulator::SimParams {
        power,
        condition_damage,
        weapon_strength,
        precision,
        ferocity,
        strike_mult: mods.total_strike_mult(),
        mode,
    };

    Some(rotation::simulator::simulate_with(
        &rotation_skills,
        duration_ms,
        &params,
        enemy,
    ))
}

/// Add weapon skill IDs for a given weapon type from the profession's weapon data.
fn add_weapon_skill_ids(
    skill_ids: &mut Vec<u32>,
    profession: &Profession,
    weapon_type: &str,
    db: &GameDb,
    _weapon_set: u8,
) {
    if let Some(weapon_info) = profession.weapons.get(weapon_type) {
        for skill_ref in &weapon_info.skills {
            let id = skill_ref.id;
            if db.skills.contains_key(&id) {
                skill_ids.push(id);
            }
        }
    }
}

/// Convert a `ValidatedBuild` into a `SynergyResult` by computing stats, combat
/// metrics, and rotation simulation.  This is used by `optimize_v2()` to package
/// the beam-search winner as the standard output type.
pub fn synergy_result_from_validated(
    validated: ValidatedBuild,
    db: &GameDb,
    profession_name: &str,
    ctx: &BalanceContext,
    scenario: Option<&crate::scenario::ScenarioSpec>,
) -> SynergyResult {
    let (full_stats, modifiers) = calculate_validated_stats(&validated, db, profession_name, ctx);
    let derived = stats::compute_derived(&full_stats, profession_name);
    let buff_profiles = combat::buff_profiles_for_profession(profession_name, ctx);
    let cw = combat::condition_weights_for_profession(profession_name, ctx);
    let combat_solo = combat::calculate_combat_performance(
        &full_stats,
        &derived,
        &modifiers,
        &buff_profiles[0],
        &cw,
        profession_name,
        ctx,
    );
    let combat_party = combat::calculate_combat_performance(
        &full_stats,
        &derived,
        &modifiers,
        &buff_profiles[1],
        &cw,
        profession_name,
        ctx,
    );
    let combat_squad = combat::calculate_combat_performance(
        &full_stats,
        &derived,
        &modifiers,
        &buff_profiles[2],
        &cw,
        profession_name,
        ctx,
    );
    let rotation = simulate_validated_rotation(&validated, db, &full_stats, scenario);
    let (data_quality, quality_reasons) = quality_from_modifiers(
        &modifiers,
        &validated.warnings,
        !validated.errors.is_empty(),
        ctx.game_mode.label(),
    );
    SynergyResult {
        validated,
        stats: full_stats,
        combat_solo,
        combat_party,
        combat_squad,
        modifiers,
        rotation,
        data_quality,
        quality_reasons,
    }
}

/// Run the fully deterministic synergy optimization pipeline.
/// No LLM calls — all selections are algorithmic via synergy scoring.
/// Optional Gemini client is used only for explanation generation (not build selection).
// Deterministic pipeline entry point; the db, weights, context, optional LLM
// client, and callbacks are distinct concerns — a params struct adds no clarity.
#[allow(clippy::too_many_arguments)]
pub fn optimize_deterministic(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    llm_client: Option<&dyn LlmClient>,
    _current_build_summary: Option<&str>,
    locks: &gw2_core::types::BuildLocks,
    scenario: Option<&crate::scenario::ScenarioSpec>,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<SynergyResult, String> {
    // 1. DETERMINISTIC gear prefix selection (reuse existing)
    on_progress(OptimizeProgress {
        stage: "Selecting gear prefix...".into(),
        done: false,
    });
    let gear_match = scoring::select_gear_prefix(weights);
    let determined_prefix = gear_match.primary;

    // 2. Run the full synergy pipeline
    let mut result = crate::synergy_pipeline::optimize_synergy(
        db,
        profession_name,
        weights,
        ctx,
        determined_prefix,
        locks,
        scenario,
        on_progress,
    )?;

    // 3. Optional: LLM explanation pass
    if let Some(client) = llm_client {
        on_progress(OptimizeProgress {
            stage: "Generating build explanation...".into(),
            done: false,
        });

        // Build a compact summary for the LLM
        let specs_summary: Vec<String> = result
            .validated
            .specializations
            .iter()
            .map(|s| {
                let traits_str = s.trait_names.join(", ");
                if s.elite {
                    format!("{} (Elite): {}", s.name, traits_str)
                } else {
                    format!("{}: {}", s.name, traits_str)
                }
            })
            .collect();

        let rune_name = result
            .validated
            .rune
            .as_ref()
            .map(|r| r.name.as_str())
            .unwrap_or("None");
        let sigil_names: Vec<&str> = result
            .validated
            .sigils
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        let relic_name = result
            .validated
            .relic
            .as_ref()
            .map(|r| r.name.as_str())
            .unwrap_or("None");

        let set1 = format!(
            "{}{}",
            result
                .validated
                .weapons
                .set1
                .main_hand
                .as_deref()
                .unwrap_or("?"),
            result
                .validated
                .weapons
                .set1
                .off_hand
                .as_deref()
                .map(|o| format!(" / {}", o))
                .unwrap_or_default()
        );
        let set2 = format!(
            "{}{}",
            result
                .validated
                .weapons
                .set2
                .main_hand
                .as_deref()
                .unwrap_or("?"),
            result
                .validated
                .weapons
                .set2
                .off_hand
                .as_deref()
                .map(|o| format!(" / {}", o))
                .unwrap_or_default()
        );

        let heal = result
            .validated
            .skills
            .heal
            .as_ref()
            .map(|(_, n)| n.as_str())
            .unwrap_or("?");
        let utils: Vec<&str> = result
            .validated
            .skills
            .utilities
            .iter()
            .filter_map(|u| u.as_ref().map(|(_, n)| n.as_str()))
            .collect();
        let elite = result
            .validated
            .skills
            .elite
            .as_ref()
            .map(|(_, n)| n.as_str())
            .unwrap_or("?");

        let summary = format!(
            "Profession: {}\nGear: {}\nSpecializations:\n{}\nWeapons: Set 1: {} | Set 2: {}\n\
             Skills: Heal: {} | Utilities: {} | Elite: {}\n\
             Rune: {}\nSigils: {}\nRelic: {}\n\
             Combat (Solo): Strike DPS {:.0}, Condi DPS {:.0}, Total DPS {:.0}",
            profession_name,
            determined_prefix,
            specs_summary.join("\n"),
            set1,
            set2,
            heal,
            utils.join(", "),
            elite,
            rune_name,
            sigil_names.join(", "),
            relic_name,
            result.combat_solo.strike_dps_index,
            result.combat_solo.condition_dps_index,
            result.combat_solo.total_dps_index,
        );

        let prompt = format!(
            "You are a Guild Wars 2 build expert. Explain why the following build works well together. \
             Describe the key synergy chains between traits, rune, sigils, relic, and skills. \
             Suggest a skill rotation priority. Keep it under 200 words.\n\n{}",
            summary,
        );

        match client.generate(&prompt) {
            Ok(explanation) => {
                result.validated.synergy_explanation = explanation;
            }
            Err(_e) => {
                // LLM explanation failed, keep the template explanation
            }
        }
    }

    on_progress(OptimizeProgress {
        stage: "Done".into(),
        done: true,
    });

    Ok(result)
}

/// Run the v2 beam/evolutionary search.
///
/// Seeds from the synergy pipeline, then performs a bounded beam search over
/// complete build states using the gated referee as the fitness function.
/// If `llm_client` is Some, runs the LLM advisor post-beam to propose
/// additional candidate mutations — the referee is still the final authority.
/// Completes within `SearchConfig::time_limit_secs` (default 28 s).
// Beam-search pipeline entry point; db, weights, context, scenario, and
// optional LLM client are independent inputs — a params struct adds no clarity.
#[allow(clippy::too_many_arguments)]
pub fn optimize_v2(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &crate::scenario::ScenarioSpec,
    locks: &gw2_core::types::BuildLocks,
    llm_client: Option<&dyn LlmClient>,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<SynergyResult, String> {
    use crate::search_v2::SearchConfig;

    on_progress(OptimizeProgress {
        stage: "Running v2 search...".into(),
        done: false,
    });
    let config = SearchConfig::default();
    let mut best = crate::search_v2::optimize_v2_search(
        db,
        profession_name,
        weights,
        ctx,
        scenario,
        locks,
        &config,
        on_progress,
    )?;

    // Optional: LLM advisor pass — propose mutations, referee ranks them.
    if let Some(client) = llm_client {
        on_progress(OptimizeProgress {
            stage: "LLM advisor: evaluating mutations...".into(),
            done: false,
        });
        best = llm_advisor(best, db, profession_name, weights, ctx, scenario, client);
    }

    on_progress(OptimizeProgress {
        stage: "Done".into(),
        done: true,
    });
    Ok(synergy_result_from_validated(
        best,
        db,
        profession_name,
        ctx,
        Some(scenario),
    ))
}

/// Post-beam LLM advisor: ask the LLM for candidate mutations, evaluate each
/// through the referee, return the best improvement found (or original if none better).
///
/// The LLM is a *search policy* — it proposes swaps. The referee decides winners.
/// LLM errors are silently logged and the original build is returned unchanged.
pub fn llm_advisor(
    current: crate::validation::ValidatedBuild,
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &crate::scenario::ScenarioSpec,
    llm_client: &dyn LlmClient,
) -> crate::validation::ValidatedBuild {
    // Build a compact prompt asking for 3 specific swaps to try.
    let current_gear = current
        .gear_prefix
        .as_ref()
        .map(|p| p.name.as_str())
        .unwrap_or("Unknown");
    let current_rune = current
        .rune
        .as_ref()
        .map(|r| r.name.as_str())
        .unwrap_or("None");
    let current_sigils: Vec<&str> = current.sigils.iter().map(|s| s.name.as_str()).collect();

    let prompt = format!(
        "You are a Guild Wars 2 build advisor. The current optimized build for {} uses:\n\
         - Gear prefix: {}\n\
         - Rune: {}\n\
         - Sigils: {}\n\n\
         Game mode: {}. Scoring priorities: Power={:.1}, Condition={:.1}, Sustain={:.1}, Control={:.1}\n\n\
         Suggest exactly 3 alternative gear prefix or rune swaps to try that might score better \
         given these priorities. Format each suggestion as:\n\
         SWAP: gear_prefix=[name] OR rune=[name]\n\
         Only suggest changes to gear_prefix or rune. Do not suggest spec changes.",
        profession_name,
        current_gear,
        current_rune,
        current_sigils.join(", "),
        ctx.game_mode.label(),
        weights.power, weights.condition, weights.sustain, weights.control,
    );

    // Get current score for comparison baseline.
    let current_report = crate::referee::evaluate_validated_build(
        &current,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
    );
    let baseline_score = current_report.user_intent_score;

    // Call LLM.
    let response = match llm_client.generate(&prompt) {
        Ok(r) => r,
        Err(_) => {
            // LLM advisor failure is non-fatal — return original build silently.
            return current;
        }
    };

    // Parse SWAP: lines from response.
    let mut best_validated = current.clone();
    let mut best_score = baseline_score;

    for line in response.lines() {
        let line = line.trim();
        if !line.starts_with("SWAP:") {
            continue;
        }
        let swap_part = &line["SWAP:".len()..].trim().to_lowercase();

        let mut candidate = current.clone();

        if let Some(rest) = swap_part.strip_prefix("gear_prefix=") {
            let prefix_name = rest.trim().trim_matches('"').trim_matches('\'');
            // Use the centralized deterministic helper (exact match wins, else
            // shortest-fuzzy with id tiebreak). Previously this called
            // `to_lowercase()` on every itemstat in the loop.
            if let Some(item_stat) = db.itemstat_by_name(prefix_name) {
                candidate.gear_prefix = Some(crate::validation::ValidatedGearPrefix {
                    itemstat_id: item_stat.id,
                    name: item_stat.name.clone(),
                });
            } else {
                continue; // Skip if prefix not found in DB
            }
        } else if let Some(rest) = swap_part.strip_prefix("rune=") {
            let rune_name = rest.trim().trim_matches('"').trim_matches('\'');
            // Hoist the needle lowercase once so we don't re-allocate it on
            // every item probed.
            let rune_needle = rune_name.to_lowercase();
            let found_rune = db.runes.iter().find_map(|&id| {
                db.items
                    .get(&id)
                    .filter(|item| item.name.to_lowercase().contains(&rune_needle))
                    .map(|item| crate::validation::ValidatedItem {
                        id: item.id,
                        name: item.name.clone(),
                    })
            });
            match found_rune {
                Some(r) => candidate.rune = Some(r),
                None => continue,
            }
        } else {
            continue;
        }

        // Evaluate the mutation through the referee.
        let report = crate::referee::evaluate_validated_build(
            &candidate,
            db,
            profession_name,
            weights,
            ctx,
            scenario,
        );
        if report.user_intent_score > best_score {
            best_score = report.user_intent_score;
            best_validated = candidate;
        }
    }

    best_validated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_budget_lookups() {
        // Verify slot budget data is accessible and returns expected ThreeStat major values.
        // Source: data/slot_budgets/level80_ascended.json (verified from GW2 API items)
        let budgets = data::slot_budgets::slot_budgets();
        assert_eq!(
            budgets.major_for_api_slot("Coat"),
            141,
            "Coat ThreeStat major should be 141"
        );
        assert_eq!(
            budgets.major_for_api_slot("Helm"),
            63,
            "Helm ThreeStat major should be 63"
        );
        assert_eq!(
            budgets.major_for_api_slot("Amulet"),
            157,
            "Amulet ThreeStat major should be 157"
        );
        assert_eq!(
            budgets.major_for_api_slot("Leggings"),
            94,
            "Leggings ThreeStat major should be 94"
        );
        // WeaponA1 maps to WeaponTwoHand
        assert_eq!(
            budgets.major_for_api_slot("WeaponA1"),
            251,
            "WeaponA1 (TwoHand) ThreeStat major should be 251"
        );
        // WeaponA2 maps to WeaponOneHand
        assert_eq!(
            budgets.major_for_api_slot("WeaponA2"),
            125,
            "WeaponA2 (OneHand) ThreeStat major should be 125"
        );
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
        let power_weights = OptimizationWeights::preset_power_dps().to_stat_weights();
        let no_locks = gw2_core::types::BuildLocks::default();
        let selected =
            select_best_major_traits(&major_traits, &power_weights, &traits_cache, &no_locks, 0);
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
        traits_cache.insert(
            100,
            GW2Trait {
                id: 100,
                name: "Power Trait".into(),
                tier: 1,
                order: 0,
                description: None,
                slot: "Major".into(),
                icon: None,
                specialization: 1,
                skills: vec![],
                facts: vec![Fact::AttributeAdjust {
                    text: Some("Power".into()),
                    icon: None,
                    value: Some(150),
                    target: Some("Power".into()),
                }],
                traited_facts: vec![],
            },
        );
        // Trait 101: gives +150 Vitality (bad for PowerDPS)
        traits_cache.insert(
            101,
            GW2Trait {
                id: 101,
                name: "Vitality Trait".into(),
                tier: 1,
                order: 1,
                description: None,
                slot: "Major".into(),
                icon: None,
                specialization: 1,
                skills: vec![],
                facts: vec![Fact::AttributeAdjust {
                    text: Some("Vitality".into()),
                    icon: None,
                    value: Some(150),
                    target: Some("Vitality".into()),
                }],
                traited_facts: vec![],
            },
        );
        // Trait 102: nothing
        traits_cache.insert(
            102,
            GW2Trait {
                id: 102,
                name: "Empty Trait".into(),
                tier: 1,
                order: 2,
                description: None,
                slot: "Major".into(),
                icon: None,
                specialization: 1,
                skills: vec![],
                facts: vec![],
                traited_facts: vec![],
            },
        );

        let power_weights = OptimizationWeights::preset_power_dps().to_stat_weights();
        let no_locks = gw2_core::types::BuildLocks::default();
        let selected =
            select_best_major_traits(&major_traits, &power_weights, &traits_cache, &no_locks, 1);
        // First column should select trait 100 (Power bonus)
        assert_eq!(
            selected[0], 100,
            "PowerDPS should prefer Power trait over Vitality"
        );
    }

    #[test]
    fn test_optimize_returns_candidates() {
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
            specs.insert(
                id,
                Specialization {
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
                },
            );
        }

        let no_locks = gw2_core::types::BuildLocks::default();
        let ctx = crate::balance::BalanceContext::pve();
        let candidates = optimize(
            &profession,
            &OptimizationWeights::preset_power_dps(),
            None,
            &HashMap::new(),
            &itemstats,
            &specs,
            &HashMap::new(),
            |_| {},
            3,
            &ctx,
            &no_locks,
            &HashMap::new(), // no PvP amulets needed for PvE
        )
        .expect("optimize() should succeed with valid data");

        assert!(!candidates.is_empty());
        // Should be sorted by score descending
        for i in 1..candidates.len() {
            assert!(candidates[i - 1].score >= candidates[i].score);
        }
    }

    /// Helper to build a minimal Warrior profession with 5 core specs for PvP tests.
    fn test_warrior_profession_and_specs() -> (Profession, HashMap<u32, Specialization>) {
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
            specs.insert(
                id,
                Specialization {
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
                },
            );
        }
        (profession, specs)
    }

    #[test]
    fn test_pvp_mode_dispatches_to_pvp_path() {
        let (profession, specs) = test_warrior_profession_and_specs();
        let mut pvp_amulets = HashMap::new();
        pvp_amulets.insert(
            4,
            PvpAmulet {
                id: 4,
                name: "Assassin Amulet".into(),
                icon: None,
                attributes: {
                    let mut m = HashMap::new();
                    m.insert("Power".into(), 900);
                    m.insert("Precision".into(), 1200);
                    m.insert("CritDamage".into(), 900);
                    m
                },
            },
        );

        let no_locks = gw2_core::types::BuildLocks::default();
        let ctx = crate::balance::BalanceContext::pvp();
        let candidates = optimize(
            &profession,
            &OptimizationWeights::preset_power_dps(),
            None,
            &HashMap::new(),
            &HashMap::new(), // no itemstats needed for PvP
            &specs,
            &HashMap::new(),
            |_| {},
            3,
            &ctx,
            &no_locks,
            &pvp_amulets,
        )
        .expect("PvP optimize should succeed with amulet data");

        assert!(!candidates.is_empty());
        // All PvP candidates should have a pvp_amulet set
        for c in &candidates {
            assert!(
                c.pvp_amulet.is_some(),
                "PvP candidate should have pvp_amulet set"
            );
        }
    }

    #[test]
    fn test_pvp_amulet_stats_applied_to_base() {
        let (profession, specs) = test_warrior_profession_and_specs();
        let mut pvp_amulets = HashMap::new();
        pvp_amulets.insert(
            4,
            PvpAmulet {
                id: 4,
                name: "Assassin Amulet".into(),
                icon: None,
                attributes: {
                    let mut m = HashMap::new();
                    m.insert("Power".into(), 900);
                    m.insert("Precision".into(), 1200);
                    m.insert("CritDamage".into(), 900);
                    m
                },
            },
        );

        let no_locks = gw2_core::types::BuildLocks::default();
        let ctx = crate::balance::BalanceContext::pvp();
        let candidates = optimize(
            &profession,
            &OptimizationWeights::preset_power_dps(),
            None,
            &HashMap::new(),
            &HashMap::new(),
            &specs,
            &HashMap::new(),
            |_| {},
            3,
            &ctx,
            &no_locks,
            &pvp_amulets,
        )
        .expect("PvP optimize should succeed");

        // With Assassin Amulet: base 1000 + amulet 900 Power = 1900
        let c = &candidates[0];
        assert!(
            (c.stats.power - 1900.0).abs() < 1.0,
            "Power should be base 1000 + 900 amulet = 1900, got {}",
            c.stats.power
        );
        assert!(
            (c.stats.precision - 2200.0).abs() < 1.0,
            "Precision should be base 1000 + 1200 amulet = 2200, got {}",
            c.stats.precision
        );
        // CritDamage maps to ferocity (base 0 + 900)
        assert!(
            (c.stats.ferocity - 900.0).abs() < 1.0,
            "Ferocity should be 0 base + 900 amulet = 900, got {}",
            c.stats.ferocity
        );
    }

    #[test]
    fn test_pvp_error_on_empty_amulets() {
        let (profession, specs) = test_warrior_profession_and_specs();
        let no_locks = gw2_core::types::BuildLocks::default();
        let ctx = crate::balance::BalanceContext::pvp();
        let result = optimize(
            &profession,
            &OptimizationWeights::preset_power_dps(),
            None,
            &HashMap::new(),
            &HashMap::new(),
            &specs,
            &HashMap::new(),
            |_| {},
            3,
            &ctx,
            &no_locks,
            &HashMap::new(), // empty pvp_amulets
        );

        assert!(
            result.is_err(),
            "PvP optimization should error with no amulet data"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("No PvP amulet data"),
            "Error should mention missing amulet data, got: {}",
            err
        );
    }

    #[test]
    fn test_pvp_no_slot_budgets_used() {
        // PvP path should work even with no itemstats (slot budgets not used)
        let (profession, specs) = test_warrior_profession_and_specs();
        let mut pvp_amulets = HashMap::new();
        pvp_amulets.insert(
            1,
            PvpAmulet {
                id: 1,
                name: "Test Amulet".into(),
                icon: None,
                attributes: {
                    let mut m = HashMap::new();
                    m.insert("Power".into(), 500);
                    m
                },
            },
        );

        let no_locks = gw2_core::types::BuildLocks::default();
        let ctx = crate::balance::BalanceContext::pvp();
        // Pass completely empty itemstats — PvP path should not need them
        let result = optimize(
            &profession,
            &OptimizationWeights::preset_power_dps(),
            None,
            &HashMap::new(),
            &HashMap::new(), // empty itemstats
            &specs,
            &HashMap::new(),
            |_| {},
            3,
            &ctx,
            &no_locks,
            &pvp_amulets,
        );

        assert!(
            result.is_ok(),
            "PvP path should succeed without itemstats: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_pve_candidates_have_no_pvp_amulet() {
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
        let (profession, specs) = test_warrior_profession_and_specs();
        let no_locks = gw2_core::types::BuildLocks::default();
        let ctx = crate::balance::BalanceContext::pve();
        let candidates = optimize(
            &profession,
            &OptimizationWeights::preset_power_dps(),
            None,
            &HashMap::new(),
            &itemstats,
            &specs,
            &HashMap::new(),
            |_| {},
            3,
            &ctx,
            &no_locks,
            &HashMap::new(),
        )
        .expect("PvE optimize should succeed");

        for c in &candidates {
            assert!(
                c.pvp_amulet.is_none(),
                "PvE candidates should have pvp_amulet = None"
            );
        }
    }

    #[test]
    fn simulate_validated_rotation_counts_cleanse_from_skill_facts() {
        let cleanse_skill = gw2_api::models::Skill {
            id: 90_001,
            name: "Cleanse Utility".into(),
            description: None,
            icon: None,
            chat_link: None,
            skill_type: None,
            weapon_type: None,
            professions: vec!["Warrior".into()],
            slot: Some("Utility".into()),
            facts: vec![
                Fact::Recharge {
                    text: Some("Recharge".into()),
                    icon: None,
                    value: Some(20.0),
                },
                Fact::Number {
                    text: Some("Conditions Removed".into()),
                    icon: None,
                    value: Some(2),
                },
            ],
            traited_facts: vec![],
            categories: vec![],
            attunement: None,
            cost: None,
            dual_wield: None,
            flip_skill: None,
            initiative: None,
            next_chain: None,
            prev_chain: None,
            transform_skills: vec![],
            bundle_skills: vec![],
            toolbelt_skill: None,
            flags: vec![],
            specialization: None,
        };

        let mut db = GameDb {
            items: HashMap::new(),
            itemstats: HashMap::new(),
            skills: HashMap::new(),
            traits: HashMap::new(),
            specializations: HashMap::new(),
            professions: HashMap::new(),
            legends: HashMap::new(),
            pvp_amulets: HashMap::new(),
            skills_by_profession: HashMap::new(),
            traits_by_spec: HashMap::new(),
            items_by_type: HashMap::new(),
            runes: vec![],
            sigils: vec![],
            relics: vec![],
            skill_to_palette: HashMap::new(),
            palette_to_skill: HashMap::new(),
            traits_by_condition: HashMap::new(),
            skills_by_condition: HashMap::new(),
            traits_by_buff: HashMap::new(),
            skills_by_buff: HashMap::new(),
        };
        db.skills.insert(cleanse_skill.id, cleanse_skill);

        let mut validated = ValidatedBuild::default();
        validated.skills.utilities = vec![Some((90_001, "Cleanse Utility".into()))];

        let stats = stats::base_stats();
        let rotation = simulate_validated_rotation(&validated, &db, &stats, None)
            .expect("utility skill should produce a rotation");

        assert_eq!(rotation.cleanse_count, 1);
        assert!(
            rotation.cleanse_rate_per_20s > 0.0,
            "cleanse fact should contribute to cleanse rate"
        );
    }
}
