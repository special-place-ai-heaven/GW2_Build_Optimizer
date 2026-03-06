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

use gw2_api::models::{Profession, Skill, Specialization, Trait as GW2Trait};
use crate::balance::BalanceContext;
use crate::combat;
use crate::data;
use crate::engine::{self, OptimizeProgress, SynergyResult};
use crate::gamedb::GameDb;
use crate::scoring::{score_with_weights, OptimizationWeights};
use crate::search::search_spec_combos;
use crate::stats;
use crate::synergy::{
    self, ComponentId, NormalizedEffect, SynergyLink, extract_relic_effects,
    extract_rune_effects, extract_sigil_effects, extract_skill_effects,
    extract_trait_effects, score_normalized_effect, compute_marginal_synergy,
};
use crate::validation::{
    ValidatedBuild, ValidatedGearPrefix, ValidatedItem, ValidatedSkills, ValidatedSpec,
    ValidatedWeaponSet, ValidatedWeapons,
};

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
    weapons: (Option<String>, Option<String>, Option<String>, Option<String>),
    /// Skill selections: (heal, utilities[3], elite).
    heal: Option<(u32, String)>,
    utilities: Vec<(u32, String)>,
    elite_skill: Option<(u32, String)>,
    /// Synergy links discovered during selection.
    synergy_links: Vec<SynergyLink>,
}

// ─── Main Entry Point ───

/// Run the full deterministic synergy pipeline.
/// Returns a SynergyResult with a fully determined build.
pub fn optimize_synergy(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    gear_prefix_name: &str,
    locks: &gw2_core::types::BuildLocks,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<SynergyResult, String> {
    let profession = db.profession(profession_name)
        .ok_or_else(|| format!("Profession '{}' not found", profession_name))?;

    // Stage 2: Specs + Traits
    on_progress(OptimizeProgress {
        stage: "Evaluating specializations and traits...".into(),
        done: false,
    });
    let mut candidates = select_specs_and_traits(
        profession, weights, &db.specializations, &db.traits, locks,
    );

    if candidates.is_empty() {
        return Err(format!("No valid spec/trait combinations found for {}", profession_name));
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
    select_sigils(&mut candidates, db, weights);

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
    select_skills(&mut candidates, db, profession_name, weights);

    // Stage 8: Final ranking with full combat performance
    on_progress(OptimizeProgress {
        stage: "Computing final combat metrics...".into(),
        done: false,
    });

    let best = rank_and_select(
        &candidates, db, profession_name, gear_prefix_name, weights, ctx,
    )?;

    // Convert to SynergyResult
    on_progress(OptimizeProgress {
        stage: "Building validated result...".into(),
        done: false,
    });

    build_synergy_result(best, db, profession_name, gear_prefix_name, weights, ctx, on_progress)
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
        let spec_ids: Vec<u32> = cores
            .iter()
            .copied()
            .chain(elite.iter().copied())
            .collect();

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
                let r: Vec<usize> = (0..3).filter(|&i| spec.major_traits[i] == locked_id).collect();
                if r.is_empty() { vec![0, 1, 2] } else { r }
            } else {
                vec![0, 1, 2]
            };
            let master_range: Vec<usize> = if let Some(locked_id) = trait_lock.and_then(|t| t[1]) {
                let r: Vec<usize> = (0..3).filter(|&i| spec.major_traits[3 + i] == locked_id).collect();
                if r.is_empty() { vec![0, 1, 2] } else { r }
            } else {
                vec![0, 1, 2]
            };
            let grandmaster_range: Vec<usize> = if let Some(locked_id) = trait_lock.and_then(|t| t[2]) {
                let r: Vec<usize> = (0..3).filter(|&i| spec.major_traits[6 + i] == locked_id).collect();
                if r.is_empty() { vec![0, 1, 2] } else { r }
            } else {
                vec![0, 1, 2]
            };

            // Enumerate trait combos (respecting locks — locked columns have 1 option)
            let mut configs: Vec<(Vec<u32>, f64)> = Vec::new();
            for &a in &adept_range {
                for &m in &master_range {
                    for &g in &grandmaster_range {
                        let traits = vec![
                            spec.major_traits[a],       // Adept
                            spec.major_traits[3 + m],   // Master
                            spec.major_traits[6 + g],   // Grandmaster
                        ];

                        // Score this config
                        let all_ids: Vec<u32> = spec.minor_traits.iter()
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
                    let (syn, links) = compute_marginal_synergy(&effs, &accumulated, weights);
                    total_score += syn;
                    all_links.extend(links);
                    accumulated.push((ComponentId::Trait(tid), effs));
                }
            }

            // Keep top 10 most impactful synergy links to avoid explanation clutter
            all_links.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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
                synergy_links: all_links,
            });
        }
    }

    // Sort by score and keep top 5
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(5);

    candidates
}

/// Generate cross-product of per-spec trait configurations.
fn cross_product_trait_configs(
    per_spec: &[Vec<(Vec<u32>, f64)>],
) -> Vec<Vec<Vec<u32>>> {
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

fn select_rune(
    candidates: &mut [SynergyCandidate],
    db: &GameDb,
    weights: &OptimizationWeights,
) {
    let runes = db.all_runes();
    // Filter to Superior runes only
    let superior_runes: Vec<_> = runes.iter()
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
            let base: f64 = effects.iter()
                .map(|e| score_normalized_effect(e, weights))
                .sum();

            // Synergy with existing traits/specs
            let (syn, links) = compute_marginal_synergy(&effects, &candidate.accumulated, weights);

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
            candidate.accumulated.push((ComponentId::Rune(rune.0), best_effects));
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
) {
    let sigils = db.all_sigils();
    let superior_sigils: Vec<_> = sigils.iter()
        .filter(|s| s.name.contains("Superior"))
        .collect();

    for candidate in candidates.iter_mut() {
        // GW2 sigil rule: duplicates are forbidden *within* a weapon set (Set 1 or Set 2),
        // but the same sigil type IS allowed in both sets independently.
        // We select 2 sigils per set with separate dedup sets.
        for set_idx in 0..2usize {
            let _ = set_idx; // slot identity only used for ordering; sets are symmetric
            let mut set_ids: Vec<u32> = Vec::new();

            for _slot in 0..2 {
                let mut best_score = f64::NEG_INFINITY;
                let mut best_sigil: Option<(u32, String)> = None;
                let mut best_effects: Vec<NormalizedEffect> = Vec::new();
                let mut best_links: Vec<SynergyLink> = Vec::new();

                for &&sigil in &superior_sigils {
                    if set_ids.contains(&sigil.id) {
                        continue; // No duplicate sigils within this weapon set
                    }

                    let effects = extract_sigil_effects(sigil);
                    let base: f64 = effects.iter()
                        .map(|e| score_normalized_effect(e, weights))
                        .sum();
                    let (syn, links) = compute_marginal_synergy(&effects, &candidate.accumulated, weights);

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
                    candidate.synergy_links.extend(best_links);
                    candidate.accumulated.push((ComponentId::Sigil(sigil.0), best_effects));
                    candidate.sigils.push(sigil);
                    candidate.score += best_score;
                }
            }
        }
    }
}

// ─── Stage 5: Relic ───

fn select_relic(
    candidates: &mut [SynergyCandidate],
    db: &GameDb,
    weights: &OptimizationWeights,
) {
    let relics = db.all_relics();

    for candidate in candidates.iter_mut() {
        let mut best_score = f64::NEG_INFINITY;
        let mut best_relic: Option<(u32, String)> = None;
        let mut best_effects: Vec<NormalizedEffect> = Vec::new();
        let mut best_links: Vec<SynergyLink> = Vec::new();

        for &relic in &relics {
            let effects = extract_relic_effects(relic);

            let base: f64 = effects.iter()
                .map(|e| score_normalized_effect(e, weights))
                .sum();
            let (syn, links) = compute_marginal_synergy(&effects, &candidate.accumulated, weights);

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
            candidate.accumulated.push((ComponentId::Relic(relic.0), best_effects));
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
        let elite_spec_ids: Vec<u32> = candidate.spec_ids.iter()
            .filter(|&&id| db.specializations.get(&id).map(|s| s.elite).unwrap_or(false))
            .copied()
            .collect();

        // Filter weapons by elite spec gate
        let available: Vec<(&str, bool)> = profession.weapons.iter()
            .filter(|(_, info)| {
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

        // Generate weapon combos (simplified: pick best set 1, then best set 2)
        let mut best_set1 = (None, None);
        let mut best_set1_score = f64::NEG_INFINITY;

        for &(weapon, is_2h) in &available {
            let info = &profession.weapons[weapon];
            let is_main = info.flags.iter().any(|f| f == "Mainhand" || f == "TwoHand");
            if is_2h {
                // Two-handed weapon as set 1
                let score = score_weapon_skills(weapon, profession, db, weights, &candidate.accumulated);
                if score > best_set1_score {
                    best_set1_score = score;
                    best_set1 = (Some(weapon.to_string()), None);
                }
            } else if is_main {
                // Main-hand + each valid off-hand
                for &(off_weapon, off_2h) in &available {
                    if off_2h { continue; }
                    let off_info = &profession.weapons[off_weapon];
                    let off_is_off = off_info.flags.iter().any(|f| f == "Offhand");
                    if !off_is_off { continue; }

                    let score = score_weapon_skills(weapon, profession, db, weights, &candidate.accumulated)
                        + score_weapon_skills(off_weapon, profession, db, weights, &candidate.accumulated);
                    if score > best_set1_score {
                        best_set1_score = score;
                        best_set1 = (Some(weapon.to_string()), Some(off_weapon.to_string()));
                    }
                }

                // Main-hand only (no off-hand)
                let score = score_weapon_skills(weapon, profession, db, weights, &candidate.accumulated);
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
                let score = score_weapon_skills(weapon, profession, db, weights, &candidate.accumulated);
                if score > best_set2_score {
                    best_set2_score = score;
                    best_set2 = (Some(weapon.to_string()), None);
                }
            } else if is_main {
                for &(off_weapon, off_2h) in &available {
                    if off_2h { continue; }
                    let off_info = &profession.weapons[off_weapon];
                    if !off_info.flags.iter().any(|f| f == "Offhand") { continue; }

                    let score = score_weapon_skills(weapon, profession, db, weights, &candidate.accumulated)
                        + score_weapon_skills(off_weapon, profession, db, weights, &candidate.accumulated);
                    if score > best_set2_score {
                        best_set2_score = score;
                        best_set2 = (Some(weapon.to_string()), Some(off_weapon.to_string()));
                    }
                }

                // Main-hand only (no off-hand) — mirrors Set 1 lines 555-560.
                // Without this, Set 2 can never be main-hand-only even when that
                // scores higher than any main+offhand combination.
                let score = score_weapon_skills(weapon, profession, db, weights, &candidate.accumulated);
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
            let (syn, _) = compute_marginal_synergy(&effects, accumulated, weights);
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
) {
    let prof_skills = db.profession_skills(profession_name);

    for candidate in candidates.iter_mut() {
        // Filter skills by elite spec gate: only allow skills whose specialization
        // is None (core skill) or matches one of the candidate's equipped specs.
        let gated_skills: Vec<&&Skill> = prof_skills.iter()
            .filter(|s| match s.specialization {
                Some(spec_id) => candidate.spec_ids.contains(&spec_id),
                None => true,
            })
            .collect();

        // Heal skill
        let heals: Vec<&&Skill> = gated_skills.iter()
            .filter(|s| s.slot.as_deref() == Some("Heal"))
            .copied()
            .collect();
        if let Some((id, name, links)) = pick_best_skill(&heals, weights, &candidate.accumulated) {
            // Add heal effects to accumulated for subsequent skill synergy scoring
            if let Some(skill) = db.skills.get(&id) {
                let effects = extract_skill_effects(skill);
                candidate.accumulated.push((ComponentId::Skill(id), effects));
            }
            candidate.synergy_links.extend(links);
            candidate.heal = Some((id, name));
        }

        // Elite skill
        let elites: Vec<&&Skill> = gated_skills.iter()
            .filter(|s| s.slot.as_deref() == Some("Elite"))
            .copied()
            .collect();
        if let Some((id, name, links)) = pick_best_skill(&elites, weights, &candidate.accumulated) {
            // Add elite effects to accumulated for subsequent skill synergy scoring
            if let Some(skill) = db.skills.get(&id) {
                let effects = extract_skill_effects(skill);
                candidate.accumulated.push((ComponentId::Skill(id), effects));
            }
            candidate.synergy_links.extend(links);
            candidate.elite_skill = Some((id, name));
        }

        // Utility skills (3, greedy sequential)
        let utilities: Vec<&&Skill> = gated_skills.iter()
            .filter(|s| s.slot.as_deref() == Some("Utility"))
            .copied()
            .collect();

        let mut used_ids: Vec<u32> = Vec::new();
        for _ in 0..3 {
            let available: Vec<&&Skill> = utilities.iter()
                .filter(|s| !used_ids.contains(&s.id))
                .copied()
                .collect();

            if let Some((id, name, links)) = pick_best_skill(&available, weights, &candidate.accumulated) {
                used_ids.push(id);
                // Add effects to accumulated for next utility selection
                if let Some(skill) = db.skills.get(&id) {
                    let effects = extract_skill_effects(skill);
                    candidate.accumulated.push((ComponentId::Skill(id), effects));
                }
                candidate.synergy_links.extend(links);
                candidate.utilities.push((id, name));
            }
        }
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
        let base: f64 = effects.iter()
            .map(|e| score_normalized_effect(e, weights))
            .sum();
        let (syn, links) = compute_marginal_synergy(&effects, accumulated, weights);

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

    let gear_prefix_id = db.itemstats.values()
        .find(|is| is.name.contains(gear_prefix_name))
        .map(|is| is.id);

    let solo_profile = &combat::buff_profiles_for_profession(profession_name, ctx)[0];

    for (idx, candidate) in candidates.iter().enumerate() {
        let stats = compute_candidate_stats(
            candidate, db, gear_prefix_id,
        );
        let derived = stats::compute_derived(&stats, profession_name);

        // Use default (identity) modifiers for ranking to avoid trait fact inflation.
        // extract_damage_modifiers() treats conditional/proc Fact::Percent values
        // as permanent multipliers, producing total_strike_mult of 5-15x.
        // The synergy scoring already evaluates trait modifier value.
        let modifiers = combat::DamageModifiers::default();

        let combat_perf = combat::calculate_combat_performance(
            &stats, &derived, &modifiers, solo_profile,
            &combat::condition_weights_for_profession(profession_name, ctx),
            profession_name,
            ctx,
        );

        let combat_score = score_with_weights(&combat_perf, weights);
        // Blend: 40% combat performance (gear-only), 60% synergy score (captures trait value)
        let max_synergy = candidates.iter().map(|c| c.score).fold(0.0_f64, f64::max).max(1.0);
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
) -> stats::StatBlock {
    let mut full_stats = stats::base_stats();

    // Gear stats (from prefix applied to each slot via slot budget data)
    if let Some(stat_id) = gear_prefix_id {
        if let Some(itemstat) = db.itemstats.get(&stat_id) {
            let budgets = data::slot_budgets::slot_budgets();
            let shape = data::stat_shape_from_attr_count(itemstat.attributes.len());
            for &(slot_type, _) in data::EQUIPMENT_SLOTS {
                if let Some(budget) = budgets.get(slot_type, shape) {
                    engine::add_budget_stats_for_itemstat(
                        &mut full_stats, itemstat, budget,
                    );
                }
            }
        }
    }

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
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<SynergyResult, String> {
    // Build ValidatedBuild
    let mut validated = ValidatedBuild::default();

    // Gear prefix
    if let Some(is) = db.itemstats.values().find(|is| is.name.contains(gear_prefix_name)) {
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
        utilities: candidate.utilities.iter().map(|u| Some(u.clone())).collect(),
        elite: candidate.elite_skill.clone(),
    };

    // Rune
    if let Some((id, name)) = &candidate.rune {
        validated.rune = Some(ValidatedItem { id: *id, name: name.clone() });
    }

    // Sigils
    for (id, name) in &candidate.sigils {
        validated.sigils.push(ValidatedItem { id: *id, name: name.clone() });
    }

    // Relic
    if let Some((id, name)) = &candidate.relic {
        validated.relic = Some(ValidatedItem { id: *id, name: name.clone() });
    }

    // Synergy explanation
    validated.synergy_explanation = synergy::template_explanation(
        &candidate.synergy_links,
        gear_prefix_name,
        profession_name,
    );

    // Calculate stats
    on_progress(OptimizeProgress {
        stage: "Calculating final stats...".into(),
        done: false,
    });

    let gear_prefix_id = validated.gear_prefix.as_ref().map(|p| p.itemstat_id);
    let full_stats = compute_candidate_stats(&candidate, db, gear_prefix_id);
    let derived = stats::compute_derived(&full_stats, profession_name);

    // Extract damage modifiers from traits/rune/sigils/relic, but cap to prevent
    // inflation from conditional/proc Fact::Percent values being treated as permanent.
    let mut modifiers = combat::extract_damage_modifiers(
        &candidate.all_trait_ids,
        candidate.rune.as_ref().map(|(id, _)| *id),
        &candidate.sigils.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        candidate.relic.as_ref().map(|(id, _)| *id),
        &db.traits,
        &db.items,
        ctx,
    );
    // Cap multiplicative modifiers to realistic ranges.
    // extract_damage_modifiers() treats conditional/proc Fact::Percent as permanent,
    // producing runaway multiplication (total_strike_mult of 5-15x).
    // In GW2, permanent trait modifiers rarely exceed ~30% total (3-4 sources of 5-7%).
    // Keep only the top-3 largest modifiers from each category.
    cap_modifiers_vec(&mut modifiers.strike_pct, 3);
    cap_modifiers_vec(&mut modifiers.condition_pct, 3);
    cap_modifiers_vec(&mut modifiers.crit_damage_pct, 3);
    cap_modifiers_vec(&mut modifiers.healing_pct, 3);

    // 3-tier combat using profession-specific rotation profiles
    let buff_profiles = combat::buff_profiles_for_profession(profession_name, ctx);
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

    // Rotation
    on_progress(OptimizeProgress {
        stage: "Simulating rotation...".into(),
        done: false,
    });
    let rotation_result = engine::simulate_validated_rotation(&validated, db, &full_stats);

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

/// Keep only the top-N largest modifiers in a Vec, discarding the rest.
/// Prevents runaway multiplicative inflation from conditional/proc trait facts.
fn cap_modifiers_vec(v: &mut Vec<f64>, max_entries: usize) {
    if v.len() > max_entries {
        v.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        v.truncate(max_entries);
    }
}
