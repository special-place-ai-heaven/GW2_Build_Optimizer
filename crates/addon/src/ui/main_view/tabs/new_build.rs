//! New Build tab — role picker + comparison view + chat refinement.

use nexus::imgui::Ui;

use crate::state::AddonState;

use super::{optimization, render_left_section_header, render_optimization_progress};

/// Role picker grid for the 'New Build' tab.
/// Shows 8 generic archetypes + WvW/PvP-specific roles when appropriate.
/// Selecting a role updates weights, wvw_combat_tier, and selected_role in state.
fn render_role_picker(ui: &Ui, state: &mut AddonState) {
    use gw2_optimizer::scenario::RoleObjective;

    render_left_section_header(ui, "SELECT ROLE", state.config.section_spacing);
    ui.spacing();
    ui.text_colored([0.6, 0.6, 0.6, 1.0], "Choose what this build should do:");
    ui.spacing();

    // Generic roles (all modes)
    let generic_roles: &[(RoleObjective, &str)] = &[
        (RoleObjective::PowerDps, "Direct damage, burst & sustained strike DPS"),
        (RoleObjective::CondiDps, "Condition damage, DoTs and pressure"),
        (RoleObjective::Sustain, "Bruiser — balanced offense and self-healing"),
        (RoleObjective::Tank, "Maximum toughness and vitality for frontline"),
        (RoleObjective::Healer, "Party/squad healing and sustain"),
        (RoleObjective::Disabler, "CC, interrupts, and movement denial"),
        (RoleObjective::Buffer, "Boon generation and support for allies"),
    ];

    // WvW-specific roles (only shown in WvW mode)
    let wvw_roles: &[(RoleObjective, &str)] = &[
        (RoleObjective::WvWRoamer, "Solo / small-scale — mobility and self-sustain"),
        (RoleObjective::WvWZergDps, "Large squad — group damage and AoE pressure"),
        (RoleObjective::WvWZergSupport, "Squad healer/support — stability and boon generation"),
        (RoleObjective::WvWDisruptor, "Boon corruption, CC, and movement denial"),
    ];

    // PvP-specific roles (only shown in PvP mode)
    let pvp_roles: &[(RoleObjective, &str)] = &[
        (RoleObjective::PvPBurst, "Spike damage with CC setup"),
        (RoleObjective::PvPSustain, "Point-holder — bunker and boon generation"),
        (RoleObjective::PvPDisruptor, "CC and boon denial pressure"),
    ];

    let game_mode = state.main.game_mode.clone();
    let current = state.main.selected_role;

    let roles_to_show: Vec<(RoleObjective, &str)> = {
        let mut v: Vec<(RoleObjective, &str)> = generic_roles.to_vec();
        match game_mode {
            gw2_core::types::GameMode::WvW => v.extend_from_slice(wvw_roles),
            gw2_core::types::GameMode::PvP => v.extend_from_slice(pvp_roles),
            _ => {}
        }
        v
    };

    // Render as a two-column grid of selectable items
    let avail_w = ui.content_region_avail()[0];
    let btn_w = (avail_w - 8.0) / 2.0;

    for (i, (role, desc)) in roles_to_show.iter().enumerate() {
        let is_selected = current.map(|r| r == *role).unwrap_or(false);
        let col = i % 2;

        if col == 1 {
            ui.same_line_with_spacing(0.0, 8.0);
        }

        // Selected: brighter border + gold text
        let label = format!("{}##role_{}", role.label(), i);
        let (text_col, bg_col): ([f32; 4], [f32; 4]) = if is_selected {
            ([1.0, 0.88, 0.35, 1.0], [0.3, 0.25, 0.05, 0.9])
        } else {
            ([0.85, 0.80, 0.70, 1.0], [0.15, 0.14, 0.10, 0.8])
        };

        let _bg = ui.push_style_color(nexus::imgui::StyleColor::Button, bg_col);
        let _bg_h = ui.push_style_color(nexus::imgui::StyleColor::ButtonHovered, [bg_col[0] + 0.1, bg_col[1] + 0.1, bg_col[2] + 0.05, 1.0]);
        let _bg_a = ui.push_style_color(nexus::imgui::StyleColor::ButtonActive, [0.4, 0.33, 0.08, 1.0]);
        let _tc = ui.push_style_color(nexus::imgui::StyleColor::Text, text_col);

        if ui.button_with_size(&label, [btn_w, 28.0]) {
            // Apply selection
            state.main.selected_role = Some(*role);
            state.main.wvw_combat_tier = role.combat_tier();
            state.main.weights = role.to_weights(&game_mode);
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
        if ui.is_item_hovered() {
            ui.tooltip_text(desc);
        }

        drop(_tc); drop(_bg_a); drop(_bg_h); drop(_bg);
    }

    ui.spacing();
    // Show selected role hint
    if let Some(role) = current {
        ui.text_colored([0.6, 0.9, 0.6, 1.0], &format!("  Selected: {} — click 'Optimize Build' in the left panel", role.label()));
    } else {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "  Select a role above, then click 'Optimize Build'");
    }
    ui.spacing();
}

pub(in crate::ui::main_view) fn render_new_build_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.selected_character.is_none() {
        ui.text_colored(
            [0.6, 0.6, 0.7, 1.0],
            "Select a character from the left panel to create a new build.",
        );
        return;
    }

    // Show optimization error
    if let Some(err) = state.main.comparison.error.clone() {
        ui.text_colored([1.0, 0.3, 0.0, 1.0], &format!("[!] {}", err));
        ui.same_line();
        if ui.small_button("Dismiss##opt_err") {
            state.main.comparison.error = None;
        }
        ui.spacing();
    }

    // Optimization progress banner
    if state.main.optimizing {
        render_optimization_progress(ui, &state.main.optimize_stage, ui.frame_count());
    }

    // ── Role picker (shown until a result exists) ──────────────────────────
    if state.main.comparison.suggestions.is_empty() && !state.main.optimizing {
        render_role_picker(ui, state);
    }

    // Show comparison if suggestions exist
    if !state.main.comparison.suggestions.is_empty() {
        if let Some(ref build) = state.main.current_build {
            let stats = state.main.current_stats.clone();
            if let Some(new_idx) = crate::ui::comparison::render_comparison(
                ui,
                build,
                stats.as_ref(),
                &state.main.comparison,
            ) {
                state.main.comparison.selected_suggestion = new_idx;
            }
        } else {
            ui.text_colored([1.0, 1.0, 0.0, 1.0], "Waiting for build data to load...");
        }

        // Save Build + Clear
        super::saveload::render_save_build_ui(ui, state);
        ui.same_line();
        if ui.small_button("Clear Results") {
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
    }

    // Chat bar at bottom
    ui.spacing();
    if let Some(msg) = crate::ui::chat_bar::render_chat_bar(ui, &mut state.main.chat) {
        optimization::send_chat_message(state, msg);
    }
}
