//! Gemini provider — implements `LlmClient` by delegating to the existing `GeminiClient`.
//! This is a thin adapter: all HTTP, rate limiting, caching, and retry logic
//! stays in `crate::gemini::GeminiClient`. This module just bridges the trait.

use serde_json::Value;

use super::body::MAX_LLM_BODY;
use super::{KeyValidationResult, LlmClient, LlmError, ToolDefinition};
use crate::gemini::{self, GeminiClient as RawGeminiClient, GeminiError};

/// Reject a response larger than [`MAX_LLM_BODY`] at the trait boundary.
///
/// The socket read itself lives in `crate::gemini::read_gemini_stream`, which
/// is outside this module's ownership, so the peak allocation there is not
/// bounded by this check. What it does bound is everything downstream: a
/// runaway body is not cloned into the response cache, the prompt pipeline,
/// or the chat history.
///
/// ponytail: this is the adapter half of the cap. The real fix is
/// `body_capped` around the reader in `crates/optimizer/src/gemini.rs`
/// (handed off — see leaf-1.1.1's report).
fn body_capped(text: String) -> Result<String, LlmError> {
    if text.len() as u64 > MAX_LLM_BODY {
        return Err(LlmError::Api {
            status: 502,
            message: format!(
                "Gemini response exceeded the {} MiB body cap and was dropped",
                MAX_LLM_BODY / (1024 * 1024)
            ),
        });
    }
    Ok(text)
}

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
///
/// Gemini validates the tool schema *before* generation and hard-400s the
/// whole request on a keyword it does not know — measured 2026-08-27:
/// `Unknown name "additionalProperties" at
/// 'tools[0].function_declarations[0].parameters'`. `$schema` is rejected the
/// same way; `enum`, `anyOf`, `oneOf`, `minimum` and `maximum` are accepted.
/// Today's definitions come from `gemini_tools::tool_declarations()` and
/// carry neither, but a `ToolDefinition` is provider-neutral by contract, so
/// an OpenAI-shaped schema reaching here would take the addon's primary
/// pipeline down at request time. Strip them on the way in.
fn to_gemini_tools(tools: &[ToolDefinition]) -> Vec<gemini::Tool> {
    vec![gemini::Tool {
        function_declarations: tools
            .iter()
            .map(|td| gemini::FunctionDeclaration {
                name: td.name.clone(),
                description: td.description.clone(),
                parameters: strip_unsupported_schema_keywords(td.parameters.clone()),
            })
            .collect(),
    }]
}

/// JSON Schema keywords Gemini's tool validator rejects outright.
const GEMINI_REJECTED_SCHEMA_KEYS: [&str; 2] = ["additionalProperties", "$schema"];

/// Recursively drop [`GEMINI_REJECTED_SCHEMA_KEYS`] from a JSON Schema.
fn strip_unsupported_schema_keywords(mut schema: Value) -> Value {
    match &mut schema {
        Value::Object(map) => {
            for key in GEMINI_REJECTED_SCHEMA_KEYS {
                map.remove(key);
            }
            for value in map.values_mut() {
                *value = strip_unsupported_schema_keywords(value.take());
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                *item = strip_unsupported_schema_keywords(item.take());
            }
        }
        _ => {}
    }
    schema
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
        self.inner
            .generate(prompt)
            .map_err(LlmError::from)
            .and_then(body_capped)
    }

    fn generate_cached(&self, prompt: &str) -> Result<String, LlmError> {
        self.inner
            .generate_cached(prompt)
            .map_err(LlmError::from)
            .and_then(body_capped)
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
            .and_then(body_capped)
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
    fn strips_schema_keywords_gemini_rejects() {
        let defs = vec![ToolDefinition {
            name: "t".into(),
            description: "d".into(),
            parameters: serde_json::json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "nested": {
                        "type": "object",
                        "additionalProperties": true,
                        "properties": { "leaf": { "type": "string", "enum": ["a", "b"] } }
                    },
                    "list": {
                        "type": "array",
                        "items": { "type": "object", "additionalProperties": false }
                    }
                }
            }),
        }];

        let params = &to_gemini_tools(&defs)[0].function_declarations[0].parameters;
        let rendered = params.to_string();
        assert!(
            !rendered.contains("additionalProperties"),
            "Gemini 400s on this keyword: {rendered}"
        );
        assert!(!rendered.contains("$schema"), "{rendered}");
        // Everything Gemini does accept must survive untouched.
        assert_eq!(params["type"], serde_json::json!("object"));
        assert_eq!(
            params["properties"]["nested"]["properties"]["leaf"]["enum"],
            serde_json::json!(["a", "b"])
        );
        assert_eq!(
            params["properties"]["list"]["items"]["type"],
            serde_json::json!("object")
        );
    }

    #[test]
    fn body_cap_rejects_an_oversized_response() {
        let ok = "x".repeat(1024);
        assert_eq!(body_capped(ok.clone()).expect("under cap"), ok);
        let huge = "x".repeat(MAX_LLM_BODY as usize + 1);
        assert!(body_capped(huge).is_err());
    }

    #[test]
    fn test_remaining_quota_default() {
        let client = GeminiLlmClient::new("fake-key", "gemini-2.5-flash").unwrap();
        assert_eq!(client.remaining_quota(), 250);
    }
}
