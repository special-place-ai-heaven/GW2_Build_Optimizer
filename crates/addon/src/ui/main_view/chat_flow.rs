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

/// How long the first attempt may take before a refused plate is served as-is
/// instead of being composed again.
///
/// A refusal costs a whole second tool run. On a fast model the first is over
/// in well under a minute and the second is cheap insurance; on a free one it
/// is minutes, and stacking two is how a refusal became a wait long enough to
/// read as a hang. Past this mark the player is better served by the plate in
/// hand with its caveat written on it - which is what the tail of the loop
/// already does.
const SECOND_ATTEMPT_CUTOFF: std::time::Duration = std::time::Duration::from_secs(180);

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

    // A specialization the player names decides the profession, over the
    // character on the left. Scourge equipped and "make me a ritualist" is
    // still a Necromancer; a Guardian selected and "make me a ritualist" is
    // a Necromancer build with nothing equipped to compare against.
    let no_character = state.main.current_build.is_none();
    let wished_profession: Option<String> = state.main.game_db.as_deref().and_then(|db| {
        let wished = wished_elite_spec(db, &message)?;
        db.specializations
            .values()
            .find(|s| s.elite && s.name.eq_ignore_ascii_case(&wished))
            .map(|s| s.profession.clone())
    });
    let switched_profession = wished_profession
        .as_ref()
        .is_some_and(|p| !p.eq_ignore_ascii_case(&profession));
    let named_a_spec = wished_profession.is_some();
    if let (true, Some(p)) = (switched_profession, wished_profession) {
        profession = p;
    }
    // "Improve my build" with no character, or a character with no build
    // and equipment resolved, and no specialization named: there is nothing
    // to improve, and no model call can find out what the player meant. Say
    // so, in Choya's voice. A named specialization bypasses this: that is a
    // build request, and the selection on the left does not matter to it.
    if no_character && !named_a_spec && asks_about_own_build(&message) {
        crate::ui::chat_bar::add_ai_response(&mut state.main.chat, t("choya.select_first"));
        return;
    }

    state.main.chat_epoch = state.main.chat_epoch.wrapping_add(1);
    let epoch = state.main.chat_epoch;
    state.main.chat.waiting = true;
    state.main.provider_issue = None;
    state.main.chat_wait_started = Some(std::time::Instant::now());
    state.main.optimize_stage = t("choya.thinking");

    let config = state.config.clone();
    let character = if switched_profession {
        String::new()
    } else {
        state
            .main
            .current_build
            .as_ref()
            .map(summarize_resolved_build)
            .unwrap_or_default()
    };
    let game_mode_label = state.main.game_mode.label().to_string();
    let scale = if state.main.game_mode == gw2_core::types::GameMode::WvW {
        state.main.combat_tier.label()
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
        .map(|r| r.family_brief(&state.main.game_mode, state.main.combat_tier))
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
    let loadout = if switched_profession {
        None
    } else {
        state.main.current_build.clone()
    };
    // The plate is ranked before it is served, so the chat path needs the same
    // scenario the Improve button builds — same tier mapping, same role
    // profile. Without one there is nothing for the referee to judge against.
    let combat_tier = match state.main.game_mode {
        gw2_core::types::GameMode::WvW => state.main.combat_tier,
        gw2_core::types::GameMode::PvP => gw2_optimizer::scenario::CombatTier::Solo,
        gw2_core::types::GameMode::PvE => gw2_optimizer::scenario::CombatTier::Party,
    };
    let selected_role = state.main.selected_role;
    // The message outranks the left panel. A specialization the player
    // names replaces the panel's elite lock for this request, so the
    // deterministic reference handed to the model is a build in the
    // specialization they asked for, not the one they happen to have
    // equipped. Name none, and the panel's selection is what gets improved.
    let mut chat_locks = state.main.build_locks.clone();
    if let Some(db) = state.main.game_db.as_deref() {
        if let Some(wished) = wished_elite_spec(db, &message) {
            if let Some(spec) = db
                .specializations
                .values()
                .find(|s| s.elite && s.name.eq_ignore_ascii_case(&wished))
            {
                if chat_locks.specs[2] != Some(spec.id) {
                    chat_locks.specs[2] = Some(spec.id);
                    chat_locks.trait_locks.clear();
                }
            }
        }
    }
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

            // Set when Choya tried to plate a build and could not serve one.
            // Recorded here rather than guessed downstream: a reply with no
            // build looks identical to a greeting from the outside, and only
            // one of the two should be answered with other people's builds.
            let plate_refused = std::cell::Cell::new(false);
            // The optimizer's own build for this request, kept outside the
            // closure so a model failure can still serve it.
            let fallback_reference: std::cell::RefCell<Option<String>> =
                std::cell::RefCell::new(None);
            // The handshake. What this model can do, measured once and kept;
            // the run below is shaped by it and refines it afterwards.
            let model_id = config.active_model_id().to_string();
            let profiles =
                std::cell::RefCell::new(gw2_optimizer::llm::profile::Store::load(&addon_dir));
            let round_secs: std::cell::RefCell<Vec<f32>> = std::cell::RefCell::new(Vec::new());
            let repair_used = std::cell::Cell::new(false);
            let result = (|| -> Result<gw2_optimizer::prompts::GeminiBuildResponse, String> {
                let client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                    .map_err(|e| e.to_string())?;

                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                let (profile, probed) = profiles.borrow_mut().ensure(
                    client.as_ref(),
                    &model_id,
                    gw2_optimizer::llm::profile::now_secs(),
                );
                nexus::log::log(
                    nexus::log::LogLevel::Info,
                    "GW2BuildOpt",
                    format!(
                        "Choya handshake{}: {}",
                        if probed { " (probed now)" } else { "" },
                        profile.summary()
                    ),
                );
                if let Some(e) = profiles.borrow().last_probe_error.as_deref() {
                    nexus::log::log(
                        nexus::log::LogLevel::Warning,
                        "GW2BuildOpt",
                        format!("Choya handshake got no answer ({e}); running on the assumed profile"),
                    );
                }
                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                if let Some(ref db) = db_clone {
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
                    // A worked answer for this exact scenario, handed to the
                    // model in Context.
                    //
                    // `get_optimizer_results` is advertised in the prompt but
                    // this path passes it an empty candidate list, so it has
                    // only ever replied "No optimizer results available" -
                    // the model composes blind while a proven-viable build is
                    // 30ms away. Measured 2026-09-05 on the player's own
                    // cache, WvW Roam/Support with Scourge locked: the
                    // deterministic seed lands at 68% health, repeatable, in
                    // 28ms, while the plate the referee refused that evening
                    // sat at 44% and could not repeat. The floor is free;
                    // withholding it is not.
                    let reference = reference_build(
                        db,
                        &profession,
                        &weights,
                        &chat_balance_ctx,
                        &chat_locks,
                        &scenario,
                    );
                    // Whether the model was handed a worked answer, and what
                    // that answer scored, is the first thing worth knowing when
                    // a plate is refused: a build that lands far below a
                    // reference it was shown is a different failure from one
                    // that never saw a reference at all.
                    *fallback_reference.borrow_mut() = reference
                        .as_ref()
                        .map(|r| format!("{}\n{}", r.line, r.verdict));
                    nexus::log::log(
                        nexus::log::LogLevel::Info,
                        "GW2BuildOpt",
                        match reference.as_ref() {
                            Some(r) => {
                                format!(
                                    "Choya reference for {profession}: {} | {}",
                                    r.line, r.verdict
                                )
                            }
                            None => format!(
                                "Choya has no deterministic reference for {profession}                                  (pipeline produced none) - composing unaided"
                            ),
                        },
                    );
                    if let Some(r) = reference.as_ref() {
                        // Not "which already passes every viability check": it
                        // sometimes does not, and the verdict line below now
                        // says which. Asserting a pass over notes that read
                        // fail told the model the bar was clearable when the
                        // search had already proved it was not.
                        kitchen.push_str(
                            "\nWorked answer from the deterministic optimizer for \
                             this exact Mode/Scale/Role:\n  ",
                        );
                        kitchen.push_str(&r.line);
                        kitchen.push_str("\n  ");
                        kitchen.push_str(&r.verdict);
                        kitchen.push_str(
                            "\nThat is your floor, not your answer: it ignores what \
                             the player just asked for. Match its survivability at \
                             least, then beat it on what they actually said. If you \
                             depart from it, be able to say why.",
                        );
                    }

                    // Who on the account can wear this. Only when the plate is
                    // not for the selected character: a named specialization
                    // of another profession, or no character at all.
                    if switched_profession || no_character {
                        kitchen.push_str(&roster_note(&addon_dir, &profession));
                    }

                    // Handed over, not fetched. See `profession_reference`:
                    // enumerating the specs, traits and skills cost 21s of a
                    // 31s tool phase and six round-trips, for data that is
                    // already in memory.
                    kitchen.push_str(&gw2_optimizer::gemini_tools::profession_reference(
                        db,
                        &profession,
                    ));

                    // Every tool stays on the table, including the two whose
                    // answer is already in the first message. Withholding
                    // them was tried 2026-09-07: the prompt still names them,
                    // so the model narrated "I'll start by checking what
                    // specs..." with no call to make, and a text-only turn is
                    // the final answer. A wasted round beats a wasted run.
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
                    let mut last_plate: Option<gw2_optimizer::prompts::GeminiBuildResponse> = None;
                    let started = std::time::Instant::now();
                    for attempt in 1..=2u32 {
                        if token.is_cancelled() {
                            return Err("Cancelled".into());
                        }
                        let mut prompt = gw2_optimizer::prompts::chat_refinement_prompt_with_tools(
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
                        let mut round_started = std::time::Instant::now();
                        // A model the handshake found cannot drive tools is
                        // not asked to: it gets the whole kitchen in one
                        // message and writes the plate from it.
                        let response = if profile.max_turns(client.thrifty()) == 0 {
                            client
                                .generate_brief(&prompt, 8_192)
                                .map_err(|e| e.to_string())?
                        } else {
                            client
                                .generate_with_tools_progress(
                                    &prompt,
                                    &tools,
                                    &mut |name: &str, args: &serde_json::Value| {
                                        let stale = crate::state::with_state(|s| {
                                            s.main.chat_epoch != epoch
                                        })
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
                                    // Eight for a model the handshake found
                                    // quick; fewer for a slow one, so the run
                                    // still ends in a plate inside the budget.
                                    profile.max_turns(client.thrifty()),
                                    &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                                        // How long each round actually took.
                                        // Without it a run that ends on a
                                        // deadline says only that it ended -
                                        // not whether one round stalled or
                                        // eight were merely slow, which are
                                        // opposite faults with opposite fixes.
                                        let round = round_started.elapsed();
                                        round_started = std::time::Instant::now();
                                        if !tool_names.is_empty() {
                                            round_secs.borrow_mut().push(round.as_secs_f32());
                                        }
                                        nexus::log::log(
                                            nexus::log::LogLevel::Info,
                                            "GW2BuildOpt",
                                            format!(
                                                "Choya round {turn}/{max_turns} took {:.1}s: {}",
                                                round.as_secs_f32(),
                                                if tool_names.is_empty() {
                                                    "writing the build".to_string()
                                                } else {
                                                    tool_names.join(", ")
                                                }
                                            ),
                                        );
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
                            // Prose where a plate was due. In-game 2026-09-07
                            // (minimax-m3:free): eight rounds of lookups, then
                            // a paragraph naming the traits and no JSON - a
                            // build the player could read and not wear. One
                            // repair request, no tools, asks for the plate of
                            // what it just wrote; only if that fails too is
                            // the prose served as conversation.
                            Err(_) => {
                                let repair = format!(
                                    "You answered in prose:\n\n{response}\n\nServe that \
                                     as the plate now: ONLY the JSON build object from \
                                     your instructions - \"specializations\" as objects \
                                     with \"name\", \"elite\" and \"traits\" (three each), \
                                     \"weapons\" with set1/set2 main/off, \"skills\" with \
                                     heal/utilities/elite, \"rune\", \"sigils\", \"relic\", \
                                     \"stat_prefix\", \"explanation\". No text outside \
                                     the JSON."
                                );
                                let repaired = client
                                    .generate_brief(&repair, 8_192)
                                    .ok()
                                    .and_then(|r| {
                                        gw2_optimizer::prompts::parse_gemini_build(&r).ok()
                                    });
                                nexus::log::log(
                                    nexus::log::LogLevel::Info,
                                    "GW2BuildOpt",
                                    format!(
                                        "Choya answered in prose; repair request {}",
                                        if repaired.is_some() {
                                            "produced a plate"
                                        } else {
                                            "did not"
                                        }
                                    ),
                                );
                                if let Some(p) = repaired {
                                    repair_used.set(true);
                                    p
                                } else {
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
                            }
                        };
                        if let Some(ref cur) = loadout {
                            fill_holes_from_loadout(&mut parsed, cur);
                        }
                        apply_radar_prefix(&mut parsed, &weights, &message);

                        // The prompt allows a spoken reply with no plate for
                        // greetings and questions. In-game 2026-09-07
                        // (minimax-m3:free) a build request came back in that
                        // shape with the whole build inside "explanation" -
                        // valid JSON, so the parse-failure repair never saw
                        // it. A build request answered with no plate gets the
                        // same one repair: serve what you just said.
                        if parsed.specializations.is_empty() && wants_a_build(&message) {
                            let repair = format!(
                                "You described the build in prose:\n\n{}\n\nServe that \
                                 as the plate now: ONLY the JSON build object from your \
                                 instructions - \"specializations\" as three objects with \
                                 \"name\", \"elite\" and \"traits\" (three each), \"weapons\" \
                                 with set1/set2 main/off, \"skills\" with \
                                 heal/utilities/elite, \"rune\", \"sigils\", \"relic\", \
                                 \"stat_prefix\", \"explanation\". Empty \"specializations\" \
                                 is not an answer to a build request.",
                                parsed.explanation
                            );
                            let plated = client
                                .generate_brief(&repair, 8_192)
                                .ok()
                                .and_then(|r| gw2_optimizer::prompts::parse_gemini_build(&r).ok())
                                .filter(|p| !p.specializations.is_empty());
                            nexus::log::log(
                                nexus::log::LogLevel::Info,
                                "GW2BuildOpt",
                                format!(
                                    "Choya spoke the build instead of plating it; repair request {}",
                                    if plated.is_some() {
                                        "produced a plate"
                                    } else {
                                        "did not"
                                    }
                                ),
                            );
                            if let Some(p) = plated {
                                repair_used.set(true);
                                parsed = p;
                                if let Some(ref cur) = loadout {
                                    fill_holes_from_loadout(&mut parsed, cur);
                                }
                                apply_radar_prefix(&mut parsed, &weights, &message);
                            }
                        }

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

                        // The specialization the player named is not a
                        // preference the ranking may trade away: "scourge"
                        // means a Scourge plate, whatever else scores better.
                        let shortfall = match wished_elite_spec(db, &message) {
                            Some(wished)
                                if !parsed
                                    .specializations
                                    .iter()
                                    .any(|(name, _)| name.eq_ignore_ascii_case(&wished)) =>
                            {
                                Err(format!(
                                    "the player asked for {wished} and this plate does not run {wished}"
                                ))
                            }
                            _ => plate_shortfall(
                                &validated,
                                baseline.as_ref(),
                                db,
                                &plate_profession,
                                &weights,
                                &chat_balance_ctx,
                                &scenario,
                                reference.as_ref().map_or(&[][..], |r| &r.unreachable),
                            ),
                        };
                        match shortfall {
                            // Served, with whatever the non-blocking gates
                            // had to say written on it. A caveat the player
                            // can read beats a gate that silently vetoes.
                            Ok(concerns) => {
                                if !concerns.is_empty() {
                                    let note =
                                        tf("fmt.plate_concern", &[("concern", &concerns.join("; "))]);
                                    parsed.explanation = if parsed.explanation.trim().is_empty() {
                                        note
                                    } else {
                                        format!("{}

{}", parsed.explanation.trim(), note)
                                    };
                                }
                                return Ok(parsed);
                            }
                            Err(why) => {
                                let spent = started.elapsed();
                                nexus::log::log(
                                    nexus::log::LogLevel::Info,
                                    "GW2BuildOpt",
                                    format!(
                                        "Choya plate refused (attempt {attempt}, {:.0}s spent): {why}",
                                        spent.as_secs_f32()
                                    ),
                                );
                                feedback = Some(why.clone());
                                rejected = Some(why);
                                last_plate = Some(parsed);
                                // A second go is another whole tool run. On a
                                // slow model the first one can already have
                                // eaten the player's patience, and two stacked
                                // runs are how a refusal turned into a wait
                                // long enough to read as a hang.
                                if spent >= SECOND_ATTEMPT_CUTOFF {
                                    break;
                                }
                            }
                        }
                    }

                    // Nothing to keep. The gate exists to stop a worse build
                    // replacing one the player is already wearing, and with
                    // no character selected there is no such build — so
                    // refusing here hands back nothing at all, which is the
                    // one outcome worse than an imperfect build. Serve the
                    // last plate and say what is weak about it.
                    if baseline.is_none() {
                        if let Some(mut plate) = last_plate {
                            let concern = rejected.unwrap_or_default();
                            plate.explanation = if plate.explanation.trim().is_empty() {
                                tf("fmt.plated_with_concern", &[("concern", &concern)])
                            } else {
                                format!(
                                    "{}\n\n{}",
                                    plate.explanation.trim(),
                                    tf("fmt.plate_concern", &[("concern", &concern)])
                                )
                            };
                            return Ok(plate);
                        }
                    }

                    // Both attempts lost to the player's own build — which
                    // only reaches here when there IS one. Serving the second
                    // one anyway is the bug this gate exists to stop, so
                    // Choya says what happened and the build stands.
                    // ponytail: plain English like `KEPT_GEAR_HEADLINE` in
                    // optimize_flow; move behind `t("choya.kept")` when
                    // `locales/` is next open.
                    plate_refused.set(true);
                    Ok(gw2_optimizer::prompts::GeminiBuildResponse {
                        explanation: tf(
                            "fmt.kept_your_build",
                            &[("why", &rejected.unwrap_or_default())],
                        ),
                        ..Default::default()
                    })
                } else {
                    Err("Game data not loaded".into())
                }
            })();
            // The run refines the handshake: how long a round really took
            // here, whether the plate needed repairing, whether it failed.
            profiles.borrow_mut().record_run(
                &model_id,
                &round_secs.borrow(),
                repair_used.get(),
                result.is_err(),
            );

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
                                    let body = chat_display_text(
                                        &raw.explanation,
                                        raw.specializations.len(),
                                        &errors,
                                    );
                                    // A refused plate is a dead end, and a
                                    // dead end gets somewhere to go: the
                                    // community builds are offered under it.
                                    if plate_refused.get() {
                                        crate::ui::chat_bar::add_failed_build_response(
                                            &mut s.main.chat,
                                            body,
                                        );
                                    } else {
                                        crate::ui::chat_bar::add_ai_response(&mut s.main.chat, body);
                                    }
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
                                    .map(|v| {
                                        v.errors
                                            .iter()
                                            .map(|e| e.detail.clone())
                                            .chain(v.warnings.iter().cloned())
                                            .collect()
                                    })
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
                                    attach_chat_stats(
                                        &mut suggestion,
                                        db,
                                        &profession,
                                        &live_mode,
                                        validated.as_ref(),
                                    );
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
                            // format_provider_issue turns this into a category
                            // ("Request timed out"), which is all the player
                            // needs and nowhere near enough to diagnose from.
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2BuildOpt",
                                format!("Choya request failed: {e}"),
                            );
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
                                // The run must end in a build. The optimizer's
                                // own answer for this request has been in hand
                                // since before the first round; a model that
                                // ran out of clock does not take it with it.
                                match fallback_reference.borrow().as_deref() {
                                    Some(build) => {
                                        crate::ui::chat_bar::add_ai_response(
                                            &mut s.main.chat,
                                            format!(
                                                "{msg}\n\n{}\n{build}",
                                                t("choya.fallback_reference")
                                            ),
                                        );
                                    }
                                    None => {
                                        crate::ui::chat_bar::add_ai_response(&mut s.main.chat, msg);
                                    }
                                }
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
/// One line of build plus one line of referee verdict for the deterministic
/// answer to this scenario, or `None` if the pipeline cannot produce one.
///
/// The synergy pipeline is the cheap tier - measured at 28ms against the
/// player's own cache - and its answer is already referee-viable, so there is
/// no reason for the chat to reason from nothing.
/// The deterministic optimizer's best answer for one request, and what the
/// referee made of it.
struct Reference {
    /// One-line build summary, handed to the model as its floor.
    line: String,
    /// Gate-by-gate notes, prefixed with whether it actually passed.
    verdict: String,
    /// Blocking gates this build could not clear. An exhaustive search over
    /// the whole profession missed them, so they are not a bar the model can
    /// be held to either: for this one request they caveat a plate instead of
    /// refusing it. Empty in the normal case, where the seed passes.
    unreachable: Vec<gw2_optimizer::referee::ViabilityGate>,
}

fn reference_build(
    db: &GameDb,
    profession_name: &str,
    weights: &gw2_optimizer::scoring::OptimizationWeights,
    ctx: &BalanceContext,
    locks: &gw2_core::types::BuildLocks,
    scenario: &gw2_optimizer::scenario::ScenarioSpec,
) -> Option<Reference> {
    let prefix = gw2_optimizer::scoring::select_gear_prefix(weights).primary;
    let seed = gw2_optimizer::synergy_pipeline::optimize_synergy(
        db,
        profession_name,
        weights,
        ctx,
        prefix,
        locks,
        Some(scenario),
        &mut |_| {},
    )
    .ok()?;
    let v = &seed.validated;
    if !v.errors.is_empty() {
        return None;
    }
    let name = |o: &Option<(u32, String)>| {
        o.as_ref()
            .map(|(_, n)| n.as_str())
            .unwrap_or("-")
            .to_string()
    };
    let specs: Vec<String> = v
        .specializations
        .iter()
        .map(|s| {
            let traits: Vec<&str> = s.trait_names.iter().map(|t| t.as_str()).collect();
            format!("{} [{}]", s.name, traits.join(", "))
        })
        .collect();
    let utils: Vec<String> = v.skills.utilities.iter().map(name).collect();
    let hand = |h: &Option<String>| h.clone().unwrap_or_else(|| "-".into());
    let line = format!(
        "{prefix} | {} | heal {} | utilities {} | elite {} | weapons {}/{} + {}/{}",
        specs.join(" | "),
        name(&v.skills.heal),
        utils.join(", "),
        name(&v.skills.elite),
        hand(&v.weapons.set1.main_hand),
        hand(&v.weapons.set1.off_hand),
        hand(&v.weapons.set2.main_hand),
        hand(&v.weapons.set2.off_hand),
    );
    // The gate notes carry the numbers the plate will be judged on.
    let report = gw2_optimizer::referee::evaluate_validated_build(
        v,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
    );
    let notes = report
        .viability
        .gates
        .iter()
        .map(|g| format!("{:?}: {}", g.gate, g.note))
        .collect::<Vec<_>>()
        .join("; ");
    let unreachable: Vec<gw2_optimizer::referee::ViabilityGate> = report
        .viability
        .gates
        .iter()
        .filter(|g| !g.passed && g.gate.blocks())
        .map(|g| g.gate.clone())
        .collect();
    // "It passes with" used to be printed unconditionally, over gate notes
    // that said otherwise. In-game 2026-09-06, Necromancer WvW Roam/Support:
    // the seed came back `SustainRecovery: survived=true, health=34%,
    // margin=-951/s, repeatable=false` - a fail - and was handed to the model
    // as a worked answer that "already passes every viability check". Every
    // plate the model then composed was refused on that same gate, twice per
    // message, which is where the free-model timeouts were being spent.
    let verdict = if unreachable.is_empty() {
        format!("It passes with - {notes}")
    } else {
        let names: Vec<String> = unreachable.iter().map(|g| format!("{g:?}")).collect();
        format!(
            "It is the best build the search could find and it still fails {} - {notes}",
            names.join(", ")
        )
    };
    Some(Reference {
        line,
        verdict,
        unreachable,
    })
}

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
/// What to change to pass a gate, in the model's vocabulary rather than the
/// referee's. The gate note already carries the numbers; a model handed
/// "SustainRecovery (margin=-566/s, repeatable=false)" and nothing else has
/// to guess which half of the build to touch, and its second attempt is the
/// player's last one.
fn gate_remedy(gate: &gw2_optimizer::referee::ViabilityGate) -> &'static str {
    use gw2_optimizer::referee::ViabilityGate as G;
    match gate {
        G::StunbreakCount => "add a stunbreak - it has no way out of a hard CC",
        G::StabilityAccess => {
            "give it Stability, or an evade, block, invuln or stealth it can use on demand"
        }
        G::CleanseRate => {
            "add condition cleanse: a cleansing utility, a trait that cleanses, or a rune or              relic that does"
        }
        G::EffectiveHealth => {
            "raise effective health: Toughness and Vitality in the prefix, or a defensive              specialization"
        }
        G::MobilityOut => "add a disengage: stealth, an evade, a block, or a movement skill",
        G::HarasserStrip => "strip or corrupt boons before the damage lands, not after",
        G::BoonUptime => {
            "keep the boons up: Concentration for duration, a source with a shorter cooldown,              or one that pulses instead of firing once"
        }
        G::EncounterOutcome => "raise damage - the target does not go down inside the clock",
        G::SecureCompletion => "add an interrupt so the target cannot recover",
        G::ProtectedExecution => {
            "cover the chain - the key skills are being interrupted before they land"
        }
        G::SustainRecovery => {
            "raise sustain so it survives the answer and keeps going: healing per second,              Protection or barrier uptime, Toughness and Vitality, or a heal skill with a              shorter cooldown"
        }
        G::ResourceLegality => {
            "the rotation spends more of the profession resource than it generates - cheaper              skills, or one that generates"
        }
    }
}

/// Whether a failed gate refuses a plate outright, or only writes a caveat on
/// it.
///
/// `blocks` is the population answer and `unreachable` the per-request one;
/// see the comment in [`plate_shortfall`] for what each measures.
fn gate_vetoes(
    gate: &gw2_optimizer::referee::ViabilityGate,
    unreachable: &[gw2_optimizer::referee::ViabilityGate],
) -> bool {
    gate.blocks() && !unreachable.contains(gate)
}

// Every argument is one input the referee needs and none has a sensible
// default; bundling them into a struct would move the same list one line up.
#[allow(clippy::too_many_arguments)]
fn plate_shortfall(
    plate: &gw2_optimizer::validation::ValidatedBuild,
    baseline: Option<&gw2_optimizer::referee::RefereeReport>,
    db: &GameDb,
    profession_name: &str,
    weights: &gw2_optimizer::scoring::OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &gw2_optimizer::scenario::ScenarioSpec,
    unreachable: &[gw2_optimizer::referee::ViabilityGate],
) -> Result<Vec<String>, String> {
    let report = gw2_optimizer::referee::evaluate_validated_build(
        plate,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
    );
    // Two ways a failed gate loses its veto.
    //
    // `ViabilityGate::blocks` is the population answer: a gate the published
    // meta itself fails is describing us, not the game.
    //
    // `unreachable` is the per-request one. The deterministic optimizer
    // searched this exact profession, mode, scale and role and its best answer
    // failed these gates too, so they are not a bar a plate can be refused
    // against - the search already proved nothing clears them here. Without
    // this the loop is unwinnable: every plate is refused, both attempts burn
    // a whole tool run each, and the player waits out the request deadline for
    // an answer that could never have been served. Measured in-game
    // 2026-09-06, Necromancer WvW Roam/Support on SustainRecovery.
    //
    // Either way the gate still becomes a caveat the player reads. Nothing is
    // hidden, and the plate must still beat their equipped build below.
    let vetoes =
        |g: &gw2_optimizer::referee::GateResult| !g.passed && gate_vetoes(&g.gate, unreachable);
    let concerns: Vec<String> = report
        .viability
        .gates
        .iter()
        .filter(|g| !g.passed && !vetoes(g))
        .map(|g| gate_remedy(&g.gate).to_string())
        .collect();
    if !report.viability.is_viable {
        let failed: Vec<String> = report
            .viability
            .gates
            .iter()
            .filter(|g| vetoes(g))
            .map(|g| format!("{:?} ({}) - {}", g.gate, g.note, gate_remedy(&g.gate)))
            .collect();
        if !failed.is_empty() {
            return Err(format!(
                "that build is not viable in this game mode. Failed checks: {}",
                failed.join("; ")
            ));
        }
    }
    // No baseline is not a pass mark, it is an unarmed gate: viability alone
    // still had to hold above.
    let Some(baseline) = baseline else {
        return Ok(concerns);
    };
    if super::optimize_flow::beats_baseline(
        &gw2_optimizer::referee::search_rank(&report),
        &gw2_optimizer::referee::search_rank(baseline),
    ) {
        return Ok(concerns);
    }
    Err(format!(
        "that build does not beat what the player is already wearing \
         (their build scores {:.0}, yours {:.0} on the same weights). \
         Keep what already works and change only what you can argue is better.",
        baseline.user_intent_score, report.user_intent_score,
    ))
}

/// Whether the player asked for something to equip, as opposed to chatting.
/// Decides only whether an empty plate is a failure worth one repair
/// request; a false positive costs one short request, a false negative
/// costs the player the build.
pub(super) fn wants_a_build(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "build",
        "loadout",
        "improve",
        "gear",
        "setup",
        "set up",
        "spec ",
        "make me",
        "give me",
        "optimi",
        "rotation",
        "what should i run",
        "what should i play",
    ]
    .iter()
    .any(|k| lower.contains(k))
}

/// Whether the message is about the player's own equipped build rather than
/// a build they describe: "improve this", "my current build", "what I have
/// on". Only meaningful when nothing is selected, where it decides between
/// asking them to pick a character and composing blind.
pub(super) fn asks_about_own_build(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "improve",
        "my build",
        "my current",
        "this build",
        "this character",
        "equipped",
        "what i have",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// One line about the player's own characters for a build not tied to the
/// selected one: who could wear it, or that nobody on the account can yet.
/// Read from the character cache, so it costs no API call.
fn roster_note(addon_dir: &std::path::Path, profession: &str) -> String {
    let cache = gw2_api::cache::DataCache::new(addon_dir.join("cache"));
    let names = cache.load_characters().ok().flatten().unwrap_or_default();
    let roster: Vec<(String, String)> = names
        .iter()
        .filter_map(|name| {
            let tabs: serde_json::Value = cache.load_character(name, "buildtabs").ok()??;
            let prof = tabs
                .as_array()?
                .iter()
                .find_map(|t| t["build"]["profession"].as_str().map(String::from))?;
            Some((name.clone(), prof))
        })
        .collect();
    let wearers: Vec<&str> = roster
        .iter()
        .filter(|(_, p)| p.eq_ignore_ascii_case(profession))
        .map(|(n, _)| n.as_str())
        .collect();
    if roster.is_empty() {
        return String::new();
    }
    if wearers.is_empty() {
        let others: Vec<String> = roster.iter().map(|(n, p)| format!("{n} ({p})")).collect();
        format!(
            "\nThe player has no {profession} on this account (their characters: {}). Plate the \
             {profession} build they asked for anyway, say in one clause that they have no \
             {profession} yet, and do not refuse. There is no equipped gear to compare against.\n",
            others.join(", ")
        )
    } else {
        format!(
            "\nThe player's {profession} characters: {}. This build is for one of them; they \
             have not selected that character, so there is no equipped gear to compare against.\n",
            wearers.join(", ")
        )
    }
}

/// The elite specialization the player named in their message, if any and
/// not negated ("not scourge", "no scourge"). Matched on whole words against
/// the game data, so "reaper" in "grim reaper of a build" still counts and
/// "harbingers" does not misread as a different spec.
pub(super) fn wished_elite_spec(db: &GameDb, message: &str) -> Option<String> {
    let words: Vec<String> = message
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase().trim_end_matches('s').to_string())
        .collect();
    let mut specs: Vec<&gw2_api::models::Specialization> =
        db.specializations.values().filter(|s| s.elite).collect();
    specs.sort_by_key(|s| s.id);
    specs.into_iter().find_map(|spec| {
        let wanted = spec.name.to_lowercase().trim_end_matches('s').to_string();
        let at = words.iter().position(|w| *w == wanted)?;
        let negated = at > 0 && matches!(words[at - 1].as_str(), "not" | "no" | "without");
        (!negated).then(|| spec.name.clone())
    })
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
    use super::{
        asks_about_own_build, gate_vetoes, plate_is_servable, wants_a_build, wished_elite_spec,
    };

    #[test]
    fn improving_nothing_is_told_from_asking_for_something() {
        assert!(asks_about_own_build(
            "Improve my current equipped build. Keep the playstyle, raise the weak axes."
        ));
        assert!(asks_about_own_build("can you improve this?"));
        assert!(!asks_about_own_build("Make me a badass Ritualist build."));
        assert!(!asks_about_own_build("Build me a WvW roaming loadout."));
    }

    fn necro_db() -> gw2_optimizer::gamedb::GameDb {
        let mut db = gw2_optimizer::gamedb::GameDb::empty_for_tests();
        for (id, name, elite) in [
            (53, "Reaper", true),
            (34, "Scourge", true),
            (39, "Curses", false),
        ] {
            db.specializations.insert(
                id,
                gw2_api::models::Specialization {
                    id,
                    name: name.into(),
                    profession: "Necromancer".into(),
                    elite,
                    minor_traits: vec![],
                    major_traits: vec![],
                    weapon_trait: None,
                    icon: None,
                    background: None,
                    profession_icon: None,
                    profession_icon_big: None,
                },
            );
        }
        db
    }

    #[test]
    fn the_named_elite_spec_is_read_from_the_message() {
        let db = necro_db();
        assert_eq!(
            wished_elite_spec(&db, "Make me a badass Scourge build."),
            Some("Scourge".into())
        );
        assert_eq!(
            wished_elite_spec(&db, "reaper, power, roaming"),
            Some("Reaper".into())
        );
        assert_eq!(
            wished_elite_spec(&db, "make me a good reaper"),
            Some("Reaper".into())
        );
        assert_eq!(
            wished_elite_spec(&db, "one of those reapers"),
            Some("Reaper".into()),
            "plural"
        );
        assert_eq!(
            wished_elite_spec(&db, "anything but not scourge"),
            None,
            "negated"
        );
        assert_eq!(
            wished_elite_spec(&db, "a curses build"),
            None,
            "core line is not an elite wish"
        );
        assert_eq!(wished_elite_spec(&db, "power build please"), None);
    }

    #[test]
    fn a_build_request_is_told_from_chat() {
        assert!(wants_a_build("Make me a badass Ritualist build."));
        assert!(wants_a_build("improve this"));
        assert!(wants_a_build("what should I run in wvw?"));
        assert!(!wants_a_build("hi choya"));
        assert!(!wants_a_build("what does Dread do?"));
    }
    use gw2_optimizer::referee::ViabilityGate as G;

    /// In-game 2026-09-06, Necromancer WvW Roam/Support: the deterministic
    /// optimizer's own best answer came back `SustainRecovery: survived=true,
    /// health=34%, margin=-951/s, repeatable=false`. A dedicated healer's
    /// output goes to allies, so it never wins the solo sustain race the gate
    /// models, and `CombatKind::Support` demands `repeatable`. Every plate the
    /// model composed was refused on that gate, twice per message - two whole
    /// tool runs spent on a bar an exhaustive search had already proved
    /// nothing could clear.
    #[test]
    fn a_gate_the_search_could_not_clear_cannot_refuse_a_plate() {
        assert!(
            gate_vetoes(&G::SustainRecovery, &[]),
            "the gate still refuses a plate on a request where the seed passed"
        );
        assert!(
            !gate_vetoes(&G::SustainRecovery, &[G::SustainRecovery]),
            "the seed failed this gate too, so it caveats the plate, not refuses it"
        );
        assert!(
            gate_vetoes(&G::SustainRecovery, &[G::CleanseRate]),
            "an unrelated unreachable gate must not disarm this one"
        );
        assert!(
            !gate_vetoes(&G::HarasserStrip, &[]),
            "gates the published meta itself fails were never vetoes"
        );
    }

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
