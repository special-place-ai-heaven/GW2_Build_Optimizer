//! Settings tab — AI provider, API keys + model picker, theme, cache, benchmarks.

use nexus::imgui::{ComboBox, Selectable, Ui};

use crate::state::AddonState;
use crate::ui::theme;
use gw2_core::i18n::{t, tf};

use super::super::{build_display, stats};

pub(in crate::ui::main_view) fn render_settings_tab(ui: &Ui, state: &mut AddonState) {
    let avail_w = ui.content_region_avail()[0];
    let scale = state.config.font_scale.max(0.5);
    let gutter = 48.0 * scale;
    let col_w = ((avail_w - gutter) * 0.5).max(220.0);

    ui.columns(2, "##settings_cols", false);
    ui.set_column_width(0, col_w);

    // ── LEFT COLUMN ─────────────────────────────────────────────────
    build_display::render_card_header(ui, &t("settings.ai_provider"), [1.0, 0.88, 0.35, 1.0]);
    render_api_keys_section(ui, state, col_w);
    ui.spacing();
    render_model_picker_section(ui, state, col_w);

    ui.dummy([0.0, 8.0]);

    build_display::render_card_header(ui, &t("settings.opt_defaults"), [1.0, 0.88, 0.35, 1.0]);
    {
        ui.text(t("settings.default_mode"));
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

    build_display::render_card_header(ui, &t("settings.legend"), [0.7, 0.7, 0.7, 1.0]);
    ui.spacing();
    ui.text_colored([0.3, 0.9, 0.3, 1.0], format!("* {}", t("settings.verified")));
    ui.same_line();
    ui.text_colored([0.6, 0.6, 0.6, 1.0], t("settings.verified_note"));
    ui.text_colored([0.95, 0.75, 0.15, 1.0], format!("* {}", t("settings.provisional")));
    ui.same_line();
    ui.text_colored([0.6, 0.6, 0.6, 1.0], t("settings.provisional_note"));
    ui.text_colored([1.0, 0.3, 0.2, 1.0], format!("* {}", t("settings.blocked")));
    ui.same_line();
    ui.text_colored([0.6, 0.6, 0.6, 1.0], t("settings.blocked_note"));

    // ── RIGHT COLUMN ────────────────────────────────────────────────
    ui.next_column();
    ui.indent_by(gutter);

    build_display::render_card_header(ui, &t("settings.ui_prefs"), [1.0, 0.88, 0.35, 1.0]);
    render_theme_section(ui, state, col_w);

    ui.dummy([0.0, 8.0]);

    build_display::render_card_header(ui, &t("settings.cache"), [1.0, 0.88, 0.35, 1.0]);
    render_cache_section(ui, state);

    ui.dummy([0.0, 8.0]);

    build_display::render_card_header(ui, &t("settings.benchmarks"), [0.6, 0.8, 1.0, 1.0]);
    render_benchmark_section(ui, state);

    ui.unindent_by(gutter);
    ui.columns(1, "##settings_end", false);

    // ── Footer ─────────────────────────────────────────────────────
    ui.dummy([0.0, 4.0]);
    ui.separator();
    ui.dummy([0.0, 2.0]);
    ui.text_colored(
        [0.4, 0.4, 0.4, 1.0],
        format!(
            "{} {}  —  {}",
            t("info.product"),
            tf("fmt.version", &[("ver", crate::VERSION)]),
            tf("fmt.ai", &[("provider", state.config.active_provider.label())]),
        ),
    );
}

fn render_api_keys_section(ui: &Ui, state: &mut AddonState, _col_w: f32) {
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
            tf("fmt.key_configured", &[("provider", &provider_label)]),
        );
    } else {
        ui.text_colored(
            [1.0, 0.5, 0.0, 1.0],
            tf("fmt.key_not_set", &[("provider", &provider_label)]),
        );
    }

    if has_key {
        ui.same_line();
        let validating = state.main.settings_key_validating;
        if validating {
            let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
            theme::gold_button_sized(ui, &t("btn.testing"), [100.0, 0.0]);
            style.pop();
        } else if theme::gold_button_sized(ui, &t("btn.test"), [60.0, 0.0]) {
            state.main.settings_key_validating = true;
            state.main.settings_key_status = Some(t("btn.testing"));
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
                                s.main.settings_key_status =
                                    Some(tf("fmt.failed", &[("err", &e.to_string())]));
                                s.main.settings_key_warning = None;
                            }
                            None => { /* cancelled — flag cleared */ }
                        }
                    });
                }));
            });
        }
    }

    let save_label = t("btn.save");
    let gap = 10.0;
    let btn_w = ui.calc_text_size(save_label.as_str())[0] + 24.0;
    let input_w = (ui.content_region_avail()[0] - btn_w - gap).max(80.0);
    ui.set_next_item_width(input_w);
    ui.input_text(
        &format!("##{}_key", provider_label),
        &mut state.main.settings_key_input,
    )
    .hint(&t("settings.enter_key"))
    .build();
    ui.same_line_with_spacing(0.0, gap);
    let validating = state.main.settings_key_validating;
    if validating {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, "...", [btn_w, 0.0]);
        style.pop();
    } else if theme::gold_button_sized(ui, save_label, [btn_w, 0.0]) {
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
            state.main.settings_key_status = Some(t("settings.saved_validating"));
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
                                s.main.settings_key_status = Some(tf(
                                    "fmt.saved_validation_failed",
                                    &[("err", &e.to_string())],
                                ));
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
        } else if state.main.settings_key_validating {
            [0.7, 0.7, 0.7, 1.0]
        } else {
            [1.0, 0.3, 0.3, 1.0]
        };
        ui.text_colored(col, status);
    }
    if let Some(ref w) = state.main.settings_key_warning {
        ui.text_colored(
            [1.0, 0.7, 0.0, 1.0],
            format!("  {}", tf("fmt.warning", &[("msg", w)])),
        );
    }
}

fn model_catalog(state: &AddonState) -> Vec<(String, String)> {
    if !state.main.available_models.is_empty() {
        return state.main.available_models.clone();
    }
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
}

fn render_model_combo(
    ui: &Ui,
    state: &mut AddonState,
    preview: &str,
    display_models: &[(String, String)],
    current_model: &str,
    id: &str,
) {
    if let Some(_c) = ComboBox::new(&format!("##{id}_model"))
        .preview_value(preview)
        .begin(ui)
    {
        ui.set_next_item_width(-1.0);
        ui.input_text(
            &format!("##{id}_model_search"),
            &mut state.main.settings_model_search,
        )
        .hint(&t("settings.search_models"))
        .build();
        let needle = state.main.settings_model_search.trim().to_lowercase();
        let mut visible = 0usize;
        for (mid, label) in display_models {
            if !needle.is_empty()
                && !mid.to_lowercase().contains(&needle)
                && !label.to_lowercase().contains(&needle)
            {
                continue;
            }
            visible += 1;
            let sel = *mid == current_model;
            if Selectable::new(label).selected(sel).build(ui) {
                state.config.set_active_model_id(mid.clone());
                state.main.provider_issue = None;
                let _ = state.config.save(&state.config_path);
            }
        }
        if visible == 0 && !needle.is_empty() {
            ui.text_colored(
                [0.7, 0.7, 0.7, 1.0],
                tf(
                    "fmt.no_models",
                    &[("q", state.main.settings_model_search.trim())],
                ),
            );
        }
    }
}

/// Compact provider + model row for the Choya header.
pub(in crate::ui::main_view) fn render_talk_model_row(ui: &Ui, state: &mut AddonState) {
    let has_key = state.config.has_active_llm_key();
    if state.main.available_models.is_empty() && !state.main.models_loading && has_key {
        stats::start_fetch_models(state);
    }
    let current_model = state.config.active_model_id().to_string();
    let display_models = model_catalog(state);
    let preview = display_models
        .iter()
        .find(|(id, _)| *id == current_model)
        .map(|(_, l)| l.as_str())
        .unwrap_or(&current_model)
        .to_string();

    let avail = ui.content_region_avail()[0];
    ui.set_next_item_width((avail * 0.34).clamp(96.0, 150.0));
    let provider_preview = state.config.active_provider.short_label().to_string();
    if let Some(_c) = ComboBox::new("##talk_provider")
        .preview_value(&provider_preview)
        .begin(ui)
    {
        for provider in &gw2_core::config::LlmProvider::ALL {
            let sel = state.config.active_provider == *provider;
            if Selectable::new(provider.short_label())
                .selected(sel)
                .build(ui)
                && !sel
            {
                state.config.active_provider = provider.clone();
                state.main.available_models.clear();
                state.main.models_error = None;
                state.main.settings_model_search.clear();
                state.main.provider_issue = None;
                let _ = state.config.save(&state.config_path);
            }
        }
    }
    ui.same_line_with_spacing(0.0, 6.0);
    let rest = ui.content_region_avail()[0];
    ui.set_next_item_width((rest - 28.0).max(120.0));
    render_model_combo(ui, state, &preview, &display_models, &current_model, "talk");
    if state.main.models_loading {
        ui.same_line();
        ui.text_colored(theme::MUTED, "...");
    }
    if let Some(err) = state.main.models_error.clone() {
        theme::wrapped(ui, theme::WARN, &err);
    }
    if let Some(issue) = state.main.provider_issue.clone() {
        theme::wrapped(ui, theme::ERR, &issue);
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
    ui.text(t("settings.model"));
    ui.same_line();
    ui.set_next_item_width(col_w - 140.0);
    render_model_combo(
        ui,
        state,
        preview,
        &display_models,
        &current_model,
        config_field,
    );
    ui.same_line();
    if state.main.models_loading {
        ui.text_colored([0.7, 0.7, 0.7, 1.0], "...");
    } else if ui.small_button(&format!("{}##models", t("btn.refresh"))) {
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
        format!(
            "{}",
            tf(
                "fmt.usage_today",
                &[("n", &state.main.settings_usage_today.to_string())]
            )
        ),
    );
}

fn render_theme_section(ui: &Ui, state: &mut AddonState, col_w: f32) {
    let right_item_w = col_w - 12.0;

    ui.text(t("settings.language"));
    ui.set_next_item_width(right_item_w * 0.6);
    let resolved = gw2_core::i18n::resolve(&state.config.ui_language);
    let cache = gw2_api::cache::DataCache::new(state.addon_dir.join("cache"));
    let build = state.main.live_build_number.or(state.config.cache_build_number);
    let preview_code = if state.config.ui_language.eq_ignore_ascii_case("auto") {
        resolved
    } else {
        state.config.ui_language.as_str()
    };
    let (preview_mark, _) = pack_mark(gw2_api::localize::pack_status(&cache, preview_code, build));
    let auto_preview = format!(
        "{preview_mark} {} — {}",
        t("settings.language_auto"),
        gw2_core::i18n::language_by_code(resolved)
            .map(|l| l.native_name)
            .unwrap_or("English")
    );
    let preview = if state.config.ui_language.eq_ignore_ascii_case("auto") {
        auto_preview
    } else {
        format!(
            "{preview_mark} {}",
            gw2_core::i18n::language_by_code(&state.config.ui_language)
                .map(|l| l.native_name)
                .unwrap_or(state.config.ui_language.as_str())
        )
    };
    if let Some(_c) = ComboBox::new("##ui_language")
        .preview_value(&preview)
        .begin(ui)
    {
        let auto_sel = state.config.ui_language.eq_ignore_ascii_case("auto");
        let auto_code = gw2_core::i18n::resolve("auto");
        let (auto_mark, auto_color) =
            pack_mark(gw2_api::localize::pack_status(&cache, auto_code, build));
        {
            let auto_label = format!("{auto_mark} {}", t("settings.language_auto"));
            let _color = ui.push_style_color(nexus::imgui::StyleColor::Text, auto_color);
            if Selectable::new(&auto_label)
                .selected(auto_sel)
                .build(ui)
                && !auto_sel
            {
                state.config.ui_language = "auto".into();
                gw2_core::i18n::set_language("auto");
                let _ = state.config.save(&state.config_path);
                super::super::stats::ensure_localized_names(state);
            }
        }
        for lang in gw2_core::i18n::LANGUAGES {
            let sel = state.config.ui_language == lang.code;
            let (mark, color) =
                pack_mark(gw2_api::localize::pack_status(&cache, lang.code, build));
            let label = format!("{mark} {}", lang.native_name);
            let _color = ui.push_style_color(nexus::imgui::StyleColor::Text, color);
            if Selectable::new(&label)
                .selected(sel)
                .build(ui)
                && !sel
            {
                state.config.ui_language = lang.code.into();
                gw2_core::i18n::set_language(lang.code);
                let _ = state.config.save(&state.config_path);
                super::super::stats::ensure_localized_names(state);
            }
        }
    }
    ui.text_colored(theme::MUTED, t("settings.lang_pack_legend"));
    ui.spacing();

    ui.text(t("settings.opacity"));
    ui.set_next_item_width(right_item_w * 0.6);
    let mut opacity = state.config.window_opacity;
    if nexus::imgui::Slider::new("##opacity", 0.3, 1.0)
        .display_format("%.2f")
        .build(ui, &mut opacity)
    {
        state.config.window_opacity = opacity;
        let _ = state.config.save(&state.config_path);
    }

    ui.text(t("settings.scale"));
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
    ui.text_colored([0.7, 0.7, 0.75, 1.0], t("settings.layout"));

    let left_l = t("settings.left_panel");
    let pad_l = t("settings.panel_padding");
    let sp_l = t("settings.section_spacing");
    let ind_l = t("settings.content_indent");
    let fields: [(&str, &str, f32, f32, f32); 4] = [
        (left_l.as_str(), "##left_panel_w", 320.0, 480.0, 5.0),
        (pad_l.as_str(), "##panel_pad", 0.0, 20.0, 1.0),
        (sp_l.as_str(), "##section_sp", 0.0, 16.0, 1.0),
        (ind_l.as_str(), "##content_ind", 0.0, 20.0, 1.0),
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
    if ui.small_button(&t("btn.reset_layout")) {
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


fn pack_mark(status: gw2_api::localize::PackStatus) -> (&'static str, [f32; 4]) {
    match status {
        gw2_api::localize::PackStatus::Ready => ("*", theme::OPTIMIZED),
        gw2_api::localize::PackStatus::Missing => ("!", theme::ERR),
        gw2_api::localize::PackStatus::Stale => ("~", theme::WARN),
        gw2_api::localize::PackStatus::None => ("-", theme::MUTED),
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
        ui.text(format!("{} {}", t("label.gw2_api_key"), display));
    }
    if let Some(build) = state.config.cache_build_number {
        if let Some(live) = state.main.live_build_number {
            if live != build {
                ui.text_colored(
                    theme::WARN,
                    tf(
                        "fmt.game_build_live",
                        &[
                            ("cached", &build.to_string()),
                            ("live", &live.to_string()),
                        ],
                    ),
                );
            } else {
                ui.text(tf("fmt.game_build", &[("n", &build.to_string())]));
            }
        } else {
            ui.text(tf("fmt.game_build", &[("n", &build.to_string())]));
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
    ui.text(tf(
        "fmt.data_size",
        &[("size", &format_bytes(state.main.settings_cache_size))],
    ));
    ui.same_line();
    ui.text_colored(
        theme::MUTED,
        tf(
            "fmt.icons_size",
            &[("size", &format_bytes(state.main.settings_graphics_size))],
        ),
    );
    ui.same_line();
    let refreshing = state.main.game_db_loading;
    if refreshing {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, t("btn.clear_cache"), [100.0, 0.0]);
        style.pop();
    } else if theme::gold_button_sized(ui, t("btn.clear_cache"), [100.0, 0.0]) {
        let cache = gw2_api::cache::DataCache::new(&cache_dir);
        if let Err(e) = cache.clear_all() {
            state.main.error = Some(tf("fmt.err_clear_cache", &[("err", &e.to_string())]));
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
    if ui.checkbox(
        format!("{}##auto_refresh", t("settings.auto_refresh")),
        &mut auto_refresh,
    ) {
        state.config.auto_refresh_cache = auto_refresh;
        let _ = state.config.save(&state.config_path);
    }

    ui.spacing();
    if refreshing {
        let stage = state.main.game_refresh_stage.clone();
        let downloading = t("settings.downloading");
        ui.text_colored(
            [1.0, 1.0, 0.0, 1.0],
            if stage.is_empty() {
                downloading.as_str()
            } else {
                &stage
            },
        );
        if let Some(ref dl) = state.setup.download_progress {
            let overlay = format!(
                "{}/{} — {}",
                dl.current_step, dl.total_steps, dl.step_name
            );
            theme::download_scribble(ui, dl.fraction(), &overlay);
        }
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        theme::gold_button_sized(ui, t("btn.refreshing"), [160.0, 0.0]);
        style.pop();
    } else if theme::gold_button_sized(ui, t("btn.refresh_game"), [160.0, 0.0]) {
        let cache = gw2_api::cache::DataCache::new(&cache_dir);
        if let Err(e) = cache.clear_all() {
            state.main.error = Some(tf("fmt.err_refresh", &[("err", &e.to_string())]));
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
        if theme::gold_button_sized(ui, t("btn.reset_setup"), [160.0, 0.0]) {
            state.main.confirm_reset = true;
        }
    } else {
        ui.text_colored([1.0, 0.3, 0.0, 1.0], t("settings.reset_q"));
        if theme::gold_button_sized(ui, t("btn.yes_reset"), [100.0, 0.0]) {
            state.main.confirm_reset = false;
            if let Err(e) = state.reset_to_first_run() {
                state.main.error = Some(tf("fmt.err_reset", &[("err", &e.to_string())]));
            }
        }
        ui.same_line();
        if theme::gold_button_sized(ui, t("btn.cancel"), [80.0, 0.0]) {
            state.main.confirm_reset = false;
        }
    }
}

fn render_benchmark_section(ui: &Ui, state: &mut AddonState) {
    ui.spacing();
    ui.text_colored(
        [0.7, 0.7, 0.7, 1.0],
        t("settings.sources"),
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
            tf(
                "fmt.synced",
                &[
                    ("when", last),
                    ("sc", &sc.to_string()),
                    ("hs", &hs.to_string()),
                    ("gj", &gj.to_string()),
                ],
            ),
        );
    } else {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], t("settings.never_synced"));
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
                t("btn.syncing")
            } else {
                t("btn.sync")
            },
            [160.0, 0.0],
        );
    } else if theme::gold_button_sized(ui, t("btn.sync"), [160.0, 0.0]) {
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
