use super::optimization::{
    apply_gemini_response, candidate_to_suggestion, humanize_tool_names, result_alert_tab,
    simulate_suggestion_rotation, summarize_resolved_build, synergy_result_to_suggestion,
};
use crate::state::AddonState;
use crate::ui::gear_sheet::piece_gear_slot;
use gw2_core::i18n::{t, tf};
use gw2_core::types::GearSlot;
use gw2_optimizer::balance::BalanceContext;
use gw2_optimizer::scoring::OptimizationWeights;
use gw2_optimizer::ScenarioSpec;

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
    // Current character build for the Improve always-better baseline gate.
    let loadout = state.main.current_build.clone();
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

                // ═══ Improve always-better baseline (spec §12.4): rank the
                // user's OWN current gear under this run's weights so a worse
                // optimizer result is refused. New Build has no baseline.
                let improve_baseline = capture_improve_baseline(
                    loadout.as_ref(),
                    &db,
                    &profession_name,
                    &weights,
                    &balance_ctx,
                    &scenario,
                );

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
                            let (served, serves_baseline) = apply_improve_baseline_gate(
                                synergy_result,
                                improve_baseline.as_ref(),
                                &db,
                                &profession_name,
                                &weights,
                                &balance_ctx,
                                &scenario,
                            );
                            // Serving the baseline means their build, not an improvement.
                            let label_override = if serves_baseline {
                                None
                            } else {
                                locked_spec_name
                                    .as_ref()
                                    .map(|n| tf("fmt.improved", &[("name", n)]))
                            };
                            let suggestion = synergy_result_to_suggestion(
                                &served,
                                &db,
                                &profession_name,
                                &scenario,
                                selected_role,
                                label_override,
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
                            let (served, serves_baseline) = apply_improve_baseline_gate(
                                synergy_result,
                                improve_baseline.as_ref(),
                                &db,
                                &profession_name,
                                &weights,
                                &balance_ctx,
                                &scenario,
                            );
                            // Serving the baseline means their build, not an improvement.
                            let label_override = if serves_baseline {
                                None
                            } else {
                                locked_spec_name
                                    .as_ref()
                                    .map(|n| tf("fmt.improved", &[("name", n)]))
                            };
                            let suggestion = synergy_result_to_suggestion(
                                &served,
                                &db,
                                &profession_name,
                                &scenario,
                                selected_role,
                                label_override,
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

// ────────────────────────────────────────────────────────────────────────────
// Improve always-better baseline gate (spec §12.4: Improve locks existing gear)
// ────────────────────────────────────────────────────────────────────────────

/// The user's own current build, once evaluated under this run's weights.
struct ImproveBaseline {
    validated: gw2_optimizer::validation::ValidatedBuild,
    report: gw2_optimizer::referee::RefereeReport,
}

/// Message shown when the optimizer could not beat the user's current gear.
const BASELINE_KEPT_REASON: &str =
    "Your current gear already outperforms every candidate for these weights — kept your build.";

/// Capture and rank the user's current build for the Improve entry point.
/// `None` without a loadout (New Build is ungated) or when the resolved names
/// do not survive validation (`errors` non-empty → no comparable baseline).
fn capture_improve_baseline(
    loadout: Option<&gw2_core::types::ResolvedBuild>,
    db: &gw2_optimizer::gamedb::GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &ScenarioSpec,
) -> Option<ImproveBaseline> {
    let plate = baseline_plate_from_loadout(loadout?);
    let validated = gw2_optimizer::validation::validate_gemini_build(&plate, db, profession_name);
    if !validated.errors.is_empty() {
        return None;
    }
    let report = gw2_optimizer::referee::evaluate_validated_build(
        &validated,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
    );
    Some(ImproveBaseline { validated, report })
}

/// Serve-time gate: whichever result outranks wins lexicographically; equality
/// keeps the user's gear (no churn without a measurable win). Returns the
/// SynergyResult to serve plus whether that is the user's own baseline.
#[allow(clippy::too_many_arguments)]
fn apply_improve_baseline_gate(
    result: gw2_optimizer::engine::SynergyResult,
    baseline: Option<&ImproveBaseline>,
    db: &gw2_optimizer::gamedb::GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &ScenarioSpec,
) -> (gw2_optimizer::engine::SynergyResult, bool) {
    let Some(baseline) = baseline else {
        return (result, false);
    };
    let result_report = gw2_optimizer::referee::evaluate_validated_build(
        &result.validated,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
    );
    if beats_baseline(
        &gw2_optimizer::referee::search_rank(&result_report),
        &gw2_optimizer::referee::search_rank(&baseline.report),
    ) {
        let mut result = result;
        result
            .quality_reasons
            .push(gw2_optimizer::data::quality::DataQualityReason {
                field: "improve.baseline".into(),
                entity: profession_name.into(),
                modes: vec![ctx.game_mode.label().to_string()],
                explanation: improve_quality_reason(
                    result_report.user_intent_score,
                    baseline.report.user_intent_score,
                ),
            });
        (result, false)
    } else {
        nexus::log::log(
            nexus::log::LogLevel::Info,
            "GW2BuildOpt",
            "Improve baseline gate: optimizer did not outrank the user's current gear; serving their build",
        );
        let mut kept = gw2_optimizer::engine::synergy_result_from_validated(
            baseline.validated.clone(),
            db,
            profession_name,
            ctx,
            Some(scenario),
        );
        kept.quality_reasons
            .push(gw2_optimizer::data::quality::DataQualityReason {
                field: "improve.baseline".into(),
                entity: profession_name.into(),
                modes: vec![ctx.game_mode.label().to_string()],
                explanation: BASELINE_KEPT_REASON.to_string(),
            });
        (kept, true)
    }
}

/// Lexicographic strictly-greater comparison of referee ranks. Equal ranks
/// mean the optimizer matched but did not beat the current gear.
fn beats_baseline(result_rank: &[i64; 9], baseline_rank: &[i64; 9]) -> bool {
    result_rank > baseline_rank
}

/// Quality-reason text for a result that beat the current gear.
fn improve_quality_reason(result_intent: f64, baseline_intent: f64) -> String {
    if baseline_intent > 0.0 && result_intent >= 0.0 {
        format!(
            "Improves on your current gear (intent {:+.0}% vs your current build)",
            (result_intent - baseline_intent) / baseline_intent * 100.0
        )
    } else {
        // Baseline carried no viable intent signal — skip the percentage.
        "Improves on your current gear".into()
    }
}

/// Encode the user's CURRENT character into a Gemini plate so
/// [`gw2_optimizer::validation::validate_gemini_build`] can resolve it back
/// into a validated build — the same converter Chat plates go through.
/// Weapon sets follow `lock_panel::resolved_gear_names` indexing (set 0 →
/// Set 1 slots, the rest → Set 2); pieces use the shared `piece_gear_slot`
/// mapping. PvP has no gear pieces, so its amulet surfaces via `stat_prefix`.
fn baseline_plate_from_loadout(
    loadout: &gw2_core::types::ResolvedBuild,
) -> gw2_optimizer::prompts::GeminiBuildResponse {
    let set_label = |index: usize| if index == 0 { 1 } else { 2 };

    let specializations: Vec<(String, Vec<String>)> = loadout
        .specializations
        .iter()
        .map(|spec| (spec.name.clone(), selected_trait_names(spec)))
        .collect();

    // "Set N: Main / Off" strings — the validator re-parses this shape.
    let mut weapons = Vec::new();
    for (index, set) in loadout.weapons.iter().enumerate() {
        let Some(main_hand) = set.main_hand.as_ref().map(|w| w.name.clone()) else {
            continue;
        };
        match set.off_hand.as_ref().map(|w| w.name.as_str()) {
            Some(off_hand) => weapons.push(format!(
                "Set {}: {} / {}",
                set_label(index),
                main_hand,
                off_hand
            )),
            None => weapons.push(format!("Set {}: {}", set_label(index), main_hand)),
        }
    }

    let mut skills = Vec::new();
    if let Some(skill) = &loadout.skills.heal {
        skills.push(format!("Heal: {}", skill.name));
    }
    for skill in loadout.skills.utilities.iter().flatten() {
        skills.push(format!("Utility: {}", skill.name));
    }
    if let Some(skill) = &loadout.skills.elite {
        skills.push(format!("Elite: {}", skill.name));
    }

    // Sigils per position: [set1_main, set1_off, set2_main, set2_off].
    let mut sigils_map = std::collections::HashMap::new();
    for (index, set) in loadout.weapons.iter().enumerate() {
        let keys = [
            format!("set{}_main", set_label(index)),
            format!("set{}_off", set_label(index)),
        ];
        for (key, sigil) in keys.iter().zip(set.sigils.iter()) {
            sigils_map.insert(key.clone(), sigil.name.clone());
        }
    }

    // Per-piece prefixes over armor, trinkets, and both weapon sets.
    let mut gear_slots = std::collections::HashMap::new();
    for piece in loadout.armor.iter().chain(loadout.trinkets.iter()) {
        let prefix = piece.stat_prefix.trim();
        if prefix.is_empty() {
            continue;
        }
        if let Some(slot) = piece_gear_slot(&piece.slot) {
            gear_slots.insert(slot.kebab_name().to_string(), prefix.to_string());
        }
    }
    for (index, set) in loadout.weapons.iter().enumerate() {
        let prefix = set.stat_prefix.trim();
        if prefix.is_empty() || (set.main_hand.is_none() && set.off_hand.is_none()) {
            continue;
        }
        let prefix = prefix.to_string();
        let (main_slot, off_slot) = if index == 0 {
            (GearSlot::WeaponSet1Main, GearSlot::WeaponSet1Off)
        } else {
            (GearSlot::WeaponSet2Main, GearSlot::WeaponSet2Off)
        };
        if set.main_hand.is_some() {
            gear_slots.insert(main_slot.kebab_name().to_string(), prefix.clone());
        }
        if set.off_hand.is_some() {
            gear_slots.insert(off_slot.kebab_name().to_string(), prefix.clone());
        }
    }

    // Primary weapon-set prefix first, then any piece, then PvP amulet stem.
    let stat_prefix = loadout
        .weapons
        .iter()
        .map(|s| s.stat_prefix.as_str())
        .chain(loadout.armor.iter().map(|p| p.stat_prefix.as_str()))
        .chain(loadout.trinkets.iter().map(|p| p.stat_prefix.as_str()))
        .map(str::trim)
        .find(|prefix| !prefix.is_empty())
        .map(str::to_string)
        .or_else(|| {
            loadout
                .pvp_amulet
                .as_ref()
                .map(|a| a.name.trim_end_matches(" Amulet").trim().to_string())
        });

    gw2_optimizer::prompts::GeminiBuildResponse {
        specializations,
        weapons,
        skills,
        rune: loadout
            .rune
            .as_ref()
            .map(|r| r.name.clone())
            .unwrap_or_default(),
        relic: loadout
            .relic
            .as_ref()
            .map(|r| r.name.clone())
            .unwrap_or_default(),
        stat_prefix: stat_prefix.unwrap_or_default(),
        sigils_map: Some(sigils_map),
        gear_slots: Some(gear_slots),
        ..gw2_optimizer::prompts::GeminiBuildResponse::default()
    }
}

/// The three selected major traits per column, falling back to any column
/// option marked selected — same sourcing as Chat plates.
fn selected_trait_names(spec: &gw2_core::types::ResolvedSpec) -> Vec<String> {
    let mut traits: Vec<(usize, String)> = spec
        .traits_selected
        .iter()
        .filter(|trait_| trait_.selected && trait_.column < 3)
        .map(|trait_| (trait_.column, trait_.name.clone()))
        .collect();
    for (column, options) in spec.traits_available.iter().enumerate() {
        if traits.iter().any(|(c, _)| *c == column) {
            continue;
        }
        if let Some(option) = options.iter().find(|o| o.selected) {
            traits.push((column, option.name.clone()));
        }
    }
    traits.sort_by_key(|(column, _)| *column);
    traits.into_iter().take(3).map(|(_, name)| name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_rank_keeps_user_gear() {
        let rank = [
            1i64, 6, 900_000, 400_000, 500_000, 200_000, 700, 1_000_000, 10,
        ];
        assert!(!beats_baseline(&rank, &rank));
    }

    #[test]
    fn lower_rank_keeps_user_gear() {
        // Losing on an early key is decisive regardless of later dominance.
        assert!(!beats_baseline(
            &[1, 5, 999_999, 999_999, 999_999, 999_999, 999_999, 999_999, 999],
            &[1, 6, 0, 0, 0, 0, 0, 0, 0]
        ));
    }

    #[test]
    fn later_key_breaks_leading_tie() {
        // WvW-shaped ranks: intent tied, raw direction decides.
        assert!(beats_baseline(
            &[1, 6, 1, 1, 500_000, 250_000, 700, 1_000_000, 10],
            &[1, 6, 1, 1, 500_000, 240_000, 700, 1_000_000, 9]
        ));
    }

    #[test]
    fn earlier_key_wins_over_bigger_later_key() {
        assert!(beats_baseline(
            &[1, 6, 300_000, -50, 0, 100, 0, 0, -50],
            &[1, 6, 299_000, 0, 0, 999_999, 0, 0, 999]
        ));
    }

    #[test]
    fn improve_reason_reports_percent_delta() {
        assert_eq!(
            improve_quality_reason(0.12, 0.10),
            "Improves on your current gear (intent +20% vs your current build)"
        );
    }

    #[test]
    fn improve_reason_skips_percentage_without_baseline_signal() {
        assert_eq!(
            improve_quality_reason(-1.0, -1.0),
            "Improves on your current gear"
        );
    }

    #[test]
    fn plate_encodes_loadout_for_validation() {
        use gw2_core::types::{
            ResolvedGearPiece, ResolvedSkills, ResolvedSpec, ResolvedTrait, ResolvedUpgrade,
            ResolvedWeaponSet, SkillInfo, TraitOption, UpgradeInfo, WeaponInfo,
        };

        let mut spec = ResolvedSpec {
            id: 52,
            name: "Firebrand".into(),
            elite: true,
            ..Default::default()
        };
        spec.traits_selected = vec![
            ResolvedTrait {
                id: 11,
                name: "Stalwart Might".into(),
                column: 1,
                selected: true,
                ..Default::default()
            },
            ResolvedTrait {
                id: 10,
                name: "Unbroken Lines".into(),
                column: 0,
                selected: true,
                ..Default::default()
            },
        ];
        spec.traits_available = vec![
            Vec::new(),
            Vec::new(),
            vec![TraitOption {
                id: 12,
                name: "Tome of Courage".into(),
                selected: true,
            }],
        ];

        let loadout = gw2_core::types::ResolvedBuild {
            specializations: vec![spec],
            skills: ResolvedSkills {
                heal: Some(SkillInfo {
                    id: 1,
                    name: "Mantra of Flame".into(),
                }),
                utilities: vec![Some(SkillInfo {
                    id: 2,
                    name: "Purging Flames".into(),
                })],
                elite: None,
            },
            weapons: vec![ResolvedWeaponSet {
                label: "Set 1".into(),
                stat_prefix: "Berserker's".into(),
                main_hand: Some(WeaponInfo {
                    name: "Greatsword".into(),
                    ..Default::default()
                }),
                off_hand: None,
                sigils: vec![UpgradeInfo {
                    id: 7,
                    name: "Superior Sigil of Air".into(),
                }],
            }],
            armor: vec![ResolvedGearPiece {
                slot: "Helm".into(),
                stat_prefix: "Valkyrie".into(),
                ..Default::default()
            }],
            trinkets: vec![
                ResolvedGearPiece {
                    slot: "Amulet".into(),
                    stat_prefix: "Marauder's".into(),
                    ..Default::default()
                },
                ResolvedGearPiece {
                    slot: "Ring1".into(),
                    stat_prefix: String::new(),
                    ..Default::default()
                },
            ],
            rune: Some(ResolvedUpgrade {
                id: 3,
                name: "Superior Rune of the Thief".into(),
            }),
            ..Default::default()
        };

        let plate = baseline_plate_from_loadout(&loadout);

        // Traits sorted by column with the availability fallback applied.
        assert_eq!(
            plate.specializations[0].0, "Firebrand",
            "spec display name must pass through unchanged"
        );
        assert_eq!(
            plate.specializations[0].1,
            vec!["Unbroken Lines", "Stalwart Might", "Tome of Courage"]
        );
        assert_eq!(plate.weapons, vec!["Set 1: Greatsword"]);
        assert_eq!(
            plate.skills,
            vec!["Heal: Mantra of Flame", "Utility: Purging Flames"]
        );
        assert_eq!(plate.rune, "Superior Rune of the Thief");
        assert_eq!(plate.relic, "");
        assert_eq!(plate.stat_prefix, "Berserker's");

        let gear = plate.gear_slots.expect("per-slot map present");
        assert_eq!(gear.get("helm").map(String::as_str), Some("Valkyrie"));
        assert_eq!(gear.get("amulet").map(String::as_str), Some("Marauder's"));
        assert_eq!(
            gear.get("weapon-set-1-main").map(String::as_str),
            Some("Berserker's")
        );
        assert!(
            !gear.contains_key("ring-1"),
            "empty prefix pieces are skipped"
        );
        assert!(!gear.contains_key("weapon-set-1-off"));

        let sigils = plate.sigils_map.expect("sigil position map present");
        assert_eq!(
            sigils.get("set1_main").map(String::as_str),
            Some("Superior Sigil of Air")
        );
    }

    #[test]
    fn pvp_amulet_seeds_stat_prefix_when_no_pieces() {
        let loadout = gw2_core::types::ResolvedBuild {
            pvp_amulet: Some(gw2_core::types::ResolvedPvpAmulet {
                name: "Berserker Amulet".into(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let plate = baseline_plate_from_loadout(&loadout);

        assert_eq!(plate.stat_prefix, "Berserker");
        assert!(plate.specializations.is_empty());
        assert_eq!(plate.weapons, Vec::<String>::new());
        assert!(!plate.gear_slots.unwrap().contains_key("amulet"));
    }
}
