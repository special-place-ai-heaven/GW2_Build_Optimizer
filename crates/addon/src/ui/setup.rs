use nexus::imgui::{ProgressBar, Ui};

use crate::state::{AddonState, DownloadState, KeyStatus, Screen, SetupStep};
use crate::ui::theme;

pub fn render_setup(ui: &Ui, state: &mut AddonState, step: SetupStep) {
    theme::header(ui, "FIRST-TIME SETUP");

    let steps = [
        (SetupStep::Gw2ApiKey, "GW2 key"),
        (SetupStep::LlmApiKey, "AI key"),
        (SetupStep::DataDownload, "Game data"),
        (SetupStep::Complete, "Ready"),
    ];
    let current_idx = match step {
        SetupStep::Gw2ApiKey => 0,
        SetupStep::LlmApiKey => 1,
        SetupStep::DataDownload => 2,
        SetupStep::Complete => 3,
    };
    for (i, (target, name)) in steps.iter().enumerate() {
        if i > 0 {
            ui.same_line_with_spacing(0.0, 8.0);
        }
        let selected = i == current_idx;
        let done = i < current_idx;
        if theme::pill(ui, name, selected || done, &format!("##setup_step_{i}")) && done {
            state.screen = Screen::Setup(target.clone());
        }
    }
    ui.spacing();
    ui.spacing();

    match step {
        SetupStep::Gw2ApiKey => render_gw2_key_step(ui, state),
        SetupStep::LlmApiKey => render_llm_key_step(ui, state),
        SetupStep::DataDownload => render_download_step(ui, state),
        SetupStep::Complete => render_complete_step(ui, state),
    }
}

fn render_gw2_key_step(ui: &Ui, state: &mut AddonState) {
    theme::header(ui, "STEP 1  ·  GW2 API KEY");
    ui.spacing();

    ui.text_wrapped(
        "Create an API key at ArenaNet's website. \
         Copy the URL below and paste it in your browser:",
    );
    ui.spacing();

    // Copyable URL
    let url = "https://account.arena.net/applications";
    let mut url_buf = String::from(url);
    ui.set_next_item_width(-1.0);
    ui.input_text("##gw2_url", &mut url_buf)
        .read_only(true)
        .build();
    ui.spacing();

    ui.text_wrapped("Create a 'New Key', name it anything, and select these permissions:");
    ui.bullet_text("account (required)");
    ui.bullet_text("characters (required)");
    ui.bullet_text("builds (required)");
    ui.bullet_text("inventories (recommended)");
    ui.bullet_text("unlocks (recommended)");
    ui.spacing();

    // Key input
    ui.text("Paste your API key:");
    ui.set_next_item_width(-1.0);
    ui.input_text("##gw2_key", &mut state.setup.gw2_key_input)
        .build();
    ui.spacing();

    // Validate button
    let can_validate = !state.setup.gw2_key_input.is_empty()
        && state.setup.gw2_key_status != KeyStatus::Validating;

    if theme::gold_button_sized(ui, "Validate", [120.0, 0.0]) && can_validate {
        let key = state.setup.gw2_key_input.clone();
        state.setup.gw2_key_status = KeyStatus::Validating;

        // Run validation in a background thread.
        // Always populate scope table even if required scopes are missing.
        let tx_key = key.clone();
        let token = state.cancel_token.clone();
        std::thread::spawn(move || {
            let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Reset the Validating spinner on every exit path (including cancel).
                // Without this, navigating away mid-validation pins the status text on
                // "Validating…" until the user clicks Validate again.
                enum SetupOutcome {
                    Cancelled,
                    Invalid(String),
                    Valid {
                        scopes: Vec<(String, bool)>,
                        missing: Vec<String>,
                    },
                }

                let outcome: SetupOutcome = 'validate: {
                    if token.is_cancelled() {
                        break 'validate SetupOutcome::Cancelled;
                    }
                    let client = match gw2_api::client::Gw2Client::with_key(&tx_key) {
                        Ok(c) => c,
                        Err(e) => break 'validate SetupOutcome::Invalid(e.to_string()),
                    };

                    if token.is_cancelled() {
                        break 'validate SetupOutcome::Cancelled;
                    }
                    let info: gw2_api::client::TokenInfo = match client.get("tokeninfo") {
                        Ok(i) => i,
                        Err(e) => break 'validate SetupOutcome::Invalid(e.to_string()),
                    };

                    if token.is_cancelled() {
                        break 'validate SetupOutcome::Cancelled;
                    }

                    let required = ["account", "characters", "builds"];
                    let recommended = ["inventories", "unlocks"];
                    let scopes: Vec<(String, bool)> = required
                        .iter()
                        .chain(recommended.iter())
                        .map(|scope| {
                            (
                                scope.to_string(),
                                info.permissions.contains(&scope.to_string()),
                            )
                        })
                        .collect();
                    let missing: Vec<String> = required
                        .iter()
                        .filter(|s| !info.permissions.contains(&s.to_string()))
                        .map(|s| s.to_string())
                        .collect();
                    SetupOutcome::Valid { scopes, missing }
                };

                crate::state::with_state(|s| match outcome {
                    SetupOutcome::Cancelled => {
                        // Clear the Validating state but leave the scope table alone.
                        if matches!(s.setup.gw2_key_status, KeyStatus::Validating) {
                            s.setup.gw2_key_status = KeyStatus::NotValidated;
                        }
                    }
                    SetupOutcome::Invalid(e) => {
                        s.setup.gw2_key_status = KeyStatus::Invalid(e);
                    }
                    SetupOutcome::Valid { scopes, missing } => {
                        s.setup.gw2_key_scopes = scopes;
                        if missing.is_empty() {
                            s.setup.gw2_key_status = KeyStatus::Valid;
                            s.config.gw2_api_key = Some(tx_key);
                            if let Err(e) = s.config.save(&s.config_path) {
                                nexus::log::log(
                                    nexus::log::LogLevel::Warning,
                                    "GW2BuildOpt",
                                    format!("Config save failed: {}", e),
                                );
                            }
                        } else {
                            s.setup.gw2_key_status = KeyStatus::Invalid(format!(
                                "Missing required scopes: {}",
                                missing.join(", ")
                            ));
                        }
                    }
                });
            }));
            if panic_result.is_err() {
                nexus::log::log(
                    nexus::log::LogLevel::Warning,
                    "GW2BuildOpt",
                    "bg thread panicked: setup_gw2_key_validation",
                );
                crate::state::with_state(|s| {
                    s.setup.gw2_key_status = KeyStatus::Invalid("thread panicked".into());
                });
            }
        });
    }

    ui.same_line();

    // Status indicator
    match &state.setup.gw2_key_status {
        KeyStatus::NotValidated => ui.text("Enter your key and click Validate"),
        KeyStatus::Validating => ui.text("Validating..."),
        KeyStatus::Valid => {
            ui.text_colored(theme::OPTIMIZED, "Valid!");
        }
        KeyStatus::Invalid(msg) => {
            ui.text_colored(theme::ERR, format!("Error: {}", msg));
        }
    }

    // Show scopes if validated
    if !state.setup.gw2_key_scopes.is_empty() {
        ui.spacing();
        ui.text("Permissions:");
        for (scope, present) in &state.setup.gw2_key_scopes {
            if *present {
                ui.text_colored(theme::OPTIMIZED, format!("  [v] {}", scope));
            } else {
                ui.text_colored(theme::WARN, format!("  [x] {} (missing)", scope));
            }
        }
    }

    // Next button
    ui.spacing();
    if state.setup.gw2_key_status == KeyStatus::Valid
        && theme::gold_button_sized(ui, "Next >>", [120.0, 0.0])
    {
        state.screen = Screen::Setup(SetupStep::LlmApiKey);
    }
}

fn render_llm_key_step(ui: &Ui, state: &mut AddonState) {
    use gw2_core::config::LlmProvider;

    theme::header(ui, "STEP 2  ·  AI PROVIDER");
    ui.spacing();

    ui.text_wrapped(
        "Choose an AI provider and enter your API key. \
         The optimizer uses AI for build synergy reasoning.",
    );
    ui.spacing();

    // Provider radio buttons
    ui.text("Provider:");
    for provider in &LlmProvider::ALL {
        let label = provider.label();
        if ui.radio_button_bool(label, state.config.active_provider == *provider)
            && state.config.active_provider != *provider
        {
            state.config.active_provider = provider.clone();
            // Reset validation when switching providers
            state.setup.llm_key_input.clear();
            state.setup.llm_key_status = KeyStatus::NotValidated;
            // Pre-fill if we already have a key for this provider
            if let Some(key) = state.config.active_api_key() {
                state.setup.llm_key_input = key.to_string();
                state.setup.llm_key_status = KeyStatus::Valid;
            }
        }
    }
    ui.spacing();

    // Provider-specific help text and URL
    let (help_text, url, url_instructions) = match state.config.active_provider {
        LlmProvider::Gemini => (
            "Get a free Gemini API key from Google AI Studio:",
            "https://aistudio.google.com/apikey",
            "Click 'Create API key', select any project, and copy the key.",
        ),
        LlmProvider::OpenAI => (
            "Get an OpenAI API key from the OpenAI platform:",
            "https://platform.openai.com/api-keys",
            "Click 'Create new secret key', name it, and copy the key.",
        ),
        LlmProvider::Anthropic => (
            "Get an Anthropic API key from the Anthropic console:",
            "https://console.anthropic.com/settings/keys",
            "Click 'Create Key', name it, and copy the key.",
        ),
        LlmProvider::OpenRouter => (
            "Get an OpenRouter API key (one key, hundreds of models):",
            "https://openrouter.ai/keys",
            "Click 'Create Key', name it, and copy the key. You can pre-load credits at openrouter.ai/credits.",
        ),
    };

    ui.text_wrapped(help_text);
    ui.spacing();

    let mut url_buf = String::from(url);
    ui.set_next_item_width(-1.0);
    ui.input_text("##llm_url", &mut url_buf)
        .read_only(true)
        .build();
    ui.spacing();

    ui.text_wrapped(url_instructions);
    ui.spacing();

    // Key input
    let provider_label = state.config.active_provider.label();
    ui.text(format!("Paste your {} API key:", provider_label));
    ui.set_next_item_width(-1.0);
    ui.input_text("##llm_key", &mut state.setup.llm_key_input)
        .build();
    ui.spacing();

    // Validate button
    let can_validate = !state.setup.llm_key_input.is_empty()
        && state.setup.llm_key_status != KeyStatus::Validating;

    if theme::gold_button_sized(ui, "Validate", [120.0, 0.0]) && can_validate {
        let key = state.setup.llm_key_input.clone();
        let provider = state.config.active_provider.clone();
        state.setup.llm_key_status = KeyStatus::Validating;

        let token = state.cancel_token.clone();
        std::thread::spawn(move || {
            let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Reset the Validating spinner on every exit path. Without this,
                // a cancellation mid-validation pins the status on "Validating…".
                let result = if token.is_cancelled() {
                    None
                } else {
                    let r = (|| -> Result<(), gw2_optimizer::llm::LlmError> {
                        use gw2_optimizer::llm::LlmClient;
                        match provider {
                            LlmProvider::Gemini => {
                                let c = gw2_optimizer::llm::gemini::GeminiLlmClient::new(
                                    &key,
                                    gw2_core::config::DEFAULT_GEMINI_MODEL,
                                )?;
                                c.validate_key()
                            }
                            LlmProvider::OpenAI => {
                                let c = gw2_optimizer::llm::openai::OpenAiClient::new(
                                    &key,
                                    gw2_core::config::DEFAULT_OPENAI_MODEL,
                                )?;
                                c.validate_key()
                            }
                            LlmProvider::Anthropic => {
                                let c = gw2_optimizer::llm::anthropic::AnthropicClient::new(
                                    &key,
                                    gw2_core::config::DEFAULT_ANTHROPIC_MODEL,
                                )?;
                                c.validate_key()
                            }
                            LlmProvider::OpenRouter => {
                                let c = gw2_optimizer::llm::openrouter::OpenRouterClient::new(
                                    &key,
                                    gw2_core::config::DEFAULT_OPENROUTER_MODEL,
                                )?;
                                c.validate_key()
                            }
                        }
                    })();
                    if token.is_cancelled() {
                        None
                    } else {
                        Some(r)
                    }
                };

                crate::state::with_state(|s| match result {
                    Some(Ok(())) => {
                        s.setup.llm_key_status = KeyStatus::Valid;
                        // Store key in the correct provider slot
                        match s.config.active_provider {
                            LlmProvider::Gemini => {
                                s.config.gemini_api_key = Some(key);
                            }
                            LlmProvider::OpenAI => {
                                s.config.openai_api_key = Some(key);
                            }
                            LlmProvider::Anthropic => {
                                s.config.anthropic_api_key = Some(key);
                            }
                            LlmProvider::OpenRouter => {
                                s.config.openrouter_api_key = Some(key);
                            }
                        }
                        if let Err(e) = s.config.save(&s.config_path) {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2BuildOpt",
                                format!("Config save failed: {}", e),
                            );
                        }
                    }
                    Some(Err(e)) => {
                        s.setup.llm_key_status = KeyStatus::Invalid(e.to_string());
                    }
                    None => {
                        // Cancelled. Clear the Validating spinner without overwriting
                        // a status the user has since set (e.g. by switching providers).
                        if matches!(s.setup.llm_key_status, KeyStatus::Validating) {
                            s.setup.llm_key_status = KeyStatus::NotValidated;
                        }
                    }
                });
            }));
            if panic_result.is_err() {
                nexus::log::log(
                    nexus::log::LogLevel::Warning,
                    "GW2BuildOpt",
                    "bg thread panicked: setup_llm_key_validation",
                );
                crate::state::with_state(|s| {
                    s.setup.llm_key_status = KeyStatus::Invalid("thread panicked".into());
                });
            }
        });
    }

    ui.same_line();

    match &state.setup.llm_key_status {
        KeyStatus::NotValidated => ui.text("Enter your key and click Validate"),
        KeyStatus::Validating => ui.text("Validating..."),
        KeyStatus::Valid => {
            ui.text_colored(theme::OPTIMIZED, "Valid!");
        }
        KeyStatus::Invalid(msg) => {
            ui.text_colored(theme::ERR, format!("Error: {}", msg));
        }
    }

    // Navigation
    ui.spacing();
    if theme::gold_button_sized(ui, "<< Back", [120.0, 0.0]) {
        state.screen = Screen::Setup(SetupStep::Gw2ApiKey);
    }
    if state.setup.llm_key_status == KeyStatus::Valid {
        ui.same_line();
        if theme::gold_button_sized(ui, "Next >>", [120.0, 0.0]) {
            state.screen = Screen::Setup(SetupStep::DataDownload);
        }
    }
}

fn render_download_step(ui: &Ui, state: &mut AddonState) {
    theme::header(ui, "STEP 3  ·  GAME DATA");
    ui.spacing();

    ui.text_wrapped(
        "Downloading skills, traits, items, and other game data. \
         This only happens once and when the game updates.",
    );
    ui.spacing();

    let progress_snapshot = state.setup.download_progress.clone();
    match progress_snapshot {
        None => {
            // Not started yet — show start button
            if theme::gold_button_sized(ui, "Start Download", [160.0, 0.0]) {
                state.setup.download_progress = Some(DownloadState {
                    current_step: 0,
                    total_steps: 9,
                    step_name: "Starting...".into(),
                    done: false,
                    error: None,
                });

                let cache_dir = state.addon_dir.join("cache");
                let token = state.cancel_token.clone();

                std::thread::spawn(move || {
                    let panic_result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            // Drive the download in a single labelled block so every exit
                            // path lands at the unified state write below. Previously,
                            // cancel mid-download froze the progress bar with no Retry/Next
                            // affordance.
                            enum DlOutcome {
                                Cancelled,
                                ClientError(String),
                                DownloadError(String),
                                Ok(u32),
                            }

                            let outcome: DlOutcome = 'download: {
                                if token.is_cancelled() {
                                    break 'download DlOutcome::Cancelled;
                                }
                                let client = match gw2_api::client::Gw2Client::without_key() {
                                    Ok(c) => c,
                                    Err(e) => {
                                        break 'download DlOutcome::ClientError(e.to_string())
                                    }
                                };
                                let cache = gw2_api::cache::DataCache::new(&cache_dir);

                                let token_inner = token.clone();
                                let result =
                                    gw2_api::download::download_all(&client, &cache, |progress| {
                                        if token_inner.is_cancelled() {
                                            return;
                                        }
                                        crate::state::with_state(|s| {
                                            let name = if let Some(ref detail) = progress.detail {
                                                format!("{} ({})", progress.step_name, detail)
                                            } else {
                                                progress.step_name.clone()
                                            };
                                            s.setup.download_progress = Some(DownloadState {
                                                current_step: progress.current_step,
                                                total_steps: progress.total_steps,
                                                step_name: name,
                                                done: progress.done,
                                                error: None,
                                            });
                                        });
                                    });

                                if token.is_cancelled() {
                                    break 'download DlOutcome::Cancelled;
                                }
                                match result {
                                    Ok(b) => DlOutcome::Ok(b),
                                    Err(e) => DlOutcome::DownloadError(e.to_string()),
                                }
                            };

                            crate::state::with_state(|s| match outcome {
                                DlOutcome::Ok(build) => {
                                    s.config.cache_build_number = Some(build);
                                    if let Err(e) = s.config.save(&s.config_path) {
                                        nexus::log::log(
                                            nexus::log::LogLevel::Warning,
                                            "GW2BuildOpt",
                                            format!("Config save failed: {}", e),
                                        );
                                    }
                                    if let Some(ref mut dl) = s.setup.download_progress {
                                        dl.done = true;
                                    }
                                }
                                DlOutcome::ClientError(e) | DlOutcome::DownloadError(e) => {
                                    if let Some(ref mut dl) = s.setup.download_progress {
                                        dl.error = Some(e);
                                    }
                                }
                                DlOutcome::Cancelled => {
                                    // Surface cancellation as an error so the user gets the
                                    // Retry button. Otherwise the progress bar freezes with
                                    // no way forward.
                                    if let Some(ref mut dl) = s.setup.download_progress {
                                        dl.error = Some("Cancelled".into());
                                    }
                                }
                            });
                        }));
                    if panic_result.is_err() {
                        nexus::log::log(
                            nexus::log::LogLevel::Warning,
                            "GW2BuildOpt",
                            "bg thread panicked: setup_data_download",
                        );
                        crate::state::with_state(|s| {
                            s.setup.download_progress = Some(crate::state::DownloadState {
                                current_step: 0,
                                total_steps: 0,
                                step_name: String::new(),
                                done: true,
                                error: Some("thread panicked".into()),
                            });
                        });
                    }
                });
            }
        }
        Some(dl) => {
            // Show progress
            let fraction = if dl.total_steps > 0 {
                dl.current_step as f32 / dl.total_steps as f32
            } else {
                0.0
            };

            let overlay = format!("{}/{} - {}", dl.current_step, dl.total_steps, dl.step_name);
            ProgressBar::new(fraction).overlay_text(&overlay).build(ui);

            if let Some(ref err) = dl.error {
                ui.spacing();
                ui.text_colored(theme::ERR, format!("Error: {}", err));
                ui.spacing();
                if theme::gold_button_sized(ui, "Retry", [120.0, 0.0]) {
                    state.setup.download_progress = None;
                }
            } else if dl.done {
                ui.spacing();
                ui.text_colored(theme::OPTIMIZED, "Download complete!");
                ui.spacing();
                if theme::gold_button_sized(ui, "Next >>", [120.0, 0.0]) {
                    state.screen = Screen::Setup(SetupStep::Complete);
                }
            }
        }
    }

    // Back button (only if not downloading)
    let is_downloading = state
        .setup
        .download_progress
        .as_ref()
        .is_some_and(|dl| !dl.done && dl.error.is_none());

    if !is_downloading {
        ui.spacing();
        if theme::gold_button_sized(ui, "<< Back", [120.0, 0.0]) {
            state.screen = Screen::Setup(SetupStep::LlmApiKey);
        }
    }
}

fn render_complete_step(ui: &Ui, state: &mut AddonState) {
    theme::header(ui, "SETUP COMPLETE");
    ui.spacing();

    ui.text_colored(theme::OPTIMIZED, "Everything is configured and ready.");
    ui.spacing();

    ui.text_wrapped(
        "You can now use the GW2 Build Optimizer. \
         Press Ctrl+Shift+O anytime to open this window.",
    );
    ui.spacing();

    if theme::gold_button_sized(ui, "Get Started >>", [160.0, 0.0]) {
        state.screen = Screen::Main;
    }
}
