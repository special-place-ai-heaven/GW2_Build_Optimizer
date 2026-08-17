//! Settings tab — AI provider, API keys + model picker, theme, cache, benchmarks.

use nexus::imgui::{ComboBox, Selectable, Ui};

use crate::state::AddonState;
use crate::ui::theme;

use super::super::{build_display, stats};

pub(in crate::ui::main_view) fn render_settings_tab(ui: &Ui, state: &mut AddonState) {
    let avail_w = ui.content_region_avail()[0];
    let col_w = (avail_w - 12.0) / 2.0;

    ui.columns(2, "##settings_cols", false);
    ui.set_column_width(0, col_w);

    // ── LEFT COLUMN ─────────────────────────────────────────────────
    build_display::render_card_header(ui, "AI PROVIDER", [1.0, 0.88, 0.35, 1.0]);
    render_api_keys_section(ui, state, col_w);
    ui.spacing();
    render_model_picker_section(ui, state, col_w);

    ui.dummy([0.0, 8.0]);

    build_display::render_card_header(ui, "OPTIMIZATION DEFAULTS", [1.0, 0.88, 0.35, 1.0]);
    {
        ui.text("Default Game Mode:");
        let current_default = state
            .config
            .default_game_mode
            .clone()
            .unwrap_or_else(|| "PvE".into());
        for mode in &["PvE", "PvP", "WvW"] {
            let is_sel = current_default == *mode;
            if ui.radio_button_bool(mode, is_sel) && !is_sel {
                state.config.default_game_mode = Some(mode.to_string());
                let _ = state.config.save(&state.config_path);
            }
        }
    }

    ui.dummy([0.0, 8.0]);

    build_display::render_card_header(ui, "DATA QUALITY LEGEND", [0.7, 0.7, 0.7, 1.0]);
    ui.spacing();
    ui.text_colored([0.3, 0.9, 0.3, 1.0], "\u{25cf} Verified");
    ui.same_line();
    ui.text_colored([0.6, 0.6, 0.6, 1.0], "— All input data is source-backed.");
    ui.text_colored([0.95, 0.75, 0.15, 1.0], "\u{25cf} Provisional");
    ui.same_line();
    ui.text_colored([0.6, 0.6, 0.6, 1.0], "— Some data estimated. Less certain.");
    ui.text_colored([1.0, 0.3, 0.2, 1.0], "\u{25cf} Blocked");
    ui.same_line();
    ui.text_colored([0.6, 0.6, 0.6, 1.0], "— Critical data missing.");

    // ── RIGHT COLUMN ────────────────────────────────────────────────
    ui.next_column();

    build_display::render_card_header(ui, "UI PREFERENCES", [1.0, 0.88, 0.35, 1.0]);
    render_theme_section(ui, state, col_w);

    ui.dummy([0.0, 8.0]);

    build_display::render_card_header(ui, "CACHE & DATA", [1.0, 0.88, 0.35, 1.0]);
    render_cache_section(ui, state);

    ui.dummy([0.0, 8.0]);

    build_display::render_card_header(ui, "BENCHMARK DATA", [0.6, 0.8, 1.0, 1.0]);
    render_benchmark_section(ui, state);

    ui.columns(1, "##settings_end", false);

    // ── Footer ─────────────────────────────────────────────────────
    ui.dummy([0.0, 4.0]);
    ui.separator();
    ui.dummy([0.0, 2.0]);
    ui.text_colored(
        [0.4, 0.4, 0.4, 1.0],
        format!(
            "GW2 Build Optimizer v{}  —  AI: {}",
            crate::VERSION,
            state.config.active_provider.label()
        ),
    );
}

fn render_api_keys_section(ui: &Ui, state: &mut AddonState, col_w: f32) {
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
        state.main.settings_model_search.clear();
        if let Err(e) = state.config.save(&state.config_path) {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                format!("Config save failed: {}", e),
            );
        }
    }
    ui.spacing();

    let provider_label = state.config.active_provider.label().to_string();
    let has_key = state.config.has_active_llm_key();
    if has_key {
        ui.text_colored(
            [0.0, 1.0, 0.0, 1.0],
            format!("{} Key: configured", provider_label),
        );
    } else {
        ui.text_colored(
            [1.0, 0.5, 0.0, 1.0],
            format!("{} Key: not set", provider_label),
        );
    }

    if has_key {
        ui.same_line();
        let validating = state.main.settings_key_validating;
        if validating {
            let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
            theme::gold_button_sized(ui, "Testing...", [100.0, 0.0]);
            style.pop();
        } else if theme::gold_button_sized(ui, "Test", [60.0, 0.0]) {
            state.main.settings_key_validating = true;
            state.main.settings_key_status = Some("Testing...".into());
            state.main.settings_key_valid = false;
            state.main.settings_key_warning = None;
            let addon_dir = state.addon_dir.clone();
            let config_snapshot = state.config.clone();
            let token = state.cancel_token.clone();
            std::thread::spawn(move || {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // Always clear settings_key_validating on every exit path.
                    let result = if token.is_cancelled() {
                        None
                    } else {
                        let r = gw2_optimizer::llm::create_client(&config_snapshot, &addon_dir)
                            .map(|c| c.validate_key_detailed());
                        if token.is_cancelled() {
                            None
                        } else {
                            Some(r)
                        }
                    };
                    crate::state::with_state(|s| {
                        s.main.settings_key_validating = false;
                        match result {
                            Some(Ok(v)) => {
                                s.main.settings_key_valid = v.valid;
                                s.main.settings_key_status = Some(v.message);
                                s.main.settings_key_warning = v.warning;
                            }
                            Some(Err(e)) => {
                                s.main.settings_key_valid = false;
                                s.main.settings_key_status = Some(format!("Failed: {}", e));
                                s.main.settings_key_warning = None;
                            }
                            None => { /* cancelled — flag cleared */ }
                        }
                    });
                }));
            });
        }
    }

    ui.set_next_item_width(col_w - 80.0);
    ui.input_text(
        &format!("##{}_key", provider_label),
        &mut state.main.settings_key_input,
    )
    .hint("Enter new API key...")
    .build();
    ui.same_line();
    let validating = state.main.settings_key_validating;
    if validating {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, "...", [50.0, 0.0]);
        style.pop();
    } else if theme::gold_button_sized(ui, "Save", [50.0, 0.0]) {
        let key = state.main.settings_key_input.trim().to_string();
        if !key.is_empty() {
            match state.config.active_provider {
                gw2_core::config::LlmProvider::Gemini => {
                    state.config.gemini_api_key = Some(key.clone())
                }
                gw2_core::config::LlmProvider::OpenAI => {
                    state.config.openai_api_key = Some(key.clone())
                }
                gw2_core::config::LlmProvider::Anthropic => {
                    state.config.anthropic_api_key = Some(key.clone())
                }
                gw2_core::config::LlmProvider::OpenRouter => {
                    state.config.openrouter_api_key = Some(key.clone())
                }
            }
            let _ = state.config.save(&state.config_path);
            state.main.settings_key_input.clear();
            state.main.settings_key_status = Some("Saved. Validating...".into());
            state.main.settings_key_valid = false;
            state.main.settings_key_validating = true;
            let addon_dir = state.addon_dir.clone();
            let config_snapshot = state.config.clone();
            let token = state.cancel_token.clone();
            std::thread::spawn(move || {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // Always clear settings_key_validating on every exit path.
                    let result = if token.is_cancelled() {
                        None
                    } else {
                        let r = gw2_optimizer::llm::create_client(&config_snapshot, &addon_dir)
                            .map(|c| c.validate_key_detailed());
                        if token.is_cancelled() {
                            None
                        } else {
                            Some(r)
                        }
                    };
                    crate::state::with_state(|s| {
                        s.main.settings_key_validating = false;
                        match result {
                            Some(Ok(v)) => {
                                s.main.settings_key_valid = v.valid;
                                s.main.settings_key_status = Some(v.message);
                                s.main.settings_key_warning = v.warning;
                            }
                            Some(Err(e)) => {
                                s.main.settings_key_valid = false;
                                s.main.settings_key_status =
                                    Some(format!("Saved but validation failed: {}", e));
                                s.main.settings_key_warning = None;
                            }
                            None => { /* cancelled — flag cleared */ }
                        }
                    });
                }));
            });
        }
    }

    if let Some(ref status) = state.main.settings_key_status {
        let col = if state.main.settings_key_valid {
            [0.0, 1.0, 0.0, 1.0]
        } else if status.contains("saved")
            || status.contains("Testing")
            || status.contains("Validating")
        {
            [0.7, 0.7, 0.7, 1.0]
        } else {
            [1.0, 0.3, 0.3, 1.0]
        };
        ui.text_colored(col, status);
    }
    if let Some(ref w) = state.main.settings_key_warning {
        ui.text_colored([1.0, 0.7, 0.0, 1.0], format!("  Warning: {}", w));
    }
}

fn render_model_picker_section(ui: &Ui, state: &mut AddonState, col_w: f32) {
    let current_model = match state.config.active_provider {
        gw2_core::config::LlmProvider::Gemini => state.config.gemini_model_id().to_string(),
        gw2_core::config::LlmProvider::OpenAI => state.config.openai_model_id().to_string(),
        gw2_core::config::LlmProvider::Anthropic => state.config.anthropic_model_id().to_string(),
        gw2_core::config::LlmProvider::OpenRouter => state.config.openrouter_model_id().to_string(),
    };
    let config_field = match state.config.active_provider {
        gw2_core::config::LlmProvider::Gemini => "gemini",
        gw2_core::config::LlmProvider::OpenAI => "openai",
        gw2_core::config::LlmProvider::Anthropic => "anthropic",
        gw2_core::config::LlmProvider::OpenRouter => "openrouter",
    };
    let has_key = state.config.has_active_llm_key();
    if state.main.available_models.is_empty() && !state.main.models_loading && has_key {
        stats::start_fetch_models(state);
    }
    let display_models: Vec<(String, String)> = if !state.main.available_models.is_empty() {
        state.main.available_models.clone()
    } else {
        let hardcoded: &[(&str, &str)] = match state.config.active_provider {
            gw2_core::config::LlmProvider::Gemini => gw2_core::config::GEMINI_MODELS,
            gw2_core::config::LlmProvider::OpenAI => gw2_core::config::OPENAI_MODELS,
            gw2_core::config::LlmProvider::Anthropic => gw2_core::config::ANTHROPIC_MODELS,
            gw2_core::config::LlmProvider::OpenRouter => gw2_core::config::OPENROUTER_MODELS,
        };
        hardcoded
            .iter()
            .map(|(id, label)| (id.to_string(), label.to_string()))
            .collect()
    };
    let preview = display_models
        .iter()
        .find(|(id, _)| *id == current_model)
        .map(|(_, l)| l.as_str())
        .unwrap_or(&current_model);
    ui.text("Model:");
    ui.same_line();
    ui.set_next_item_width(col_w - 140.0);
    if let Some(_c) = ComboBox::new(&format!("##{}_model", config_field))
        .preview_value(preview)
        .begin(ui)
    {
        // Search box pinned at the top of the dropdown — filters the model
        // list by case-insensitive substring match against both id and
        // display label. OpenRouter's catalog can have hundreds of entries,
        // so without a filter the user has to scroll/eyeball through them.
        ui.set_next_item_width(-1.0);
        ui.input_text(
            &format!("##{}_model_search", config_field),
            &mut state.main.settings_model_search,
        )
        .hint("Search models...")
        .build();
        let needle = state.main.settings_model_search.trim().to_lowercase();
        let mut visible = 0usize;
        for (id, label) in &display_models {
            if !needle.is_empty()
                && !id.to_lowercase().contains(&needle)
                && !label.to_lowercase().contains(&needle)
            {
                continue;
            }
            visible += 1;
            let sel = *id == current_model;
            if Selectable::new(label).selected(sel).build(ui) {
                match state.config.active_provider {
                    gw2_core::config::LlmProvider::Gemini => {
                        state.config.gemini_model = Some(id.clone())
                    }
                    gw2_core::config::LlmProvider::OpenAI => {
                        state.config.openai_model = Some(id.clone())
                    }
                    gw2_core::config::LlmProvider::Anthropic => {
                        state.config.anthropic_model = Some(id.clone())
                    }
                    gw2_core::config::LlmProvider::OpenRouter => {
                        state.config.openrouter_model = Some(id.clone())
                    }
                }
                let _ = state.config.save(&state.config_path);
            }
        }
        if visible == 0 && !needle.is_empty() {
            ui.text_colored(
                [0.7, 0.7, 0.7, 1.0],
                format!(
                    "No models match \"{}\"",
                    state.main.settings_model_search.trim()
                ),
            );
        }
    }
    ui.same_line();
    if state.main.models_loading {
        ui.text_colored([0.7, 0.7, 0.7, 1.0], "...");
    } else if ui.small_button("Refresh##models") {
        state.main.available_models.clear();
        state.main.models_error = None;
        stats::start_fetch_models(state);
    }
    if let Some(ref err) = state.main.models_error {
        ui.text_colored([1.0, 0.5, 0.0, 1.0], format!("  {}", err));
    }

    ui.spacing();
    let usage_filename = match state.config.active_provider {
        gw2_core::config::LlmProvider::Gemini => "gemini_usage.json",
        gw2_core::config::LlmProvider::OpenAI => "openai_usage.json",
        gw2_core::config::LlmProvider::Anthropic => "anthropic_usage.json",
        gw2_core::config::LlmProvider::OpenRouter => "openrouter_usage.json",
    };
    let usage_path = state.addon_dir.join(usage_filename);
    // Refresh the usage display at most ~once per second (~60 frames at 60fps).
    // Previously this read from disk on every render frame just to display a
    // counter that changes at most a few times per minute.
    if state.main.settings_usage_frames == 0 {
        state.main.settings_usage_today = std::fs::read_to_string(&usage_path)
            .ok()
            .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
            .and_then(|v| v.get("requests_today").and_then(|x| x.as_u64()))
            .unwrap_or(0);
        state.main.settings_usage_frames = 60;
    } else {
        state.main.settings_usage_frames -= 1;
    }
    ui.text_colored(
        [0.5, 0.5, 0.5, 1.0],
        format!("Usage today: {} requests", state.main.settings_usage_today),
    );
}

fn render_theme_section(ui: &Ui, state: &mut AddonState, col_w: f32) {
    let right_item_w = col_w - 12.0;

    ui.text("Window Opacity:");
    ui.set_next_item_width(right_item_w * 0.6);
    let mut opacity = state.config.window_opacity;
    if nexus::imgui::Slider::new("##opacity", 0.3, 1.0)
        .display_format("%.2f")
        .build(ui, &mut opacity)
    {
        state.config.window_opacity = opacity;
        let _ = state.config.save(&state.config_path);
    }

    ui.text("Global Scale:");
    ui.set_next_item_width(right_item_w * 0.6);
    let mut scale = state.config.font_scale;
    if nexus::imgui::Slider::new("##font_scale", 0.5, 2.0)
        .display_format("%.2f")
        .build(ui, &mut scale)
    {
        state.config.font_scale = scale;
        let _ = state.config.save(&state.config_path);
    }

    ui.spacing();
    ui.text_colored([0.7, 0.7, 0.75, 1.0], "Layout Tuning:");

    let fields: &mut [(&str, &str, f32, f32, f32)] = &mut [
        ("Left Panel Width:", "##left_panel_w", 320.0, 480.0, 5.0),
        ("Panel Padding:", "##panel_pad", 0.0, 20.0, 1.0),
        ("Section Spacing:", "##section_sp", 0.0, 16.0, 1.0),
        ("Content Indent:", "##content_ind", 0.0, 20.0, 1.0),
    ];
    let vals: &mut [f32] = &mut [
        state.config.left_panel_width,
        state.config.panel_padding,
        state.config.section_spacing,
        state.config.content_indent,
    ];
    let mut dirty = false;

    for (i, (label, id, min, max, step)) in fields.iter().enumerate() {
        ui.text(label);
        ui.same_line_with_pos(right_item_w * 0.45);
        ui.set_next_item_width(right_item_w * 0.35);
        if nexus::imgui::InputFloat::new(ui, id, &mut vals[i])
            .step(*step)
            .step_fast(*step * 5.0)
            .build()
        {
            vals[i] = vals[i].clamp(*min, *max);
            dirty = true;
        }
    }
    if dirty {
        state.config.left_panel_width = vals[0];
        state.config.panel_padding = vals[1];
        state.config.section_spacing = vals[2];
        state.config.content_indent = vals[3];
        let _ = state.config.save(&state.config_path);
    }

    ui.spacing();
    if ui.small_button("Reset Layout Defaults") {
        state.config.left_panel_width = 360.0;
        state.config.panel_padding = 6.0;
        state.config.section_spacing = 4.0;
        state.config.content_indent = 4.0;
        state.config.window_x = None;
        state.config.window_y = None;
        state.config.window_w = None;
        state.config.window_h = None;
        state.force_window_pos = true;
        let _ = state.config.save(&state.config_path);
    }
}

fn render_cache_section(ui: &Ui, state: &mut AddonState) {
    if let Some(ref key) = state.config.gw2_api_key {
        let display = if key.chars().count() > 12 {
            let pre: String = key.chars().take(8).collect();
            let suf: String = key
                .chars()
                .rev()
                .take(4)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            format!("{}...{}", pre, suf)
        } else {
            "****".into()
        };
        ui.text(format!("GW2 API Key: {}", display));
    }
    if let Some(build) = state.config.cache_build_number {
        if let Some(live) = state.main.live_build_number {
            if live != build {
                ui.text_colored(
                    theme::WARN,
                    format!("Game build: {build} (live {live} — refresh data)"),
                );
            } else {
                ui.text(format!("Game build: {build}"));
            }
        } else {
            ui.text(format!("Game build: {build}"));
        }
    }

    let cache_dir = state.addon_dir.join("cache");
    // Throttle the directory scan to ~once per second. The cache holds ~10–20
    // files including a ~50 MB items.json; scanning + metadata-statting every
    // render frame just to display "Cache: X MB" hits disk at ~60 Hz.
    if state.main.settings_cache_size_frames == 0 {
        state.main.settings_cache_size = calculate_dir_size(&cache_dir);
        state.main.settings_graphics_size = calculate_dir_size(&cache_dir.join("graphics"));
        state.main.settings_cache_size_frames = 60;
    } else {
        state.main.settings_cache_size_frames -= 1;
    }
    ui.text(format!(
        "Data: {}",
        format_bytes(state.main.settings_cache_size)
    ));
    ui.same_line();
    ui.text_colored(
        theme::MUTED,
        format!("Icons: {}", format_bytes(state.main.settings_graphics_size)),
    );
    ui.same_line();
    let refreshing = state.main.game_db_loading;
    if refreshing {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, "Clear Cache", [100.0, 0.0]);
        style.pop();
    } else if theme::gold_button_sized(ui, "Clear Cache", [100.0, 0.0]) {
        let cache = gw2_api::cache::DataCache::new(&cache_dir);
        if let Err(e) = cache.clear_all() {
            state.main.error = Some(format!("Failed to clear cache: {}", e));
        } else {
            state.config.cache_build_number = None;
            let _ = state.config.save(&state.config_path);
            state.main.game_db = None;
            state.setup.download_progress = None;
            // Force the cached "Cache: …" label to recompute on the next frame
            // instead of waiting for the throttle to roll over.
            state.main.settings_cache_size_frames = 0;
            stats::start_game_data_refresh(state);
        }
    }

    ui.spacing();
    let mut auto_refresh = state.config.auto_refresh_cache;
    if ui.checkbox("Auto-refresh on startup", &mut auto_refresh) {
        state.config.auto_refresh_cache = auto_refresh;
        let _ = state.config.save(&state.config_path);
    }

    ui.spacing();
    if refreshing {
        let stage = state.main.game_refresh_stage.clone();
        ui.text_colored(
            [1.0, 1.0, 0.0, 1.0],
            if stage.is_empty() {
                "Downloading game data..."
            } else {
                &stage
            },
        );
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, "Refreshing...", [160.0, 0.0]);
        style.pop();
    } else if theme::gold_button_sized(ui, "Refresh Game Data", [160.0, 0.0]) {
        let cache = gw2_api::cache::DataCache::new(&cache_dir);
        if let Err(e) = cache.clear_all() {
            state.main.error = Some(format!("Failed to refresh: {}", e));
        } else {
            state.config.cache_build_number = None;
            let _ = state.config.save(&state.config_path);
            state.main.game_db = None;
            state.setup.download_progress = None;
            // Force the cached "Cache: …" label to recompute on the next frame.
            state.main.settings_cache_size_frames = 0;
            stats::start_game_data_refresh(state);
        }
    }

    ui.spacing();
    if !state.main.confirm_reset {
        if theme::gold_button_sized(ui, "Reset Setup", [160.0, 0.0]) {
            state.main.confirm_reset = true;
        }
    } else {
        ui.text_colored([1.0, 0.3, 0.0, 1.0], "Reset all settings?");
        if theme::gold_button_sized(ui, "Yes, Reset", [100.0, 0.0]) {
            state.main.confirm_reset = false;
            if let Err(e) = state.reset_to_first_run() {
                state.main.error = Some(format!("Reset failed: {}", e));
            }
        }
        ui.same_line();
        if theme::gold_button_sized(ui, "Cancel", [80.0, 0.0]) {
            state.main.confirm_reset = false;
        }
    }
}

fn render_benchmark_section(ui: &Ui, state: &mut AddonState) {
    ui.spacing();
    ui.text_colored(
        [0.7, 0.7, 0.7, 1.0],
        "Snowcrows (PvE) · Hardstuck · GuildJen (WvW/PvP)",
    );
    ui.spacing();
    if let Some(ref last) = state.main.benchmark_last_synced {
        let sc = state
            .main
            .benchmark_counts
            .get("snowcrows")
            .copied()
            .unwrap_or(0);
        let hs = state
            .main
            .benchmark_counts
            .get("hardstuck")
            .copied()
            .unwrap_or(0);
        let gj = state
            .main
            .benchmark_counts
            .get("guildjen")
            .copied()
            .unwrap_or(0);
        ui.text_colored(
            [0.5, 0.9, 0.5, 1.0],
            format!("Synced: {}  SC:{} HS:{} GJ:{}", last, sc, hs, gj),
        );
    } else {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "Never synced.");
    }
    if let Some(ref err) = state.main.benchmark_error.clone() {
        let short = if err.chars().count() > 80 {
            format!("{}…", err.chars().take(80).collect::<String>())
        } else {
            err.clone()
        };
        ui.text_colored([1.0, 0.4, 0.2, 1.0], format!("[!] {}", short));
        if ui.is_item_hovered() {
            ui.tooltip_text(err);
        }
    }
    ui.spacing();
    let sync_disabled = state.main.benchmark_running || state.main.game_db.is_none();
    if sync_disabled {
        let _dim = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(
            ui,
            if state.main.benchmark_running {
                "Syncing..."
            } else {
                "Sync Benchmarks"
            },
            [160.0, 0.0],
        );
    } else if theme::gold_button_sized(ui, "Sync Benchmarks", [160.0, 0.0]) {
        let addon_dir = state.addon_dir.clone();
        let token = state.cancel_token.clone();
        state.main.benchmark_running = true;
        state.main.benchmark_error = None;
        std::thread::spawn(move || {
            // Cancel-aware end-to-end. Previously, an entry-point or post-scrape cancel
            // returned early, leaving `benchmark_running = true` and the button locked
            // on "Syncing…". Now every exit path resets the flag.
            let cancel_check = token.clone();
            let results = if token.is_cancelled() {
                None
            } else {
                let r =
                    gw2_optimizer::scraper::scrape_all(&addon_dir, &|| cancel_check.is_cancelled());
                if token.is_cancelled() {
                    None
                } else {
                    Some(r)
                }
            };
            crate::state::with_state(|s| {
                s.main.benchmark_running = false;
                let Some(results) = results else {
                    return;
                };
                let mut counts = std::collections::HashMap::new();
                let mut errors = Vec::new();
                for r in &results {
                    counts.insert(r.source.clone(), r.builds.len());
                    if let Some(ref e) = r.error {
                        errors.push(format!("{}: {}", r.source, e));
                    }
                }
                s.main.benchmark_counts = counts;
                s.main.benchmark_error = if errors.is_empty() {
                    None
                } else {
                    Some(errors.join(" | "))
                };
                let total: usize = results.iter().map(|r| r.builds.len()).sum();
                if total > 0 {
                    // Use chrono for the YYYY-MM-DD label. The previous manual calendar
                    // arithmetic (1970 + days/365, days%365/30 + 1, …) ignored leap years
                    // and assumed 30-day months, drifting ~14 days off the real date.
                    s.main.benchmark_last_synced =
                        Some(chrono::Utc::now().format("%Y-%m-%d").to_string());
                }
            });
        });
    }
}

fn calculate_dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
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
