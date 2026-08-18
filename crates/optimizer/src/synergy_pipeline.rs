//! Deterministic synergy pipeline: greedy layered search for optimal builds.
//!
//! Pipeline stages:
//! 1. Gear prefix (cosine similarity — reuses existing `select_gear_prefix`)
//! 2. Specs + Traits (exhaustive spec combos × pruned trait cross-product + synergy scoring)
//! 3. Rune (score all Superior runes against accumulated trait effects)
//! 4. Sigils (greedy sequential selection)
//! 5. Relic (score all relics against accumulated build effects)
//! 6. Weapons (enumerate valid combos per profession + elite spec gates)
//! 7. Skills (greedy selection: heal, elite, 3× utility)
//!
//! Returns top candidates with pre-computed synergy chain data.

use std::collections::HashMap;

use crate::balance::BalanceContext;
use crate::combat;
use crate::engine::{self, OptimizeProgress, SynergyResult};
use crate::gamedb::GameDb;
use crate::scenario::ScenarioSpec;
use crate::scoring::{score_with_weights, OptimizationWeights};
use crate::search::search_spec_combos;
use crate::stats;
use crate::synergy::{
    self, compute_marginal_synergy, extract_relic_effects, extract_rune_effects,
    extract_sigil_effects, extract_skill_effects, extract_trait_effects, score_normalized_effect,
    ComponentId, NormalizedEffect, SynergyLink,
};
use crate::text_util::{
    normalize_sigil_family, text_describes_condition_cleanse, text_describes_stability,
};
use crate::validation::{
    ValidatedBuild, ValidatedGearPrefix, ValidatedItem, ValidatedSkills, ValidatedSpec,
    ValidatedWeaponSet, ValidatedWeapons,
};
use gw2_api::models::facts::Fact;
use gw2_api::models::{Profession, Skill, Specialization, Trait as GW2Trait};
use gw2_core::types::GameMode;

/// Internal candidate from the synergy pipeline.
#[derive(Debug, Clone)]
struct SynergyCandidate {
    /// Spec IDs (2 core + optional elite).
    spec_ids: Vec<u32>,
    #[allow(dead_code)]
    elite_spec: Option<u32>,
    /// Selected major trait IDs per spec (3 per spec).
    selected_major_traits: Vec<u32>,
    /// All equipped trait IDs (minor + major).
    all_trait_ids: Vec<u32>,
    /// Accumulated effects from all components selected so far.
    accumulated: Vec<(ComponentId, Vec<NormalizedEffect>)>,
    /// Combined score (combat + synergy).
    score: f64,
    /// Rune selection.
    rune: Option<(u32, String)>,
    /// Sigil selections.
    sigils: Vec<(u32, String)>,
    /// Relic selection.
    relic: Option<(u32, String)>,
    /// Weapon sets: (set1_main, set1_off, set2_main, set2_off).
    weapons: (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    /// Skill selections: (heal, utilities[3], elite).
    heal: Option<(u32, String)>,
    utilities: Vec<(u32, String)>,
    elite_skill: Option<(u32, String)>,
    /// Revenant legends, active first.
    legends: Vec<String>,
    aquatic_legends: Vec<String>,
    /// Synergy links discovered during selection.
    synergy_links: Vec<SynergyLink>,
}

// ─── Main Entry Point ───

/// Run the full deterministic synergy pipeline.
/// Returns a SynergyResult with a fully determined build.
#[allow(clippy::too_many_arguments)]
pub fn optimize_synergy(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    gear_prefix_name: &str,
    locks: &gw2_core::types::BuildLocks,
    scenario: Option<&ScenarioSpec>,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<SynergyResult, String> {
    let profession = db
        .profession(profession_name)
        .ok_or_else(|| format!("Profession '{}' not found", profession_name))?;

    // Stage 2: Specs + Traits
    on_progress(OptimizeProgress {
        stage: "Evaluating specializations and traits...".into(),
        done: false,
    });
    let mut candidates =
        select_specs_and_traits(profession, weights, &db.specializations, &db.traits, locks);

    if candidates.is_empty() {
        return Err(format!(
            "No valid spec/trait combinations found for {}",
            profession_name
        ));
    }

    // Stage 3: Rune
    on_progress(OptimizeProgress {
        stage: "Selecting optimal rune...".into(),
        done: false,
    });
    select_rune(&mut candidates, db, weights);

    // Stage 4: Sigils
    on_progress(OptimizeProgress {
        stage: "Selecting optimal sigils...".into(),
        done: false,
    });
    select_sigils(&mut candidates, db, weights, ctx);

    // Stage 5: Relic
    on_progress(OptimizeProgress {
        stage: "Selecting optimal relic...".into(),
        done: false,
    });
    select_relic(&mut candidates, db, weights);

    // Stage 6: Weapons
    on_progress(OptimizeProgress {
        stage: "Selecting optimal weapons...".into(),
        done: false,
    });
    select_weapons(&mut candidates, profession, db, weights);

    // Stage 7: Skills
    on_progress(OptimizeProgress {
        stage: "Selecting optimal skills...".into(),
        done: false,
    });
    select_skills(&mut candidates, db, profession_name, weights, ctx);

    // Stage 8: Final ranking with full combat performance
    on_progress(OptimizeProgress {
        stage: "Computing final combat metrics...".into(),
        done: false,
    });

    let best = rank_and_select(
        &candidates,
        db,
        profession_name,
        gear_prefix_name,
        weights,
        ctx,
        scenario,
    )?;

    // Convert to SynergyResult
    on_progress(OptimizeProgress {
        stage: "Building validated result...".into(),
        done: false,
    });

    build_synergy_result(
        best,
        db,
        profession_name,
        gear_prefix_name,
        weights,
        ctx,
        scenario,
        on_progress,
    )
}

// ─── Stage 2: Specs + Traits ───

fn select_specs_and_traits(
    profession: &Profession,
    weights: &OptimizationWeights,
    specs_cache: &HashMap<u32, Specialization>,
    traits_cache: &HashMap<u32, GW2Trait>,
    locks: &gw2_core::types::BuildLocks,
) -> Vec<SynergyCandidate> {
    let spec_combos = search_spec_combos(&profession.specializations, specs_cache, locks);

    let mut candidates = Vec::new();

    for (elite, cores) in &spec_combos {
        let spec_ids: Vec<u32> = cores.iter().copied().chain(elite.iter().copied()).collect();

        // For each spec combo, find the best trait configuration
        // using a two-pass approach: independent per-spec, then cross-spec synergy

        // Pass 1: Find top-3 trait configs per spec independently
        let mut per_spec_configs: Vec<Vec<(Vec<u32>, f64)>> = Vec::new();

        for &spec_id in &spec_ids {
            let Some(spec) = specs_cache.get(&spec_id) else {
                per_spec_configs.push(vec![(Vec::new(), 0.0)]);
                continue;
            };

            if spec.major_traits.len() != 9 {
                // Non-standard spec: take all minor traits, no major trait selection
                per_spec_configs.push(vec![(Vec::new(), 0.0)]);
                continue;
            }

            // Check trait locks for this spec
            let trait_lock = locks.trait_locks.get(&spec_id);

            // Build ranges for each column: locked = single option, unlocked = 0..3.
            // If a locked trait ID is not found in that column (data mismatch), fall back
            // to all 3 options rather than producing an empty range (which collapses the
            // cross-product to zero candidates with no error message).
            let adept_range: Vec<usize> = if let Some(locked_id) = trait_lock.and_then(|t| t[0]) {
                let r: Vec<usize> = (0..3)
                    .filter(|&i| spec.major_traits[i] == locked_id)
                    .collect();
                if r.is_empty() {
                    vec![0, 1, 2]
                } else {
                    r
                }
            } else {
                vec![0, 1, 2]
            };
            let master_range: Vec<usize> = if let Some(locked_id) = trait_lock.and_then(|t| t[1]) {
                let r: Vec<usize> = (0..3)
                    .filter(|&i| spec.major_traits[3 + i] == locked_id)
                    .collect();
                if r.is_empty() {
                    vec![0, 1, 2]
                } else {
                    r
                }
            } else {
                vec![0, 1, 2]
            };
            let grandmaster_range: Vec<usize> =
                if let Some(locked_id) = trait_lock.and_then(|t| t[2]) {
                    let r: Vec<usize> = (0..3)
                        .filter(|&i| spec.major_traits[6 + i] == locked_id)
                        .collect();
                    if r.is_empty() {
                        vec![0, 1, 2]
                    } else {
                        r
                    }
                } else {
                    vec![0, 1, 2]
                };

            // Enumerate trait combos (respecting locks — locked columns have 1 option)
            let mut configs: Vec<(Vec<u32>, f64)> = Vec::new();
            for &a in &adept_range {
                for &m in &master_range {
                    for &g in &grandmaster_range {
                        let traits = vec![
                            spec.major_traits[a],     // Adept
                            spec.major_traits[3 + m], // Master
                            spec.major_traits[6 + g], // Grandmaster
                        ];

                        // Score this config
                        let all_ids: Vec<u32> = spec
                            .minor_traits
                            .iter()
                            .copied()
                            .chain(traits.iter().copied())
                            .collect();

                        let mut score = 0.0;
                        let mut effects = Vec::new();
                        for &tid in &all_ids {
                            if let Some(t) = traits_cache.get(&tid) {
                                let effs = extract_trait_effects(t, &all_ids);
                                for eff in &effs {
                                    score += score_normalized_effect(eff, weights);
                                }
                                effects.push((ComponentId::Trait(tid), effs));
                            }
                        }

                        // Intra-spec synergy
                        for i in 0..effects.len() {
                            let (syn, _) = compute_marginal_synergy(
                                &effects[i].1,
                                &effects[..i],
                                weights,
                                Some(&effects[i].0),
                            );
                            score += syn;
                        }

                        configs.push((traits, score));
                    }
                }
            }

            // Keep top 3
            configs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            configs.truncate(3);
            per_spec_configs.push(configs);
        }

        // Pass 2: Cross-product top configs for cross-spec synergy
        let combo_count: usize = per_spec_configs.iter().map(|c| c.len().max(1)).product();
        let combo_limit = combo_count.min(27); // Cap at 27 to keep perf reasonable

        // Generate cross-products
        let cross_products = cross_product_trait_configs(&per_spec_configs);

        for trait_combo in cross_products.into_iter().take(combo_limit) {
            // Collect all trait IDs
            let mut all_major: Vec<u32> = Vec::new();
            let mut all_trait_ids: Vec<u32> = Vec::new();

            for (spec_idx, &spec_id) in spec_ids.iter().enumerate() {
                if let Some(spec) = specs_cache.get(&spec_id) {
                    all_trait_ids.extend(&spec.minor_traits);
                }
                all_major.extend(&trait_combo[spec_idx]);
                all_trait_ids.extend(&trait_combo[spec_idx]);
            }

            // Score with full cross-spec synergy
            let mut total_score = 0.0;
            let mut accumulated = Vec::new();
            let mut all_links: Vec<SynergyLink> = Vec::new();

            for &tid in &all_trait_ids {
                if let Some(t) = traits_cache.get(&tid) {
                    let effs = extract_trait_effects(t, &all_trait_ids);
                    for eff in &effs {
                        total_score += score_normalized_effect(eff, weights);
                    }
                    let new_id = ComponentId::Trait(tid);
                    let (syn, links) =
                        compute_marginal_synergy(&effs, &accumulated, weights, Some(&new_id));
                    total_score += syn;
                    all_links.extend(links);
                    accumulated.push((new_id, effs));
                }
            }

            // Keep top 10 most impactful synergy links to avoid explanation clutter
            all_links.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            all_links.truncate(10);

            candidates.push(SynergyCandidate {
                spec_ids: spec_ids.clone(),
                elite_spec: *elite,
                selected_major_traits: all_major,
                all_trait_ids,
                accumulated,
                score: total_score,
                rune: None,
                sigils: Vec::new(),
                relic: None,
                weapons: (None, None, None, None),
                heal: None,
                utilities: Vec::new(),
                elite_skill: None,
                legends: Vec::new(),
                aquatic_legends: Vec::new(),
                synergy_links: all_links,
            });
        }
    }

    // Sort by score and keep top 5
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(5);

    candidates
}

/// Generate cross-product of per-spec trait configurations.
fn cross_product_trait_configs(per_spec: &[Vec<(Vec<u32>, f64)>]) -> Vec<Vec<Vec<u32>>> {
    if per_spec.is_empty() {
        return vec![Vec::new()];
    }

    let first = &per_spec[0];
    let rest = cross_product_trait_configs(&per_spec[1..]);

    let mut result = Vec::new();
    for (traits, _score) in first {
        for rest_combo in &rest {
            let mut combo = vec![traits.clone()];
            combo.extend(rest_combo.iter().cloned());
            result.push(combo);
        }
    }
    result
}

// ─── Stage 3: Rune ───

fn select_rune(candidates: &mut [SynergyCandidate], db: &GameDb, weights: &OptimizationWeights) {
    let runes = db.all_runes();
    // Filter to Superior runes only
    let superior_runes: Vec<_> = runes
        .iter()
        .filter(|r| r.name.contains("Superior"))
        .collect();

    for candidate in candidates.iter_mut() {
        let mut best_score = f64::NEG_INFINITY;
        let mut best_rune: Option<(u32, String)> = None;
        let mut best_effects: Vec<NormalizedEffect> = Vec::new();
        let mut best_links: Vec<SynergyLink> = Vec::new();

        for &&rune in &superior_runes {
            let effects = extract_rune_effects(rune);

            // Base score
            let base: f64 = effects
                .iter()
                .map(|e| score_normalized_effect(e, weights))
                .sum();

            // Synergy with existing traits/specs
            let new_id = ComponentId::Rune(rune.id);
            let (syn, links) =
                compute_marginal_synergy(&effects, &candidate.accumulated, weights, Some(&new_id));

            let total = base + syn;
            if total > best_score {
                best_score = total;
                best_rune = Some((rune.id, rune.name.clone()));
                best_effects = effects;
                best_links = links;
            }
        }

        if let Some(rune) = best_rune {
            candidate.synergy_links.extend(best_links);
            candidate
                .accumulated
                .push((ComponentId::Rune(rune.0), best_effects));
            candidate.rune = Some(rune);
            candidate.score += best_score;
        }
    }
}

// ─── Stage 4: Sigils ───

fn select_sigils(
    candidates: &mut [SynergyCandidate],
    db: &GameDb,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
) {
    let sigils = db.all_sigils();
    let superior_sigils: Vec<_> = sigils
        .iter()
        .filter(|s| s.name.contains("Superior"))
        .collect();

    for candidate in candidates.iter_mut() {
        // GW2 sigil rule: duplicates are forbidden *within* a weapon set (Set 1 or Set 2),
        // but the same sigil family (e.g. "Sigil of Force") IS allowed in both sets
        // independently. We select 2 sigils per set with separate dedup sets.
        for set_idx in 0..2usize {
            let _ = set_idx; // slot identity only used for ordering; sets are symmetric
            let mut set_ids: Vec<u32> = Vec::new();
            let mut set_families: Vec<String> = Vec::new();

            for _slot in 0..2 {
                let mut best_score = f64::NEG_INFINITY;
                let mut best_sigil: Option<(u32, String)> = None;
                let mut best_effects: Vec<NormalizedEffect> = Vec::new();
                let mut best_links: Vec<SynergyLink> = Vec::new();

                for &&sigil in &superior_sigils {
                    // Prevent duplicates within a weapon set. Some sigils exist as
                    // multiple items with the same display name (e.g. PvE/PvP versions),
                    // so we dedupe by a normalized "family" key rather than raw ID.
                    if set_ids.contains(&sigil.id) {
                        continue;
                    }
                    let family = normalize_sigil_family(&sigil.name);
                    if set_families.iter().any(|f| f == &family) {
                        continue;
                    }

                    let effects = extract_sigil_effects(sigil, ctx);
                    let base: f64 = effects
                        .iter()
                        .map(|e| score_normalized_effect(e, weights))
                        .sum();
                    let new_id = ComponentId::Sigil(sigil.id);
                    let (syn, links) = compute_marginal_synergy(
                        &effects,
                        &candidate.accumulated,
                        weights,
                        Some(&new_id),
                    );

                    let total = base + syn;
                    if total > best_score {
                        best_score = total;
                        best_sigil = Some((sigil.id, sigil.name.clone()));
                        best_effects = effects;
                        best_links = links;
                    }
                }

                if let Some(sigil) = best_sigil {
                    set_ids.push(sigil.0);
                    set_families.push(normalize_sigil_family(&sigil.1));
                    candidate.synergy_links.extend(best_links);
                    candidate
                        .accumulated
                        .push((ComponentId::Sigil(sigil.0), best_effects));
                    candidate.sigils.push(sigil);
                    candidate.score += best_score;
                }
            }
        }
    }
}

// ─── Stage 5: Relic ───

fn select_relic(candidates: &mut [SynergyCandidate], db: &GameDb, weights: &OptimizationWeights) {
    let relics = db.all_relics();

    for candidate in candidates.iter_mut() {
        let mut best_score = f64::NEG_INFINITY;
        let mut best_relic: Option<(u32, String)> = None;
        let mut best_effects: Vec<NormalizedEffect> = Vec::new();
        let mut best_links: Vec<SynergyLink> = Vec::new();

        for &relic in &relics {
            let effects = extract_relic_effects(relic);

            let base: f64 = effects
                .iter()
                .map(|e| score_normalized_effect(e, weights))
                .sum();
            let new_id = ComponentId::Relic(relic.id);
            let (syn, links) =
                compute_marginal_synergy(&effects, &candidate.accumulated, weights, Some(&new_id));

            let total = base + syn;
            if total > best_score {
                best_score = total;
                best_relic = Some((relic.id, relic.name.clone()));
                best_effects = effects;
                best_links = links;
            }
        }

        if let Some(relic) = best_relic {
            candidate.synergy_links.extend(best_links);
            candidate
                .accumulated
                .push((ComponentId::Relic(relic.0), best_effects));
            candidate.relic = Some(relic);
            candidate.score += best_score;
        }
    }
}

// ─── Stage 6: Weapons ───

fn select_weapons(
    candidates: &mut [SynergyCandidate],
    profession: &Profession,
    db: &GameDb,
    weights: &OptimizationWeights,
) {
    // Build list of valid weapon combos for this profession
    for candidate in candidates.iter_mut() {
        let elite_spec_ids: Vec<u32> = candidate
            .spec_ids
            .iter()
            .filter(|&&id| {
                db.specializations
                    .get(&id)
                    .map(|s| s.elite)
                    .unwrap_or(false)
            })
            .copied()
            .collect();

        // Filter weapons by elite spec gate
        let available: Vec<(&str, bool)> = profession
            .weapons
            .iter()
            .filter(|(_, info)| {
                if info.is_aquatic() {
                    return false;
                }
                if let Some(req_spec) = info.specialization {
                    elite_spec_ids.contains(&req_spec)
                } else {
                    true
                }
            })
            .map(|(name, info)| {
                let is_two_hand = info.flags.iter().any(|f| f == "TwoHand");
                (name.as_str(), is_two_hand)
            })
            .collect();

        // Cache per-weapon synergy scores once. Each weapon was previously scored
        // ~12 times per candidate (once per mh+oh combination per set, once main-only
        // per set, two sets). Caching collapses that to O(weapons) calls.
        let weapon_scores: std::collections::HashMap<&str, f64> = available
            .iter()
            .map(|(name, _)| {
                (
                    *name,
                    score_weapon_skills(name, profession, db, weights, &candidate.accumulated),
                )
            })
            .collect();

        // Generate weapon combos (simplified: pick best set 1, then best set 2)
        let mut best_set1 = (None, None);
        let mut best_set1_score = f64::NEG_INFINITY;

        for &(weapon, is_2h) in &available {
            let info = &profession.weapons[weapon];
            let is_main = info.flags.iter().any(|f| f == "Mainhand" || f == "TwoHand");
            if is_2h {
                // Two-handed weapon as set 1
                let score = weapon_scores[weapon];
                if score > best_set1_score {
                    best_set1_score = score;
                    best_set1 = (Some(weapon.to_string()), None);
                }
            } else if is_main {
                // Main-hand + each valid off-hand
                for &(off_weapon, off_2h) in &available {
                    if off_2h {
                        continue;
                    }
                    let off_info = &profession.weapons[off_weapon];
                    let off_is_off = off_info.flags.iter().any(|f| f == "Offhand");
                    if !off_is_off {
                        continue;
                    }

                    let score = weapon_scores[weapon] + weapon_scores[off_weapon];
                    if score > best_set1_score {
                        best_set1_score = score;
                        best_set1 = (Some(weapon.to_string()), Some(off_weapon.to_string()));
                    }
                }

                // Main-hand only (no off-hand)
                let score = weapon_scores[weapon];
                if score > best_set1_score {
                    best_set1_score = score;
                    best_set1 = (Some(weapon.to_string()), None);
                }
            }
        }

        // Set 2: pick a different weapon combo
        let mut best_set2 = (None, None);
        let mut best_set2_score = f64::NEG_INFINITY;

        for &(weapon, is_2h) in &available {
            // Skip if same as set 1 main
            if best_set1.0.as_deref() == Some(weapon) {
                continue; // Don't reuse set 1's primary weapon in set 2
            }

            let info = &profession.weapons[weapon];
            let is_main = info.flags.iter().any(|f| f == "Mainhand" || f == "TwoHand");

            if is_2h {
                let score = weapon_scores[weapon];
                if score > best_set2_score {
                    best_set2_score = score;
                    best_set2 = (Some(weapon.to_string()), None);
                }
            } else if is_main {
                for &(off_weapon, off_2h) in &available {
                    if off_2h {
                        continue;
                    }
                    let off_info = &profession.weapons[off_weapon];
                    if !off_info.flags.iter().any(|f| f == "Offhand") {
                        continue;
                    }

                    let score = weapon_scores[weapon] + weapon_scores[off_weapon];
                    if score > best_set2_score {
                        best_set2_score = score;
                        best_set2 = (Some(weapon.to_string()), Some(off_weapon.to_string()));
                    }
                }

                // Main-hand only (no off-hand) — mirrors Set 1.
                // Without this, Set 2 can never be main-hand-only even when that
                // scores higher than any main+offhand combination.
                let score = weapon_scores[weapon];
                if score > best_set2_score {
                    best_set2_score = score;
                    best_set2 = (Some(weapon.to_string()), None);
                }
            }
        }

        candidate.weapons = (best_set1.0, best_set1.1, best_set2.0, best_set2.1);
        if best_set1_score.is_finite() {
            candidate.score += best_set1_score;
        }
        if best_set2_score.is_finite() {
            candidate.score += best_set2_score;
        }
    }
}

fn score_weapon_skills(
    weapon_type: &str,
    profession: &Profession,
    db: &GameDb,
    weights: &OptimizationWeights,
    accumulated: &[(ComponentId, Vec<NormalizedEffect>)],
) -> f64 {
    let Some(weapon_info) = profession.weapons.get(weapon_type) else {
        return 0.0;
    };

    let mut score = 0.0;
    for skill_ref in &weapon_info.skills {
        if let Some(skill) = db.skills.get(&skill_ref.id) {
            let effects = extract_skill_effects(skill);
            for eff in &effects {
                score += score_normalized_effect(eff, weights);
            }
            let new_id = ComponentId::Skill(skill.id);
            let (syn, _) = compute_marginal_synergy(&effects, accumulated, weights, Some(&new_id));
            score += syn;
        }
    }
    score
}

// ─── Stage 7: Skills ───

fn select_skills(
    candidates: &mut [SynergyCandidate],
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
) {
    if profession_name == "Revenant" && !db.legends.is_empty() {
        select_revenant_legends(candidates, db, weights);
        return;
    }

    let prof_skills = db.profession_skills(profession_name);

    for candidate in candidates.iter_mut() {
        // Filter skills by elite spec gate: only allow skills whose specialization
        // is None (core skill) or matches one of the candidate's equipped specs.
        let gated_skills: Vec<&&Skill> = prof_skills
            .iter()
            .filter(|s| match s.specialization {
                Some(spec_id) => candidate.spec_ids.contains(&spec_id),
                None => true,
            })
            .filter(|s| db.skill_to_palette.is_empty() || db.skill_palette_id(s.id) != 0)
            .collect();

        // Heal skill
        let heals: Vec<&&Skill> = gated_skills
            .iter()
            .filter(|s| s.slot.as_deref() == Some("Heal"))
            .copied()
            .collect();
        if let Some((id, name, links)) = pick_best_skill(&heals, weights, &candidate.accumulated) {
            add_selected_skill_effects(candidate, db, id, links);
            candidate.heal = Some((id, name));
        }

        // Elite skill
        let elites: Vec<&&Skill> = gated_skills
            .iter()
            .filter(|s| s.slot.as_deref() == Some("Elite"))
            .copied()
            .collect();
        if let Some((id, name, links)) = pick_best_skill(&elites, weights, &candidate.accumulated) {
            add_selected_skill_effects(candidate, db, id, links);
            candidate.elite_skill = Some((id, name));
        }

        // Utility skills (3, greedy sequential)
        let utilities: Vec<&&Skill> = gated_skills
            .iter()
            .filter(|s| s.slot.as_deref() == Some("Utility"))
            .copied()
            .collect();

        let mut used_ids: Vec<u32> = Vec::new();
        if matches!(ctx.game_mode, GameMode::PvP | GameMode::WvW) {
            select_required_competitive_utilities(
                &utilities,
                candidate,
                db,
                weights,
                &mut used_ids,
            );
        }

        for _ in 0..3 {
            if used_ids.len() >= 3 {
                break;
            }

            let available: Vec<&&Skill> = utilities
                .iter()
                .filter(|s| !used_ids.contains(&s.id))
                .copied()
                .collect();

            if let Some((id, name, links)) =
                pick_best_skill(&available, weights, &candidate.accumulated)
            {
                used_ids.push(id);
                add_selected_skill_effects(candidate, db, id, links);
                candidate.utilities.push((id, name));
            }
        }
    }
}

/// Revenant heal/utilities/elite come from legendary stances, not a free mix.
fn select_revenant_legends(
    candidates: &mut [SynergyCandidate],
    db: &GameDb,
    weights: &OptimizationWeights,
) {
    if db.legends.is_empty() {
        return;
    }
    for candidate in candidates.iter_mut() {
        let mut ranked: Vec<(f64, String)> = db
            .legends
            .keys()
            .filter(|id| db.legend_available(id, &candidate.spec_ids))
            .map(|id| {
                (
                    score_legend(db, id, weights, &candidate.accumulated),
                    id.clone(),
                )
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
        ranked.truncate(2);
        let ids: Vec<String> = ranked.into_iter().map(|(_, id)| id).collect();
        if ids.is_empty() {
            continue;
        }
        apply_legend_skills(candidate, db, &ids[0]);
        candidate.legends = ids.clone();
        candidate.aquatic_legends = ids;
    }
}

fn score_legend(
    db: &GameDb,
    legend_id: &str,
    weights: &OptimizationWeights,
    accumulated: &[(ComponentId, Vec<NormalizedEffect>)],
) -> f64 {
    let Some(legend) = db.legends.get(legend_id) else {
        return 0.0;
    };
    let mut ids = vec![legend.heal, legend.elite];
    ids.extend(legend.utilities.iter().copied());
    let mut score = 0.0;
    for id in ids {
        let Some(skill) = db.skills.get(&id) else {
            continue;
        };
        let effects = extract_skill_effects(skill);
        for eff in &effects {
            score += score_normalized_effect(eff, weights);
        }
        let (syn, _) = compute_marginal_synergy(
            &effects,
            accumulated,
            weights,
            Some(&ComponentId::Skill(id)),
        );
        score += syn;
    }
    score
}

fn apply_legend_skills(candidate: &mut SynergyCandidate, db: &GameDb, legend_id: &str) {
    let Some(legend) = db.legends.get(legend_id) else {
        return;
    };
    let name_of = |id: u32| {
        db.skills
            .get(&id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("Skill {id}"))
    };
    candidate.heal = Some((legend.heal, name_of(legend.heal)));
    if db.skills.contains_key(&legend.heal) {
        add_selected_skill_effects(candidate, db, legend.heal, Vec::new());
    }
    candidate.utilities = legend
        .utilities
        .iter()
        .take(3)
        .map(|&id| (id, name_of(id)))
        .collect();
    let util_ids: Vec<u32> = candidate.utilities.iter().map(|(id, _)| *id).collect();
    for id in util_ids {
        if db.skills.contains_key(&id) {
            add_selected_skill_effects(candidate, db, id, Vec::new());
        }
    }
    candidate.elite_skill = Some((legend.elite, name_of(legend.elite)));
    if db.skills.contains_key(&legend.elite) {
        add_selected_skill_effects(candidate, db, legend.elite, Vec::new());
    }
}

fn add_selected_skill_effects(
    candidate: &mut SynergyCandidate,
    db: &GameDb,
    id: u32,
    links: Vec<SynergyLink>,
) {
    if let Some(skill) = db.skills.get(&id) {
        let effects = extract_skill_effects(skill);
        candidate
            .accumulated
            .push((ComponentId::Skill(id), effects));
    }
    candidate.synergy_links.extend(links);
}

#[derive(Debug, Clone, Copy)]
enum CompetitiveUtilityGate {
    Stunbreak,
    Stability,
    Cleanse,
}

#[derive(Default)]
struct CompetitiveUtilityCoverage {
    stunbreak: bool,
    stability: bool,
    cleanse: bool,
}

impl CompetitiveUtilityCoverage {
    fn add_skill(&mut self, skill: &Skill) {
        self.stunbreak |= skill_is_stunbreak(skill);
        self.stability |= skill_has_stability(skill);
        self.cleanse |= skill_cleanse_count(skill) > 0;
    }

    fn satisfies(&self, gate: CompetitiveUtilityGate) -> bool {
        match gate {
            CompetitiveUtilityGate::Stunbreak => self.stunbreak,
            CompetitiveUtilityGate::Stability => self.stability,
            CompetitiveUtilityGate::Cleanse => self.cleanse,
        }
    }
}

impl CompetitiveUtilityGate {
    fn matches(self, skill: &Skill) -> bool {
        match self {
            CompetitiveUtilityGate::Stunbreak => skill_is_stunbreak(skill),
            CompetitiveUtilityGate::Stability => skill_has_stability(skill),
            CompetitiveUtilityGate::Cleanse => skill_cleanse_count(skill) > 0,
        }
    }
}

fn select_required_competitive_utilities(
    utilities: &[&&Skill],
    candidate: &mut SynergyCandidate,
    db: &GameDb,
    weights: &OptimizationWeights,
    used_ids: &mut Vec<u32>,
) {
    let mut coverage = CompetitiveUtilityCoverage::default();
    for selected in [&candidate.heal, &candidate.elite_skill]
        .into_iter()
        .flatten()
    {
        if let Some(skill) = db.skills.get(&selected.0) {
            coverage.add_skill(skill);
        }
    }

    for gate in [
        CompetitiveUtilityGate::Stunbreak,
        CompetitiveUtilityGate::Stability,
        CompetitiveUtilityGate::Cleanse,
    ] {
        if used_ids.len() >= 3 || coverage.satisfies(gate) {
            continue;
        }

        let mut available: Vec<&&Skill> = Vec::new();
        for skill_ref in utilities {
            let skill = **skill_ref;
            if !used_ids.contains(&skill.id) && gate.matches(skill) {
                available.push(*skill_ref);
            }
        }

        if let Some((id, name, links)) =
            pick_best_skill(&available, weights, &candidate.accumulated)
        {
            used_ids.push(id);
            add_selected_skill_effects(candidate, db, id, links);
            candidate.utilities.push((id, name));
            if let Some(skill) = db.skills.get(&id) {
                coverage.add_skill(skill);
            }
        }
    }
}

pub(crate) fn skill_is_stunbreak(skill: &Skill) -> bool {
    skill.facts.iter().any(|fact| {
        matches!(
            fact,
            Fact::StunBreak {
                value: Some(true),
                ..
            }
        )
    })
}

pub(crate) fn skill_has_stability(skill: &Skill) -> bool {
    let from_facts = skill.facts.iter().any(|fact| match fact {
        Fact::Buff {
            status: Some(status),
            ..
        }
        | Fact::PrefixedBuff {
            status: Some(status),
            ..
        } => status.eq_ignore_ascii_case("Stability"),
        _ => false,
    });
    from_facts
        || skill
            .description
            .as_deref()
            .is_some_and(text_describes_stability)
}

pub(crate) fn skill_cleanse_count(skill: &Skill) -> u32 {
    let fact_count: u32 = skill
        .facts
        .iter()
        .filter_map(condition_cleanse_count_from_fact)
        .sum();
    if fact_count > 0 {
        return fact_count;
    }

    if skill
        .description
        .as_deref()
        .is_some_and(text_describes_condition_cleanse)
    {
        1
    } else {
        0
    }
}

fn condition_cleanse_count_from_fact(fact: &Fact) -> Option<u32> {
    match fact {
        Fact::Number {
            text: Some(text),
            value,
            ..
        } if text_describes_condition_cleanse(text) => Some((*value).unwrap_or(1).max(1) as u32),
        _ => None,
    }
}

fn pick_best_skill(
    skills: &[&&Skill],
    weights: &OptimizationWeights,
    accumulated: &[(ComponentId, Vec<NormalizedEffect>)],
) -> Option<(u32, String, Vec<SynergyLink>)> {
    let mut best_score = f64::NEG_INFINITY;
    let mut best: Option<(u32, String, Vec<SynergyLink>)> = None;

    for &&skill in skills {
        let effects = extract_skill_effects(skill);
        let base: f64 = effects
            .iter()
            .map(|e| score_normalized_effect(e, weights))
            .sum();
        let new_id = ComponentId::Skill(skill.id);
        let (syn, links) = compute_marginal_synergy(&effects, accumulated, weights, Some(&new_id));

        let total = base + syn;
        if total > best_score {
            best_score = total;
            best = Some((skill.id, skill.name.clone(), links));
        }
    }

    best
}

// ─── Final Ranking ───

fn rank_and_select(
    candidates: &[SynergyCandidate],
    db: &GameDb,
    profession_name: &str,
    gear_prefix_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: Option<&ScenarioSpec>,
) -> Result<SynergyCandidate, String> {
    if candidates.is_empty() {
        return Err(format!(
            "No valid spec/trait combinations found for {}. \
             If trait locks are set, verify locked traits belong to the selected specializations.",
            profession_name
        ));
    }

    // Re-score candidates with full combat performance
    let mut scored: Vec<(usize, f64)> = Vec::new();

    // Resolve the gear prefix once with the shared deterministic policy.
    let gear_prefix_id = db.itemstat_by_name(gear_prefix_name).map(|is| is.id);

    let profiles = combat::buff_profiles_for_profession(profession_name, ctx);
    let idx = scenario
        .map(|s| crate::rotation::combat_model::buff_profile_index(s.combat_tier))
        .unwrap_or(0)
        .min(profiles.len().saturating_sub(1));
    let scale_profile = &profiles[idx];
    // Hoist condition weights outside the candidate loop — they only depend on
    // (profession, mode) and don't change per candidate. Previously this rebuilt
    // a ConditionWeights via rotation-profile HashMap lookup on every iteration.
    let cond_weights = combat::condition_weights_for_profession(profession_name, ctx);

    // Compute max synergy once (loop-invariant) — candidates don't change during ranking
    let max_synergy = candidates
        .iter()
        .map(|c| c.score)
        .fold(0.0_f64, f64::max)
        .max(1.0);

    for (idx, candidate) in candidates.iter().enumerate() {
        let stats = compute_candidate_stats(candidate, db, gear_prefix_id, ctx);
        let derived = stats::compute_derived(&stats, profession_name);

        let modifiers = combat::extract_damage_modifiers(
            &candidate.all_trait_ids,
            candidate.rune.as_ref().map(|(id, _)| *id),
            &candidate
                .sigils
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            candidate.relic.as_ref().map(|(id, _)| *id),
            &db.traits,
            &db.items,
            ctx,
        );

        let combat_perf = combat::calculate_combat_performance(
            &stats,
            &derived,
            &modifiers,
            scale_profile,
            &cond_weights,
            profession_name,
            ctx,
        );

        let combat_score = score_with_weights(&combat_perf, weights);
        // Blend: 40% combat (gear + parsed modifiers), 60% synergy score.
        let synergy_normalized = candidate.score / max_synergy;
        let final_score = combat_score * 0.4 + synergy_normalized * 0.6;

        scored.push((idx, final_score));
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(candidates[scored[0].0].clone())
}

fn compute_candidate_stats(
    candidate: &SynergyCandidate,
    db: &GameDb,
    gear_prefix_id: Option<u32>,
    ctx: &BalanceContext,
) -> stats::StatBlock {
    let mut full_stats = stats::base_stats();

    crate::engine::apply_optimized_gear_stats(&mut full_stats, db, gear_prefix_id, ctx);

    // Include trait flat stat bonuses (AttributeAdjust facts) so that estimated stats
    // use the same calculation as calculate_full_stats() on the current build.
    // Both sides of the comparison now include trait stat bonuses, making them
    // apples-to-apples. The same function is used for current build stats display.
    let trait_stats = stats::calculate_trait_stats(&candidate.all_trait_ids, &db.traits);
    full_stats.power += trait_stats.power;
    full_stats.precision += trait_stats.precision;
    full_stats.toughness += trait_stats.toughness;
    full_stats.vitality += trait_stats.vitality;
    full_stats.condition_damage += trait_stats.condition_damage;
    full_stats.expertise += trait_stats.expertise;
    full_stats.concentration += trait_stats.concentration;
    full_stats.ferocity += trait_stats.ferocity;
    full_stats.healing_power += trait_stats.healing_power;

    // Trait stat conversions (BuffConversion facts — permanent passives like
    // "7% of Toughness becomes Power"). Applied after flat bonuses.
    stats::apply_trait_conversions(&mut full_stats, &candidate.all_trait_ids, &db.traits);

    full_stats
}

// ─── Build SynergyResult ───

fn build_synergy_result(
    candidate: SynergyCandidate,
    db: &GameDb,
    profession_name: &str,
    gear_prefix_name: &str,
    _weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: Option<&ScenarioSpec>,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<SynergyResult, String> {
    // Build ValidatedBuild
    let mut validated = ValidatedBuild::default();

    // Gear prefix. `gear_prefix_name` is the cosine-selected canonical name
    // (e.g. "Berserker's"). Use the shared deterministic lookup so the same
    // input always resolves to the same itemstat across runs.
    if let Some(is) = db.itemstat_by_name(gear_prefix_name) {
        validated.gear_prefix = Some(ValidatedGearPrefix {
            itemstat_id: is.id,
            name: is.name.clone(),
        });
    }

    // Specializations
    for &spec_id in &candidate.spec_ids {
        if let Some(spec) = db.specializations.get(&spec_id) {
            let mut major_trait_ids = Vec::new();
            let mut major_trait_names = Vec::new();
            let mut all_ids = spec.minor_traits.clone();

            for &tid in &candidate.selected_major_traits {
                if let Some(t) = db.traits.get(&tid) {
                    if t.specialization == spec_id {
                        major_trait_ids.push(tid);
                        major_trait_names.push(t.name.clone());
                        all_ids.push(tid);
                    }
                }
            }

            validated.specializations.push(ValidatedSpec {
                spec_id,
                name: spec.name.clone(),
                elite: spec.elite,
                trait_ids: major_trait_ids,
                trait_names: major_trait_names,
                all_trait_ids: all_ids,
            });
        }
    }

    // Weapons
    validated.weapons = ValidatedWeapons {
        set1: ValidatedWeaponSet {
            main_hand: candidate.weapons.0.clone(),
            off_hand: candidate.weapons.1.clone(),
        },
        set2: ValidatedWeaponSet {
            main_hand: candidate.weapons.2.clone(),
            off_hand: candidate.weapons.3.clone(),
        },
    };

    // Skills
    validated.skills = ValidatedSkills {
        heal: candidate.heal.clone(),
        utilities: candidate
            .utilities
            .iter()
            .map(|u| Some(u.clone()))
            .collect(),
        elite: candidate.elite_skill.clone(),
    };
    validated.legends = candidate.legends.clone();
    validated.aquatic_legends = candidate.aquatic_legends.clone();

    // Rune
    if let Some((id, name)) = &candidate.rune {
        validated.rune = Some(ValidatedItem {
            id: *id,
            name: name.clone(),
        });
    }

    // Sigils
    for (id, name) in &candidate.sigils {
        validated.sigils.push(ValidatedItem {
            id: *id,
            name: name.clone(),
        });
    }

    // Relic
    if let Some((id, name)) = &candidate.relic {
        validated.relic = Some(ValidatedItem {
            id: *id,
            name: name.clone(),
        });
    }

    // Synergy explanation
    validated.synergy_explanation =
        synergy::template_explanation(&candidate.synergy_links, gear_prefix_name, profession_name);

    // Calculate stats
    on_progress(OptimizeProgress {
        stage: "Calculating final stats...".into(),
        done: false,
    });

    let gear_prefix_id = validated.gear_prefix.as_ref().map(|p| p.itemstat_id);
    let full_stats = compute_candidate_stats(&candidate, db, gear_prefix_id, ctx);
    let derived = stats::compute_derived(&full_stats, profession_name);

    // Extract damage modifiers from traits/rune/sigils/relic, but cap to prevent
    // inflation from conditional/proc Fact::Percent values being treated as permanent.
    let modifiers = combat::extract_damage_modifiers(
        &candidate.all_trait_ids,
        candidate.rune.as_ref().map(|(id, _)| *id),
        &candidate
            .sigils
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        candidate.relic.as_ref().map(|(id, _)| *id),
        &db.traits,
        &db.items,
        ctx,
    );

    // 3-tier combat using profession-specific rotation profiles
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

    // Rotation
    on_progress(OptimizeProgress {
        stage: "Simulating rotation...".into(),
        done: false,
    });
    let rotation_result =
        engine::simulate_validated_rotation(&validated, db, &full_stats, scenario);

    on_progress(OptimizeProgress {
        stage: "Done".into(),
        done: true,
    });

    let (data_quality, quality_reasons) = engine::quality_from_modifiers(
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

#[cfg(test)]
mod runtime_diagnostics_tests {
    use super::*;

    use gw2_api::models::{
        EquipmentPiece, EquipmentStats, EquipmentTab, Fact, Item, ItemDetails, ItemStat,
        Profession, Skill, Specialization, StatAttribute, Trait as GW2Trait,
    };

    use crate::balance::BalanceContext;
    use crate::scenario::{CombatTier, ScenarioSpec};
    use crate::scoring::{score_with_weights, OptimizationWeights};

    fn make_damage_trait(id: u32, specialization: u32, pct: f64) -> GW2Trait {
        GW2Trait {
            id,
            name: format!("Diag Damage Trait {}", id),
            icon: None,
            description: Some(format!("Damage increased by {}%", pct)),
            specialization,
            tier: 1,
            order: 0,
            slot: "Major".into(),
            facts: vec![Fact::Percent {
                text: Some(format!("Damage increased by {}%", pct)),
                icon: None,
                percent: Some(pct),
            }],
            traited_facts: vec![],
            skills: vec![],
        }
    }

    fn make_vitality_trait(id: u32, specialization: u32, val: i32) -> GW2Trait {
        GW2Trait {
            id,
            name: format!("Diag Vitality Trait {}", id),
            icon: None,
            description: Some(format!("+{} Vitality", val)),
            specialization,
            tier: 1,
            order: 0,
            slot: "Major".into(),
            facts: vec![Fact::AttributeAdjust {
                text: Some("Vitality".into()),
                icon: None,
                value: Some(val),
                target: Some("Vitality".into()),
            }],
            traited_facts: vec![],
            skills: vec![],
        }
    }

    fn make_spec(
        spec_id: u32,
        elite: bool,
        profession: &str,
        minor: u32,
        major: [u32; 9],
    ) -> Specialization {
        Specialization {
            id: spec_id,
            name: format!("DiagSpec{}", spec_id),
            profession: profession.into(),
            elite,
            minor_traits: vec![minor],
            major_traits: major.to_vec(),
            weapon_trait: None,
            icon: None,
            background: None,
            profession_icon: None,
            profession_icon_big: None,
        }
    }

    fn make_equipment_item(id: u32) -> Item {
        Item {
            id,
            name: "Diag Ascended Armor Piece".into(),
            description: None,
            icon: None,
            item_type: "Armor".into(),
            rarity: "Ascended".into(),
            level: 80,
            vendor_value: Some(1),
            chat_link: None,
            default_skin: None,
            flags: vec![],
            game_types: vec!["PvE".into(), "WvW".into()],
            restrictions: vec![],
            details: Some(ItemDetails {
                detail_type: Some("Helm".into()),
                weight_class: Some("Heavy".into()),
                defense: Some(127),
                damage_type: None,
                min_power: None,
                max_power: None,
                suffix: None,
                bonuses: vec![],
                infusion_upgrade_flags: vec![],
                infusion_slots: vec![],
                attribute_adjustment: Some(141.0),
                infix_upgrade: None,
                suffix_item_id: None,
                secondary_suffix_item_id: None,
                stat_choices: vec![584],
            }),
        }
    }

    fn make_utility_skill(
        id: u32,
        name: &str,
        facts: Vec<Fact>,
        description: Option<&str>,
    ) -> Skill {
        Skill {
            id,
            name: name.into(),
            description: description.map(str::to_string),
            icon: None,
            chat_link: None,
            skill_type: None,
            weapon_type: None,
            professions: vec!["Warrior".into()],
            slot: Some("Utility".into()),
            facts,
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
        }
    }

    fn make_diag_db() -> GameDb {
        let mut itemstats = HashMap::new();
        // Canonical berserker-like profile.
        itemstats.insert(
            584,
            ItemStat {
                id: 584,
                name: "Berserker's".into(),
                attributes: vec![
                    StatAttribute {
                        attribute: "Power".into(),
                        multiplier: 0.35,
                        value: 32,
                    },
                    StatAttribute {
                        attribute: "Precision".into(),
                        multiplier: 0.25,
                        value: 18,
                    },
                    StatAttribute {
                        attribute: "CritDamage".into(),
                        multiplier: 0.25,
                        value: 18,
                    },
                ],
            },
        );
        // Ambiguous contains-match profile; weak on purpose.
        itemstats.insert(
            1584,
            ItemStat {
                id: 1584,
                name: "Berserker's Echo".into(),
                attributes: vec![
                    StatAttribute {
                        attribute: "Power".into(),
                        multiplier: 0.05,
                        value: 0,
                    },
                    StatAttribute {
                        attribute: "Precision".into(),
                        multiplier: 0.05,
                        value: 0,
                    },
                    StatAttribute {
                        attribute: "CritDamage".into(),
                        multiplier: 0.05,
                        value: 0,
                    },
                ],
            },
        );

        let mut traits = HashMap::new();

        // Spec 10 traits
        traits.insert(1001, make_damage_trait(1001, 10, 40.0)); // minor
        traits.insert(1010, make_damage_trait(1010, 10, 40.0));
        traits.insert(1011, make_vitality_trait(1011, 10, 150));
        traits.insert(1012, make_vitality_trait(1012, 10, 150));
        traits.insert(1020, make_damage_trait(1020, 10, 40.0));
        traits.insert(1021, make_vitality_trait(1021, 10, 150));
        traits.insert(1022, make_vitality_trait(1022, 10, 150));
        traits.insert(1030, make_damage_trait(1030, 10, 40.0));
        traits.insert(1031, make_vitality_trait(1031, 10, 150));
        traits.insert(1032, make_vitality_trait(1032, 10, 150));

        // Spec 11 traits
        traits.insert(1101, make_damage_trait(1101, 11, 40.0)); // minor
        traits.insert(1110, make_damage_trait(1110, 11, 40.0));
        traits.insert(1111, make_vitality_trait(1111, 11, 150));
        traits.insert(1112, make_vitality_trait(1112, 11, 150));
        traits.insert(1120, make_damage_trait(1120, 11, 40.0));
        traits.insert(1121, make_vitality_trait(1121, 11, 150));
        traits.insert(1122, make_vitality_trait(1122, 11, 150));
        traits.insert(1130, make_damage_trait(1130, 11, 40.0));
        traits.insert(1131, make_vitality_trait(1131, 11, 150));
        traits.insert(1132, make_vitality_trait(1132, 11, 150));

        // Spec 12 traits
        traits.insert(1201, make_damage_trait(1201, 12, 40.0)); // minor
        traits.insert(1210, make_damage_trait(1210, 12, 40.0));
        traits.insert(1211, make_vitality_trait(1211, 12, 150));
        traits.insert(1212, make_vitality_trait(1212, 12, 150));
        traits.insert(1220, make_damage_trait(1220, 12, 40.0));
        traits.insert(1221, make_vitality_trait(1221, 12, 150));
        traits.insert(1222, make_vitality_trait(1222, 12, 150));
        traits.insert(1230, make_damage_trait(1230, 12, 40.0));
        traits.insert(1231, make_vitality_trait(1231, 12, 150));
        traits.insert(1232, make_vitality_trait(1232, 12, 150));

        // Elite spec 30 traits
        traits.insert(1301, make_damage_trait(1301, 30, 40.0)); // minor
        traits.insert(1310, make_damage_trait(1310, 30, 40.0));
        traits.insert(1311, make_vitality_trait(1311, 30, 150));
        traits.insert(1312, make_vitality_trait(1312, 30, 150));
        traits.insert(1320, make_damage_trait(1320, 30, 40.0));
        traits.insert(1321, make_vitality_trait(1321, 30, 150));
        traits.insert(1322, make_vitality_trait(1322, 30, 150));
        traits.insert(1330, make_damage_trait(1330, 30, 40.0));
        traits.insert(1331, make_vitality_trait(1331, 30, 150));
        traits.insert(1332, make_vitality_trait(1332, 30, 150));

        let mut specializations = HashMap::new();
        specializations.insert(
            10,
            make_spec(
                10,
                false,
                "Warrior",
                1001,
                [1010, 1011, 1012, 1020, 1021, 1022, 1030, 1031, 1032],
            ),
        );
        specializations.insert(
            11,
            make_spec(
                11,
                false,
                "Warrior",
                1101,
                [1110, 1111, 1112, 1120, 1121, 1122, 1130, 1131, 1132],
            ),
        );
        specializations.insert(
            12,
            make_spec(
                12,
                false,
                "Warrior",
                1201,
                [1210, 1211, 1212, 1220, 1221, 1222, 1230, 1231, 1232],
            ),
        );
        specializations.insert(
            30,
            make_spec(
                30,
                true,
                "Warrior",
                1301,
                [1310, 1311, 1312, 1320, 1321, 1322, 1330, 1331, 1332],
            ),
        );

        let profession = Profession {
            id: "Warrior".into(),
            name: "Warrior".into(),
            code: None,
            specializations: vec![10, 11, 12, 30],
            weapons: HashMap::new(),
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        };

        let mut professions = HashMap::new();
        professions.insert("Warrior".into(), profession);

        let mut items = HashMap::new();
        items.insert(1000, make_equipment_item(1000));

        let mut traits_by_spec: HashMap<u32, Vec<u32>> = HashMap::new();
        for t in traits.values() {
            traits_by_spec
                .entry(t.specialization)
                .or_default()
                .push(t.id);
        }

        GameDb {
            items,
            itemstats,
            skills: HashMap::new(),
            traits,
            specializations,
            professions,
            legends: HashMap::new(),
            pvp_amulets: HashMap::new(),
            skills_by_profession: HashMap::new(),
            traits_by_spec,
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
            localized: None,
        }
    }

    fn loaded_build_trait_ids(db: &GameDb, spec_ids: &[u32]) -> Vec<u32> {
        let mut out = Vec::new();
        for spec_id in spec_ids {
            let spec = db.specializations.get(spec_id).expect("spec should exist");
            out.extend(spec.minor_traits.iter().copied());
            // Simulate loaded build selecting first trait in each column.
            if spec.major_traits.len() == 9 {
                out.push(spec.major_traits[0]);
                out.push(spec.major_traits[3]);
                out.push(spec.major_traits[6]);
            }
        }
        out
    }

    fn loaded_equipment(itemstat_id: u32) -> EquipmentTab {
        let mut equipment = Vec::new();
        for &slot in crate::search::STAT_SLOTS {
            equipment.push(EquipmentPiece {
                id: 1000,
                slot: slot.to_string(),
                location: Some("Equipped".into()),
                skin: None,
                upgrades: vec![],
                infusions: vec![],
                binding: None,
                bound_to: None,
                dyes: vec![],
                stats: Some(EquipmentStats {
                    id: itemstat_id,
                    attributes: None,
                }),
            });
        }

        EquipmentTab {
            tab: 1,
            name: Some("Diag Equip".into()),
            is_active: true,
            equipment,
            equipment_pvp: None,
        }
    }

    #[test]
    fn optimize_synergy_wvw_selects_required_viability_utilities() {
        let mut db = make_diag_db();
        let skills = vec![
            make_utility_skill(
                9_001,
                "Damage Utility A",
                vec![Fact::Percent {
                    text: Some("Damage Increase".into()),
                    icon: None,
                    percent: Some(25.0),
                }],
                None,
            ),
            make_utility_skill(
                9_002,
                "Damage Utility B",
                vec![Fact::Percent {
                    text: Some("Damage Increase".into()),
                    icon: None,
                    percent: Some(20.0),
                }],
                None,
            ),
            make_utility_skill(
                9_003,
                "Damage Utility C",
                vec![Fact::Percent {
                    text: Some("Damage Increase".into()),
                    icon: None,
                    percent: Some(15.0),
                }],
                None,
            ),
            make_utility_skill(
                9_101,
                "Stunbreak Utility",
                vec![Fact::StunBreak {
                    text: Some("Breaks Stun".into()),
                    icon: None,
                    value: Some(true),
                }],
                None,
            ),
            make_utility_skill(
                9_102,
                "Stability Utility",
                vec![Fact::Buff {
                    text: Some("Apply Buff/Condition".into()),
                    icon: None,
                    duration: Some(5),
                    status: Some("Stability".into()),
                    description: None,
                    apply_count: Some(1),
                }],
                None,
            ),
            make_utility_skill(
                9_103,
                "Cleanse Utility",
                vec![Fact::Number {
                    text: Some("Conditions Removed".into()),
                    icon: None,
                    value: Some(2),
                }],
                Some("Remove conditions from yourself."),
            ),
        ];
        db.skills_by_profession
            .insert("Warrior".into(), skills.iter().map(|s| s.id).collect());
        for skill in skills {
            db.skills.insert(skill.id, skill);
        }

        let weights = OptimizationWeights::preset_power_dps();
        let ctx = BalanceContext::new(gw2_core::types::GameMode::WvW);
        let mut progress = |_p: crate::engine::OptimizeProgress| {};
        let result = optimize_synergy(
            &db,
            "Warrior",
            &weights,
            &ctx,
            "Berserker's",
            &gw2_core::types::BuildLocks::default(),
            None,
            &mut progress,
        )
        .expect("synthetic Warrior should optimize");

        let selected_utilities: Vec<u32> = result
            .validated
            .skills
            .utilities
            .iter()
            .filter_map(|slot| slot.as_ref().map(|(id, _)| *id))
            .collect();

        assert!(selected_utilities.contains(&9_101), "missing stunbreak");
        assert!(selected_utilities.contains(&9_102), "missing stability");
        assert!(selected_utilities.contains(&9_103), "missing cleanse");

        let scenario = ScenarioSpec::from_balance_context(&ctx).with_combat_tier(CombatTier::Squad);
        let report = crate::referee::evaluate_validated_build(
            &result.validated,
            &db,
            "Warrior",
            &weights,
            &ctx,
            &scenario,
        );
        assert!(
            report.viability.is_viable,
            "selected utilities should satisfy competitive gates: {:?}",
            report.viability.gates
        );
    }

    #[test]
    fn test_runtime_non_improvement_diagnostics_two_contexts() {
        // Diagnostic: ranking now uses extracted modifiers (not empty / top-3 cap).
        let db = make_diag_db();
        let weights = OptimizationWeights::preset_power_dps();
        let ctx = BalanceContext::pve();
        let gear_prefix = crate::scoring::select_gear_prefix(&weights).primary;

        let contexts: Vec<(&str, gw2_core::types::BuildLocks, Vec<u32>)> = vec![
            (
                "Context-A (unlocked, loaded spec set: 10/11/30)",
                gw2_core::types::BuildLocks::default(),
                vec![10, 11, 30],
            ),
            (
                "Context-B (Improve-style elite lock slot2=30, loaded spec set: 10/12/30)",
                {
                    let mut l = gw2_core::types::BuildLocks::default();
                    l.specs[2] = Some(30);
                    l
                },
                vec![10, 12, 30],
            ),
        ];

        for (name, locks, loaded_specs) in contexts {
            println!("\n================ {} ================", name);
            println!(
                "weights: P={:.2} C={:.2} B={:.2} H={:.2} S={:.2} Ctrl={:.2} [{}]",
                weights.power,
                weights.condition,
                weights.boon_support,
                weights.healing,
                weights.sustain,
                weights.control,
                weights.summary_label()
            );
            println!(
                "locks: specs={:?}, trait_lock_specs={}",
                locks.specs,
                locks.trait_locks.len()
            );
            println!("deterministic prefix: {}", gear_prefix);

            let prefix_matches: Vec<(u32, String)> = db
                .itemstats
                .values()
                .filter(|is| is.name.contains(gear_prefix))
                .map(|is| (is.id, is.name.clone()))
                .collect();
            println!("prefix contains() matches: {:?}", prefix_matches);

            // ---- Baseline current-build computation (mirrors addon current-build path contracts) ----
            let loaded_trait_ids = loaded_build_trait_ids(&db, &loaded_specs);
            let equipment = loaded_equipment(584);
            let (baseline_stats, baseline_derived) = crate::stats::calculate_full_stats(
                &equipment,
                &loaded_trait_ids,
                None,
                &[],
                "Warrior",
                &db.items,
                &db.itemstats,
                &db.traits,
            );
            let baseline_mods = crate::combat::extract_damage_modifiers(
                &loaded_trait_ids,
                None,
                &[],
                None,
                &db.traits,
                &db.items,
                &ctx,
            );
            let baseline_perf = crate::combat::calculate_combat_performance(
                &baseline_stats,
                &baseline_derived,
                &baseline_mods,
                &crate::combat::buff_profiles_for_profession("Warrior", &ctx)[0],
                &crate::combat::condition_weights_for_profession("Warrior", &ctx),
                "Warrior",
                &ctx,
            );
            let baseline_score = score_with_weights(&baseline_perf, &weights);
            println!(
                "baseline stats/perf: power={:.1}, precision={:.1}, strike_dps={:.1}, total_dps={:.1}, score={:.4}",
                baseline_stats.power,
                baseline_stats.precision,
                baseline_perf.strike_dps_index,
                baseline_perf.total_dps_index,
                baseline_score
            );
            println!(
                "baseline modifiers: strike_mod_count={}, total_strike_mult={:.4}",
                baseline_mods.strike_pct.len(),
                baseline_mods.total_strike_mult()
            );

            // ---- Candidate generation and top pre-ranking traces ----
            let profession = db.profession("Warrior").expect("profession should exist");
            let mut candidates = select_specs_and_traits(
                profession,
                &weights,
                &db.specializations,
                &db.traits,
                &locks,
            );
            assert!(
                !candidates.is_empty(),
                "diagnostic setup should produce candidates"
            );
            select_rune(&mut candidates, &db, &weights);
            select_sigils(&mut candidates, &db, &weights, &ctx);
            select_relic(&mut candidates, &db, &weights);
            select_weapons(&mut candidates, profession, &db, &weights);
            select_skills(&mut candidates, &db, "Warrior", &weights, &ctx);

            println!("candidate_count_after_pipeline: {}", candidates.len());
            let mut synergy_only: Vec<(usize, f64, Vec<u32>)> = candidates
                .iter()
                .enumerate()
                .map(|(i, c)| (i, c.score, c.spec_ids.clone()))
                .collect();
            synergy_only.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (rank, (idx, score, specs)) in synergy_only.iter().take(5).enumerate() {
                println!(
                    "pre-rank synergy #{:<2} idx={} score={:.4} specs={:?}",
                    rank + 1,
                    idx,
                    score,
                    specs
                );
            }

            // ---- Ranking traces (combat_score without modifiers + final blend) ----
            let gear_prefix_id = db
                .itemstats
                .values()
                .find(|is| is.name.contains(gear_prefix))
                .map(|is| is.id);
            println!(
                "ranking contains() chosen gear_prefix_id: {:?}",
                gear_prefix_id
            );
            let max_synergy = candidates
                .iter()
                .map(|c| c.score)
                .fold(0.0_f64, f64::max)
                .max(1.0);
            let mut ranked_preview: Vec<(usize, f64, f64, f64, Vec<u32>)> = Vec::new();
            for (idx, c) in candidates.iter().enumerate() {
                let stats = compute_candidate_stats(c, &db, gear_prefix_id, &ctx);
                let derived = crate::stats::compute_derived(&stats, "Warrior");
                let modifiers = crate::combat::extract_damage_modifiers(
                    &c.all_trait_ids,
                    c.rune.as_ref().map(|(id, _)| *id),
                    &c.sigils.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                    c.relic.as_ref().map(|(id, _)| *id),
                    &db.traits,
                    &db.items,
                    &ctx,
                );
                let combat_perf = crate::combat::calculate_combat_performance(
                    &stats,
                    &derived,
                    &modifiers,
                    &crate::combat::buff_profiles_for_profession("Warrior", &ctx)[0],
                    &crate::combat::condition_weights_for_profession("Warrior", &ctx),
                    "Warrior",
                    &ctx,
                );
                let combat_score = score_with_weights(&combat_perf, &weights);
                let final_score = combat_score * 0.4 + (c.score / max_synergy) * 0.6;
                ranked_preview.push((idx, c.score, combat_score, final_score, c.spec_ids.clone()));
            }
            ranked_preview
                .sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
            for (rank, (idx, syn, combat_score, final_score, specs)) in
                ranked_preview.iter().take(5).enumerate()
            {
                println!(
                    "rank-trace #{:<2} idx={} synergy={:.4} combat={:.4} final={:.4} specs={:?}",
                    rank + 1,
                    idx,
                    syn,
                    combat_score,
                    final_score,
                    specs
                );
            }

            let selected = rank_and_select(
                &candidates,
                &db,
                "Warrior",
                gear_prefix,
                &weights,
                &ctx,
                None,
            )
            .expect("rank_and_select should produce best candidate");

            // ---- Final suggestion trace (with capped modifiers, as used by deterministic output) ----
            let mut progress = |_p: crate::engine::OptimizeProgress| {};
            let result = build_synergy_result(
                selected.clone(),
                &db,
                "Warrior",
                gear_prefix,
                &weights,
                &ctx,
                None,
                &mut progress,
            )
            .expect("build_synergy_result should succeed");

            let suggestion_score = score_with_weights(&result.combat_solo, &weights);
            println!(
                "final suggestion: strike_dps={:.1}, total_dps={:.1}, score={:.4}, capped_total_strike_mult={:.4}",
                result.combat_solo.strike_dps_index,
                result.combat_solo.total_dps_index,
                suggestion_score,
                result.modifiers.total_strike_mult()
            );
            println!(
                "validated gear prefix resolved in result: {:?}",
                result
                    .validated
                    .gear_prefix
                    .as_ref()
                    .map(|p| (p.itemstat_id, p.name.clone()))
            );

            // Uncapped candidate combat for direct contract mismatch visibility.
            let uncapped_mods = crate::combat::extract_damage_modifiers(
                &selected.all_trait_ids,
                selected.rune.as_ref().map(|(id, _)| *id),
                &selected
                    .sigils
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>(),
                selected.relic.as_ref().map(|(id, _)| *id),
                &db.traits,
                &db.items,
                &ctx,
            );
            let selected_stats = compute_candidate_stats(
                &selected,
                &db,
                result.validated.gear_prefix.as_ref().map(|p| p.itemstat_id),
                &ctx,
            );
            let selected_derived = crate::stats::compute_derived(&selected_stats, "Warrior");
            let uncapped_perf = crate::combat::calculate_combat_performance(
                &selected_stats,
                &selected_derived,
                &uncapped_mods,
                &crate::combat::buff_profiles_for_profession("Warrior", &ctx)[0],
                &crate::combat::condition_weights_for_profession("Warrior", &ctx),
                "Warrior",
                &ctx,
            );
            let uncapped_score = score_with_weights(&uncapped_perf, &weights);
            println!(
                "uncapped selected candidate: strike_mod_count={}, total_strike_mult={:.4}, score={:.4}",
                uncapped_mods.strike_pct.len(),
                uncapped_mods.total_strike_mult(),
                uncapped_score
            );

            println!(
                "comparison: baseline_score={:.4} vs suggestion_score={:.4} (delta={:.4})",
                baseline_score,
                suggestion_score,
                suggestion_score - baseline_score
            );

            assert!(
                result.combat_solo.total_dps_index + 0.1 >= baseline_perf.total_dps_index,
                "optimizer must not regress vs loaded baseline in {}, got baseline_total_dps={:.1}, suggestion_total_dps={:.1}",
                name,
                baseline_perf.total_dps_index,
                result.combat_solo.total_dps_index
            );
        }
    }
}

#[cfg(test)]
mod revenant_legend_tests {
    use super::*;
    use crate::balance::BalanceContext;
    use crate::scoring::OptimizationWeights;
    use gw2_api::models::{Legend, Skill};
    use gw2_core::types::GameMode;

    fn legend(
        id: &str,
        code: u32,
        heal: u32,
        elite: u32,
        utilities: [u32; 3],
        swap: u32,
    ) -> Legend {
        Legend {
            id: id.into(),
            code: Some(code),
            swap,
            heal,
            elite,
            utilities: utilities.to_vec(),
        }
    }

    fn empty_candidate() -> SynergyCandidate {
        SynergyCandidate {
            spec_ids: vec![],
            elite_spec: None,
            selected_major_traits: Vec::new(),
            all_trait_ids: Vec::new(),
            accumulated: Vec::new(),
            score: 0.0,
            rune: None,
            sigils: Vec::new(),
            relic: None,
            weapons: (None, None, None, None),
            heal: None,
            utilities: Vec::new(),
            elite_skill: None,
            legends: Vec::new(),
            aquatic_legends: Vec::new(),
            synergy_links: Vec::new(),
        }
    }

    fn slot_skill(id: u32, slot: &str) -> Skill {
        Skill {
            id,
            name: format!("Skill {id}"),
            description: None,
            icon: None,
            chat_link: None,
            skill_type: None,
            weapon_type: None,
            professions: vec!["Ranger".into()],
            slot: Some(slot.into()),
            facts: vec![],
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
        }
    }

    #[test]
    fn select_skills_skips_heals_without_template_palette() {
        let mut db = GameDb::empty_for_tests();
        db.skills.insert(200, slot_skill(200, "Heal"));
        db.skills.insert(100, slot_skill(100, "Heal"));
        db.skills_by_profession
            .insert("Ranger".into(), vec![200, 100]);
        db.skill_to_palette.insert(100, 120);
        let mut candidates = [empty_candidate()];
        select_skills(
            &mut candidates,
            &db,
            "Ranger",
            &OptimizationWeights::preset_power_dps(),
            &BalanceContext::new(GameMode::PvE),
        );
        assert_eq!(candidates[0].heal.as_ref().map(|(id, _)| *id), Some(100));
    }

    #[test]
    fn revenant_bar_is_one_legend_not_a_mix() {
        let mut db = GameDb::empty_for_tests();
        db.legends.insert(
            "Legend2".into(),
            legend("Legend2", 2, 100, 104, [101, 102, 103], 10),
        );
        db.legends.insert(
            "Legend3".into(),
            legend("Legend3", 3, 200, 204, [201, 202, 203], 11),
        );
        let mut candidates = [empty_candidate()];
        select_revenant_legends(
            &mut candidates,
            &db,
            &OptimizationWeights::preset_power_dps(),
        );
        let c = &candidates[0];
        assert_eq!(c.legends.len(), 2, "two stances for the template");
        let active = db.legends.get(&c.legends[0]).expect("active legend");
        assert_eq!(c.heal.as_ref().map(|(id, _)| *id), Some(active.heal));
        assert_eq!(
            c.elite_skill.as_ref().map(|(id, _)| *id),
            Some(active.elite)
        );
        let util_ids: Vec<u32> = c.utilities.iter().map(|(id, _)| *id).collect();
        assert_eq!(util_ids, active.utilities);
        assert!(
            !active.utilities.iter().any(|u| db
                .legends
                .get(&c.legends[1])
                .unwrap()
                .utilities
                .contains(u)),
            "utilities must not mix the inactive legend"
        );
    }
}

#[cfg(test)]
mod land_weapon_tests {
    use super::*;
    use gw2_api::models::{Profession, WeaponInfo};

    fn weapon(flags: &[&str]) -> WeaponInfo {
        WeaponInfo {
            specialization: None,
            flags: flags.iter().map(|s| (*s).to_string()).collect(),
            skills: vec![],
        }
    }

    fn guardian_with_trident() -> Profession {
        let mut weapons = std::collections::HashMap::new();
        weapons.insert("Sword".into(), weapon(&["Mainhand"]));
        weapons.insert("Focus".into(), weapon(&["Offhand"]));
        weapons.insert("Staff".into(), weapon(&["TwoHand"]));
        weapons.insert("Trident".into(), weapon(&["TwoHand", "Aquatic"]));
        Profession {
            id: "Guardian".into(),
            name: "Guardian".into(),
            code: None,
            specializations: vec![],
            weapons,
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        }
    }

    fn empty_candidate() -> SynergyCandidate {
        SynergyCandidate {
            spec_ids: vec![],
            elite_spec: None,
            selected_major_traits: Vec::new(),
            all_trait_ids: Vec::new(),
            accumulated: Vec::new(),
            score: 0.0,
            rune: None,
            sigils: Vec::new(),
            relic: None,
            weapons: (None, None, None, None),
            heal: None,
            utilities: Vec::new(),
            elite_skill: None,
            legends: Vec::new(),
            aquatic_legends: Vec::new(),
            synergy_links: Vec::new(),
        }
    }

    #[test]
    fn select_weapons_never_puts_trident_on_a_land_set() {
        let prof = guardian_with_trident();
        let db = GameDb::empty_for_tests();
        let mut candidates = [empty_candidate()];
        select_weapons(&mut candidates, &prof, &db, &OptimizationWeights::default());
        let (s1m, s1o, s2m, s2o) = &candidates[0].weapons;
        for w in [s1m, s1o, s2m, s2o].into_iter().flatten() {
            assert_ne!(
                w.as_str(),
                "Trident",
                "land sets must not include underwater weapons; got {:?}",
                candidates[0].weapons
            );
        }
        assert!(
            s1m.is_some(),
            "should still pick a land set; got {:?}",
            candidates[0].weapons
        );
    }
}
