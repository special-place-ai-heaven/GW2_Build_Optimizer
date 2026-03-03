use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gw2_core::config::AppConfig;
use gw2_core::types::{BuildLocks, GameMode, ResolvedBuild, SavedBuild, StatBlock};
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::scoring::OptimizationWeights;
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
    /// Progress text for game data refresh (separate from optimize_stage to avoid clobbering).
    pub game_refresh_stage: String,
    // Optimization state
    pub optimizing: bool,
    pub optimize_stage: String,
    /// 5-axis optimization weights (Power, Disable, Condition, Heal, Sustain).
    /// Drives gear search, trait selection, and build scoring.
    pub weights: OptimizationWeights,
    /// Which radar chart axis is being dragged (None = no drag).
    pub radar_dragging: Option<usize>,
    // Save/Load
    pub saved_builds: Vec<SavedBuild>,
    pub saved_builds_loaded: bool,
    pub save_name_input: String,
    pub save_status: Option<String>,
    // Settings
    pub confirm_reset: bool,
    /// Input buffer for new API key entry in Settings (current provider).
    pub settings_key_input: String,
    /// Status of the last key validation attempt in Settings.
    pub settings_key_status: Option<String>,
    /// Whether the key validation passed (true = green, false = orange/red).
    pub settings_key_valid: bool,
    /// Optional warning (e.g. billing/quota) shown below the status message.
    pub settings_key_warning: Option<String>,
    /// Whether a key validation is in progress in Settings.
    pub settings_key_validating: bool,
    // UX feedback
    /// Frame counter for auto-dismissing save status messages (~180 frames ≈ 3s at 60fps).
    pub save_status_frames: u32,
    /// Index of the saved build pending delete confirmation (None = no dialog).
    pub confirm_delete: Option<usize>,
    /// Frame counter while chat is in "waiting" state; used for timeout recovery.
    pub chat_wait_frames: u32,
    /// Frame counter for "Copied!" tooltip feedback.
    pub copy_feedback_frames: u32,
    // Dynamic model list
    /// Models fetched from the active provider's API: (id, display_name).
    pub available_models: Vec<(String, String)>,
    /// Whether a model list fetch is in progress.
    pub models_loading: bool,
    /// Error from the last model list fetch.
    pub models_error: Option<String>,
    // API health
    pub api_status: ApiStatus,
    /// Frame counter for periodic API health checks (~3600 frames ≈ 60s at 60fps).
    pub api_status_frames: u32,
    /// Whether a health check is currently in flight.
    pub api_health_checking: bool,
    // Spec & Trait Locks
    /// Granular lock constraints for optimizer (which specs/traits to preserve).
    pub build_locks: BuildLocks,
    /// Whether the locks panel is expanded in the left menu.
    pub locks_panel_expanded: bool,
}

/// GW2 API health status, checked periodically via `/v2/build`.
#[derive(Default, Debug, Clone, PartialEq)]
pub enum ApiStatus {
    #[default]
    Unknown,
    Online,
    Degraded,
    Offline,
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
    LlmApiKey,
    DataDownload,
    Complete,
}

#[derive(Default)]
pub struct SetupState {
    // GW2 key input
    pub gw2_key_input: String,
    pub gw2_key_status: KeyStatus,
    pub gw2_key_scopes: Vec<(String, bool)>, // (scope_name, present)
    // AI provider key input (provider-agnostic)
    pub llm_key_input: String,
    pub llm_key_status: KeyStatus,
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
    let (config, config_err) = AppConfig::load(&config_path);

    let screen = if config.is_setup_complete() {
        Screen::Main
    } else if config.has_gw2_key() && config.has_active_llm_key() {
        // Keys present but cache missing — go to download
        Screen::Setup(SetupStep::DataDownload)
    } else if config.has_gw2_key() {
        Screen::Setup(SetupStep::LlmApiKey)
    } else {
        Screen::Setup(SetupStep::Gw2ApiKey)
    };

    let mut setup = SetupState::default();
    if let Some(ref key) = config.gw2_api_key {
        setup.gw2_key_input = key.clone();
        setup.gw2_key_status = KeyStatus::Valid;
    }
    if let Some(key) = config.active_api_key() {
        setup.llm_key_input = key.to_string();
        setup.llm_key_status = KeyStatus::Valid;
    }

    let mut main = MainState::default();
    // Surface any config parse error in the UI status bar
    main.error = config_err;
    // Apply saved default game mode from config
    let default_mode_label = config.default_game_mode.as_deref().unwrap_or("PvE");
    main.game_mode = match default_mode_label {
        "PvP" => gw2_core::types::GameMode::PvP,
        "WvW" => gw2_core::types::GameMode::WvW,
        _ => gw2_core::types::GameMode::PvE,
    };
    main.weights = OptimizationWeights::default_for_mode(main.game_mode.label());
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

#[cfg(test)]
mod tests {
    use super::*;
    use gw2_core::config::AppConfig;
    use gw2_optimizer::scoring::OptimizationWeights;

    /// Reset the global STATE to None for test isolation.
    /// Call as the first line of every test that calls init(), clear(), or with_state().
    /// Required: tests share a global static Mutex — without this, a prior test's
    /// init() leaves state set, causing the next test to start with stale data.
    fn reset_state() {
        *super::lock_state() = None;
    }

    /// Write `config` as config.json inside a per-test temp dir and return the dir.
    /// AppConfig::save() creates parent dirs automatically.
    fn config_in_tempdir(config: &AppConfig, label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("gw2_state_test_{}_{}", std::process::id(), label));
        let cfg_path = AppConfig::config_path(&dir);
        config.save(&cfg_path).unwrap();
        dir
    }

    // ── CancellationToken ─────────────────────────────────────────────────────

    #[test]
    fn test_cancel_token_new_is_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancel_token_cancel_sets_flag() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cancel_token_clone_sees_cancellation() {
        // Verifies that clones share the same Arc<AtomicBool> — threads cloning
        // the token at spawn time will see the cancellation signal from clear().
        let token = CancellationToken::new();
        let clone = token.clone();
        token.cancel();
        assert!(clone.is_cancelled());
    }

    // ── MainState::default() — initial loading-flag safety ────────────────────
    //
    // Risk: non-optimizer background threads in main_view.rs (lines 1501, 1584,
    // 2168, 2199, 2279, 2312, 3233) have no catch_unwind.  A panic in any of
    // those threads leaves these flags stuck `true` permanently.  This test
    // verifies the safe initial boundary: all flags must start false so any
    // stuck-flag regression is immediately visible as a state divergence.

    #[test]
    fn test_main_state_default_fields() {
        let main = MainState::default();
        assert!(main.characters.is_empty(), "characters must start empty");
        assert!(!main.optimizing,          "optimizing must start false");
        assert!(!main.characters_loading,  "characters_loading must start false (stuck-loading-flag risk)");
        assert!(!main.game_db_loading,     "game_db_loading must start false (stuck-loading-flag risk)");
        assert!(!main.build_loading,       "build_loading must start false (stuck-loading-flag risk)");
        assert_eq!(
            main.weights,
            OptimizationWeights::default(),
            "default weights should be preset_balanced; init() overrides to PvE default"
        );
        assert!(
            main.build_locks.specs.iter().all(|s| s.is_none()),
            "build_locks.specs must start as [None; 3]"
        );
        assert!(
            main.build_locks.trait_locks.is_empty(),
            "build_locks.trait_locks must start empty"
        );
    }

    // ── init() screen routing ─────────────────────────────────────────────────

    #[test]
    fn test_init_routes_to_gw2_key_when_no_keys() {
        reset_state();
        // No config.json in the dir → AppConfig::load returns default (no keys).
        let dir = std::env::temp_dir()
            .join(format!("gw2_state_test_{}_no_keys", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        let screen = with_state(|s| s.screen.clone()).unwrap();
        assert_eq!(screen, Screen::Setup(SetupStep::Gw2ApiKey));
        reset_state();
    }

    #[test]
    fn test_init_routes_to_llm_key_when_only_gw2_key() {
        reset_state();
        let config = AppConfig {
            gw2_api_key: Some("test-gw2-key".into()),
            ..Default::default()
        };
        let dir = config_in_tempdir(&config, "gw2_only");
        init(dir);
        let screen = with_state(|s| s.screen.clone()).unwrap();
        assert_eq!(screen, Screen::Setup(SetupStep::LlmApiKey));
        reset_state();
    }

    #[test]
    fn test_init_routes_to_data_download_when_keys_present_no_cache() {
        reset_state();
        let config = AppConfig {
            gw2_api_key: Some("test-gw2-key".into()),
            gemini_api_key: Some("test-gemini-key".into()),
            cache_build_number: None,
            ..Default::default()
        };
        let dir = config_in_tempdir(&config, "keys_no_cache");
        init(dir);
        let screen = with_state(|s| s.screen.clone()).unwrap();
        assert_eq!(screen, Screen::Setup(SetupStep::DataDownload));
        reset_state();
    }

    #[test]
    fn test_init_routes_to_main_when_setup_complete() {
        reset_state();
        let config = AppConfig {
            gw2_api_key: Some("test-gw2-key".into()),
            gemini_api_key: Some("test-gemini-key".into()),
            cache_build_number: Some(12345),
            ..Default::default()
        };
        let dir = config_in_tempdir(&config, "setup_complete");
        init(dir);
        let screen = with_state(|s| s.screen.clone()).unwrap();
        assert_eq!(screen, Screen::Main);
        reset_state();
    }

    // ── init() loading-flag safety ────────────────────────────────────────────
    // Complementary to test_main_state_default_fields: confirms init() itself
    // never sets any loading flag — they are set exclusively by background threads.
    // If this test fails, it means init() started a background op without a
    // matching clear path, introducing a permanent stuck-flag risk.

    #[test]
    fn test_init_loading_flags_start_false() {
        reset_state();
        let dir = std::env::temp_dir()
            .join(format!("gw2_state_test_{}_loading_flags", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        with_state(|s| {
            assert!(!s.main.optimizing,         "optimizing must be false after init");
            assert!(!s.main.characters_loading, "characters_loading must be false after init");
            assert!(!s.main.game_db_loading,    "game_db_loading must be false after init");
            assert!(!s.main.build_loading,      "build_loading must be false after init");
        });
        reset_state();
    }

    // ── with_state ────────────────────────────────────────────────────────────

    #[test]
    fn test_with_state_returns_none_when_uninitialized() {
        reset_state();
        let result = with_state(|_s| 42);
        assert!(result.is_none());
    }

    #[test]
    fn test_with_state_invokes_closure_when_initialized() {
        reset_state();
        let dir = std::env::temp_dir()
            .join(format!("gw2_state_test_{}_with_state_init", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        let result = with_state(|s| s.window_visible);
        assert_eq!(result, Some(false));
        reset_state();
    }

    // ── clear() ──────────────────────────────────────────────────────────────

    #[test]
    fn test_clear_cancels_token() {
        reset_state();
        let dir = std::env::temp_dir()
            .join(format!("gw2_state_test_{}_clear_cancel", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        let token = with_state(|s| s.cancel_token.clone()).unwrap();
        assert!(!token.is_cancelled(), "token must not be cancelled before clear");
        clear();
        assert!(token.is_cancelled(), "token clone must see cancellation after clear");
    }

    #[test]
    fn test_clear_drops_state() {
        reset_state();
        let dir = std::env::temp_dir()
            .join(format!("gw2_state_test_{}_clear_drops", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        assert!(with_state(|_s| ()).is_some(), "state must be Some after init");
        clear();
        assert!(with_state(|_s| ()).is_none(), "state must be None after clear");
    }
}
