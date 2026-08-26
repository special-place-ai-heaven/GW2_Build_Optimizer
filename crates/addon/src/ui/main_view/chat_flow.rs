use super::optimization::{
    apply_gemini_response, apply_radar_prefix, attach_chat_stats, chat_display_text,
    fill_holes_from_loadout, format_provider_issue, gemini_from_validated, humanize_tool_names,
    keep_equipped_weapons, kitchen_brief, result_alert_tab, simulate_suggestion_rotation,
    suggestion_to_chat_code, summarize_resolved_build, summarize_suggestion,
    validated_build_to_chat_code,
};
use crate::state::AddonState;
use gw2_core::i18n::{t, tf};
use gw2_optimizer::balance::BalanceContext;

/// Send a chat order to the chef (active LLM) for a plated build.
/// Uses function calling so the chef has the full pantry and every station.
pub(super) fn send_chat_message(state: &mut AddonState, message: String) {
    let (display, inbound_chips, _chef_order) =
        crate::chat_links::annotate_order(&message, state.main.game_db.as_ref());
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
    let has_loadout = !character.is_empty();
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
    let token = state.cancel_token.clone();
    let db_clone = state.main.game_db.clone();
    let weights = state.main.weights.clone();
    let loadout = state.main.current_build.clone();
    let chat_balance_ctx = BalanceContext::new(state.main.game_mode.clone());

    std::thread::spawn(move || {
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
                    let prompt = gw2_optimizer::prompts::chat_refinement_prompt_with_tools(
                        &profession,
                        &game_mode_label,
                        &message,
                        &kitchen,
                        gw2_core::i18n::choya_name_for(&config.ui_language),
                    );
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

                    let response = if has_loadout {
                        client.generate(&prompt).map_err(|e| e.to_string())?
                    } else {
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
                                3,
                                &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                                    let tools_str = humanize_tool_names(tool_names);
                                    crate::state::with_state(|s| {
                                        if s.main.chat_epoch != epoch {
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

                    let mut parsed = match gw2_optimizer::prompts::parse_gemini_build(&response) {
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
                    Ok(parsed)
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
                                "Kitchen validation errors: {}",
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
                                    (s.main.game_db.clone(), s.main.game_mode.clone())
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
}
