//! New Build tab — scenario summary + comparison view + chat refinement.
//! Role lives in the left menu (shared across modes).

use nexus::imgui::Ui;

use crate::state::AddonState;
use crate::ui::theme;

use super::{optimization, render_optimization_progress};

fn render_scenario_ready(ui: &Ui, state: &AddonState) {
    let mode = state.main.game_mode.label();
    let role = state
        .main
        .selected_role
        .map(|r| r.play_label())
        .unwrap_or("pick a role");
    let line = if state.main.game_mode == gw2_core::types::GameMode::WvW {
        format!(
            "{} · {} · {}",
            mode,
            state.main.wvw_combat_tier.label(),
            role
        )
    } else {
        format!("{} · {}", mode, role)
    };
    theme::wrapped(ui, theme::GOLD, &line);
    ui.spacing();
    if state.main.selected_role.is_none() {
        theme::wrapped(ui, theme::MUTED, "Pick a role on the left, then Optimize.");
    } else {
        theme::wrapped(
            ui,
            theme::MUTED,
            "Same job in every mode — this mode sets the spec and weights.",
        );
        ui.spacing();
        theme::wrapped(
            ui,
            theme::MUTED,
            "Click Optimize in the left panel when you're ready.",
        );
    }
}

pub(in crate::ui::main_view) fn render_new_build_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.selected_character.is_none() {
        theme::wrapped(
            ui,
            theme::MUTED,
            "Select a character from the left panel to create a new build.",
        );
        return;
    }

    if let Some(err) = state.main.comparison.error.clone() {
        ui.text_colored(theme::ERR, format!("[!] {}", err));
        ui.same_line();
        if ui.small_button("Dismiss##opt_err") {
            state.main.comparison.error = None;
        }
        ui.spacing();
    }

    if state.main.optimizing {
        render_optimization_progress(ui, &state.main.optimize_stage, ui.frame_count());
    }

    if state.main.comparison.suggestions.is_empty() && !state.main.optimizing {
        render_scenario_ready(ui, state);
    }

    if !state.main.comparison.suggestions.is_empty() {
        let footer = 36.0 + crate::ui::chat_bar::reserved_height(&state.main.chat);
        let scroll_h = (ui.content_region_avail()[1] - footer).max(64.0);
        nexus::imgui::ChildWindow::new("##new_build_scroll")
            .size([0.0, scroll_h])
            .build(ui, || {
                if let Some(build) = state.main.current_build.clone() {
                    let stats = state.main.current_stats.clone();
                    crate::ui::comparison::render_comparison(
                        ui,
                        &build,
                        stats.as_ref(),
                        &mut state.main.comparison,
                        state.main.game_db.as_ref(),
                    );
                } else {
                    ui.text_colored(theme::WARN, "Waiting for build data to load...");
                }
            });

        super::saveload::render_save_build_ui(ui, state);
        ui.same_line();
        if ui.small_button("Clear Results") {
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
    }

    ui.spacing();
    if let Some(msg) = crate::ui::chat_bar::render_chat_bar(ui, &mut state.main.chat) {
        optimization::send_chat_message(state, msg);
    }
}
