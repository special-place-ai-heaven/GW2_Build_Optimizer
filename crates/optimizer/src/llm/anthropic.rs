//! Anthropic provider — implements `LlmClient` for Claude models.
//! Uses the Anthropic Messages API with tool use.
//! Auth: `x-api-key` header + `anthropic-version: 2023-06-01`.
//!
//! Key differences from OpenAI/Gemini:
//! - System prompt is a top-level `system` field, not a message
//! - Tool results use `tool_result` content blocks with `tool_use_id`
//! - `max_tokens` is mandatory
//! - Response content is an array of typed blocks (text, tool_use)

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::rate::{PersistedUsage, RateTracker};
use super::{KeyValidationResult, LlmClient, LlmError, ToolDefinition};

/// Anthropic's conservative default requests-per-minute ceiling.
const RPM_LIMIT: u32 = 50;

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicClient {
    api_key: String,
    model: String,
    http: reqwest::blocking::Client,
    cache: crate::llm::response_cache::ResponseCache,
    rate: Mutex<RateTracker>,
    usage_path: Option<PathBuf>,
}

// ─── Anthropic API Types ───

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

/// Anthropic content can be a simple string or an array of content blocks.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Serialize, Debug, Clone)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Option<Vec<ContentBlock>>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

impl AnthropicClient {
    pub fn new(api_key: &str, model: &str) -> Result<Self, LlmError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;
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
            cache: crate::llm::response_cache::ResponseCache::new(1800, 64),
            rate: Mutex::new(rate),
            usage_path: Some(usage_path),
        })
    }

    fn send_messages(
        &self,
        messages: &[AnthropicMessage],
        system: Option<&str>,
        tools: Option<&[ToolDefinition]>,
        max_tokens: u32,
    ) -> Result<MessagesResponse, LlmError> {
        const MAX_RETRIES: u32 = 3;

        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .check_and_reserve()?;

        let anthropic_tools = tools.map(|defs| {
            defs.iter()
                .map(|td| AnthropicTool {
                    name: td.name.clone(),
                    description: td.description.clone(),
                    input_schema: td.parameters.clone(),
                })
                .collect::<Vec<_>>()
        });

        let request = MessagesRequest {
            model: self.model.clone(),
            max_tokens,
            messages: messages.to_vec(),
            system: system.map(|s| s.to_string()),
            tools: anthropic_tools,
        };

        let url = format!("{}/messages", ANTHROPIC_API_BASE);
        let mut last_error: Option<LlmError> = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(5 * (1 << (attempt - 1)));
                std::thread::sleep(delay);
            }

            let resp = match self
                .http
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
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
                    let body: MessagesResponse = match resp.json() {
                        Ok(b) => b,
                        Err(e) => {
                            self.rate
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .undo_reserve();
                            return Err(LlmError::Http(e.to_string()));
                        }
                    };

                    // Persist usage
                    {
                        let rate = self.rate.lock().unwrap_or_else(|e| e.into_inner());
                        self.persist_usage(&rate);
                    }

                    return Ok(body);
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
                500 | 502 | 503 | 529 => {
                    // 529 = Anthropic overloaded
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
            message: "Anthropic server error after retries".into(),
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

/// Extract text from Anthropic content blocks.
fn extract_text(blocks: &[ContentBlock]) -> Option<String> {
    let texts: Vec<&str> = blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    if texts.is_empty() {
        None
    } else {
        Some(texts.join(""))
    }
}

/// Extract tool use blocks from content.
fn extract_tool_uses(blocks: &[ContentBlock]) -> Vec<(String, String, Value)> {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, name, input } => {
                Some((id.clone(), name.clone(), input.clone()))
            }
            _ => None,
        })
        .collect()
}

/// Drop oldest tool-call turn(s) when the conversation exceeds the token
/// budget. An Anthropic turn is one assistant message (with tool_use blocks)
/// followed by one user message (with tool_result blocks); pairs must be
/// dropped atomically. The initial user prompt and the most recent turn are
/// always preserved.
fn trim_messages(messages: &mut Vec<AnthropicMessage>, budget_tokens: usize) {
    use super::trim::estimate_tokens;

    fn message_tokens(m: &AnthropicMessage) -> usize {
        match &m.content {
            AnthropicContent::Text(s) => estimate_tokens(s),
            AnthropicContent::Blocks(blocks) => blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => estimate_tokens(text),
                    ContentBlock::ToolUse { input, .. } => estimate_tokens(&input.to_string()),
                    ContentBlock::ToolResult { content, .. } => estimate_tokens(content),
                })
                .sum(),
        }
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
            .filter(|(_, m)| m.role == "assistant")
            .map(|(i, _)| i)
            .collect();

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

impl LlmClient for AnthropicClient {
    fn provider_name(&self) -> &str {
        "Anthropic"
    }

    fn validate_key(&self) -> Result<(), LlmError> {
        // Anthropic doesn't have a models list endpoint like OpenAI/Gemini.
        // Use a minimal messages call with max_tokens=1 to validate the key.
        let messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicContent::Text("Hi".to_string()),
        }];

        let request = MessagesRequest {
            model: self.model.clone(),
            max_tokens: 1,
            messages,
            system: None,
            tools: None,
        };

        let url = format!("{}/messages", ANTHROPIC_API_BASE);
        let resp = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| LlmError::Http(e.to_string()))?;

        match resp.status().as_u16() {
            200 => Ok(()),
            401 => Err(LlmError::InvalidKey),
            // 400 with "credit balance" means key is valid but account has no credits.
            // 403 means key is valid but account is disabled/restricted.
            // Accept these as valid keys — billing is a separate concern.
            400 | 403 => {
                let body = resp.text().unwrap_or_default();
                if body.contains("credit balance")
                    || body.contains("billing")
                    || body.contains("disabled")
                    || body.contains("permission")
                {
                    Ok(())
                } else {
                    Err(LlmError::Api {
                        status: 400,
                        message: body,
                    })
                }
            }
            429 => Ok(()), // Rate limited means key is valid
            status => {
                let body = resp.text().unwrap_or_default();
                Err(LlmError::Api {
                    status,
                    message: body,
                })
            }
        }
    }

    fn validate_key_detailed(&self) -> KeyValidationResult {
        let messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicContent::Text("Hi".to_string()),
        }];

        let request = MessagesRequest {
            model: self.model.clone(),
            max_tokens: 1,
            messages,
            system: None,
            tools: None,
        };

        let url = format!("{}/messages", ANTHROPIC_API_BASE);
        let resp = match self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                return KeyValidationResult {
                    valid: false,
                    message: "Cannot connect to Anthropic API. Check your internet connection."
                        .into(),
                    warning: Some(e.to_string()),
                };
            }
        };

        let status = resp.status().as_u16();
        let body = resp.text().unwrap_or_default();

        match status {
            200 => KeyValidationResult {
                valid: true,
                message: "Anthropic key validated successfully!".into(),
                warning: None,
            },
            401 => KeyValidationResult {
                valid: false,
                message: "Invalid Anthropic API key. Check that you copied the full key from console.anthropic.com/settings/keys.".into(),
                warning: None,
            },
            400 if super::has_billing_keyword(&body) => KeyValidationResult {
                valid: true,
                message: "Anthropic key is valid!".into(),
                warning: Some("Your account has no credits. Add credits at console.anthropic.com to use this provider.".into()),
            },
            403 => KeyValidationResult {
                valid: true,
                message: "Anthropic key is valid!".into(),
                warning: Some("Your account may be restricted. Check console.anthropic.com/settings for details.".into()),
            },
            429 => KeyValidationResult {
                valid: true,
                message: "Anthropic key is valid!".into(),
                warning: Some("Currently rate-limited. Try again shortly.".into()),
            },
            _ => KeyValidationResult {
                valid: false,
                message: format!("Anthropic API error (HTTP {}).", status),
                warning: if body.is_empty() { None } else { Some(body) },
            },
        }
    }

    fn generate(&self, prompt: &str) -> Result<String, LlmError> {
        let messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicContent::Text(prompt.to_string()),
        }];

        let response = self.send_messages(&messages, None, None, 8192)?;
        let blocks = response
            .content
            .ok_or_else(|| LlmError::Parse("No content from Anthropic".into()))?;
        extract_text(&blocks).ok_or_else(|| LlmError::Parse("No text in Anthropic response".into()))
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
        let mut messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicContent::Text(prompt.to_string()),
        }];

        let mut last_text: Option<String> = None;

        for turn in 0..max_turns {
            trim_messages(&mut messages, super::trim::SAFE_PROMPT_BUDGET_TOKENS);
            let response = self.send_messages(&messages, None, Some(tools), 4096)?;

            let blocks = response.content.unwrap_or_default();

            // Capture text
            if let Some(text) = extract_text(&blocks) {
                last_text = Some(text);
            }

            // Check for tool use
            let tool_uses = extract_tool_uses(&blocks);
            if tool_uses.is_empty() {
                // No tool calls — return text
                return last_text
                    .ok_or_else(|| LlmError::Parse("No response text from Anthropic".into()));
            }

            // Report progress
            let tool_names: Vec<String> =
                tool_uses.iter().map(|(_, name, _)| name.clone()).collect();
            on_progress(turn + 1, max_turns, &tool_names);

            // Add assistant message with all content blocks
            messages.push(AnthropicMessage {
                role: "assistant".to_string(),
                content: AnthropicContent::Blocks(blocks),
            });

            // Execute each tool and build result blocks
            let mut result_blocks = Vec::new();
            for (tool_use_id, name, input) in &tool_uses {
                let result = execute_tool(name, input);
                let result_str = serde_json::to_string(&result).unwrap_or_default();
                result_blocks.push(ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: result_str,
                });
            }

            // Tool results go in a user message
            messages.push(AnthropicMessage {
                role: "user".to_string(),
                content: AnthropicContent::Blocks(result_blocks),
            });
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
        let url = format!("{}/models", ANTHROPIC_API_BASE);
        let resp = self
            .http
            .get(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
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
            display_name: Option<String>,
        }

        let body: ModelsResponse = resp.json().map_err(|e| LlmError::Parse(e.to_string()))?;

        let entries = body.data.unwrap_or_default();

        let mut models: Vec<super::ModelInfo> = entries
            .into_iter()
            .map(|m| super::ModelInfo {
                display_name: m.display_name.unwrap_or_else(|| m.id.clone()),
                id: m.id,
            })
            .collect();

        // Sort by ID (alphabetical puts claude-haiku before claude-sonnet, etc.)
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
    use super::*;

    #[test]
    fn test_provider_name() {
        let client = AnthropicClient::new("fake-key", "claude-sonnet-4-6").unwrap();
        assert_eq!(client.provider_name(), "Anthropic");
    }

    #[test]
    fn test_remaining_quota_default() {
        let client = AnthropicClient::new("fake-key", "claude-sonnet-4-6").unwrap();
        assert_eq!(client.remaining_quota(), 10000);
    }

    #[test]
    fn test_tool_definition_to_anthropic_format() {
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

        let anthropic_tools: Vec<AnthropicTool> = defs
            .iter()
            .map(|td| AnthropicTool {
                name: td.name.clone(),
                description: td.description.clone(),
                input_schema: td.parameters.clone(),
            })
            .collect();

        assert_eq!(anthropic_tools.len(), 1);
        assert_eq!(anthropic_tools[0].name, "get_profession_info");

        // Verify serialization format
        let json = serde_json::to_value(&anthropic_tools[0]).unwrap();
        assert_eq!(json["name"], "get_profession_info");
        assert_eq!(json["input_schema"]["type"], "object");
    }

    #[test]
    fn test_content_block_serialization() {
        let text_block = ContentBlock::Text {
            text: "Hello".into(),
        };
        let json = serde_json::to_value(&text_block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "Hello");

        let tool_use = ContentBlock::ToolUse {
            id: "toolu_123".into(),
            name: "get_profession_info".into(),
            input: serde_json::json!({"profession": "Warrior"}),
        };
        let json = serde_json::to_value(&tool_use).unwrap();
        assert_eq!(json["type"], "tool_use");
        assert_eq!(json["id"], "toolu_123");
        assert_eq!(json["name"], "get_profession_info");

        let tool_result = ContentBlock::ToolResult {
            tool_use_id: "toolu_123".into(),
            content: r#"{"name":"Warrior"}"#.into(),
        };
        let json = serde_json::to_value(&tool_result).unwrap();
        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["tool_use_id"], "toolu_123");
    }

    #[test]
    fn test_extract_text_from_blocks() {
        let blocks = vec![
            ContentBlock::Text {
                text: "Hello ".into(),
            },
            ContentBlock::ToolUse {
                id: "t1".into(),
                name: "test".into(),
                input: Value::Null,
            },
            ContentBlock::Text {
                text: "world".into(),
            },
        ];
        assert_eq!(extract_text(&blocks), Some("Hello world".into()));
    }

    #[test]
    fn test_extract_text_empty_blocks() {
        let blocks = vec![ContentBlock::ToolUse {
            id: "t1".into(),
            name: "test".into(),
            input: Value::Null,
        }];
        assert_eq!(extract_text(&blocks), None);
    }

    #[test]
    fn test_extract_tool_uses() {
        let blocks = vec![
            ContentBlock::Text {
                text: "Let me check".into(),
            },
            ContentBlock::ToolUse {
                id: "toolu_1".into(),
                name: "get_profession_info".into(),
                input: serde_json::json!({"profession": "Warrior"}),
            },
            ContentBlock::ToolUse {
                id: "toolu_2".into(),
                name: "get_spec_traits".into(),
                input: serde_json::json!({"spec_name": "Berserker"}),
            },
        ];

        let uses = extract_tool_uses(&blocks);
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].0, "toolu_1");
        assert_eq!(uses[0].1, "get_profession_info");
        assert_eq!(uses[1].0, "toolu_2");
        assert_eq!(uses[1].1, "get_spec_traits");
    }

    #[test]
    fn test_trim_messages_drops_oldest_turn() {
        fn user_text(s: &str) -> AnthropicMessage {
            AnthropicMessage {
                role: "user".into(),
                content: AnthropicContent::Text(s.into()),
            }
        }
        fn assistant_turn(id: &str, name: &str, payload: &str) -> AnthropicMessage {
            AnthropicMessage {
                role: "assistant".into(),
                content: AnthropicContent::Blocks(vec![ContentBlock::ToolUse {
                    id: id.into(),
                    name: name.into(),
                    input: serde_json::json!({ "q": payload }),
                }]),
            }
        }
        fn user_tool_result(id: &str, payload: &str) -> AnthropicMessage {
            AnthropicMessage {
                role: "user".into(),
                content: AnthropicContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: id.into(),
                    content: payload.into(),
                }]),
            }
        }

        let filler = "x".repeat(400);
        let mut messages = vec![
            user_text("initial prompt"),
            assistant_turn("tu_1", "get_trait_details", &filler),
            user_tool_result("tu_1", &filler),
            assistant_turn("tu_2", "get_trait_details", &filler),
            user_tool_result("tu_2", &filler),
            assistant_turn("tu_3", "get_trait_details", &filler),
            user_tool_result("tu_3", &filler),
        ];
        let original_len = messages.len();

        trim_messages(&mut messages, 200);

        assert!(
            messages.len() < original_len,
            "expected trimming, got {}",
            messages.len()
        );
        // Initial prompt preserved.
        assert_eq!(messages[0].role, "user");
        match &messages[0].content {
            AnthropicContent::Text(s) => assert_eq!(s, "initial prompt"),
            _ => panic!("first message lost text content"),
        }
        // Last turn's tool_use_id still matches its tool_result.
        let last_assistant_idx = messages
            .iter()
            .rposition(|m| m.role == "assistant")
            .expect("must retain at least one assistant");
        let last_use_id = match &messages[last_assistant_idx].content {
            AnthropicContent::Blocks(blocks) => blocks
                .iter()
                .find_map(|b| match b {
                    ContentBlock::ToolUse { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .expect("last assistant has a tool_use block"),
            _ => panic!("last assistant not block-shaped"),
        };
        assert_eq!(last_use_id, "tu_3");
        // Every tool_result still has a matching tool_use earlier.
        for (i, m) in messages.iter().enumerate() {
            if let AnthropicContent::Blocks(blocks) = &m.content {
                for b in blocks {
                    if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                        let paired = messages[..i].iter().any(|prev| {
                                if let AnthropicContent::Blocks(pbs) = &prev.content {
                                    pbs.iter().any(|pb| {
                                        matches!(pb, ContentBlock::ToolUse { id, .. } if id == tool_use_id)
                                    })
                                } else {
                                    false
                                }
                            });
                        assert!(paired, "orphaned tool_use_id {}", tool_use_id);
                    }
                }
            }
        }
    }

    #[test]
    fn test_trim_messages_noop_under_budget() {
        let mut messages = vec![AnthropicMessage {
            role: "user".into(),
            content: AnthropicContent::Text("short".into()),
        }];
        trim_messages(&mut messages, 10_000);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_anthropic_content_text_serialization() {
        let msg = AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicContent::Text("Hello".to_string()),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
    }

    #[test]
    fn test_anthropic_content_blocks_serialization() {
        let msg = AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "toolu_123".into(),
                content: r#"{"ok": true}"#.into(),
            }]),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert!(json["content"].is_array());
        assert_eq!(json["content"][0]["type"], "tool_result");
    }

    #[test]
    fn test_tool_use_id_round_trip() {
        // Simulate Anthropic's response: an assistant message with one tool_use
        // block. Parse like `send_messages` does, extract the id via the same
        // `extract_tool_uses` helper `generate_with_tools_progress` uses, then
        // build the follow-up tool_result block and assert the id survives.
        let server_response_json = r#"{
            "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "toolu_01A8bL9Qrx", "name": "square_number", "input": {"number": 7}}
            ]
        }"#;

        let body: MessagesResponse =
            serde_json::from_str(server_response_json).expect("parse MessagesResponse");
        let blocks = body.content.expect("content blocks");
        let uses = extract_tool_uses(&blocks);
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].0, "toolu_01A8bL9Qrx");
        assert_eq!(uses[0].1, "square_number");

        let result_block = ContentBlock::ToolResult {
            tool_use_id: uses[0].0.clone(),
            content: r#"{"result":49}"#.into(),
        };
        let wire = serde_json::to_value(&result_block).unwrap();
        assert_eq!(wire["type"], "tool_result");
        assert_eq!(wire["tool_use_id"], "toolu_01A8bL9Qrx");
    }
}
