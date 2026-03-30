//! OpenAI provider — implements `LlmClient` for GPT-4o and compatible models.
//! Uses the OpenAI Chat Completions API with function calling.
//! API key is sent via `Authorization: Bearer <key>` header.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{KeyValidationResult, LlmClient, LlmError, ToolDefinition};

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

struct RateTracker {
    requests_this_minute: u32,
    minute_start: Instant,
    requests_today: u32,
    current_day: u64,
}

#[derive(Serialize, Deserialize)]
struct PersistedUsage {
    day: u64,
    requests_today: u32,
}

fn current_epoch_day() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86400
}

impl RateTracker {
    fn new() -> Self {
        Self {
            requests_this_minute: 0,
            minute_start: Instant::now(),
            requests_today: 0,
            current_day: current_epoch_day(),
        }
    }

    fn from_persisted(persisted: PersistedUsage) -> Self {
        let today = current_epoch_day();
        let requests_today = if persisted.day == today {
            persisted.requests_today
        } else {
            0
        };
        Self {
            requests_this_minute: 0,
            minute_start: Instant::now(),
            requests_today,
            current_day: today,
        }
    }

    fn check_and_reserve(&mut self) -> Result<(), LlmError> {
        let today = current_epoch_day();
        if today != self.current_day {
            self.requests_today = 0;
            self.current_day = today;
        }

        let now = Instant::now();
        if now.duration_since(self.minute_start).as_secs() >= 60 {
            self.requests_this_minute = 0;
            self.minute_start = now;
        }

        // OpenAI tier limits vary; use conservative defaults
        if self.requests_this_minute >= 60 {
            return Err(LlmError::RateLimited);
        }

        self.requests_this_minute += 1;
        self.requests_today += 1;
        Ok(())
    }

    fn undo_reserve(&mut self) {
        self.requests_this_minute = self.requests_this_minute.saturating_sub(1);
        self.requests_today = self.requests_today.saturating_sub(1);
    }

    fn remaining_today(&self) -> u32 {
        // OpenAI doesn't have a hard daily limit for paid users;
        // track usage for display purposes
        10000u32.saturating_sub(self.requests_today)
    }

    fn to_persisted(&self) -> PersistedUsage {
        PersistedUsage {
            day: self.current_day,
            requests_today: self.requests_today,
        }
    }
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
            rate: Mutex::new(RateTracker::new()),
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
                Some(persisted) => RateTracker::from_persisted(persisted),
                None => RateTracker::new(),
            }
        } else {
            RateTracker::new()
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

                    let message = body
                        .choices
                        .and_then(|c| c.into_iter().next())
                        .and_then(|c| c.message)
                        .ok_or_else(|| LlmError::Parse("No response from OpenAI".into()))?;

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
                let _ = std::fs::write(path, json);
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
                let warning = if body.contains("quota") || body.contains("exceeded") || body.contains("billing") {
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
                if body.contains("billing") || body.contains("quota")
                    || body.contains("exceeded") || body.contains("insufficient")
                {
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
        let defs = vec![ToolDefinition {
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
    fn test_rate_tracker_rpm_limit() {
        let mut tracker = RateTracker::new();
        for _ in 0..60 {
            assert!(tracker.check_and_reserve().is_ok());
        }
        // 61st should fail
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_rate_tracker_undo_reserve() {
        let mut tracker = RateTracker::new();
        tracker.check_and_reserve().unwrap();
        assert_eq!(tracker.requests_this_minute, 1);
        tracker.undo_reserve();
        assert_eq!(tracker.requests_this_minute, 0);
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
}
