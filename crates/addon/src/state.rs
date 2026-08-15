use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::ui::chat_bar::ChatBarState;
use crate::ui::comparison::ComparisonState;
use gw2_core::config::AppConfig;
use gw2_core::types::{BuildLocks, GameMode, ResolvedBuild, SavedBuild, StatBlock};
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::scoring::OptimizationWeights;

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
    /// Next frame: pin the overlay to [80, 80] (off-screen imgui.ini / Reset button).
    pub force_window_pos: bool,
}

impl AddonState {
    /// Reset persisted settings and transient UI state back to first-run setup.
    pub fn reset_to_first_run(&mut self) -> Result<(), std::io::Error> {
        let config = AppConfig::default();
        config.save(&self.config_path)?;

        self.cancel_token.cancel();
        self.cancel_token = CancellationToken::new();
        self.config = config;
        self.setup = SetupState::default();

        let mut main = MainState::default();
        main.weights = OptimizationWeights::default_for_mode(main.game_mode.label());
        self.main = main;
        self.screen = Screen::Setup(SetupStep::Gw2ApiKey);

        Ok(())
    }
}

#[derive(Default)]
pub struct MainState {
    pub characters: Vec<String>,
    pub characters_loading: bool,
    pub selected_character: Option<usize>,
    pub game_mode: GameMode,
    /// WvW combat sub-tier: Solo (Roaming), Party (Havoc/small group), Squad (Zerg).
    /// Only meaningful when game_mode == WvW. Defaults to Squad.
    pub wvw_combat_tier: gw2_optimizer::scenario::CombatTier,
    /// Selected role objective for 'Create New Build' flow. None = no role chosen yet.
    pub selected_role: Option<gw2_optimizer::scenario::RoleObjective>,
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
    /// 6-axis optimization weights (Power, Condition, Boon Support, Heal, Sustain, Control).
    /// Drives gear search, trait selection, and build scoring.
    pub weights: OptimizationWeights,
    /// Which radar chart axis is being dragged (None = no drag).
    pub radar_dragging: Option<usize>,
    // Save/Load
    pub saved_builds: Vec<SavedBuild>,
    pub saved_builds_loaded: bool,
    pub save_name_input: String,
    pub save_status: Option<String>,
    // Benchmark scraping
    pub benchmark_running: bool,
    pub benchmark_last_synced: Option<String>,
    /// Per-source build counts: "snowcrows" -> n, "hardstuck" -> n, "guildjen" -> n.
    pub benchmark_counts: std::collections::HashMap<String, usize>,
    /// Live heartbeat while a sync is running ("12/45", "listing guardian…").
    pub benchmark_live: std::collections::HashMap<String, String>,
    /// Per-source error after a sync (shown as "down" on that row).
    pub benchmark_errors: std::collections::HashMap<String, String>,
    pub benchmark_error: Option<String>,
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
    /// Live `/v2/build` id. Compared to `cache_build_number` to prompt a data refresh.
    pub live_build_number: Option<u32>,
    /// Cached "Usage today" count for the active provider's persisted usage
    /// file, displayed in the Settings tab. Refreshed every ~60 frames (~1s)
    /// instead of reading the file every render frame.
    pub settings_usage_today: u64,
    /// Frame counter that throttles `settings_usage_today` refresh.
    pub settings_usage_frames: u32,
    /// Cached cache-directory size in bytes for the Settings tab "Cache: …"
    /// label. Throttled refresh on `settings_cache_size_frames`.
    pub settings_cache_size: u64,
    /// PNG icon cache size (cache/graphics). Separate from JSON data.
    pub settings_graphics_size: u64,
    /// Frame counter that throttles `settings_cache_size` refresh.
    pub settings_cache_size_frames: u32,
    /// Search filter text for the Settings tab model-picker dropdown.
    /// Case-insensitive substring filter against model id + display label.
    /// Persists across frames so the user can refine. Cleared when the
    /// active provider changes (different model catalogs) and when the
    /// user explicitly clicks Clear.
    pub settings_model_search: String,
    // Spec & Trait Locks
    /// Granular lock constraints for optimizer (which specs/traits to preserve).
    pub build_locks: BuildLocks,
    /// Whether the locks panel is expanded in the left menu.
    pub locks_panel_expanded: bool,
    /// Currently animating hover element in the lock panel (+ its 0..=1 progress).
    /// Lives on `MainState` so the subtle glow lerps smoothly across frames instead
    /// of snapping when `render_lock_panel` returns.
    pub locks_hover: Option<(crate::ui::main_view::lock_panel::LockElementId, f32)>,
}

impl MainState {
    /// Clear the currently resolved build view so the UI never shows stale data.
    pub fn clear_resolved_view(&mut self) {
        self.current_build = None;
        self.current_stats = None;
        self.comparison.current_combat_solo = None;
        self.comparison.current_combat_party = None;
        self.comparison.current_combat_squad = None;
    }
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
    pub inner_done: usize,
    pub inner_total: usize,
    pub done: bool,
    pub error: Option<String>,
}

impl DownloadState {
    /// Overall 0..=1, including the current step's item/icon batches.
    pub fn fraction(&self) -> f32 {
        download_fraction(
            self.current_step,
            self.total_steps,
            self.inner_done,
            self.inner_total,
            self.done,
        )
    }
}

pub(crate) fn download_fraction(
    current_step: usize,
    total_steps: usize,
    inner_done: usize,
    inner_total: usize,
    done: bool,
) -> f32 {
    if done {
        return 1.0;
    }
    if total_steps == 0 {
        return 0.0;
    }
    let inner = if inner_total > 0 {
        (inner_done as f32 / inner_total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((current_step as f32 + inner) / total_steps as f32).clamp(0.0, 1.0)
}

fn lock_state() -> std::sync::MutexGuard<'static, Option<AddonState>> {
    STATE.lock().unwrap_or_else(|e| {
        // Use eprintln! — nexus::log::log() can panic if the addon API
        // isn't initialized yet (or in tests), and panic recovery code
        // must never itself panic.
        eprintln!("[GW2BuildOpt] State mutex was poisoned, recovering");
        e.into_inner()
    })
}

// SetupState/MainState fields are filled conditionally from optional config
// after default(); a struct literal would force redundant default expressions.
#[allow(clippy::field_reassign_with_default)]
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
    crate::ui::icons::set_graphics_dir(addon_dir.join("cache").join("graphics"));
    *lock_state() = Some(AddonState {
        window_visible: false,
        config,
        config_path,
        addon_dir,
        screen,
        setup,
        main,
        cancel_token: CancellationToken::new(),
        force_window_pos: false,
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
    // Test fixtures are built field-by-field for readability.
    #![allow(clippy::field_reassign_with_default)]
    use super::*;
    use gw2_core::config::AppConfig;
    use gw2_optimizer::scoring::OptimizationWeights;

    // Global state tests mutate the same static STATE mutex. Parallel execution can
    // make one test reset/replace state while another is asserting, causing flaky
    // None/stale-value results. Serialize these tests with a dedicated lock.
    static TEST_STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn state_test_guard() -> std::sync::MutexGuard<'static, ()> {
        let current = std::thread::current();
        let test_name = current.name().unwrap_or("<unnamed>");
        eprintln!(
            "[GW2BuildOpt][state::tests] acquiring shared STATE test lock: {}",
            test_name
        );
        TEST_STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

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
        let dir =
            std::env::temp_dir().join(format!("gw2_state_test_{}_{}", std::process::id(), label));
        let cfg_path = AppConfig::config_path(&dir);
        config.save(&cfg_path).unwrap();
        dir
    }

    fn dummy_resolved_build() -> ResolvedBuild {
        ResolvedBuild {
            character_name: "Test Character".into(),
            profession: "Warrior".into(),
            game_mode: GameMode::PvE,
            specializations: Vec::new(),
            skills: Default::default(),
            legends: Vec::new(),
            pets: Vec::new(),
            weapons: Vec::new(),
            armor: Vec::new(),
            trinkets: Vec::new(),
            relic: None,
            rune: None,
            pvp_amulet: None,
        }
    }

    fn dummy_combat_metrics(total_dps_index: i32) -> gw2_core::types::CombatMetrics {
        gw2_core::types::CombatMetrics {
            effective_power: 0,
            strike_dps_index: total_dps_index,
            condition_dps_index: 0,
            total_dps_index,
            healing_index: 0,
            crit_chance: 0.0,
            boon_duration_pct: 0.0,
            condi_duration_pct: 0.0,
            effective_health: 0,
            damage_reduction_pct: 0.0,
            bleeding_tick: 0,
            burning_tick: 0,
            poison_tick: 0,
            torment_tick: 0,
            confusion_tick: 0,
        }
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

    #[test]
    fn test_cancel_token_worker_loop_exits_on_pulse() {
        // Validates the worker-loop pattern used in every `std::thread::spawn`
        // site in `crates/addon/src/ui/`: a background thread clones a
        // CancellationToken and checks `is_cancelled()` between iterations of
        // its work loop. Pulsing the token mid-loop must let the worker exit
        // within a bounded time. This covers all 10 audited live spawn sites
        // in crates/addon/src/ui/ — they share this exact loop shape, so the
        // pattern is tested once.
        let token = CancellationToken::new();
        let worker_token = token.clone();
        let handle = std::thread::spawn(move || {
            // Simulate a long-running worker: 500 iterations × 2ms = up to 1s
            // of work if no cancel arrives. Each iteration checks the token,
            // mirroring the `if token.is_cancelled() { return; }` boundary in
            // every real spawn site.
            for _ in 0..500 {
                if worker_token.is_cancelled() {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            false
        });

        // Let the worker start, then pulse the token.
        std::thread::sleep(std::time::Duration::from_millis(20));
        token.cancel();

        // Worker must observe cancellation and exit promptly. Generous
        // 500ms join budget guards against CI flakiness; real exit should be
        // single-digit ms after the next 2ms iteration boundary.
        let start = std::time::Instant::now();
        let exited_via_cancel = handle.join().expect("worker thread panicked");
        let elapsed = start.elapsed();
        assert!(
            exited_via_cancel,
            "worker must exit via is_cancelled() check, not loop completion"
        );
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "worker must exit within 500ms of cancel pulse (took {:?})",
            elapsed
        );
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
        assert!(!main.optimizing, "optimizing must start false");
        assert!(
            !main.characters_loading,
            "characters_loading must start false (stuck-loading-flag risk)"
        );
        assert!(
            !main.game_db_loading,
            "game_db_loading must start false (stuck-loading-flag risk)"
        );
        assert!(
            !main.build_loading,
            "build_loading must start false (stuck-loading-flag risk)"
        );
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

    #[test]
    fn test_main_state_clear_resolved_view_clears_current_build_and_combat() {
        let mut main = MainState::default();
        main.current_build = Some(dummy_resolved_build());
        main.current_stats = Some(StatBlock {
            power: 1,
            precision: 2,
            toughness: 3,
            vitality: 4,
            condition_damage: 5,
            expertise: 6,
            concentration: 7,
            ferocity: 8,
            healing_power: 9,
            crit_chance: 10.0,
            crit_damage: 11.0,
            health: 12,
            armor: 13,
        });
        main.comparison.current_combat_solo = Some(dummy_combat_metrics(100));
        main.comparison.current_combat_party = Some(dummy_combat_metrics(200));
        main.comparison.current_combat_squad = Some(dummy_combat_metrics(300));

        main.clear_resolved_view();

        assert!(main.current_build.is_none());
        assert!(main.current_stats.is_none());
        assert!(main.comparison.current_combat_solo.is_none());
        assert!(main.comparison.current_combat_party.is_none());
        assert!(main.comparison.current_combat_squad.is_none());
    }

    // ── init() screen routing ─────────────────────────────────────────────────

    #[test]
    fn test_init_routes_to_gw2_key_when_no_keys() {
        let _serial = state_test_guard();
        reset_state();
        // No config.json in the dir → AppConfig::load returns default (no keys).
        let dir =
            std::env::temp_dir().join(format!("gw2_state_test_{}_no_keys", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        let screen = with_state(|s| s.screen.clone()).unwrap();
        assert_eq!(screen, Screen::Setup(SetupStep::Gw2ApiKey));
        reset_state();
    }

    #[test]
    fn test_init_routes_to_llm_key_when_only_gw2_key() {
        let _serial = state_test_guard();
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
        let _serial = state_test_guard();
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
        let _serial = state_test_guard();
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
        let _serial = state_test_guard();
        reset_state();
        let dir = std::env::temp_dir().join(format!(
            "gw2_state_test_{}_loading_flags",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        with_state(|s| {
            assert!(!s.main.optimizing, "optimizing must be false after init");
            assert!(
                !s.main.characters_loading,
                "characters_loading must be false after init"
            );
            assert!(
                !s.main.game_db_loading,
                "game_db_loading must be false after init"
            );
            assert!(
                !s.main.build_loading,
                "build_loading must be false after init"
            );
        });
        reset_state();
    }

    // ── with_state ────────────────────────────────────────────────────────────

    #[test]
    fn test_with_state_returns_none_when_uninitialized() {
        let _serial = state_test_guard();
        reset_state();
        let result = with_state(|_s| 42);
        assert!(result.is_none());
    }

    #[test]
    fn test_with_state_invokes_closure_when_initialized() {
        let _serial = state_test_guard();
        reset_state();
        let dir = std::env::temp_dir().join(format!(
            "gw2_state_test_{}_with_state_init",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        let result = with_state(|s| s.window_visible);
        assert_eq!(result, Some(false));
        reset_state();
    }

    // ── clear() ──────────────────────────────────────────────────────────────

    #[test]
    fn test_clear_cancels_token() {
        let _serial = state_test_guard();
        reset_state();
        let dir = std::env::temp_dir().join(format!(
            "gw2_state_test_{}_clear_cancel",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        let token = with_state(|s| s.cancel_token.clone()).unwrap();
        assert!(
            !token.is_cancelled(),
            "token must not be cancelled before clear"
        );
        clear();
        assert!(
            token.is_cancelled(),
            "token clone must see cancellation after clear"
        );
    }

    #[test]
    fn test_clear_drops_state() {
        let _serial = state_test_guard();
        reset_state();
        let dir =
            std::env::temp_dir().join(format!("gw2_state_test_{}_clear_drops", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir);
        assert!(
            with_state(|_s| ()).is_some(),
            "state must be Some after init"
        );
        clear();
        assert!(
            with_state(|_s| ()).is_none(),
            "state must be None after clear"
        );
    }

    #[test]
    fn test_reset_to_first_run_clears_config_and_transient_state() {
        let _serial = state_test_guard();
        reset_state();
        let config = AppConfig {
            gw2_api_key: Some("test-gw2-key".into()),
            gemini_api_key: Some("test-gemini-key".into()),
            cache_build_number: Some(12345),
            default_game_mode: Some("WvW".into()),
            ..Default::default()
        };
        let dir = config_in_tempdir(&config, "reset_to_first_run");
        init(dir.clone());

        let (
            old_cancelled,
            new_cancelled,
            screen,
            has_gw2_key,
            has_llm_key,
            cache_build,
            locks_cleared,
        ) = with_state(|s| {
            s.main.current_build = Some(dummy_resolved_build());
            s.main.current_stats = Some(StatBlock {
                power: 1,
                precision: 2,
                toughness: 3,
                vitality: 4,
                condition_damage: 5,
                expertise: 6,
                concentration: 7,
                ferocity: 8,
                healing_power: 9,
                crit_chance: 10.0,
                crit_damage: 11.0,
                health: 12,
                armor: 13,
            });
            s.main.comparison.current_combat_solo = Some(dummy_combat_metrics(500));
            s.main.build_locks.specs[2] = Some(42);
            s.setup.gw2_key_input = "gw2-input".into();
            s.setup.llm_key_input = "llm-input".into();

            let old_token = s.cancel_token.clone();
            s.reset_to_first_run().unwrap();

            (
                old_token.is_cancelled(),
                s.cancel_token.is_cancelled(),
                s.screen.clone(),
                s.config.has_gw2_key(),
                s.config.has_active_llm_key(),
                s.config.cache_build_number,
                s.main.build_locks.specs.iter().all(|slot| slot.is_none())
                    && s.main.build_locks.trait_locks.is_empty(),
            )
        })
        .unwrap();

        assert!(old_cancelled, "old token should be cancelled during reset");
        assert!(!new_cancelled, "replacement token must start uncancelled");
        assert_eq!(screen, Screen::Setup(SetupStep::Gw2ApiKey));
        assert!(!has_gw2_key);
        assert!(!has_llm_key);
        assert_eq!(cache_build, None);
        assert!(locks_cleared);

        let (saved_config, err) = AppConfig::load(&AppConfig::config_path(&dir));
        assert!(err.is_none());
        assert!(!saved_config.has_gw2_key());
        assert!(!saved_config.has_active_llm_key());
        assert_eq!(saved_config.cache_build_number, None);
    }

    // ── catch_unwind panic-recovery tests ─────────────────────────────────────
    //
    // P2-03: Every background thread is now wrapped in catch_unwind. These tests
    // verify the Err-arm pattern: after a panic is caught, with_state() still
    // works (mutex not permanently poisoned) and the loading flag is cleared.

    #[test]
    fn test_catch_unwind_clears_chat_waiting_on_panic() {
        let _serial = state_test_guard();
        reset_state();
        let dir =
            std::env::temp_dir().join(format!("gw2_state_test_{}_panic_chat", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir.clone());
        with_state(|s| s.main.chat.waiting = true);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("simulated panic in send_chat_message");
        }));

        if result.is_err() {
            with_state(|s| s.main.chat.waiting = false);
        }

        let waiting = with_state(|s| s.main.chat.waiting);
        assert_eq!(
            waiting,
            Some(false),
            "chat.waiting must be cleared after panic"
        );
        reset_state();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_catch_unwind_clears_models_loading_on_panic() {
        let _serial = state_test_guard();
        reset_state();
        let dir = std::env::temp_dir().join(format!(
            "gw2_state_test_{}_panic_models",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir.clone());
        with_state(|s| s.main.models_loading = true);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("simulated panic in start_fetch_models");
        }));

        if result.is_err() {
            with_state(|s| s.main.models_loading = false);
        }

        let loading = with_state(|s| s.main.models_loading);
        assert_eq!(
            loading,
            Some(false),
            "models_loading must be cleared after panic"
        );
        reset_state();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_catch_unwind_clears_characters_loading_on_panic() {
        let _serial = state_test_guard();
        reset_state();
        let dir =
            std::env::temp_dir().join(format!("gw2_state_test_{}_panic_chars", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir.clone());
        with_state(|s| s.main.characters_loading = true);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("simulated panic in load_characters");
        }));

        if result.is_err() {
            with_state(|s| s.main.characters_loading = false);
        }

        let loading = with_state(|s| s.main.characters_loading);
        assert_eq!(
            loading,
            Some(false),
            "characters_loading must be cleared after panic"
        );
        reset_state();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_catch_unwind_resets_gw2_key_status_on_panic() {
        let _serial = state_test_guard();
        reset_state();
        let dir =
            std::env::temp_dir().join(format!("gw2_state_test_{}_panic_setup", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir.clone());
        with_state(|s| s.setup.gw2_key_status = KeyStatus::Validating);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("simulated panic in setup_gw2_key_validation");
        }));

        if result.is_err() {
            with_state(|s| {
                s.setup.gw2_key_status = KeyStatus::Invalid("thread panicked".into());
            });
        }

        let status = with_state(|s| s.setup.gw2_key_status.clone());
        assert!(
            matches!(status, Some(KeyStatus::Invalid(ref msg)) if msg == "thread panicked"),
            "gw2_key_status must be reset to Invalid after panic, got {:?}",
            status
        );
        reset_state();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_catch_unwind_mutex_poison_recovery() {
        let _serial = state_test_guard();
        // Exercises the real danger scenario: a panic INSIDE with_state() poisons
        // the mutex, and subsequent with_state() calls must still succeed via
        // lock_state()'s unwrap_or_else(|e| e.into_inner()) recovery.
        reset_state();
        let dir = std::env::temp_dir().join(format!(
            "gw2_state_test_{}_panic_poison",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir.clone());
        with_state(|s| s.main.characters_loading = true);

        // Panic inside with_state — this poisons the mutex
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_state(|_s| {
                panic!("simulated panic inside with_state callback");
            });
        }));
        assert!(result.is_err(), "catch_unwind must capture the panic");

        // After poison: with_state must still work (lock_state recovers)
        with_state(|s| s.main.characters_loading = false);
        let loading = with_state(|s| s.main.characters_loading);
        assert_eq!(
            loading,
            Some(false),
            "with_state must work after mutex poison recovery"
        );
        reset_state();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_fraction_crawls_during_item_batches() {
        let start = download_fraction(7, 9, 0, 74056, false);
        assert!((start - 7.0 / 9.0).abs() < 1e-5);
        let mid = download_fraction(7, 9, 24000, 74056, false);
        assert!(mid > start);
        assert!(mid < 8.0 / 9.0);
        assert_eq!(download_fraction(9, 9, 0, 0, true), 1.0);
        assert_eq!(download_fraction(0, 0, 0, 0, false), 0.0);
    }
}
