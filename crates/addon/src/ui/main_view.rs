use nexus::imgui::{ChildWindow, ComboBox, Selectable, Ui};
use base64::Engine as _;

use crate::state::{AddonState, MainTab};
use gw2_optimizer::scoring::{AggressionLevel, Archetype};

mod build_display;

pub fn render_main(ui: &Ui, state: &mut AddonState) {
    // Trigger character load on first render
    if state.main.characters.is_empty() && !state.main.characters_loading {
        load_characters(state);
    }

    // Load GameDb once on first entry (S11-T06)
    if state.main.game_db.is_none() && !state.main.game_db_loading {
        load_game_db(state);
    }

    // Loading banner (GameDb)
    if state.main.game_db_loading {
        ui.text_colored([1.0, 1.0, 0.0, 1.0], "Loading game data...");
    }

    // Error bar at top (dismissible)
    if let Some(ref err) = state.main.error.clone() {
        ui.text_colored([1.0, 0.3, 0.0, 1.0], &format!("[!] {}", err));
        ui.same_line();
        if ui.small_button("Dismiss##err") {
            state.main.error = None;
        }
        ui.separator();
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

    // Left menu panel (fixed width)
    let menu_width = 180.0;

    ChildWindow::new("##left_menu")
        .size([menu_width, 0.0])
        .build(ui, || {
            render_left_menu(ui, state);
        });

    ui.same_line();

    // Main content area (fills remaining width)
    ChildWindow::new("##main_content")
        .size([0.0, 0.0])
        .build(ui, || {
            render_main_content(ui, state);
        });
}

fn render_left_menu(ui: &Ui, state: &mut AddonState) {
    // Character dropdown
    ui.text("Character:");
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
        load_character_tabs(state, name);
    }

    // Build Template dropdown (shown when tabs are loaded)
    if !state.main.build_tabs.is_empty() {
        ui.spacing();
        ui.text("Build Template:");
        ui.set_next_item_width(-1.0);
        let bt_preview = state.main.selected_build_tab
            .and_then(|i| state.main.build_tabs.get(i))
            .map(|t| {
                let name = t.build.name.as_deref().unwrap_or("Unnamed");
                format!("Tab {}: {}", t.tab, name)
            })
            .unwrap_or_else(|| "Select...".into());

        let bt_count = state.main.build_tabs.len();
        let bt_labels: Vec<(usize, String)> = state.main.build_tabs.iter().enumerate()
            .map(|(i, t)| {
                let name = t.build.name.as_deref().unwrap_or("Unnamed");
                (i, format!("Tab {}: {}", t.tab, name))
            }).collect();

        let mut bt_changed: Option<usize> = None;
        if bt_count > 0 {
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
            update_build_chat_code(state);
            resolve_selected_build(state);
        }
    }

    // Equipment Template dropdown
    if !state.main.equipment_tabs.is_empty() {
        ui.spacing();
        ui.text("Equipment Template:");
        ui.set_next_item_width(-1.0);
        let et_preview = state.main.selected_equipment_tab
            .and_then(|i| state.main.equipment_tabs.get(i))
            .map(|t| {
                let name = t.name.as_deref().unwrap_or("Unnamed");
                format!("Tab {}: {}", t.tab, name)
            })
            .unwrap_or_else(|| "Select...".into());

        let et_count = state.main.equipment_tabs.len();
        let et_labels: Vec<(usize, String)> = state.main.equipment_tabs.iter().enumerate()
            .map(|(i, t)| {
                let name = t.name.as_deref().unwrap_or("Unnamed");
                (i, format!("Tab {}: {}", t.tab, name))
            }).collect();

        let mut et_changed: Option<usize> = None;
        if et_count > 0 {
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
            resolve_selected_build(state);
        }
    }

    // Build resolution indicator
    if state.main.build_loading {
        ui.spacing();
        ui.text_colored([1.0, 1.0, 0.0, 1.0], "Resolving build...");
    }

    // Build chat code display with Copy button
    if let Some(ref code) = state.main.build_chat_code.clone() {
        ui.spacing();
        ui.text("Chat Code:");
        ui.same_line();
        if state.main.copy_feedback_frames > 0 {
            ui.text_colored([0.0, 1.0, 0.0, 1.0], "Copied!");
            state.main.copy_feedback_frames -= 1;
        } else if ui.small_button("Copy##chatcode") {
            ui.set_clipboard_text(code);
            state.main.copy_feedback_frames = 120; // ~2 seconds
        }
        // Show code below (read-only input for easy select-all)
        ui.set_next_item_width(-1.0);
        let mut code_buf = code.clone();
        ui.input_text("##chat_code_display", &mut code_buf)
            .read_only(true)
            .build();
    }

    ui.spacing();
    ui.separator();
    ui.spacing();

    // Game mode selector
    ui.text("Game Mode:");
    for mode in &gw2_core::types::GameMode::ALL {
        let selected = state.main.game_mode == *mode;
        if ui.radio_button_bool(mode.label(), selected) {
            state.main.game_mode = mode.clone();
            state.main.aggression_index = AggressionLevel::default_for_mode(mode.label()).to_index() as i32;
            resolve_selected_build(state);
        }
    }

    // Aggression level slider (shared across tabs)
    ui.spacing();
    {
        let level = AggressionLevel::from_index(state.main.aggression_index as usize);
        ui.text(format!("Playstyle: {}", level.label()));
    }
    ui.set_next_item_width(-1.0);
    nexus::imgui::Slider::new("##aggression", 0, 4)
        .display_format("")
        .build(ui, &mut state.main.aggression_index);
    ui.text_colored([0.6, 0.6, 0.6, 1.0], "Defense <-> Offense");

    ui.spacing();
    ui.separator();
    ui.spacing();

    // Menu tabs
    let tabs = [
        (MainTab::NewBuild, "New Build"),
        (MainTab::Improve, "Improve Character"),
        (MainTab::SaveLoad, "Save / Load"),
        (MainTab::Settings, "Settings"),
    ];

    for (tab, label) in &tabs {
        let selected = state.main.active_tab == *tab;
        if Selectable::new(label).selected(selected).build(ui) {
            state.main.active_tab = tab.clone();
        }
    }

    // Refresh button at bottom
    ui.spacing();
    ui.separator();
    ui.spacing();
    if state.main.characters_loading {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size("Refreshing...", [-1.0, 0.0]);
        style.pop();
    } else if ui.button_with_size("Refresh Data", [-1.0, 0.0]) {
        load_characters(state);
    }

    // Error is shown in the top-level error bar (render_main), not duplicated here
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
        ui.text("Select a character from the left menu to create a new build.");
        return;
    }

    ui.text("New Build");
    ui.separator();

    // Show optimization progress
    if state.main.optimizing {
        ui.text_colored([1.0, 1.0, 0.0, 1.0], &format!("Optimizing: {}", state.main.optimize_stage));
        ui.spacing();
    }

    // Archetype selector
    ui.text("Select build archetype:");
    ui.spacing();
    let optimizing = state.main.optimizing;
    let game_db_ready = state.main.game_db.is_some();
    for archetype in &Archetype::ALL {
        let disabled = optimizing || !game_db_ready;
        if disabled {
            let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
            ui.button_with_size(archetype.label(), [200.0, 0.0]);
            style.pop();
            if ui.is_item_hovered() {
                ui.tooltip_text(if optimizing { "Optimization in progress..." } else { "Waiting for game data to load..." });
            }
        } else if ui.button_with_size(archetype.label(), [200.0, 0.0]) {
            start_optimization(state, archetype.clone(), None);
        }
        ui.spacing();
    }

    // Show comparison if suggestions exist
    if !state.main.comparison.suggestions.is_empty() {
        ui.separator();
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

        // Save Build button + Clear Results
        if !state.main.comparison.suggestions.is_empty() {
            render_save_build_ui(ui, state);
            ui.spacing();
            if ui.small_button("Clear Results") {
                state.main.comparison.suggestions.clear();
                state.main.comparison.error = None;
            }
        }
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

    // Collect data for optimization before borrowing build
    let profession_name = state.main.current_build.as_ref().map(|b| b.profession.clone());
    let stats_snapshot = state.main.current_stats.clone();
    let archetype = state.main.current_build.as_ref()
        .map(|b| infer_archetype_from_build(b, stats_snapshot.as_ref()))
        .unwrap_or(Archetype::PowerDPS);

    // Check button press in separate scope to avoid borrow conflict
    let improve_disabled = state.main.optimizing || state.main.game_db.is_none();
    let should_optimize = if improve_disabled {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size("Improve This Build", [200.0, 30.0]);
        style.pop();
        if ui.is_item_hovered() {
            ui.tooltip_text(if state.main.optimizing { "Optimization in progress..." } else { "Waiting for game data to load..." });
        }
        false
    } else {
        state.main.current_build.is_some() && ui.button_with_size("Improve This Build", [200.0, 30.0])
    };

    if let Some(ref build) = state.main.current_build {
        let stats = state.main.current_stats.clone();

        // Show current build
        build_display::render_build(ui, build, stats.as_ref());

        ui.spacing();
        ui.separator();

        // Show optimization progress (S11-T05)
        if state.main.optimizing {
            ui.text_colored([1.0, 1.0, 0.0, 1.0], &format!("Optimizing: {}", state.main.optimize_stage));
            ui.spacing();
        }
    }

    // Handle optimization after the build borrow ends
    if should_optimize {
        if let Some(ref prof_name) = profession_name {
            start_optimization_with_profession(state, archetype, prof_name);
        }
    }

    // Show comparison if suggestions exist (separate from build display)
    if !state.main.comparison.suggestions.is_empty() {
        if let Some(ref build) = state.main.current_build {
            let stats = state.main.current_stats.clone();
            ui.spacing();
            if let Some(new_idx) = crate::ui::comparison::render_comparison(
                ui,
                build,
                stats.as_ref(),
                &state.main.comparison,
            ) {
                state.main.comparison.selected_suggestion = new_idx;
            }
        }

        render_save_build_ui(ui, state);
        ui.spacing();
        if ui.small_button("Clear Results##improve") {
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
    }

    // Chat bar at bottom
    if state.main.current_build.is_some() {
        ui.spacing();
        if let Some(msg) = crate::ui::chat_bar::render_chat_bar(ui, &mut state.main.chat) {
            state.main.chat.waiting = true;
            send_chat_message(state, msg);
        }
    } else if state.main.selected_character.is_some() {
        ui.text("Loading character build...");
    } else {
        ui.text("Select a character from the left menu.");
    }
}

fn render_settings_tab(ui: &Ui, state: &mut AddonState) {
    ui.text("Settings");
    ui.separator();
    ui.spacing();

    // API Keys
    if let Some(ref key) = state.config.gw2_api_key {
        let display = if key.len() > 12 {
            format!("{}...{}", &key[..8], &key[key.len() - 4..])
        } else {
            "****".into()
        };
        ui.text(&format!("GW2 API Key: {}", display));
    }
    if state.config.gemini_api_key.is_some() {
        ui.text("Gemini API Key: configured");
    }
    if let Some(build) = state.config.cache_build_number {
        ui.text(&format!("Cached game build: {}", build));
    }

    ui.spacing();
    ui.separator();
    ui.spacing();

    // Cache size display
    let cache_dir = state.addon_dir.join("cache");
    let cache_size = calculate_dir_size(&cache_dir);
    ui.text(&format!("Cache size: {}", format_bytes(cache_size)));
    ui.same_line();
    if ui.button_with_size("Clear Cache", [100.0, 0.0]) {
        let _ = std::fs::remove_dir_all(&cache_dir);
        state.config.cache_build_number = None;
        if let Err(e) = state.config.save(&state.config_path) {
            nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
        }
        state.main.game_db = None;
    }

    ui.spacing();

    // Gemini quota display
    let usage_path = state.addon_dir.join("gemini_usage.json");
    if let Ok(json) = std::fs::read_to_string(&usage_path) {
        if let Ok(usage) = serde_json::from_str::<serde_json::Value>(&json) {
            let today = usage.get("requests_today").and_then(|v| v.as_u64()).unwrap_or(0);
            ui.text(&format!("Gemini usage today: {} / 250 requests", today));
        }
    } else {
        ui.text("Gemini usage today: 0 / 250 requests");
    }

    ui.spacing();
    ui.separator();
    ui.spacing();

    // Cache management — actually re-download game data
    let refreshing = state.main.game_db_loading;
    if refreshing {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size("Refreshing...", [200.0, 0.0]);
        style.pop();
    } else if ui.button_with_size("Refresh Game Data", [200.0, 0.0]) {
        // Clear cache and re-download
        let cache_dir = state.addon_dir.join("cache");
        let _ = std::fs::remove_dir_all(&cache_dir);
        state.config.cache_build_number = None;
        if let Err(e) = state.config.save(&state.config_path) {
            nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
        }
        state.main.game_db = None;
        // Trigger re-download via setup flow
        state.setup.download_progress = None;
        start_game_data_refresh(state);
    }

    ui.spacing();

    // Reset Setup with confirmation
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

    ui.spacing();
    ui.separator();
    ui.spacing();

    // About
    ui.text("GW2 Build Optimizer v1.0.0");
    ui.text("Powered by Google Gemini AI");
    ui.text_wrapped("Optimizes builds using GW2 API data and LLM reasoning about trait/sigil/rune/relic synergies.");
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

    state.main.characters_loading = true;
    state.main.error = None;
    let key = key.clone();
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let result = gw2_api::client::Gw2Client::with_key(&key)
            .and_then(|c| c.fetch_characters());

        if token.is_cancelled() { return; }

        crate::state::with_state(|s| {
            s.main.characters_loading = false;
            match result {
                Ok(chars) => s.main.characters = chars,
                Err(e) => s.main.error = Some(e.to_string()),
            }
        });
    });
}

/// Phase 1: Fetch build tabs + equipment tabs from API, store in state.
fn load_character_tabs(state: &mut AddonState, character_name: String) {
    let Some(ref key) = state.config.gw2_api_key else {
        return;
    };

    state.main.build_loading = true;
    state.main.error = None;
    let key = key.clone();
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
                Ok((build_tabs, equipment_tabs)) => {
                    // Auto-select active tabs
                    let bt_idx = build_tabs.iter().position(|t| t.is_active).unwrap_or(0);
                    let et_idx = equipment_tabs.iter().position(|t| t.is_active).unwrap_or(0);
                    s.main.build_tabs = build_tabs;
                    s.main.equipment_tabs = equipment_tabs;
                    s.main.selected_build_tab = if s.main.build_tabs.is_empty() { None } else { Some(bt_idx) };
                    s.main.selected_equipment_tab = if s.main.equipment_tabs.is_empty() { None } else { Some(et_idx) };
                    // Generate chat code for the selected build tab
                    update_build_chat_code_inner(s);
                    // Trigger Phase 2: resolve the selected build
                    resolve_selected_build_inner(s);
                }
                Err(e) => {
                    s.main.build_loading = false;
                    s.main.error = Some(e);
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

    let Some(ref key) = state.config.gw2_api_key else { return; };
    let _ = key; // key not needed for resolve (uses cache)

    state.main.build_loading = true;
    state.main.error = None;
    let cache_dir = state.addon_dir.join("cache");
    let game_mode = state.main.game_mode.clone();
    let char_name = state.main.selected_character
        .and_then(|i| state.main.characters.get(i).cloned())
        .unwrap_or_default();
    let expected_char = char_name.clone();
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let cache = gw2_api::cache::DataCache::new(&cache_dir);
        let result = resolve_build(&char_name, &bt.build, &et, &cache, &game_mode);

        if token.is_cancelled() { return; }

        // Also calculate stats from the equipment + traits
        let stats_result = calculate_current_stats(&bt.build, &et, &cache, &game_mode);

        if token.is_cancelled() { return; }

        crate::state::with_state(|s| {
            // Stale-result guard
            let current_char = s.main.selected_character
                .and_then(|i| s.main.characters.get(i).cloned());
            if current_char.as_deref() != Some(&expected_char) {
                s.main.build_loading = false;
                return;
            }

            s.main.build_loading = false;
            match result {
                Ok(build) => {
                    s.main.current_build = Some(build);
                    match stats_result {
                        Ok((stats, combat_solo, combat_party, combat_squad)) => {
                            s.main.current_stats = Some(stats);
                            s.main.comparison.current_combat_solo = combat_solo;
                            s.main.comparison.current_combat_party = combat_party;
                            s.main.comparison.current_combat_squad = combat_squad;
                        }
                        Err(_) => {
                            s.main.current_stats = None;
                        }
                    }
                }
                Err(e) => s.main.error = Some(e),
            }
        });
    });
}

fn resolve_build(
    character_name: &str,
    build: &gw2_api::models::Build,
    equipment: &gw2_api::models::EquipmentTab,
    cache: &gw2_api::cache::DataCache,
    game_mode: &gw2_core::types::GameMode,
) -> Result<gw2_core::types::ResolvedBuild, String> {
    use gw2_core::types::*;

    let specs: Vec<gw2_api::models::Specialization> = cache
        .load("specializations").map_err(|e| e.to_string())?.unwrap_or_default();
    let traits: Vec<gw2_api::models::Trait> = cache
        .load("traits").map_err(|e| e.to_string())?.unwrap_or_default();
    let skills_cache: Vec<gw2_api::models::Skill> = cache
        .load("skills").map_err(|e| e.to_string())?.unwrap_or_default();
    let items: Vec<gw2_api::models::Item> = cache
        .load("items").map_err(|e| e.to_string())?.unwrap_or_default();
    let itemstats: Vec<gw2_api::models::ItemStat> = cache
        .load("itemstats").ok().flatten().unwrap_or_default();

    let resolved_specs = resolve_specs(build, &specs, &traits);
    let resolved_skills = resolve_skills(build, &skills_cache);
    let (weapons, armor, trinkets_vec, rune, relic_resolved) =
        resolve_equipment(equipment, &items, &itemstats);
    let pvp_amulet = resolve_pvp_amulet(game_mode, equipment, cache);

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

/// Calculate the current build's stats using the full stat pipeline.
type CombatBundle = (
    gw2_core::types::StatBlock,
    Option<gw2_core::types::CombatMetrics>,
    Option<gw2_core::types::CombatMetrics>,
    Option<gw2_core::types::CombatMetrics>,
);

fn calculate_current_stats(
    build: &gw2_api::models::Build,
    equipment: &gw2_api::models::EquipmentTab,
    cache: &gw2_api::cache::DataCache,
    game_mode: &gw2_core::types::GameMode,
) -> Result<CombatBundle, String> {
    use std::collections::HashMap;

    let items_vec: Vec<gw2_api::models::Item> = cache
        .load("items").map_err(|e| e.to_string())?.unwrap_or_default();
    let itemstats_vec: Vec<gw2_api::models::ItemStat> = cache
        .load("itemstats").map_err(|e| e.to_string())?.unwrap_or_default();
    let traits_vec: Vec<gw2_api::models::Trait> = cache
        .load("traits").map_err(|e| e.to_string())?.unwrap_or_default();
    let pvp_amulets_vec: Vec<gw2_api::models::PvpAmulet> = cache
        .load("pvp_amulets").ok().flatten().unwrap_or_default();

    let items_cache: HashMap<u32, gw2_api::models::Item> =
        items_vec.into_iter().map(|i| (i.id, i)).collect();
    let itemstats_cache: HashMap<u32, gw2_api::models::ItemStat> =
        itemstats_vec.into_iter().map(|i| (i.id, i)).collect();
    let traits_cache: HashMap<u32, gw2_api::models::Trait> =
        traits_vec.into_iter().map(|t| (t.id, t)).collect();

    let profession = build.profession.clone().unwrap_or_default();

    // PvP mode: stats come from amulet
    if *game_mode == gw2_core::types::GameMode::PvP {
        if let Some(ref pvp) = equipment.equipment_pvp {
            if let Some(amulet_id) = pvp.amulet {
                if let Some(amulet) = pvp_amulets_vec.iter().find(|a| a.id == amulet_id) {
                    let opt_stats = gw2_optimizer::stats::calculate_pvp_stats(&amulet.attributes);
                    let derived = gw2_optimizer::stats::compute_derived(&opt_stats, &profession);
                    let stats = gw2_core::types::StatBlock {
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
                    };
                    // PvP: no trait/item modifiers, compute with default modifiers
                    let modifiers = gw2_optimizer::combat::DamageModifiers::default();
                    let (solo, party, squad) = compute_3tier_combat(
                        &opt_stats, &derived, &modifiers, &profession,
                    );
                    return Ok((stats, solo, party, squad));
                }
            }
        }
    }

    // PvE/WvW: collect equipped trait IDs (major + minor)
    let specs_vec: Vec<gw2_api::models::Specialization> = cache
        .load("specializations").ok().flatten().unwrap_or_default();

    let mut equipped_trait_ids = Vec::new();
    for spec_sel in &build.specializations {
        for &trait_id in &spec_sel.traits {
            if let Some(tid) = trait_id {
                equipped_trait_ids.push(tid);
            }
        }
        // Also include minor traits from the spec
        if let Some(spec_id) = spec_sel.id {
            if let Some(spec) = specs_vec.iter().find(|s| s.id == spec_id) {
                equipped_trait_ids.extend(&spec.minor_traits);
            }
        }
    }

    // Find rune ID (first upgrade component that's a rune)
    let rune_id = equipment.equipment.iter()
        .flat_map(|p| p.upgrades.iter())
        .find_map(|&uid| {
            items_cache.get(&uid).and_then(|item| {
                item.details.as_ref().and_then(|d| {
                    if d.detail_type.as_deref() == Some("Rune") { Some(uid) } else { None }
                })
            })
        });

    // Find sigil IDs
    let sigil_ids: Vec<u32> = equipment.equipment.iter()
        .flat_map(|p| p.upgrades.iter())
        .filter_map(|&uid| {
            items_cache.get(&uid).and_then(|item| {
                item.details.as_ref().and_then(|d| {
                    if d.detail_type.as_deref() == Some("Sigil") { Some(uid) } else { None }
                })
            })
        })
        .collect();

    let (opt_stats, derived) = gw2_optimizer::stats::calculate_full_stats(
        equipment,
        &equipped_trait_ids,
        rune_id,
        &sigil_ids,
        &profession,
        &items_cache,
        &itemstats_cache,
        &traits_cache,
    );

    // Extract damage modifiers from equipped traits + items for combat metrics
    let relic_id = equipment.equipment.iter()
        .find(|p| p.slot == "Relic")
        .map(|p| p.id);
    let modifiers = gw2_optimizer::combat::extract_damage_modifiers(
        &equipped_trait_ids,
        rune_id,
        &sigil_ids,
        relic_id,
        &traits_cache,
        &items_cache,
    );

    // Compute 3-tier combat metrics for current build
    let (combat_solo, combat_party, combat_squad) = compute_3tier_combat(
        &opt_stats, &derived, &modifiers, &profession,
    );

    Ok((gw2_core::types::StatBlock {
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
    }, combat_solo, combat_party, combat_squad))
}

fn resolve_specs(
    build: &gw2_api::models::Build,
    specs: &[gw2_api::models::Specialization],
    traits: &[gw2_api::models::Trait],
) -> Vec<gw2_core::types::ResolvedSpec> {
    use gw2_core::types::*;
    build.specializations.iter().filter_map(|sel| {
        let spec_id = sel.id?;
        let spec = specs.iter().find(|s| s.id == spec_id)?;
        let traits_selected: Vec<ResolvedTrait> = sel.traits.iter().enumerate()
            .filter_map(|(col, trait_id)| {
                let tid = (*trait_id)?;
                let t = traits.iter().find(|t| t.id == tid)?;
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

fn resolve_skills(
    build: &gw2_api::models::Build,
    skills_cache: &[gw2_api::models::Skill],
) -> gw2_core::types::ResolvedSkills {
    use gw2_core::types::*;
    let find_skill = |id: u32| -> Option<SkillInfo> {
        skills_cache.iter().find(|s| s.id == id).map(|s| SkillInfo {
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

fn resolve_equipment(
    equipment: &gw2_api::models::EquipmentTab,
    items: &[gw2_api::models::Item],
    itemstats: &[gw2_api::models::ItemStat],
) -> (
    Vec<gw2_core::types::ResolvedWeaponSet>,
    Vec<gw2_core::types::ResolvedGearPiece>,
    Vec<gw2_core::types::ResolvedGearPiece>,
    Option<gw2_core::types::ResolvedUpgrade>,
    Option<gw2_core::types::ResolvedRelic>,
) {
    use gw2_core::types::*;

    let find_item = |id: u32| -> Option<&gw2_api::models::Item> {
        items.iter().find(|i| i.id == id)
    };

    let mut armor = Vec::new();
    let mut trinkets_vec = Vec::new();
    let mut rune = None;
    let mut relic_resolved = None;
    let mut ws1 = ResolvedWeaponSet { label: "Set 1".into(), ..Default::default() };
    let mut ws2 = ResolvedWeaponSet { label: "Set 2".into(), ..Default::default() };

    for piece in &equipment.equipment {
        let item = find_item(piece.id);
        let item_name = item.map(|i| i.name.clone()).unwrap_or_else(|| format!("#{}", piece.id));
        let stat_prefix = piece.stats.as_ref()
            .and_then(|s| itemstats.iter().find(|is| is.id == s.id).map(|is| is.name.clone()))
            .unwrap_or_default();

        let extract_sigils = |piece: &gw2_api::models::EquipmentPiece, ws: &mut ResolvedWeaponSet| {
            for &uid in &piece.upgrades {
                if let Some(u) = find_item(uid) {
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
                        if let Some(u) = find_item(uid) {
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

fn resolve_pvp_amulet(
    game_mode: &gw2_core::types::GameMode,
    equipment: &gw2_api::models::EquipmentTab,
    cache: &gw2_api::cache::DataCache,
) -> Option<gw2_core::types::ResolvedPvpAmulet> {
    use gw2_core::types::*;
    if *game_mode != GameMode::PvP { return None; }
    let pvp_eq = equipment.equipment_pvp.as_ref()?;
    let amulet_id = pvp_eq.amulet?;
    let pvp_amulets: Vec<gw2_api::models::PvpAmulet> = cache
        .load("pvp_amulets").ok().flatten().unwrap_or_default();
    pvp_amulets.iter().find(|a| a.id == amulet_id).map(|a| {
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
                s.main.optimize_stage = detail;
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
                    match db_result {
                        Ok(db) => {
                            nexus::log::log(nexus::log::LogLevel::Info, "GW2 Build Optimizer", "Game data refreshed successfully");
                            s.main.game_db = Some(db);
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
                    s.main.error = Some(format!("Refresh failed: {}", e));
                });
            }
        }
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
                }
                Err(e) => {
                    s.main.error = Some(format!("Failed to load game data: {}", e));
                }
            }
        });
    });
}

/// Start optimization in background thread (S11-T01, S11-T02, S11-T03)
fn start_optimization(state: &mut AddonState, archetype: Archetype, _current_build: Option<&gw2_core::types::ResolvedBuild>) {
    // Guard against concurrent optimization
    if state.main.optimizing {
        return;
    }

    // Get profession from current build
    let profession_name = state.main.current_build.as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();

    start_optimization_with_profession(state, archetype, &profession_name);
}

/// Start optimization with explicit profession name (avoids borrow conflicts)
fn start_optimization_with_profession(state: &mut AddonState, archetype: Archetype, profession_name: &str) {
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
    let gemini_key = state.config.gemini_api_key.clone();
    let game_mode = state.main.game_mode.clone();
    let game_mode_label = game_mode.label().to_string();
    let current_build_summary = state.main.current_build.as_ref()
        .map(|b| summarize_resolved_build(b));
    let addon_dir = state.addon_dir.clone();
    let token = state.cancel_token.clone();
    let aggression = AggressionLevel::from_index(state.main.aggression_index as usize);

    state.main.optimizing = true;
    state.main.optimize_stage = "Starting...".into();
    state.main.comparison.suggestions.clear();
    state.main.comparison.loading = true;
    state.main.comparison.error = None;

    std::thread::spawn(move || {
        let result = (|| -> Result<Vec<crate::ui::comparison::BuildSuggestion>, String> {
            if token.is_cancelled() { return Err("Cancelled".into()); }

            let db = db.ok_or("GameDb not loaded")?;
            let profession = db.profession(&profession_name)
                .ok_or_else(|| format!("Profession {} not found", profession_name))?;

            let token_progress = token.clone();
            let candidates = gw2_optimizer::engine::optimize(
                profession,
                &archetype,
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
                Some(&aggression),
            );

            if token.is_cancelled() { return Err("Cancelled".into()); }

            let mut suggestions: Vec<crate::ui::comparison::BuildSuggestion> =
                candidates.iter().map(|c| candidate_to_suggestion(c, &db)).collect();

            // Enrich top suggestion with Gemini LLM reasoning
            if let Some(ref key) = gemini_key {
                if token.is_cancelled() { return Err("Cancelled".into()); }

                crate::state::with_state(|s| {
                    s.main.optimize_stage = "Consulting Gemini for synergy analysis...".into();
                });

                match enrich_with_gemini(
                    key,
                    &profession_name,
                    &archetype,
                    &game_mode_label,
                    aggression,
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
                            &format!("Gemini enrichment skipped: {}", e),
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
        changes_made: Vec::new(),
        estimated_stats,
        combat_solo,
        combat_party,
        combat_squad,
        rotation: None,
    }
}

/// Run rotation simulation for a suggestion's skills and attach the results.
fn simulate_suggestion_rotation(
    suggestion: &mut crate::ui::comparison::BuildSuggestion,
    db: &gw2_optimizer::gamedb::GameDb,
) {
    if suggestion.skills.is_empty() {
        return;
    }

    // Resolve skill names to IDs
    let skill_ids: Vec<u32> = suggestion.skills.iter()
        .filter_map(|name| {
            db.skills.values().find(|s| s.name.eq_ignore_ascii_case(name)).map(|s| s.id)
        })
        .collect();

    if skill_ids.is_empty() {
        return;
    }

    let rotation_skills = gw2_optimizer::rotation::builder::build_rotation_skills(&skill_ids, db);
    if rotation_skills.is_empty() {
        return;
    }

    // Extract stats from estimated_stats for the simulation
    let stats = suggestion.estimated_stats.as_ref();
    let power = stats.map(|s| s.power as f64).unwrap_or(1000.0);
    let condition_damage = stats.map(|s| s.condition_damage as f64).unwrap_or(0.0);
    let weapon_strength = 1100.0; // reference weapon strength (same as combat.rs)

    let result = gw2_optimizer::rotation::simulator::simulate(
        &rotation_skills, 0, power, condition_damage, weapon_strength,
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

/// Infer archetype from current build stats.
/// Compares stat investment above base (Power/Precision start at 1000)
/// so gear-driven stats are properly weighted against base-zero stats.
fn infer_archetype_from_build(_build: &gw2_core::types::ResolvedBuild, stats: Option<&gw2_core::types::StatBlock>) -> Archetype {
    let Some(stats) = stats else {
        return Archetype::PowerDPS;
    };

    // Investment above base values (Power & Precision base = 1000, rest = 0)
    let power_inv = (stats.power - 1000).max(0);
    let prec_inv = (stats.precision - 1000).max(0);

    // Score each archetype based on stat investment signals
    let scores: [(i32, Archetype); 7] = [
        // PowerDPS: power + precision + ferocity
        (power_inv + prec_inv + stats.ferocity, Archetype::PowerDPS),
        // ConditionDPS: condition damage + expertise
        (stats.condition_damage * 2 + stats.expertise, Archetype::ConditionDPS),
        // SustainHybrid: balanced power + defense
        (power_inv + prec_inv / 2 + stats.toughness / 2 + stats.vitality / 2, Archetype::SustainHybrid),
        // Tank: toughness + vitality (scaled down since it's a sum of two)
        (stats.toughness + stats.vitality, Archetype::Tank),
        // BoonSupport: concentration-heavy
        (stats.concentration * 3, Archetype::BoonSupport),
        // HealSupport: healing-heavy
        (stats.healing_power * 3, Archetype::HealSupport),
        // CelestialHybrid: moderate everything (detect low variance across stats)
        ({
            let vals = [power_inv, prec_inv, stats.ferocity, stats.condition_damage,
                        stats.expertise, stats.concentration, stats.healing_power,
                        stats.toughness, stats.vitality];
            let min = vals.iter().copied().min().unwrap_or(0);
            let max = vals.iter().copied().max().unwrap_or(0);
            // Low spread between min/max = Celestial-like; bonus if all stats are moderate
            let spread_bonus = if max > 0 && max - min < max / 2 { min * 2 } else { 0 };
            spread_bonus
        }, Archetype::CelestialHybrid),
    ];

    scores.iter()
        .max_by_key(|(v, _)| v)
        .map(|(_, a)| a.clone())
        .unwrap_or(Archetype::PowerDPS)
}

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

/// Call Gemini to enrich the top optimizer suggestion with LLM reasoning.
/// Uses function calling (tool use) so Gemini can query game data and simulate builds.
fn enrich_with_gemini(
    key: &str,
    profession_name: &str,
    archetype: &Archetype,
    game_mode: &str,
    aggression: AggressionLevel,
    candidates: &[gw2_optimizer::engine::BuildCandidate],
    db: &gw2_optimizer::gamedb::GameDb,
    current_build_summary: Option<&str>,
    suggestions: &mut [crate::ui::comparison::BuildSuggestion],
    addon_dir: &std::path::Path,
) -> Result<(), String> {
    let usage_path = addon_dir.join("gemini_usage.json");
    let client = gw2_optimizer::gemini::GeminiClient::with_persistence(key, usage_path)
        .map_err(|e| e.to_string())?;

    // Build tool-aware prompt
    let prompt = if current_build_summary.is_some() {
        gw2_optimizer::prompts::improve_build_prompt_with_tools(
            profession_name, archetype, game_mode, &aggression,
        )
    } else {
        gw2_optimizer::prompts::new_build_prompt_with_tools(
            profession_name, archetype, game_mode, &aggression,
        )
    };

    let tools = gw2_optimizer::gemini_tools::tool_declarations();
    let build_summary_owned = current_build_summary.map(|s| s.to_string());
    let ctx = gw2_optimizer::gemini_tools::ToolContext {
        db,
        profession_name,
        candidates,
        current_build_summary: build_summary_owned.as_deref(),
        aggression_level: aggression,
    };

    let response = client.generate_with_tools(
        &prompt,
        tools,
        |name, args| gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx),
        8,
    ).map_err(|e| e.to_string())?;

    let gemini_build = gw2_optimizer::prompts::parse_gemini_build(&response)
        .map_err(|e| format!("Parse failed: {}", e))?;

    if let Some(first) = suggestions.first_mut() {
        apply_gemini_response(first, &gemini_build);
        // Run rotation simulation now that Gemini has populated skills
        simulate_suggestion_rotation(first, db);
    }

    Ok(())
}

/// Send a chat message to Gemini for build refinement.
/// Uses function calling so Gemini can query game data to answer questions.
fn send_chat_message(state: &mut AddonState, message: String) {
    // Guard against concurrent chat messages
    if state.main.chat.waiting {
        return;
    }

    let gemini_key = match state.config.gemini_api_key.clone() {
        Some(key) => key,
        None => {
            crate::ui::chat_bar::add_ai_response(
                &mut state.main.chat,
                "No Gemini API key configured.".into(),
            );
            return;
        }
    };

    let profession = state.main.current_build.as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();
    let build_summary = state.main.current_build.as_ref()
        .map(|b| summarize_resolved_build(b));
    let addon_dir = state.addon_dir.clone();
    let token = state.cancel_token.clone();
    let db_clone = state.main.game_db.clone();
    let aggression = AggressionLevel::from_index(state.main.aggression_index as usize);

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let result = (|| -> Result<gw2_optimizer::prompts::GeminiBuildResponse, String> {
            let usage_path = addon_dir.join("gemini_usage.json");
            let client = gw2_optimizer::gemini::GeminiClient::with_persistence(&gemini_key, usage_path)
                .map_err(|e| e.to_string())?;

            if token.is_cancelled() { return Err("Cancelled".into()); }

            // Use tool-enabled generation if GameDb is available
            if let Some(ref db) = db_clone {
                let prompt = gw2_optimizer::prompts::chat_refinement_prompt_with_tools(
                    &profession, &message,
                );
                let tools = gw2_optimizer::gemini_tools::tool_declarations();
                let empty_candidates = vec![];
                let ctx = gw2_optimizer::gemini_tools::ToolContext {
                    db,
                    profession_name: &profession,
                    candidates: &empty_candidates,
                    current_build_summary: build_summary.as_deref(),
                    aggression_level: aggression,
                };

                let response = client.generate_with_tools(
                    &prompt,
                    tools,
                    |name, args| gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx),
                    8,
                ).map_err(|e| e.to_string())?;

                gw2_optimizer::prompts::parse_gemini_build(&response)
                    .map_err(|e| format!("Parse failed: {}", e))
            } else {
                // Fallback: no GameDb, use simple prompt
                let build_summary_str = build_summary.as_deref().unwrap_or("");
                let context = gw2_optimizer::prompts::build_game_context(
                    &profession, &Archetype::PowerDPS, "PvE",
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
    ui.text("Saved Builds");
    ui.separator();

    // Lazy-load saved builds on first view
    if !state.main.saved_builds_loaded {
        let storage = gw2_core::storage::BuildStorage::new(&state.addon_dir);
        state.main.saved_builds = storage.list();
        state.main.saved_builds_loaded = true;
    }

    if state.main.saved_builds.is_empty() {
        ui.text_wrapped("No saved builds yet. Optimize a build and use the Save button to save it here.");
        return;
    }

    ui.text(&format!("{} saved build(s):", state.main.saved_builds.len()));
    ui.spacing();

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
        ui.text_colored([0.7, 0.7, 0.7, 1.0], &format!("  {} | {} | {} | {}", character, mode, prefix, time));

        ui.spacing();
        ui.separator();
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
