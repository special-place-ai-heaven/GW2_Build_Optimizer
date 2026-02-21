use nexus::imgui::{ProgressBar, Ui};

use crate::state::{
    AddonState, DownloadState, KeyStatus, Screen, SetupStep,
};

pub fn render_setup(ui: &Ui, state: &mut AddonState, step: SetupStep) {
    ui.text("First-Time Setup");
    ui.separator();

    // Step indicators
    let steps = ["GW2 API Key", "Gemini API Key", "Download Data", "Done"];
    let current_idx = match step {
        SetupStep::Gw2ApiKey => 0,
        SetupStep::GeminiApiKey => 1,
        SetupStep::DataDownload => 2,
        SetupStep::Complete => 3,
    };
    let indicator: String = steps
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if i < current_idx {
                format!("[v] {}", name)
            } else if i == current_idx {
                format!("[>] {}", name)
            } else {
                format!("[ ] {}", name)
            }
        })
        .collect::<Vec<_>>()
        .join("  ");
    ui.text(&indicator);
    ui.spacing();
    ui.separator();
    ui.spacing();

    match step {
        SetupStep::Gw2ApiKey => render_gw2_key_step(ui, state),
        SetupStep::GeminiApiKey => render_gemini_key_step(ui, state),
        SetupStep::DataDownload => render_download_step(ui, state),
        SetupStep::Complete => render_complete_step(ui, state),
    }
}

fn render_gw2_key_step(ui: &Ui, state: &mut AddonState) {
    ui.text("Step 1: GW2 API Key");
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
    ui.input_text("##gw2_url", &mut url_buf).read_only(true).build();
    ui.spacing();

    ui.text_wrapped(
        "Create a 'New Key', name it anything, and select these permissions:",
    );
    ui.bullet_text("account (required)");
    ui.bullet_text("characters (required)");
    ui.bullet_text("builds (required)");
    ui.bullet_text("inventories (recommended)");
    ui.bullet_text("unlocks (recommended)");
    ui.spacing();

    // Key input
    ui.text("Paste your API key:");
    ui.set_next_item_width(-1.0);
    ui.input_text("##gw2_key", &mut state.setup.gw2_key_input).build();
    ui.spacing();

    // Validate button
    let can_validate = !state.setup.gw2_key_input.is_empty()
        && state.setup.gw2_key_status != KeyStatus::Validating;

    if ui.button_with_size("Validate", [120.0, 0.0]) && can_validate {
        let key = state.setup.gw2_key_input.clone();
        state.setup.gw2_key_status = KeyStatus::Validating;

        // Run validation in a background thread.
        // Always populate scope table even if required scopes are missing.
        let tx_key = key.clone();
        std::thread::spawn(move || {
            let client = match gw2_api::client::Gw2Client::with_key(&tx_key) {
                Ok(c) => c,
                Err(e) => {
                    crate::state::with_state(|s| {
                        s.setup.gw2_key_status = KeyStatus::Invalid(e.to_string());
                    });
                    return;
                }
            };

            // Fetch token info (always, to populate scope table)
            let info: gw2_api::client::TokenInfo = match client.get("tokeninfo") {
                Ok(i) => i,
                Err(e) => {
                    crate::state::with_state(|s| {
                        s.setup.gw2_key_status = KeyStatus::Invalid(e.to_string());
                    });
                    return;
                }
            };

            let required = ["account", "characters", "builds"];
            let recommended = ["inventories", "unlocks"];
            let all_scopes: Vec<_> = required
                .iter()
                .chain(recommended.iter())
                .map(|scope| {
                    let present = info.permissions.contains(&scope.to_string());
                    (scope.to_string(), present)
                })
                .collect();

            let missing_required: Vec<_> = required
                .iter()
                .filter(|s| !info.permissions.contains(&s.to_string()))
                .collect();

            crate::state::with_state(|s| {
                s.setup.gw2_key_scopes = all_scopes;
                if missing_required.is_empty() {
                    s.setup.gw2_key_status = KeyStatus::Valid;
                    s.config.gw2_api_key = Some(tx_key);
                    let _ = s.config.save(&s.config_path);
                } else {
                    let names: Vec<_> = missing_required.iter().map(|s| s.to_string()).collect();
                    s.setup.gw2_key_status = KeyStatus::Invalid(
                        format!("Missing required scopes: {}", names.join(", ")),
                    );
                }
            });
        });
    }

    ui.same_line();

    // Status indicator
    match &state.setup.gw2_key_status {
        KeyStatus::NotValidated => ui.text("Enter your key and click Validate"),
        KeyStatus::Validating => ui.text("Validating..."),
        KeyStatus::Valid => {
            ui.text_colored([0.0, 1.0, 0.0, 1.0], "Valid!");
        }
        KeyStatus::Invalid(msg) => {
            ui.text_colored([1.0, 0.0, 0.0, 1.0], &format!("Error: {}", msg));
        }
    }

    // Show scopes if validated
    if !state.setup.gw2_key_scopes.is_empty() {
        ui.spacing();
        ui.text("Permissions:");
        for (scope, present) in &state.setup.gw2_key_scopes {
            if *present {
                ui.text_colored([0.0, 1.0, 0.0, 1.0], &format!("  [v] {}", scope));
            } else {
                ui.text_colored([1.0, 0.3, 0.0, 1.0], &format!("  [x] {} (missing)", scope));
            }
        }
    }

    // Next button
    ui.spacing();
    if state.setup.gw2_key_status == KeyStatus::Valid {
        if ui.button_with_size("Next >>", [120.0, 0.0]) {
            state.screen = Screen::Setup(SetupStep::GeminiApiKey);
        }
    }
}

fn render_gemini_key_step(ui: &Ui, state: &mut AddonState) {
    ui.text("Step 2: Google Gemini API Key");
    ui.spacing();

    ui.text_wrapped(
        "Get a free Gemini API key from Google AI Studio. \
         Copy the URL below and paste it in your browser:",
    );
    ui.spacing();

    let url = "https://aistudio.google.com/apikey";
    let mut url_buf = String::from(url);
    ui.set_next_item_width(-1.0);
    ui.input_text("##gemini_url", &mut url_buf).read_only(true).build();
    ui.spacing();

    ui.text_wrapped(
        "Click 'Create API key', select any project, and copy the key.",
    );
    ui.spacing();

    // Key input
    ui.text("Paste your Gemini API key:");
    ui.set_next_item_width(-1.0);
    ui.input_text("##gemini_key", &mut state.setup.gemini_key_input)
        .build();
    ui.spacing();

    // Validate button
    let can_validate = !state.setup.gemini_key_input.is_empty()
        && state.setup.gemini_key_status != KeyStatus::Validating;

    if ui.button_with_size("Validate", [120.0, 0.0]) && can_validate {
        let key = state.setup.gemini_key_input.clone();
        state.setup.gemini_key_status = KeyStatus::Validating;

        std::thread::spawn(move || {
            let result = gw2_optimizer::gemini::GeminiClient::new(&key)
                .and_then(|c| c.validate_key());

            crate::state::with_state(|s| match result {
                Ok(()) => {
                    s.setup.gemini_key_status = KeyStatus::Valid;
                    s.config.gemini_api_key = Some(key);
                    let _ = s.config.save(&s.config_path);
                }
                Err(e) => {
                    s.setup.gemini_key_status =
                        KeyStatus::Invalid(e.to_string());
                }
            });
        });
    }

    ui.same_line();

    match &state.setup.gemini_key_status {
        KeyStatus::NotValidated => ui.text("Enter your key and click Validate"),
        KeyStatus::Validating => ui.text("Validating..."),
        KeyStatus::Valid => {
            ui.text_colored([0.0, 1.0, 0.0, 1.0], "Valid!");
        }
        KeyStatus::Invalid(msg) => {
            ui.text_colored([1.0, 0.0, 0.0, 1.0], &format!("Error: {}", msg));
        }
    }

    // Navigation
    ui.spacing();
    if ui.button_with_size("<< Back", [120.0, 0.0]) {
        state.screen = Screen::Setup(SetupStep::Gw2ApiKey);
    }
    if state.setup.gemini_key_status == KeyStatus::Valid {
        ui.same_line();
        if ui.button_with_size("Next >>", [120.0, 0.0]) {
            state.screen = Screen::Setup(SetupStep::DataDownload);
        }
    }
}

fn render_download_step(ui: &Ui, state: &mut AddonState) {
    ui.text("Step 3: Download Game Data");
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
            if ui.button_with_size("Start Download", [160.0, 0.0]) {
                state.setup.download_progress = Some(DownloadState {
                    current_step: 0,
                    total_steps: 8,
                    step_name: "Starting...".into(),
                    done: false,
                    error: None,
                });

                let cache_dir = state.addon_dir.join("cache");

                std::thread::spawn(move || {
                    let client = match gw2_api::client::Gw2Client::without_key() {
                        Ok(c) => c,
                        Err(e) => {
                            crate::state::with_state(|s| {
                                if let Some(ref mut dl) = s.setup.download_progress {
                                    dl.error = Some(e.to_string());
                                }
                            });
                            return;
                        }
                    };
                    let cache = gw2_api::cache::DataCache::new(&cache_dir);

                    let result = gw2_api::download::download_all(&client, &cache, |progress| {
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

                    crate::state::with_state(|s| match result {
                        Ok(build) => {
                            s.config.cache_build_number = Some(build);
                            let _ = s.config.save(&s.config_path);
                            if let Some(ref mut dl) = s.setup.download_progress {
                                dl.done = true;
                            }
                        }
                        Err(e) => {
                            if let Some(ref mut dl) = s.setup.download_progress {
                                dl.error = Some(e.to_string());
                            }
                        }
                    });
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

            let overlay = format!(
                "{}/{} - {}",
                dl.current_step, dl.total_steps, dl.step_name
            );
            ProgressBar::new(fraction)
                .overlay_text(&overlay)
                .build(ui);

            if let Some(ref err) = dl.error {
                ui.spacing();
                ui.text_colored([1.0, 0.0, 0.0, 1.0], &format!("Error: {}", err));
                ui.spacing();
                if ui.button_with_size("Retry", [120.0, 0.0]) {
                    state.setup.download_progress = None;
                }
            } else if dl.done {
                ui.spacing();
                ui.text_colored([0.0, 1.0, 0.0, 1.0], "Download complete!");
                ui.spacing();
                if ui.button_with_size("Next >>", [120.0, 0.0]) {
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
        if ui.button_with_size("<< Back", [120.0, 0.0]) {
            state.screen = Screen::Setup(SetupStep::GeminiApiKey);
        }
    }
}

fn render_complete_step(ui: &Ui, state: &mut AddonState) {
    ui.text("Setup Complete!");
    ui.spacing();

    ui.text_colored(
        [0.0, 1.0, 0.0, 1.0],
        "Everything is configured and ready.",
    );
    ui.spacing();

    ui.text_wrapped(
        "You can now use the GW2 Build Optimizer. \
         Press Ctrl+Shift+O anytime to open this window.",
    );
    ui.spacing();

    if ui.button_with_size("Get Started >>", [160.0, 0.0]) {
        state.screen = Screen::Main;
    }
}
