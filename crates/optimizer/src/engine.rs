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
use crate::llm::LlmClient;
use crate::gemini_tools::{self, ToolContext};
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
            let name = db.specializations.get(id).map(|s| s.name.as_str()).unwrap_or("Unknown");
            let elite = db.specializations.get(id).is_some_and(|s| s.elite);
            let elite_tag = if elite { " (Elite)" } else { "" };
            parts.push(format!("Slot {} LOCKED to \"{}\"{}", slot + 1, name, elite_tag));

            // Trait locks for this spec
            if let Some(trait_cols) = locks.trait_locks.get(id) {
                for (col, trait_id) in trait_cols.iter().enumerate() {
                    if let Some(tid) = trait_id {
                        let tier = match col { 0 => "Adept", 1 => "Master", _ => "Grandmaster" };
                        let tname = db.traits.get(tid).map(|t| t.name.as_str()).unwrap_or("Unknown");
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
        return optimize_pvp(profession, weights, specs_cache, traits_cache, &mut on_progress, top_n, locks, ctx, pvp_amulets)
            .and_then(|v| if v.is_empty() {
                Err(format!("No PvP candidates found for {} / {}", profession.name, weights.summary_label()))
            } else {
                Ok(v)
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
            weights.summary_label(), itemstats_cache.len()
        ));
    }

    // Score each gear candidate (preliminary — no traits/modifiers yet)
    let empty_mods = DamageModifiers::default();
    let solo_profile = &combat::default_buff_profiles(ctx)[0];
    let cw = combat::condition_weights_for_profession(&profession.name, ctx);
    for candidate in &mut gear_candidates {
        let mock_stats = calculate_candidate_stats(candidate, itemstats_cache);
        let mut full_stats = stats::base_stats();
        full_stats += &mock_stats;
        let derived = stats::compute_derived(&full_stats, &profession.name);
        let perf = combat::calculate_combat_performance(
            &full_stats, &derived, &empty_mods, solo_profile,
            &cw,
            &profession.name,
            ctx,
        );
        candidate.score = score_with_weights(&perf, weights);
    }

    // Sort by score descending
    gear_candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    gear_candidates.truncate(top_n * 3); // keep extra — traits can shift rankings significantly

    on_progress(OptimizeProgress {
        stage: "Evaluating specialization combinations...".into(),
        done: false,
    });

    // 2. Find valid spec combinations
    let spec_combos = search_spec_combos(&profession.specializations, specs_cache, locks);
    if spec_combos.is_empty() {
        let core_count = profession.specializations.iter()
            .filter(|id| specs_cache.get(id).is_some_and(|s| !s.elite))
            .count();
        let elite_count = profession.specializations.iter()
            .filter(|id| specs_cache.get(id).is_some_and(|s| s.elite))
            .count();
        return Err(format!(
            "No valid spec combinations for {}. Has {} core specs (need ≥3) and {} elite specs. \
             {} of {} spec IDs found in GameDb.",
            profession.name, core_count, elite_count,
            profession.specializations.iter().filter(|id| specs_cache.contains_key(id)).count(),
            profession.specializations.len()
        ));
    }

    let stat_weights = weights.to_stat_weights();

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
                        &spec.major_traits, &stat_weights, traits_cache, locks, spec_id,
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
                &trait_ids, None, &[], None, traits_cache, _items_cache, ctx,
            );

            // Calculate combat performance with Solo profile
            let combat_perf = combat::calculate_combat_performance(
                &full_stats, &derived, &modifiers, solo_profile,
                &cw,
                &profession.name,
                ctx,
            );
            let score = score_with_weights(&combat_perf, weights);

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
                pvp_amulet: None,
                data_quality: data::DataQuality::Verified,
                quality_reasons: vec![],
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
            gear_candidates.len(), spec_combos.len(), profession.name, weights.summary_label()
        ));
    }

    Ok(all_candidates)
}

/// PvP optimization: iterates PvP amulets × spec/trait combos (gear is replaced by amulet system).
/// PvP amulet stats REPLACE gear stats — the stat block is: base_stats + amulet + traits.
/// Slot-budget data is NOT loaded during PvP optimization.
/// Returns an error if no PvP amulet data is available (no silent zero-stat fallback).
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

    let solo_profile = &combat::default_buff_profiles(ctx)[0];
    let stat_weights = weights.to_stat_weights();
    let cw = combat::condition_weights_for_profession(&profession.name, ctx);

    for amulet in pvp_amulets.values() {
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
                        &spec.major_traits, &stat_weights, traits_cache, locks, spec_id,
                    );
                    trait_ids.extend(best);
                }
            }

            // PvP stat block: base_stats + amulet stats + trait stats (no gear)
            let mut full_stats = stats::base_stats();

            // Apply amulet stats (replaces gear stats)
            for (attr, &value) in &amulet.attributes {
                full_stats.add(attr, value as f64);
            }

            // Apply trait stats
            let trait_stats = stats::calculate_trait_stats(&trait_ids, traits_cache);
            full_stats += &trait_stats;
            stats::apply_trait_conversions(&mut full_stats, &trait_ids, traits_cache);

            let derived = stats::compute_derived(&full_stats, &profession.name);

            // Extract modifiers from traits only (PvP has no gear modifiers)
            let modifiers = combat::extract_damage_modifiers(
                &trait_ids, None, &[], None, traits_cache, &HashMap::new(), ctx,
            );
            let combat_perf = combat::calculate_combat_performance(
                &full_stats, &derived, &modifiers, solo_profile,
                &cw,
                &profession.name,
                ctx,
            );
            let score = score_with_weights(&combat_perf, weights);

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
                pvp_amulet: Some(PvpAmuletCandidate {
                    id: amulet.id,
                    name: amulet.name.clone(),
                    stats: amulet.attributes.clone(),
                }),
                data_quality: data::DataQuality::Verified,
                quality_reasons: vec![],
            });
        }
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

/// Run the synergy-driven optimization pipeline.
/// Sends ALL profession data to Gemini in a single prompt for holistic synergy reasoning.
/// Returns a fully validated build with combat metrics at 3 buff tiers.
pub fn optimize_with_gemini(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    llm_client: &dyn LlmClient,
    current_build_summary: Option<&str>,
    locks: &gw2_core::types::BuildLocks,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<SynergyResult, String> {
    // 1. DETERMINISTIC gear prefix selection — this is authoritative, LLM cannot override
    on_progress(OptimizeProgress {
        stage: "Selecting gear prefix...".into(),
        done: false,
    });
    let gear_match = scoring::select_gear_prefix(weights);
    let determined_prefix = gear_match.primary;

    // Also get the tier-based pool for context (Gemini sees what's available)
    let tier_prefixes = scoring::select_prefixes_by_tiers(weights);
    let gear_prefixes: Vec<&str> = tier_prefixes.iter().map(|s| *s).collect();

    // 2. Build comprehensive pre-computed context
    on_progress(OptimizeProgress {
        stage: "Building profession context...".into(),
        done: false,
    });
    let mode_str = match ctx.game_mode {
        GameMode::PvE => "PvE",
        GameMode::PvP => "PvP",
        GameMode::WvW => "WvW",
    };
    let context_config = ContextConfig {
        db,
        profession_name,
        weights,
        game_mode: mode_str,
        gear_prefixes,
        current_build_summary,
        determined_prefix: Some(determined_prefix),
    };
    let pre_computed_context = context::build_gemini_context(&context_config);

    // 3. Build the synergy-focused prompt (includes determined prefix constraint)
    on_progress(OptimizeProgress {
        stage: "Preparing Gemini prompt...".into(),
        done: false,
    });
    let lock_constraints = describe_lock_constraints(locks, db);
    let lock_constraint_ref = if lock_constraints.is_empty() { None } else { Some(lock_constraints.as_str()) };
    let prompt = prompts::synergy_build_prompt(
        profession_name,
        weights,
        mode_str,
        &pre_computed_context,
        current_build_summary,
        Some(determined_prefix),
        lock_constraint_ref,
    );

    // 4. Call LLM with tools available for optional verification
    on_progress(OptimizeProgress {
        stage: format!("{} reasoning about synergies...", llm_client.provider_name()),
        done: false,
    });
    let tools = crate::llm::tools::tool_definitions();
    // Build a minimal ToolContext — candidates are empty since the LLM is choosing the build
    let tool_ctx = ToolContext {
        db,
        profession_name,
        candidates: &[],
        current_build_summary,
        weights: weights.clone(),
        balance_ctx: ctx,
    };
    let provider_name = llm_client.provider_name().to_string();
    let llm_response = llm_client
        .generate_with_tools_progress(
            &prompt,
            &tools,
            &mut |name: &str, args: &serde_json::Value| gemini_tools::execute_tool(name, args, &tool_ctx),
            5, // max 5 tool-calling turns for verification
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
        .map_err(|e| format!("LLM call failed: {}", e))?;

    // 5. Parse the Gemini response and OVERRIDE the gear prefix
    on_progress(OptimizeProgress {
        stage: "Parsing Gemini build...".into(),
        done: false,
    });
    let mut parsed = prompts::parse_gemini_build(&llm_response)
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    // CRITICAL: Override Gemini's stat_prefix with our deterministic choice.
    // Gemini is unreliable at following gear constraints — it frequently picks
    // healing gear regardless of weight settings. The deterministic selector
    // (cosine similarity against purpose profiles) is authoritative.
    parsed.stat_prefix = determined_prefix.to_string();

    // 6. Validate against GameDb
    on_progress(OptimizeProgress {
        stage: "Validating build...".into(),
        done: false,
    });
    let validated = validation::validate_gemini_build(&parsed, db, profession_name);

    // Check for blocking errors: no specializations, OR any hard validation error
    // (e.g., elite-spec-gated weapon without the required spec equipped).
    if validated.specializations.is_empty() {
        return Err(format!(
            "Validation failed — no specializations resolved. Errors: {}",
            validated.errors.join("; ")
        ));
    }
    if !validated.errors.is_empty() {
        return Err(format!(
            "Validation failed — build has hard errors: {}",
            validated.errors.join("; ")
        ));
    }

    // 7. Calculate stats from validated gear prefix + trait modifiers
    on_progress(OptimizeProgress {
        stage: "Calculating stats...".into(),
        done: false,
    });
    let (full_stats, modifiers) =
        calculate_validated_stats(&validated, db, profession_name, ctx);

    let derived = stats::compute_derived(&full_stats, profession_name);

    // 8. Compute 3-tier combat performance
    on_progress(OptimizeProgress {
        stage: "Computing combat performance...".into(),
        done: false,
    });
    let buff_profiles = combat::default_buff_profiles(ctx);
    let cw = combat::condition_weights_for_profession(profession_name, ctx);
    let combat_solo = combat::calculate_combat_performance(
        &full_stats, &derived, &modifiers, &buff_profiles[0], &cw, profession_name, ctx,
    );
    let combat_party = combat::calculate_combat_performance(
        &full_stats, &derived, &modifiers, &buff_profiles[1], &cw, profession_name, ctx,
    );
    let combat_squad = combat::calculate_combat_performance(
        &full_stats, &derived, &modifiers, &buff_profiles[2], &cw, profession_name, ctx,
    );

    // 9. Simulate rotation from validated skills
    on_progress(OptimizeProgress {
        stage: "Simulating rotation...".into(),
        done: false,
    });
    let rotation_result = simulate_validated_rotation(&validated, db, &full_stats);

    on_progress(OptimizeProgress {
        stage: "Done".into(),
        done: true,
    });

    Ok(SynergyResult {
        validated,
        stats: full_stats,
        combat_solo,
        combat_party,
        combat_squad,
        modifiers,
        rotation: rotation_result,
        data_quality: data::DataQuality::Verified,
        quality_reasons: vec![],
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

    // Gear stats from validated prefix using slot budget data
    if let Some(ref prefix) = validated.gear_prefix {
        if let Some(itemstat) = db.itemstats.get(&prefix.itemstat_id) {
            let budgets = data::slot_budgets::slot_budgets();
            let shape = data::stat_shape_from_attr_count(itemstat.attributes.len());
            for &(slot_type, _) in data::EQUIPMENT_SLOTS {
                if let Some(budget) = budgets.get(slot_type, shape) {
                    add_budget_stats_for_itemstat(
                        &mut full_stats, itemstat, budget,
                    );
                }
            }
        }
    }

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
    let rune_id = validated.rune.as_ref().map(|r| r.id);
    let sigil_ids: Vec<u32> = validated.sigils.iter().map(|s| s.id).collect();
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
) -> Option<rotation::SimulationResult> {
    // Collect all skill IDs from validated skills
    let mut skill_ids: Vec<u32> = Vec::new();

    if let Some((id, _)) = &validated.skills.heal {
        skill_ids.push(*id);
    }
    for util in &validated.skills.utilities {
        if let Some((id, _)) = util {
            skill_ids.push(*id);
        }
    }
    if let Some((id, _)) = &validated.skills.elite {
        skill_ids.push(*id);
    }

    // Resolve weapon skills from validated weapon types
    let profession_name = if let Some(spec) = validated.specializations.first() {
        // Use the profession from the spec
        db.specializations.get(&spec.spec_id)
            .map(|s| s.profession.as_str())
            .unwrap_or("")
    } else {
        ""
    };

    // Find weapon skills for each weapon set
    if let Some(profession) = db.professions.values().find(|p| p.name == profession_name) {
        // Set 1
        if let Some(ref main) = validated.weapons.set1.main_hand {
            add_weapon_skill_ids(&mut skill_ids, profession, main, db, 1);
        }
        if let Some(ref off) = validated.weapons.set1.off_hand {
            add_weapon_skill_ids(&mut skill_ids, profession, off, db, 1);
        }
        // Set 2
        if let Some(ref main) = validated.weapons.set2.main_hand {
            add_weapon_skill_ids(&mut skill_ids, profession, main, db, 2);
        }
        if let Some(ref off) = validated.weapons.set2.off_hand {
            add_weapon_skill_ids(&mut skill_ids, profession, off, db, 2);
        }
    }

    if skill_ids.is_empty() {
        return None;
    }

    let rotation_skills = rotation::builder::build_rotation_skills(&skill_ids, db);
    if rotation_skills.is_empty() {
        return None;
    }

    let power = stats.get("Power");
    let condition_damage = stats.get("ConditionDamage");
    let weapon_strength = 1100.0; // GW2 reference weapon strength

    Some(rotation::simulator::simulate(
        &rotation_skills,
        0, // use default duration
        power,
        condition_damage,
        weapon_strength,
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

/// Run the fully deterministic synergy optimization pipeline.
/// No LLM calls — all selections are algorithmic via synergy scoring.
/// Optional Gemini client is used only for explanation generation (not build selection).
pub fn optimize_deterministic(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    llm_client: Option<&dyn LlmClient>,
    _current_build_summary: Option<&str>,
    locks: &gw2_core::types::BuildLocks,
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
        db, profession_name, weights, ctx, determined_prefix, locks, on_progress,
    )?;

    // 3. Optional: LLM explanation pass
    if let Some(client) = llm_client {
        on_progress(OptimizeProgress {
            stage: "Generating build explanation...".into(),
            done: false,
        });

        // Build a compact summary for the LLM
        let specs_summary: Vec<String> = result.validated.specializations.iter()
            .map(|s| {
                let traits_str = s.trait_names.join(", ");
                if s.elite {
                    format!("{} (Elite): {}", s.name, traits_str)
                } else {
                    format!("{}: {}", s.name, traits_str)
                }
            })
            .collect();

        let rune_name = result.validated.rune.as_ref()
            .map(|r| r.name.as_str())
            .unwrap_or("None");
        let sigil_names: Vec<&str> = result.validated.sigils.iter()
            .map(|s| s.name.as_str())
            .collect();
        let relic_name = result.validated.relic.as_ref()
            .map(|r| r.name.as_str())
            .unwrap_or("None");

        let set1 = format!(
            "{}{}",
            result.validated.weapons.set1.main_hand.as_deref().unwrap_or("?"),
            result.validated.weapons.set1.off_hand.as_deref()
                .map(|o| format!(" / {}", o))
                .unwrap_or_default()
        );
        let set2 = format!(
            "{}{}",
            result.validated.weapons.set2.main_hand.as_deref().unwrap_or("?"),
            result.validated.weapons.set2.off_hand.as_deref()
                .map(|o| format!(" / {}", o))
                .unwrap_or_default()
        );

        let heal = result.validated.skills.heal.as_ref()
            .map(|(_, n)| n.as_str())
            .unwrap_or("?");
        let utils: Vec<&str> = result.validated.skills.utilities.iter()
            .filter_map(|u| u.as_ref().map(|(_, n)| n.as_str()))
            .collect();
        let elite = result.validated.skills.elite.as_ref()
            .map(|(_, n)| n.as_str())
            .unwrap_or("?");

        let summary = format!(
            "Profession: {}\nGear: {}\nSpecializations:\n{}\nWeapons: Set 1: {} | Set 2: {}\n\
             Skills: Heal: {} | Utilities: {} | Elite: {}\n\
             Rune: {}\nSigils: {}\nRelic: {}\n\
             Combat (Solo): Strike DPS {:.0}, Condi DPS {:.0}, Total DPS {:.0}",
            profession_name, determined_prefix,
            specs_summary.join("\n"),
            set1, set2,
            heal, utils.join(", "), elite,
            rune_name, sigil_names.join(", "), relic_name,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_budget_lookups() {
        // Verify slot budget data is accessible and returns expected ThreeStat major values.
        // Source: data/slot_budgets/level80_ascended.json (verified from GW2 API items)
        let budgets = data::slot_budgets::slot_budgets();
        assert_eq!(
            budgets.major_for_api_slot("Coat"), 141,
            "Coat ThreeStat major should be 141"
        );
        assert_eq!(
            budgets.major_for_api_slot("Helm"), 63,
            "Helm ThreeStat major should be 63"
        );
        assert_eq!(
            budgets.major_for_api_slot("Amulet"), 157,
            "Amulet ThreeStat major should be 157"
        );
        assert_eq!(
            budgets.major_for_api_slot("Leggings"), 94,
            "Leggings ThreeStat major should be 94"
        );
        // WeaponA1 maps to WeaponTwoHand
        assert_eq!(
            budgets.major_for_api_slot("WeaponA1"), 251,
            "WeaponA1 (TwoHand) ThreeStat major should be 251"
        );
        // WeaponA2 maps to WeaponOneHand
        assert_eq!(
            budgets.major_for_api_slot("WeaponA2"), 125,
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
        let selected = select_best_major_traits(&major_traits, &power_weights, &traits_cache, &no_locks, 0);
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

        let power_weights = OptimizationWeights::preset_power_dps().to_stat_weights();
        let no_locks = gw2_core::types::BuildLocks::default();
        let selected = select_best_major_traits(&major_traits, &power_weights, &traits_cache, &no_locks, 1);
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
        (profession, specs)
    }

    #[test]
    fn test_pvp_mode_dispatches_to_pvp_path() {
        let (profession, specs) = test_warrior_profession_and_specs();
        let mut pvp_amulets = HashMap::new();
        pvp_amulets.insert(4, PvpAmulet {
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
        });

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
            assert!(c.pvp_amulet.is_some(), "PvP candidate should have pvp_amulet set");
        }
    }

    #[test]
    fn test_pvp_amulet_stats_applied_to_base() {
        let (profession, specs) = test_warrior_profession_and_specs();
        let mut pvp_amulets = HashMap::new();
        pvp_amulets.insert(4, PvpAmulet {
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
        });

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

        assert!(result.is_err(), "PvP optimization should error with no amulet data");
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
        pvp_amulets.insert(1, PvpAmulet {
            id: 1,
            name: "Test Amulet".into(),
            icon: None,
            attributes: {
                let mut m = HashMap::new();
                m.insert("Power".into(), 500);
                m
            },
        });

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

        assert!(result.is_ok(), "PvP path should succeed without itemstats: {:?}", result.err());
    }

    #[test]
    fn test_pve_candidates_have_no_pvp_amulet() {
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
            assert!(c.pvp_amulet.is_none(), "PvE candidates should have pvp_amulet = None");
        }
    }
}
