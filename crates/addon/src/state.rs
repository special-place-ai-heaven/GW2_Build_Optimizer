use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gw2_core::config::AppConfig;
use gw2_core::types::{GameMode, ResolvedBuild, SavedBuild, StatBlock};
use gw2_optimizer::gamedb::GameDb;
use crate::ui::chat_bar::ChatBarState;
use crate::ui::comparison::ComparisonState;

static STATE: Mutex<Option<AddonState>> = Mutex::new(None);

/// A cancellation token that background threads check to know when to stop.
/// Backed by `Arc<AtomicBool>` — cloning is cheap and each thread gets its own Arc ref.
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Signal all threads holding a clone of this token to stop.
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

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
    /// Cancellation token — cloned into every background thread.
    /// Cancelled on addon unload so threads exit early.
    pub cancel_token: CancellationToken,
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
    // Template selection
    pub build_tabs: Vec<gw2_api::models::BuildTab>,
    pub equipment_tabs: Vec<gw2_api::models::EquipmentTab>,
    pub selected_build_tab: Option<usize>,
    pub selected_equipment_tab: Option<usize>,
    pub build_chat_code: Option<String>,
    // Left menu
    pub active_tab: MainTab,
    // Comparison view
    pub comparison: ComparisonState,
    // Chat bar
    pub chat: ChatBarState,
    // GameDb (loaded once on main screen entry)
    pub game_db: Option<GameDb>,
    pub game_db_loading: bool,
    // Optimization state
    pub optimizing: bool,
    pub optimize_stage: String,
    /// Aggression level slider index (0=FullDefense, 1=Defensive, 2=Balanced, 3=Aggressive, 4=FullOffense).
    /// Default: 3 (Aggressive, matches PvE default). Adjusted when game mode changes.
    pub aggression_index: i32,
    // Save/Load
    pub saved_builds: Vec<SavedBuild>,
    pub saved_builds_loaded: bool,
    pub save_name_input: String,
    pub save_status: Option<String>,
    // Settings
    pub confirm_reset: bool,
    // UX feedback
    /// Frame counter for auto-dismissing save status messages (~180 frames ≈ 3s at 60fps).
    pub save_status_frames: u32,
    /// Index of the saved build pending delete confirmation (None = no dialog).
    pub confirm_delete: Option<usize>,
    /// Frame counter while chat is in "waiting" state; used for timeout recovery.
    pub chat_wait_frames: u32,
    /// Frame counter for "Copied!" tooltip feedback.
    pub copy_feedback_frames: u32,
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
    STATE.lock().unwrap_or_else(|e| {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            "State mutex was poisoned, recovering",
        );
        e.into_inner()
    })
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

    let mut main = MainState::default();
    main.aggression_index = 3; // Aggressive (PvE default)
    *lock_state() = Some(AddonState {
        window_visible: false,
        config,
        config_path,
        addon_dir,
        screen,
        setup,
        main,
        cancel_token: CancellationToken::new(),
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

/// Clear state on addon unload.
/// Cancels the token first so background threads exit early,
/// then drops the state to release all resources.
pub fn clear() {
    let mut guard = lock_state();
    if let Some(ref state) = *guard {
        state.cancel_token.cancel();
    }
    *guard = None;
}

/// Access state for reading/writing in UI code.
pub fn with_state<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut AddonState) -> R,
{
    lock_state().as_mut().map(f)
}
