//! Improve tab — current vs optimized side-by-side, lock panel.

use nexus::imgui::{ChildWindow, Selectable, Ui};

use crate::state::AddonState;
use crate::ui::comparison::ResultPane;
use crate::ui::theme;
use gw2_core::i18n::t;

use super::{build_display, lock_panel, render_optimization_progress};

pub(in crate::ui::main_view) fn render_improve_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.build_loading {
        ui.text_colored(theme::WARN, t("status.resolving_api"));
        return;
    }

    // Error display
    if let Some(err) = state.main.comparison.error.clone() {
        ui.text_colored(theme::ERR, format!("[!] {}", err));
        ui.same_line();
        if ui.small_button(&format!("{}##opt_err_improve", t("btn.dismiss"))) {
            state.main.comparison.error = None;
        }
        ui.spacing();
    }

    // ── Optimization progress banner ──
    if state.main.optimizing {
        render_optimization_progress(ui, &state.main.optimize_stage, ui.frame_count());
    }

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

    // ── Two-panel layout: Current Build | Optimized Build ──
    let has_suggestion = !state.main.comparison.suggestions.is_empty();
    let footer = if has_suggestion { 36.0 } else { 0.0 };

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
                        state.main.comparison.show_optimized = true;
                    }
                    if i < tab_count - 1 {
                        ui.same_line();
                    }
                }
                ui.spacing();
            }

            if let Some(spec_name) = locked_spec_name.as_deref() {
                ui.text_colored(theme::OPTIMIZED, format!("Locked: {}", spec_name));
                ui.same_line();
                if ui.small_button("Unlock##improve") {
                    state.main.build_locks.specs[2] = None;
                }
                ui.same_line_with_spacing(0.0, 12.0);
            }
            crate::ui::comparison::render_result_pane_tabs(
                ui,
                &mut state.main.comparison.result_pane,
            );
            if state.main.comparison.result_pane == ResultPane::Build {
                ui.same_line_with_spacing(0.0, 16.0);
                crate::ui::gear_sheet::render_view_toggle(
                    ui,
                    &mut state.main.comparison.show_optimized,
                );
            }
            ui.spacing();

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
                        let viewing = state.main.comparison.show_optimized;
                        if viewing {
                            build_display::render_suggestion_skills(ui, &suggestion, db_ref);
                        } else {
                            build_display::render_build_skills(ui, &build, db_ref);
                        }
                        ui.spacing();
                        if viewing {
                            lock_panel::render_optimized_specs_panel(
                                ui,
                                db_ref.map(|db| db as &gw2_optimizer::gamedb::GameDb),
                                &suggestion.specializations,
                                "OPTIMIZED SPECS & TRAITS",
                            );
                        } else {
                            let mut specs_open = true;
                            lock_panel::render_lock_panel(
                                ui,
                                &mut state.main.build_locks,
                                &mut specs_open,
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
                    build_display::render_build_skills(ui, &build, state.main.game_db.as_ref());
                    {
                        let db_ref = state.main.game_db.as_ref();
                        let mut specs_open = true;
                        lock_panel::render_lock_panel(
                            ui,
                            &mut state.main.build_locks,
                            &mut specs_open,
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
}
