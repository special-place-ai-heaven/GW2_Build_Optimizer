//! OpenRouter provider — implements `LlmClient` for any model hosted on
//! https://openrouter.ai. OpenRouter exposes an OpenAI-compatible Chat
//! Completions API (with tools/function calling) and a `/models` endpoint
//! that lists every supported model across providers (Anthropic, OpenAI,
//! Google, Mistral, Meta, etc.). One API key, many models.
//!
//! Differences from the OpenAI provider:
//!  - Base URL: `https://openrouter.ai/api/v1`
//!  - Optional `HTTP-Referer` + `X-Title` headers identify this app to
//!    OpenRouter's analytics/leaderboards.
//!  - Model IDs are slash-prefixed (`anthropic/claude-sonnet-4-5`).
//!
//! API key is sent via `Authorization: Bearer <key>` header, same as OpenAI.
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Deserialize;
use serde_json::Value;

use super::body::{json_capped, read_body_capped};
use super::openai_compat::{
    http_client, send_chat, Message, ProviderCore, CHAT_REQUEST_TIMEOUT, MAX_COMPLETION_TOKENS,
    METADATA_TIMEOUT, REASONING_TOKEN_CAP,
};
use super::rate::{persist_usage, PersistedUsage, RateTracker};
use super::trim::trim_openai_messages;
use super::{KeyValidationResult, LlmClient, LlmError, ToolDefinition};

/// OpenRouter's conservative default requests-per-minute ceiling.
const RPM_LIMIT: u32 = 60;

const OPENROUTER_API_BASE: &str = "https://openrouter.ai/api/v1";
/// Identify this app in OpenRouter's request logs / leaderboards. Optional
/// but recommended by OpenRouter docs.
const OPENROUTER_HTTP_REFERER: &str =
    "https://github.com/special-place-ai-heaven/GW2_Build_Optimizer";
const OPENROUTER_X_TITLE: &str = "GW2 Build Optimizer";

pub struct OpenRouterClient {
    api_key: String,
    model: String,
    http: reqwest::blocking::Client,
    cache: crate::llm::response_cache::ResponseCache,
    rate: Mutex<RateTracker>,
    usage_path: Option<PathBuf>,
}

impl OpenRouterClient {
    pub fn new(api_key: &str, model: &str) -> Result<Self, LlmError> {
        let http = http_client()?;
        Ok(Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            http,
            cache: crate::llm::response_cache::ResponseCache::new(1800, 64),
            rate: Mutex::new(RateTracker::new(RPM_LIMIT)),
            usage_path: None,
        })
    }

    pub fn with_persistence(
        api_key: &str,
        model: &str,
        usage_path: PathBuf,
    ) -> Result<Self, LlmError> {
        let http = http_client()?;

        let rate = if usage_path.exists() {
            match std::fs::read_to_string(&usage_path)
                .ok()
                .and_then(|s| serde_json::from_str::<PersistedUsage>(&s).ok())
            {
                Some(persisted) => RateTracker::from_persisted(persisted, RPM_LIMIT),
                None => RateTracker::new(RPM_LIMIT),
            }
        } else {
            RateTracker::new(RPM_LIMIT)
        };

        Ok(Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            http,
            cache: crate::llm::response_cache::ResponseCache::new(1800, 64),
            rate: Mutex::new(rate),
            usage_path: Some(usage_path),
        })
    }

    fn send_chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Message, LlmError> {
        let extra_headers = [
            ("HTTP-Referer", OPENROUTER_HTTP_REFERER.to_string()),
            ("X-Title", OPENROUTER_X_TITLE.to_string()),
        ];
        let is_cancelled = super::cancel::is_cancelled;
        let core = ProviderCore {
            http: &self.http,
            rate: &self.rate,
            api_key: &self.api_key,
            base_url: OPENROUTER_API_BASE,
            model: &self.model,
            extra_headers: &extra_headers,
            label: "OpenRouter",
            max_tokens: MAX_COMPLETION_TOKENS,
            reasoning_max_tokens: Some(REASONING_TOKEN_CAP),
            // OpenRouter is the one base URL that understands the top-level
            // `provider` routing block.
            supports_provider_prefs: true,
            require_tool_endpoints: tools.is_some(),
            request_timeout: CHAT_REQUEST_TIMEOUT,
            max_retries: 2,
            is_cancelled: &is_cancelled,
        };
        let message = send_chat(core, messages, tools)?;
        // Persist usage
        {
            let rate = self.rate.lock().unwrap_or_else(|e| e.into_inner());
            self.persist_usage(&rate);
        }
        Ok(message)
    }

    fn persist_usage(&self, rate: &RateTracker) {
        persist_usage(self.usage_path.as_deref(), rate);
    }
}

impl LlmClient for OpenRouterClient {
    fn provider_name(&self) -> &str {
        "OpenRouter"
    }

    fn validate_key(&self) -> Result<(), LlmError> {
        let url = format!("{}/models", OPENROUTER_API_BASE);
        let resp = self
            .http
            .get(&url)
            .timeout(METADATA_TIMEOUT)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .map_err(|e| LlmError::Http(e.to_string()))?;

        match resp.status().as_u16() {
            200 => Ok(()),
            401 => Err(LlmError::InvalidKey),
            429 => Ok(()), // Rate limited means key is valid
            status => {
                let body = read_body_capped(resp);
                // Billing/quota errors mean the key is valid but account has issues
                if body.contains("billing")
                    || body.contains("quota")
                    || body.contains("exceeded")
                    || body.contains("insufficient")
                {
                    Ok(())
                } else {
                    Err(LlmError::Api {
                        status,
                        message: body,
                    })
                }
            }
        }
    }

    fn validate_key_detailed(&self) -> KeyValidationResult {
        let url = format!("{}/models", OPENROUTER_API_BASE);
        let resp = match self
            .http
            .get(&url)
            .timeout(METADATA_TIMEOUT)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                return KeyValidationResult {
                    valid: false,
                    message: "Cannot connect to OpenRouter API. Check your internet connection."
                        .into(),
                    warning: Some(e.to_string()),
                };
            }
        };

        let status = resp.status().as_u16();
        let body = read_body_capped(resp);

        match status {
            200 => KeyValidationResult {
                valid: true,
                message: "OpenRouter key validated successfully!".into(),
                warning: None,
            },
            401 => KeyValidationResult {
                valid: false,
                message: "Invalid OpenRouter API key. Check that you copied the full key from openrouter.ai/keys.".into(),
                warning: None,
            },
            429 => {
                let warning = if super::has_billing_keyword(&body) {
                    "Your account has exceeded its usage limit. Check credits at openrouter.ai/credits."
                } else {
                    "Currently rate-limited. Try again shortly."
                };
                KeyValidationResult {
                    valid: true,
                    message: "OpenRouter key is valid!".into(),
                    warning: Some(warning.into()),
                }
            }
            _ => {
                if super::has_billing_keyword(&body) {
                    KeyValidationResult {
                        valid: true,
                        message: "OpenRouter key is valid!".into(),
                        warning: Some("Your account may be out of credits. Top up at openrouter.ai/credits.".into()),
                    }
                } else {
                    KeyValidationResult {
                        valid: false,
                        message: format!("OpenRouter API error (HTTP {}).", status),
                        warning: if body.is_empty() { None } else { Some(body) },
                    }
                }
            }
        }
    }

    fn generate(&self, prompt: &str) -> Result<String, LlmError> {
        let messages = vec![Message {
            role: "user".to_string(),
            content: Some(prompt.to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];

        let response = self.send_chat(&messages, None)?;
        response
            .content
            .ok_or_else(|| LlmError::Parse("No response text from OpenRouter".into()))
    }

    fn generate_cached(&self, prompt: &str) -> Result<String, LlmError> {
        if let Some(text) = self.cache.get(prompt) {
            return Ok(text);
        }

        let text = self.generate(prompt)?;
        self.cache.insert(prompt, text.clone());

        Ok(text)
    }

    fn generate_with_tools_progress(
        &self,
        prompt: &str,
        tools: &[ToolDefinition],
        execute_tool: &mut dyn FnMut(&str, &Value) -> Value,
        max_turns: usize,
        on_progress: &mut dyn FnMut(usize, usize, &[String]),
    ) -> Result<String, LlmError> {
        let mut messages = vec![Message {
            role: "user".to_string(),
            content: Some(prompt.to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];

        let mut last_text: Option<String> = None;

        for turn in 0..max_turns {
            // Between turns as well as inside the stream: a tool loop is up to
            // max_turns whole requests, so checking only inside one of them
            // still leaves the worker running after the flag flips.
            if super::cancel::is_cancelled() {
                return Err(LlmError::Unavailable(super::cancel::CANCELLED.to_string()));
            }
            trim_openai_messages(&mut messages, super::trim::SAFE_PROMPT_BUDGET_TOKENS);
            let response = self.send_chat(&messages, Some(tools))?;

            // Capture text content
            if let Some(ref text) = response.content {
                if !text.is_empty() {
                    last_text = Some(text.clone());
                }
            }

            // Check for tool calls
            let tool_calls = match response.tool_calls {
                Some(ref calls) if !calls.is_empty() => calls.clone(),
                _ => {
                    // No tool calls — return text
                    return last_text
                        .or(response.content)
                        .ok_or_else(|| LlmError::Parse("No response text from OpenRouter".into()));
                }
            };

            // Report progress
            let tool_names: Vec<String> = tool_calls
                .iter()
                .map(|tc| tc.function.name.clone())
                .collect();
            on_progress(turn + 1, max_turns, &tool_names);

            // Add assistant message (with tool_calls) to conversation
            messages.push(response);

            // Execute each tool call and add responses
            for tc in &tool_calls {
                // OpenAI sends arguments as a JSON *string* — parse it
                let args: Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

                let result = execute_tool(&tc.function.name, &args);
                let result_str = serde_json::to_string(&result).unwrap_or_default();

                messages.push(Message {
                    role: "tool".to_string(),
                    content: Some(result_str),
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                });
            }
        }

        // Max turns exceeded
        last_text.ok_or_else(|| {
            LlmError::Parse(format!(
                "Tool loop exceeded {} turns with no text response",
                max_turns
            ))
        })
    }

    fn list_models(&self) -> Result<Vec<super::ModelInfo>, LlmError> {
        // OpenRouter `/models` endpoint returns ALL hosted models across every
        // upstream provider (Anthropic, OpenAI, Google, Mistral, Meta, etc.).
        // Unlike OpenAI's `/models` (which mixes in audio/image/embedding
        // endpoints under one account), OpenRouter's catalog is already
        // pre-filtered to chat-capable LLMs — no OpenAI-style include/exclude
        // prefix list needed. We just deserialize, sort, and surface them.
        let url = format!("{}/models", OPENROUTER_API_BASE);
        let resp = self
            .http
            .get(&url)
            .timeout(METADATA_TIMEOUT)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", OPENROUTER_HTTP_REFERER)
            .header("X-Title", OPENROUTER_X_TITLE)
            .send()
            .map_err(|e| LlmError::Http(e.to_string()))?;

        match resp.status().as_u16() {
            200 => {}
            401 => return Err(LlmError::InvalidKey),
            429 => return Err(LlmError::RateLimited),
            status => {
                let body = read_body_capped(resp);
                return Err(LlmError::Api {
                    status,
                    message: body,
                });
            }
        }

        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Option<Vec<ModelEntry>>,
        }
        #[derive(Deserialize)]
        struct ModelEntry {
            id: String,
            /// Human-readable name when OpenRouter provides one (e.g.
            /// "Claude Sonnet 4.5"). Falls back to the slug when missing.
            #[serde(default)]
            name: Option<String>,
        }

        let body: ModelsResponse = json_capped(resp)?;
        let entries = body.data.unwrap_or_default();

        let mut models: Vec<super::ModelInfo> = entries
            .into_iter()
            .map(|m| super::ModelInfo {
                display_name: m.name.unwrap_or_else(|| m.id.clone()),
                id: m.id,
            })
            .collect();

        // Sort alphabetically by id — gives a stable, easily-scannable list
        // grouped by upstream provider (anthropic/*, google/*, openai/*…).
        models.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(models)
    }

    fn remaining_quota(&self) -> u32 {
        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remaining_today()
    }

    fn clear_cache(&self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::super::openai_compat::{
        FunctionCallData, OpenAiFunction, OpenAiTool, ToolCallResponse,
    };
    use super::super::sse::{read_stream, StreamedMessage};
    use super::*;

    #[test]
    fn test_provider_name() {
        let client = OpenRouterClient::new("fake-key", "anthropic/claude-sonnet-4-5").unwrap();
        assert_eq!(client.provider_name(), "OpenRouter");
    }

    #[test]
    fn test_remaining_quota_default() {
        let client = OpenRouterClient::new("fake-key", "gpt-4o").unwrap();
        assert_eq!(client.remaining_quota(), 10000);
    }

    #[test]
    fn test_tool_definition_to_openai_format() {
        let defs = [ToolDefinition {
            name: "get_profession_info".into(),
            description: "Get profession details".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "profession": { "type": "string" }
                },
                "required": ["profession"]
            }),
        }];

        let openai_tools: Vec<OpenAiTool> = defs
            .iter()
            .map(|td| OpenAiTool {
                tool_type: "function".to_string(),
                function: OpenAiFunction {
                    name: td.name.clone(),
                    description: td.description.clone(),
                    parameters: td.parameters.clone(),
                },
            })
            .collect();

        assert_eq!(openai_tools.len(), 1);
        assert_eq!(openai_tools[0].tool_type, "function");
        assert_eq!(openai_tools[0].function.name, "get_profession_info");

        // Verify serialization format
        let json = serde_json::to_value(&openai_tools[0]).unwrap();
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "get_profession_info");
    }

    #[test]
    fn test_parse_tool_call_arguments_as_string() {
        // OpenAI sends arguments as a JSON string, not an object
        let tc = ToolCallResponse {
            id: "call_abc123".into(),
            call_type: "function".into(),
            function: FunctionCallData {
                name: "get_profession_info".into(),
                arguments: r#"{"profession":"Warrior"}"#.into(),
            },
        };

        let args: Value = serde_json::from_str(&tc.function.arguments).unwrap();
        assert_eq!(args["profession"], "Warrior");
    }

    #[test]
    fn test_message_serialization() {
        // User message
        let msg = Message {
            role: "user".to_string(),
            content: Some("Hello".to_string()),
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
        assert!(json.get("tool_calls").is_none());
        assert!(json.get("tool_call_id").is_none());

        // Tool response message
        let tool_msg = Message {
            role: "tool".to_string(),
            content: Some(r#"{"result": "ok"}"#.to_string()),
            tool_calls: None,
            tool_call_id: Some("call_abc123".to_string()),
        };
        let json = serde_json::to_value(&tool_msg).unwrap();
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_abc123");
    }

    #[test]
    fn test_tool_call_id_round_trip() {
        // Simulate a streamed server response carrying an assistant message
        // with one tool call — accumulated the same way `send_chat` does via
        // `read_stream` — then echo the id back on a tool-role follow-up, the
        // same path `generate_with_tools_progress` uses.
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":null}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_fRzHUzNm7\",\"type\":\"function\",\"function\":{\"name\":\"square_number\",\"arguments\":\"{\\\"number\\\":7}\"}}]}}]}\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n",
            "data: [DONE]\n",
        );

        let assistant_msg = match read_stream(sse.as_bytes(), &|| false).expect("stream parses") {
            StreamedMessage::Message(m) => m,
            StreamedMessage::Empty(finish) => panic!("unexpected empty stream: {finish}"),
        };
        let tool_calls = assistant_msg.tool_calls.clone().expect("tool_calls");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_fRzHUzNm7");
        assert_eq!(tool_calls[0].function.name, "square_number");
        assert_eq!(tool_calls[0].function.arguments, "{\"number\":7}");

        let follow_up = Message {
            role: "tool".into(),
            content: Some(r#"{"result":49}"#.into()),
            tool_calls: None,
            tool_call_id: Some(tool_calls[0].id.clone()),
        };
        let wire = serde_json::to_value(&follow_up).unwrap();
        assert_eq!(wire["role"], "tool");
        assert_eq!(wire["tool_call_id"], "call_fRzHUzNm7");
    }
}
