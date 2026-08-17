use nexus::imgui::{ChildWindow, ComboBox, Selectable, TreeNodeFlags, Ui};

use crate::state::{AddonState, MainTab};
use crate::ui::theme;
use gw2_core::types::GameMode;
use gw2_optimizer::scenario::{CombatTier, RoleObjective};
use gw2_optimizer::scoring::OptimizationWeights;

pub(crate) mod build_display;
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
    if (state.main.api_status_frames >= 3600
        || state.main.api_status == crate::state::ApiStatus::Unknown)
        && !state.main.api_health_checking
    {
        stats::check_api_health(state);
        state.main.api_status_frames = 0;
    }

    // Auto-dismiss save status after ~180 frames (~3s at 60fps)
    if state.main.save_status.is_some() {
        state.main.save_status_frames += 1;
        if state.main.save_status_frames > 180 {
            state.main.save_status = None;
            state.main.save_status_frames = 0;
        }
    }

    // Chat timeout. Wall clock, not FPS. Include the live model so a stall isn't a mystery.
    const KITCHEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
    if state.main.chat.waiting {
        let started = state
            .main
            .chat_wait_started
            .get_or_insert_with(std::time::Instant::now);
        if started.elapsed() >= KITCHEN_TIMEOUT {
            state.main.chat_epoch = state.main.chat_epoch.wrapping_add(1);
            state.main.chat.waiting = false;
            state.main.chat_wait_started = None;
            state.main.optimize_stage.clear();
            let msg = optimization::format_provider_issue(
                "timeout",
                state.config.active_provider.short_label(),
                state.config.active_model_id(),
            );
            state.main.provider_issue = Some(msg.clone());
            crate::ui::chat_bar::add_ai_response(&mut state.main.chat, msg);
        }
    } else {
        state.main.chat_wait_started = None;
    }

    // ── Top status bar: API health + loading + errors ──
    render_top_status_bar(ui, state);

    // ── Horizontal tab bar (main navigation) ──
    render_top_tabs(ui, state);

    // ── Two-column layout: left dynamic panel + center content ──
    let pad = state.config.panel_padding;
    let content_indent = state.config.content_indent;
    let avail = ui.content_region_avail();
    let left_panel_width = {
        let scale_row = theme::segment_row_min_width(ui, &["Roam", "Havoc", "Cloud/Zerg"]);
        let min_left = (scale_row + pad * 2.0 + 18.0).max(360.0);
        let want = (state.config.left_panel_width * scale).max(min_left);
        let cap = (avail[0] * 0.58).max(min_left);
        want.min(cap)
    };
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

    if state.main.chat.dirty {
        crate::ui::chat_bar::save_history(&state.addon_dir, &state.main.chat.history);
        state.main.chat.dirty = false;
    }
}

/// One always-on chat strip. Follows Current vs Optimized focus.
fn render_focus_chat(ui: &Ui, state: &mut AddonState) {
    let current = state.main.build_chat_code.clone();
    let (source, code) = state.main.comparison.chat_focus(current.as_deref());
    crate::ui::comparison::render_chat_code_copy(
        ui,
        source,
        code.as_deref(),
        "top",
        &mut state.main.copy_feedback_frames,
    );
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
    let (label, color) = match state.main.api_status {
        crate::state::ApiStatus::Unknown => ("Checking API\u{2026}", crate::ui::theme::MUTED),
        crate::state::ApiStatus::Online => ("API ready", crate::ui::theme::OPTIMIZED),
        crate::state::ApiStatus::Degraded => ("API slow", crate::ui::theme::GOLD),
        crate::state::ApiStatus::Offline => ("API offline", crate::ui::theme::ERR),
    };
    {
        let p = ui.cursor_screen_pos();
        ui.get_window_draw_list()
            .add_circle([p[0] + 8.0, p[1] + 8.0], 4.0, color)
            .filled(true)
            .build();
    }
    ui.dummy([16.0, 16.0]);
    ui.same_line();
    ui.text_colored(color, label);
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

    if let (Some(cached), Some(live)) = (
        state.config.cache_build_number,
        state.main.live_build_number,
    ) {
        if cached != live && !state.main.game_db_loading {
            ui.same_line();
            ui.text_colored(
                theme::WARN,
                format!("| Game data stale ({cached} \u{2192} {live})"),
            );
            if ui.is_item_hovered() {
                ui.tooltip_text(
                    "ArenaNet shipped a new game build. Refresh game data — icons stay cached.",
                );
            }
            ui.same_line();
            if theme::gold_button(ui, "Refresh##stale_data") {
                stats::start_game_data_refresh(state);
            }
        }
    }

    // Loading banner (GameDb)
    if state.main.game_db_loading {
        ui.same_line();
        let stage = &state.main.game_refresh_stage;
        if !stage.is_empty() {
            ui.text_colored(theme::WARN, format!("| {}", stage));
        } else {
            ui.text_colored(theme::WARN, "| Loading game data...");
        }
    }

    // Optimization progress
    if state.main.optimizing {
        ui.same_line();
        ui.text_colored(theme::WARN, format!("| {}", state.main.optimize_stage));
    }

    // Error bar (dismissible)
    if let Some(ref err) = state.main.error {
        ui.text_colored(theme::ERR, format!("  [!] {}", err));
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
                [0.10, 0.08, 0.05, 0.95],
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
                crate::ui::theme::GOLD,
            )
            .filled(true)
            .build();
        // Dimmer full-width stripe underneath
        draw_list
            .add_rect(
                [start[0], start[1]],
                [start[0] + width, start[1] + 3.0],
                [0.55, 0.42, 0.16, 0.35],
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
                .add_circle([dot_x, dot_y], 3.5, [1.0, 0.84, 0.38, alpha])
                .filled(true)
                .build();
        }

        // "OPTIMIZING..." title
        draw_list.add_text(
            [start[0] + 56.0, start[1] + 10.0],
            crate::ui::theme::GOLD,
            "Finding a better build\u{2026}",
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
                crate::ui::theme::GOLD_FILL,
            )
            .filled(true)
            .rounding(2.0)
            .build();

        // Border
        draw_list
            .add_rect(
                [start[0], start[1]],
                [start[0] + width, start[1] + 62.0],
                crate::ui::theme::GOLD_DIM,
            )
            .rounding(6.0)
            .build();
    }

    ui.dummy([0.0, 66.0]);
    ui.spacing();
}

/// Horizontal tab bar for main navigation (styled buttons with active indicator).
fn render_top_tabs(ui: &Ui, state: &mut AddonState) {
    let modes = [
        (MainTab::NewBuild, "New Build"),
        (MainTab::Improve, "Improve Build"),
        (MainTab::Talk, "Choya"),
    ];
    for (i, (tab, label)) in modes.iter().enumerate() {
        if i > 0 {
            ui.same_line_with_spacing(0.0, 8.0);
        }
        let is_active = state.main.active_tab == *tab;
        let pulse = if state.main.tab_alert.as_ref() == Some(tab) && !is_active {
            0.45 + 0.55 * (ui.frame_count() as f32 * 0.14).sin().abs()
        } else {
            0.0
        };
        if crate::ui::theme::pill_pulse(
            ui,
            label,
            is_active,
            &format!("##main_tab_{}", label),
            pulse,
        ) {
            state.main.active_tab = tab.clone();
            if state.main.tab_alert.as_ref() == Some(tab) {
                state.main.tab_alert = None;
            }
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

    ui.same_line_with_spacing(0.0, 28.0);
    for (tab, label) in [
        (MainTab::SaveLoad, "Saves"),
        (MainTab::Settings, "Settings"),
    ] {
        let is_active = state.main.active_tab == tab;
        if crate::ui::theme::pill(ui, label, is_active, &format!("##main_tab_{}", label)) {
            state.main.active_tab = tab;
        }
        ui.same_line_with_spacing(0.0, 8.0);
    }

    render_focus_chat(ui, state);
}

/// Dynamic left panel: content varies by active tab.
fn render_left_panel(ui: &Ui, state: &mut AddonState) {
    // ── Character section (always visible except Settings) ──
    if state.main.active_tab != MainTab::Settings {
        render_left_character_section(ui, state);
    }

    match state.main.active_tab {
        MainTab::NewBuild | MainTab::Improve | MainTab::Talk => {
            render_left_build_controls(ui, state);
        }
        MainTab::SaveLoad => {
            // Minimal: just refresh button
            ui.spacing();
            if state.main.characters_loading {
                let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
                theme::gold_button_sized(ui, "Refreshing...", [-1.0, 0.0]);
                style.pop();
            } else if theme::gold_button_sized(ui, "Refresh Data", [-1.0, 0.0]) {
                character::load_characters(state);
            }
        }
        MainTab::Settings => {
            // Settings info
            render_left_section_header(ui, "INFO", state.config.section_spacing);
            ui.text_colored(theme::MUTED, "  GW2 Build Optimizer");
            ui.text_colored(theme::MUTED, format!("  v{}", crate::VERSION));
            ui.spacing();
            let provider_label = state.config.active_provider.label();
            ui.text_colored(theme::MUTED, format!("  AI: {}", provider_label));
        }
    }
}

/// Render a compact section header with accent line in the left panel.
pub(super) fn render_left_section_header(ui: &Ui, title: &str, spacing: f32) {
    ui.dummy([0.0, spacing]); // gap above
    {
        let pos = ui.cursor_screen_pos();
        let width = ui.content_region_avail()[0];
        let th = ui.calc_text_size(title)[1];
        let bar_h = (th + 10.0).max(22.0);
        let ty = pos[1] + ((bar_h - th) * 0.5).round();
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [pos[0], pos[1]],
                [pos[0] + width, pos[1] + bar_h],
                [0.18, 0.15, 0.08, 0.95],
            )
            .filled(true)
            .rounding(4.0)
            .build();
        draw_list
            .add_rect(
                [pos[0], pos[1]],
                [pos[0] + 3.0, pos[1] + bar_h],
                crate::ui::theme::GOLD,
            )
            .filled(true)
            .rounding(2.0)
            .build();
        draw_list.add_text([pos[0] + 8.0, ty], crate::ui::theme::GOLD, title);
        ui.dummy([0.0, bar_h]);
    }
    ui.dummy([0.0, spacing * 0.5]); // gap below
}

/// WvW fight scale: Roam / Havoc / Cloud/Zerg.
/// Independent of role/task — changing scale does not rewrite healer vs DPS weights.
fn render_wvw_sub_role(ui: &Ui, state: &mut AddonState) {
    render_left_section_header(ui, "SCALE", state.config.section_spacing);
    let tiers = [CombatTier::Solo, CombatTier::Party, CombatTier::Squad];
    let labels: Vec<&str> = tiers.iter().map(|t| t.label()).collect();
    let selected = tiers
        .iter()
        .position(|t| *t == state.main.wvw_combat_tier)
        .unwrap_or(2);
    if let Some(i) = theme::segment_row(ui, &labels, selected, "##scale") {
        state.main.wvw_combat_tier = tiers[i];
        state.main.comparison.suggestions.clear();
        state.main.comparison.error = None;
    }
}

fn role_pip(role: RoleObjective) -> [f32; 4] {
    match role {
        RoleObjective::WvWRoamer => [0.95, 0.48, 0.18, 1.0],
        RoleObjective::PowerDps => theme::PIP_DAMAGE,
        RoleObjective::CondiDps => [0.52, 0.82, 0.28, 1.0],
        RoleObjective::Hybrid => [0.95, 0.78, 0.35, 1.0],
        RoleObjective::Sustain => [0.72, 0.52, 0.88, 1.0],
        RoleObjective::Staller => [0.62, 0.48, 0.32, 1.0],
        RoleObjective::Healer => theme::PIP_HEAL,
        RoleObjective::Buffer => [0.32, 0.78, 0.82, 1.0],
        RoleObjective::Disabler => theme::PIP_CTRL,
        RoleObjective::Tank => theme::PIP_FRONT,
        RoleObjective::WvWZergDps | RoleObjective::PvPBurst => theme::PIP_DAMAGE,
        RoleObjective::WvWZergSupport => theme::PIP_HEAL,
        RoleObjective::WvWDisruptor | RoleObjective::PvPDisruptor => theme::PIP_CTRL,
        RoleObjective::PvPSustain => [0.32, 0.78, 0.82, 1.0],
    }
}

fn role_hint(role: RoleObjective) -> &'static str {
    match role {
        RoleObjective::WvWRoamer => "Pick and leave. Burst a target, crack cover, get out.",
        RoleObjective::PowerDps => "Strike damage — burst and sustained power DPS.",
        RoleObjective::CondiDps => "Condition pressure and duration.",
        RoleObjective::Hybrid => {
            "Jack of all trades. Celestial-style, master of none. Occupies until backup."
        }
        RoleObjective::Sustain => "Bruiser — fights and lives.",
        RoleObjective::Staller => {
            "Troll. Stall, don't kill. Evade, port, stealth, speed — live through a blob until the group arrives."
        }
        RoleObjective::Healer => "Healing and group sustain.",
        RoleObjective::Buffer => "Boons and support for allies.",
        RoleObjective::Disabler => "CC, interrupts, boon strip.",
        RoleObjective::Tank => "Frontline / commander — toughness and presence.",
        _ => role.label(),
    }
}

fn apply_role(state: &mut AddonState, role: RoleObjective) {
    state.main.selected_role = Some(role);
    state.main.weights = role.to_weights(&state.main.game_mode);
    state.main.comparison.suggestions.clear();
    state.main.comparison.selected_suggestion = 0;
    state.main.comparison.error = None;
}

fn render_role_chips(ui: &Ui, state: &mut AddonState) {
    render_left_section_header(ui, "ROLE", state.config.section_spacing);

    let current = state.main.selected_role;
    let avail = ui.content_region_avail()[0];
    let mut row_x = 0.0_f32;
    let mut picked: Option<RoleObjective> = None;
    for role in RoleObjective::PLAY_ROLES {
        let label = role.play_label();
        let id = format!("##play_{:?}", role);
        let [cw, _] = theme::select_chip_size(ui, label, true);
        theme::wrap_chip(ui, avail, &mut row_x, cw, 4.0);
        let selected = current == Some(role);
        if theme::select_chip(ui, label, selected, &id, Some(role_pip(role))) {
            picked = Some(role);
        }
        if ui.is_item_hovered() {
            ui.tooltip_text(role_hint(role));
        }
    }
    if let Some(role) = picked {
        apply_role(state, role);
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
            if Selectable::new(name).selected(selected).build(ui)
                && state.main.selected_character != Some(i)
            {
                new_selection = Some((i, name.clone()));
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
        state.main.comparison.show_optimized = false;
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
                    if Selectable::new(label).selected(selected).build(ui)
                        && state.main.selected_build_tab != Some(*i)
                    {
                        bt_changed = Some(*i);
                    }
                }
            }
        }

        if let Some(idx) = bt_changed {
            state.main.selected_build_tab = Some(idx);
            state.main.comparison.suggestions.clear();
            state.main.comparison.selected_suggestion = 0;
            state.main.comparison.error = None;
            state.main.comparison.show_optimized = false;
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
                    if Selectable::new(label).selected(selected).build(ui)
                        && state.main.selected_equipment_tab != Some(*i)
                    {
                        et_changed = Some(*i);
                    }
                }
            }
        }

        if let Some(idx) = et_changed {
            state.main.selected_equipment_tab = Some(idx);
            state.main.comparison.suggestions.clear();
            state.main.comparison.selected_suggestion = 0;
            state.main.comparison.error = None;
            state.main.comparison.show_optimized = false;
            resolution::resolve_selected_build(state);
        }
    }

    // Build resolution indicator
    if state.main.build_loading {
        ui.text_colored(theme::WARN, "Resolving build...");
    }
}

/// Build controls: mode, scale, shared roles, optional weight radar, actions.
fn render_left_build_controls(ui: &Ui, state: &mut AddonState) {
    render_left_section_header(ui, "MODE", state.config.section_spacing);
    let mode_idx = GameMode::ALL
        .iter()
        .position(|m| *m == state.main.game_mode)
        .unwrap_or(0);
    if let Some(i) = theme::segment_row(ui, &["PvE", "PvP", "WvW"], mode_idx, "##mode") {
        let mode = GameMode::ALL[i].clone();
        state.main.game_mode = mode.clone();
        state.main.weights = if let Some(role) = state.main.selected_role {
            role.to_weights(&mode)
        } else {
            OptimizationWeights::default_for_mode(mode.label())
        };
        state.main.comparison.suggestions.clear();
        state.main.comparison.selected_suggestion = 0;
        state.main.comparison.error = None;
        state.main.build_locks = gw2_core::types::BuildLocks::default();
        resolution::resolve_selected_build(state);
    }

    if state.main.game_mode == GameMode::WvW {
        render_wvw_sub_role(ui, state);
    }

    render_role_chips(ui, state);

    if ui.collapsing_header("Fine-tune weights", TreeNodeFlags::empty()) {
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

        let show_current = matches!(state.main.active_tab, MainTab::Improve | MainTab::Talk);
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
    }

    ui.spacing();
    let role_bit = state
        .main
        .selected_role
        .map(|r| r.play_label())
        .unwrap_or("pick a role");
    let focus = if state.main.game_mode == GameMode::WvW {
        format!(
            "{} · {} · {}",
            state.main.game_mode.label(),
            state.main.wvw_combat_tier.label(),
            role_bit
        )
    } else {
        format!("{} · {}", state.main.game_mode.label(), role_bit)
    };
    theme::wrapped(ui, theme::CURRENT, &focus);

    render_left_section_header(ui, "ACTIONS", state.config.section_spacing);

    let is_improve = state.main.active_tab == MainTab::Improve;
    let btn_label_owned;
    let btn_label: &str = if is_improve {
        "Improve Build"
    } else if let Some(role) = state.main.selected_role {
        btn_label_owned = format!("Optimize: {}", role.play_label());
        &btn_label_owned
    } else {
        "Optimize Build"
    };
    let disabled = state.main.optimizing
        || state.main.chat.waiting
        || state.main.game_db.is_none()
        || (is_improve && state.main.current_build.is_none());

    if disabled {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, btn_label, [-1.0, 28.0]);
        style.pop();
        if ui.is_item_hovered() {
            ui.tooltip_text(if state.main.optimizing {
                "Optimization in progress..."
            } else if state.main.chat.waiting {
                "Thinking..."
            } else if state.main.game_db.is_none() {
                "Waiting for game data to load..."
            } else {
                "Select a character first"
            });
        }
    } else if theme::gold_button_sized(ui, btn_label, [-1.0, 28.0]) {
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

    ui.dummy([0.0, 2.0]);
    if state.main.characters_loading {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, "Refreshing...", [-1.0, 0.0]);
        style.pop();
    } else if ui.small_button("Refresh Data") {
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
        MainTab::Talk => {
            tabs::kitchen::render_talk_tab(ui, state);
        }
        MainTab::SaveLoad => {
            tabs::saveload::render_saveload_tab(ui, state);
        }
        MainTab::Settings => {
            tabs::settings::render_settings_tab(ui, state);
        }
    }
}
