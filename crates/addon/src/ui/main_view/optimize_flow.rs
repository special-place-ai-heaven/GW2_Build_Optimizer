use super::optimization::{
    apply_gemini_response, candidate_to_suggestion, humanize_tool_names, result_alert_tab,
    simulate_suggestion_rotation, summarize_resolved_build, synergy_result_to_suggestion,
};
use crate::state::AddonState;
use gw2_core::i18n::{t, tf};
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
        state.main.error = Some(t("err.no_gamedb"));
        return;
    }

    if state.main.chat.waiting {
        state.main.error = Some(t("err.chat_busy"));
        return;
    }

    if profession_name.is_empty() {
        state.main.error = Some(t("err.no_character"));
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
    let combat_tier = match game_mode {
        gw2_core::types::GameMode::WvW => state.main.wvw_combat_tier,
        gw2_core::types::GameMode::PvP => gw2_optimizer::scenario::CombatTier::Solo,
        gw2_core::types::GameMode::PvE => gw2_optimizer::scenario::CombatTier::Party,
    };
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
    state.main.optimize_stage = t("status.starting");

    // Log the weights and deterministic gear prefix for debugging
    let gear_match = gw2_optimizer::scoring::select_gear_prefix(&weights);
    let tier_label = combat_tier.label().to_string();
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
                    let combat_kind = selected_role
                        .map(|r| r.combat_kind_for_weights(&weights))
                        .unwrap_or_else(|| {
                            if weights.condition > weights.power {
                                gw2_optimizer::scenario::CombatKind::CondiRamp
                            } else {
                                gw2_optimizer::scenario::CombatKind::StrikeSpike
                            }
                        });
                    ScenarioSpec {
                        game_mode: balance_ctx.game_mode.clone(),
                        combat_tier,
                        combat_kind,
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
                        &|| token.is_cancelled(),
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
                                    .map(|n| tf("fmt.improved", &[("name", n)])),
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
                        Some(&scenario),
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
                                    .map(|n| tf("fmt.improved", &[("name", n)])),
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
                                    "Deterministic engine failed, falling back to legacy: {}",
                                    e
                                ),
                            );
                        }
                    }
                }

                // ═══ Fallback 2: Legacy pipeline (no LLM invent-a-build) ═══
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
                        s.main.optimize_stage = t("status.consulting");
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
                            s.main.tab_alert =
                                Some(result_alert_tab(s.main.current_build.is_some()));
                            s.main.provider_issue = None;
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
                tf("fmt.internal_panic", &[("msg", s)])
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                tf("fmt.internal_panic", &[("msg", s)])
            } else {
                t("err.opt_panic")
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
                    s.main.optimize_stage = tf(
                        "fmt.ai_thinking",
                        &[
                            ("turn", &turn.to_string()),
                            ("max", &max_turns.to_string()),
                            ("tools", &tools_str),
                        ],
                    );
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
        s.main.optimize_stage = t("status.applying");
    });

    if let Some(first) = suggestions.first_mut() {
        apply_gemini_response(first, &gemini_build);
        // Validator-resolved per-slot prefixes are the authoritative gear data.
        first.slot_prefixes = Some(validated.gear_slots.clone());
        // Run rotation simulation now that LLM has populated skills
        simulate_suggestion_rotation(first, db, balance_ctx);
    }

    Ok(())
}
