use nexus::imgui::{ChildWindow, ComboBox, Selectable, Ui};
use base64::Engine as _;

use crate::state::{AddonState, MainTab};
use gw2_optimizer::scoring::OptimizationWeights;

mod build_display;
mod lock_panel;

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
    let _s1 = ui.push_style_var(nexus::imgui::StyleVar::FramePadding([4.0 * scale, 3.0 * scale]));
    let _s2 = ui.push_style_var(nexus::imgui::StyleVar::ItemSpacing([8.0 * scale, 4.0 * scale]));
    let _s3 = ui.push_style_var(nexus::imgui::StyleVar::ItemInnerSpacing([4.0 * scale, 4.0 * scale]));

    // Trigger character load on first render
    if state.main.characters.is_empty() && !state.main.characters_loading {
        load_characters(state);
    }

    // Load GameDb once on first entry (S11-T06)
    if state.main.game_db.is_none() && !state.main.game_db_loading {
        load_game_db(state);
    }

    // Periodic API health check (~every 60s at 60fps)
    state.main.api_status_frames += 1;
    if state.main.api_status_frames >= 3600 || state.main.api_status == crate::state::ApiStatus::Unknown {
        if !state.main.api_health_checking {
            check_api_health(state);
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

    ChildWindow::new("##left_panel_global")
        .size([left_panel_width, avail[1]])
        .build(ui, || {
            ui.dummy([0.0, 2.0]);
            ui.indent_by(pad);
            render_left_panel(ui, state);
            ui.unindent_by(pad);
        });

    ui.same_line();

    ChildWindow::new("##center_content")
        .size([0.0, avail[1]])
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
        draw_list.add_rect(
            [start[0] - 1.0, start[1]],
            [start[0] + width + 1.0, start[1] + 18.0],
            HEADER_BG,
        ).filled(true).build();
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
            crate::state::ApiStatus::Offline => "GW2 API is unavailable. Cached data is being used.",
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
        ui.text_colored([1.0, 1.0, 0.0, 1.0], &format!("| {}", state.main.optimize_stage));
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
        draw_list.add_line(
            [pos[0] - 1.0, pos[1]],
            [pos[0] + width + 1.0, pos[1]],
            ACCENT_COLOR,
        ).thickness(1.0).build();
    }
    ui.dummy([0.0, 2.0]);
}

/// Prominent animated progress banner for optimization.
fn render_optimization_progress(ui: &Ui, stage: &str, frame_count: i32) {
    let frame_count = frame_count as u32;
    ui.spacing();
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];

    {
        let draw_list = ui.get_window_draw_list();

        // Dark card background
        draw_list.add_rect(
            [start[0], start[1]],
            [start[0] + width, start[1] + 62.0],
            [0.10, 0.10, 0.16, 0.95],
        ).filled(true).rounding(6.0).build();

        // Animated accent stripe at top (sweeping cyan glow)
        let cycle = (frame_count % 180) as f32 / 180.0;
        let glow_x = start[0] + cycle * width;
        let glow_half = width * 0.15;
        draw_list.add_rect(
            [(glow_x - glow_half).max(start[0]), start[1]],
            [(glow_x + glow_half).min(start[0] + width), start[1] + 3.0],
            [0.3, 0.9, 1.0, 0.9],
        ).filled(true).build();
        // Dimmer full-width stripe underneath
        draw_list.add_rect(
            [start[0], start[1]],
            [start[0] + width, start[1] + 3.0],
            [0.2, 0.5, 0.7, 0.3],
        ).filled(true).build();

        // Spinner dots (3 pulsing dots)
        let dot_y = start[1] + 18.0;
        for i in 0..3u32 {
            let phase = ((frame_count + i * 20) % 60) as f32 / 60.0;
            let alpha = 0.3 + 0.7 * (phase * std::f32::consts::PI * 2.0).sin().abs();
            let dot_x = start[0] + 16.0 + i as f32 * 12.0;
            draw_list.add_circle(
                [dot_x, dot_y],
                3.5,
                [0.3_f32, 0.8, 1.0, alpha],
            ).filled(true).build();
        }

        // "OPTIMIZING..." title
        draw_list.add_text(
            [start[0] + 56.0, start[1] + 10.0],
            [0.4, 0.9, 1.0, 1.0],
            "OPTIMIZING...",
        );

        // Stage detail text
        let detail = if stage.is_empty() { "Starting optimization pipeline..." } else { stage };
        draw_list.add_text(
            [start[0] + 16.0, start[1] + 34.0],
            [0.6, 0.65, 0.75, 1.0],
            detail,
        );

        // Animated progress bar at bottom
        let bar_y = start[1] + 56.0;
        // Background
        draw_list.add_rect(
            [start[0] + 8.0, bar_y],
            [start[0] + width - 8.0, bar_y + 4.0],
            [0.15, 0.15, 0.2, 0.6],
        ).filled(true).rounding(2.0).build();
        // Sweeping fill
        let bar_inner = width - 16.0;
        let sweep_width = bar_inner * 0.35;
        let sweep_cycle = ((frame_count % 150) as f32) / 150.0;
        let sweep_x = start[0] + 8.0 + sweep_cycle * (bar_inner + sweep_width) - sweep_width;
        draw_list.add_rect(
            [sweep_x.max(start[0] + 8.0), bar_y],
            [(sweep_x + sweep_width).min(start[0] + width - 8.0), bar_y + 4.0],
            [0.3, 0.8, 1.0, 0.7],
        ).filled(true).rounding(2.0).build();

        // Border
        draw_list.add_rect(
            [start[0], start[1]],
            [start[0] + width, start[1] + 62.0],
            [0.25, 0.5, 0.7, 0.3],
        ).rounding(6.0).build();
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
        if i > 0 { ui.same_line(); }

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
                draw_list.add_line(
                    [btn_pos[0], btn_pos[1] + text_size[1] + 1.0],
                    [btn_pos[0] + text_size[0], btn_pos[1] + text_size[1] + 1.0],
                    [0.6, 0.5, 0.3, 0.5],
                ).thickness(1.0).build();
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
                            auto_populate_locks(&build_clone, &mut state.main.build_locks);
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
            draw_list.add_line(
                [btn_pos[0], btn_pos[1] + text_size[1] + 2.0],
                [btn_pos[0] + text_size[0], btn_pos[1] + text_size[1] + 2.0],
                [1.0, 0.75, 0.2, 0.9],
            ).thickness(2.0).build();
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
                load_characters(state);
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
fn render_left_section_header(ui: &Ui, title: &str, spacing: f32) {
    ui.dummy([0.0, spacing]); // gap above
    {
        let pos = ui.cursor_screen_pos();
        let width = ui.content_region_avail()[0];
        let draw_list = ui.get_window_draw_list();
        draw_list.add_rect(
            [pos[0], pos[1]],
            [pos[0] + width, pos[1] + 18.0],
            [0.22, 0.19, 0.10, 0.9],
        ).filled(true).build();
        draw_list.add_text(
            [pos[0] + 6.0, pos[1] + 2.0],
            [0.85, 0.72, 0.3, 1.0],
            title,
        );
    }
    ui.dummy([0.0, 20.0]);
    ui.dummy([0.0, spacing * 0.5]); // gap below
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

    if let Some(_combo) = ComboBox::new("##char_select").preview_value(&preview).begin(ui) {
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
        load_character_tabs(state, name);
    }

    // Build Template dropdown
    if !state.main.build_tabs.is_empty() {
        ui.spacing();
        ui.text_colored([0.6, 0.6, 0.7, 1.0], "Build:");
        ui.set_next_item_width(-1.0);
        let bt_preview = state.main.selected_build_tab
            .and_then(|i| state.main.build_tabs.get(i))
            .map(|t| {
                let name = t.build.name.as_deref().unwrap_or("Unnamed");
                format!("Tab {}: {}", t.tab, name)
            })
            .unwrap_or_else(|| "Select...".into());

        let bt_labels: Vec<(usize, String)> = state.main.build_tabs.iter().enumerate()
            .map(|(i, t)| {
                let name = t.build.name.as_deref().unwrap_or("Unnamed");
                (i, format!("Tab {}: {}", t.tab, name))
            }).collect();

        let mut bt_changed: Option<usize> = None;
        if !bt_labels.is_empty() {
            if let Some(_combo) = ComboBox::new("##build_tab_select").preview_value(&bt_preview).begin(ui) {
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
            update_build_chat_code(state);
            resolve_selected_build(state);
        }
    }

    // Equipment Template dropdown
    if !state.main.equipment_tabs.is_empty() {
        ui.text_colored([0.6, 0.6, 0.7, 1.0], "Equipment:");
        ui.set_next_item_width(-1.0);
        let et_preview = state.main.selected_equipment_tab
            .and_then(|i| state.main.equipment_tabs.get(i))
            .map(|t| {
                let name = t.name.as_deref().unwrap_or("Unnamed");
                format!("Tab {}: {}", t.tab, name)
            })
            .unwrap_or_else(|| "Select...".into());

        let et_labels: Vec<(usize, String)> = state.main.equipment_tabs.iter().enumerate()
            .map(|(i, t)| {
                let name = t.name.as_deref().unwrap_or("Unnamed");
                (i, format!("Tab {}: {}", t.tab, name))
            }).collect();

        let mut et_changed: Option<usize> = None;
        if !et_labels.is_empty() {
            if let Some(_combo) = ComboBox::new("##equip_tab_select").preview_value(&et_preview).begin(ui) {
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
            resolve_selected_build(state);
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
            resolve_selected_build(state);
        }
        ui.same_line();
    }
    ui.new_line();

    // Radar chart + presets
    render_left_section_header(ui, "OPTIMIZATION WEIGHTS", state.config.section_spacing);

    // Compute overlays for radar chart
    let current_axes = state.main.comparison.current_combat_solo.as_ref()
        .map(crate::ui::radar_chart::compute_axes_from_metrics);
    let optimized_axes = if !state.main.comparison.suggestions.is_empty() {
        let idx = state.main.comparison.selected_suggestion
            .min(state.main.comparison.suggestions.len() - 1);
        state.main.comparison.suggestions[idx].combat_solo.as_ref()
            .map(crate::ui::radar_chart::compute_axes_from_metrics)
    } else {
        None
    };

    let show_current = state.main.active_tab == MainTab::Improve;
    let _chart_modified = crate::ui::radar_chart::render_radar_chart(
        ui,
        &mut state.main.weights,
        &mut state.main.radar_dragging,
        if show_current { current_axes.as_ref() } else { None },
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
    ui.text_colored([0.6, 0.8, 1.0, 1.0], &format!("  Focus: {}", summary));

    // Action buttons
    render_left_section_header(ui, "ACTIONS", state.config.section_spacing);

    let is_improve = state.main.active_tab == MainTab::Improve;
    let btn_label = if is_improve { "Improve Build" } else { "Optimize Build" };
    let disabled = state.main.optimizing || state.main.game_db.is_none()
        || (is_improve && state.main.current_build.is_none());

    if disabled {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size(btn_label, [-1.0, 28.0]);
        style.pop();
        if ui.is_item_hovered() {
            ui.tooltip_text(if state.main.optimizing { "Optimization in progress..." }
                else if state.main.game_db.is_none() { "Waiting for game data to load..." }
                else { "Select a character first" });
        }
    } else if ui.button_with_size(btn_label, [-1.0, 28.0]) {
        if is_improve {
            let profession_name = state.main.current_build.as_ref().map(|b| b.profession.clone());
            if let Some(ref prof_name) = profession_name {
                start_optimization_with_profession(state, prof_name);
            }
        } else {
            start_optimization(state);
        }
    }

    // Refresh button
    ui.dummy([0.0, 2.0]);
    if state.main.characters_loading {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size("Refreshing...", [-1.0, 0.0]);
        style.pop();
    } else if ui.button_with_size("Refresh Data", [-1.0, 0.0]) {
        load_characters(state);
    }
    ui.dummy([0.0, 4.0]);
}

fn render_main_content(ui: &Ui, state: &mut AddonState) {
    match state.main.active_tab {
        MainTab::NewBuild => {
            render_new_build_tab(ui, state);
        }
        MainTab::Improve => {
            render_improve_tab(ui, state);
        }
        MainTab::SaveLoad => {
            render_saveload_tab(ui, state);
        }
        MainTab::Settings => {
            render_settings_tab(ui, state);
        }
    }
}

fn render_new_build_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.selected_character.is_none() {
        ui.text_colored([0.6, 0.6, 0.7, 1.0], "Select a character from the left panel to create a new build.");
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
        render_save_build_ui(ui, state);
        ui.same_line();
        if ui.small_button("Clear Results") {
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
    } else if let Some(ref build) = state.main.current_build {
        // Show current build when no suggestions yet
        let stats = state.main.current_stats.clone();
        build_display::render_build_card(ui, build, stats.as_ref());
    } else {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "Use the Optimize Build button in the left panel.");
    }

    // Chat bar at bottom
    ui.spacing();
    if let Some(msg) = crate::ui::chat_bar::render_chat_bar(ui, &mut state.main.chat) {
        state.main.chat.waiting = true;
        send_chat_message(state, msg);
    }
}

fn render_improve_tab(ui: &Ui, state: &mut AddonState) {
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

    // ── Two-panel layout: Current Build | Optimized Build ──
    let has_suggestion = !state.main.comparison.suggestions.is_empty();
    let avail = ui.content_region_avail();
    let _panel_height = avail[1] - 40.0; // leave room for chat bar

    if state.main.current_build.is_some() {
        // Clone build data upfront to avoid borrow conflicts with mutable lock_panel state
        let build = state.main.current_build.clone().unwrap();
        let stats = state.main.current_stats.clone();
        let profession_name = build.profession.clone();
        let current_specs: Vec<(u32, Vec<u32>)> = build.specializations.iter().map(|spec| {
            let selected_ids: Vec<u32> = spec.traits_selected.iter()
                .filter(|t| t.selected).map(|t| t.id).collect();
            (spec.id, selected_ids)
        }).collect();

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
                    if Selectable::new(&format!("[{}]##sug_{}", label, i))
                        .selected(selected)
                        .size([0.0, 0.0])
                        .build(ui)
                    {
                        state.main.comparison.selected_suggestion = i;
                    }
                    if i < tab_count - 1 { ui.same_line(); }
                }
                ui.spacing();
            }

            let avail_after = ui.content_region_avail();
            let scroll_height = avail_after[1] - 60.0; // room for save + chat

            let idx = state.main.comparison.selected_suggestion
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
                        );
                    }
                    ui.next_column();
                    ui.indent_by(6.0);
                    // RIGHT: Optimized Build header + read-only specs
                    build_display::render_card_header(ui, "OPTIMIZED BUILD", [0.3, 1.0, 0.5, 1.0]);
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
                    build_display::render_build_stats(ui, stats.as_ref(), suggestion.estimated_stats.as_ref());
                    ui.next_column();
                    ui.indent_by(6.0);
                    build_display::render_suggestion_stats(ui, &suggestion, stats.as_ref());
                    ui.unindent_by(6.0);
                    ui.columns(1, "##imp_stats_end", false);

                    // ── COMBAT PERFORMANCE SECTION ──
                    ui.columns(2, "##imp_combat", false);
                    ui.set_column_offset(1, col1_offset);
                    build_display::render_build_combat(ui, current_combat_solo.as_ref(), suggestion.combat_solo.as_ref());
                    ui.next_column();
                    ui.indent_by(6.0);
                    build_display::render_suggestion_combat(ui, &suggestion, current_combat_solo.as_ref());
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
                        );
                    }
                    build_display::render_build_card_no_specs(ui, &build, stats.as_ref(), None);
                });
        }
    } else if state.main.selected_character.is_some() {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "Loading character build...");
    } else {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "Select a character from the left panel.");
    }

    // Save build UI + clear button
    if has_suggestion {
        render_save_build_ui(ui, state);
        ui.same_line();
        if ui.small_button("Clear Results##improve") {
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
    }

    // Chat bar at bottom
    if state.main.current_build.is_some() {
        if let Some(msg) = crate::ui::chat_bar::render_chat_bar(ui, &mut state.main.chat) {
            state.main.chat.waiting = true;
            send_chat_message(state, msg);
        }
    }
}

fn render_settings_tab(ui: &Ui, state: &mut AddonState) {
    // ═══ Section 1: AI Provider Configuration ═══
    build_display::render_card_header(ui, "AI PROVIDER", [1.0, 0.88, 0.35, 1.0]);
    {

        // Provider radio buttons
        let mut provider_changed = false;
        for provider in &gw2_core::config::LlmProvider::ALL {
            let is_selected = state.config.active_provider == *provider;
            if ui.radio_button_bool(provider.label(), is_selected) && !is_selected {
                state.config.active_provider = provider.clone();
                provider_changed = true;
            }
        }
        if provider_changed {
            state.main.settings_key_input.clear();
            state.main.settings_key_status = None;
            state.main.settings_key_valid = false;
            state.main.settings_key_warning = None;
            state.main.available_models.clear();
            state.main.models_error = None;
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

        ui.spacing();

        // Per-provider key status + input
        let provider_label = state.config.active_provider.label().to_string();
        let has_key = state.config.has_active_llm_key();
        if has_key {
            ui.text_colored([0.0, 1.0, 0.0, 1.0], &format!("{} API Key: configured", provider_label));
        } else {
            ui.text_colored([1.0, 0.5, 0.0, 1.0], &format!("{} API Key: not set", provider_label));
        }

        // Test Connection button (for already-saved key)
        if has_key {
            ui.same_line();
            let validating = state.main.settings_key_validating;
            if validating {
                let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
                ui.button_with_size("Testing...", [120.0, 0.0]);
                style.pop();
            } else if ui.button_with_size("Test Connection", [120.0, 0.0]) {
                state.main.settings_key_validating = true;
                state.main.settings_key_status = Some("Testing connection...".into());
                state.main.settings_key_valid = false;
                state.main.settings_key_warning = None;
                let addon_dir = state.addon_dir.clone();
                let config_snapshot = state.config.clone();
                let token = state.cancel_token.clone();
                std::thread::spawn(move || {
                    if token.is_cancelled() { return; }
                    let result = gw2_optimizer::llm::create_client(&config_snapshot, &addon_dir)
                        .map(|c| c.validate_key_detailed());
                    if token.is_cancelled() { return; }
                    crate::state::with_state(|s| {
                        s.main.settings_key_validating = false;
                        match result {
                            Ok(validation) => {
                                s.main.settings_key_valid = validation.valid;
                                s.main.settings_key_status = Some(validation.message);
                                s.main.settings_key_warning = validation.warning;
                            }
                            Err(e) => {
                                s.main.settings_key_valid = false;
                                s.main.settings_key_status = Some(format!("Connection failed: {}", e));
                                s.main.settings_key_warning = None;
                            }
                        }
                    });
                });
            }
        }

        // Key input + Save
        ui.set_next_item_width(300.0);
        ui.input_text(&format!("##{}_key_input", provider_label), &mut state.main.settings_key_input)
            .hint("Enter new API key...")
            .build();
        ui.same_line();
        let validating = state.main.settings_key_validating;
        if validating {
            let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
            ui.button_with_size("Saving...", [100.0, 0.0]);
            style.pop();
        } else if ui.button_with_size("Save Key", [100.0, 0.0]) {
            let key = state.main.settings_key_input.trim().to_string();
            if !key.is_empty() {
                // Save key immediately
                match state.config.active_provider {
                    gw2_core::config::LlmProvider::Gemini => {
                        state.config.gemini_api_key = Some(key.clone());
                    }
                    gw2_core::config::LlmProvider::OpenAI => {
                        state.config.openai_api_key = Some(key.clone());
                    }
                    gw2_core::config::LlmProvider::Anthropic => {
                        state.config.anthropic_api_key = Some(key.clone());
                    }
                }
                if let Err(e) = state.config.save(&state.config_path) {
                    nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
                }
                state.main.settings_key_input.clear();
                state.main.settings_key_status = Some("Key saved. Validating...".into());
                state.main.settings_key_valid = false;
                state.main.settings_key_warning = None;

                // Validate in background using detailed validation
                state.main.settings_key_validating = true;
                let addon_dir = state.addon_dir.clone();
                let config_snapshot = state.config.clone();
                let token = state.cancel_token.clone();
                std::thread::spawn(move || {
                    if token.is_cancelled() { return; }
                    let result = gw2_optimizer::llm::create_client(&config_snapshot, &addon_dir)
                        .map(|c| c.validate_key_detailed());
                    if token.is_cancelled() { return; }
                    crate::state::with_state(|s| {
                        s.main.settings_key_validating = false;
                        match result {
                            Ok(validation) => {
                                s.main.settings_key_valid = validation.valid;
                                s.main.settings_key_status = Some(validation.message);
                                s.main.settings_key_warning = validation.warning;
                            }
                            Err(e) => {
                                s.main.settings_key_valid = false;
                                s.main.settings_key_status = Some(format!("Key saved but validation failed: {}", e));
                                s.main.settings_key_warning = None;
                            }
                        }
                    });
                });
            }
        }

        // Status display
        if let Some(ref status) = state.main.settings_key_status {
            if state.main.settings_key_valid {
                ui.text_colored([0.0, 1.0, 0.0, 1.0], status);
            } else if status.contains("saved") || status.contains("Testing") || status.contains("Validating") {
                ui.text_colored([0.7, 0.7, 0.7, 1.0], status);
            } else {
                ui.text_colored([1.0, 0.3, 0.3, 1.0], status);
            }
        }
        // Warning display (billing/quota)
        if let Some(ref warning) = state.main.settings_key_warning {
            ui.text_colored([1.0, 0.7, 0.0, 1.0], &format!("  Warning: {}", warning));
        }

        ui.spacing();

        // Model selector for the active provider (dynamic fetch with fallback)
        let current_model = match state.config.active_provider {
            gw2_core::config::LlmProvider::Gemini => state.config.gemini_model_id().to_string(),
            gw2_core::config::LlmProvider::OpenAI => state.config.openai_model_id().to_string(),
            gw2_core::config::LlmProvider::Anthropic => state.config.anthropic_model_id().to_string(),
        };
        let config_field = match state.config.active_provider {
            gw2_core::config::LlmProvider::Gemini => "gemini",
            gw2_core::config::LlmProvider::OpenAI => "openai",
            gw2_core::config::LlmProvider::Anthropic => "anthropic",
        };

        // Auto-fetch models when list is empty, key exists, and not already loading
        if state.main.available_models.is_empty() && !state.main.models_loading && has_key {
            start_fetch_models(state);
        }

        // Build combined model list: dynamic fetched + hardcoded fallback
        let display_models: Vec<(String, String)> = if !state.main.available_models.is_empty() {
            state.main.available_models.clone()
        } else {
            // Fallback to hardcoded constants
            let hardcoded: &[(&str, &str)] = match state.config.active_provider {
                gw2_core::config::LlmProvider::Gemini => gw2_core::config::GEMINI_MODELS,
                gw2_core::config::LlmProvider::OpenAI => gw2_core::config::OPENAI_MODELS,
                gw2_core::config::LlmProvider::Anthropic => gw2_core::config::ANTHROPIC_MODELS,
            };
            hardcoded.iter().map(|(id, label)| (id.to_string(), label.to_string())).collect()
        };

        let preview = display_models.iter()
            .find(|(id, _)| *id == current_model)
            .map(|(_, label)| label.as_str())
            .unwrap_or(&current_model);

        ui.text("Model:");
        ui.same_line();
        ui.set_next_item_width(300.0);
        if let Some(_combo) = ComboBox::new(&format!("##{}_model", config_field))
            .preview_value(preview)
            .begin(ui)
        {
            for (id, label) in &display_models {
                let selected = *id == current_model;
                if Selectable::new(label).selected(selected).build(ui) {
                    match state.config.active_provider {
                        gw2_core::config::LlmProvider::Gemini => {
                            state.config.gemini_model = Some(id.clone());
                        }
                        gw2_core::config::LlmProvider::OpenAI => {
                            state.config.openai_model = Some(id.clone());
                        }
                        gw2_core::config::LlmProvider::Anthropic => {
                            state.config.anthropic_model = Some(id.clone());
                        }
                    }
                    if let Err(e) = state.config.save(&state.config_path) {
                        nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
                    }
                }
            }
        }
        ui.same_line();
        if state.main.models_loading {
            ui.text_colored([0.7, 0.7, 0.7, 1.0], "Loading...");
        } else if ui.button("Refresh Models") {
            state.main.available_models.clear();
            state.main.models_error = None;
            start_fetch_models(state);
        }
        if let Some(ref err) = state.main.models_error {
            ui.text_colored([1.0, 0.5, 0.0, 1.0], &format!("  Model fetch: {}", err));
        }

        ui.spacing();

        // Usage / quota display
        let usage_filename = match state.config.active_provider {
            gw2_core::config::LlmProvider::Gemini => "gemini_usage.json",
            gw2_core::config::LlmProvider::OpenAI => "openai_usage.json",
            gw2_core::config::LlmProvider::Anthropic => "anthropic_usage.json",
        };
        let usage_path = state.addon_dir.join(usage_filename);
        if let Ok(json) = std::fs::read_to_string(&usage_path) {
            if let Ok(usage) = serde_json::from_str::<serde_json::Value>(&json) {
                let today = usage.get("requests_today").and_then(|v| v.as_u64()).unwrap_or(0);
                ui.text(&format!("{} usage today: {} requests", provider_label, today));
            }
        } else {
            ui.text(&format!("{} usage today: 0 requests", provider_label));
        }

    }

    // ═══ Section 2: UI Preferences ═══
    build_display::render_card_header(ui, "UI PREFERENCES", [1.0, 0.88, 0.35, 1.0]);
    {

        // Window opacity slider
        ui.text("Window Opacity:");
        ui.set_next_item_width(200.0);
        let mut opacity = state.config.window_opacity;
        if nexus::imgui::Slider::new("##opacity", 0.3, 1.0)
            .display_format("%.2f")
            .build(ui, &mut opacity)
        {
            state.config.window_opacity = opacity;
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

        // Global UI scale (text + elements)
        ui.text("Global Scale:");
        ui.set_next_item_width(200.0);
        let mut scale = state.config.font_scale;
        if nexus::imgui::Slider::new("##font_scale", 0.5, 2.0)
            .display_format("%.2f")
            .build(ui, &mut scale)
        {
            state.config.font_scale = scale;
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

        ui.spacing();
        ui.separator();
        ui.spacing();
        ui.text_colored([0.7, 0.7, 0.75, 1.0], "Layout Tuning:");
        ui.spacing();

        // Left panel width — use InputFloat to avoid slider-in-resizing-container jump
        ui.text("Left Panel Width:");
        ui.set_next_item_width(200.0);
        let mut lpw = state.config.left_panel_width;
        if nexus::imgui::InputFloat::new(ui, "##left_panel_w", &mut lpw)
            .step(5.0)
            .step_fast(20.0)
            .build()
        {
            state.config.left_panel_width = lpw.clamp(180.0, 400.0);
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

        // Panel padding
        ui.text("Panel Padding:");
        ui.set_next_item_width(200.0);
        let mut pp = state.config.panel_padding;
        if nexus::imgui::InputFloat::new(ui, "##panel_pad", &mut pp)
            .step(1.0)
            .step_fast(4.0)
            .build()
        {
            state.config.panel_padding = pp.clamp(0.0, 20.0);
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

        // Section spacing
        ui.text("Section Spacing:");
        ui.set_next_item_width(200.0);
        let mut ss = state.config.section_spacing;
        if nexus::imgui::InputFloat::new(ui, "##section_sp", &mut ss)
            .step(1.0)
            .step_fast(4.0)
            .build()
        {
            state.config.section_spacing = ss.clamp(0.0, 16.0);
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

        // Content indent
        ui.text("Content Indent:");
        ui.set_next_item_width(200.0);
        let mut ci = state.config.content_indent;
        if nexus::imgui::InputFloat::new(ui, "##content_ind", &mut ci)
            .step(1.0)
            .step_fast(4.0)
            .build()
        {
            state.config.content_indent = ci.clamp(0.0, 20.0);
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

        // Reset to defaults button
        ui.spacing();
        if ui.small_button("Reset Layout Defaults") {
            state.config.left_panel_width = 255.0;
            state.config.panel_padding = 6.0;
            state.config.section_spacing = 4.0;
            state.config.content_indent = 4.0;
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

    }

    // ═══ Section 3: Optimization Defaults ═══
    build_display::render_card_header(ui, "OPTIMIZATION DEFAULTS", [1.0, 0.88, 0.35, 1.0]);
    {

        ui.text("Default Game Mode:");
        let current_default = state.config.default_game_mode.clone().unwrap_or_else(|| "PvE".into());
        for mode in &["PvE", "PvP", "WvW"] {
            let is_selected = current_default == *mode;
            if ui.radio_button_bool(mode, is_selected) && !is_selected {
                state.config.default_game_mode = Some(mode.to_string());
                if let Err(e) = state.config.save(&state.config_path) {
                    nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
                }
            }
        }

    }

    // ═══ Section 4: Cache & Data Management ═══
    build_display::render_card_header(ui, "CACHE & DATA", [1.0, 0.88, 0.35, 1.0]);
    {

        // GW2 API Key display
        if let Some(ref key) = state.config.gw2_api_key {
            let display = if key.chars().count() > 12 {
                let prefix: String = key.chars().take(8).collect();
                let suffix: String = key.chars().rev().take(4).collect::<String>().chars().rev().collect();
                format!("{}...{}", prefix, suffix)
            } else {
                "****".into()
            };
            ui.text(&format!("GW2 API Key: {}", display));
        }
        if let Some(build) = state.config.cache_build_number {
            ui.text(&format!("Cached game build: {}", build));
        }

        ui.spacing();

        // Cache size + clear
        let cache_dir = state.addon_dir.join("cache");
        let cache_size = calculate_dir_size(&cache_dir);
        ui.text(&format!("Cache size: {}", format_bytes(cache_size)));
        ui.same_line();
        let already_refreshing = state.main.game_db_loading;
        if already_refreshing {
            let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
            ui.button_with_size("Clear Cache", [100.0, 0.0]);
            style.pop();
        } else if ui.button_with_size("Clear Cache", [100.0, 0.0]) {
            let _ = std::fs::remove_dir_all(&cache_dir);
            state.config.cache_build_number = None;
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
            state.main.game_db = None;
            state.setup.download_progress = None;
            start_game_data_refresh(state);
        }

        ui.spacing();

        // Auto-refresh toggle
        let mut auto_refresh = state.config.auto_refresh_cache;
        if ui.checkbox("Auto-refresh cache on startup", &mut auto_refresh) {
            state.config.auto_refresh_cache = auto_refresh;
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
        }

        ui.spacing();

        // Refresh game data
        let refreshing = state.main.game_db_loading;
        if refreshing {
            let stage = &state.main.game_refresh_stage;
            if !stage.is_empty() {
                ui.text_colored([1.0, 1.0, 0.0, 1.0], stage);
            } else {
                ui.text_colored([1.0, 1.0, 0.0, 1.0], "Downloading game data...");
            }
            let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
            ui.button_with_size("Refreshing...", [200.0, 0.0]);
            style.pop();
        } else if ui.button_with_size("Refresh Game Data", [200.0, 0.0]) {
            let cache_dir_refresh = state.addon_dir.join("cache");
            let _ = std::fs::remove_dir_all(&cache_dir_refresh);
            state.config.cache_build_number = None;
            if let Err(e) = state.config.save(&state.config_path) {
                nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
            }
            state.main.game_db = None;
            state.setup.download_progress = None;
            start_game_data_refresh(state);
        }

        ui.spacing();

        // Reset setup
        if !state.main.confirm_reset {
            if ui.button_with_size("Reset Setup", [200.0, 0.0]) {
                state.main.confirm_reset = true;
            }
        } else {
            ui.text_colored([1.0, 0.3, 0.0, 1.0], "Are you sure? This will clear all settings.");
            if ui.button_with_size("Yes, Reset", [100.0, 0.0]) {
                state.main.confirm_reset = false;
                state.screen = crate::state::Screen::Setup(crate::state::SetupStep::Gw2ApiKey);
            }
            ui.same_line();
            if ui.button_with_size("Cancel", [100.0, 0.0]) {
                state.main.confirm_reset = false;
            }
        }

    }

    ui.spacing();

    // About
    ui.text_colored([0.5, 0.5, 0.5, 1.0], "GW2 Build Optimizer v1.0.0");
    let provider_label = state.config.active_provider.label();
    ui.text_colored([0.5, 0.5, 0.5, 1.0], &format!("Powered by {} AI", provider_label));
}

fn calculate_dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0; };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn load_characters(state: &mut AddonState) {
    let Some(ref key) = state.config.gw2_api_key else {
        state.main.error = Some("No GW2 API key configured".into());
        return;
    };

    state.main.error = None;

    // Phase 1: try loading from cache instantly
    let cache_dir = state.addon_dir.join("cache");
    let cache = gw2_api::cache::DataCache::new(&cache_dir);
    if let Ok(Some(cached_chars)) = cache.load_characters() {
        state.main.characters = cached_chars;
        state.main.characters_loading = false;
    } else {
        // No cache — show loading indicator until API responds
        state.main.characters_loading = true;
    }

    // Phase 2: background refresh from API
    let key = key.clone();
    let token = state.cancel_token.clone();
    let had_cache = !state.main.characters.is_empty();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let result = gw2_api::client::Gw2Client::with_key(&key)
            .and_then(|c| c.fetch_characters());

        if token.is_cancelled() { return; }

        crate::state::with_state(|s| {
            s.main.characters_loading = false;
            match result {
                Ok(fresh_chars) => {
                    // Save to cache for next time
                    let cache_dir = s.addon_dir.join("cache");
                    let cache = gw2_api::cache::DataCache::new(&cache_dir);
                    let _ = cache.save_characters(&fresh_chars);

                    // Only update UI if data changed
                    if s.main.characters != fresh_chars {
                        s.main.characters = fresh_chars;
                    }
                }
                Err(e) => {
                    // If we had cached data, don't overwrite it with an error
                    if !had_cache {
                        s.main.error = Some(e.to_string());
                    }
                    // Update API health status on failure
                    s.main.api_status = crate::state::ApiStatus::Offline;
                }
            }
        });
    });
}

/// Apply fetched tabs to state: auto-select active tabs, generate chat code, resolve build.
fn apply_character_tabs(
    state: &mut AddonState,
    build_tabs: Vec<gw2_api::models::BuildTab>,
    equipment_tabs: Vec<gw2_api::models::EquipmentTab>,
) {
    let bt_idx = build_tabs.iter().position(|t| t.is_active).unwrap_or(0);
    let et_idx = equipment_tabs.iter().position(|t| t.is_active).unwrap_or(0);
    state.main.build_tabs = build_tabs;
    state.main.equipment_tabs = equipment_tabs;
    state.main.selected_build_tab = if state.main.build_tabs.is_empty() { None } else { Some(bt_idx) };
    state.main.selected_equipment_tab = if state.main.equipment_tabs.is_empty() { None } else { Some(et_idx) };
    update_build_chat_code_inner(state);
    resolve_selected_build_inner(state);
}

/// Phase 1: Load character tabs from cache (instant) then refresh from API in background.
fn load_character_tabs(state: &mut AddonState, character_name: String) {
    let key = match state.config.gw2_api_key.clone() {
        Some(k) => k,
        None => return,
    };

    state.main.error = None;

    // Phase 1: try loading from cache instantly
    let cache_dir = state.addon_dir.join("cache");
    let cache = gw2_api::cache::DataCache::new(&cache_dir);
    let cached_bt: Option<Vec<gw2_api::models::BuildTab>> =
        cache.load_character(&character_name, "buildtabs").ok().flatten();
    let cached_et: Option<Vec<gw2_api::models::EquipmentTab>> =
        cache.load_character(&character_name, "equiptabs").ok().flatten();

    let had_cache = if let (Some(bt), Some(et)) = (cached_bt, cached_et) {
        // Cache hit — display immediately
        apply_character_tabs(state, bt, et);
        state.main.build_loading = false;
        true
    } else {
        // No cache — show loading indicator
        state.main.build_loading = true;
        false
    };

    // Phase 2: background refresh from API
    let expected_char = character_name.clone();
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let result = (|| -> Result<(Vec<gw2_api::models::BuildTab>, Vec<gw2_api::models::EquipmentTab>), String> {
            let client = gw2_api::client::Gw2Client::with_key(&key)
                .map_err(|e| e.to_string())?;
            let build_tabs = client.fetch_build_tabs(&expected_char).map_err(|e| e.to_string())?;
            if token.is_cancelled() { return Err("Cancelled".into()); }
            let equipment_tabs = client.fetch_equipment_tabs(&expected_char).map_err(|e| e.to_string())?;
            Ok((build_tabs, equipment_tabs))
        })();

        if token.is_cancelled() { return; }

        crate::state::with_state(|s| {
            // Stale-result guard: discard if user switched characters
            let current_char = s.main.selected_character
                .and_then(|i| s.main.characters.get(i).cloned());
            if current_char.as_deref() != Some(&expected_char) {
                s.main.build_loading = false;
                return;
            }

            match result {
                Ok((fresh_bt, fresh_et)) => {
                    // Save to cache for next time
                    let cache_dir = s.addon_dir.join("cache");
                    let cache = gw2_api::cache::DataCache::new(&cache_dir);
                    let _ = cache.save_character(&expected_char, "buildtabs", &fresh_bt);
                    let _ = cache.save_character(&expected_char, "equiptabs", &fresh_et);

                    // Compare: only update UI if data actually changed
                    let bt_changed = serde_json::to_string(&s.main.build_tabs).ok()
                        != serde_json::to_string(&fresh_bt).ok();
                    let et_changed = serde_json::to_string(&s.main.equipment_tabs).ok()
                        != serde_json::to_string(&fresh_et).ok();

                    if bt_changed || et_changed {
                        apply_character_tabs(s, fresh_bt, fresh_et);
                    }
                    s.main.build_loading = false;
                }
                Err(e) => {
                    s.main.build_loading = false;
                    // If we had cached data, don't overwrite with error
                    if !had_cache {
                        s.main.error = Some(e);
                    }
                    s.main.api_status = crate::state::ApiStatus::Offline;
                }
            }
        });
    });
}

/// Phase 2: Resolve build from currently selected tabs. Called on tab change or game mode change.
fn resolve_selected_build(state: &mut AddonState) {
    resolve_selected_build_inner(state);
}

fn resolve_selected_build_inner(state: &mut AddonState) {
    let build_tab = state.main.selected_build_tab
        .and_then(|i| state.main.build_tabs.get(i))
        .cloned();
    let equip_tab = state.main.selected_equipment_tab
        .and_then(|i| state.main.equipment_tabs.get(i))
        .cloned();

    let (Some(bt), Some(et)) = (build_tab, equip_tab) else {
        state.main.build_loading = false;
        return;
    };

    // GameDb required — if not loaded yet, skip; load_game_db() will trigger resolve when ready
    let Some(ref db) = state.main.game_db else {
        return;
    };

    let game_mode = state.main.game_mode.clone();
    let char_name = state.main.selected_character
        .and_then(|i| state.main.characters.get(i).cloned())
        .unwrap_or_default();

    // Synchronous resolve — all lookups are O(1) HashMap hits on the in-memory GameDb
    state.main.build_loading = false;
    state.main.error = None;

    match resolve_build_from_db(&char_name, &bt.build, &et, db, &game_mode) {
        Ok(build) => {
            // Auto-populate locks from current build in Improve mode
            if state.main.active_tab == MainTab::Improve {
                auto_populate_locks(&build, &mut state.main.build_locks);
            }
            state.main.current_build = Some(build);
            match calculate_current_stats_from_db(&bt.build, &et, db, &game_mode) {
                Ok((stats, combat_solo, combat_party, combat_squad)) => {
                    state.main.current_stats = Some(stats);
                    state.main.comparison.current_combat_solo = combat_solo;
                    state.main.comparison.current_combat_party = combat_party;
                    state.main.comparison.current_combat_squad = combat_squad;
                }
                Err(_) => {
                    // Clear stats AND combat metrics together — stale combat data from
                    // a previous build would be shown alongside the new current_build.
                    state.main.current_stats = None;
                    state.main.comparison.current_combat_solo = None;
                    state.main.comparison.current_combat_party = None;
                    state.main.comparison.current_combat_squad = None;
                }
            }
        }
        Err(e) => state.main.error = Some(e),
    }
}

/// Auto-populate BuildLocks from the current resolved build.
/// Locks only the elite specialization slot (slot 2) so the optimizer preserves the
/// profession identity. Core specs and all traits remain unlocked by default.
fn auto_populate_locks(build: &gw2_core::types::ResolvedBuild, locks: &mut gw2_core::types::BuildLocks) {
    // Start with everything unlocked
    locks.specs = [None; 3];
    locks.trait_locks.clear();

    // Only lock the elite specialization (slot 2) — traits remain unlocked
    if let Some(spec) = build.specializations.get(2) {
        if spec.id != 0 {
            locks.specs[2] = Some(spec.id);
        }
    }
}

/// Resolve the current build using the in-memory GameDb (O(1) lookups, zero disk I/O).
fn resolve_build_from_db(
    character_name: &str,
    build: &gw2_api::models::Build,
    equipment: &gw2_api::models::EquipmentTab,
    db: &gw2_optimizer::gamedb::GameDb,
    game_mode: &gw2_core::types::GameMode,
) -> Result<gw2_core::types::ResolvedBuild, String> {
    use gw2_core::types::*;

    let resolved_specs = resolve_specs_db(build, db);
    let resolved_skills = resolve_skills_db(build, db);
    let (weapons, armor, trinkets_vec, rune, relic_resolved) = resolve_equipment_db(equipment, db);
    let pvp_amulet = resolve_pvp_amulet_db(game_mode, equipment, db);

    Ok(ResolvedBuild {
        character_name: character_name.to_string(),
        profession: build.profession.clone().unwrap_or_default(),
        game_mode: game_mode.clone(),
        specializations: resolved_specs,
        skills: resolved_skills,
        weapons, armor, trinkets: trinkets_vec,
        relic: relic_resolved, rune,
        pvp_amulet,
    })
}

/// Calculate current stats using the in-memory GameDb (O(1) lookups, zero disk I/O).
fn calculate_current_stats_from_db(
    build: &gw2_api::models::Build,
    equipment: &gw2_api::models::EquipmentTab,
    db: &gw2_optimizer::gamedb::GameDb,
    game_mode: &gw2_core::types::GameMode,
) -> Result<CombatBundle, String> {
    let profession = build.profession.clone().unwrap_or_default();

    // PvP mode: stats come from amulet (O(1) lookup in db.pvp_amulets)
    if *game_mode == gw2_core::types::GameMode::PvP {
        if let Some(ref pvp) = equipment.equipment_pvp {
            if let Some(amulet_id) = pvp.amulet {
                if let Some(amulet) = db.pvp_amulets.get(&amulet_id) {
                    let opt_stats = gw2_optimizer::stats::calculate_pvp_stats(&amulet.attributes);
                    let derived = gw2_optimizer::stats::compute_derived(&opt_stats, &profession);
                    let stats = opt_stats_to_stat_block(&opt_stats, &derived);
                    let modifiers = gw2_optimizer::combat::DamageModifiers::default();
                    let (solo, party, squad) = compute_3tier_combat(
                        &opt_stats, &derived, &modifiers, &profession,
                    );
                    return Ok((stats, solo, party, squad));
                }
            }
        }
    }

    // PvE/WvW: collect equipped trait IDs (major + minor) via O(1) lookups
    let mut equipped_trait_ids = Vec::new();
    for spec_sel in &build.specializations {
        for &trait_id in &spec_sel.traits {
            if let Some(tid) = trait_id {
                equipped_trait_ids.push(tid);
            }
        }
        if let Some(spec_id) = spec_sel.id {
            if let Some(spec) = db.specializations.get(&spec_id) {
                equipped_trait_ids.extend(&spec.minor_traits);
            }
        }
    }

    // Find rune/sigil IDs from equipment upgrades (O(1) item lookups)
    let rune_id = equipment.equipment.iter()
        .flat_map(|p| p.upgrades.iter())
        .find_map(|&uid| {
            db.items.get(&uid).and_then(|item| {
                item.details.as_ref().and_then(|d| {
                    if d.detail_type.as_deref() == Some("Rune") { Some(uid) } else { None }
                })
            })
        });

    let sigil_ids: Vec<u32> = equipment.equipment.iter()
        .flat_map(|p| p.upgrades.iter())
        .filter_map(|&uid| {
            db.items.get(&uid).and_then(|item| {
                item.details.as_ref().and_then(|d| {
                    if d.detail_type.as_deref() == Some("Sigil") { Some(uid) } else { None }
                })
            })
        })
        .collect();

    // Pass GameDb's pre-indexed HashMaps directly — no copying needed
    let (opt_stats, derived) = gw2_optimizer::stats::calculate_full_stats(
        equipment,
        &equipped_trait_ids,
        rune_id,
        &sigil_ids,
        &profession,
        &db.items,
        &db.itemstats,
        &db.traits,
    );

    let relic_id = equipment.equipment.iter()
        .find(|p| p.slot == "Relic")
        .map(|p| p.id);
    let modifiers = gw2_optimizer::combat::extract_damage_modifiers(
        &equipped_trait_ids,
        rune_id,
        &sigil_ids,
        relic_id,
        &db.traits,
        &db.items,
    );

    let (combat_solo, combat_party, combat_squad) = compute_3tier_combat(
        &opt_stats, &derived, &modifiers, &profession,
    );

    Ok((opt_stats_to_stat_block(&opt_stats, &derived), combat_solo, combat_party, combat_squad))
}

/// Convert optimizer StatBlock + DerivedStats to display StatBlock.
fn opt_stats_to_stat_block(
    opt_stats: &gw2_optimizer::stats::StatBlock,
    derived: &gw2_optimizer::stats::DerivedStats,
) -> gw2_core::types::StatBlock {
    gw2_core::types::StatBlock {
        power: opt_stats.power.round() as i32,
        precision: opt_stats.precision.round() as i32,
        toughness: opt_stats.toughness.round() as i32,
        vitality: opt_stats.vitality.round() as i32,
        condition_damage: opt_stats.condition_damage.round() as i32,
        expertise: opt_stats.expertise.round() as i32,
        concentration: opt_stats.concentration.round() as i32,
        ferocity: opt_stats.ferocity.round() as i32,
        healing_power: opt_stats.healing_power.round() as i32,
        crit_chance: derived.crit_chance,
        crit_damage: derived.crit_damage,
        health: derived.health.round() as i32,
        armor: derived.armor.round() as i32,
    }
}

/// CombatBundle type alias for stat + 3-tier combat metrics.
type CombatBundle = (
    gw2_core::types::StatBlock,
    Option<gw2_core::types::CombatMetrics>,
    Option<gw2_core::types::CombatMetrics>,
    Option<gw2_core::types::CombatMetrics>,
);

/// Resolve specializations using GameDb O(1) lookups.
fn resolve_specs_db(
    build: &gw2_api::models::Build,
    db: &gw2_optimizer::gamedb::GameDb,
) -> Vec<gw2_core::types::ResolvedSpec> {
    use gw2_core::types::*;
    build.specializations.iter().filter_map(|sel| {
        let spec_id = sel.id?;
        let spec = db.specializations.get(&spec_id)?;
        let traits_selected: Vec<ResolvedTrait> = sel.traits.iter().enumerate()
            .filter_map(|(col, trait_id)| {
                let tid = (*trait_id)?;
                let t = db.traits.get(&tid)?;
                Some(ResolvedTrait {
                    id: t.id, name: t.name.clone(),
                    description: t.description.clone().unwrap_or_default(),
                    column: col, selected: true,
                })
            }).collect();
        Some(ResolvedSpec {
            id: spec.id, name: spec.name.clone(), elite: spec.elite,
            traits_selected, traits_available: Vec::new(),
        })
    }).collect()
}

/// Resolve skills using GameDb O(1) lookups.
fn resolve_skills_db(
    build: &gw2_api::models::Build,
    db: &gw2_optimizer::gamedb::GameDb,
) -> gw2_core::types::ResolvedSkills {
    use gw2_core::types::*;
    let find_skill = |id: u32| -> Option<SkillInfo> {
        db.skills.get(&id).map(|s| SkillInfo {
            id: s.id,
            name: s.name.clone(),
        })
    };
    if let Some(ref sk) = build.skills {
        ResolvedSkills {
            heal: sk.heal.and_then(&find_skill),
            utilities: sk.utilities.iter().map(|id| id.and_then(&find_skill)).collect(),
            elite: sk.elite.and_then(&find_skill),
        }
    } else {
        ResolvedSkills::default()
    }
}

/// Resolve equipment using GameDb O(1) lookups.
fn resolve_equipment_db(
    equipment: &gw2_api::models::EquipmentTab,
    db: &gw2_optimizer::gamedb::GameDb,
) -> (
    Vec<gw2_core::types::ResolvedWeaponSet>,
    Vec<gw2_core::types::ResolvedGearPiece>,
    Vec<gw2_core::types::ResolvedGearPiece>,
    Option<gw2_core::types::ResolvedUpgrade>,
    Option<gw2_core::types::ResolvedRelic>,
) {
    use gw2_core::types::*;

    let mut armor = Vec::new();
    let mut trinkets_vec = Vec::new();
    let mut rune = None;
    let mut relic_resolved = None;
    let mut ws1 = ResolvedWeaponSet { label: "Set 1".into(), ..Default::default() };
    let mut ws2 = ResolvedWeaponSet { label: "Set 2".into(), ..Default::default() };

    for piece in &equipment.equipment {
        let item = db.items.get(&piece.id);
        let item_name = item.map(|i| i.name.clone()).unwrap_or_else(|| format!("#{}", piece.id));
        let stat_prefix = piece.stats.as_ref()
            .and_then(|s| db.itemstats.get(&s.id).map(|is| is.name.clone()))
            .unwrap_or_default();

        let extract_sigils = |piece: &gw2_api::models::EquipmentPiece, ws: &mut ResolvedWeaponSet| {
            for &uid in &piece.upgrades {
                if let Some(u) = db.items.get(&uid) {
                    ws.sigils.push(UpgradeInfo { id: uid, name: u.name.clone() });
                }
            }
        };

        match piece.slot.as_str() {
            "WeaponA1" => {
                ws1.main_hand = Some(WeaponInfo { name: item_name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.clone()).unwrap_or_default() });
                extract_sigils(piece, &mut ws1);
            }
            "WeaponA2" => {
                ws1.off_hand = Some(WeaponInfo { name: item_name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.clone()).unwrap_or_default() });
                extract_sigils(piece, &mut ws1);
            }
            "WeaponB1" => {
                ws2.main_hand = Some(WeaponInfo { name: item_name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.clone()).unwrap_or_default() });
                extract_sigils(piece, &mut ws2);
            }
            "WeaponB2" => {
                ws2.off_hand = Some(WeaponInfo { name: item_name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.clone()).unwrap_or_default() });
                extract_sigils(piece, &mut ws2);
            }
            "Helm" | "Shoulders" | "Coat" | "Gloves" | "Leggings" | "Boots" => {
                if rune.is_none() {
                    if let Some(&uid) = piece.upgrades.first() {
                        if let Some(u) = db.items.get(&uid) {
                            rune = Some(ResolvedUpgrade { id: uid, name: u.name.clone() });
                        }
                    }
                }
                armor.push(ResolvedGearPiece { slot: piece.slot.clone(), name: item_name, stat_prefix, infusions: Vec::new() });
            }
            "Backpack" | "Accessory1" | "Accessory2" | "Amulet" | "Ring1" | "Ring2" => {
                trinkets_vec.push(ResolvedGearPiece { slot: piece.slot.clone(), name: item_name, stat_prefix, infusions: Vec::new() });
            }
            "Relic" => {
                relic_resolved = Some(ResolvedRelic {
                    id: piece.id, name: item_name,
                    description: item.and_then(|i| i.description.clone()).unwrap_or_default(),
                });
            }
            _ => {}
        }
    }

    let mut weapons = Vec::new();
    if ws1.main_hand.is_some() || ws1.off_hand.is_some() { weapons.push(ws1); }
    if ws2.main_hand.is_some() || ws2.off_hand.is_some() { weapons.push(ws2); }

    (weapons, armor, trinkets_vec, rune, relic_resolved)
}

/// Resolve PvP amulet using GameDb O(1) lookup.
fn resolve_pvp_amulet_db(
    game_mode: &gw2_core::types::GameMode,
    equipment: &gw2_api::models::EquipmentTab,
    db: &gw2_optimizer::gamedb::GameDb,
) -> Option<gw2_core::types::ResolvedPvpAmulet> {
    use gw2_core::types::*;
    if *game_mode != GameMode::PvP { return None; }
    let pvp_eq = equipment.equipment_pvp.as_ref()?;
    let amulet_id = pvp_eq.amulet?;
    db.pvp_amulets.get(&amulet_id).map(|a| {
        ResolvedPvpAmulet {
            id: a.id,
            name: a.name.clone(),
            stats: a.attributes.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        }
    })
}

/// Update build chat code from currently selected build tab.
fn update_build_chat_code(state: &mut AddonState) {
    update_build_chat_code_inner(state);
}

fn update_build_chat_code_inner(state: &mut AddonState) {
    let build_tab = state.main.selected_build_tab
        .and_then(|i| state.main.build_tabs.get(i));
    let game_db = state.main.game_db.as_ref();

    if let (Some(bt), Some(db)) = (build_tab, game_db) {
        state.main.build_chat_code = generate_build_chat_code(&bt.build, db);
    } else {
        state.main.build_chat_code = None;
    }
}

/// Generate GW2 build template chat code from a Build.
/// Format: 0x0D + profession_code(1) + 3x(spec_id(1) + trait_bits(1)) + 10x skill_palette(2 LE)
/// + 16 bytes profession-specific + base64 → [&...]
fn generate_build_chat_code(
    build: &gw2_api::models::Build,
    db: &gw2_optimizer::gamedb::GameDb,
) -> Option<String> {
    let profession_name = build.profession.as_deref()?;
    let profession = db.profession(profession_name)?;
    let code = profession.code?;
    if code > 255 { return None; }
    let profession_code = code as u8;

    let mut buf: Vec<u8> = Vec::with_capacity(44);
    buf.push(0x0D); // chat code type: build template
    buf.push(profession_code);

    // 3 specialization slots: spec_id(1 byte) + trait_choices(1 byte)
    for i in 0..3 {
        if let Some(sel) = build.specializations.get(i) {
            if let Some(spec_id) = sel.id {
                if spec_id > 255 { return None; }
                buf.push(spec_id as u8);

                // Encode trait choices as 2-bit positions packed into 1 byte
                // Bits: 00CCBBAA where AA = col0, BB = col1, CC = col2
                let spec = db.spec(spec_id);
                let mut trait_byte: u8 = 0;
                for (col, trait_id) in sel.traits.iter().enumerate() {
                    if col >= 3 { break; }
                    if let Some(tid) = trait_id {
                        // Find position of this trait in the column (0=top, 1=mid, 2=bot)
                        if let Some(spec_data) = spec {
                            let col_start = col * 3;
                            let position = spec_data.major_traits.iter()
                                .skip(col_start).take(3)
                                .position(|&mt| mt == *tid);
                            if let Some(pos) = position {
                                trait_byte |= ((pos as u8 + 1) & 0x03) << (col * 2);
                            }
                        }
                    }
                }
                buf.push(trait_byte);
            } else {
                buf.push(0); // no spec
                buf.push(0);
            }
        } else {
            buf.push(0);
            buf.push(0);
        }
    }

    // 5 terrestrial skills as palette IDs (u16 LE): heal, util1, util2, util3, elite
    // Interleaved with 5 aquatic skills
    let terrestrial_skills = build.skills.as_ref().map(|sk| {
        let mut ids = vec![sk.heal.unwrap_or(0)];
        for u in &sk.utilities {
            ids.push(u.unwrap_or(0));
        }
        while ids.len() < 4 { ids.push(0); }
        ids.push(sk.elite.unwrap_or(0));
        ids
    }).unwrap_or_else(|| vec![0; 5]);

    let aquatic_skills = build.aquatic_skills.as_ref().map(|sk| {
        let mut ids = vec![sk.heal.unwrap_or(0)];
        for u in &sk.utilities {
            ids.push(u.unwrap_or(0));
        }
        while ids.len() < 4 { ids.push(0); }
        ids.push(sk.elite.unwrap_or(0));
        ids
    }).unwrap_or_else(|| vec![0; 5]);

    // Interleave: terr_heal, aqua_heal, terr_util1, aqua_util1, ..., terr_elite, aqua_elite
    for i in 0..5 {
        let t_skill = terrestrial_skills.get(i).copied().unwrap_or(0);
        let t_palette = db.skill_to_palette.get(&t_skill).copied().unwrap_or(0);
        buf.extend_from_slice(&(t_palette as u16).to_le_bytes());

        let a_skill = aquatic_skills.get(i).copied().unwrap_or(0);
        let a_palette = db.skill_to_palette.get(&a_skill).copied().unwrap_or(0);
        buf.extend_from_slice(&(a_palette as u16).to_le_bytes());
    }

    // 16 bytes profession-specific data
    match profession_name {
        "Ranger" => {
            // Ranger pets: 4 bytes (terrestrial1, terrestrial2, aquatic1, aquatic2) + 12 zeros
            if let Some(ref pets) = build.pets {
                for pet in pets.terrestrial.iter().take(2) {
                    buf.push(pet.unwrap_or(0) as u8);
                }
                for pet in pets.aquatic.iter().take(2) {
                    buf.push(pet.unwrap_or(0) as u8);
                }
            } else {
                buf.extend_from_slice(&[0u8; 4]);
            }
            buf.extend_from_slice(&[0u8; 12]);
        }
        "Revenant" => {
            // Revenant legends: 4 bytes (legend number parsed from "LegendN" ID) + 12 zeros
            let legend_to_byte = |legend: &Option<String>| -> u8 {
                legend.as_deref().and_then(|l| {
                    l.strip_prefix("Legend").and_then(|n| n.parse::<u8>().ok())
                }).unwrap_or(0)
            };
            let legends = &build.legends;
            buf.push(legends.first().map(|l| legend_to_byte(l)).unwrap_or(0));
            buf.push(legends.get(1).map(|l| legend_to_byte(l)).unwrap_or(0));
            let aquatic_legends = &build.aquatic_legends;
            buf.push(aquatic_legends.first().map(|l| legend_to_byte(l)).unwrap_or(0));
            buf.push(aquatic_legends.get(1).map(|l| legend_to_byte(l)).unwrap_or(0));
            buf.extend_from_slice(&[0u8; 12]);
        }
        _ => {
            buf.extend_from_slice(&[0u8; 16]);
        }
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(&buf);
    Some(format!("[&{}]", encoded))
}

/// Fetch available models from the active provider's API in a background thread.
fn start_fetch_models(state: &mut AddonState) {
    state.main.models_loading = true;
    state.main.models_error = None;
    let addon_dir = state.addon_dir.clone();
    let config_snapshot = state.config.clone();
    let token = state.cancel_token.clone();
    std::thread::spawn(move || {
        if token.is_cancelled() { return; }
        let result = gw2_optimizer::llm::create_client(&config_snapshot, &addon_dir)
            .map_err(|e| e.to_string())
            .and_then(|c| c.list_models().map_err(|e| e.to_string()));
        if token.is_cancelled() { return; }
        crate::state::with_state(|s| {
            s.main.models_loading = false;
            match result {
                Ok(models) => {
                    s.main.available_models = models
                        .into_iter()
                        .map(|m| (m.id, m.display_name))
                        .collect();
                    s.main.models_error = None;
                }
                Err(e) => {
                    s.main.models_error = Some(e);
                }
            }
        });
    });
}

/// Re-download game data from the GW2 API, then reload GameDb.
fn start_game_data_refresh(state: &mut AddonState) {
    state.main.game_db_loading = true;
    let cache_dir = state.addon_dir.join("cache");
    let config_path = state.config_path.clone();
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let client = match gw2_api::client::Gw2Client::without_key() {
            Ok(c) => c,
            Err(e) => {
                crate::state::with_state(|s| {
                    s.main.game_db_loading = false;
                    s.main.error = Some(format!("Refresh failed: {}", e));
                });
                return;
            }
        };
        let cache = gw2_api::cache::DataCache::new(&cache_dir);

        let result = gw2_api::download::download_all(&client, &cache, |progress| {
            if token.is_cancelled() { return; }
            crate::state::with_state(|s| {
                let detail = if let Some(ref d) = progress.detail {
                    format!("Refreshing: {} ({})", progress.step_name, d)
                } else {
                    format!("Refreshing: {}", progress.step_name)
                };
                s.main.game_refresh_stage = detail;
            });
        });

        if token.is_cancelled() { return; }

        match result {
            Ok(build_number) => {
                // Save new build number
                crate::state::with_state(|s| {
                    s.config.cache_build_number = Some(build_number);
                    if let Err(e) = s.config.save(&config_path) {
                        nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
                    }
                });

                // Reload GameDb from fresh cache
                if token.is_cancelled() { return; }
                let cache2 = gw2_api::cache::DataCache::new(&cache_dir);
                let db_result = gw2_optimizer::gamedb::GameDb::load(&cache2);

                if token.is_cancelled() { return; }

                crate::state::with_state(|s| {
                    s.main.game_db_loading = false;
                    s.main.game_refresh_stage = String::new();
                    match db_result {
                        Ok(db) => {
                            nexus::log::log(nexus::log::LogLevel::Info, "GW2 Build Optimizer", "Game data refreshed successfully");
                            s.main.game_db = Some(db);
                            // Re-resolve build with fresh data
                            if s.main.selected_build_tab.is_some() && s.main.selected_equipment_tab.is_some() {
                                resolve_selected_build_inner(s);
                            }
                        }
                        Err(e) => {
                            s.main.error = Some(format!("Failed to reload game data: {}", e));
                        }
                    }
                });
            }
            Err(e) => {
                crate::state::with_state(|s| {
                    s.main.game_db_loading = false;
                    s.main.game_refresh_stage = String::new();
                    s.main.error = Some(format!("Refresh failed: {}", e));
                });
            }
        }
    });
}

/// Lightweight API health check: pings GET /v2/build (unauthenticated, returns a single integer).
fn check_api_health(state: &mut AddonState) {
    state.main.api_health_checking = true;
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let start = std::time::Instant::now();
        let result = gw2_api::client::Gw2Client::without_key()
            .and_then(|c| c.get_build_number());

        if token.is_cancelled() { return; }

        let status = match result {
            Ok(_) => {
                if start.elapsed().as_secs() >= 5 {
                    crate::state::ApiStatus::Degraded
                } else {
                    crate::state::ApiStatus::Online
                }
            }
            Err(_) => crate::state::ApiStatus::Offline,
        };

        crate::state::with_state(|s| {
            s.main.api_status = status;
            s.main.api_health_checking = false;
        });
    });
}

/// Load GameDb once on main screen entry (S11-T06)
fn load_game_db(state: &mut AddonState) {
    state.main.game_db_loading = true;
    let cache_dir = state.addon_dir.join("cache");
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let cache = gw2_api::cache::DataCache::new(&cache_dir);
        let result = gw2_optimizer::gamedb::GameDb::load(&cache);

        if token.is_cancelled() { return; }

        crate::state::with_state(|s| {
            s.main.game_db_loading = false;
            match result {
                Ok(db) => {
                    nexus::log::log(
                        nexus::log::LogLevel::Info,
                        "GW2 Build Optimizer",
                        &db.summary(),
                    );
                    s.main.game_db = Some(db);
                    // If build tabs were loaded before GameDb, trigger resolve now
                    if s.main.selected_build_tab.is_some() && s.main.selected_equipment_tab.is_some() {
                        resolve_selected_build_inner(s);
                    }
                }
                Err(e) => {
                    s.main.error = Some(format!("Failed to load game data: {}", e));
                }
            }
        });
    });
}

/// Start optimization in background thread (S11-T01, S11-T02, S11-T03)
fn start_optimization(state: &mut AddonState) {
    // Guard against concurrent optimization
    if state.main.optimizing {
        return;
    }

    // Get profession from current build
    let profession_name = state.main.current_build.as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();

    start_optimization_with_profession(state, &profession_name);
}

/// Start optimization with explicit profession name (avoids borrow conflicts).
/// Uses `state.main.build_locks` for spec/trait lock constraints.
fn start_optimization_with_profession(state: &mut AddonState, profession_name: &str) {
    if state.main.game_db.is_none() {
        state.main.error = Some("Game data not loaded. Wait for cache to load.".into());
        return;
    }

    if profession_name.is_empty() {
        state.main.error = Some("No character selected".into());
        return;
    }

    let db = state.main.game_db.clone();
    let profession_name = profession_name.to_string();
    let config = state.config.clone();
    let game_mode = state.main.game_mode.clone();
    let game_mode_label = game_mode.label().to_string();
    let current_build_summary = state.main.current_build.as_ref()
        .map(|b| summarize_resolved_build(b));
    let addon_dir = state.addon_dir.clone();
    let token = state.cancel_token.clone();
    let weights = state.main.weights.clone();
    let build_locks = state.main.build_locks.clone();

    state.main.optimizing = true;
    state.main.optimize_stage = "Starting...".into();

    // Log the weights and deterministic gear prefix for debugging
    let gear_match = gw2_optimizer::scoring::select_gear_prefix(&weights);
    nexus::log::log(
        nexus::log::LogLevel::Info,
        "GW2BuildOpt",
        &format!(
            "Optimizing {}/{}: weights P={:.2} D={:.2} C={:.2} H={:.2} S={:.2} ({}) -> gear: {} (sim={:.3})",
            profession_name, game_mode_label,
            weights.power, weights.disable, weights.condition, weights.healing, weights.sustain,
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
                if token.is_cancelled() { return Err("Cancelled".into()); }

                let db = db.ok_or("GameDb not loaded")?;

                // ═══ Primary: Deterministic synergy engine (no LLM for build selection) ═══
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
                        &game_mode,
                        llm_ref,
                        current_build_summary.as_deref(),
                        &build_locks,
                        &mut |progress: gw2_optimizer::engine::OptimizeProgress| {
                            if token_det.is_cancelled() { return; }
                            crate::state::with_state(|s| {
                                s.main.optimize_stage = progress.stage.clone();
                            });
                        },
                    ) {
                        Ok(synergy_result) => {
                            if token.is_cancelled() { return Err("Cancelled".into()); }
                            let suggestion = synergy_result_to_suggestion(&synergy_result, &profession_name);
                            return Ok(vec![suggestion]);
                        }
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                &format!("Deterministic engine failed, trying Gemini pipeline: {}", e),
                            );
                            // Fall through to Gemini pipeline
                        }
                    }
                }

                // ═══ Fallback 1: LLM synergy pipeline (LLM-driven build selection) ═══
                if config.has_active_llm_key() {
                    let llm_client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                        .map_err(|e| e.to_string())?;

                    let token_synergy = token.clone();
                    match gw2_optimizer::engine::optimize_with_gemini(
                        &db,
                        &profession_name,
                        &weights,
                        &game_mode,
                        llm_client.as_ref(),
                        current_build_summary.as_deref(),
                        &build_locks,
                        &mut |progress| {
                            if token_synergy.is_cancelled() { return; }
                            crate::state::with_state(|s| {
                                s.main.optimize_stage = progress.stage.clone();
                            });
                        },
                    ) {
                        Ok(synergy_result) => {
                            if token.is_cancelled() { return Err("Cancelled".into()); }
                            let suggestion = synergy_result_to_suggestion(&synergy_result, &profession_name);
                            return Ok(vec![suggestion]);
                        }
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                &format!("LLM pipeline failed, falling back to legacy: {}", e),
                            );
                            // Fall through to legacy pipeline
                        }
                    }
                }

                // ═══ Legacy pipeline (fallback or no Gemini key) ═══
                let profession = db.profession(&profession_name)
                    .ok_or_else(|| format!("Profession '{}' not found in GameDb", profession_name))?;

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
                        if token_progress.is_cancelled() { return; }
                        crate::state::with_state(|s| {
                            s.main.optimize_stage = progress.stage.clone();
                        });
                    },
                    5,
                    &game_mode,
                    &build_locks,
                )?;

                if token.is_cancelled() { return Err("Cancelled".into()); }

                let mut suggestions: Vec<crate::ui::comparison::BuildSuggestion> =
                    candidates.iter().map(|c| candidate_to_suggestion(c, &db)).collect();

                // Enrich top suggestion with LLM reasoning (legacy path)
                if config.has_active_llm_key() {
                    if token.is_cancelled() { return Err("Cancelled".into()); }

                    crate::state::with_state(|s| {
                        s.main.optimize_stage = "Consulting AI for synergy analysis...".into();
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
                    ) {
                        Ok(()) => {}
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                &format!("LLM enrichment skipped: {}", e),
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
                    match result {
                        Ok(suggestions) => {
                            s.main.comparison.suggestions = suggestions;
                            s.main.comparison.selected_suggestion = 0;
                        }
                        Err(e) => {
                            s.main.comparison.error = Some(e);
                        }
                    }
                });
            }
        }));

        // If the thread panicked, recover and show error
        if let Err(panic_info) = thread_result {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("Internal error (panic): {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("Internal error (panic): {}", s)
            } else {
                "Internal error: optimization thread panicked".into()
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

/// Convert CombatPerformance to the display-friendly CombatMetrics bridge type.
fn perf_to_combat_metrics(perf: &gw2_optimizer::combat::CombatPerformance) -> gw2_core::types::CombatMetrics {
    gw2_core::types::CombatMetrics {
        effective_power: perf.effective_power.round() as i32,
        strike_dps_index: perf.strike_dps_index.round() as i32,
        condition_dps_index: perf.condition_dps_index.round() as i32,
        total_dps_index: perf.total_dps_index.round() as i32,
        healing_index: perf.healing_power_index.round() as i32,
        crit_chance: perf.crit_chance,
        boon_duration_pct: perf.boon_duration_pct,
        condi_duration_pct: perf.condi_duration_pct,
        effective_health: perf.effective_health.round() as i32,
        damage_reduction_pct: perf.damage_reduction_pct,
        bleeding_tick: perf.condition_ticks.bleeding.round() as i32,
        burning_tick: perf.condition_ticks.burning.round() as i32,
        poison_tick: perf.condition_ticks.poison.round() as i32,
        torment_tick: perf.condition_ticks.torment.round() as i32,
        confusion_tick: perf.condition_ticks.confusion.round() as i32,
    }
}

/// Compute 3-tier combat metrics (Solo, Party, Full Squad) from stats + modifiers.
fn compute_3tier_combat(
    stats: &gw2_optimizer::stats::StatBlock,
    derived: &gw2_optimizer::stats::DerivedStats,
    modifiers: &gw2_optimizer::combat::DamageModifiers,
    profession: &str,
) -> (Option<gw2_core::types::CombatMetrics>, Option<gw2_core::types::CombatMetrics>, Option<gw2_core::types::CombatMetrics>) {
    let profiles = gw2_optimizer::combat::default_buff_profiles();
    let compute = |profile: &gw2_optimizer::combat::BuffProfile| -> gw2_core::types::CombatMetrics {
        let perf = gw2_optimizer::combat::calculate_combat_performance(
            stats, derived, modifiers, profile, profession,
        );
        perf_to_combat_metrics(&perf)
    };
    (profiles.get(0).map(&compute), profiles.get(1).map(&compute), profiles.get(2).map(&compute))
}

/// Convert BuildCandidate to BuildSuggestion for display (S11-T04)
fn candidate_to_suggestion(
    candidate: &gw2_optimizer::engine::BuildCandidate,
    db: &gw2_optimizer::gamedb::GameDb,
) -> crate::ui::comparison::BuildSuggestion {
    use crate::ui::comparison::BuildSuggestion;

    // Get spec names with actually selected traits (not all 9)
    let mut specializations = Vec::new();
    if let Some(elite_id) = candidate.elite_spec {
        if let Some(spec) = db.spec(elite_id) {
            let traits: Vec<String> = candidate.equipped_traits.iter()
                .filter(|tid| spec.major_traits.contains(tid))
                .filter_map(|&tid| db.traits.get(&tid).map(|t| t.name.clone()))
                .collect();
            specializations.push((format!("{} [E]", spec.name), traits));
        }
    }
    for &core_id in &candidate.core_specs {
        if let Some(spec) = db.spec(core_id) {
            let traits: Vec<String> = candidate.equipped_traits.iter()
                .filter(|tid| spec.major_traits.contains(tid))
                .filter_map(|&tid| db.traits.get(&tid).map(|t| t.name.clone()))
                .collect();
            specializations.push((spec.name.clone(), traits));
        }
    }

    // Convert stats from optimizer::stats::StatBlock to core::types::StatBlock
    let estimated_stats = Some(gw2_core::types::StatBlock {
        power: candidate.stats.power.round() as i32,
        precision: candidate.stats.precision.round() as i32,
        toughness: candidate.stats.toughness.round() as i32,
        vitality: candidate.stats.vitality.round() as i32,
        condition_damage: candidate.stats.condition_damage.round() as i32,
        expertise: candidate.stats.expertise.round() as i32,
        concentration: candidate.stats.concentration.round() as i32,
        ferocity: candidate.stats.ferocity.round() as i32,
        healing_power: candidate.stats.healing_power.round() as i32,
        crit_chance: candidate.derived.crit_chance,
        crit_damage: candidate.derived.crit_damage,
        health: candidate.derived.health.round() as i32,
        armor: candidate.derived.armor.round() as i32,
    });

    // Compute combat metrics for all 3 buff profiles
    let profession_name = db.professions.values().next().map(|p| p.name.as_str()).unwrap_or("Warrior");
    // Try to determine profession from elite spec
    let prof_name = if let Some(elite_id) = candidate.elite_spec {
        db.spec(elite_id)
            .map(|s| s.profession.as_str())
            .unwrap_or(profession_name)
    } else if let Some(&core_id) = candidate.core_specs.first() {
        db.spec(core_id)
            .map(|s| s.profession.as_str())
            .unwrap_or(profession_name)
    } else {
        profession_name
    };

    let (combat_solo, combat_party, combat_squad) = compute_3tier_combat(
        &candidate.stats, &candidate.derived, &candidate.modifiers, prof_name,
    );

    BuildSuggestion {
        label: format!("Score: {:.2}", candidate.score),
        build_summary: format!("Gear: {}", candidate.gear.stat_prefix_name),
        stat_prefix: candidate.gear.stat_prefix_name.clone(),
        specializations,
        weapons: Vec::new(),
        skills: Vec::new(),
        rune: String::new(),
        sigils: Vec::new(),
        relic: String::new(),
        explanation: String::new(),
        synergy_explanation: String::new(),
        changes_made: Vec::new(),
        estimated_stats,
        combat_solo,
        combat_party,
        combat_squad,
        rotation: None,
    }
}

/// Convert a SynergyResult from the new pipeline into a BuildSuggestion for display.
fn synergy_result_to_suggestion(
    result: &gw2_optimizer::engine::SynergyResult,
    profession_name: &str,
) -> crate::ui::comparison::BuildSuggestion {
    use crate::ui::comparison::BuildSuggestion;

    let v = &result.validated;

    // Specializations: (name, [trait_name1, trait_name2, trait_name3])
    let specializations: Vec<(String, Vec<String>)> = v.specializations.iter().map(|s| {
        let label = if s.elite { format!("{} [E]", s.name) } else { s.name.clone() };
        (label, s.trait_names.clone())
    }).collect();

    // Weapons: flatten into display strings like "Set 1: Sword / Shield"
    let mut weapons = Vec::new();
    let fmt_set = |set: &gw2_optimizer::validation::ValidatedWeaponSet, label: &str| -> Option<String> {
        match (&set.main_hand, &set.off_hand) {
            (Some(main), Some(off)) => Some(format!("{}: {} / {}", label, main, off)),
            (Some(main), None) => Some(format!("{}: {}", label, main)),
            _ => None,
        }
    };
    if let Some(s) = fmt_set(&v.weapons.set1, "Set 1") { weapons.push(s); }
    if let Some(s) = fmt_set(&v.weapons.set2, "Set 2") { weapons.push(s); }

    // Skills: flatten into display strings
    let mut skills = Vec::new();
    if let Some((_, name)) = &v.skills.heal {
        skills.push(format!("Heal: {}", name));
    }
    for util in &v.skills.utilities {
        if let Some((_, name)) = util {
            skills.push(format!("Utility: {}", name));
        }
    }
    if let Some((_, name)) = &v.skills.elite {
        skills.push(format!("Elite: {}", name));
    }

    // Sigils: flatten to display strings
    let sigils: Vec<String> = v.sigils.iter().map(|s| s.name.clone()).collect();

    // Convert stats from optimizer StatBlock (f64) to core StatBlock (i32)
    let derived = gw2_optimizer::stats::compute_derived(&result.stats, profession_name);
    let estimated_stats = Some(gw2_core::types::StatBlock {
        power: result.stats.power.round() as i32,
        precision: result.stats.precision.round() as i32,
        toughness: result.stats.toughness.round() as i32,
        vitality: result.stats.vitality.round() as i32,
        condition_damage: result.stats.condition_damage.round() as i32,
        expertise: result.stats.expertise.round() as i32,
        concentration: result.stats.concentration.round() as i32,
        ferocity: result.stats.ferocity.round() as i32,
        healing_power: result.stats.healing_power.round() as i32,
        crit_chance: derived.crit_chance,
        crit_damage: derived.crit_damage,
        health: derived.health.round() as i32,
        armor: derived.armor.round() as i32,
    });

    // Convert combat performance to CombatMetrics
    let combat_solo = Some(perf_to_combat_metrics(&result.combat_solo));
    let combat_party = Some(perf_to_combat_metrics(&result.combat_party));
    let combat_squad = Some(perf_to_combat_metrics(&result.combat_squad));

    // Convert rotation simulation result
    let rotation = result.rotation.as_ref().map(|sim| {
        gw2_core::types::RotationBreakdown {
            simulated_dps: sim.total_dps.round() as i32,
            strike_dps: sim.strike_dps.round() as i32,
            condition_dps: sim.condition_dps.round() as i32,
            condition_uptime: sim.condition_uptime.iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            buff_uptime: sim.buff_uptime.iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            skill_usage: sim.skill_usage.iter()
                .map(|s| (s.name.clone(), s.cast_count, s.dps_contribution.round() as i32))
                .collect(),
            stunbreak_count: sim.stunbreak_count,
            has_stability: sim.has_stability,
            stability_uptime: sim.stability_uptime,
        }
    });

    // Build changes_made from validated structured changes
    let changes_made: Vec<String> = v.changes.iter().map(|c| {
        if c.from.is_empty() {
            format!("[{}] → {} ({})", c.slot, c.to, c.reason)
        } else {
            format!("[{}] {} → {} ({})", c.slot, c.from, c.to, c.reason)
        }
    }).collect();

    // Warnings as additional info
    let mut explanation = v.explanation.clone();
    if !v.warnings.is_empty() {
        if !explanation.is_empty() { explanation.push_str("\n\n"); }
        explanation.push_str("Warnings: ");
        explanation.push_str(&v.warnings.join("; "));
    }

    BuildSuggestion {
        label: "Synergy Build".into(),
        build_summary: format!(
            "Gear: {}",
            v.gear_prefix.as_ref().map(|p| p.name.as_str()).unwrap_or("Unknown")
        ),
        stat_prefix: v.gear_prefix.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
        specializations,
        weapons,
        skills,
        rune: v.rune.as_ref().map(|r| r.name.clone()).unwrap_or_default(),
        sigils,
        relic: v.relic.as_ref().map(|r| r.name.clone()).unwrap_or_default(),
        explanation,
        synergy_explanation: v.synergy_explanation.clone(),
        changes_made,
        estimated_stats,
        combat_solo,
        combat_party,
        combat_squad,
        rotation,
    }
}

/// Run rotation simulation for a suggestion's skills and attach the results.
///
/// Resolves ALL build skills: weapon skills from both weapon sets (tagged for
/// weapon swap scheduling) + heal/utility/elite from the skills list.
/// The simulator uses DPCT-optimal scheduling with automatic weapon swapping.
fn simulate_suggestion_rotation(
    suggestion: &mut crate::ui::comparison::BuildSuggestion,
    db: &gw2_optimizer::gamedb::GameDb,
) {
    if suggestion.skills.is_empty() && suggestion.weapons.is_empty() {
        return;
    }

    let mut all_rotation_skills: Vec<gw2_optimizer::rotation::RotationSkill> = Vec::new();

    // 1. Resolve weapon skills from suggestion.weapons (format: "Set 1: Axe / Axe")
    if !suggestion.weapons.is_empty() {
        let profession = infer_profession_from_specs(&suggestion.specializations, db);
        let weapon_sets = parse_weapon_sets(&suggestion.weapons);

        for (set_num, weapon_types) in &weapon_sets {
            let mut set_skill_ids: Vec<u32> = Vec::new();
            for wtype in weapon_types {
                for skill in db.skills.values() {
                    if skill.weapon_type.as_deref() == Some(wtype.as_str())
                        && skill.professions.iter().any(|p| p.eq_ignore_ascii_case(&profession))
                        && skill.slot.as_deref().map(|s| s.starts_with("Weapon_")).unwrap_or(false)
                        && !set_skill_ids.contains(&skill.id)
                    {
                        set_skill_ids.push(skill.id);
                    }
                }
            }
            if !set_skill_ids.is_empty() {
                let mut set_skills = gw2_optimizer::rotation::builder::build_rotation_skills(&set_skill_ids, db);
                gw2_optimizer::rotation::builder::tag_weapon_set(&mut set_skills, *set_num);
                all_rotation_skills.extend(set_skills);
            }
        }
    }

    // 2. Resolve heal/utility/elite from suggestion.skills
    //    Format: "Heal: Name", "Utils: Name1, Name2, Name3", "Elite: Name"
    let skill_names = parse_skill_names(&suggestion.skills);
    for name in &skill_names {
        if let Some(skill) = db.skills.values().find(|s| s.name.eq_ignore_ascii_case(name)) {
            if !all_rotation_skills.iter().any(|rs| rs.skill_id == skill.id) {
                let mut rs_vec = gw2_optimizer::rotation::builder::build_rotation_skills(&[skill.id], db);
                // Non-weapon skills stay at weapon_set=0 (always available)
                all_rotation_skills.append(&mut rs_vec);
            }
        }
    }

    if all_rotation_skills.is_empty() {
        return;
    }

    // Extract stats from estimated_stats for the simulation
    let stats = suggestion.estimated_stats.as_ref();
    let power = stats.map(|s| s.power as f64).unwrap_or(1000.0);
    let condition_damage = stats.map(|s| s.condition_damage as f64).unwrap_or(0.0);
    let weapon_strength = 1100.0; // reference weapon strength (same as combat.rs)

    let result = gw2_optimizer::rotation::simulator::simulate(
        &all_rotation_skills, 0, power, condition_damage, weapon_strength,
    );

    suggestion.rotation = Some(gw2_core::types::RotationBreakdown {
        simulated_dps: result.total_dps.round() as i32,
        strike_dps: result.strike_dps.round() as i32,
        condition_dps: result.condition_dps.round() as i32,
        condition_uptime: result.condition_uptime.into_iter().collect(),
        buff_uptime: result.buff_uptime.into_iter().collect(),
        skill_usage: result.skill_usage.iter()
            .map(|su| (su.name.clone(), su.cast_count, su.dps_contribution.round() as i32))
            .collect(),
        stunbreak_count: result.stunbreak_count,
        has_stability: result.has_stability,
        stability_uptime: result.stability_uptime,
    });
}

/// Parse weapon sets from suggestion.weapons strings.
/// Input format: "Set 1: Axe / Axe", "Set 2: Greatsword"
/// Returns: [(1, ["Axe", "Axe"]), (2, ["Greatsword"])]
fn parse_weapon_sets(weapons: &[String]) -> Vec<(u8, Vec<String>)> {
    let mut sets = Vec::new();
    for w in weapons {
        let set_num = if w.starts_with("Set 1") { 1u8 }
            else if w.starts_with("Set 2") { 2u8 }
            else { 1u8 }; // fallback

        let rest = w.split(':').nth(1).unwrap_or(w).trim();
        let types: Vec<String> = rest.split('/')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "null")
            .collect();

        if !types.is_empty() {
            sets.push((set_num, types));
        }
    }
    sets
}

/// Parse individual skill names from formatted suggestion.skills strings.
/// "Heal: Mending" → "Mending"
/// "Utils: Blood Reckoning, Bull's Charge, Signet of Fury" → 3 names
/// "Elite: Head Butt" → "Head Butt"
fn parse_skill_names(skills: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for s in skills {
        if let Some(rest) = s.strip_prefix("Heal: ") {
            names.push(rest.trim().to_string());
        } else if let Some(rest) = s.strip_prefix("Utils: ") {
            for name in rest.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        } else if let Some(rest) = s.strip_prefix("Elite: ") {
            names.push(rest.trim().to_string());
        } else if let Some(rest) = s.strip_prefix("Utility: ") {
            let name = rest.trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
        } else {
            // Fallback: try the whole string as a skill name
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                names.push(trimmed.to_string());
            }
        }
    }
    names
}

/// Infer profession name from specialization names in the suggestion.
fn infer_profession_from_specs(
    specs: &[(String, Vec<String>)],
    db: &gw2_optimizer::gamedb::GameDb,
) -> String {
    for (spec_name, _) in specs {
        let clean = spec_name.replace(" [E]", "");
        for spec in db.specializations.values() {
            if spec.name.eq_ignore_ascii_case(&clean) {
                return spec.profession.clone();
            }
        }
    }
    // Fallback to first profession in db
    db.professions.values().next().map(|p| p.name.clone()).unwrap_or_default()
}

// infer_weights_from_stats is now in radar_chart.rs

/// Summarize a ResolvedBuild as text for LLM prompts.
fn summarize_resolved_build(build: &gw2_core::types::ResolvedBuild) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Profession: {}", build.profession));

    let specs: Vec<String> = build.specializations.iter()
        .map(|s| {
            let elite = if s.elite { " [E]" } else { "" };
            let traits: Vec<&str> = s.traits_selected.iter().map(|t| t.name.as_str()).collect();
            format!("{}{}: {}", s.name, elite, traits.join(", "))
        }).collect();
    if !specs.is_empty() {
        parts.push(format!("Specs: {}", specs.join(" | ")));
    }

    if let Some(ref h) = build.skills.heal {
        parts.push(format!("Heal: {}", h.name));
    }
    let utils: Vec<String> = build.skills.utilities.iter()
        .filter_map(|u| u.as_ref().map(|s| s.name.clone())).collect();
    if !utils.is_empty() {
        parts.push(format!("Utils: {}", utils.join(", ")));
    }
    if let Some(ref e) = build.skills.elite {
        parts.push(format!("Elite: {}", e.name));
    }

    for set in &build.weapons {
        let mut w = Vec::new();
        if let Some(ref mh) = set.main_hand { w.push(mh.weapon_type.clone()); }
        if let Some(ref oh) = set.off_hand { w.push(oh.weapon_type.clone()); }
        if !w.is_empty() {
            parts.push(format!("{}: {}", set.label, w.join(" / ")));
        }
    }

    if !build.armor.is_empty() && !build.armor[0].stat_prefix.is_empty() {
        parts.push(format!("Gear: {}", build.armor[0].stat_prefix));
    }
    if let Some(ref r) = build.rune {
        parts.push(format!("Rune: {}", r.name));
    }
    if let Some(ref r) = build.relic {
        parts.push(format!("Relic: {}", r.name));
    }

    parts.join("\n")
}

/// Apply Gemini's parsed response onto a BuildSuggestion.
fn apply_gemini_response(
    suggestion: &mut crate::ui::comparison::BuildSuggestion,
    gemini: &gw2_optimizer::prompts::GeminiBuildResponse,
) {
    if !gemini.explanation.is_empty() {
        suggestion.explanation = gemini.explanation.clone();
    }
    if let Some(ref synergy) = gemini.synergy_explanation {
        if !synergy.is_empty() {
            suggestion.synergy_explanation = synergy.clone();
        }
    }
    if !gemini.specializations.is_empty() {
        suggestion.specializations = gemini.specializations.clone();
    }
    if !gemini.weapons.is_empty() {
        suggestion.weapons = gemini.weapons.clone();
    }
    if !gemini.skills.is_empty() {
        suggestion.skills = gemini.skills.clone();
    }
    if !gemini.rune.is_empty() {
        suggestion.rune = gemini.rune.clone();
    }
    if !gemini.sigils.is_empty() {
        suggestion.sigils = gemini.sigils.clone();
    }
    if !gemini.relic.is_empty() {
        suggestion.relic = gemini.relic.clone();
    }
    if !gemini.stat_prefix.is_empty() {
        suggestion.stat_prefix = gemini.stat_prefix.clone();
    }
    if !gemini.changes_made.is_empty() {
        suggestion.changes_made = gemini.changes_made.clone();
    }
}

/// Convert Gemini tool function names to human-readable descriptions.
fn humanize_tool_names(tool_names: &[String]) -> String {
    let labels: Vec<&str> = tool_names.iter().map(|n| match n.as_str() {
        "get_profession_info" => "reading profession",
        "get_spec_traits" => "checking traits",
        "get_trait_details" => "analyzing trait",
        "get_skill_info" => "checking skill",
        "list_runes" => "browsing runes",
        "list_sigils" => "browsing sigils",
        "list_relics" => "browsing relics",
        "calculate_stats" => "calculating stats",
        "simulate_combat" => "simulating combat",
        "score_build" => "scoring build",
        "get_current_build" => "reading current build",
        "get_optimizer_results" => "reviewing candidates",
        "search_traits_by_effect" => "searching trait synergies",
        "find_condition_sources" => "finding condition sources",
        "search_skills_by_effect" => "searching skill synergies",
        "find_synergies" => "analyzing synergies",
        "get_build_synergy_report" => "building synergy report",
        "simulate_rotation" => "simulating rotation",
        _ => "working",
    }).collect();
    labels.join(", ")
}

/// Call the active LLM provider to enrich the top optimizer suggestion with AI reasoning.
/// Uses function calling (tool use) so the LLM can query game data and simulate builds.
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
) -> Result<(), String> {
    let client = gw2_optimizer::llm::create_client(config, addon_dir)
        .map_err(|e| e.to_string())?;

    // Build tool-aware prompt
    let prompt = if current_build_summary.is_some() {
        gw2_optimizer::prompts::improve_build_prompt_with_tools(
            profession_name, weights, game_mode,
        )
    } else {
        gw2_optimizer::prompts::new_build_prompt_with_tools(
            profession_name, weights, game_mode,
        )
    };

    let tools = gw2_optimizer::llm::tools::tool_definitions();
    let build_summary_owned = current_build_summary.map(|s| s.to_string());
    let ctx = gw2_optimizer::gemini_tools::ToolContext {
        db,
        profession_name,
        candidates,
        current_build_summary: build_summary_owned.as_deref(),
        weights: weights.clone(),
    };

    let response = client.generate_with_tools_progress(
        &prompt,
        &tools,
        &mut |name: &str, args: &serde_json::Value| gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx),
        8,
        &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
            let tools_str = humanize_tool_names(tool_names);
            crate::state::with_state(|s| {
                s.main.optimize_stage = format!(
                    "AI thinking ({}/{})... {}",
                    turn, max_turns, tools_str
                );
            });
        },
    ).map_err(|e| e.to_string())?;

    let gemini_build = gw2_optimizer::prompts::parse_gemini_build(&response)
        .map_err(|e| format!("Parse failed: {}", e))?;

    // Validate LLM's output against GameDb before applying
    let validated = gw2_optimizer::validation::validate_gemini_build(&gemini_build, db, profession_name);
    if !validated.errors.is_empty() {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            &format!("Legacy enrichment validation errors: {}", validated.errors.join("; ")),
        );
    }

    crate::state::with_state(|s| {
        s.main.optimize_stage = "Applying AI build + simulating rotation...".into();
    });

    if let Some(first) = suggestions.first_mut() {
        apply_gemini_response(first, &gemini_build);
        // Run rotation simulation now that LLM has populated skills
        simulate_suggestion_rotation(first, db);
    }

    Ok(())
}

/// Send a chat message to the active LLM provider for build refinement.
/// Uses function calling so the LLM can query game data to answer questions.
fn send_chat_message(state: &mut AddonState, message: String) {
    // Guard against concurrent chat messages
    if state.main.chat.waiting {
        return;
    }

    if !state.config.has_active_llm_key() {
        crate::ui::chat_bar::add_ai_response(
            &mut state.main.chat,
            "No AI API key configured. Set one in Settings.".into(),
        );
        return;
    }

    let config = state.config.clone();
    let profession = state.main.current_build.as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();
    let build_summary = state.main.current_build.as_ref()
        .map(|b| summarize_resolved_build(b));
    let addon_dir = state.addon_dir.clone();
    let token = state.cancel_token.clone();
    let db_clone = state.main.game_db.clone();
    let weights = state.main.weights.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let result = (|| -> Result<gw2_optimizer::prompts::GeminiBuildResponse, String> {
            let client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                .map_err(|e| e.to_string())?;

            if token.is_cancelled() { return Err("Cancelled".into()); }

            // Use tool-enabled generation if GameDb is available
            if let Some(ref db) = db_clone {
                let prompt = gw2_optimizer::prompts::chat_refinement_prompt_with_tools(
                    &profession, &message,
                );
                let tools = gw2_optimizer::llm::tools::tool_definitions();
                let empty_candidates = vec![];
                let ctx = gw2_optimizer::gemini_tools::ToolContext {
                    db,
                    profession_name: &profession,
                    candidates: &empty_candidates,
                    current_build_summary: build_summary.as_deref(),
                    weights: weights.clone(),
                };

                let response = client.generate_with_tools_progress(
                    &prompt,
                    &tools,
                    &mut |name: &str, args: &serde_json::Value| gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx),
                    8,
                    &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                        let tools_str = humanize_tool_names(tool_names);
                        crate::state::with_state(|s| {
                            s.main.optimize_stage = format!(
                                "AI thinking ({}/{})... {}",
                                turn, max_turns, tools_str
                            );
                        });
                    },
                ).map_err(|e| e.to_string())?;

                gw2_optimizer::prompts::parse_gemini_build(&response)
                    .map_err(|e| format!("Parse failed: {}", e))
            } else {
                // Fallback: no GameDb, use simple prompt
                let build_summary_str = build_summary.as_deref().unwrap_or("");
                let context = gw2_optimizer::prompts::build_game_context(
                    &profession, &weights, "PvE",
                );
                let prompt = gw2_optimizer::prompts::chat_refinement_prompt(
                    &profession, build_summary_str, &message, &context,
                );
                let response = client.generate_cached(&prompt)
                    .map_err(|e| e.to_string())?;
                gw2_optimizer::prompts::parse_gemini_build(&response)
                    .map_err(|e| format!("Parse failed: {}", e))
            }
        })();

        // Validate Gemini's response against GameDb before applying (if available)
        let validated_result = result.as_ref().ok().and_then(|gemini_build| {
            db_clone.as_ref().map(|db| {
                let validated = gw2_optimizer::validation::validate_gemini_build(gemini_build, db, &profession);
                if !validated.errors.is_empty() {
                    nexus::log::log(
                        nexus::log::LogLevel::Warning,
                        "GW2BuildOpt",
                        &format!("Chat refinement validation errors: {}", validated.errors.join("; ")),
                    );
                }
                validated
            })
        });
        let _ = validated_result; // Validation logged; apply_gemini_response uses raw parsed fields

        if !token.is_cancelled() {
            crate::state::with_state(|s| {
                match result {
                    Ok(gemini_build) => {
                        let display = if gemini_build.explanation.is_empty() {
                            "Build updated.".to_string()
                        } else {
                            gemini_build.explanation.clone()
                        };
                        crate::ui::chat_bar::add_ai_response(&mut s.main.chat, display);

                        let mut suggestion = crate::ui::comparison::BuildSuggestion {
                            label: "Chat Refinement".into(),
                            ..Default::default()
                        };
                        apply_gemini_response(&mut suggestion, &gemini_build);
                        if let Some(ref db) = s.main.game_db {
                            simulate_suggestion_rotation(&mut suggestion, db);
                        }
                        s.main.comparison.error = None;
                        s.main.comparison.suggestions.push(suggestion);
                        s.main.comparison.selected_suggestion =
                            s.main.comparison.suggestions.len() - 1;
                    }
                    Err(e) => {
                        crate::ui::chat_bar::add_ai_response(
                            &mut s.main.chat,
                            format!("Error: {}", e),
                        );
                    }
                }
            });
        }
    });
}

// ─── Save/Load ───────────────────────────────────────────────────────────

/// Render the save build UI (name input + Save button) below the comparison view.
fn render_save_build_ui(ui: &Ui, state: &mut AddonState) {
    if state.main.comparison.suggestions.is_empty() {
        return;
    }
    ui.spacing();
    ui.separator();
    ui.text("Save Build:");
    ui.same_line();
    ui.set_next_item_width(200.0);
    ui.input_text("##save_name", &mut state.main.save_name_input).build();
    ui.same_line();

    let can_save = !state.main.save_name_input.trim().is_empty();
    let save_clicked = if can_save {
        ui.button_with_size("Save", [60.0, 0.0])
    } else {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size("Save", [60.0, 0.0]);
        style.pop();
        if ui.is_item_hovered() {
            ui.tooltip_text("Enter a build name first");
        }
        false
    };
    if save_clicked {
        let idx = state.main.comparison.selected_suggestion
            .min(state.main.comparison.suggestions.len().saturating_sub(1));
        let suggestion = &state.main.comparison.suggestions[idx];
        let character_name = state.main.current_build.as_ref()
            .map(|b| b.character_name.clone())
            .unwrap_or_default();
        let game_mode = state.main.game_mode.clone();
        let saved = suggestion_to_saved(
            &state.main.save_name_input,
            &character_name,
            &game_mode,
            suggestion,
        );

        let storage = gw2_core::storage::BuildStorage::new(&state.addon_dir);
        match storage.save(&saved) {
            Ok(()) => {
                state.main.save_status = Some(format!("Saved '{}'", saved.name));
                state.main.save_status_frames = 0;
                state.main.save_name_input.clear();
                state.main.saved_builds_loaded = false; // force refresh
            }
            Err(e) => {
                state.main.save_status = Some(format!("Save failed: {}", e));
                state.main.save_status_frames = 0;
            }
        }
    }

    if let Some(ref status) = state.main.save_status {
        ui.same_line();
        if status.starts_with("Save failed") {
            ui.text_colored([1.0, 0.3, 0.0, 1.0], status);
        } else {
            ui.text_colored([0.0, 1.0, 0.0, 1.0], status);
        }
    }
}

/// Render the Save/Load tab.
fn render_saveload_tab(ui: &Ui, state: &mut AddonState) {
    // Lazy-load saved builds on first view
    if !state.main.saved_builds_loaded {
        let storage = gw2_core::storage::BuildStorage::new(&state.addon_dir);
        state.main.saved_builds = storage.list();
        state.main.saved_builds_loaded = true;
    }

    build_display::render_card_header(
        ui,
        &format!("SAVED BUILDS ({})", state.main.saved_builds.len()),
        [1.0, 0.88, 0.35, 1.0],
    );

    if state.main.saved_builds.is_empty() {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "No saved builds yet.");
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "Optimize a build, then use Save to store it here.");
        return;
    }

    // Snapshot for iteration (avoids borrow conflict with mut state)
    let builds_snapshot: Vec<(String, String, String, String, String)> = state.main.saved_builds.iter()
        .map(|b| {
            let time = format_timestamp(b.timestamp);
            let mode = b.game_mode.label().to_string();
            (b.name.clone(), b.character_name.clone(), b.stat_prefix.clone(), time, mode)
        })
        .collect();

    let mut load_idx: Option<usize> = None;
    let mut request_delete: Option<usize> = None;

    for (i, (name, character, prefix, time, mode)) in builds_snapshot.iter().enumerate() {
        // Name + action buttons on one line
        ui.text_colored([0.6, 0.8, 1.0, 1.0], name);
        ui.same_line();
        if ui.button_with_size(&format!("Load##load_{}", i), [50.0, 0.0]) {
            load_idx = Some(i);
        }
        ui.same_line();

        // Delete with confirmation
        if state.main.confirm_delete == Some(i) {
            ui.text_colored([1.0, 0.3, 0.0, 1.0], "Delete?");
            ui.same_line();
            if ui.small_button(&format!("Yes##confirm_del_{}", i)) {
                request_delete = Some(i);
                state.main.confirm_delete = None;
            }
            ui.same_line();
            if ui.small_button(&format!("No##cancel_del_{}", i)) {
                state.main.confirm_delete = None;
            }
        } else if ui.button_with_size(&format!("Delete##del_{}", i), [50.0, 0.0]) {
            state.main.confirm_delete = Some(i);
        }

        // Details on second line
        ui.text_colored([0.55, 0.55, 0.55, 1.0], &format!("  {} | {} | {} | {}", character, mode, prefix, time));

        ui.spacing();
    }

    // Handle load
    if let Some(idx) = load_idx {
        let saved = state.main.saved_builds[idx].clone();
        let mut suggestion = saved_to_suggestion(&saved);
        // Run rotation simulation if GameDb is available
        if let Some(ref db) = state.main.game_db {
            simulate_suggestion_rotation(&mut suggestion, db);
        }
        state.main.comparison.suggestions = vec![suggestion];
        state.main.comparison.selected_suggestion = 0;
        state.main.comparison.error = None;
        state.main.active_tab = MainTab::NewBuild;
    }

    // Handle delete (confirmed)
    if let Some(idx) = request_delete {
        let name = state.main.saved_builds[idx].name.clone();
        let storage = gw2_core::storage::BuildStorage::new(&state.addon_dir);
        match storage.delete(&name) {
            Ok(()) => {
                state.main.saved_builds.remove(idx);
                // Reset confirmation index if it was beyond the removed item
                if let Some(ref mut ci) = state.main.confirm_delete {
                    if *ci > idx { *ci -= 1; }
                    else if *ci == idx { state.main.confirm_delete = None; }
                }
            }
            Err(e) => {
                state.main.error = Some(format!("Delete failed: {}", e));
            }
        }
    }
}

/// Convert a BuildSuggestion to a SavedBuild.
fn suggestion_to_saved(
    name: &str,
    character_name: &str,
    game_mode: &gw2_core::types::GameMode,
    suggestion: &crate::ui::comparison::BuildSuggestion,
) -> gw2_core::types::SavedBuild {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    gw2_core::types::SavedBuild {
        name: name.trim().to_string(),
        timestamp,
        character_name: character_name.to_string(),
        game_mode: game_mode.clone(),
        label: suggestion.label.clone(),
        stat_prefix: suggestion.stat_prefix.clone(),
        specializations: suggestion.specializations.clone(),
        weapons: suggestion.weapons.clone(),
        skills: suggestion.skills.clone(),
        rune: suggestion.rune.clone(),
        sigils: suggestion.sigils.clone(),
        relic: suggestion.relic.clone(),
        explanation: suggestion.explanation.clone(),
        synergy_explanation: suggestion.synergy_explanation.clone(),
        changes_made: suggestion.changes_made.clone(),
        estimated_stats: suggestion.estimated_stats.clone(),
    }
}

/// Convert a SavedBuild back to a BuildSuggestion for display.
/// Recomputes combat metrics from estimated stats if available.
fn saved_to_suggestion(
    saved: &gw2_core::types::SavedBuild,
) -> crate::ui::comparison::BuildSuggestion {
    // Recompute combat metrics from saved stats (lossy i32→f64 but good enough for display)
    let (combat_solo, combat_party, combat_squad) = saved.estimated_stats.as_ref()
        .map(|est| {
            let stats = gw2_optimizer::stats::StatBlock {
                power: est.power as f64,
                precision: est.precision as f64,
                toughness: est.toughness as f64,
                vitality: est.vitality as f64,
                condition_damage: est.condition_damage as f64,
                expertise: est.expertise as f64,
                concentration: est.concentration as f64,
                ferocity: est.ferocity as f64,
                healing_power: est.healing_power as f64,
            };
            // Use a generic profession name — the exact profession mainly affects health
            let derived = gw2_optimizer::stats::compute_derived(&stats, "Warrior");
            let mods = gw2_optimizer::combat::DamageModifiers::default();
            compute_3tier_combat(&stats, &derived, &mods, "Warrior")
        })
        .unwrap_or((None, None, None));

    crate::ui::comparison::BuildSuggestion {
        label: if saved.label.is_empty() { saved.name.clone() } else { saved.label.clone() },
        build_summary: String::new(),
        stat_prefix: saved.stat_prefix.clone(),
        specializations: saved.specializations.clone(),
        weapons: saved.weapons.clone(),
        skills: saved.skills.clone(),
        rune: saved.rune.clone(),
        sigils: saved.sigils.clone(),
        relic: saved.relic.clone(),
        explanation: saved.explanation.clone(),
        synergy_explanation: saved.synergy_explanation.clone(),
        changes_made: saved.changes_made.clone(),
        estimated_stats: saved.estimated_stats.clone(),
        combat_solo,
        combat_party,
        combat_squad,
        rotation: None,
    }
}

/// Format a Unix timestamp as a readable date+time string (YYYY-MM-DD HH:MM).
fn format_timestamp(timestamp: u64) -> String {
    let secs_per_day: u64 = 86400;
    let day_secs = timestamp % secs_per_day;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let days = timestamp / secs_per_day;
    // Days since epoch to Y/M/D
    let mut y = 1970u64;
    let mut remaining_days = days;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days_in_months: [u64; 12] = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut m = 11;
    for (i, &dim) in days_in_months.iter().enumerate() {
        if remaining_days < dim {
            m = i;
            break;
        }
        remaining_days -= dim;
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m + 1, remaining_days + 1, hours, minutes)
}
