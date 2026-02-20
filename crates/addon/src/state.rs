use std::path::PathBuf;
use std::sync::Mutex;

use gw2_core::config::AppConfig;
use gw2_core::types::{GameMode, ResolvedBuild, StatBlock};

static STATE: Mutex<Option<AddonState>> = Mutex::new(None);

pub struct AddonState {
    pub window_visible: bool,
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub addon_dir: PathBuf,
    pub screen: Screen,
    // Setup wizard transient state
    pub setup: SetupState,
    // Main UI state
    pub main: MainState,
}

#[derive(Default)]
pub struct MainState {
    pub characters: Vec<String>,
    pub characters_loading: bool,
    pub selected_character: Option<usize>,
    pub game_mode: GameMode,
    pub current_build: Option<ResolvedBuild>,
    pub current_stats: Option<StatBlock>,
    pub build_loading: bool,
    pub error: Option<String>,
    // Left menu
    pub active_tab: MainTab,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub enum MainTab {
    #[default]
    NewBuild,
    Improve,
    SaveLoad,
    Settings,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Setup(SetupStep),
    Main,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetupStep {
    Gw2ApiKey,
    GeminiApiKey,
    DataDownload,
    Complete,
}

#[derive(Default)]
pub struct SetupState {
    // GW2 key input
    pub gw2_key_input: String,
    pub gw2_key_status: KeyStatus,
    pub gw2_key_scopes: Vec<(String, bool)>, // (scope_name, present)
    // Gemini key input
    pub gemini_key_input: String,
    pub gemini_key_status: KeyStatus,
    // Data download
    pub download_progress: Option<DownloadState>,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub enum KeyStatus {
    #[default]
    NotValidated,
    Validating,
    Valid,
    Invalid(String), // error message
}

#[derive(Debug, Clone)]
pub struct DownloadState {
    pub current_step: usize,
    pub total_steps: usize,
    pub step_name: String,
    pub done: bool,
    pub error: Option<String>,
}

fn lock_state() -> std::sync::MutexGuard<'static, Option<AddonState>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn init(addon_dir: PathBuf) {
    let config_path = AppConfig::config_path(&addon_dir);
    let config = AppConfig::load(&config_path);

    let screen = if config.is_setup_complete() {
        Screen::Main
    } else if config.has_gw2_key() && config.has_gemini_key() {
        // Keys present but cache missing — go to download
        Screen::Setup(SetupStep::DataDownload)
    } else if config.has_gw2_key() {
        Screen::Setup(SetupStep::GeminiApiKey)
    } else {
        Screen::Setup(SetupStep::Gw2ApiKey)
    };

    let mut setup = SetupState::default();
    if let Some(ref key) = config.gw2_api_key {
        setup.gw2_key_input = key.clone();
        setup.gw2_key_status = KeyStatus::Valid;
    }
    if let Some(ref key) = config.gemini_api_key {
        setup.gemini_key_input = key.clone();
        setup.gemini_key_status = KeyStatus::Valid;
    }

    *lock_state() = Some(AddonState {
        window_visible: false,
        config,
        config_path,
        addon_dir,
        screen,
        setup,
        main: MainState::default(),
    });
}

pub fn toggle_window() {
    if let Some(state) = lock_state().as_mut() {
        state.window_visible = !state.window_visible;
    }
}

pub fn is_window_visible() -> bool {
    lock_state()
        .as_ref()
        .map(|s| s.window_visible)
        .unwrap_or(false)
}

/// Clear state on addon unload to prevent stale background thread writes.
pub fn clear() {
    *lock_state() = None;
}

/// Access state for reading/writing in UI code.
pub fn with_state<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut AddonState) -> R,
{
    lock_state().as_mut().map(f)
}
