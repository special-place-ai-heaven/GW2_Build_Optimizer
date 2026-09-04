//! Settings tab — AI provider, API keys + model picker, theme, cache, benchmarks.

use nexus::imgui::{
    ColorButton, ColorEditInputMode, ColorFormat, ColorPicker, ColorPickerMode, ComboBox,
    Selectable, StyleColor, StyleVar, Ui,
};

use crate::state::{AddonState, CancellationToken};
use crate::ui::theme;
use gw2_core::config::{CustomTheme, NewsKind, NewsLayout, NewsSource, ThemeConfig};
use gw2_core::i18n::{t, tf};

use super::super::{build_display, stats};

thread_local! {
    /// Per-field "reveal password" toggle for the Settings tab LLM API-key
    /// input. Transient UI-only state scoped to the render thread — it must not
    /// survive a save/reload and does not belong on `AddonState`.
    static SHOW_LLM_KEY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the Settings LLM API-key input should mask its contents (ImGui's
/// `InputTextFlags::PASSWORD`), given whether the user has toggled "reveal"
/// on. Same helper Setup uses: Settings is the same overlay a player might be
/// streaming or screenshotting while pasting a live key, so it defaults to
/// masked.
fn key_field_is_masked(revealed: bool) -> bool {
    !revealed
}

pub(in crate::ui::main_view) fn render_settings_tab(ui: &Ui, state: &mut AddonState) {
    let avail_w = ui.content_region_avail()[0];
    let scale = state.config.font_scale.max(0.5);
    let gutter = 48.0 * scale;
    let col_w = ((avail_w - gutter) * 0.5).max(220.0);

    ui.columns(2, "##settings_cols", false);
    ui.set_column_width(0, col_w);

    // ── LEFT COLUMN ─────────────────────────────────────────────────
    build_display::render_card_header(ui, &t("settings.ai_provider"), theme::pal().gold);
    render_api_keys_section(ui, state, col_w);
    ui.spacing();
    render_model_picker_section(ui, state, col_w);

    ui.dummy([0.0, 8.0]);

    build_display::render_card_header(ui, &t("settings.opt_defaults"), theme::pal().gold);
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

    build_display::render_card_header(ui, &t("settings.legend"), theme::pal().muted);
    ui.spacing();
    ui.text_colored(
        [0.3, 0.9, 0.3, 1.0],
        format!("* {}", t("settings.verified")),
    );
    theme::wrapped(ui, theme::pal().muted, &t("settings.verified_note"));
    ui.text_colored(
        [0.95, 0.75, 0.15, 1.0],
        format!("* {}", t("settings.provisional")),
    );
    theme::wrapped(ui, theme::pal().muted, &t("settings.provisional_note"));
    ui.text_colored([1.0, 0.3, 0.2, 1.0], format!("* {}", t("settings.blocked")));
    theme::wrapped(ui, theme::pal().muted, &t("settings.blocked_note"));

    ui.dummy([0.0, 8.0]);
    build_display::render_card_header(ui, &t("settings.news"), theme::pal().gold);
    render_news_sources(ui, state, col_w);

    // ── RIGHT COLUMN ────────────────────────────────────────────────
    ui.next_column();
    ui.indent_by(gutter);

    build_display::render_card_header(ui, &t("settings.ui_prefs"), theme::pal().gold);
    render_theme_section(ui, state, col_w);

    ui.unindent_by(gutter);
    ui.columns(1, "##settings_split_end", false);

    ui.dummy([0.0, 8.0]);
    ui.columns(2, "##settings_bottom", false);
    ui.set_column_width(0, col_w);
    build_display::render_card_header(ui, &t("settings.cache"), theme::pal().gold);
    render_cache_section(ui, state);
    ui.next_column();
    ui.indent_by(gutter);
    build_display::render_card_header(ui, &t("settings.benchmarks"), [0.6, 0.8, 1.0, 1.0]);
    render_benchmark_section(ui, state);
    ui.unindent_by(gutter);
    ui.columns(1, "##settings_end", false);

    // ── Footer ─────────────────────────────────────────────────────
    ui.dummy([0.0, 4.0]);
    ui.separator();
    ui.dummy([0.0, 2.0]);
    ui.text_colored(
        theme::pal().muted,
        format!(
            "{} {}  —  {}",
            t("info.product"),
            tf("fmt.version", &[("ver", crate::VERSION)]),
            tf(
                "fmt.ai",
                &[("provider", state.config.active_provider.label())]
            ),
        ),
    );
}

/// Spawn a tracked background worker whose result always folds back into
/// `AddonState` through `apply` — on success, on cooperative cancellation, and
/// on a **panic**.
///
/// `AddonState::spawn_worker` already wraps the whole worker body in a
/// containment `catch_unwind` so a panic can never reach the Nexus runtime,
/// but that guard runs *around* this entire closure: if `risky` panics,
/// nothing written after it in the same closure would run — including a
/// caller's "still running" flag reset, which is exactly how a Settings-tab
/// spinner gets stuck forever. Catching only `risky` here means `apply`
/// always runs afterward, with the panic surfaced as `Err` instead of
/// silently skipped.
///
/// Returns `false` when the OS refused to start the thread at all (`risky`
/// never ran); callers that already flipped a "loading" flag before calling
/// this must clear it themselves in that case.
fn spawn_flag_guarded<T>(
    state: &mut AddonState,
    worker_name: &'static str,
    risky: impl FnOnce(&CancellationToken) -> T + Send + 'static,
    apply: impl FnOnce(&mut AddonState, std::thread::Result<T>) + Send + 'static,
) -> bool
where
    T: Send + 'static,
{
    state.spawn_worker(worker_name, move |token| {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| risky(&token)));
        crate::state::with_state(|s| apply(s, outcome));
    })
}

/// Spawn the shared "validate the active provider's API key" worker used by
/// both the Test and Save buttons. `on_failure_key` selects the "validation
/// failed" translation key so the two flows can word the same failure
/// differently ("Test failed…" vs "Saved, but validation failed…").
fn spawn_key_validation(
    state: &mut AddonState,
    worker_name: &'static str,
    on_failure_key: &'static str,
) {
    let addon_dir = state.addon_dir.clone();
    let config_snapshot = state.config.clone();
    let spawned = spawn_flag_guarded(
        state,
        worker_name,
        move |token| {
            if token.is_cancelled() {
                None
            } else {
                let r = gw2_optimizer::llm::create_client(&config_snapshot, &addon_dir)
                    .map(|c| c.validate_key_detailed());
                if token.is_cancelled() {
                    None
                } else {
                    Some(r)
                }
            }
        },
        move |s, outcome| {
            s.main.settings_key_validating = false;
            match outcome {
                Ok(Some(Ok(v))) => {
                    s.main.settings_key_valid = v.valid;
                    s.main.settings_key_status = Some(v.message);
                    s.main.settings_key_warning = v.warning;
                }
                Ok(Some(Err(e))) => {
                    s.main.settings_key_valid = false;
                    s.main.settings_key_status =
                        Some(tf(on_failure_key, &[("err", &e.to_string())]));
                    s.main.settings_key_warning = None;
                }
                Ok(None) => { /* cancelled — flag cleared above */ }
                Err(_) => {
                    nexus::log::log(
                        nexus::log::LogLevel::Warning,
                        "GW2BuildOpt",
                        format!("bg thread panicked: {}", worker_name),
                    );
                    s.main.settings_key_valid = false;
                    // Overwrite whatever "Testing…"/"Saved, validating…" status was
                    // showing — leaving it in place would read as a red "Testing…"
                    // once `settings_key_valid`/`settings_key_validating` both flip
                    // false. Same literal `setup.rs` uses for its own panic paths.
                    s.main.settings_key_status = Some("thread panicked".into());
                    s.main.settings_key_warning = None;
                }
            }
        },
    );
    if !spawned {
        state.main.settings_key_validating = false;
    }
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

    let row_w = (col_w - 8.0).max(80.0);
    let provider_label = state.config.active_provider.label().to_string();
    let has_key = state.config.has_active_llm_key();
    let status_origin = ui.cursor_screen_pos();
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
    let after_status = ui.cursor_screen_pos();

    if has_key {
        let validating = state.main.settings_key_validating;
        let test_label = if validating {
            t("btn.testing")
        } else {
            t("btn.test")
        };
        let test_w = theme::gold_button_width(ui, test_label.as_str());
        ui.set_cursor_screen_pos([
            status_origin[0] + (row_w - test_w).max(0.0),
            status_origin[1],
        ]);
        if validating {
            let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
            theme::gold_button_sized(ui, test_label.as_str(), [test_w, 0.0]);
            style.pop();
        } else if theme::gold_button_sized(ui, test_label.as_str(), [test_w, 0.0]) {
            state.main.settings_key_validating = true;
            state.main.settings_key_status = Some(t("btn.testing"));
            state.main.settings_key_valid = false;
            state.main.settings_key_warning = None;
            spawn_key_validation(state, "settings-test-key", "fmt.failed");
        }
    }
    ui.set_cursor_screen_pos([
        status_origin[0],
        after_status[1].max(status_origin[1] + theme::control_height(ui)) + 2.0,
    ]);

    let save_label = t("btn.save");
    let gap = 8.0;
    let save_origin = ui.cursor_screen_pos();
    let btn_w = theme::gold_button_width(ui, save_label.as_str());
    // Key input — masked by default; see Setup's LLM key step for why.
    let show_llm_key = SHOW_LLM_KEY.with(|c| c.get());
    let toggle_label = if show_llm_key { "Hide" } else { "Show" };
    let toggle_w = theme::gold_button_width(ui, toggle_label) + 8.0;
    let input_w = (row_w - btn_w - toggle_w - gap).max(40.0);
    ui.set_next_item_width(input_w);
    ui.input_text(
        &format!("##{}_key", provider_label),
        &mut state.main.settings_key_input,
    )
    .hint(&t("settings.enter_key"))
    .password(key_field_is_masked(show_llm_key))
    .build();
    ui.same_line();
    if theme::gold_button_sized(ui, toggle_label, [toggle_w - 8.0, 0.0]) {
        SHOW_LLM_KEY.with(|c| c.set(!show_llm_key));
    }
    ui.set_cursor_screen_pos([save_origin[0] + row_w - btn_w, save_origin[1]]);
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
            spawn_key_validation(state, "settings-save-key", "fmt.saved_validation_failed");
        }
    }
    ui.set_cursor_screen_pos([
        save_origin[0],
        save_origin[1] + theme::control_height(ui).max(ui.frame_height()) + 4.0,
    ]);

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
    width: f32,
) {
    let origin = ui.cursor_screen_pos();
    ui.set_next_item_width(width);
    if let Some(_c) = ComboBox::new(&format!("##{id}_model"))
        .preview_value("\u{00A0}")
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
                theme::pal().muted,
                tf(
                    "fmt.no_models",
                    &[("q", state.main.settings_model_search.trim())],
                ),
            );
        }
    }
    theme::paint_centered_combo_preview(ui, preview, origin, width);
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
    let gap = 8.0;
    let load_w = if state.main.models_loading {
        ui.calc_text_size("...")[0] + gap
    } else {
        0.0
    };
    let provider_need = gw2_core::config::LlmProvider::ALL
        .iter()
        .map(|p| theme::combo_width_for(ui, p.short_label()))
        .fold(0.0_f32, f32::max);
    let leftover = (avail - load_w).max(0.0);
    let provider_w = provider_need.min(leftover);
    let rest = leftover - provider_w;

    let provider_origin = ui.cursor_screen_pos();
    ui.set_next_item_width(provider_w);
    if let Some(_c) = ComboBox::new("##talk_provider")
        .preview_value("\u{00A0}")
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
    theme::paint_centered_combo_preview(
        ui,
        state.config.active_provider.short_label(),
        provider_origin,
        provider_w,
    );

    let model_w = if rest >= gap + 80.0 {
        rest - gap
    } else {
        leftover.max(80.0)
    };
    if rest >= gap + 80.0 {
        ui.same_line_with_spacing(0.0, gap);
    }
    render_model_combo(
        ui,
        state,
        &preview,
        &display_models,
        &current_model,
        "talk",
        model_w,
    );
    if state.main.models_loading {
        ui.same_line();
        ui.text_colored(theme::pal().muted, "...");
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
    let row_w = (col_w - 8.0).max(80.0);
    let origin = ui.cursor_screen_pos();
    let model_label = t("settings.model");
    let label_w = ui.calc_text_size(model_label.as_str())[0];
    let refresh = t("btn.refresh");
    let refresh_w = theme::gold_button_width(ui, refresh.as_str());
    let gap = 8.0;
    let combo_w = (row_w - label_w - refresh_w - gap * 2.0).max(48.0);
    let row_h = ui.frame_height().max(theme::control_height(ui));

    ui.set_cursor_screen_pos([
        origin[0],
        origin[1] + ((row_h - ui.text_line_height()) * 0.5).max(0.0),
    ]);
    ui.text(model_label);
    ui.set_cursor_screen_pos([origin[0] + label_w + gap, origin[1]]);
    render_model_combo(
        ui,
        state,
        preview,
        &display_models,
        &current_model,
        config_field,
        combo_w,
    );
    ui.set_cursor_screen_pos([origin[0] + row_w - refresh_w, origin[1]]);
    if state.main.models_loading {
        ui.text_colored(theme::pal().muted, "...");
    } else if theme::gold_button_sized(ui, format!("{}##models", refresh), [refresh_w, 0.0]) {
        state.main.available_models.clear();
        state.main.models_error = None;
        stats::start_fetch_models(state);
    }
    ui.set_cursor_screen_pos([origin[0], origin[1] + row_h + 4.0]);
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
        theme::pal().muted,
        tf(
            "fmt.usage_today",
            &[("n", state.main.settings_usage_today.to_string().as_str())],
        ),
    );
}

/// News sources, sized to live inside one settings column.
///
/// This used to end the outer two-column split and lay the four kinds out with
/// `ui.columns(4, ...)` across the FULL window width — five checkboxes spread
/// over four columns of a 2000px pane, with the last three nearly empty. It
/// now sits in the left column under the legend, and the kinds are two
/// content-sized groups rather than four stretched columns.
///
/// Groups, not `ui.columns`: ImGui's legacy columns do not nest, and this is
/// rendered inside the settings split.
fn render_news_sources(ui: &Ui, state: &mut AddonState, col_w: f32) {
    let desk = t("news.layout.desk");
    let mag = t("news.layout.magazine");
    let reader = t("news.layout.reader");
    let labels = [desk.as_str(), mag.as_str(), reader.as_str()];
    let selected = match state.config.news.layout {
        NewsLayout::Desk => 0,
        NewsLayout::Magazine => 1,
        NewsLayout::Reader => 2,
    };
    let seg_w = theme::segment_row_min_width(ui, &labels) + 4.0;
    let seg_h = theme::control_height(ui) + 6.0;
    let seg_id = "##set_news_layout_row";
    nexus::imgui::ChildWindow::new(seg_id)
        .size([seg_w, seg_h])
        .border(false)
        .build(ui, || {
            if let Some(i) = theme::segment_row(ui, &labels, selected, "##set_news_layout") {
                state.config.news.layout = match i {
                    1 => NewsLayout::Magazine,
                    2 => NewsLayout::Reader,
                    _ => NewsLayout::Desk,
                };
                let _ = state.config.save(&state.config_path);
            }
        });
    // The stills toggle keeps the layout row's line only while the column can
    // hold both; in a narrow column it drops underneath instead of clipping.
    let stills = format!("{}##set_news_stills", t("news.images"));
    if seg_w + 12.0 + ui.calc_text_size(&stills)[0] + ui.frame_height() + 8.0 <= col_w {
        ui.same_line_with_spacing(0.0, 12.0);
    }
    let mut images = state.config.news.show_images;
    if ui.checkbox(&stills, &mut images) {
        state.config.news.show_images = images;
        let _ = state.config.save(&state.config_path);
    }
    if ui.is_item_hovered() {
        theme::wide_tooltip(ui, |ui| ui.text(t("settings.news_hint")));
    }

    ui.dummy([0.0, 6.0]);

    // Two content-sized groups: Articles + Patch notes on the left, Video +
    // Guides on the right. Five ticks over four full-width columns was the
    // sparsest thing on the page.
    let half = NewsKind::ALL.len() / 2;
    let kind_block = |ui: &Ui, state: &mut AddonState, kinds: &[NewsKind]| {
        ui.group(|| {
            for (n, kind) in kinds.iter().enumerate() {
                if n > 0 {
                    ui.dummy([0.0, 2.0]);
                }
                ui.text_colored(theme::pal().gold, t(kind.settings_key()));
                for &src in kind.sources() {
                    news_source_tick(ui, state, src);
                }
            }
        });
    };
    kind_block(ui, state, &NewsKind::ALL[..half]);
    ui.same_line_with_spacing(0.0, 24.0);
    kind_block(ui, state, &NewsKind::ALL[half..]);
}

fn news_source_tick(ui: &Ui, state: &mut AddonState, src: NewsSource) {
    let mut on = state.config.news.get(src);
    let label = format!("{}##news_src_{}", t(src.label_key()), src.index());
    if ui.checkbox(label, &mut on) {
        state.config.news.set(src, on);
        let _ = state.config.save(&state.config_path);
        if on {
            crate::news::kick(state, &[src]);
        }
    }
    if ui.is_item_hovered() {
        theme::wide_tooltip(ui, |ui| ui.text(t(src.hint_key())));
    }
}

fn render_theme_section(ui: &Ui, state: &mut AddonState, col_w: f32) {
    let right_item_w = col_w - 12.0;

    ui.text(t("settings.language"));
    ui.set_next_item_width(right_item_w * 0.6);
    let resolved = gw2_core::i18n::resolve(&state.config.ui_language);
    let cache = gw2_api::cache::DataCache::new(state.addon_dir.join("cache"));
    let build = state
        .main
        .live_build_number
        .or(state.config.cache_build_number);
    tick_pack_status_cache(build);
    let preview_code = if state.config.ui_language.eq_ignore_ascii_case("auto") {
        resolved
    } else {
        state.config.ui_language.as_str()
    };
    let (preview_mark, _) = pack_mark(cached_pack_status(&cache, preview_code, build));
    let font_pref = state.config.ui_font.clone();
    let ui_lang_pref = state.config.ui_language.clone();
    let auto_preview = format!(
        "{preview_mark} {} - {}",
        t("settings.language_auto"),
        gw2_core::i18n::language_by_code(resolved)
            .map(|l| crate::ui::fonts::language_label(l, &font_pref, &ui_lang_pref))
            .unwrap_or("English")
    );
    let preview = if state.config.ui_language.eq_ignore_ascii_case("auto") {
        auto_preview
    } else {
        format!(
            "{preview_mark} {}",
            gw2_core::i18n::language_by_code(&state.config.ui_language)
                .map(|l| crate::ui::fonts::language_label(l, &font_pref, &ui_lang_pref))
                .unwrap_or(state.config.ui_language.as_str())
        )
    };
    if let Some(_c) = ComboBox::new("##ui_language")
        .preview_value(&preview)
        .begin(ui)
    {
        let auto_sel = state.config.ui_language.eq_ignore_ascii_case("auto");
        let auto_code = gw2_core::i18n::resolve("auto");
        let (auto_mark, auto_color) = pack_mark(cached_pack_status(&cache, auto_code, build));
        {
            let auto_label = format!("{auto_mark} {}", t("settings.language_auto"));
            let _color = ui.push_style_color(nexus::imgui::StyleColor::Text, auto_color);
            if Selectable::new(&auto_label).selected(auto_sel).build(ui) && !auto_sel {
                state.config.ui_language = "auto".into();
                gw2_core::i18n::set_language("auto");
                let _ = state.config.save(&state.config_path);
                super::super::stats::ensure_localized_names(state);
            }
        }
        for lang in gw2_core::i18n::LANGUAGES {
            let sel = state.config.ui_language == lang.code;
            let (mark, color) = pack_mark(cached_pack_status(&cache, lang.code, build));
            let label = format!(
                "{mark} {}",
                crate::ui::fonts::language_label(lang, &font_pref, &ui_lang_pref)
            );
            let _color = ui.push_style_color(nexus::imgui::StyleColor::Text, color);
            if Selectable::new(&label).selected(sel).build(ui) && !sel {
                state.config.ui_language = lang.code.into();
                gw2_core::i18n::set_language(lang.code);
                let _ = state.config.save(&state.config_path);
                super::super::stats::ensure_localized_names(state);
            }
        }
    }
    theme::wrapped(ui, theme::pal().muted, &t("settings.lang_pack_legend"));
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

    ui.text(t("settings.font"));
    ui.set_next_item_width(right_item_w * 0.6);
    let current_font = state.config.ui_font.clone();
    let font_preview = t(crate::ui::fonts::label_key(&current_font));
    if let Some(_c) = ComboBox::new("##ui_font")
        .preview_value(&font_preview)
        .begin(ui)
    {
        for (id, key) in crate::ui::fonts::combo_options() {
            let label = t(key);
            let sel = current_font == id;
            if Selectable::new(&label).selected(sel).build(ui) && !sel {
                state.config.ui_font = id.to_string();
                let _ = state.config.save(&state.config_path);
            }
        }
    }
    theme::wrapped(ui, theme::pal().muted, &t("settings.font_hint"));

    ui.dummy([0.0, 8.0]);
    render_theme_style_section(ui, state, right_item_w);

    ui.spacing();
    ui.text_colored(theme::pal().muted, t("settings.layout"));

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
    if ui.small_button(t("btn.reset_layout")) {
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

/// The five custom-theme base colors in rail order: `(label key, description
/// key)`. The index is [`crate::state::MainState::theme_edit_slot`] and it
/// matches `CustomTheme`'s field order, so the two `theme_base*` helpers stay
/// trivially checkable against each other.
const THEME_SLOTS: [(&str, &str); 5] = [
    ("settings.theme_bg", "settings.theme_bg_desc"),
    ("settings.theme_panel", "settings.theme_panel_desc"),
    ("settings.theme_accent", "settings.theme_accent_desc"),
    ("settings.theme_text", "settings.theme_text_desc"),
    ("settings.theme_muted", "settings.theme_muted_desc"),
];

fn theme_base(c: &CustomTheme, i: usize) -> [f32; 3] {
    match i {
        0 => c.bg,
        1 => c.panel,
        2 => c.accent,
        3 => c.text,
        _ => c.muted,
    }
}

fn theme_base_mut(c: &mut CustomTheme, i: usize) -> &mut [f32; 3] {
    match i {
        0 => &mut c.bg,
        1 => &mut c.panel,
        2 => &mut c.accent,
        3 => &mut c.text,
        _ => &mut c.muted,
    }
}

/// Selection marks for the picked rail row, given the swatch's screen-space
/// top-left. Everything is drawn OUTSIDE the swatch, on the section ground,
/// so no mark has to out-contrast the color it is marking — the awkward case
/// (the player sets a base to the same color as the marker) becomes
/// mark-vs-ground, the contrast every button and separator already rides on.
///
/// The caret is `cream`, deliberately not the accent: `cream` is the theme's
/// text color and the ground is its background, so cream-on-ground cannot
/// fail without every word in the addon becoming unreadable. It is the mark
/// that survives a degenerate palette where `accent == bg` collapses both the
/// gold ring and the row's header plate. Shape as well as color, so it also
/// survives greyscale.
fn draw_slot_marker(ui: &Ui, swatch: [f32; 2], sw: f32) {
    let p = theme::pal();
    let (x, y) = (swatch[0], swatch[1]);
    let dl = ui.get_window_draw_list();
    dl.add_rect([x - 3.0, y - 3.0], [x + sw + 3.0, y + sw + 3.0], p.gold)
        .thickness(2.0)
        .rounding(4.0)
        .build();
    let cy = y + sw * 0.5;
    dl.add_triangle([x - 5.0, cy], [x - 11.0, cy - 5.0], [x - 11.0, cy + 5.0], p.cream)
        .filled(true)
        .build();
}

/// Copy the current preset's five base colors into an UNTOUCHED custom theme,
/// so opening the custom editor starts from the look the player was just
/// wearing instead of a fixed palette. Returns whether it wrote anything.
///
/// The untouched guard is the whole point: once a player has edited or named a
/// custom theme, switching preset -> custom -> preset and back must hand their
/// theme back, not overwrite it. `"custom"` and unknown ids have no bases to
/// copy, so those are no-ops too.
fn seed_custom_from_preset(theme: &mut ThemeConfig) -> bool {
    if theme.custom != CustomTheme::default() {
        return false;
    }
    let Some([bg, panel, accent, text, muted]) = theme::preset_bases(&theme.preset) else {
        return false;
    };
    let c = &mut theme.custom;
    c.bg = bg;
    c.panel = panel;
    c.accent = accent;
    c.text = text;
    c.muted = muted;
    true
}

/// Runtime theme picker: the built-in presets from `theme::preset_ids()` plus
/// one user-defined custom theme. Selecting a row applies instantly —
/// `theme::apply_theme` is a cheap palette rebuild, so it runs on every
/// change for live preview — while persistence uses the same
/// deactivate-after-edit debounce as the radio volume slider (persist once on
/// release/defocus, not per drag tick or keystroke).
fn render_theme_style_section(ui: &Ui, state: &mut AddonState, right_item_w: f32) {
    theme::header(ui, &t("settings.theme_section"));

    let is_custom = state.config.theme.preset == "custom";
    let custom_name = state.config.theme.custom.name.trim().to_string();
    let custom_row_label = if custom_name.is_empty() {
        t("settings.theme_custom")
    } else {
        custom_name
    };
    let preview = if is_custom {
        custom_row_label.clone()
    } else {
        theme::preset_ids()
            .iter()
            .copied()
            .find(|&(id, _)| state.config.theme.preset == id)
            .map(|(_, name)| name.to_string())
            .unwrap_or_else(|| state.config.theme.preset.clone())
    };
    let style = ui.clone_style();
    let fh = ui.frame_height();
    let gap = style.item_spacing[0];

    // Preset combo and theme name share one line: the name only exists in
    // custom mode, so a full row of its own bought nothing but height in the
    // one direction this panel has least of.
    let combo_w = (right_item_w * 0.42).max(fh * 5.0).min(right_item_w);
    let name_w = right_item_w - combo_w - gap;
    let name_beside = name_w >= fh * 6.0;

    ui.set_next_item_width(combo_w);
    if let Some(_c) = ComboBox::new("##theme_preset")
        .preview_value(&preview)
        .begin(ui)
    {
        for &(id, name) in theme::preset_ids() {
            let sel = !is_custom && state.config.theme.preset == id;
            if Selectable::new(name).selected(sel).build(ui) && !sel {
                state.config.theme.preset = id.to_string();
                theme::apply_theme(&state.config.theme);
                crate::ui::save_config_detached(state);
            }
        }
        // The custom row's visible text is the user's theme name (or the
        // localized "Custom" placeholder). "###" pins the ImGui id to the
        // suffix alone, so renaming the theme, or naming it after a preset,
        // never changes/collides ids ("##" would still hash the label).
        let label = format!("{}###theme_custom_row", custom_row_label);
        if Selectable::new(&label).selected(is_custom).build(ui) && !is_custom {
            seed_custom_from_preset(&mut state.config.theme);
            state.config.theme.preset = "custom".into();
            theme::apply_theme(&state.config.theme);
            crate::ui::save_config_detached(state);
        }
    }

    // Re-read: the combo above may have just changed it.
    if state.config.theme.preset != "custom" {
        return;
    }

    // Beside the combo when the column can hold both, under it when it cannot.
    if name_beside {
        ui.same_line();
        ui.set_next_item_width(name_w);
    } else {
        ui.set_next_item_width(right_item_w * 0.6);
    }
    ui.input_text("##theme_custom_name", &mut state.config.theme.custom.name)
        .hint(&t("settings.theme_name_hint"))
        .build();
    if ui.is_item_deactivated_after_edit() {
        crate::ui::save_config_detached(state);
    }

    // The swatch grid is a new block, not another row of the form above it.
    // One item_spacing does not read as a break; this does.
    ui.dummy([0.0, style.item_spacing[1]]);

    // ── custom base colors: swatch grid + one always-visible picker ──
    //
    // Click a swatch (or its label) to point the picker at that base, edit it
    // in place, then click the next one. No popup, no modal.
    //
    // The grid is 2 columns x 3 rows, filled COLUMN-major, so the two surface
    // colors and the accent stay together on the left and the two type colors
    // pair on the right. A single vertical list of five spent height — the one
    // axis a settings pane never has to spare — to say what two columns say in
    // three rows.
    //
    // ColorPicker4's total width IS the item width we set, provided nothing
    // is drawn past it (hence `label(false)` + `side_preview(false)`), and its
    // saturation/value square is `width - (frame_height + item_inner_spacing.x)`.
    // The popup this replaces asked for `frame_height * 12` — a ~238px square.
    // `frame_height * 7` lands ~128px: 29% of the pixel area, still over a
    // pixel per saturation percent. That is the "a bit too large" fix, in
    // numbers.
    const ROWS: usize = 3;
    let fs = state.config.font_scale.max(0.5);
    let lane = 12.0; // caret + ring live here, left of the swatch
    let col_gap = 12.0; // grid | picker gutter
    let trail = 6.0; // so the row plate does not end flush against the text
    let picker_w = (fh * 7.0).min(right_item_w).max(fh * 4.0);

    let labels: [String; 5] = std::array::from_fn(|i| t(THEME_SLOTS[i].0));
    // Measure the labels, never assume them: "Background" is 10 chars,
    // "Gedämpfter Text" 15, "Przygaszony tekst" 17, "Приглушённый текст" 18.
    // The grid sits beside the picker only when the widest label in the
    // CURRENT language at the CURRENT font scale actually fits there; below
    // that the section stacks. In side-by-side mode `cell_w` is at least the
    // width this test demanded, so a label cannot clip by construction.
    let widest = labels
        .iter()
        .map(|l| ui.calc_text_size(l)[0])
        .fold(0.0_f32, f32::max);
    let grid_need = lane + (fh + gap + widest + trail) * 2.0 + gap;
    let side_by_side = right_item_w >= grid_need + col_gap + picker_w;
    let grid_w = if side_by_side {
        right_item_w - col_gap - picker_w
    } else {
        right_item_w
    };
    // Cells are sized to their content, not stretched to the pane: a row plate
    // running the full width of the column was pure decoration and made the
    // five look like menu entries rather than swatches.
    let cell_w = ((grid_w - lane - gap) * 0.5).max(fh + 24.0);
    let label_w = (cell_w - fh - gap).max(24.0);

    let slot = state.main.theme_edit_slot.min(4);
    let mut pick: Option<usize> = None;
    ui.group(|| {
        // The ring reaches 4px past the swatch; ambient row spacing would let
        // it touch the neighbouring row's plate. x is unchanged.
        let _sp = ui.push_style_var(StyleVar::ItemSpacing([gap, 8.0]));
        // A frame-height row beside a frame-height swatch: centre the label.
        let _al = ui.push_style_var(StyleVar::SelectableTextAlign([0.0, 0.5]));
        let p = theme::pal();
        ui.indent_by(lane);
        for row in 0..ROWS {
            for col in 0..2 {
                // Column-major: left column 0,1,2 — right column 3,4 and one
                // empty cell at the bottom right.
                let i = col * ROWS + row;
                let Some(label) = labels.get(i) else {
                    continue;
                };
                if col > 0 {
                    ui.same_line();
                }
                let sel = i == slot;
                let c = theme_base(&state.config.theme.custom, i);
                let swatch = ui.cursor_screen_pos();

                // "###" hashes only the suffix, so widget identity survives a
                // language switch mid-edit. The visible half is still the
                // localized label, which ImGui's built-in color tooltip shows
                // as the tooltip title — free, localized, and a bonus cue
                // rather than the only one.
                if ColorButton::new(
                    format!("{label}###theme_sw_{i}"),
                    [c[0], c[1], c[2], 1.0],
                )
                .size([fh, fh])
                .alpha(false)
                // Without the border, a swatch set to the panel color
                // dissolves into the plate behind it.
                .border(true)
                // ColorButton is a drag-drop SOURCE by default. A drag off a
                // swatch eats the press so the click never lands, and a
                // dropped color mutates a base with no ActiveId transition,
                // which the commit-on-deactivate save below would miss.
                .drag_drop(false)
                .build(ui)
                {
                    pick = Some(i);
                }

                ui.same_line();
                {
                    // Selected row in cream, the other four muted: the grid
                    // reads as one live cell and four parked ones.
                    let _tc =
                        ui.push_style_color(StyleColor::Text, if sel { p.cream } else { p.muted });
                    // Explicit width: a 0-width Selectable spans the whole
                    // column, which both swallows the second grid column and
                    // shoves the picker off the right edge.
                    if Selectable::new(format!("{label}###theme_row_{i}"))
                        .selected(sel)
                        .size([label_w, fh])
                        .build(ui)
                    {
                        pick = Some(i);
                    }
                }

                // After both items, so the marks land on top of the row plate.
                if sel {
                    draw_slot_marker(ui, swatch, fh);
                }
            }
        }
        ui.unindent_by(lane);
    });

    // Apply the click BEFORE the picker is submitted: on a frame where a rail
    // click landed the picker cannot be the active item, so re-pointing it is
    // safe and it follows the new slot with zero lag.
    if let Some(i) = pick {
        state.main.theme_edit_slot = i;
    }
    let slot = state.main.theme_edit_slot.min(4);

    // Plain `same_line` (offset 0) means "previous line end + spacing", which
    // after a group is the rail's right edge. NOT `same_line_with_pos`: that
    // offset is measured from the window position plus group and column
    // offsets and excludes window padding, so inside this two-column settings
    // layout the number passed is not the x you get.
    if side_by_side {
        ui.same_line_with_spacing(0.0, col_gap);
    }
    let edited = {
        let value = theme_base_mut(&mut state.config.theme.custom, slot);
        ui.set_next_item_width(picker_w);
        ColorPicker::new("###theme_picker", value)
            .label(false)
            // Reclaims frame_height*3 of width, and its "Current"/"Original"
            // captions are hardcoded English in the widget source. The rail
            // is the live preview anyway.
            .side_preview(false)
            // ColorPicker4 only implies NoSmallPreview when the SIDE preview
            // is on, so with side_preview off this must be set explicitly or
            // the hex row keeps a redundant swatch.
            .small_preview(false)
            .alpha(false)
            .alpha_bar(false)
            // The right-click menu writes g.ColorEditOptions, which persists
            // in imgui.ini and feeds the picker/input mask back in — a player
            // who once chose "Hue wheel" in the old popup would get a wheel
            // here and a broken layout.
            .options(false)
            .mode(ColorPickerMode::HueBar) // exactly one PickerMask bit
            .input_mode(ColorEditInputMode::Rgb) // exactly one InputMask bit
            .inputs(true)
            .display_hex(true) // the one field people paste a color into
            .display_rgb(false)
            .display_hsv(false)
            .format(ColorFormat::U8)
            .build(ui)
    };
    // Valid immediately after `build`: ColorPicker4 wraps itself in
    // BeginGroup/EndGroup and EndGroup forwards both the Edited and
    // Deactivated status flags to the group item — the same release/defocus
    // debounce the old ColorEdit rows used.
    let commit = ui.is_item_deactivated_after_edit();

    if edited {
        theme::apply_theme(&state.config.theme);
    }
    if commit {
        crate::ui::save_config_detached(state);
    }

    // ── caption: what the selected base actually paints ──────────────
    // One description instead of five, at full column width where the long
    // German and Russian strings wrap best. Naming the slot in words is also
    // the one selection cue no color choice can erase.
    ui.spacing();
    theme::wrapped(ui, theme::pal().cream, &labels[slot]);
    ui.set_window_font_scale(fs * 0.85);
    {
        // Reserve the tallest of the five so clicking down the rail never
        // shifts the controls below this section. Measured at the scale it
        // renders at, which is why this sits after the push.
        let wrap_w = ui.content_region_avail()[0].max(8.0);
        let reserve = THEME_SLOTS
            .iter()
            .map(|(_, dk)| ui.calc_text_size_with_opts(t(dk), false, wrap_w)[1])
            .fold(0.0_f32, f32::max);
        let top = ui.cursor_screen_pos()[1];
        theme::wrapped(ui, theme::pal().muted, &t(THEME_SLOTS[slot].1));
        let used = ui.cursor_screen_pos()[1] - top - style.item_spacing[1];
        if reserve - used > 0.5 {
            ui.dummy([0.0, reserve - used]);
        }
    }
    theme::wrapped(ui, theme::pal().muted, &t("settings.theme_custom_hint"));
    // Restore to `fs`, NOT 1.0: `render_main` sets the window font scale to
    // config.font_scale, so resetting to 1.0 here silently un-scaled every
    // section after this one for any player not on 1.0.
    ui.set_window_font_scale(fs);
}

fn pack_mark(status: gw2_api::localize::PackStatus) -> (&'static str, [f32; 4]) {
    match status {
        gw2_api::localize::PackStatus::Ready => ("*", theme::OPTIMIZED),
        gw2_api::localize::PackStatus::Missing => ("!", theme::ERR),
        gw2_api::localize::PackStatus::Stale => ("~", theme::WARN),
        gw2_api::localize::PackStatus::None => ("-", theme::pal().muted),
    }
}

/// Frame-throttled, memoized wrapper around `gw2_api::localize::pack_status`.
///
/// `pack_status` opens and fully deserializes the cached name-pack JSON just
/// to read one `build` field — for the ~800 KB `de`/`es`/`fr`/`zh` packs that
/// is a real parse, not a stat. `render_theme_section` called it once per
/// frame for the current language, and once per language again while the
/// picker combo was open (5-6 parses in the same frame). This cache
/// recomputes at most once every `REFRESH_FRAMES` frames — same throttle
/// shape as `settings_cache_size_frames` in `render_cache_section` — and can
/// be forced early with `invalidate_pack_status_cache` right after an action
/// that actually changes the packs on disk (cache clear, game-data refresh).
///
/// Lives in a `thread_local` instead of on `MainState`: the render thread is
/// the only caller, this leaf's write set does not include `state.rs`, and
/// nothing here needs to be persisted — it is pure UI-side memoization.
struct PackStatusCache {
    frames_left: u32,
    build: Option<u32>,
    statuses: std::collections::HashMap<String, gw2_api::localize::PackStatus>,
}

impl PackStatusCache {
    /// ~1 second at 60 fps — matches `settings_cache_size_frames`'s throttle.
    const REFRESH_FRAMES: u32 = 60;

    fn new() -> Self {
        Self {
            frames_left: 0,
            build: None,
            statuses: std::collections::HashMap::new(),
        }
    }
}

thread_local! {
    static PACK_STATUS_CACHE: std::cell::RefCell<PackStatusCache> =
        std::cell::RefCell::new(PackStatusCache::new());
}

/// Call once per frame (at the top of `render_theme_section`) before reading
/// `cached_pack_status`. Expires the cache when the throttle window elapses or
/// `build` changes — e.g. the periodic API-health check picks up a new game
/// patch mid-session, which can flip a pack from "Ready" to "Stale".
fn tick_pack_status_cache(build: Option<u32>) {
    PACK_STATUS_CACHE.with(|cell| {
        let mut c = cell.borrow_mut();
        if c.frames_left == 0 || c.build != build {
            c.statuses.clear();
            c.build = build;
            c.frames_left = PackStatusCache::REFRESH_FRAMES;
        } else {
            c.frames_left -= 1;
        }
    });
}

/// Force the next frame's `tick_pack_status_cache` to recompute immediately
/// instead of waiting out the throttle window. Call right after clearing the
/// cache dir or kicking off a game-data refresh — both can change the on-disk
/// packs `pack_status` reports on.
fn invalidate_pack_status_cache() {
    PACK_STATUS_CACHE.with(|c| c.borrow_mut().frames_left = 0);
}

/// Throttled, memoized `gw2_api::localize::pack_status`. See `PackStatusCache`
/// and `tick_pack_status_cache`.
fn cached_pack_status(
    cache: &gw2_api::cache::DataCache,
    lang: &str,
    build: Option<u32>,
) -> gw2_api::localize::PackStatus {
    PACK_STATUS_CACHE.with(|cell| {
        let mut c = cell.borrow_mut();
        *c.statuses
            .entry(lang.to_string())
            .or_insert_with(|| gw2_api::localize::pack_status(cache, lang, build))
    })
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
                        &[("cached", &build.to_string()), ("live", &live.to_string())],
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
        theme::pal().muted,
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
            // The cleared cache dir also wiped the lang-pack files `pack_status`
            // reports on — force the language combo's Ready/Missing/Stale marks
            // to recompute now instead of showing stale "Ready" for up to a
            // second.
            invalidate_pack_status_cache();
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
            let overlay = format!("{}/{} — {}", dl.current_step, dl.total_steps, dl.step_name);
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
            invalidate_pack_status_cache();
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
    ui.text_colored(theme::pal().muted, t("settings.sources"));
    ui.spacing();
    if state.main.benchmark_running {
        let live = ["snowcrows", "hardstuck", "guildjen"]
            .iter()
            .filter_map(|k| state.main.benchmark_live.get(*k).map(|s| s.as_str()))
            .collect::<Vec<_>>()
            .join("  ·  ");
        ui.text_colored(
            theme::pal().muted,
            if live.is_empty() {
                t("btn.syncing")
            } else {
                live
            },
        );
    } else if let Some(ref last) = state.main.benchmark_last_synced {
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
        ui.text_colored(theme::pal().muted, t("settings.never_synced"));
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
        state.main.benchmark_running = true;
        state.main.benchmark_error = None;
        // Cancel-aware end-to-end, same shape as before, but the scrape itself
        // is now caught on its own (see `spawn_flag_guarded`) so a panic
        // mid-scrape still resets `benchmark_running` instead of locking the
        // button on "Syncing…" forever.
        let spawned = spawn_flag_guarded(
            state,
            "settings-benchmark-sync",
            move |token| {
                if token.is_cancelled() {
                    None
                } else {
                    let r = gw2_optimizer::scraper::scrape_all_with_progress(
                        &addon_dir,
                        &|| token.is_cancelled(),
                        &|src, msg| {
                            let src = src.to_string();
                            let msg = msg.to_string();
                            let _ = crate::state::with_state(|s| {
                                s.main.benchmark_live.insert(src.clone(), msg.clone());
                            });
                        },
                    );
                    if token.is_cancelled() {
                        None
                    } else {
                        Some(r)
                    }
                }
            },
            |s, outcome| {
                s.main.benchmark_running = false;
                s.main.benchmark_live.clear();
                let results = match outcome {
                    Ok(Some(results)) => results,
                    Ok(None) => return,
                    Err(_) => {
                        s.main.benchmark_error = Some("thread panicked".into());
                        return;
                    }
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
                s.main.benchmark_last_synced =
                    Some(chrono::Utc::now().format("%Y-%m-%d").to_string());
            },
        );
        if !spawned {
            state.main.benchmark_running = false;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh global `STATE` rooted at a per-test temp dir, mirroring
    /// `state::tests::init_worker_test` (that helper is private to `state.rs`'s
    /// own test module, so this leaf's tests need their own copy built from the
    /// `pub` `init`/`clear` functions).
    fn init_test_state(label: &str) {
        crate::state::clear();
        let dir = std::env::temp_dir().join(format!(
            "gw2_settings_test_{}_{}",
            std::process::id(),
            label
        ));
        std::fs::create_dir_all(&dir).unwrap();
        crate::state::init(dir);
    }

    /// The exact bug this leaf fixes: a Settings-tab "in progress" flag (e.g.
    /// `settings_key_validating`, `benchmark_running`) that never clears
    /// because the risky call panicked before the code that resets it could
    /// run. This spawns a real worker through the real `spawn_flag_guarded`
    /// helper (the one every settings.rs call site uses) with a `risky`
    /// closure that genuinely panics, then asserts the flag comes back clear.
    #[test]
    fn settings_spawn_clears_flags_on_panic() {
        let _serial = crate::state::state_test_guard();
        init_test_state("spawn_panic_clears_flag");

        let apply_saw_panic = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let w_apply_saw_panic = apply_saw_panic.clone();

        crate::state::with_state(|s| {
            // Prime the flag exactly like a real Test/Save/Sync click would.
            s.main.settings_key_validating = true;
            spawn_flag_guarded(
                s,
                "test-settings-panic",
                // Stands in for `create_client(...).map(|c| c.validate_key_detailed())`
                // blowing up mid-call.
                |_token| panic!("boom: simulated risky-closure panic"),
                move |s, outcome: std::thread::Result<()>| {
                    s.main.settings_key_validating = false;
                    w_apply_saw_panic.store(outcome.is_err(), std::sync::atomic::Ordering::SeqCst);
                },
            );
        });

        let report = crate::state::join_workers(std::time::Duration::from_secs(5));
        assert_eq!(
            report.joined, 1,
            "the panicking worker must still be joined: {report}"
        );
        assert_eq!(
            report.panicked, 0,
            "spawn_worker's own containment guard must swallow the unwind \
             before the thread ends: {report}"
        );
        assert!(
            apply_saw_panic.load(std::sync::atomic::Ordering::SeqCst),
            "apply must observe the risky closure's panic as Err, not skip it"
        );
        assert_eq!(
            crate::state::with_state(|s| s.main.settings_key_validating),
            Some(false),
            "settings_key_validating must be cleared even though the risky \
             closure panicked — a stuck flag here is a spinner the user can \
             never clear"
        );

        crate::state::clear();
    }

    /// `cached_pack_status` must serve repeated lookups for the same language
    /// from its in-frame cache instead of hitting `pack_status` (and the ~800
    /// KB pack file behind it) again — verified by counting real calls to the
    /// underlying `gw2_api::localize::pack_status` indirectly: the cache
    /// returns the *first* value for a language until `tick_pack_status_cache`
    /// or `invalidate_pack_status_cache` next asks it to expire.
    #[test]
    fn cached_pack_status_reuses_value_within_a_frame() {
        let dir = std::env::temp_dir().join(format!(
            "gw2_settings_test_{}_pack_status_cache",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cache = gw2_api::cache::DataCache::new(&dir);

        // "en" is not in `API_LANGS`, so `pack_status` always returns `None`
        // cheaply — this test is about the cache plumbing, not disk I/O.
        invalidate_pack_status_cache();
        tick_pack_status_cache(Some(1));
        let first = cached_pack_status(&cache, "en", Some(1));
        let second = cached_pack_status(&cache, "en", Some(1));
        assert_eq!(
            first, second,
            "two lookups in the same tick must agree (cache hit, not a fresh parse)"
        );

        // A build-number change (a new game patch landing mid-session) must
        // still be observed on the very next tick rather than staying stuck
        // on the old cached value for the rest of the throttle window.
        tick_pack_status_cache(Some(2));
        let after_build_change = cached_pack_status(&cache, "en", Some(2));
        assert_eq!(
            after_build_change,
            gw2_api::localize::PackStatus::None,
            "a build change must not desync the cache from what pack_status would say"
        );
    }

    /// Clicking Custom must open the editor on the preset the player is
    /// looking at — and must never eat a custom theme they already made.
    #[test]
    fn seed_custom_from_preset_only_touches_an_untouched_theme() {
        use gw2_core::config::CustomTheme;

        // Fresh custom theme + a real preset selected: seeded from that preset.
        let mut fresh = ThemeConfig {
            preset: "molten-ember".into(),
            custom: CustomTheme::default(),
        };
        assert!(seed_custom_from_preset(&mut fresh));
        let [bg, panel, accent, text, muted] =
            theme::preset_bases("molten-ember").expect("molten-ember is a built-in preset");
        assert_eq!(
            (
                fresh.custom.bg,
                fresh.custom.panel,
                fresh.custom.accent,
                fresh.custom.text,
                fresh.custom.muted
            ),
            (bg, panel, accent, text, muted)
        );
        assert!(
            fresh.custom.name.is_empty(),
            "seeding copies colors, never a name"
        );

        // A theme the player has edited is left exactly as they left it.
        let mine = CustomTheme {
            name: "Pinkfrost".into(),
            bg: [0.10, 0.02, 0.08],
            panel: [0.18, 0.05, 0.14],
            accent: [1.0, 0.35, 0.75],
            text: [0.96, 0.90, 0.94],
            muted: [0.60, 0.48, 0.56],
        };
        let mut edited = ThemeConfig {
            preset: "verdant-wilds".into(),
            custom: mine.clone(),
        };
        assert!(!seed_custom_from_preset(&mut edited));
        assert_eq!(edited.custom, mine, "an edited custom theme survives");

        // Nothing to copy from: "custom" itself, and an unknown id.
        for preset in ["custom", "no-such-theme"] {
            let mut t = ThemeConfig {
                preset: preset.into(),
                custom: CustomTheme::default(),
            };
            assert!(!seed_custom_from_preset(&mut t), "{preset} has no bases");
            assert_eq!(t.custom, CustomTheme::default());
        }
    }

    /// Every i18n key the Theme section renders must exist in the locale
    /// catalogs — `t()` echoes the key itself when it is missing everywhere,
    /// so equality means a hole in the Settings chrome. (Key parity across
    /// all 12 locales is asserted by gw2-core's
    /// `every_locale_parses_and_covers_english_keys`; this guards the
    /// renderer's key spelling against the catalog.)
    #[test]
    fn theme_section_locale_keys_exist() {
        for key in [
            "settings.theme_section",
            "settings.theme_custom",
            "settings.theme_name_hint",
            "settings.theme_bg",
            "settings.theme_panel",
            "settings.theme_accent",
            "settings.theme_text",
            "settings.theme_muted",
            "settings.theme_bg_desc",
            "settings.theme_panel_desc",
            "settings.theme_accent_desc",
            "settings.theme_text_desc",
            "settings.theme_muted_desc",
            "settings.theme_custom_hint",
        ] {
            assert_ne!(t(key), key, "locale catalogs are missing {key}");
        }
    }

    /// Settings LLM key InputText is the same overlay a player may stream or
    /// screenshot. `key_field_is_masked` is the exact function the
    /// `.password(...)` call site uses, and the reveal toggle must default to
    /// hidden — same contract as Setup's `setup_key_fields_are_masked`.
    #[test]
    fn settings_llm_key_field_is_masked() {
        assert!(
            key_field_is_masked(false),
            "an un-revealed key field must render with the PASSWORD flag set"
        );
        assert!(
            !key_field_is_masked(true),
            "toggling reveal on must unmask the field"
        );

        let llm_default_revealed = SHOW_LLM_KEY.with(|c| c.get());
        assert!(
            !llm_default_revealed,
            "the Settings LLM key reveal toggle must default to hidden"
        );
        assert!(
            key_field_is_masked(llm_default_revealed),
            "the Settings LLM key field must be masked on first render"
        );
    }
}
