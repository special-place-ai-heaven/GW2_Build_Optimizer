//! Provider-neutral LLM abstraction layer.
//! Defines the `LlmClient` trait that all AI providers implement,
//! plus shared types (`ToolDefinition`, `LlmError`) used across providers.

pub mod anthropic;
pub mod gemini;
pub mod openai;
pub(crate) mod retry;
pub mod tools;
pub(crate) mod trim;

use serde_json::Value;

/// Model info returned by a provider's model listing API.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model ID used in API calls (e.g. "gpt-4o", "gemini-2.5-flash", "claude-sonnet-4-6").
    pub id: String,
    /// Human-readable display name (e.g. "GPT-4o", "Gemini 2.5 Flash", "Claude Sonnet 4.6").
    pub display_name: String,
}

/// Result of a detailed key validation with user-friendly messages.
#[derive(Debug, Clone)]
pub struct KeyValidationResult {
    /// Whether the key is structurally valid (authentication passed).
    pub valid: bool,
    /// User-friendly status message (e.g. "Key validated successfully!").
    pub message: String,
    /// Optional warning about billing/quota (key valid but account has issues).
    pub warning: Option<String>,
}

/// Provider-neutral error type for all LLM operations.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("Invalid API key")]
    InvalidKey,
    #[error("Rate limited — try again later")]
    RateLimited,
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("LLM unavailable: {0}")]
    Unavailable(String),
}

/// Provider-neutral tool/function definition.
/// Each provider translates this to its own wire format internally.
/// Uses JSON Schema for parameters (common to Gemini, OpenAI, and Anthropic).
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the function parameters.
    pub parameters: Value,
}

/// A tool call returned by the LLM (provider has already parsed its native format).
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Provider-specific call ID (OpenAI: tool_call_id, Anthropic: tool_use_id, Gemini: none).
    pub id: Option<String>,
    pub name: String,
    pub arguments: Value,
}

/// The provider-neutral LLM client trait.
///
/// Implementors handle all provider-specific concerns internally:
/// - API authentication and endpoint URLs
/// - Request/response format translation
/// - Rate limiting and quota tracking
/// - Tool call wire format (Gemini functionDeclarations, OpenAI functions, Anthropic tools)
///
/// All methods are `&self` — clients manage their own internal mutability via `Mutex`.
/// `Send + Sync` required because clients are shared across background threads.
pub trait LlmClient: Send + Sync {
    /// Human-readable provider name (e.g. "Gemini", "OpenAI", "Anthropic").
    fn provider_name(&self) -> &str;

    /// Validate the API key without consuming quota.
    fn validate_key(&self) -> Result<(), LlmError>;

    /// Validate key with detailed, user-friendly result.
    /// Default implementation wraps `validate_key()` with generic messages.
    /// Providers should override for billing/quota-specific feedback.
    fn validate_key_detailed(&self) -> KeyValidationResult {
        match self.validate_key() {
            Ok(()) => KeyValidationResult {
                valid: true,
                message: format!("{} key validated successfully!", self.provider_name()),
                warning: None,
            },
            Err(LlmError::InvalidKey) => KeyValidationResult {
                valid: false,
                message: format!(
                    "Invalid {} API key. Check that you copied the full key.",
                    self.provider_name()
                ),
                warning: None,
            },
            Err(LlmError::RateLimited) => KeyValidationResult {
                valid: true,
                message: format!("{} key is valid.", self.provider_name()),
                warning: Some("Currently rate-limited. Try again shortly.".into()),
            },
            Err(LlmError::Http(ref msg)) => KeyValidationResult {
                valid: false,
                message: format!(
                    "Cannot connect to {} API. Check your internet connection.",
                    self.provider_name()
                ),
                warning: Some(msg.clone()),
            },
            Err(LlmError::Api {
                status,
                ref message,
            }) => KeyValidationResult {
                valid: false,
                message: format!(
                    "{} API returned error (HTTP {}).",
                    self.provider_name(),
                    status
                ),
                warning: Some(message.clone()),
            },
            Err(e) => KeyValidationResult {
                valid: false,
                message: format!("{} validation failed: {}", self.provider_name(), e),
                warning: None,
            },
        }
    }

    /// Simple text generation (no caching, no tools).
    fn generate(&self, prompt: &str) -> Result<String, LlmError>;

    /// Text generation with response caching (same prompt within TTL returns cached result).
    fn generate_cached(&self, prompt: &str) -> Result<String, LlmError>;

    /// Multi-turn generation with tool/function calling.
    ///
    /// The LLM can call tools to query game data and calculations.
    /// `execute_tool` is called for each tool invocation with (name, arguments) and returns a result.
    /// `on_progress` is called after each tool-calling round: (turn, max_turns, tool_names_called).
    /// Returns the final text response after all tool calls are resolved.
    fn generate_with_tools_progress(
        &self,
        prompt: &str,
        tools: &[ToolDefinition],
        execute_tool: &mut dyn FnMut(&str, &Value) -> Value,
        max_turns: usize,
        on_progress: &mut dyn FnMut(usize, usize, &[String]),
    ) -> Result<String, LlmError>;

    /// List available models from the provider's API.
    /// Returns model IDs and display names for UI dropdowns.
    /// Falls back to an empty list on error — callers should use hardcoded defaults.
    fn list_models(&self) -> Result<Vec<ModelInfo>, LlmError>;

    /// Remaining daily API quota (requests or tokens, provider-specific).
    fn remaining_quota(&self) -> u32;

    /// Clear the response cache.
    fn clear_cache(&self);
}

/// Convenience: generate with tools but no progress callback.
pub fn generate_with_tools(
    client: &dyn LlmClient,
    prompt: &str,
    tools: &[ToolDefinition],
    execute_tool: &mut dyn FnMut(&str, &Value) -> Value,
    max_turns: usize,
) -> Result<String, LlmError> {
    client.generate_with_tools_progress(prompt, tools, execute_tool, max_turns, &mut |_, _, _| {})
}

/// Case-insensitive check whether an API error body mentions a billing,
/// quota, or credit-balance issue. Used by `validate_key_detailed` overrides
/// to distinguish "key is valid but account has no credits" from "key is
/// invalid". Includes language-neutral Google API status codes so Gemini's
/// non-English responses still match.
pub(crate) fn has_billing_keyword(message: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "billing",
        "quota",
        "exceeded",
        "payment",
        "credit",
        "insufficient",
        // Google API canonical status codes (stable across locales).
        "resource_exhausted",
        "failed_precondition",
    ];
    let lower = message.to_lowercase();
    KEYWORDS.iter().any(|kw| lower.contains(kw))
}

#[cfg(test)]
mod billing_tests {
    use super::has_billing_keyword;

    #[test]
    fn matches_english_keywords_case_insensitively() {
        assert!(has_billing_keyword("Your billing account is suspended"));
        assert!(has_billing_keyword("QUOTA exceeded for this project"));
        assert!(has_billing_keyword("Credit balance is too low"));
        assert!(has_billing_keyword("payment method required"));
        assert!(has_billing_keyword("insufficient funds"));
    }

    #[test]
    fn matches_google_status_codes() {
        assert!(has_billing_keyword(
            r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED"}}"#
        ));
        assert!(has_billing_keyword(
            r#"{"error":{"status":"FAILED_PRECONDITION","message":"..."}}"#
        ));
    }

    #[test]
    fn does_not_match_generic_errors() {
        assert!(!has_billing_keyword("Bad request: missing required field"));
        assert!(!has_billing_keyword("Internal server error"));
        assert!(!has_billing_keyword(""));
    }
}


/// Create an LLM client based on the current config.
/// Dispatches to the correct provider and configures persistence.
pub fn create_client(
    config: &gw2_core::config::AppConfig,
    addon_dir: &std::path::Path,
) -> Result<Box<dyn LlmClient>, LlmError> {
    use gw2_core::config::LlmProvider;

    match config.active_provider {
        LlmProvider::Gemini => {
            let key = config
                .gemini_api_key
                .as_deref()
                .ok_or_else(|| LlmError::Unavailable("No Gemini API key configured".into()))?;
            let model = config.gemini_model_id();
            let usage_path = addon_dir.join("gemini_usage.json");
            let client = gemini::GeminiLlmClient::with_persistence(key, model, usage_path)?;
            Ok(Box::new(client))
        }
        LlmProvider::OpenAI => {
            let key = config
                .openai_api_key
                .as_deref()
                .ok_or_else(|| LlmError::Unavailable("No OpenAI API key configured".into()))?;
            let model = config.openai_model_id();
            let usage_path = addon_dir.join("openai_usage.json");
            let client = openai::OpenAiClient::with_persistence(key, model, usage_path)?;
            Ok(Box::new(client))
        }
        LlmProvider::Anthropic => {
            let key = config
                .anthropic_api_key
                .as_deref()
                .ok_or_else(|| LlmError::Unavailable("No Anthropic API key configured".into()))?;
            let model = config.anthropic_model_id();
            let usage_path = addon_dir.join("anthropic_usage.json");
            let client = anthropic::AnthropicClient::with_persistence(key, model, usage_path)?;
            Ok(Box::new(client))
        }
    }
}
