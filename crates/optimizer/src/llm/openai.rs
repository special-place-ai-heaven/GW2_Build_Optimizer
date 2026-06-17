//! OpenAI provider — implements `LlmClient` for GPT-4o and compatible models.
//! Uses the OpenAI Chat Completions API with function calling.
//! API key is sent via `Authorization: Bearer <key>` header.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::rate::{PersistedUsage, RateTracker};
use super::{KeyValidationResult, LlmClient, LlmError, ToolDefinition};

/// OpenAI's conservative default requests-per-minute ceiling.
const RPM_LIMIT: u32 = 60;

const OPENAI_API_BASE: &str = "https://api.openai.com/v1";

pub struct OpenAiClient {
    api_key: String,
    model: String,
    http: reqwest::blocking::Client,
    cache: Mutex<HashMap<String, CachedResponse>>,
    rate: Mutex<RateTracker>,
    usage_path: Option<PathBuf>,
}

struct CachedResponse {
    text: String,
    cached_at: Instant,
}

// ─── OpenAI API Types ───

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Message {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCallResponse>>,
    /// For role="tool" messages: the ID of the tool call being responded to.
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Serialize, Debug, Clone)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ToolCallResponse {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: FunctionCallData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FunctionCallData {
    name: String,
    /// OpenAI sends arguments as a JSON *string*, not an object.
    arguments: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Option<Vec<Choice>>,
}

#[derive(Deserialize)]
struct Choice {
    message: Option<Message>,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

impl OpenAiClient {
    pub fn new(api_key: &str, model: &str) -> Result<Self, LlmError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;
        Ok(Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            http,
            cache: Mutex::new(HashMap::new()),
            rate: Mutex::new(RateTracker::new(RPM_LIMIT)),
            usage_path: None,
        })
    }

    pub fn with_persistence(
        api_key: &str,
        model: &str,
        usage_path: PathBuf,
    ) -> Result<Self, LlmError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;

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
            cache: Mutex::new(HashMap::new()),
            rate: Mutex::new(rate),
            usage_path: Some(usage_path),
        })
    }

    fn send_chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Message, LlmError> {
        const MAX_RETRIES: u32 = 3;

        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .check_and_reserve()?;

        let openai_tools = tools.map(|defs| {
            defs.iter()
                .map(|td| OpenAiTool {
                    tool_type: "function".to_string(),
                    function: OpenAiFunction {
                        name: td.name.clone(),
                        description: td.description.clone(),
                        parameters: td.parameters.clone(),
                    },
                })
                .collect::<Vec<_>>()
        });

        let request = ChatRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            tools: openai_tools,
            max_tokens: Some(8192),
        };

        let url = format!("{}/chat/completions", OPENAI_API_BASE);
        let mut last_error: Option<LlmError> = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(5 * (1 << (attempt - 1)));
                std::thread::sleep(delay);
            }

            let resp = match self
                .http
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
            {
                Ok(r) => r,
                Err(e) => {
                    if attempt == MAX_RETRIES - 1 {
                        self.rate
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .undo_reserve();
                        return Err(LlmError::Http(e.to_string()));
                    }
                    last_error = Some(LlmError::Http(e.to_string()));
                    continue;
                }
            };

            let status = resp.status().as_u16();
            match status {
                200 => {
                    let body: ChatResponse = match resp.json() {
                        Ok(b) => b,
                        Err(e) => {
                            self.rate
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .undo_reserve();
                            return Err(LlmError::Http(e.to_string()));
                        }
                    };

                    let message = match body
                        .choices
                        .and_then(|c| c.into_iter().next())
                        .and_then(|c| c.message)
                    {
                        Some(m) => m,
                        None => {
                            // 200 with empty choices — release the rate slot so this
                            // dead trip doesn't count against the daily counter.
                            self.rate
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .undo_reserve();
                            return Err(LlmError::Parse("No response from OpenAI".into()));
                        }
                    };

                    // Persist usage
                    {
                        let rate = self.rate.lock().unwrap_or_else(|e| e.into_inner());
                        self.persist_usage(&rate);
                    }

                    return Ok(message);
                }
                401 => {
                    self.rate
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .undo_reserve();
                    return Err(LlmError::InvalidKey);
                }
                429 => {
                    self.rate
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .undo_reserve();
                    return Err(LlmError::RateLimited);
                }
                500 | 502 | 503 => {
                    let body = resp.text().unwrap_or_default();
                    last_error = Some(LlmError::Api {
                        status,
                        message: body,
                    });
                    continue;
                }
                _ => {
                    let body = resp.text().unwrap_or_default();
                    self.rate
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .undo_reserve();
                    return Err(LlmError::Api {
                        status,
                        message: body,
                    });
                }
            }
        }

        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .undo_reserve();
        Err(last_error.unwrap_or_else(|| LlmError::Api {
            status: 500,
            message: "OpenAI server error after retries".into(),
        }))
    }

    fn persist_usage(&self, rate: &RateTracker) {
        if let Some(ref path) = self.usage_path {
            let persisted = rate.to_persisted();
            if let Ok(json) = serde_json::to_string(&persisted) {
                let tmp = path.with_extension("tmp");
                if std::fs::write(&tmp, &json).is_ok() {
                    let _ = std::fs::rename(&tmp, path);
                } else {
                    let _ = std::fs::remove_file(&tmp);
                }
            }
        }
    }
}

impl LlmClient for OpenAiClient {
    fn provider_name(&self) -> &str {
        "OpenAI"
    }

    fn validate_key(&self) -> Result<(), LlmError> {
        let url = format!("{}/models", OPENAI_API_BASE);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .map_err(|e| LlmError::Http(e.to_string()))?;

        match resp.status().as_u16() {
            200 => Ok(()),
            401 => Err(LlmError::InvalidKey),
            429 => Ok(()), // Rate limited means key is valid
            status => {
                let body = resp.text().unwrap_or_default();
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
        let url = format!("{}/models", OPENAI_API_BASE);
        let resp = match self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                return KeyValidationResult {
                    valid: false,
                    message: "Cannot connect to OpenAI API. Check your internet connection.".into(),
                    warning: Some(e.to_string()),
                };
            }
        };

        let status = resp.status().as_u16();
        let body = resp.text().unwrap_or_default();

        match status {
            200 => KeyValidationResult {
                valid: true,
                message: "OpenAI key validated successfully!".into(),
                warning: None,
            },
            401 => KeyValidationResult {
                valid: false,
                message: "Invalid OpenAI API key. Check that you copied the full key from platform.openai.com/api-keys.".into(),
                warning: None,
            },
            429 => {
                let warning = if super::has_billing_keyword(&body) {
                    "Your account has exceeded its usage limit. Check billing at platform.openai.com/account/billing."
                } else {
                    "Currently rate-limited. Try again shortly."
                };
                KeyValidationResult {
                    valid: true,
                    message: "OpenAI key is valid!".into(),
                    warning: Some(warning.into()),
                }
            }
            _ => {
                if super::has_billing_keyword(&body) {
                    KeyValidationResult {
                        valid: true,
                        message: "OpenAI key is valid!".into(),
                        warning: Some("Your account may have billing issues. Check platform.openai.com/account/billing.".into()),
                    }
                } else {
                    KeyValidationResult {
                        valid: false,
                        message: format!("OpenAI API error (HTTP {}).", status),
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
            .ok_or_else(|| LlmError::Parse("No response text from OpenAI".into()))
    }

    fn generate_cached(&self, prompt: &str) -> Result<String, LlmError> {
        let key = prompt.to_string();

        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = cache.get(&key) {
                if cached.cached_at.elapsed().as_secs() < 1800 {
                    return Ok(cached.text.clone());
                }
            }
        }

        let text = self.generate(prompt)?;

        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.insert(
                key,
                CachedResponse {
                    text: text.clone(),
                    cached_at: Instant::now(),
                },
            );
        }

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
            trim_messages(&mut messages, super::trim::SAFE_PROMPT_BUDGET_TOKENS);
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
                        .ok_or_else(|| LlmError::Parse("No response text from OpenAI".into()));
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
        let url = format!("{}/models", OPENAI_API_BASE);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .map_err(|e| LlmError::Http(e.to_string()))?;

        match resp.status().as_u16() {
            200 => {}
            401 => return Err(LlmError::InvalidKey),
            429 => return Err(LlmError::RateLimited),
            status => {
                let body = resp.text().unwrap_or_default();
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
            #[allow(dead_code)]
            created: Option<u64>,
        }

        let body: ModelsResponse = resp.json().map_err(|e| LlmError::Parse(e.to_string()))?;

        let entries = body.data.unwrap_or_default();

        // Filter to chat-capable models; exclude embeddings, image, audio, etc.
        let exclude_patterns = [
            "embedding",
            "dall-e",
            "whisper",
            "tts",
            "babbage",
            "davinci",
            "moderation",
        ];
        let include_prefixes = ["gpt-", "o1", "o3", "o4", "chatgpt-"];

        let mut models: Vec<super::ModelInfo> = entries
            .into_iter()
            .filter(|m| {
                let id = m.id.to_lowercase();
                // Must match at least one include prefix
                let included = include_prefixes.iter().any(|p| id.starts_with(p));
                // Must not match any exclude pattern
                let excluded = exclude_patterns.iter().any(|p| id.contains(p));
                included && !excluded
            })
            .map(|m| {
                let display = openai_display_name(&m.id);
                super::ModelInfo {
                    id: m.id,
                    display_name: display,
                }
            })
            .collect();

        // Sort: newer/better models first (gpt-4o before gpt-4o-mini, o3 before o1)
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
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }
}

/// Drop oldest tool-call turn(s) when the conversation exceeds the token
/// budget. A "turn" is one assistant message with `tool_calls` plus the
/// tool-role messages that reference its ids; these must be dropped as an
/// atomic unit so the remaining assistant/tool pairing stays valid. The
/// initial user prompt (messages[0]) and the most recent turn are always
/// preserved.
fn trim_messages(messages: &mut Vec<Message>, budget_tokens: usize) {
    use super::trim::estimate_tokens;

    fn message_tokens(m: &Message) -> usize {
        let content = m.content.as_deref().map(estimate_tokens).unwrap_or(0);
        let tool_calls = m
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| estimate_tokens(&tc.function.arguments))
                    .sum::<usize>()
            })
            .unwrap_or(0);
        content + tool_calls
    }

    let mut total: usize = messages.iter().map(message_tokens).sum();
    if total <= budget_tokens {
        return;
    }

    loop {
        let turn_starts: Vec<usize> = messages
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, m)| m.role == "assistant" && m.tool_calls.is_some())
            .map(|(i, _)| i)
            .collect();

        // Keep at least the initial prompt + the most recent turn.
        if turn_starts.len() < 2 {
            return;
        }

        messages.drain(turn_starts[0]..turn_starts[1]);
        total = messages.iter().map(message_tokens).sum();
        if total <= budget_tokens {
            return;
        }
    }
}

/// Derive a human-readable display name from an OpenAI model ID.
fn openai_display_name(id: &str) -> String {
    match id {
        "gpt-4o" => "GPT-4o".into(),
        "gpt-4o-mini" => "GPT-4o Mini".into(),
        "gpt-4-turbo" => "GPT-4 Turbo".into(),
        "gpt-4" => "GPT-4".into(),
        "gpt-3.5-turbo" => "GPT-3.5 Turbo".into(),
        "o1" => "o1 (reasoning)".into(),
        "o1-mini" => "o1-mini (reasoning)".into(),
        "o3-mini" => "o3-mini (reasoning)".into(),
        "chatgpt-4o-latest" => "ChatGPT-4o Latest".into(),
        _ => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let client = OpenAiClient::new("fake-key", "gpt-4o").unwrap();
        assert_eq!(client.provider_name(), "OpenAI");
    }

    #[test]
    fn test_remaining_quota_default() {
        let client = OpenAiClient::new("fake-key", "gpt-4o").unwrap();
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
    fn test_trim_messages_drops_oldest_turn() {
        fn user_msg(text: &str) -> Message {
            Message {
                role: "user".into(),
                content: Some(text.into()),
                tool_calls: None,
                tool_call_id: None,
            }
        }
        fn assistant_with_call(id: &str, args: &str) -> Message {
            Message {
                role: "assistant".into(),
                content: None,
                tool_calls: Some(vec![ToolCallResponse {
                    id: id.into(),
                    call_type: "function".into(),
                    function: FunctionCallData {
                        name: "get_trait_details".into(),
                        arguments: args.into(),
                    },
                }]),
                tool_call_id: None,
            }
        }
        fn tool_result(id: &str, body: &str) -> Message {
            Message {
                role: "tool".into(),
                content: Some(body.into()),
                tool_calls: None,
                tool_call_id: Some(id.into()),
            }
        }

        // Big filler (~100 chars = ~25 tokens each)
        let filler = "x".repeat(400);

        let mut messages = vec![
            user_msg("initial prompt"),
            assistant_with_call("call_1", &format!(r#"{{"q":"{}"}}"#, filler)),
            tool_result("call_1", &filler),
            assistant_with_call("call_2", &format!(r#"{{"q":"{}"}}"#, filler)),
            tool_result("call_2", &filler),
            assistant_with_call("call_3", &format!(r#"{{"q":"{}"}}"#, filler)),
            tool_result("call_3", &filler),
        ];
        let original_len = messages.len();

        // Budget of 200 tokens = 800 chars. Each turn is >= 200 chars of tool args + 400 chars of result.
        trim_messages(&mut messages, 200);

        assert!(
            messages.len() < original_len,
            "expected trimming, got {} messages",
            messages.len()
        );
        // Initial user prompt preserved.
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content.as_deref(), Some("initial prompt"));
        // Most recent turn preserved (the call_3 pair).
        let last_assistant_idx = messages
            .iter()
            .rposition(|m| m.role == "assistant")
            .expect("must retain an assistant message");
        let last_tc = messages[last_assistant_idx]
            .tool_calls
            .as_ref()
            .expect("assistant has tool_calls")
            .first()
            .unwrap();
        assert_eq!(last_tc.id, "call_3");
        // Every tool message still refers to a tool_call_id present on a preceding assistant.
        for (i, m) in messages.iter().enumerate() {
            if m.role == "tool" {
                let id = m.tool_call_id.as_deref().unwrap();
                let found = messages[..i].iter().any(|prev| {
                    prev.tool_calls
                        .as_ref()
                        .is_some_and(|tcs| tcs.iter().any(|tc| tc.id == id))
                });
                assert!(found, "orphaned tool_call_id {}", id);
            }
        }
    }

    #[test]
    fn test_trim_messages_noop_under_budget() {
        let mut messages = vec![
            Message {
                role: "user".into(),
                content: Some("short prompt".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "assistant".into(),
                content: Some("short reply".into()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let original_len = messages.len();
        trim_messages(&mut messages, 10_000);
        assert_eq!(messages.len(), original_len);
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
        // Simulate a server response containing an assistant message with one
        // tool call. Parse it the same way `send_chat` does, then echo the id
        // back on a tool-role follow-up message — the same echo path
        // `generate_with_tools_progress` uses.
        let server_response_json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_fRzHUzNm7",
                        "type": "function",
                        "function": {"name": "square_number", "arguments": "{\"number\":7}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }"#;

        let body: ChatResponse =
            serde_json::from_str(server_response_json).expect("parse ChatResponse");
        let assistant_msg = body
            .choices
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.message)
            .expect("assistant message");
        let tool_calls = assistant_msg.tool_calls.clone().expect("tool_calls");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_fRzHUzNm7");

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
