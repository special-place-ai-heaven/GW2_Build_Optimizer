use super::stats::{compute_3tier_combat, perf_to_combat_metrics};
use crate::state::AddonState;
use gw2_optimizer::balance::BalanceContext;
use gw2_optimizer::scoring::OptimizationWeights;

/// Start optimization in background thread (S11-T01, S11-T02, S11-T03)
pub(super) fn start_optimization(state: &mut AddonState) {
    // Guard against concurrent optimization
    if state.main.optimizing {
        return;
    }

    // Get profession from current build
    let profession_name = state
        .main
        .current_build
        .as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();

    start_optimization_with_profession(state, &profession_name);
}

/// Start optimization with explicit profession name (avoids borrow conflicts).
/// Uses `state.main.build_locks` for spec/trait lock constraints.
pub(super) fn start_optimization_with_profession(state: &mut AddonState, profession_name: &str) {
    if state.main.game_db.is_none() {
        state.main.error = Some("Game data not loaded. Wait for cache to load.".into());
        return;
    }

    if profession_name.is_empty() {
        state.main.error = Some("No character selected".into());
        return;
    }

    let db = state.main.game_db.clone();
    let profession_name = profession_name.to_string();
    let config = state.config.clone();
    let game_mode = state.main.game_mode.clone();
    let game_mode_label = game_mode.label().to_string();
    let balance_ctx = BalanceContext::new(game_mode.clone());
    let current_build_summary = state
        .main
        .current_build
        .as_ref()
        .map(summarize_resolved_build);
    let addon_dir = state.addon_dir.clone();
    let token = state.cancel_token.clone();
    let weights = state.main.weights.clone();
    let selected_role = state.main.selected_role;
    let build_locks = state.main.build_locks.clone();
    // Capture WvW combat tier for ScenarioSpec construction inside the thread.
    let wvw_combat_tier = state.main.wvw_combat_tier;
    // Capture locked elite spec name for the Improve Build label.
    let locked_spec_name: Option<String> =
        build_locks.specs.get(2).and_then(|s| *s).and_then(|id| {
            state
                .main
                .game_db
                .as_ref()
                .and_then(|db| db.spec(id))
                .map(|s| s.name.clone())
        });
    // Capture selection snapshot so results can be discarded if the user switches
    // character or build tab while optimization is running (TOCTOU guard).
    let optimizing_for_char = state.main.selected_character;
    let optimizing_for_build_tab = state.main.selected_build_tab;
    let optimizing_for_equip_tab = state.main.selected_equipment_tab;

    state.main.optimizing = true;
    state.main.optimize_stage = "Starting...".into();

    // Log the weights and deterministic gear prefix for debugging
    let gear_match = gw2_optimizer::scoring::select_gear_prefix(&weights);
    let tier_label = if game_mode == gw2_core::types::GameMode::WvW {
        wvw_combat_tier.label().to_string()
    } else {
        String::new()
    };
    nexus::log::log(
        nexus::log::LogLevel::Info,
        "GW2BuildOpt",
        format!(
            "Optimizing {}/{}{}: weights P={:.2} C={:.2} B={:.2} H={:.2} S={:.2} Ctrl={:.2} ({}) -> gear: {} (sim={:.3})",
            profession_name,
            game_mode_label,
            if tier_label.is_empty() { String::new() } else { format!(" [{}]", tier_label) },
            weights.power, weights.condition, weights.boon_support, weights.healing, weights.sustain, weights.control,
            weights.summary_label(),
            gear_match.primary, gear_match.similarity,
        ),
    );
    state.main.comparison.suggestions.clear();
    state.main.comparison.loading = true;
    state.main.comparison.error = None;

    std::thread::spawn(move || {
        let panic_token = token.clone();
        let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let result = (|| -> Result<Vec<crate::ui::comparison::BuildSuggestion>, String> {
                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                let db = db.ok_or("GameDb not loaded")?;

                // Build a mode + tier-aware scenario for the referee and optimize_v2.
                let scenario = {
                    use gw2_optimizer::scenario::{
                        OptimizationTarget, ScenarioSpec, TargetProfile,
                    };
                    // Map the captured wvw_combat_tier (gw2_optimizer::scenario::CombatTier)
                    // directly — it's the same type.
                    ScenarioSpec {
                        game_mode: balance_ctx.game_mode.clone(),
                        combat_tier: wvw_combat_tier,
                        combat_kind: gw2_optimizer::scenario::CombatKind::StrikeSpike,
                        target_profile: TargetProfile::Single,
                        optimization_target: OptimizationTarget {
                            label: balance_ctx.game_mode.label().to_string(),
                        },
                        patch_id: Some(balance_ctx.patch_id.clone()),
                    }
                };

                // ═══ Primary: optimize_v2 — beam search over complete build states ═══
                {
                    let token_v2 = token.clone();
                    // Create LLM client for the advisor pass (optional — errors silently skip).
                    let llm_for_advisor: Option<Box<dyn gw2_optimizer::llm::LlmClient>> =
                        gw2_optimizer::llm::create_client(&config, &addon_dir).ok();
                    let llm_ref = llm_for_advisor.as_ref().map(|c| c.as_ref());
                    match gw2_optimizer::engine::optimize_v2(
                        &db,
                        &profession_name,
                        &weights,
                        &balance_ctx,
                        &scenario,
                        &build_locks,
                        llm_ref,
                        &mut |progress: gw2_optimizer::engine::OptimizeProgress| {
                            if token_v2.is_cancelled() {
                                return;
                            }
                            crate::state::with_state(|s| {
                                s.main.optimize_stage = progress.stage.clone();
                            });
                        },
                    ) {
                        Ok(synergy_result) => {
                            if token.is_cancelled() {
                                return Err("Cancelled".into());
                            }
                            let suggestion = synergy_result_to_suggestion(
                                &synergy_result,
                                &db,
                                &profession_name,
                                &scenario,
                                selected_role,
                                locked_spec_name
                                    .as_ref()
                                    .map(|n| format!("Improved: {}", n)),
                                &addon_dir,
                                &weights,
                            );
                            return Ok(vec![suggestion]);
                        }
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                format!(
                                    "optimize_v2 failed, falling back to synergy engine: {}",
                                    e
                                ),
                            );
                            // Fall through to legacy synergy engine
                        }
                    }
                }

                // ═══ Fallback 1: Deterministic synergy engine (no LLM for build selection) ═══
                {
                    let llm_client_opt: Option<Box<dyn gw2_optimizer::llm::LlmClient>> =
                        gw2_optimizer::llm::create_client(&config, &addon_dir).ok();

                    let token_det = token.clone();
                    let llm_ref: Option<&dyn gw2_optimizer::llm::LlmClient> =
                        llm_client_opt.as_ref().map(|c| c.as_ref());
                    match gw2_optimizer::engine::optimize_deterministic(
                        &db,
                        &profession_name,
                        &weights,
                        &balance_ctx,
                        llm_ref,
                        current_build_summary.as_deref(),
                        &build_locks,
                        &mut |progress: gw2_optimizer::engine::OptimizeProgress| {
                            if token_det.is_cancelled() {
                                return;
                            }
                            crate::state::with_state(|s| {
                                s.main.optimize_stage = progress.stage.clone();
                            });
                        },
                    ) {
                        Ok(synergy_result) => {
                            if token.is_cancelled() {
                                return Err("Cancelled".into());
                            }
                            let suggestion = synergy_result_to_suggestion(
                                &synergy_result,
                                &db,
                                &profession_name,
                                &scenario,
                                selected_role,
                                locked_spec_name
                                    .as_ref()
                                    .map(|n| format!("Improved: {}", n)),
                                &addon_dir,
                                &weights,
                            );
                            return Ok(vec![suggestion]);
                        }
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                format!(
                                    "Deterministic engine failed, trying Gemini pipeline: {}",
                                    e
                                ),
                            );
                            // Fall through to Gemini pipeline
                        }
                    }
                }

                // ═══ Fallback 2: LLM synergy pipeline (LLM-driven build selection) ═══
                if config.has_active_llm_key() {
                    let llm_client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                        .map_err(|e| e.to_string())?;

                    let token_synergy = token.clone();
                    match gw2_optimizer::engine::optimize_with_gemini(
                        &db,
                        &profession_name,
                        &weights,
                        &balance_ctx,
                        llm_client.as_ref(),
                        current_build_summary.as_deref(),
                        &build_locks,
                        &mut |progress| {
                            if token_synergy.is_cancelled() {
                                return;
                            }
                            crate::state::with_state(|s| {
                                s.main.optimize_stage = progress.stage.clone();
                            });
                        },
                    ) {
                        Ok(synergy_result) => {
                            if token.is_cancelled() {
                                return Err("Cancelled".into());
                            }
                            let suggestion = synergy_result_to_suggestion(
                                &synergy_result,
                                &db,
                                &profession_name,
                                &scenario,
                                selected_role,
                                locked_spec_name
                                    .as_ref()
                                    .map(|n| format!("Improved: {}", n)),
                                &addon_dir,
                                &weights,
                            );
                            return Ok(vec![suggestion]);
                        }
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                format!("LLM pipeline failed, falling back to legacy: {}", e),
                            );
                            // Fall through to legacy pipeline
                        }
                    }
                }

                // ═══ Fallback 3: Legacy pipeline (no Gemini key) ═══
                let profession = db.profession(&profession_name).ok_or_else(|| {
                    format!("Profession '{}' not found in GameDb", profession_name)
                })?;

                let token_progress = token.clone();
                let candidates = gw2_optimizer::engine::optimize(
                    profession,
                    &weights,
                    None,
                    &db.items,
                    &db.itemstats,
                    &db.specializations,
                    &db.traits,
                    |progress| {
                        if token_progress.is_cancelled() {
                            return;
                        }
                        crate::state::with_state(|s| {
                            s.main.optimize_stage = progress.stage.clone();
                        });
                    },
                    5,
                    &balance_ctx,
                    &build_locks,
                    &db.pvp_amulets,
                )?;

                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                let mut suggestions: Vec<crate::ui::comparison::BuildSuggestion> = candidates
                    .iter()
                    .map(|c| candidate_to_suggestion(c, &db, &balance_ctx))
                    .collect();

                // Enrich top suggestion with LLM reasoning (legacy path)
                if config.has_active_llm_key() {
                    if token.is_cancelled() {
                        return Err("Cancelled".into());
                    }

                    crate::state::with_state(|s| {
                        s.main.optimize_stage = "Consulting AI for synergy analysis...".into();
                    });

                    match enrich_with_llm(
                        &config,
                        &profession_name,
                        &weights,
                        &game_mode_label,
                        &candidates,
                        &db,
                        current_build_summary.as_deref(),
                        &mut suggestions,
                        &addon_dir,
                        &balance_ctx,
                    ) {
                        Ok(()) => {}
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                format!("LLM enrichment skipped: {}", e),
                            );
                        }
                    }
                }

                Ok(suggestions)
            })();

            if !token.is_cancelled() {
                crate::state::with_state(|s| {
                    s.main.optimizing = false;
                    s.main.comparison.loading = false;
                    // Discard results if the user switched character or build tab while
                    // optimization was running — showing results for a different context
                    // would silently corrupt the comparison panel.
                    let context_changed = s.main.selected_character != optimizing_for_char
                        || s.main.selected_build_tab != optimizing_for_build_tab
                        || s.main.selected_equipment_tab != optimizing_for_equip_tab;
                    if context_changed {
                        return;
                    }
                    match result {
                        Ok(suggestions) => {
                            s.main.comparison.suggestions = suggestions;
                            s.main.comparison.selected_suggestion = 0;
                            s.main.comparison.show_optimized = true;
                        }
                        Err(e) => {
                            s.main.comparison.error = Some(e);
                        }
                    }
                });
            } else {
                // Cancelled mid-flight. Without resetting these flags the UI stays stuck
                // on the "Optimizing…" spinner until another optimization completes.
                crate::state::with_state(|s| {
                    s.main.optimizing = false;
                    s.main.comparison.loading = false;
                });
            }
        }));

        // If the thread panicked, recover and show error
        if let Err(panic_info) = thread_result {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("Internal error (panic): {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("Internal error (panic): {}", s)
            } else {
                "Internal error: optimization thread panicked".into()
            };
            if !panic_token.is_cancelled() {
                crate::state::with_state(|s| {
                    s.main.optimizing = false;
                    s.main.comparison.loading = false;
                    s.main.comparison.error = Some(msg);
                });
            }
        }
    });
}

/// Convert a SynergyResult from the new pipeline into a BuildSuggestion for display.
// Display adapter; db, profession, scenario, role, and result are distinct
// inputs threaded straight through — a params struct adds no clarity here.
#[allow(clippy::too_many_arguments)]
fn synergy_result_to_suggestion(
    result: &gw2_optimizer::engine::SynergyResult,
    db: &gw2_optimizer::gamedb::GameDb,
    profession_name: &str,
    scenario: &gw2_optimizer::scenario::ScenarioSpec,
    role: Option<gw2_optimizer::scenario::RoleObjective>,
    label_override: Option<String>,
    addon_dir: &std::path::Path,
    weights: &gw2_optimizer::scoring::OptimizationWeights,
) -> crate::ui::comparison::BuildSuggestion {
    use crate::ui::comparison::BuildSuggestion;

    let v = &result.validated;
    let chat_code = validated_build_to_chat_code(v, profession_name, db);

    // Specializations: (name, [trait_name1, trait_name2, trait_name3])
    let specializations: Vec<(String, Vec<String>)> = v
        .specializations
        .iter()
        .map(|s| {
            let label = if s.elite {
                format!("{} [E]", s.name)
            } else {
                s.name.clone()
            };
            (label, s.trait_names.clone())
        })
        .collect();

    // Weapons: flatten into display strings like "Set 1: Sword / Shield"
    let mut weapons = Vec::new();
    let fmt_set =
        |set: &gw2_optimizer::validation::ValidatedWeaponSet, label: &str| -> Option<String> {
            match (&set.main_hand, &set.off_hand) {
                (Some(main), Some(off)) => Some(format!("{}: {} / {}", label, main, off)),
                (Some(main), None) => Some(format!("{}: {}", label, main)),
                _ => None,
            }
        };
    if let Some(s) = fmt_set(&v.weapons.set1, "Set 1") {
        weapons.push(s);
    }
    if let Some(s) = fmt_set(&v.weapons.set2, "Set 2") {
        weapons.push(s);
    }

    // Skills: flatten into display strings
    let mut skills = Vec::new();
    if !v.legends.is_empty() {
        let names: Vec<String> = v
            .legends
            .iter()
            .map(|id| {
                db.legends
                    .get(id)
                    .and_then(|l| db.skills.get(&l.swap))
                    .map(|s| crate::ui::comparison::compact_stance_name(&s.name))
                    .unwrap_or_else(|| id.clone())
            })
            .collect();
        skills.push(format!("Stances: {}", names.join(" / ")));
    }
    if let Some((t1, t2, _, _)) = v.pets {
        let ids: Vec<String> = [t1, t2]
            .into_iter()
            .flatten()
            .map(|id| format!("#{id}"))
            .collect();
        if !ids.is_empty() {
            skills.push(format!("Pets: {}", ids.join(" / ")));
        }
    }
    if let Some((_, name)) = &v.skills.heal {
        skills.push(format!("Heal: {}", name));
    }
    for (_, name) in v.skills.utilities.iter().flatten() {
        skills.push(format!("Utility: {}", name));
    }
    if let Some((_, name)) = &v.skills.elite {
        skills.push(format!("Elite: {}", name));
    }

    // Sigils: flatten to display strings
    let sigils: Vec<String> = v.sigils.iter().map(|s| s.name.clone()).collect();

    // Convert stats from optimizer StatBlock (f64) to core StatBlock (i32)
    let derived = gw2_optimizer::stats::compute_derived(&result.stats, profession_name);
    let estimated_stats = Some(gw2_core::types::StatBlock {
        power: result.stats.power.round() as i32,
        precision: result.stats.precision.round() as i32,
        toughness: result.stats.toughness.round() as i32,
        vitality: result.stats.vitality.round() as i32,
        condition_damage: result.stats.condition_damage.round() as i32,
        expertise: result.stats.expertise.round() as i32,
        concentration: result.stats.concentration.round() as i32,
        ferocity: result.stats.ferocity.round() as i32,
        healing_power: result.stats.healing_power.round() as i32,
        crit_chance: derived.crit_chance,
        crit_damage: derived.crit_damage,
        health: derived.health.round() as i32,
        armor: derived.armor.round() as i32,
    });

    // Convert combat performance to CombatMetrics
    let combat_solo = Some(perf_to_combat_metrics(&result.combat_solo));
    let combat_party = Some(perf_to_combat_metrics(&result.combat_party));
    let combat_squad = Some(perf_to_combat_metrics(&result.combat_squad));

    // Convert rotation simulation result
    let rotation = result
        .rotation
        .as_ref()
        .map(|sim| gw2_core::types::RotationBreakdown {
            simulated_dps: sim.total_dps.round() as i32,
            strike_dps: sim.strike_dps.round() as i32,
            condition_dps: sim.condition_dps.round() as i32,
            condition_uptime: sim
                .condition_uptime
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            buff_uptime: sim
                .buff_uptime
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            skill_usage: sim
                .skill_usage
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        s.cast_count,
                        s.dps_contribution.round() as i32,
                    )
                })
                .collect(),
            stunbreak_count: sim.stunbreak_count,
            has_stability: sim.has_stability,
            stability_uptime: sim.stability_uptime,
            cleanse_count: sim.cleanse_count,
            cleanse_rate_per_20s: sim.cleanse_rate_per_20s,
        });

    // Build changes_made from validated structured changes
    let changes_made: Vec<String> = v
        .changes
        .iter()
        .map(|c| {
            if c.from.is_empty() {
                format!("[{}] → {} ({})", c.slot, c.to, c.reason)
            } else {
                format!("[{}] {} → {} ({})", c.slot, c.from, c.to, c.reason)
            }
        })
        .collect();

    // Warnings as additional info
    let mut explanation = v.explanation.clone();
    if !v.warnings.is_empty() {
        if !explanation.is_empty() {
            explanation.push_str("\n\n");
        }
        explanation.push_str("Warnings: ");
        explanation.push_str(&v.warnings.join("; "));
    }

    // Compute viability gates from the referee using the scenario
    let primary_combat = match scenario.combat_tier {
        gw2_optimizer::scenario::CombatTier::Solo => &result.combat_solo,
        gw2_optimizer::scenario::CombatTier::Party => &result.combat_party,
        gw2_optimizer::scenario::CombatTier::Squad => &result.combat_squad,
    };
    let viability = Some(gw2_optimizer::referee::evaluate_viability_gates(
        result.rotation.as_ref(),
        primary_combat,
        scenario,
    ));

    // Suggestion label: label_override > role name > generic
    let label = label_override
        .or_else(|| role.map(|r| r.label().to_string()))
        .unwrap_or_else(|| "Optimized Build".to_string());

    // Compute benchmark delta vs best matching community reference
    let role_hint = role.map(|r| r.label().to_string()).unwrap_or_default();
    let our_score = {
        // Use normalised strike + condi DPS index as proxy score when referee score unavailable
        let s = &result.combat_solo;
        let strike_norm = s.strike_dps_index / 3000.0;
        let condi_norm = s.condition_dps_index / 3500.0;
        strike_norm.max(condi_norm).min(1.0)
    };
    let benchmark_delta = {
        let builds = gw2_optimizer::scraper::load_benchmarks(addon_dir);
        if builds.is_empty() {
            None
        } else {
            gw2_optimizer::benchmark::compute_benchmark_delta(
                &builds,
                profession_name,
                scenario.game_mode.label(),
                &role_hint,
                weights,
                our_score,
            )
        }
    };

    BuildSuggestion {
        label,
        build_summary: format!(
            "Gear: {}",
            v.gear_prefix
                .as_ref()
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown")
        ),
        stat_prefix: v
            .gear_prefix
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        specializations,
        weapons,
        skills,
        rune: v.rune.as_ref().map(|r| r.name.clone()).unwrap_or_default(),
        sigils,
        relic: v.relic.as_ref().map(|r| r.name.clone()).unwrap_or_default(),
        chat_code,
        explanation,
        synergy_explanation: v.synergy_explanation.clone(),
        changes_made,
        estimated_stats,
        combat_solo,
        combat_party,
        combat_squad,
        rotation,
        viability,
        benchmark_delta,
        data_quality: result.data_quality.clone(),
        quality_reasons: result
            .quality_reasons
            .iter()
            .map(|r| r.to_string())
            .collect(),
    }
}

fn validated_build_to_chat_code(
    build: &gw2_optimizer::validation::ValidatedBuild,
    profession_name: &str,
    db: &gw2_optimizer::gamedb::GameDb,
) -> Option<String> {
    let skills = gw2_api::models::SkillSelection {
        heal: build.skills.heal.as_ref().map(|(id, _)| *id),
        utilities: build
            .skills
            .utilities
            .iter()
            .take(3)
            .map(|skill| skill.as_ref().map(|(id, _)| *id))
            .collect(),
        elite: build.skills.elite.as_ref().map(|(id, _)| *id),
    };
    let pets = match build.pets {
        Some((t1, t2, a1, a2)) => Some(gw2_api::models::PetSelection {
            terrestrial: vec![t1, t2],
            aquatic: vec![a1, a2],
        }),
        None if profession_name == "Ranger" => snapshot_ranger_pets(),
        None => None,
    };
    let api_build = gw2_api::models::Build {
        name: None,
        profession: Some(profession_name.to_string()),
        specializations: build
            .specializations
            .iter()
            .map(|spec| gw2_api::models::SpecSelection {
                id: Some(spec.spec_id),
                traits: spec.trait_ids.iter().take(3).map(|id| Some(*id)).collect(),
            })
            .collect(),
        skills: Some(skills.clone()),
        aquatic_skills: Some(skills),
        legends: build.legends.iter().map(|id| Some(id.clone())).collect(),
        aquatic_legends: {
            let src = if build.aquatic_legends.is_empty() {
                &build.legends
            } else {
                &build.aquatic_legends
            };
            src.iter().map(|id| Some(id.clone())).collect()
        },
        pets,
    };
    let weapons = [
        build.weapons.set1.main_hand.as_deref(),
        build.weapons.set1.off_hand.as_deref(),
        build.weapons.set2.main_hand.as_deref(),
        build.weapons.set2.off_hand.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::to_string)
    .collect::<Vec<_>>();

    super::character::generate_build_chat_code(&api_build, db, &weapons)
}

fn snapshot_ranger_pets() -> Option<gw2_api::models::PetSelection> {
    crate::state::with_state(|s| {
        s.main
            .selected_build_tab
            .and_then(|i| s.main.build_tabs.get(i).and_then(|t| t.build.pets.clone()))
    })
    .flatten()
}

fn candidate_to_suggestion(
    candidate: &gw2_optimizer::engine::BuildCandidate,
    db: &gw2_optimizer::gamedb::GameDb,
    balance_ctx: &BalanceContext,
) -> crate::ui::comparison::BuildSuggestion {
    use crate::ui::comparison::BuildSuggestion;

    // Get spec names with actually selected traits (not all 9)
    let mut specializations = Vec::new();
    if let Some(elite_id) = candidate.elite_spec {
        if let Some(spec) = db.spec(elite_id) {
            let traits: Vec<String> = candidate
                .equipped_traits
                .iter()
                .filter(|tid| spec.major_traits.contains(tid))
                .filter_map(|&tid| db.traits.get(&tid).map(|t| t.name.clone()))
                .collect();
            specializations.push((format!("{} [E]", spec.name), traits));
        }
    }
    for &core_id in &candidate.core_specs {
        if let Some(spec) = db.spec(core_id) {
            let traits: Vec<String> = candidate
                .equipped_traits
                .iter()
                .filter(|tid| spec.major_traits.contains(tid))
                .filter_map(|&tid| db.traits.get(&tid).map(|t| t.name.clone()))
                .collect();
            specializations.push((spec.name.clone(), traits));
        }
    }

    // Convert stats from optimizer::stats::StatBlock to core::types::StatBlock
    let estimated_stats = Some(gw2_core::types::StatBlock {
        power: candidate.stats.power.round() as i32,
        precision: candidate.stats.precision.round() as i32,
        toughness: candidate.stats.toughness.round() as i32,
        vitality: candidate.stats.vitality.round() as i32,
        condition_damage: candidate.stats.condition_damage.round() as i32,
        expertise: candidate.stats.expertise.round() as i32,
        concentration: candidate.stats.concentration.round() as i32,
        ferocity: candidate.stats.ferocity.round() as i32,
        healing_power: candidate.stats.healing_power.round() as i32,
        crit_chance: candidate.derived.crit_chance,
        crit_damage: candidate.derived.crit_damage,
        health: candidate.derived.health.round() as i32,
        armor: candidate.derived.armor.round() as i32,
    });

    // Compute combat metrics for all 3 buff profiles.
    // Determine profession from the candidate's specs. The "Warrior" fallback
    // is only reached if the candidate has no specs at all, which a valid
    // BuildCandidate never has — kept here so combat math always has a
    // profession name. Previously this fell back to
    // `db.professions.values().next()`, whose order is unspecified.
    let prof_name = if let Some(elite_id) = candidate.elite_spec {
        db.spec(elite_id)
            .map(|s| s.profession.as_str())
            .unwrap_or("Warrior")
    } else if let Some(&core_id) = candidate.core_specs.first() {
        db.spec(core_id)
            .map(|s| s.profession.as_str())
            .unwrap_or("Warrior")
    } else {
        "Warrior"
    };

    let (combat_solo, combat_party, combat_squad) = compute_3tier_combat(
        &candidate.stats,
        &candidate.derived,
        &candidate.modifiers,
        prof_name,
        balance_ctx,
    );

    // Legacy path: no rotation available, rotation-dependent gates produce degraded state.
    // Use a simple EHP proxy from vitality for the viability check.
    let legacy_viability = {
        let scenario = gw2_optimizer::scenario::ScenarioSpec::from_balance_context(balance_ctx);
        let proxy_perf = gw2_optimizer::combat::CombatPerformance {
            effective_health: candidate.stats.vitality * 10.0,
            ..Default::default()
        };
        gw2_optimizer::referee::evaluate_viability_gates(None, &proxy_perf, &scenario)
    };

    BuildSuggestion {
        label: format!("Score: {:.2}", candidate.score),
        build_summary: format!("Gear: {}", candidate.gear.stat_prefix_name),
        stat_prefix: candidate.gear.stat_prefix_name.clone(),
        specializations,
        weapons: Vec::new(),
        skills: Vec::new(),
        rune: String::new(),
        sigils: Vec::new(),
        relic: String::new(),
        chat_code: None,
        explanation: String::new(),
        synergy_explanation: String::new(),
        changes_made: Vec::new(),
        estimated_stats,
        combat_solo,
        combat_party,
        combat_squad,
        rotation: None,
        viability: Some(legacy_viability),
        benchmark_delta: None,
        data_quality: gw2_optimizer::data::DataQuality::Verified,
        quality_reasons: vec![],
    }
}

/// Run rotation simulation for a suggestion's skills and attach the results.
///
/// Resolves ALL build skills: weapon skills from both weapon sets (tagged for
/// weapon swap scheduling) + heal/utility/elite from the skills list.
/// The simulator uses DPCT-optimal scheduling with automatic weapon swapping.
pub(super) fn simulate_suggestion_rotation(
    suggestion: &mut crate::ui::comparison::BuildSuggestion,
    db: &gw2_optimizer::gamedb::GameDb,
) {
    if suggestion.skills.is_empty() && suggestion.weapons.is_empty() {
        return;
    }

    let mut all_rotation_skills: Vec<gw2_optimizer::rotation::RotationSkill> = Vec::new();

    // 1. Resolve weapon skills from suggestion.weapons (format: "Set 1: Axe / Axe").
    //
    // Use the pre-built `skills_by_profession` index instead of scanning all
    // ~500 skills per (profession × weapon set × weapon type) — that scan was
    // also nondeterministic across runs because `db.skills.values()` iteration
    // order is unspecified.
    if !suggestion.weapons.is_empty() {
        let profession = infer_profession_from_specs(&suggestion.specializations, db);
        let weapon_sets = parse_weapon_sets(&suggestion.weapons);
        let prof_skill_ids = db.skills_by_profession.get(profession.as_str());

        for (set_num, weapon_types) in &weapon_sets {
            let mut set_skill_ids: Vec<u32> = Vec::new();
            if let Some(ids) = prof_skill_ids {
                for &id in ids {
                    let Some(skill) = db.skills.get(&id) else {
                        continue;
                    };
                    let matches_weapon = weapon_types
                        .iter()
                        .any(|wt| skill.weapon_type.as_deref() == Some(wt.as_str()));
                    if !matches_weapon {
                        continue;
                    }
                    let is_weapon_slot = skill
                        .slot
                        .as_deref()
                        .map(|s| s.starts_with("Weapon_"))
                        .unwrap_or(false);
                    if !is_weapon_slot {
                        continue;
                    }
                    if !set_skill_ids.contains(&skill.id) {
                        set_skill_ids.push(skill.id);
                    }
                }
            }
            if !set_skill_ids.is_empty() {
                let mut set_skills =
                    gw2_optimizer::rotation::builder::build_rotation_skills(&set_skill_ids, db);
                gw2_optimizer::rotation::builder::tag_weapon_set(&mut set_skills, *set_num);
                all_rotation_skills.extend(set_skills);
            }
        }
    }

    // 2. Resolve heal/utility/elite from suggestion.skills.
    //    Format: "Heal: Name", "Utils: Name1, Name2, Name3", "Elite: Name".
    //
    // Walk skills_by_profession (sorted, scoped) instead of all db.skills —
    // deterministic order plus faster than the ~500-entry scan. We still need
    // exact-name match so the smaller candidate set is iterated linearly.
    let skill_names = parse_skill_names(&suggestion.skills);
    if !skill_names.is_empty() {
        let profession = infer_profession_from_specs(&suggestion.specializations, db);
        let prof_skill_ids = db.skills_by_profession.get(profession.as_str());
        // Hoist the sorted skill-id list once so the global fallback below
        // doesn't re-collect-and-sort `db.skills.keys()` (~500 ids) per skill
        // name. Only allocated when at least one name will be searched.
        let mut all_skill_ids_sorted: Option<Vec<u32>> = None;
        for name in &skill_names {
            let found_skill = prof_skill_ids.and_then(|ids| {
                ids.iter()
                    .filter_map(|id| db.skills.get(id))
                    .find(|s| s.name.eq_ignore_ascii_case(name))
            });
            // Fallback: scan all skills if the profession index missed (e.g.
            // shared utility-like skills not registered under profession).
            // Iterate by id so a name with multiple matches (e.g. "Bandage")
            // resolves to the same skill across runs — `HashMap::values()`
            // order is unspecified.
            let skill = found_skill.or_else(|| {
                let ids = all_skill_ids_sorted.get_or_insert_with(|| {
                    let mut v: Vec<u32> = db.skills.keys().copied().collect();
                    v.sort_unstable();
                    v
                });
                ids.iter()
                    .filter_map(|id| db.skills.get(id))
                    .find(|s| s.name.eq_ignore_ascii_case(name))
            });
            if let Some(skill) = skill {
                if !all_rotation_skills.iter().any(|rs| rs.skill_id == skill.id) {
                    let mut rs_vec =
                        gw2_optimizer::rotation::builder::build_rotation_skills(&[skill.id], db);
                    // Non-weapon skills stay at weapon_set=0 (always available)
                    all_rotation_skills.append(&mut rs_vec);
                }
            }
        }
    }

    if all_rotation_skills.is_empty() {
        return;
    }

    // Extract stats from estimated_stats for the simulation
    let stats = suggestion.estimated_stats.as_ref();
    let power = stats.map(|s| s.power as f64).unwrap_or(1000.0);
    let condition_damage = stats.map(|s| s.condition_damage as f64).unwrap_or(0.0);
    let weapon_strength = 1100.0; // reference weapon strength (same as combat.rs)

    let result = gw2_optimizer::rotation::simulator::simulate(
        &all_rotation_skills,
        0,
        power,
        condition_damage,
        weapon_strength,
    );

    suggestion.rotation = Some(gw2_core::types::RotationBreakdown {
        simulated_dps: result.total_dps.round() as i32,
        strike_dps: result.strike_dps.round() as i32,
        condition_dps: result.condition_dps.round() as i32,
        condition_uptime: result.condition_uptime.into_iter().collect(),
        buff_uptime: result.buff_uptime.into_iter().collect(),
        skill_usage: result
            .skill_usage
            .iter()
            .map(|su| {
                (
                    su.name.clone(),
                    su.cast_count,
                    su.dps_contribution.round() as i32,
                )
            })
            .collect(),
        stunbreak_count: result.stunbreak_count,
        has_stability: result.has_stability,
        stability_uptime: result.stability_uptime,
        cleanse_count: result.cleanse_count,
        cleanse_rate_per_20s: result.cleanse_rate_per_20s,
    });
}

/// Parse weapon sets from suggestion.weapons strings.
/// Input format: "Set 1: Axe / Axe", "Set 2: Greatsword"
/// Returns: [(1, ["Axe", "Axe"]), (2, ["Greatsword"])]
fn parse_weapon_sets(weapons: &[String]) -> Vec<(u8, Vec<String>)> {
    let mut sets = Vec::new();
    for w in weapons {
        let set_num = if w.starts_with("Set 1") {
            1u8
        } else if w.starts_with("Set 2") {
            2u8
        } else {
            1u8
        }; // fallback

        let rest = w.split(':').nth(1).unwrap_or(w).trim();
        let types: Vec<String> = rest
            .split('/')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "null")
            .collect();

        if !types.is_empty() {
            sets.push((set_num, types));
        }
    }
    sets
}

/// Parse individual skill names from formatted suggestion.skills strings.
/// "Heal: Mending" → "Mending"
/// "Utils: Blood Reckoning, Bull's Charge, Signet of Fury" → 3 names
/// "Elite: Head Butt" → "Head Butt"
fn parse_skill_names(skills: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for s in skills {
        if let Some(rest) = s.strip_prefix("Heal: ") {
            names.push(rest.trim().to_string());
        } else if let Some(rest) = s.strip_prefix("Utils: ") {
            for name in rest.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        } else if let Some(rest) = s.strip_prefix("Elite: ") {
            names.push(rest.trim().to_string());
        } else if let Some(rest) = s.strip_prefix("Utility: ") {
            let name = rest.trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
        } else {
            // Fallback: try the whole string as a skill name
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                names.push(trimmed.to_string());
            }
        }
    }
    names
}

/// Infer profession name from specialization names in the suggestion.
fn infer_profession_from_specs(
    specs: &[(String, Vec<String>)],
    db: &gw2_optimizer::gamedb::GameDb,
) -> String {
    // Walk specializations in id order so name collisions across
    // professions (defensive — GW2 currently has unique spec names but
    // data drift could introduce duplicates) resolve to the same
    // profession across runs and machines. `HashMap::values()` order is
    // unspecified.
    let mut spec_ids: Vec<u32> = db.specializations.keys().copied().collect();
    spec_ids.sort_unstable();
    for (spec_name, _) in specs {
        let clean = spec_name.replace(" [E]", "");
        for sid in &spec_ids {
            if let Some(spec) = db.specializations.get(sid) {
                if spec.name.eq_ignore_ascii_case(&clean) {
                    return spec.profession.clone();
                }
            }
        }
    }
    // Fallback: return empty string. The previous
    // `db.professions.values().next()` picked a random profession from
    // HashMap iteration order — non-deterministic and almost certainly the
    // wrong profession anyway. Callers downstream that key on the profession
    // (e.g. `skills_by_profession.get(name)`) will simply find nothing, which
    // is the correct outcome when we cannot infer.
    String::new()
}

/// Summarize a ResolvedBuild as text for LLM prompts.
fn summarize_resolved_build(build: &gw2_core::types::ResolvedBuild) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Profession: {}", build.profession));

    let specs: Vec<String> = build
        .specializations
        .iter()
        .map(|s| {
            let elite = if s.elite { " [E]" } else { "" };
            let traits: Vec<&str> = s.traits_selected.iter().map(|t| t.name.as_str()).collect();
            format!("{}{}: {}", s.name, elite, traits.join(", "))
        })
        .collect();
    if !specs.is_empty() {
        parts.push(format!("Specs: {}", specs.join(" | ")));
    }

    if let Some(ref h) = build.skills.heal {
        parts.push(format!("Heal: {}", h.name));
    }
    let utils: Vec<String> = build
        .skills
        .utilities
        .iter()
        .filter_map(|u| u.as_ref().map(|s| s.name.clone()))
        .collect();
    if !utils.is_empty() {
        parts.push(format!("Utils: {}", utils.join(", ")));
    }
    if let Some(ref e) = build.skills.elite {
        parts.push(format!("Elite: {}", e.name));
    }

    for set in &build.weapons {
        let mut w = Vec::new();
        if let Some(ref mh) = set.main_hand {
            w.push(mh.weapon_type.clone());
        }
        if let Some(ref oh) = set.off_hand {
            w.push(oh.weapon_type.clone());
        }
        if !w.is_empty() {
            parts.push(format!("{}: {}", set.label, w.join(" / ")));
        }
    }

    if !build.armor.is_empty() && !build.armor[0].stat_prefix.is_empty() {
        parts.push(format!("Gear: {}", build.armor[0].stat_prefix));
    }
    if let Some(ref r) = build.rune {
        parts.push(format!("Rune: {}", r.name));
    }
    if let Some(ref r) = build.relic {
        parts.push(format!("Relic: {}", r.name));
    }

    parts.join("\n")
}

/// Apply Gemini's parsed response onto a BuildSuggestion.
fn apply_gemini_response(
    suggestion: &mut crate::ui::comparison::BuildSuggestion,
    gemini: &gw2_optimizer::prompts::GeminiBuildResponse,
) {
    if !gemini.explanation.is_empty() {
        suggestion.explanation = gemini.explanation.clone();
    }
    if let Some(ref synergy) = gemini.synergy_explanation {
        if !synergy.is_empty() {
            suggestion.synergy_explanation = synergy.clone();
        }
    }
    if !gemini.specializations.is_empty() {
        suggestion.specializations = gemini.specializations.clone();
    }
    if !gemini.weapons.is_empty() {
        suggestion.weapons = gemini.weapons.clone();
    }
    if !gemini.skills.is_empty() {
        suggestion.skills = gemini.skills.clone();
    }
    if !gemini.rune.is_empty() {
        suggestion.rune = gemini.rune.clone();
    }
    if !gemini.sigils.is_empty() {
        suggestion.sigils = gemini.sigils.clone();
    }
    if !gemini.relic.is_empty() {
        suggestion.relic = gemini.relic.clone();
    }
    if !gemini.stat_prefix.is_empty() {
        suggestion.stat_prefix = gemini.stat_prefix.clone();
    }
    if !gemini.changes_made.is_empty() {
        suggestion.changes_made = gemini.changes_made.clone();
    }
}

/// Convert Gemini tool function names to human-readable descriptions.
fn humanize_tool_names(tool_names: &[String]) -> String {
    let labels: Vec<&str> = tool_names
        .iter()
        .map(|n| match n.as_str() {
            "get_profession_info" => "reading profession",
            "get_spec_traits" => "checking traits",
            "get_trait_details" => "analyzing trait",
            "get_skill_info" => "checking skill",
            "list_runes" => "browsing runes",
            "list_sigils" => "browsing sigils",
            "list_relics" => "browsing relics",
            "calculate_stats" => "calculating stats",
            "simulate_combat" => "simulating combat",
            "score_build" => "scoring build",
            "get_current_build" => "reading current build",
            "get_optimizer_results" => "reviewing candidates",
            "search_traits_by_effect" => "searching trait synergies",
            "find_condition_sources" => "finding condition sources",
            "search_skills_by_effect" => "searching skill synergies",
            "find_synergies" => "analyzing synergies",
            "get_build_synergy_report" => "building synergy report",
            "simulate_rotation" => "simulating rotation",
            _ => "working",
        })
        .collect();
    labels.join(", ")
}

/// Call the active LLM provider to enrich the top optimizer suggestion with AI reasoning.
/// Uses function calling (tool use) so the LLM can query game data and simulate builds.
// LLM enrichment call; config, profession, weights, mode, and candidates are
// independent inputs — grouping them adds indirection without clarity.
#[allow(clippy::too_many_arguments)]
fn enrich_with_llm(
    config: &gw2_core::config::AppConfig,
    profession_name: &str,
    weights: &OptimizationWeights,
    game_mode: &str,
    candidates: &[gw2_optimizer::engine::BuildCandidate],
    db: &gw2_optimizer::gamedb::GameDb,
    current_build_summary: Option<&str>,
    suggestions: &mut [crate::ui::comparison::BuildSuggestion],
    addon_dir: &std::path::Path,
    balance_ctx: &BalanceContext,
) -> Result<(), String> {
    let client = gw2_optimizer::llm::create_client(config, addon_dir).map_err(|e| e.to_string())?;

    // Build tool-aware prompt
    let prompt = if current_build_summary.is_some() {
        gw2_optimizer::prompts::improve_build_prompt_with_tools(profession_name, weights, game_mode)
    } else {
        gw2_optimizer::prompts::new_build_prompt_with_tools(profession_name, weights, game_mode)
    };

    let tools = gw2_optimizer::llm::tools::tool_definitions();
    let build_summary_owned = current_build_summary.map(|s| s.to_string());
    let ctx = gw2_optimizer::gemini_tools::ToolContext {
        db,
        profession_name,
        candidates,
        current_build_summary: build_summary_owned.as_deref(),
        weights: weights.clone(),
        balance_ctx,
    };

    let response = client
        .generate_with_tools_progress(
            &prompt,
            &tools,
            &mut |name: &str, args: &serde_json::Value| {
                gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx)
            },
            8,
            &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                let tools_str = humanize_tool_names(tool_names);
                crate::state::with_state(|s| {
                    s.main.optimize_stage =
                        format!("AI thinking ({}/{})... {}", turn, max_turns, tools_str);
                });
            },
        )
        .map_err(|e| e.to_string())?;

    let gemini_build = gw2_optimizer::prompts::parse_gemini_build(&response)
        .map_err(|e| format!("Parse failed: {}", e))?;

    // Validate LLM's output against GameDb before applying
    let validated =
        gw2_optimizer::validation::validate_gemini_build(&gemini_build, db, profession_name);
    if !validated.errors.is_empty() {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            format!(
                "Legacy enrichment validation errors: {}",
                validated
                    .errors
                    .iter()
                    .map(|e| e.detail.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        );
    }

    crate::state::with_state(|s| {
        s.main.optimize_stage = "Applying AI build + simulating rotation...".into();
    });

    if let Some(first) = suggestions.first_mut() {
        apply_gemini_response(first, &gemini_build);
        // Run rotation simulation now that LLM has populated skills
        simulate_suggestion_rotation(first, db);
    }

    Ok(())
}

/// Send a chat message to the active LLM provider for build refinement.
/// Uses function calling so the LLM can query game data to answer questions.
pub(super) fn send_chat_message(state: &mut AddonState, message: String) {
    // Guard against concurrent chat messages
    if state.main.chat.waiting {
        return;
    }

    if !state.config.has_active_llm_key() {
        crate::ui::chat_bar::add_ai_response(
            &mut state.main.chat,
            "No AI API key configured. Set one in Settings.".into(),
        );
        return;
    }

    state.main.chat.waiting = true;

    let config = state.config.clone();
    let profession = state
        .main
        .current_build
        .as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();
    let build_summary = state
        .main
        .current_build
        .as_ref()
        .map(summarize_resolved_build);
    let addon_dir = state.addon_dir.clone();
    let token = state.cancel_token.clone();
    let db_clone = state.main.game_db.clone();
    let weights = state.main.weights.clone();
    let chat_balance_ctx = BalanceContext::new(state.main.game_mode.clone());

    std::thread::spawn(move || {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if token.is_cancelled() {
                return;
            }

            let result = (|| -> Result<gw2_optimizer::prompts::GeminiBuildResponse, String> {
                let client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                    .map_err(|e| e.to_string())?;

                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                // Use tool-enabled generation if GameDb is available
                if let Some(ref db) = db_clone {
                    let prompt = gw2_optimizer::prompts::chat_refinement_prompt_with_tools(
                        &profession,
                        &message,
                    );
                    let tools = gw2_optimizer::llm::tools::tool_definitions();
                    let empty_candidates = vec![];
                    let ctx = gw2_optimizer::gemini_tools::ToolContext {
                        db,
                        profession_name: &profession,
                        candidates: &empty_candidates,
                        current_build_summary: build_summary.as_deref(),
                        weights: weights.clone(),
                        balance_ctx: &chat_balance_ctx,
                    };

                    let response = client
                        .generate_with_tools_progress(
                            &prompt,
                            &tools,
                            &mut |name: &str, args: &serde_json::Value| {
                                gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx)
                            },
                            8,
                            &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                                let tools_str = humanize_tool_names(tool_names);
                                crate::state::with_state(|s| {
                                    s.main.optimize_stage = format!(
                                        "AI thinking ({}/{})... {}",
                                        turn, max_turns, tools_str
                                    );
                                });
                            },
                        )
                        .map_err(|e| e.to_string())?;

                    gw2_optimizer::prompts::parse_gemini_build(&response)
                        .map_err(|e| format!("Parse failed: {}", e))
                } else {
                    // Fallback: no GameDb, use simple prompt
                    let build_summary_str = build_summary.as_deref().unwrap_or("");
                    let context =
                        gw2_optimizer::prompts::build_game_context(&profession, &weights, "PvE");
                    let prompt = gw2_optimizer::prompts::chat_refinement_prompt(
                        &profession,
                        build_summary_str,
                        &message,
                        &context,
                    );
                    let response = client.generate_cached(&prompt).map_err(|e| e.to_string())?;
                    gw2_optimizer::prompts::parse_gemini_build(&response)
                        .map_err(|e| format!("Parse failed: {}", e))
                }
            })();

            // Validate Gemini's response against GameDb before applying (if available)
            let validated_result = result.as_ref().ok().and_then(|gemini_build| {
                db_clone.as_ref().map(|db| {
                    let validated = gw2_optimizer::validation::validate_gemini_build(
                        gemini_build,
                        db,
                        &profession,
                    );
                    if !validated.errors.is_empty() {
                        nexus::log::log(
                            nexus::log::LogLevel::Warning,
                            "GW2BuildOpt",
                            format!(
                                "Chat refinement validation errors: {}",
                                validated
                                    .errors
                                    .iter()
                                    .map(|e| e.detail.as_str())
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            ),
                        );
                    }
                    validated
                })
            });
            let _ = validated_result; // Validation logged; apply_gemini_response uses raw parsed fields

            if !token.is_cancelled() {
                crate::state::with_state(|s| match result {
                    Ok(gemini_build) => {
                        let display = if gemini_build.explanation.is_empty() {
                            "Build updated.".to_string()
                        } else {
                            gemini_build.explanation.clone()
                        };
                        crate::ui::chat_bar::add_ai_response(&mut s.main.chat, display);

                        let mut suggestion = crate::ui::comparison::BuildSuggestion {
                            label: "Chat Refinement".into(),
                            ..Default::default()
                        };
                        apply_gemini_response(&mut suggestion, &gemini_build);
                        if let Some(ref db) = s.main.game_db {
                            simulate_suggestion_rotation(&mut suggestion, db);
                        }
                        s.main.comparison.error = None;
                        s.main.comparison.suggestions.push(suggestion);
                        s.main.comparison.selected_suggestion =
                            s.main.comparison.suggestions.len() - 1;
                        s.main.comparison.show_optimized = true;
                    }
                    Err(e) => {
                        crate::ui::chat_bar::add_ai_response(
                            &mut s.main.chat,
                            format!("Error: {}", e),
                        );
                    }
                });
            } else {
                // Cancelled mid-flight. `add_ai_response` would normally clear the
                // waiting flag; without this branch the UI stays stuck on "AI thinking…"
                // until another chat completes successfully.
                crate::state::with_state(|s| {
                    s.main.chat.waiting = false;
                });
            }
        }));
        if panic_result.is_err() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                "bg thread panicked: send_chat_message",
            );
            crate::state::with_state(|s| {
                s.main.chat.waiting = false;
            });
        }
    });
}
