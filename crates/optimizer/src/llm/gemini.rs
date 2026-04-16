//! Gemini provider — implements `LlmClient` by delegating to the existing `GeminiClient`.
//! This is a thin adapter: all HTTP, rate limiting, caching, and retry logic
//! stays in `crate::gemini::GeminiClient`. This module just bridges the trait.

use serde_json::Value;

use super::{KeyValidationResult, LlmClient, LlmError, ToolDefinition};
use crate::gemini::{self, GeminiClient as RawGeminiClient, GeminiError};

/// Convert Gemini-specific errors to provider-neutral `LlmError`.
impl From<GeminiError> for LlmError {
    fn from(e: GeminiError) -> Self {
        match e {
            GeminiError::Http(inner) => LlmError::Http(inner.to_string()),
            GeminiError::Api { status, message } => LlmError::Api { status, message },
            GeminiError::InvalidKey => LlmError::InvalidKey,
            GeminiError::RateLimited => LlmError::RateLimited,
            GeminiError::Parse(msg) => LlmError::Parse(msg),
            GeminiError::Unavailable(msg) => LlmError::Unavailable(msg),
        }
    }
}

/// Gemini LLM client implementing the provider-neutral trait.
/// Wraps the existing `GeminiClient` and delegates all operations.
pub struct GeminiLlmClient {
    inner: RawGeminiClient,
}

impl GeminiLlmClient {
    pub fn new(api_key: &str, model: &str) -> Result<Self, LlmError> {
        let inner = RawGeminiClient::new(api_key, model).map_err(LlmError::from)?;
        Ok(Self { inner })
    }

    pub fn with_persistence(
        api_key: &str,
        model: &str,
        usage_path: std::path::PathBuf,
    ) -> Result<Self, LlmError> {
        let inner = RawGeminiClient::with_persistence(api_key, model, usage_path)
            .map_err(LlmError::from)?;
        Ok(Self { inner })
    }

    /// Access the underlying raw client for backward compatibility
    /// during the migration period.
    pub fn raw(&self) -> &RawGeminiClient {
        &self.inner
    }
}

/// Convert provider-neutral `ToolDefinition` to Gemini-specific wire format.
fn to_gemini_tools(tools: &[ToolDefinition]) -> Vec<gemini::Tool> {
    vec![gemini::Tool {
        function_declarations: tools
            .iter()
            .map(|td| gemini::FunctionDeclaration {
                name: td.name.clone(),
                description: td.description.clone(),
                parameters: td.parameters.clone(),
            })
            .collect(),
    }]
}

impl LlmClient for GeminiLlmClient {
    fn provider_name(&self) -> &str {
        "Gemini"
    }

    fn validate_key(&self) -> Result<(), LlmError> {
        self.inner.validate_key().map_err(LlmError::from)
    }

    fn validate_key_detailed(&self) -> KeyValidationResult {
        match self.inner.validate_key() {
            Ok(()) => KeyValidationResult {
                valid: true,
                message: "Gemini key validated successfully!".into(),
                warning: None,
            },
            Err(GeminiError::InvalidKey) => KeyValidationResult {
                valid: false,
                message: "Invalid Gemini API key. Check that you copied the full key from aistudio.google.com/apikey.".into(),
                warning: None,
            },
            Err(GeminiError::RateLimited) => KeyValidationResult {
                valid: true,
                message: "Gemini key is valid!".into(),
                warning: Some("Currently rate-limited. Try again shortly.".into()),
            },
            Err(GeminiError::Http(ref e)) => KeyValidationResult {
                valid: false,
                message: "Cannot connect to Gemini API. Check your internet connection.".into(),
                warning: Some(e.to_string()),
            },
            Err(GeminiError::Api { status, ref message }) => {
                if super::has_billing_keyword(message) {
                    KeyValidationResult {
                        valid: true,
                        message: "Gemini key is valid!".into(),
                        warning: Some("Your account may have billing restrictions. Check aistudio.google.com for details.".into()),
                    }
                } else {
                    KeyValidationResult {
                        valid: false,
                        message: format!("Gemini API error (HTTP {}).", status),
                        warning: if message.is_empty() { None } else { Some(message.clone()) },
                    }
                }
            }
            Err(e) => KeyValidationResult {
                valid: false,
                message: format!("Gemini validation failed: {}", e),
                warning: None,
            },
        }
    }

    fn generate(&self, prompt: &str) -> Result<String, LlmError> {
        self.inner.generate(prompt).map_err(LlmError::from)
    }

    fn generate_cached(&self, prompt: &str) -> Result<String, LlmError> {
        self.inner.generate_cached(prompt).map_err(LlmError::from)
    }

    fn generate_with_tools_progress(
        &self,
        prompt: &str,
        tools: &[ToolDefinition],
        execute_tool: &mut dyn FnMut(&str, &Value) -> Value,
        max_turns: usize,
        on_progress: &mut dyn FnMut(usize, usize, &[String]),
    ) -> Result<String, LlmError> {
        let gemini_tools = to_gemini_tools(tools);
        self.inner
            .generate_with_tools_progress(
                prompt,
                gemini_tools,
                execute_tool,
                max_turns,
                on_progress,
            )
            .map_err(LlmError::from)
    }

    fn list_models(&self) -> Result<Vec<super::ModelInfo>, LlmError> {
        let raw_models = self.inner.list_models().map_err(LlmError::from)?;
        Ok(raw_models
            .into_iter()
            .map(|(id, display)| super::ModelInfo {
                id,
                display_name: display,
            })
            .collect())
    }

    fn remaining_quota(&self) -> u32 {
        self.inner.remaining_quota()
    }

    fn clear_cache(&self) {
        self.inner.clear_cache();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_error_conversion() {
        let ge = GeminiError::InvalidKey;
        let le: LlmError = ge.into();
        assert!(matches!(le, LlmError::InvalidKey));

        let ge = GeminiError::RateLimited;
        let le: LlmError = ge.into();
        assert!(matches!(le, LlmError::RateLimited));

        let ge = GeminiError::Api {
            status: 500,
            message: "test".into(),
        };
        let le: LlmError = ge.into();
        assert!(matches!(le, LlmError::Api { status: 500, .. }));

        let ge = GeminiError::Parse("bad json".into());
        let le: LlmError = ge.into();
        assert!(matches!(le, LlmError::Parse(_)));

        let ge = GeminiError::Unavailable("quota exhausted".into());
        let le: LlmError = ge.into();
        assert!(matches!(le, LlmError::Unavailable(_)));
    }

    #[test]
    fn test_tool_definition_conversion() {
        let defs = vec![ToolDefinition {
            name: "test_tool".into(),
            description: "A test tool".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "arg1": { "type": "string" }
                }
            }),
        }];

        let gemini_tools = to_gemini_tools(&defs);
        assert_eq!(gemini_tools.len(), 1);
        assert_eq!(gemini_tools[0].function_declarations.len(), 1);
        assert_eq!(gemini_tools[0].function_declarations[0].name, "test_tool");
        assert_eq!(
            gemini_tools[0].function_declarations[0].description,
            "A test tool"
        );
    }

    #[test]
    fn test_provider_name() {
        let client = GeminiLlmClient::new("fake-key", "gemini-2.5-flash").unwrap();
        assert_eq!(client.provider_name(), "Gemini");
    }

    #[test]
    fn test_remaining_quota_default() {
        let client = GeminiLlmClient::new("fake-key", "gemini-2.5-flash").unwrap();
        assert_eq!(client.remaining_quota(), 250);
    }
}
