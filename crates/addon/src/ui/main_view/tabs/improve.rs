//! Improve tab — current vs optimized side-by-side, lock panel, chat bar.

use nexus::imgui::{ChildWindow, Selectable, Ui};

use crate::state::AddonState;
use crate::ui::comparison::ResultPane;
use crate::ui::theme;

use super::{build_display, lock_panel, optimization, render_optimization_progress};

pub(in crate::ui::main_view) fn render_improve_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.build_loading {
        ui.text_colored(theme::WARN, "Resolving build from API data...");
        return;
    }

    // Error display
    if let Some(err) = state.main.comparison.error.clone() {
        ui.text_colored(theme::ERR, format!("[!] {}", err));
        ui.same_line();
        if ui.small_button("Dismiss##opt_err_improve") {
            state.main.comparison.error = None;
        }
        ui.spacing();
    }

    // ── Optimization progress banner ──
    if state.main.optimizing {
        render_optimization_progress(ui, &state.main.optimize_stage, ui.frame_count());
    }

    // ── Locked spec badge ─────────────────────────────────────────────────
    // Show which elite spec is locked so the user knows the optimizer won't change it.
    {
        let locked_spec_name = state
            .main
            .build_locks
            .specs
            .get(2)
            .and_then(|s| *s)
            .and_then(|id| {
                state
                    .main
                    .game_db
                    .as_ref()
                    .and_then(|db| db.spec(id))
                    .map(|s| s.name.clone())
            });
        if let Some(spec_name) = locked_spec_name {
            ui.text_colored(
                theme::OPTIMIZED,
                format!("  \u{1F512} Locked to: {}", spec_name),
            );
            ui.same_line();
            if ui.small_button("Unlock") {
                state.main.build_locks.specs[2] = None;
            }
            ui.spacing();
        }
    }

    // ── Two-panel layout: Current Build | Optimized Build ──
    let has_suggestion = !state.main.comparison.suggestions.is_empty();
    let show_chat = state.main.current_build.is_some();
    let footer = (if has_suggestion { 36.0 } else { 0.0 })
        + if show_chat {
            crate::ui::chat_bar::reserved_height(&state.main.chat)
        } else {
            0.0
        };

    if state.main.current_build.is_some() {
        // Clone build data upfront to avoid borrow conflicts with mutable lock_panel state
        let build = state.main.current_build.clone().unwrap();
        let stats = state.main.current_stats.clone();
        let profession_name = build.profession.clone();
        let current_specs: Vec<(u32, Vec<u32>)> = build
            .specializations
            .iter()
            .map(|spec| {
                let selected_ids: Vec<u32> = spec
                    .traits_selected
                    .iter()
                    .filter(|t| t.selected)
                    .map(|t| t.id)
                    .collect();
                (spec.id, selected_ids)
            })
            .collect();

        if has_suggestion {
            // Suggestion tabs above panels
            let tab_count = state.main.comparison.suggestions.len();
            if tab_count > 1 {
                for (i, sug) in state.main.comparison.suggestions.iter().enumerate() {
                    let selected = state.main.comparison.selected_suggestion == i;
                    let label = if sug.label.is_empty() {
                        format!("Build {}", i + 1)
                    } else if sug.label.starts_with("Score:") {
                        format!("Option {} ({})", i + 1, sug.stat_prefix)
                    } else {
                        sug.label.clone()
                    };
                    if Selectable::new(&format!("{}##sug_{}", label, i))
                        .selected(selected)
                        .size([0.0, 0.0])
                        .build(ui)
                    {
                        state.main.comparison.selected_suggestion = i;
                    }
                    if i < tab_count - 1 {
                        ui.same_line();
                    }
                }
                ui.spacing();
            }

            let idx = state
                .main
                .comparison
                .selected_suggestion
                .min(state.main.comparison.suggestions.len() - 1);
            let chat_code = state.main.comparison.suggestions[idx].chat_code.clone();
            crate::ui::comparison::render_chat_code_copy(
                ui,
                chat_code.as_deref(),
                "improve",
                &mut state.main.comparison.copy_feedback_frames,
            );
            crate::ui::comparison::render_result_pane_tabs(
                ui,
                &mut state.main.comparison.result_pane,
            );

            let scroll_height = (ui.content_region_avail()[1] - footer).max(64.0);

            let idx = state
                .main
                .comparison
                .selected_suggestion
                .min(state.main.comparison.suggestions.len() - 1);
            let suggestion = state.main.comparison.suggestions[idx].clone();
            let pane = state.main.comparison.result_pane;
            let db_ref = state.main.game_db.as_ref();
            let gain = crate::ui::gear_sheet::combat_gain(
                state.main.comparison.current_combat_solo.as_ref(),
                suggestion.combat_solo.as_ref(),
            );

            ChildWindow::new("##improve_scroll")
                .size([0.0, scroll_height])
                .build(ui, || match pane {
                    ResultPane::Build => {
                        crate::ui::gear_sheet::render_view_toggle(
                            ui,
                            &mut state.main.comparison.show_optimized,
                        );
                        let viewing = state.main.comparison.show_optimized;
                        if viewing {
                            build_display::render_card_header(
                                ui,
                                "OPTIMIZED BUILD",
                                theme::OPTIMIZED,
                            );
                            lock_panel::render_optimized_specs_panel(
                                ui,
                                db_ref.map(|db| db as &gw2_optimizer::gamedb::GameDb),
                                &suggestion.specializations,
                            );
                        } else {
                            build_display::render_card_header(ui, "CURRENT BUILD", theme::CURRENT);
                            lock_panel::render_lock_panel(
                                ui,
                                &mut state.main.build_locks,
                                &mut state.main.locks_panel_expanded,
                                db_ref.map(|db| db as &gw2_optimizer::gamedb::GameDb),
                                &profession_name,
                                &current_specs,
                                &mut state.main.locks_hover,
                            );
                        }
                        ui.spacing();
                        crate::ui::gear_sheet::render_current_sheet(
                            ui,
                            &build,
                            Some(&suggestion),
                            db_ref,
                            viewing,
                            gain,
                        );
                        ui.spacing();
                        if viewing {
                            build_display::render_suggestion_skills(ui, &suggestion, db_ref);
                        } else {
                            build_display::render_build_skills(ui, &build, db_ref);
                        }
                    }
                    ResultPane::Stats => {
                        crate::ui::comparison::render_stats_pane(
                            ui,
                            stats.as_ref(),
                            &state.main.comparison,
                            &suggestion,
                            db_ref,
                        );
                    }
                });
        } else {
            // No suggestion yet — show current build with lock panel (full width)
            let scroll_height = (ui.content_region_avail()[1] - footer).max(64.0);

            ChildWindow::new("##improve_single")
                .size([0.0, scroll_height])
                .build(ui, || {
                    build_display::render_card_header(ui, "CURRENT BUILD", theme::CURRENT);
                    {
                        let db_ref = state.main.game_db.as_ref();
                        lock_panel::render_lock_panel(
                            ui,
                            &mut state.main.build_locks,
                            &mut state.main.locks_panel_expanded,
                            db_ref.map(|db| db as &gw2_optimizer::gamedb::GameDb),
                            &profession_name,
                            &current_specs,
                            &mut state.main.locks_hover,
                        );
                    }
                    crate::ui::gear_sheet::render_current_sheet(
                        ui,
                        &build,
                        None,
                        state.main.game_db.as_ref(),
                        false,
                        0,
                    );
                    build_display::render_build_skills(ui, &build, state.main.game_db.as_ref());
                });
        }
    } else if state.main.selected_character.is_some() {
        ui.text_colored(theme::MUTED, "Loading character build...");
    } else {
        ui.text_colored(theme::MUTED, "Select a character from the left panel.");
    }

    // Save build UI + clear button
    if has_suggestion {
        super::saveload::render_save_build_ui(ui, state);
        ui.same_line();
        if ui.small_button("Clear Results##improve") {
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
    }

    // Chat bar at bottom
    if state.main.current_build.is_some() {
        if let Some(msg) = crate::ui::chat_bar::render_chat_bar(ui, &mut state.main.chat) {
            optimization::send_chat_message(state, msg);
        }
    }
}
