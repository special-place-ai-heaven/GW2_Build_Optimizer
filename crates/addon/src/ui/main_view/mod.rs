use nexus::imgui::{ChildWindow, ComboBox, Selectable, Ui};

use crate::state::{AddonState, MainTab};
use gw2_optimizer::scoring::OptimizationWeights;

mod build_display;
mod lock_panel;
mod character;
mod resolution;
mod optimization;
mod stats;

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
        character::load_characters(state);
    }

    // Load GameDb once on first entry (S11-T06)
    if state.main.game_db.is_none() && !state.main.game_db_loading {
        stats::load_game_db(state);
    }

    // Periodic API health check (~every 60s at 60fps)
    state.main.api_status_frames += 1;
    if state.main.api_status_frames >= 3600 || state.main.api_status == crate::state::ApiStatus::Unknown {
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
                            resolution::auto_populate_locks(&build_clone, &mut state.main.build_locks);
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
        character::load_character_tabs(state, name);
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
            character::update_build_chat_code(state);
            resolution::resolve_selected_build(state);
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
        optimization::send_chat_message(state, msg);
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
                    if Selectable::new(&format!("{}##sug_{}", label, i))
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
            optimization::send_chat_message(state, msg);
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
            stats::start_fetch_models(state);
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
            stats::start_fetch_models(state);
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
    ui.dummy([0.0, 8.0]);
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
    ui.dummy([0.0, 8.0]);
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
    ui.dummy([0.0, 8.0]);
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
            stats::start_game_data_refresh(state);
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
            stats::start_game_data_refresh(state);
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
        ui.spacing();
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
            optimization::simulate_suggestion_rotation(&mut suggestion, db);
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
            stats::compute_3tier_combat(&stats, &derived, &mods, "Warrior")
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
