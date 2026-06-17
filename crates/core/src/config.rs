//! Application configuration — API keys and user preferences.
//! Stored as JSON in the Nexus addon directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Which LLM provider is active.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmProvider {
    #[default]
    Gemini,
    OpenAI,
    Anthropic,
    /// OpenRouter — OpenAI-compatible API at https://openrouter.ai, gateway
    /// to hundreds of hosted models (Anthropic, OpenAI, Google, Mistral, etc.)
    /// with a single API key. Uses Chat Completions + tools format.
    OpenRouter,
}

impl LlmProvider {
    pub const ALL: [LlmProvider; 4] = [
        LlmProvider::Gemini,
        LlmProvider::OpenAI,
        LlmProvider::Anthropic,
        LlmProvider::OpenRouter,
    ];

    pub fn label(&self) -> &str {
        match self {
            LlmProvider::Gemini => "Google Gemini",
            LlmProvider::OpenAI => "OpenAI",
            LlmProvider::Anthropic => "Anthropic (Claude)",
            LlmProvider::OpenRouter => "OpenRouter",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub gw2_api_key: Option<String>,
    pub cache_build_number: Option<u32>,

    // ─── AI Provider Configuration ───
    /// Active LLM provider. Defaults to Gemini for backward compat.
    #[serde(default)]
    pub active_provider: LlmProvider,

    /// Gemini API key (Google AI Studio).
    pub gemini_api_key: Option<String>,
    /// Gemini model ID (e.g. "gemini-2.5-flash").
    #[serde(default)]
    pub gemini_model: Option<String>,

    /// OpenAI API key.
    #[serde(default)]
    pub openai_api_key: Option<String>,
    /// OpenAI model ID (e.g. "gpt-4o").
    #[serde(default)]
    pub openai_model: Option<String>,

    /// Anthropic API key.
    #[serde(default)]
    pub anthropic_api_key: Option<String>,
    /// Anthropic model ID (e.g. "claude-sonnet-4-6").
    #[serde(default)]
    pub anthropic_model: Option<String>,

    /// OpenRouter API key (https://openrouter.ai).
    #[serde(default)]
    pub openrouter_api_key: Option<String>,
    /// OpenRouter model ID (e.g. "anthropic/claude-sonnet-4-5").
    #[serde(default)]
    pub openrouter_model: Option<String>,

    // ─── UI Preferences ───
    /// Window opacity (0.0–1.0). Default 1.0.
    #[serde(default = "default_opacity")]
    pub window_opacity: f32,
    /// Font scale multiplier. Default 1.0.
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,

    // ─── Layout Tuning ───
    /// Left panel width in pixels. Default 255.
    #[serde(default = "default_left_panel_width")]
    pub left_panel_width: f32,
    /// Inner padding for left panel (px from edge). Default 6.
    #[serde(default = "default_panel_padding")]
    pub panel_padding: f32,
    /// Vertical spacing between sections (px). Default 4.
    #[serde(default = "default_section_spacing")]
    pub section_spacing: f32,
    /// Content area left indent (px). Default 4.
    #[serde(default = "default_content_indent")]
    pub content_indent: f32,

    // ─── Optimization Defaults ───
    /// Default game mode for new optimizations.
    #[serde(default)]
    pub default_game_mode: Option<String>,

    // ─── Cache & Data ───
    /// Auto-refresh game data cache on startup.
    #[serde(default)]
    pub auto_refresh_cache: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            gw2_api_key: None,
            cache_build_number: None,
            active_provider: LlmProvider::default(),
            gemini_api_key: None,
            gemini_model: None,
            openai_api_key: None,
            openai_model: None,
            anthropic_api_key: None,
            anthropic_model: None,
            openrouter_api_key: None,
            openrouter_model: None,
            window_opacity: 1.0,
            font_scale: 1.0,
            left_panel_width: 255.0,
            panel_padding: 6.0,
            section_spacing: 4.0,
            content_indent: 4.0,
            default_game_mode: None,
            auto_refresh_cache: false,
        }
    }
}

macro_rules! default_f32 {
    ($name:ident = $value:expr) => {
        fn $name() -> f32 {
            $value
        }
    };
}

default_f32!(default_opacity = 1.0);
default_f32!(default_font_scale = 1.0);
default_f32!(default_left_panel_width = 255.0);
default_f32!(default_panel_padding = 6.0);
default_f32!(default_section_spacing = 4.0);
default_f32!(default_content_indent = 4.0);

/// Known Gemini models — fallback shown when list_models() API call fails.
/// The Settings tab populates this list dynamically from the API at runtime.
pub const GEMINI_MODELS: &[(&str, &str)] = &[
    ("gemini-2.5-flash", "Gemini 2.5 Flash (fast, free tier)"),
    (
        "gemini-3-pro-preview",
        "Gemini 3 Pro Preview (advanced reasoning)",
    ),
    ("gemini-3.1-pro-preview", "Gemini 3.1 Pro Preview (latest)"),
];

pub const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";

/// Known OpenAI models available for selection.
pub const OPENAI_MODELS: &[(&str, &str)] = &[
    ("gpt-4o", "GPT-4o (multimodal, fast)"),
    ("gpt-4o-mini", "GPT-4o Mini (cheaper, fast)"),
    ("o3-mini", "o3-mini (reasoning)"),
];

pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";

/// Known Anthropic models available for selection.
pub const ANTHROPIC_MODELS: &[(&str, &str)] = &[
    ("claude-sonnet-4-6", "Claude Sonnet 4.6 (balanced)"),
    (
        "claude-haiku-4-5-20251001",
        "Claude Haiku 4.5 (fast, cheap)",
    ),
];

/// Known OpenRouter models — fallback shown when list_models() API call
/// fails. The Settings tab populates this list dynamically from the
/// OpenRouter `/models` endpoint at runtime.
pub const OPENROUTER_MODELS: &[(&str, &str)] = &[
    (
        "anthropic/claude-sonnet-4-5",
        "Claude Sonnet 4.5 (Anthropic via OR)",
    ),
    ("openai/gpt-4o-mini", "GPT-4o Mini (OpenAI via OR)"),
    (
        "google/gemini-2.5-flash",
        "Gemini 2.5 Flash (Google via OR)",
    ),
];

pub const DEFAULT_OPENROUTER_MODEL: &str = "anthropic/claude-sonnet-4-5";

pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-6";

impl AppConfig {
    pub fn gemini_model_id(&self) -> &str {
        self.gemini_model.as_deref().unwrap_or(DEFAULT_GEMINI_MODEL)
    }

    pub fn openai_model_id(&self) -> &str {
        self.openai_model.as_deref().unwrap_or(DEFAULT_OPENAI_MODEL)
    }

    pub fn anthropic_model_id(&self) -> &str {
        self.anthropic_model
            .as_deref()
            .unwrap_or(DEFAULT_ANTHROPIC_MODEL)
    }

    pub fn openrouter_model_id(&self) -> &str {
        self.openrouter_model
            .as_deref()
            .unwrap_or(DEFAULT_OPENROUTER_MODEL)
    }

    /// Get the model ID for the currently active provider.
    pub fn active_model_id(&self) -> &str {
        match self.active_provider {
            LlmProvider::Gemini => self.gemini_model_id(),
            LlmProvider::OpenAI => self.openai_model_id(),
            LlmProvider::Anthropic => self.anthropic_model_id(),
            LlmProvider::OpenRouter => self.openrouter_model_id(),
        }
    }

    /// Get the API key for the currently active provider.
    pub fn active_api_key(&self) -> Option<&str> {
        match self.active_provider {
            LlmProvider::Gemini => self.gemini_api_key.as_deref(),
            LlmProvider::OpenAI => self.openai_api_key.as_deref(),
            LlmProvider::Anthropic => self.anthropic_api_key.as_deref(),
            LlmProvider::OpenRouter => self.openrouter_api_key.as_deref(),
        }
    }

    /// Load config from disk. Returns `(config, error_message)` where
    /// `error_message` is Some if the file existed but could not be parsed.
    /// Callers should surface parse errors to the user — settings were reset.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Err(_) => (Self::default(), None),
            Ok(s) => match serde_json::from_str::<Self>(&s) {
                Ok(cfg) => (cfg, None),
                Err(e) => {
                    let msg = format!(
                        "config.json could not be parsed ({}). All settings reset to defaults.",
                        e
                    );
                    (Self::default(), Some(msg))
                }
            },
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        let tmp_path = path.with_extension("tmp");
        // Crash-safe: write to .tmp then atomic rename. Clean up the orphan
        // .tmp on either failure so it doesn't accumulate after repeated
        // failed saves (e.g. disk full, antivirus interruption).
        if let Err(e) = std::fs::write(&tmp_path, &json) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }
        if let Err(e) = std::fs::rename(&tmp_path, path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }
        Ok(())
    }

    pub fn has_gw2_key(&self) -> bool {
        self.gw2_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_gemini_key(&self) -> bool {
        self.gemini_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_openai_key(&self) -> bool {
        self.openai_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_anthropic_key(&self) -> bool {
        self.anthropic_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty())
    }

    pub fn has_openrouter_key(&self) -> bool {
        self.openrouter_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty())
    }

    /// Whether the active provider has a valid API key.
    pub fn has_active_llm_key(&self) -> bool {
        match self.active_provider {
            LlmProvider::Gemini => self.has_gemini_key(),
            LlmProvider::OpenAI => self.has_openai_key(),
            LlmProvider::Anthropic => self.has_anthropic_key(),
            LlmProvider::OpenRouter => self.has_openrouter_key(),
        }
    }

    /// Setup is complete when GW2 key + any LLM key + cache are all present.
    pub fn is_setup_complete(&self) -> bool {
        self.has_gw2_key() && self.has_active_llm_key() && self.cache_build_number.is_some()
    }

    pub fn config_path(addon_dir: &Path) -> PathBuf {
        addon_dir.join("config.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert!(!config.is_setup_complete());
        assert!(!config.has_gw2_key());
        assert!(!config.has_gemini_key());
        assert_eq!(config.active_provider, LlmProvider::Gemini);
        assert_eq!(config.window_opacity, 1.0);
        assert_eq!(config.font_scale, 1.0);
    }

    #[test]
    fn test_save_and_load() {
        let dir = env::temp_dir().join(format!("gw2_config_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let config = AppConfig {
            gw2_api_key: Some("test-key-123".into()),
            gemini_api_key: Some("gemini-key-456".into()),
            cache_build_number: Some(12345),
            ..Default::default()
        };
        config.save(&path).unwrap();

        let (loaded, err) = AppConfig::load(&path);
        assert!(err.is_none(), "unexpected parse error: {:?}", err);
        assert_eq!(loaded.gw2_api_key.as_deref(), Some("test-key-123"));
        assert!(loaded.is_setup_complete());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_backward_compat_old_config() {
        // Simulate an old config.json that only has the original 3 fields
        let json = r#"{
            "gw2_api_key": "old-key",
            "gemini_api_key": "old-gemini-key",
            "cache_build_number": 12345
        }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.active_provider, LlmProvider::Gemini);
        assert!(config.openai_api_key.is_none());
        assert!(config.anthropic_api_key.is_none());
        assert_eq!(config.window_opacity, 1.0);
        assert_eq!(config.font_scale, 1.0);
        assert!(!config.auto_refresh_cache);
        assert!(config.is_setup_complete());
    }

    #[test]
    fn test_empty_json_round_trips_to_defaults() {
        // Regression guard for the `default_f32!` macro: every serde default
        // must still yield the AppConfig::default() value when loading `{}`.
        let config: AppConfig = serde_json::from_str("{}").unwrap();
        let defaults = AppConfig::default();
        assert_eq!(config.window_opacity, defaults.window_opacity);
        assert_eq!(config.font_scale, defaults.font_scale);
        assert_eq!(config.left_panel_width, defaults.left_panel_width);
        assert_eq!(config.panel_padding, defaults.panel_padding);
        assert_eq!(config.section_spacing, defaults.section_spacing);
        assert_eq!(config.content_indent, defaults.content_indent);
    }

    #[test]
    fn test_openai_provider_setup_complete() {
        let config = AppConfig {
            gw2_api_key: Some("gw2-key".into()),
            active_provider: LlmProvider::OpenAI,
            openai_api_key: Some("sk-test123".into()),
            cache_build_number: Some(12345),
            ..Default::default()
        };
        assert!(config.is_setup_complete());
        assert_eq!(config.active_model_id(), "gpt-4o");
    }

    #[test]
    fn test_anthropic_provider_setup_complete() {
        let config = AppConfig {
            gw2_api_key: Some("gw2-key".into()),
            active_provider: LlmProvider::Anthropic,
            anthropic_api_key: Some("sk-ant-test123".into()),
            cache_build_number: Some(12345),
            ..Default::default()
        };
        assert!(config.is_setup_complete());
        assert_eq!(config.active_model_id(), "claude-sonnet-4-6");
    }

    #[test]
    fn test_provider_mismatch_not_complete() {
        // Has a Gemini key but active provider is OpenAI (no OpenAI key)
        let config = AppConfig {
            gw2_api_key: Some("gw2-key".into()),
            active_provider: LlmProvider::OpenAI,
            gemini_api_key: Some("gemini-key".into()),
            cache_build_number: Some(12345),
            ..Default::default()
        };
        assert!(!config.is_setup_complete());
    }

    #[test]
    fn test_load_parse_error_resets_to_defaults() {
        // A config file that exists but contains invalid JSON must surface
        // the parse error and fall back to defaults — this is the
        // user-visible "settings reset" path and callers rely on the
        // Some(msg) signal to tell the user their settings were dropped.
        let dir = env::temp_dir().join(format!("gw2_config_parse_err_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "{ not valid json").unwrap();

        let (loaded, err) = AppConfig::load(&path);
        assert!(err.is_some(), "expected Some(parse-error message)");
        assert!(
            err.as_deref().unwrap().contains("could not be parsed"),
            "error should mention parse failure, got: {:?}",
            err,
        );
        // Must be the exact default — no partial/lenient recovery.
        let defaults = AppConfig::default();
        assert_eq!(loaded.active_provider, defaults.active_provider);
        assert!(loaded.gw2_api_key.is_none());
        assert!(loaded.gemini_api_key.is_none());
        assert_eq!(loaded.window_opacity, defaults.window_opacity);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_active_routing_matches_provider() {
        // active_api_key / active_model_id must route to the field matching
        // active_provider, not to whichever key happens to be set. Populate
        // all three provider slots with distinct values and flip the active
        // provider across the enum.
        let mut config = AppConfig {
            gemini_api_key: Some("gemini-key".into()),
            openai_api_key: Some("openai-key".into()),
            anthropic_api_key: Some("anthropic-key".into()),
            gemini_model: Some("gemini-custom".into()),
            openai_model: Some("openai-custom".into()),
            anthropic_model: Some("anthropic-custom".into()),
            ..Default::default()
        };

        config.active_provider = LlmProvider::Gemini;
        assert_eq!(config.active_api_key(), Some("gemini-key"));
        assert_eq!(config.active_model_id(), "gemini-custom");

        config.active_provider = LlmProvider::OpenAI;
        assert_eq!(config.active_api_key(), Some("openai-key"));
        assert_eq!(config.active_model_id(), "openai-custom");

        config.active_provider = LlmProvider::Anthropic;
        assert_eq!(config.active_api_key(), Some("anthropic-key"));
        assert_eq!(config.active_model_id(), "anthropic-custom");

        // Empty slot → None key, default model id for the provider.
        let empty = AppConfig {
            active_provider: LlmProvider::Anthropic,
            ..Default::default()
        };
        assert_eq!(empty.active_api_key(), None);
        assert_eq!(empty.active_model_id(), DEFAULT_ANTHROPIC_MODEL);
    }
}
