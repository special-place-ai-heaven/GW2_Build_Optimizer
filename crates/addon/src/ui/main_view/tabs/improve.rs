//! Improve tab — current vs optimized side-by-side, lock panel, chat bar.

use nexus::imgui::{ChildWindow, Selectable, Ui};

use crate::state::AddonState;

use super::{build_display, lock_panel, optimization, render_optimization_progress};

pub(in crate::ui::main_view) fn render_improve_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.build_loading {
        ui.text_colored([1.0, 1.0, 0.0, 1.0], "Resolving build from API data...");
        return;
    }

    // Error display
    if let Some(err) = state.main.comparison.error.clone() {
        ui.text_colored([1.0, 0.3, 0.0, 1.0], &format!("[!] {}", err));
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
                [0.7, 0.9, 0.7, 1.0],
                &format!("  \u{1F512} Locked to: {}", spec_name),
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
    let avail = ui.content_region_avail();
    let _panel_height = avail[1] - 40.0; // leave room for chat bar

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

            let avail_after = ui.content_region_avail();
            let scroll_height = avail_after[1] - 60.0; // room for save + chat

            let idx = state
                .main
                .comparison
                .selected_suggestion
                .min(state.main.comparison.suggestions.len() - 1);
            let suggestion = state.main.comparison.suggestions[idx].clone();
            let current_combat_solo = state.main.comparison.current_combat_solo.clone();

            // ═══ Section-by-section grid layout ═══
            // Each section pair uses its own columns(2) block, ensuring
            // horizontal alignment: columns(1) at section end sets Y to MAX.
            ChildWindow::new("##improve_scroll")
                .size([avail[0], scroll_height])
                .build(ui, || {
                    let col_avail = ui.content_region_avail()[0];
                    let col1_offset = (col_avail + 12.0) / 2.0;

                    // ── SPEC & TRAIT SECTION ──
                    ui.columns(2, "##imp_spec", false);
                    ui.set_column_offset(1, col1_offset);
                    // LEFT: Current Build header + interactive lock panel
                    build_display::render_card_header(ui, "CURRENT BUILD", [0.5, 0.7, 1.0, 1.0]);
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
                    ui.next_column();
                    ui.indent_by(6.0);
                    // RIGHT: Optimized Build header + read-only specs
                    build_display::render_card_header(ui, "OPTIMIZED BUILD", [0.3, 1.0, 0.5, 1.0]);
                    crate::ui::comparison::render_chat_code_copy(
                        ui,
                        suggestion.chat_code.as_deref(),
                        "improve",
                    );
                    {
                        let db_ref = state.main.game_db.as_ref();
                        lock_panel::render_optimized_specs_panel(
                            ui,
                            db_ref.map(|db| db as &gw2_optimizer::gamedb::GameDb),
                            &suggestion.specializations,
                        );
                    }
                    ui.unindent_by(6.0);
                    ui.columns(1, "##imp_spec_end", false);

                    // ── SKILLS SECTION ──
                    ui.columns(2, "##imp_skills", false);
                    ui.set_column_offset(1, col1_offset);
                    build_display::render_build_skills(ui, &build);
                    ui.next_column();
                    ui.indent_by(6.0);
                    build_display::render_suggestion_skills(ui, &suggestion);
                    ui.unindent_by(6.0);
                    ui.columns(1, "##imp_skills_end", false);

                    // ── WEAPONS SECTION ──
                    ui.columns(2, "##imp_weapons", false);
                    ui.set_column_offset(1, col1_offset);
                    build_display::render_build_weapons(ui, &build);
                    ui.next_column();
                    ui.indent_by(6.0);
                    build_display::render_suggestion_weapons(ui, &suggestion);
                    ui.unindent_by(6.0);
                    ui.columns(1, "##imp_weapons_end", false);

                    // ── GEAR SECTION ──
                    ui.columns(2, "##imp_gear", false);
                    ui.set_column_offset(1, col1_offset);
                    build_display::render_build_gear(ui, &build);
                    ui.next_column();
                    ui.indent_by(6.0);
                    build_display::render_suggestion_gear(ui, &suggestion);
                    ui.unindent_by(6.0);
                    ui.columns(1, "##imp_gear_end", false);

                    // ── STATS SECTION ──
                    ui.columns(2, "##imp_stats", false);
                    ui.set_column_offset(1, col1_offset);
                    build_display::render_build_stats(
                        ui,
                        stats.as_ref(),
                        suggestion.estimated_stats.as_ref(),
                    );
                    ui.next_column();
                    ui.indent_by(6.0);
                    build_display::render_suggestion_stats(ui, &suggestion, stats.as_ref());
                    ui.unindent_by(6.0);
                    ui.columns(1, "##imp_stats_end", false);

                    // ── COMBAT PERFORMANCE SECTION ──
                    ui.columns(2, "##imp_combat", false);
                    ui.set_column_offset(1, col1_offset);
                    build_display::render_build_combat(
                        ui,
                        current_combat_solo.as_ref(),
                        suggestion.combat_solo.as_ref(),
                    );
                    ui.next_column();
                    ui.indent_by(6.0);
                    build_display::render_suggestion_combat(
                        ui,
                        &suggestion,
                        current_combat_solo.as_ref(),
                    );
                    ui.unindent_by(6.0);
                    ui.columns(1, "##imp_combat_end", false);

                    // ── ROTATION BREAKDOWN (full-width) ──
                    if let Some(ref rotation) = suggestion.rotation {
                        build_display::render_rotation_section(ui, rotation);
                    }

                    // ── WHY THIS BUILD (full-width, different background) ──
                    let explanation = if !suggestion.synergy_explanation.is_empty() {
                        &suggestion.synergy_explanation
                    } else {
                        &suggestion.explanation
                    };
                    build_display::render_why_section(ui, explanation, &suggestion.changes_made);
                });
        } else {
            // No suggestion yet — show current build with lock panel (full width)
            let avail_after = ui.content_region_avail();
            let scroll_height = avail_after[1] - 40.0;

            ChildWindow::new("##improve_single")
                .size([avail[0], scroll_height])
                .build(ui, || {
                    build_display::render_card_header(ui, "CURRENT BUILD", [0.5, 0.7, 1.0, 1.0]);
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
                    build_display::render_build_card_no_specs(ui, &build, stats.as_ref(), None);
                });
        }
    } else if state.main.selected_character.is_some() {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "Loading character build...");
    } else {
        ui.text_colored(
            [0.5, 0.5, 0.5, 1.0],
            "Select a character from the left panel.",
        );
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
