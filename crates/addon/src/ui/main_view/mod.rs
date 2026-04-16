use nexus::imgui::{ChildWindow, ComboBox, Selectable, Ui};

use crate::state::{AddonState, MainTab};
use gw2_optimizer::scoring::OptimizationWeights;

mod build_display;
mod character;
pub mod lock_panel;
mod optimization;
mod resolution;
mod stats;
mod tabs;

// ─── Color palette (shared with build_display) ───
const HEADER_BG: [f32; 4] = [0.18, 0.16, 0.10, 0.95];
const ACCENT_COLOR: [f32; 4] = [0.7, 0.55, 0.15, 0.6];

pub fn render_main(ui: &Ui, state: &mut AddonState) {
    // Force arrow cursor (prevent hand/drag cursor from Nexus overlay)
    ui.set_mouse_cursor(Some(nexus::imgui::MouseCursor::Arrow));

    // Apply global UI scale (text + element sizing)
    let scale = state.config.font_scale;
    ui.set_window_font_scale(scale);
    // Scale element sizes proportionally
    let _s1 = ui.push_style_var(nexus::imgui::StyleVar::FramePadding([
        4.0 * scale,
        3.0 * scale,
    ]));
    let _s2 = ui.push_style_var(nexus::imgui::StyleVar::ItemSpacing([
        8.0 * scale,
        4.0 * scale,
    ]));
    let _s3 = ui.push_style_var(nexus::imgui::StyleVar::ItemInnerSpacing([
        4.0 * scale,
        4.0 * scale,
    ]));

    // Trigger character load on first render
    if state.main.characters.is_empty() && !state.main.characters_loading {
        character::load_characters(state);
    }

    // Load GameDb once on first entry (S11-T06)
    if state.main.game_db.is_none() && !state.main.game_db_loading {
        stats::load_game_db(state);
    }

    // Periodic API health check (~every 60s at 60fps)
    state.main.api_status_frames += 1;
    if state.main.api_status_frames >= 3600
        || state.main.api_status == crate::state::ApiStatus::Unknown
    {
        if !state.main.api_health_checking {
            stats::check_api_health(state);
            state.main.api_status_frames = 0;
        }
    }

    // Auto-dismiss save status after ~180 frames (~3s at 60fps)
    if state.main.save_status.is_some() {
        state.main.save_status_frames += 1;
        if state.main.save_status_frames > 180 {
            state.main.save_status = None;
            state.main.save_status_frames = 0;
        }
    }

    // Chat timeout recovery: if waiting > 1800 frames (~30s), unblock
    if state.main.chat.waiting {
        state.main.chat_wait_frames += 1;
        if state.main.chat_wait_frames > 1800 {
            state.main.chat.waiting = false;
            state.main.chat_wait_frames = 0;
            crate::ui::chat_bar::add_ai_response(
                &mut state.main.chat,
                "Request timed out. Please try again.".into(),
            );
        }
    } else {
        state.main.chat_wait_frames = 0;
    }

    // ── Top status bar: API health + loading + errors ──
    render_top_status_bar(ui, state);

    // ── Horizontal tab bar (main navigation) ──
    render_top_tabs(ui, state);

    ui.spacing();

    // ── Two-column layout: left dynamic panel + center content ──
    let left_panel_width = state.config.left_panel_width;
    let pad = state.config.panel_padding;
    let content_indent = state.config.content_indent;
    let avail = ui.content_region_avail();
    // Leave a small footer margin so the bottom row (Save/Clear buttons, etc.)
    // is not tight against the game window edge or clipped on some resolutions.
    let child_height = (avail[1] - 6.0).max(0.0);

    ChildWindow::new("##left_panel_global")
        .size([left_panel_width, child_height])
        .build(ui, || {
            ui.dummy([0.0, 2.0]);
            ui.indent_by(pad);
            render_left_panel(ui, state);
            ui.unindent_by(pad);
        });

    ui.same_line();

    ChildWindow::new("##center_content")
        .size([0.0, child_height])
        .build(ui, || {
            ui.indent_by(content_indent);
            render_main_content(ui, state);
            ui.unindent_by(content_indent);
        });
}

/// Top status bar: API health indicator + loading banner + error bar.
fn render_top_status_bar(ui: &Ui, state: &mut AddonState) {
    // Draw subtle background for the status row
    {
        let start = ui.cursor_screen_pos();
        let width = ui.content_region_avail()[0];
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [start[0] - 1.0, start[1]],
                [start[0] + width + 1.0, start[1] + 18.0],
                HEADER_BG,
            )
            .filled(true)
            .build();
    }

    // API health indicator
    let (dot, label, color) = match state.main.api_status {
        crate::state::ApiStatus::Unknown => ("[o]", "Checking...", [0.5, 0.5, 0.5, 1.0]),
        crate::state::ApiStatus::Online => ("[+]", "API Online", [0.0, 0.8, 0.0, 1.0]),
        crate::state::ApiStatus::Degraded => ("[~]", "API Slow", [1.0, 0.8, 0.0, 1.0]),
        crate::state::ApiStatus::Offline => ("[-]", "API Offline", [1.0, 0.2, 0.0, 1.0]),
    };
    ui.text_colored(color, &format!(" {} {}", dot, label));
    if ui.is_item_hovered() {
        ui.tooltip_text(match state.main.api_status {
            crate::state::ApiStatus::Unknown => "Checking GW2 API availability...",
            crate::state::ApiStatus::Online => "GW2 API is responding normally.",
            crate::state::ApiStatus::Degraded => "GW2 API is responding slowly (>5s).",
            crate::state::ApiStatus::Offline => {
                "GW2 API is unavailable. Cached data is being used."
            }
        });
    }

    // Loading banner (GameDb)
    if state.main.game_db_loading {
        ui.same_line();
        let stage = &state.main.game_refresh_stage;
        if !stage.is_empty() {
            ui.text_colored([1.0, 1.0, 0.0, 1.0], &format!("| {}", stage));
        } else {
            ui.text_colored([1.0, 1.0, 0.0, 1.0], "| Loading game data...");
        }
    }

    // Optimization progress
    if state.main.optimizing {
        ui.same_line();
        ui.text_colored(
            [1.0, 1.0, 0.0, 1.0],
            &format!("| {}", state.main.optimize_stage),
        );
    }

    // Error bar (dismissible)
    if let Some(ref err) = state.main.error {
        ui.text_colored([1.0, 0.3, 0.0, 1.0], &format!("  [!] {}", err));
        ui.same_line();
        if ui.small_button("Dismiss##err") {
            state.main.error = None;
        }
    }

    // Accent line below status bar
    {
        let pos = ui.cursor_screen_pos();
        let width = ui.content_region_avail()[0];
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_line(
                [pos[0] - 1.0, pos[1]],
                [pos[0] + width + 1.0, pos[1]],
                ACCENT_COLOR,
            )
            .thickness(1.0)
            .build();
    }
    ui.dummy([0.0, 2.0]);
}

/// Prominent animated progress banner for optimization.
pub(super) fn render_optimization_progress(ui: &Ui, stage: &str, frame_count: i32) {
    let frame_count = frame_count as u32;
    ui.spacing();
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];

    {
        let draw_list = ui.get_window_draw_list();

        // Dark card background
        draw_list
            .add_rect(
                [start[0], start[1]],
                [start[0] + width, start[1] + 62.0],
                [0.10, 0.10, 0.16, 0.95],
            )
            .filled(true)
            .rounding(6.0)
            .build();

        // Animated accent stripe at top (sweeping cyan glow)
        let cycle = (frame_count % 180) as f32 / 180.0;
        let glow_x = start[0] + cycle * width;
        let glow_half = width * 0.15;
        draw_list
            .add_rect(
                [(glow_x - glow_half).max(start[0]), start[1]],
                [(glow_x + glow_half).min(start[0] + width), start[1] + 3.0],
                [0.3, 0.9, 1.0, 0.9],
            )
            .filled(true)
            .build();
        // Dimmer full-width stripe underneath
        draw_list
            .add_rect(
                [start[0], start[1]],
                [start[0] + width, start[1] + 3.0],
                [0.2, 0.5, 0.7, 0.3],
            )
            .filled(true)
            .build();

        // Spinner dots (3 pulsing dots)
        let dot_y = start[1] + 18.0;
        for i in 0..3u32 {
            let phase = ((frame_count + i * 20) % 60) as f32 / 60.0;
            let alpha = 0.3 + 0.7 * (phase * std::f32::consts::PI * 2.0).sin().abs();
            let dot_x = start[0] + 16.0 + i as f32 * 12.0;
            draw_list
                .add_circle([dot_x, dot_y], 3.5, [0.3_f32, 0.8, 1.0, alpha])
                .filled(true)
                .build();
        }

        // "OPTIMIZING..." title
        draw_list.add_text(
            [start[0] + 56.0, start[1] + 10.0],
            [0.4, 0.9, 1.0, 1.0],
            "OPTIMIZING...",
        );

        // Stage detail text
        let detail = if stage.is_empty() {
            "Starting optimization pipeline..."
        } else {
            stage
        };
        draw_list.add_text(
            [start[0] + 16.0, start[1] + 34.0],
            [0.6, 0.65, 0.75, 1.0],
            detail,
        );

        // Animated progress bar at bottom
        let bar_y = start[1] + 56.0;
        // Background
        draw_list
            .add_rect(
                [start[0] + 8.0, bar_y],
                [start[0] + width - 8.0, bar_y + 4.0],
                [0.15, 0.15, 0.2, 0.6],
            )
            .filled(true)
            .rounding(2.0)
            .build();
        // Sweeping fill
        let bar_inner = width - 16.0;
        let sweep_width = bar_inner * 0.35;
        let sweep_cycle = ((frame_count % 150) as f32) / 150.0;
        let sweep_x = start[0] + 8.0 + sweep_cycle * (bar_inner + sweep_width) - sweep_width;
        draw_list
            .add_rect(
                [sweep_x.max(start[0] + 8.0), bar_y],
                [
                    (sweep_x + sweep_width).min(start[0] + width - 8.0),
                    bar_y + 4.0,
                ],
                [0.3, 0.8, 1.0, 0.7],
            )
            .filled(true)
            .rounding(2.0)
            .build();

        // Border
        draw_list
            .add_rect(
                [start[0], start[1]],
                [start[0] + width, start[1] + 62.0],
                [0.25, 0.5, 0.7, 0.3],
            )
            .rounding(6.0)
            .build();
    }

    ui.dummy([0.0, 66.0]);
    ui.spacing();
}

/// Horizontal tab bar for main navigation (styled buttons with active indicator).
fn render_top_tabs(ui: &Ui, state: &mut AddonState) {
    let tabs = [
        (MainTab::NewBuild, "New Build"),
        (MainTab::Improve, "Improve Build"),
        (MainTab::SaveLoad, "Save / Load"),
        (MainTab::Settings, "Settings"),
    ];

    let start_y = ui.cursor_screen_pos()[1];

    for (i, (tab, label)) in tabs.iter().enumerate() {
        if i > 0 {
            ui.same_line();
        }

        let is_active = state.main.active_tab == *tab;
        let btn_pos = ui.cursor_screen_pos();

        if is_active {
            // Active tab: brighter text + underline
            ui.text_colored([1.0, 0.88, 0.35, 1.0], label);
        } else {
            // Inactive: dim + clickable
            ui.text_colored([0.6, 0.55, 0.45, 1.0], label);
        }

        // Make the text clickable
        if ui.is_item_hovered() {
            if !is_active {
                // Hover underline
                let text_size = ui.calc_text_size(label);
                let draw_list = ui.get_window_draw_list();
                draw_list
                    .add_line(
                        [btn_pos[0], btn_pos[1] + text_size[1] + 1.0],
                        [btn_pos[0] + text_size[0], btn_pos[1] + text_size[1] + 1.0],
                        [0.6, 0.5, 0.3, 0.5],
                    )
                    .thickness(1.0)
                    .build();
            }
            if ui.is_item_clicked() {
                state.main.active_tab = tab.clone();
                // Clear locks when switching to New Build, auto-populate when switching to Improve
                match tab {
                    MainTab::NewBuild => {
                        state.main.build_locks = gw2_core::types::BuildLocks::default();
                    }
                    MainTab::Improve => {
                        if let Some(ref build) = state.main.current_build {
                            let build_clone = build.clone();
                            resolution::auto_populate_locks(
                                &build_clone,
                                &mut state.main.build_locks,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        // Active tab: gold underline
        if is_active {
            let text_size = ui.calc_text_size(label);
            let draw_list = ui.get_window_draw_list();
            draw_list
                .add_line(
                    [btn_pos[0], btn_pos[1] + text_size[1] + 2.0],
                    [btn_pos[0] + text_size[0], btn_pos[1] + text_size[1] + 2.0],
                    [1.0, 0.75, 0.2, 0.9],
                )
                .thickness(2.0)
                .build();
        }

        // Tab separator
        if i < tabs.len() - 1 {
            ui.same_line();
            ui.text_colored([0.3, 0.3, 0.3, 0.5], "|");
        }
    }

    let _ = start_y; // suppress unused warning
    ui.dummy([0.0, 4.0]);
}

/// Dynamic left panel: content varies by active tab.
fn render_left_panel(ui: &Ui, state: &mut AddonState) {
    // ── Character section (always visible except Settings) ──
    if state.main.active_tab != MainTab::Settings {
        render_left_character_section(ui, state);
    }

    match state.main.active_tab {
        MainTab::NewBuild | MainTab::Improve => {
            render_left_build_controls(ui, state);
        }
        MainTab::SaveLoad => {
            // Minimal: just refresh button
            ui.spacing();
            if state.main.characters_loading {
                let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
                ui.button_with_size("Refreshing...", [-1.0, 0.0]);
                style.pop();
            } else if ui.button_with_size("Refresh Data", [-1.0, 0.0]) {
                character::load_characters(state);
            }
        }
        MainTab::Settings => {
            // Settings info
            render_left_section_header(ui, "INFO", state.config.section_spacing);
            ui.text_colored([0.6, 0.6, 0.7, 1.0], "  GW2 Build Optimizer");
            ui.text_colored([0.5, 0.5, 0.5, 1.0], "  v1.0.0");
            ui.spacing();
            let provider_label = state.config.active_provider.label();
            ui.text_colored([0.5, 0.5, 0.5, 1.0], &format!("  AI: {}", provider_label));
        }
    }
}

/// Render a compact section header with accent line in the left panel.
pub(super) fn render_left_section_header(ui: &Ui, title: &str, spacing: f32) {
    ui.dummy([0.0, spacing]); // gap above
    {
        let pos = ui.cursor_screen_pos();
        let width = ui.content_region_avail()[0];
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [pos[0], pos[1]],
                [pos[0] + width, pos[1] + 18.0],
                [0.22, 0.19, 0.10, 0.9],
            )
            .filled(true)
            .build();
        draw_list.add_text([pos[0] + 6.0, pos[1] + 2.0], [0.85, 0.72, 0.3, 1.0], title);
    }
    ui.dummy([0.0, 20.0]);
    ui.dummy([0.0, spacing * 0.5]); // gap below
}

/// WvW combat sub-role selector: Roaming / Havoc / Zerg.
/// Appears only when WvW mode is active. Changing sub-role updates wvw_combat_tier
/// and reloads weights from the matching objective profile.
fn render_wvw_sub_role(ui: &Ui, state: &mut AddonState) {
    use gw2_optimizer::scenario::CombatTier;

    render_left_section_header(ui, "WVW SUB-ROLE", state.config.section_spacing);
    ui.spacing();

    let tiers = [
        (CombatTier::Solo, "Roaming", "Solo / small-scale dueling"),
        (CombatTier::Party, "Havoc", "5-15 player small group"),
        (CombatTier::Squad, "Zerg", "Large squad / blob"),
    ];
    for (tier, label, tooltip) in &tiers {
        let selected = state.main.wvw_combat_tier == *tier;
        if ui.radio_button_bool(label, selected) && !selected {
            state.main.wvw_combat_tier = *tier;
            // Load the matching WvW profile weights for this tier
            let new_weights = match tier {
                CombatTier::Solo => gw2_optimizer::scenario::RoleObjective::WvWRoamer
                    .to_weights(&gw2_core::types::GameMode::WvW),
                CombatTier::Party => gw2_optimizer::scoring::OptimizationWeights::default_for_mode("WvW"),
                CombatTier::Squad => gw2_optimizer::scenario::RoleObjective::WvWZergDps
                    .to_weights(&gw2_core::types::GameMode::WvW),
            };
            state.main.weights = new_weights;
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
        if ui.is_item_hovered() {
            ui.tooltip_text(tooltip);
        }
        // Stack vertically to avoid clipping on narrow left panels.
        ui.spacing();
    }
}

/// Character picker + build/equip template dropdowns.
fn render_left_character_section(ui: &Ui, state: &mut AddonState) {
    render_left_section_header(ui, "CHARACTER", state.config.section_spacing);
    ui.spacing();

    // Character dropdown
    ui.set_next_item_width(-1.0);
    let preview = state
        .main
        .selected_character
        .and_then(|i| state.main.characters.get(i))
        .cloned()
        .unwrap_or_else(|| {
            if state.main.characters_loading {
                "Loading...".into()
            } else if state.main.characters.is_empty() {
                "No characters".into()
            } else {
                "Select...".into()
            }
        });

    let chars_snapshot = state.main.characters.clone();
    let mut new_selection: Option<(usize, String)> = None;

    if let Some(_combo) = ComboBox::new("##char_select")
        .preview_value(&preview)
        .begin(ui)
    {
        for (i, name) in chars_snapshot.iter().enumerate() {
            let selected = state.main.selected_character == Some(i);
            if Selectable::new(name).selected(selected).build(ui) {
                if state.main.selected_character != Some(i) {
                    new_selection = Some((i, name.clone()));
                }
            }
        }
    }

    if let Some((idx, name)) = new_selection {
        state.main.selected_character = Some(idx);
        state.main.current_build = None;
        state.main.current_stats = None;
        state.main.build_tabs.clear();
        state.main.equipment_tabs.clear();
        state.main.selected_build_tab = None;
        state.main.selected_equipment_tab = None;
        state.main.build_chat_code = None;
        // Clear prior character's suggestions, combat metrics, and locks — the new
        // character may be a different profession with different specs entirely.
        state.main.comparison.suggestions.clear();
        state.main.comparison.selected_suggestion = 0;
        state.main.comparison.error = None;
        state.main.comparison.current_combat_solo = None;
        state.main.comparison.current_combat_party = None;
        state.main.comparison.current_combat_squad = None;
        state.main.build_locks = gw2_core::types::BuildLocks::default();
        character::load_character_tabs(state, name);
    }

    // Build Template dropdown
    if !state.main.build_tabs.is_empty() {
        ui.spacing();
        ui.text_colored([0.6, 0.6, 0.7, 1.0], "Build:");
        ui.set_next_item_width(-1.0);
        let bt_preview = state
            .main
            .selected_build_tab
            .and_then(|i| state.main.build_tabs.get(i))
            .map(|t| {
                let name = t.build.name.as_deref().unwrap_or("Unnamed");
                format!("Tab {}: {}", t.tab, name)
            })
            .unwrap_or_else(|| "Select...".into());

        let bt_labels: Vec<(usize, String)> = state
            .main
            .build_tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let name = t.build.name.as_deref().unwrap_or("Unnamed");
                (i, format!("Tab {}: {}", t.tab, name))
            })
            .collect();

        let mut bt_changed: Option<usize> = None;
        if !bt_labels.is_empty() {
            if let Some(_combo) = ComboBox::new("##build_tab_select")
                .preview_value(&bt_preview)
                .begin(ui)
            {
                for (i, label) in &bt_labels {
                    let selected = state.main.selected_build_tab == Some(*i);
                    if Selectable::new(label).selected(selected).build(ui) {
                        if state.main.selected_build_tab != Some(*i) {
                            bt_changed = Some(*i);
                        }
                    }
                }
            }
        }

        if let Some(idx) = bt_changed {
            state.main.selected_build_tab = Some(idx);
            state.main.comparison.suggestions.clear();
            state.main.comparison.selected_suggestion = 0;
            state.main.comparison.error = None;
            character::update_build_chat_code(state);
            resolution::resolve_selected_build(state);
        }
    }

    // Equipment Template dropdown
    if !state.main.equipment_tabs.is_empty() {
        ui.text_colored([0.6, 0.6, 0.7, 1.0], "Equipment:");
        ui.set_next_item_width(-1.0);
        let et_preview = state
            .main
            .selected_equipment_tab
            .and_then(|i| state.main.equipment_tabs.get(i))
            .map(|t| {
                let name = t.name.as_deref().unwrap_or("Unnamed");
                format!("Tab {}: {}", t.tab, name)
            })
            .unwrap_or_else(|| "Select...".into());

        let et_labels: Vec<(usize, String)> = state
            .main
            .equipment_tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let name = t.name.as_deref().unwrap_or("Unnamed");
                (i, format!("Tab {}: {}", t.tab, name))
            })
            .collect();

        let mut et_changed: Option<usize> = None;
        if !et_labels.is_empty() {
            if let Some(_combo) = ComboBox::new("##equip_tab_select")
                .preview_value(&et_preview)
                .begin(ui)
            {
                for (i, label) in &et_labels {
                    let selected = state.main.selected_equipment_tab == Some(*i);
                    if Selectable::new(label).selected(selected).build(ui) {
                        if state.main.selected_equipment_tab != Some(*i) {
                            et_changed = Some(*i);
                        }
                    }
                }
            }
        }

        if let Some(idx) = et_changed {
            state.main.selected_equipment_tab = Some(idx);
            state.main.comparison.suggestions.clear();
            state.main.comparison.selected_suggestion = 0;
            state.main.comparison.error = None;
            resolution::resolve_selected_build(state);
        }
    }

    // Build resolution indicator
    if state.main.build_loading {
        ui.text_colored([1.0, 1.0, 0.0, 1.0], "Resolving build...");
    }

    // Build chat code display with Copy button
    if let Some(ref code) = state.main.build_chat_code.clone() {
        ui.spacing();
        ui.text_colored([0.6, 0.6, 0.7, 1.0], "Chat Code:");
        ui.same_line();
        if state.main.copy_feedback_frames > 0 {
            ui.text_colored([0.0, 1.0, 0.0, 1.0], "Copied!");
            state.main.copy_feedback_frames -= 1;
        } else if ui.small_button("Copy##chatcode") {
            ui.set_clipboard_text(code);
            state.main.copy_feedback_frames = 120;
        }
        ui.set_next_item_width(-1.0);
        let mut code_buf = code.clone();
        ui.input_text("##chat_code_display", &mut code_buf)
            .read_only(true)
            .build();
    }
}

/// Build controls: game mode, radar chart, presets, action buttons.
fn render_left_build_controls(ui: &Ui, state: &mut AddonState) {
    // Game mode selector
    render_left_section_header(ui, "GAME MODE", state.config.section_spacing);
    ui.spacing();
    for mode in &gw2_core::types::GameMode::ALL {
        let selected = state.main.game_mode == *mode;
        if ui.radio_button_bool(mode.label(), selected) {
            state.main.game_mode = mode.clone();
            state.main.weights = OptimizationWeights::default_for_mode(mode.label());
            // Clear suggestions from the prior mode — PvE results are invalid for PvP
            // and vice versa. Locks are also reset since mode changes affect stat scaling.
            state.main.comparison.suggestions.clear();
            state.main.comparison.selected_suggestion = 0;
            state.main.comparison.error = None;
            state.main.build_locks = gw2_core::types::BuildLocks::default();
            resolution::resolve_selected_build(state);
        }
        ui.same_line();
    }
    ui.new_line();

    // WvW sub-role selector (only shown when WvW mode is active)
    if state.main.game_mode == gw2_core::types::GameMode::WvW {
        render_wvw_sub_role(ui, state);
    }

    // Radar chart + presets
    render_left_section_header(ui, "OPTIMIZATION WEIGHTS", state.config.section_spacing);

    // Compute overlays for radar chart
    let current_axes = state
        .main
        .comparison
        .current_combat_solo
        .as_ref()
        .map(crate::ui::radar_chart::compute_axes_from_metrics);
    let optimized_axes = if !state.main.comparison.suggestions.is_empty() {
        let idx = state
            .main
            .comparison
            .selected_suggestion
            .min(state.main.comparison.suggestions.len() - 1);
        state.main.comparison.suggestions[idx]
            .combat_solo
            .as_ref()
            .map(crate::ui::radar_chart::compute_axes_from_metrics)
    } else {
        None
    };

    let show_current = state.main.active_tab == MainTab::Improve;
    let _chart_modified = crate::ui::radar_chart::render_radar_chart(
        ui,
        &mut state.main.weights,
        &mut state.main.radar_dragging,
        if show_current {
            current_axes.as_ref()
        } else {
            None
        },
        optimized_axes.as_ref(),
    );
    if current_axes.is_some() || optimized_axes.is_some() {
        crate::ui::radar_chart::render_legend(
            ui,
            show_current && current_axes.is_some(),
            optimized_axes.is_some(),
        );
    }

    // Preset buttons
    ui.spacing();
    if let Some(preset) = crate::ui::radar_chart::render_presets(ui) {
        state.main.weights = preset;
    }

    // Summary
    ui.spacing();
    let summary = state.main.weights.summary_label();
    let focus_label = if state.main.game_mode == gw2_core::types::GameMode::WvW {
        format!("  Focus: {} — {}", state.main.wvw_combat_tier.label(), summary)
    } else {
        format!("  Focus: {}", summary)
    };
    ui.text_colored([0.6, 0.8, 1.0, 1.0], &focus_label);

    // Action buttons
    render_left_section_header(ui, "ACTIONS", state.config.section_spacing);

    let is_improve = state.main.active_tab == MainTab::Improve;
    let btn_label_owned;
    let btn_label: &str = if is_improve {
        "Improve Build"
    } else if let Some(role) = state.main.selected_role {
        btn_label_owned = format!("Optimize: {}", role.label());
        &btn_label_owned
    } else {
        "Optimize Build"
    };
    let disabled = state.main.optimizing
        || state.main.game_db.is_none()
        || (is_improve && state.main.current_build.is_none());

    if disabled {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size(btn_label, [-1.0, 28.0]);
        style.pop();
        if ui.is_item_hovered() {
            ui.tooltip_text(if state.main.optimizing {
                "Optimization in progress..."
            } else if state.main.game_db.is_none() {
                "Waiting for game data to load..."
            } else {
                "Select a character first"
            });
        }
    } else if ui.button_with_size(btn_label, [-1.0, 28.0]) {
        if is_improve {
            let profession_name = state
                .main
                .current_build
                .as_ref()
                .map(|b| b.profession.clone());
            if let Some(ref prof_name) = profession_name {
                optimization::start_optimization_with_profession(state, prof_name);
            }
        } else {
            optimization::start_optimization(state);
        }
    }

    // Refresh button
    ui.dummy([0.0, 2.0]);
    if state.main.characters_loading {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size("Refreshing...", [-1.0, 0.0]);
        style.pop();
    } else if ui.button_with_size("Refresh Data", [-1.0, 0.0]) {
        character::load_characters(state);
    }
    ui.dummy([0.0, 4.0]);
}

fn render_main_content(ui: &Ui, state: &mut AddonState) {
    match state.main.active_tab {
        MainTab::NewBuild => {
            tabs::new_build::render_new_build_tab(ui, state);
        }
        MainTab::Improve => {
            tabs::improve::render_improve_tab(ui, state);
        }
        MainTab::SaveLoad => {
            tabs::saveload::render_saveload_tab(ui, state);
        }
        MainTab::Settings => {
            tabs::settings::render_settings_tab(ui, state);
        }
    }
}

