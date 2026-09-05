use super::optimization::{
    apply_gemini_response, apply_radar_prefix, attach_chat_stats, chat_display_text,
    fill_holes_from_loadout, format_provider_issue, gemini_from_validated, humanize_tool_names,
    keep_equipped_weapons, keep_loadout_pets, kitchen_brief, result_alert_tab,
    simulate_suggestion_rotation, suggestion_to_chat_code, summarize_resolved_build,
    summarize_suggestion, validated_build_to_chat_code,
};
use std::sync::Arc;

use crate::state::AddonState;
use gw2_core::i18n::{t, tf};
use gw2_optimizer::balance::BalanceContext;
use gw2_optimizer::gamedb::GameDb;

/// Hand a background worker its own reference to the shared game database.
///
/// `GameDb` is loaded once and can be tens of megabytes; a chat worker needs
/// it for the lifetime of its (possibly multi-turn) LLM call, so the addon
/// shares one instance via `Arc` (see `AddonState::game_db`'s doc comment)
/// instead of copying it per spawn. Routing every clone through this one
/// named function — rather than trusting each `.clone()` call site — gives
/// the "never a deep copy" invariant a single place to hold and test.
fn clone_game_db_for_worker(db: &Option<Arc<GameDb>>) -> Option<Arc<GameDb>> {
    db.clone()
}

/// Send a chat order to the chef (active LLM) for a plated build.
/// Uses function calling so the chef has the full pantry and every station.
pub(super) fn send_chat_message(state: &mut AddonState, message: String) {
    let (display, inbound_chips, _chef_order) =
        crate::chat_links::annotate_order(&message, state.main.game_db.as_deref());
    crate::ui::chat_bar::attach_order_chips(
        &mut state.main.chat,
        display.clone(),
        inbound_chips.clone(),
    );

    if !state.config.has_active_llm_key() {
        crate::ui::chat_bar::add_ai_response(&mut state.main.chat, t("choya.need_key"));
        return;
    }
    if state.main.optimizing {
        crate::ui::chat_bar::add_ai_response(&mut state.main.chat, t("choya.optimize_running"));
        return;
    }
    if state.main.game_db.is_none() {
        crate::ui::chat_bar::add_ai_response(&mut state.main.chat, t("choya.data_loading"));
        return;
    }

    let mut profession = state
        .main
        .current_build
        .as_ref()
        .map(|b| b.profession.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    if let Some(db) = state.main.game_db.as_ref() {
        if db.profession(&profession).is_none() {
            if let Some(inferred) =
                gw2_optimizer::validation::infer_profession_from_text(db, &display)
            {
                profession = inferred;
            }
        }
    }

    state.main.chat_epoch = state.main.chat_epoch.wrapping_add(1);
    let epoch = state.main.chat_epoch;
    state.main.chat.waiting = true;
    state.main.provider_issue = None;
    state.main.chat_wait_started = Some(std::time::Instant::now());
    state.main.optimize_stage = t("choya.thinking");

    let config = state.config.clone();
    let character = state
        .main
        .current_build
        .as_ref()
        .map(summarize_resolved_build)
        .unwrap_or_default();
    let game_mode_label = state.main.game_mode.label().to_string();
    let scale = if state.main.game_mode == gw2_core::types::GameMode::WvW {
        state.main.wvw_combat_tier.label()
    } else {
        "n/a"
    };
    let role_label = state
        .main
        .selected_role
        .map(|r| r.play_label())
        .unwrap_or("unspecified");
    let role_brief = state
        .main
        .selected_role
        .map(|r| r.family_brief(&state.main.game_mode, state.main.wvw_combat_tier))
        .unwrap_or("No role chip. Infer the job from the player's words.");
    let keep_weapons = keep_equipped_weapons(&display);
    let on_the_pass = state
        .main
        .comparison
        .suggestions
        .get(state.main.comparison.selected_suggestion)
        .map(summarize_suggestion)
        .unwrap_or_else(|| "(none yet — talk or run Optimize first)".into());
    let mut kitchen = kitchen_brief(
        &game_mode_label,
        scale,
        role_label,
        role_brief,
        &character,
        &on_the_pass,
        keep_weapons,
    );
    if !inbound_chips.is_empty() {
        kitchen.push_str("\nPasted: ");
        kitchen.push_str(
            &inbound_chips
                .iter()
                .map(|c| format!("{} ({})", c.label, c.kind.as_str()))
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
    let transcript = crate::ui::chat_bar::recent_transcript(&state.main.chat.history, 8);
    if !transcript.is_empty() {
        kitchen.push_str("\nRecent chat:\n");
        kitchen.push_str(&transcript);
    }
    let message = display;
    let addon_dir = state.addon_dir.clone();
    let db_clone = clone_game_db_for_worker(&state.main.game_db);
    let weights = state.main.weights.clone();
    let loadout = state.main.current_build.clone();
    // The plate is ranked before it is served, so the chat path needs the same
    // scenario the Improve button builds — same tier mapping, same role
    // profile. Without one there is nothing for the referee to judge against.
    let combat_tier = match state.main.game_mode {
        gw2_core::types::GameMode::WvW => state.main.wvw_combat_tier,
        gw2_core::types::GameMode::PvP => gw2_optimizer::scenario::CombatTier::Solo,
        gw2_core::types::GameMode::PvE => gw2_optimizer::scenario::CombatTier::Party,
    };
    let selected_role = state.main.selected_role;
    let chat_balance_ctx = BalanceContext::new(state.main.game_mode.clone());

    let spawned = state.spawn_worker("chat-message", move |token| {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if token.is_cancelled() {
                crate::state::with_state(|s| {
                    if s.main.chat_epoch == epoch {
                        s.main.chat.waiting = false;
                    }
                });
                return;
            }

            let result = (|| -> Result<gw2_optimizer::prompts::GeminiBuildResponse, String> {
                let client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                    .map_err(|e| e.to_string())?;

                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                if let Some(ref db) = db_clone {
                    let tools = gw2_optimizer::llm::tools::tool_definitions();
                    let empty_candidates = vec![];
                    let ctx = gw2_optimizer::gemini_tools::ToolContext {
                        db,
                        profession_name: &profession,
                        candidates: &empty_candidates,
                        current_build_summary: Some(kitchen.as_str()),
                        weights: weights.clone(),
                        balance_ctx: &chat_balance_ctx,
                    };
                    let scenario = gw2_optimizer::scenario::ScenarioSpec {
                        game_mode: chat_balance_ctx.game_mode.clone(),
                        combat_tier,
                        combat_kind: selected_role
                            .map(|r| r.combat_kind_for_weights(&weights))
                            .unwrap_or_else(|| {
                                if weights.condition > weights.power {
                                    gw2_optimizer::scenario::CombatKind::CondiRamp
                                } else {
                                    gw2_optimizer::scenario::CombatKind::StrikeSpike
                                }
                            }),
                        target_profile: gw2_optimizer::scenario::TargetProfile::Single,
                        optimization_target: gw2_optimizer::scenario::OptimizationTarget {
                            label: game_mode_label.clone(),
                        },
                        patch_id: Some(chat_balance_ctx.patch_id.clone()),
                        objective_profile_id: selected_role.map(|r| {
                            r.profile_id_for(&chat_balance_ctx.game_mode, combat_tier)
                                .to_string()
                        }),
                    };
                    // What the plate has to beat. Ranked once: the player's
                    // gear does not change while Choya is thinking.
                    let baseline = rank_current_build(
                        loadout.as_ref(),
                        db,
                        &profession,
                        &weights,
                        &chat_balance_ctx,
                        &scenario,
                    );

                    // Two attempts, not one: a rejected plate is told exactly
                    // which check it failed and gets to compose again. One
                    // attempt would only ever refuse; unlimited attempts would
                    // burn the player's tokens on a model that cannot get there.
                    let mut feedback: Option<String> = None;
                    let mut rejected: Option<String> = None;
                    for attempt in 1..=2u32 {
                        if token.is_cancelled() {
                            return Err("Cancelled".into());
                        }
                        let mut prompt =
                            gw2_optimizer::prompts::chat_refinement_prompt_with_tools(
                                &profession,
                                &game_mode_label,
                                &message,
                                &kitchen,
                                gw2_core::i18n::choya_name_for(&config.ui_language),
                            );
                        if let Some(ref why) = feedback {
                            prompt.push_str(&format!(
                                "\n\nYOUR PREVIOUS PLATE WAS REFUSED: {why}\n\
                                 Compose a different build that fixes exactly that, \
                                 in the same JSON shape. Do not repeat the refused one."
                            ));
                            // A second whole LLM call is another 10-40s of
                            // silence. Say why it is happening.
                            crate::state::with_state(|s| {
                                if s.main.chat_epoch == epoch {
                                    s.main.optimize_stage =
                                        "That plate did not beat your build. Trying again..."
                                            .to_string();
                                }
                            });
                        }

                        // Always with tools, equipped loadout or not. The
                        // prompt tells the model "an equipped Character
                        // loadout is your STARTING POINT, not a licence to
                        // skip the tools - you must still call get_spec_traits
                        // for every specialization you keep or change". An
                        // earlier pass removed the opposite instruction from
                        // the prompt but left this branch handing that same
                        // request zero tool declarations, so a model that
                        // obeyed had nothing to call. Measured in-game
                        // 2026-09-05 on every Google model tried:
                        // gemini-3.8-flash and gemini-flash-latest answered
                        // MALFORMED_FUNCTION_CALL, gemini-3.7-flash emitted a
                        // call with no text ("No response text"), while
                        // glm-5.3-flash - which had taken the tools branch -
                        // worked. The contradiction was the bug, not the model.
                        let response = {
                            client
                                .generate_with_tools_progress(
                                    &prompt,
                                    &tools,
                                    &mut |name: &str, args: &serde_json::Value| {
                                        let stale =
                                            crate::state::with_state(|s| s.main.chat_epoch != epoch)
                                                .unwrap_or(true);
                                        if stale {
                                            return serde_json::json!({"error": "cancelled"});
                                        }
                                        gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx)
                                    },
                                    // Same budget the Optimize advisor gets.
                                    // Composing a build is get_current_build,
                                    // then get_spec_traits for each of three
                                    // specializations, then runes/sigils/relic
                                    // - past three rounds before it can answer.
                                    // The prompt already says "take as many
                                    // tool rounds as the build needs"; three
                                    // was the number contradicting it, and
                                    // gemini-flash-latest ran out on every
                                    // request (measured in-game 2026-09-05).
                                    8,
                                    &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                                        let tools_str = humanize_tool_names(tool_names);
                                        crate::state::with_state(|s| {
                                            if s.main.chat_epoch != epoch {
                                                return;
                                            }
                                            // No tools this round means the
                                            // loop is closing and the model is
                                            // writing the build. That request
                                            // is the longest one of the run
                                            // (90s measured in-game), so the
                                            // counter must stop reading (8/8)
                                            // and looking, or it reads as a
                                            // hang. Reuses the existing
                                            // translated key rather than
                                            // adding a thirteenth string.
                                            if tool_names.is_empty() {
                                                s.main.optimize_stage = t("choya.thinking");
                                                return;
                                            }
                                            s.main.optimize_stage = tf(
                                                "fmt.choya_looking",
                                                &[
                                                    ("turn", &turn.to_string()),
                                                    ("max", &max_turns.to_string()),
                                                    ("tools", &tools_str),
                                                ],
                                            );
                                        });
                                    },
                                )
                                .map_err(|e| e.to_string())?
                        };

                        let mut parsed = match gw2_optimizer::prompts::parse_gemini_build(&response)
                        {
                            Ok(p) => p,
                            Err(_) => {
                                let explanation: String =
                                    response.chars().filter(|c| *c != '`').take(800).collect();
                                let explanation = explanation.trim().to_string();
                                if explanation.is_empty() {
                                    return Err("Empty reply".into());
                                }
                                gw2_optimizer::prompts::GeminiBuildResponse {
                                    explanation,
                                    ..Default::default()
                                }
                            }
                        };
                        if let Some(ref cur) = loadout {
                            fill_holes_from_loadout(&mut parsed, cur);
                        }
                        apply_radar_prefix(&mut parsed, &weights, &message);

                        // A reply with no complete kit is conversation, not a
                        // build. Nothing to rank, nothing to refuse.
                        let mut plate_profession = profession.clone();
                        if let Some(inferred) =
                            gw2_optimizer::validation::infer_profession_from_spec_names(
                                db,
                                parsed.specializations.iter().map(|(n, _)| n.as_str()),
                            )
                        {
                            plate_profession = inferred;
                        }
                        let validated = gw2_optimizer::validation::validate_gemini_build(
                            &parsed,
                            db,
                            &plate_profession,
                        );
                        if !plate_is_servable(&validated) {
                            return Ok(parsed);
                        }

                        match plate_shortfall(
                            &validated,
                            baseline.as_ref(),
                            db,
                            &plate_profession,
                            &weights,
                            &chat_balance_ctx,
                            &scenario,
                        ) {
                            Ok(()) => return Ok(parsed),
                            Err(why) => {
                                nexus::log::log(
                                    nexus::log::LogLevel::Info,
                                    "GW2BuildOpt",
                                    format!("Choya plate refused (attempt {attempt}): {why}"),
                                );
                                feedback = Some(why.clone());
                                rejected = Some(why);
                            }
                        }
                    }

                    // Both attempts lost to the player's own build. Serving the
                    // second one anyway is the bug this gate exists to stop, so
                    // Choya says what happened and the build stands.
                    // ponytail: plain English like `KEPT_GEAR_HEADLINE` in
                    // optimize_flow; move behind `t("choya.kept")` when
                    // `locales/` is next open.
                    Ok(gw2_optimizer::prompts::GeminiBuildResponse {
                        explanation: format!(
                            "I kept your build - nothing I plated beat it. {}",
                            rejected.unwrap_or_default()
                        ),
                        ..Default::default()
                    })
                } else {
                    Err("Game data not loaded".into())
                }
            })();

            let mut profession = profession;
            if let (Ok(parsed), Some(db)) = (result.as_ref(), db_clone.as_ref()) {
                if let Some(inferred) = gw2_optimizer::validation::infer_profession_from_spec_names(
                    db,
                    parsed.specializations.iter().map(|(n, _)| n.as_str()),
                ) {
                    profession = inferred;
                }
            }

            let validated = result.as_ref().ok().and_then(|gemini_build| {
                db_clone.as_ref().map(|db| {
                    let validated = gw2_optimizer::validation::validate_gemini_build(
                        gemini_build,
                        db,
                        &profession,
                    );
                    if !validated.errors.is_empty() && !gemini_build.specializations.is_empty() {
                        nexus::log::log(
                            nexus::log::LogLevel::Warning,
                            "GW2BuildOpt",
                            format!(
                                "Kitchen validation errors: {} | warnings: {}",
                                validated
                                    .errors
                                    .iter()
                                    .map(|e| e.detail.as_str())
                                    .collect::<Vec<_>>()
                                    .join("; "),
                                // The warnings name the trait/skill the model
                                // actually got wrong; the errors only say a
                                // count came up short. Logging errors alone
                                // made "expected 3 traits, got 2" undiagnosable.
                                validated.warnings.join("; ")
                            ),
                        );
                    }
                    validated
                })
            });

            if !token.is_cancelled() {
                // Clear the stage and drop stale results before any work.
                let stale = crate::state::with_state(|s| {
                    if s.main.chat_epoch != epoch {
                        return true;
                    }
                    s.main.optimize_stage.clear();
                    false
                })
                .unwrap_or(true);

                if !stale {
                    match result {
                        Ok(raw) => {
                            if !validated.as_ref().is_some_and(plate_is_servable) {
                                // Unservable plate: reply with the explanation text.
                                crate::state::with_state(|s| {
                                    if s.main.chat_epoch != epoch {
                                        return;
                                    }
                                    let errors: Vec<String> = validated
                                        .as_ref()
                                        .map(|v| {
                                            v.errors.iter().map(|e| e.detail.clone()).collect()
                                        })
                                        .unwrap_or_default();
                                    crate::ui::chat_bar::add_ai_response(
                                        &mut s.main.chat,
                                        chat_display_text(
                                            &raw.explanation,
                                            raw.specializations.len(),
                                            &errors,
                                        ),
                                    );
                                });
                            } else {
                                // Heavy phase — runs WITHOUT the state lock. The
                                // render callback shares this mutex and ImGui only
                                // draws on the render thread, so a stalled frame
                                // here reads as the whole game not responding.
                                let Some((live_db, live_mode)) = crate::state::with_state(|s| {
                                    (
                                        clone_game_db_for_worker(&s.main.game_db),
                                        s.main.game_mode.clone(),
                                    )
                                }) else {
                                    return; // state gone — addon shutting down
                                };
                                let plated = match &validated {
                                    Some(v) => gemini_from_validated(raw, v),
                                    None => raw,
                                };
                                let errors: Vec<String> = validated
                                    .as_ref()
                                    .map(|v| v.errors.iter().map(|e| e.detail.clone()).collect())
                                    .unwrap_or_default();
                                let body = if plated.explanation.is_empty() {
                                    t("choya.heres_a_build")
                                } else {
                                    plated.explanation.clone()
                                };
                                let display =
                                    chat_display_text(&body, plated.specializations.len(), &errors);
                                let mut suggestion = crate::ui::comparison::BuildSuggestion {
                                    label: t("choya.pick"),
                                    ..Default::default()
                                };
                                apply_gemini_response(&mut suggestion, &plated);
                                // Same as the optimize worker: keep Ranger pets
                                // on the plated suggestion. gemini_from_validated
                                // also keeps the row; this covers a missed rebuild.
                                if let Some(ref cur) = loadout {
                                    keep_loadout_pets(&mut suggestion, &cur.pets);
                                }
                                // Validator-resolved per-slot prefixes are the
                                // authoritative gear data for the sheet/locks.
                                if let Some(v) = &validated {
                                    suggestion.slot_prefixes = Some(v.gear_slots.clone());
                                }
                                if let Some(ref db) = live_db {
                                    attach_chat_stats(&mut suggestion, db, &profession, &live_mode);
                                    if let Some(v) = &validated {
                                        suggestion.chat_code =
                                            validated_build_to_chat_code(v, &profession, db);
                                    }
                                    if suggestion.chat_code.is_none() {
                                        suggestion.chat_code =
                                            suggestion_to_chat_code(&suggestion, db);
                                    }
                                    let balance_ctx = BalanceContext::new(live_mode.clone());
                                    simulate_suggestion_rotation(&mut suggestion, db, &balance_ctx);
                                }
                                let chips = match (live_db.as_ref(), validated.as_ref()) {
                                    (Some(db), Some(v)) => crate::chat_links::chips_from_plate(
                                        db,
                                        v,
                                        suggestion.chat_code.as_deref(),
                                    ),
                                    _ => suggestion
                                        .chat_code
                                        .as_deref()
                                        .filter(|c| c.starts_with("[&"))
                                        .map(|c| vec![crate::chat_links::build_template_chip(c)])
                                        .unwrap_or_default(),
                                };
                                // Apply phase — short lock, pure state mutation.
                                crate::state::with_state(|s| {
                                    if s.main.chat_epoch != epoch {
                                        return;
                                    }
                                    crate::ui::chat_bar::add_plated_response(
                                        &mut s.main.chat,
                                        display,
                                        chips,
                                        true,
                                    );
                                    s.main.comparison.error = None;
                                    s.main.comparison.suggestions.push(suggestion);
                                    s.main.comparison.selected_suggestion =
                                        s.main.comparison.suggestions.len() - 1;
                                    s.main.comparison.show_optimized = true;
                                    s.main.tab_alert =
                                        Some(result_alert_tab(s.main.current_build.is_some()));
                                    s.main.provider_issue = None;
                                });
                            }
                        }
                        Err(e) => {
                            crate::state::with_state(|s| {
                                if s.main.chat_epoch != epoch {
                                    return;
                                }
                                let msg = format_provider_issue(
                                    &e,
                                    s.config.active_provider.short_label(),
                                    s.config.active_model_id(),
                                );
                                s.main.provider_issue = Some(msg.clone());
                                crate::ui::chat_bar::add_ai_response(&mut s.main.chat, msg);
                            });
                        }
                    }
                }
            } else {
                crate::state::with_state(|s| {
                    if s.main.chat_epoch == epoch {
                        s.main.chat.waiting = false;
                    }
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
                if s.main.chat_epoch == epoch {
                    s.main.chat.waiting = false;
                }
            });
        }
    });
    if !spawned {
        // The OS refused the thread (`spawn_worker` logged it): nothing ran,
        // so nothing else will clear the "thinking" spinner this function
        // turned on above.
        state.main.chat.waiting = false;
        state.main.optimize_stage.clear();
    }
}

/// The player's own build, ranked, for the plate to beat.
///
/// `None` when there is nothing to compare against (no resolved character, or
/// gear the validator cannot resolve). A missing baseline disables the gate
/// rather than blocking the answer — the same choice the Improve tab makes.
fn rank_current_build(
    loadout: Option<&gw2_core::types::ResolvedBuild>,
    db: &GameDb,
    profession_name: &str,
    weights: &gw2_optimizer::scoring::OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &gw2_optimizer::scenario::ScenarioSpec,
) -> Option<gw2_optimizer::referee::RefereeReport> {
    let plate = super::optimize_flow::baseline_plate_from_loadout(loadout?);
    let validated = gw2_optimizer::validation::validate_gemini_build(&plate, db, profession_name);
    if !validated.errors.is_empty() {
        return None;
    }
    Some(gw2_optimizer::referee::evaluate_validated_build(
        &validated,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
    ))
}

/// Why a plate is not fit to serve, phrased for the model to act on.
///
/// `Ok(())` means serve it. The chat path used to have no such check at all:
/// [`plate_is_servable`] asks only whether three specializations carry three
/// traits each, so any structurally complete answer was plated as "Choya's
/// pick" no matter what it did to the player's build. Measured in-game
/// 2026-09-05 (Guardian, WvW Roam, Bruiser): the served plate lost 207 Power,
/// 280 Ferocity, 311 Condition Damage and 157 Healing Power to gain 81
/// Vitality, and carried zero condition cleanse in a game mode whose own
/// viability gate demands at least one — because neither the referee nor the
/// always-better baseline ever ran on this path.
fn plate_shortfall(
    plate: &gw2_optimizer::validation::ValidatedBuild,
    baseline: Option<&gw2_optimizer::referee::RefereeReport>,
    db: &GameDb,
    profession_name: &str,
    weights: &gw2_optimizer::scoring::OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &gw2_optimizer::scenario::ScenarioSpec,
) -> Result<(), String> {
    let report = gw2_optimizer::referee::evaluate_validated_build(
        plate,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
    );
    if !report.viability.is_viable {
        let failed: Vec<String> = report
            .viability
            .gates
            .iter()
            .filter(|g| !g.passed)
            .map(|g| format!("{:?} ({})", g.gate, g.note))
            .collect();
        return Err(format!(
            "that build is not viable in this game mode. Failed checks: {}",
            failed.join("; ")
        ));
    }
    // No baseline is not a pass mark, it is an unarmed gate: viability alone
    // still had to hold above.
    let Some(baseline) = baseline else {
        return Ok(());
    };
    if super::optimize_flow::beats_baseline(
        &gw2_optimizer::referee::search_rank(&report),
        &gw2_optimizer::referee::search_rank(baseline),
    ) {
        return Ok(());
    }
    Err(format!(
        "that build does not beat what the player is already wearing \
         (their build scores {:.0}, yours {:.0} on the same weights). \
         Keep what already works and change only what you can argue is better.",
        baseline.user_intent_score, report.user_intent_score,
    ))
}

pub(super) fn plate_is_servable(v: &gw2_optimizer::validation::ValidatedBuild) -> bool {
    // Weapon/prefix typos stay as warnings in the bubble. A complete kit still plates.
    v.specializations.len() == 3
        && v.specializations.iter().all(|s| s.trait_ids.len() == 3)
        && v.skills.heal.is_some()
        && v.skills.elite.is_some()
        && v.skills.utilities.iter().filter(|u| u.is_some()).count() == 3
}

#[cfg(test)]
mod tests {
    use super::plate_is_servable;

    #[test]
    fn plate_is_servable_needs_full_bar() {
        let mut v = gw2_optimizer::validation::ValidatedBuild::default();
        assert!(!plate_is_servable(&v));
        let spec = |id, name: &str| gw2_optimizer::validation::ValidatedSpec {
            spec_id: id,
            name: name.into(),
            elite: id == 3,
            trait_ids: vec![id, id + 1, id + 2],
            trait_names: vec!["a".into(), "b".into(), "c".into()],
            all_trait_ids: vec![id, id + 1, id + 2],
        };
        v.specializations = vec![spec(1, "Water"), spec(2, "Arcane"), spec(3, "Tempest")];
        v.skills.heal = Some((1, "H".into()));
        v.skills.elite = Some((9, "E".into()));
        v.skills.utilities = vec![
            Some((2, "U1".into())),
            Some((3, "U2".into())),
            Some((4, "U3".into())),
        ];
        assert!(plate_is_servable(&v));
        v.skills.utilities.pop();
        assert!(!plate_is_servable(&v));
        v.skills.utilities.push(Some((4, "U3".into())));
        assert!(plate_is_servable(&v));
        v.errors.push(gw2_optimizer::validation::ValidationReject {
            code: gw2_optimizer::validation::RejectCode::WeaponNotAvailable {
                slot: "Set 2".into(),
                weapon: "Short Bow".into(),
                profession: "Thief".into(),
            },
            detail: "Set 2: weapon 'Short Bow' not available for Thief".into(),
        });
        assert!(
            plate_is_servable(&v),
            "leftover weapon typos must not hide a complete kit"
        );
        v.specializations[1].trait_ids.pop();
        assert!(!plate_is_servable(&v));
    }

    #[test]
    fn chat_plated_path_keeps_loadout_pets() {
        // A18-4: servable chat must call keep_loadout_pets after plating,
        // same as the optimize worker. gemini_from_validated keeps the row;
        // this is the belt if the rebuild still misses.
        let src = include_str!("chat_flow.rs");
        let production = src
            .split("\n#[cfg(test)]")
            .next()
            .expect("split always yields a first chunk");
        let apply_at = production
            .find("apply_gemini_response(&mut suggestion")
            .expect("apply_gemini_response gone");
        let keep_at = production
            .find("keep_loadout_pets(&mut suggestion")
            .expect("chat plated path must call keep_loadout_pets");
        assert!(
            keep_at > apply_at,
            "keep_loadout_pets must run after apply_gemini_response"
        );
        assert!(
            production.contains("gemini_from_validated"),
            "chat still plates through gemini_from_validated"
        );
    }
}

#[cfg(test)]
mod arc_gamedb_tests {
    use super::clone_game_db_for_worker;
    use gw2_optimizer::gamedb::GameDb;
    use std::sync::Arc;

    /// C23: the chat worker must clone the `Arc<GameDb>` handle, never the
    /// `GameDb` itself. Proven independently of `clone_game_db_for_worker`'s
    /// own body: `Arc::strong_count` and `Arc::ptr_eq` observe the allocation
    /// from the outside, so a deep copy (which would still satisfy "returns
    /// an `Option<Arc<GameDb>>`") cannot pass this test by accident.
    #[test]
    fn chat_clones_arc_gamedb() {
        let db = Arc::new(GameDb::empty_for_tests());
        let slot: Option<Arc<GameDb>> = Some(db.clone());
        assert_eq!(
            Arc::strong_count(&db),
            2,
            "setup sanity check: db + slot should hold 2 references"
        );

        let handed_to_worker = clone_game_db_for_worker(&slot);

        assert_eq!(
            Arc::strong_count(&db),
            3,
            "clone_game_db_for_worker must bump the Arc refcount (a cheap \
             handle clone), not allocate a new GameDb — a strong_count that \
             does not move confirms nothing new was allocated"
        );
        let worker_db = handed_to_worker.expect("Some(db) input must produce Some(db) output");
        assert!(
            Arc::ptr_eq(&db, &worker_db),
            "clone_game_db_for_worker must point at the SAME GameDb allocation \
             as the original — a deep clone would produce a different address \
             and fail this even though both sides still deref to equal data"
        );

        // Dropping the worker's handle must release exactly one reference,
        // proving `worker_db` really is a second owner of the same
        // allocation rather than, say, a `&'static` alias of some kind.
        drop(worker_db);
        assert_eq!(Arc::strong_count(&db), 2);
    }
}
