//! Gemini API client for LLM-powered build reasoning.
//! Uses Google AI Studio's REST API (generativelanguage.googleapis.com).
//! API key is sent via x-goog-api-key header (not URL query) for security.
//! Includes response caching to minimize quota usage.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const GEMINI_MODELS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

#[derive(Debug, thiserror::Error)]
pub enum GeminiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
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

pub struct GeminiClient {
    api_key: String,
    model: String,
    http: reqwest::blocking::Client,
    cache: crate::llm::response_cache::ResponseCache,
    rate: Mutex<RateTracker>,
    usage_path: Option<PathBuf>,
}

impl GeminiClient {
    fn stream_url(&self) -> String {
        format!(
            "{}/{}:streamGenerateContent?alt=sse",
            GEMINI_API_BASE, self.model
        )
    }
}

struct RateTracker {
    requests_this_minute: u32,
    minute_start: Instant,
    requests_today: u32,
    /// Day number (Unix epoch / 86400) for daily counter reset.
    current_day: u64,
}

/// Persisted usage data saved to disk between sessions.
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
            0 // new day, reset counter
        };
        Self {
            requests_this_minute: 0,
            minute_start: Instant::now(),
            requests_today,
            current_day: today,
        }
    }

    /// Check rate limits and pre-reserve a slot (increment counters).
    /// If the request later fails, call `undo_reserve()` to release the slot.
    fn check_and_reserve(&mut self) -> Result<(), GeminiError> {
        // Reset daily counter if the day changed
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

        if self.requests_this_minute >= 10 {
            return Err(GeminiError::RateLimited);
        }
        if self.requests_today >= 240 {
            return Err(GeminiError::Unavailable(
                "Daily quota nearly exhausted (240/250)".into(),
            ));
        }

        // Reserve slot atomically with the check
        self.requests_this_minute += 1;
        self.requests_today += 1;
        Ok(())
    }

    /// Release a reserved slot if the request failed before reaching the API.
    fn undo_reserve(&mut self) {
        self.requests_this_minute = self.requests_this_minute.saturating_sub(1);
        self.requests_today = self.requests_today.saturating_sub(1);
    }

    fn remaining_today(&self) -> u32 {
        250u32.saturating_sub(self.requests_today)
    }

    fn to_persisted(&self) -> PersistedUsage {
        PersistedUsage {
            day: self.current_day,
            requests_today: self.requests_today,
        }
    }
}

// ─── Gemini API Types ───

#[derive(Serialize)]
struct GenerateRequest {
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Tool>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<Part>,
}

/// A Part can carry text, a function call (from model), or a function response (from us).
/// Uses flat optional fields for robust serde with the Gemini API.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "functionCall", skip_serializing_if = "Option::is_none")]
    function_call: Option<FunctionCall>,
    #[serde(rename = "functionResponse", skip_serializing_if = "Option::is_none")]
    function_response: Option<FunctionResponse>,
}

impl Part {
    fn text(s: impl Into<String>) -> Self {
        Part {
            text: Some(s.into()),
            function_call: None,
            function_response: None,
        }
    }

    fn function_response(name: impl Into<String>, response: serde_json::Value) -> Self {
        Part {
            text: None,
            function_call: None,
            function_response: Some(FunctionResponse {
                name: name.into(),
                response,
            }),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

// ─── Tool Declarations ───

#[derive(Serialize, Debug, Clone)]
pub struct Tool {
    #[serde(rename = "functionDeclarations")]
    pub function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Serialize, Debug, Clone)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ─── SSE Streaming ───

/// One Gemini SSE `data:` payload (streamGenerateContent?alt=sse chunks are
/// full GenerateContentResponse objects with partial candidates).
#[derive(Deserialize)]
struct StreamGenerateResponse {
    candidates: Option<Vec<Candidate>>,
    /// Mid-stream failure: `{"error":{"code":…,"message":…,"status":…}}`.
    #[serde(default)]
    error: Option<StreamErrorPayload>,
}

#[derive(Deserialize)]
struct StreamErrorPayload {
    #[serde(default)]
    code: Option<u16>,
    #[serde(default)]
    message: Option<String>,
}

/// Reads a Gemini `streamGenerateContent` SSE body into one merged `Content`.
///
/// Each chunk carries partial `candidates[0].content.parts`; text parts are
/// concatenated in arrival order and function-call parts are passed through
/// whole (Gemini emits a `functionCall` part complete in a single chunk).
/// An `error` payload is the mid-stream failure channel.
fn read_gemini_stream<R: std::io::Read>(reader: R) -> Result<Content, GeminiError> {
    use std::io::BufRead;

    let mut role: Option<String> = None;
    let mut text = String::new();
    let mut parts: Vec<Part> = Vec::new();

    for line in std::io::BufReader::new(reader).lines() {
        let line = line.map_err(|e| GeminiError::Parse(format!("stream read failed: {e}")))?;
        let line = line.trim();
        if !line.starts_with("data: ") {
            continue;
        }
        let chunk: StreamGenerateResponse = match serde_json::from_str(&line["data: ".len()..]) {
            Ok(chunk) => chunk,
            Err(_) => continue,
        };
        if let Some(err) = chunk.error {
            return Err(GeminiError::Api {
                status: err.code.unwrap_or(502),
                message: err
                    .message
                    .unwrap_or_else(|| "Gemini stream error".to_string()),
            });
        }
        let Some(content) = chunk
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
        else {
            continue;
        };
        if role.is_none() {
            role = content.role.clone();
        }
        for part in content.parts {
            if let Some(t) = part.text {
                text.push_str(&t);
            }
            if let Some(call) = part.function_call {
                parts.push(Part {
                    text: None,
                    function_call: Some(call),
                    function_response: None,
                });
            }
        }
    }

    if !text.is_empty() {
        parts.insert(0, Part::text(text));
    }
    if parts.is_empty() {
        return Err(GeminiError::Parse("No response content from Gemini".into()));
    }
    Ok(Content {
        role: role.or(Some("model".to_string())),
        parts,
    })
}

// ─── Response Types ───

#[derive(Deserialize)]
struct Candidate {
    content: Option<Content>,
}

/// Drop oldest tool-call turn(s) when the conversation exceeds the token
/// budget. A Gemini turn is one model Content (with function_call parts)
/// followed by one user Content (with function_response parts); pairs must
/// be dropped atomically. The initial user prompt and the most recent turn
/// are always preserved.
fn trim_contents(contents: &mut Vec<Content>, budget_tokens: usize) {
    use crate::llm::trim::estimate_tokens;

    fn part_tokens(p: &Part) -> usize {
        let text = p.text.as_deref().map(estimate_tokens).unwrap_or(0);
        let fc = p
            .function_call
            .as_ref()
            .map(|fc| estimate_tokens(&fc.args.to_string()))
            .unwrap_or(0);
        let fr = p
            .function_response
            .as_ref()
            .map(|fr| estimate_tokens(&fr.response.to_string()))
            .unwrap_or(0);
        text + fc + fr
    }
    fn content_tokens(c: &Content) -> usize {
        c.parts.iter().map(part_tokens).sum()
    }

    let mut total: usize = contents.iter().map(content_tokens).sum();
    if total <= budget_tokens {
        return;
    }

    loop {
        let turn_starts: Vec<usize> = contents
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, c)| c.parts.iter().any(|p| p.function_call.is_some()))
            .map(|(i, _)| i)
            .collect();

        if turn_starts.len() < 2 {
            return;
        }

        contents.drain(turn_starts[0]..turn_starts[1]);
        total = contents.iter().map(content_tokens).sum();
        if total <= budget_tokens {
            return;
        }
    }
}

impl GeminiClient {
    pub fn new(api_key: &str, model: &str) -> Result<Self, GeminiError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()?;
        Ok(Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            http,
            cache: crate::llm::response_cache::ResponseCache::new(1800, 64),
            rate: Mutex::new(RateTracker::new()),
            usage_path: None,
        })
    }

    /// Create a client with persistent rate tracking.
    /// Loads existing usage from `usage_path` and saves after each request.
    pub fn with_persistence(
        api_key: &str,
        model: &str,
        usage_path: PathBuf,
    ) -> Result<Self, GeminiError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()?;

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
            cache: crate::llm::response_cache::ResponseCache::new(1800, 64),
            rate: Mutex::new(rate),
            usage_path: Some(usage_path),
        })
    }

    /// Validate the API key using the models list endpoint (no quota consumed).
    pub fn validate_key(&self) -> Result<(), GeminiError> {
        let resp = self
            .http
            .get(GEMINI_MODELS_URL)
            .header("x-goog-api-key", &self.api_key)
            .send()?;

        match resp.status().as_u16() {
            200 => Ok(()),
            401 | 403 => Err(GeminiError::InvalidKey),
            429 => Err(GeminiError::RateLimited),
            status => {
                let body = resp.text().unwrap_or_default();
                Err(GeminiError::Api {
                    status,
                    message: body,
                })
            }
        }
    }

    /// List models that support content generation.
    /// Calls `GET /v1beta/models` and filters by `supportedGenerationMethods`.
    pub fn list_models(&self) -> Result<Vec<(String, String)>, GeminiError> {
        let resp = self
            .http
            .get(GEMINI_MODELS_URL)
            .header("x-goog-api-key", &self.api_key)
            .send()?;

        match resp.status().as_u16() {
            200 => {}
            401 | 403 => return Err(GeminiError::InvalidKey),
            429 => return Err(GeminiError::RateLimited),
            status => {
                let body = resp.text().unwrap_or_default();
                return Err(GeminiError::Api {
                    status,
                    message: body,
                });
            }
        }

        #[derive(Deserialize)]
        struct ModelsResponse {
            models: Option<Vec<ModelEntry>>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ModelEntry {
            name: Option<String>,
            display_name: Option<String>,
            supported_generation_methods: Option<Vec<String>>,
        }

        let body: ModelsResponse = resp.json().map_err(|e| GeminiError::Parse(e.to_string()))?;

        let models = body.models.unwrap_or_default();
        let mut result: Vec<(String, String)> = models
            .into_iter()
            .filter(|m| {
                m.supported_generation_methods
                    .as_ref()
                    .is_some_and(|methods| methods.iter().any(|m| m == "generateContent"))
            })
            .filter_map(|m| {
                let name = m.name?;
                // Strip "models/" prefix: "models/gemini-2.5-flash" → "gemini-2.5-flash"
                let id = name.strip_prefix("models/").unwrap_or(&name).to_string();
                let display = m.display_name.unwrap_or_else(|| id.clone());
                Some((id, display))
            })
            .collect();

        // Sort: models with "flash" first (cheapest), then alphabetically
        result.sort_by(|a, b| {
            let a_flash = a.0.contains("flash");
            let b_flash = b.0.contains("flash");
            match (a_flash, b_flash) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.0.cmp(&b.0),
            }
        });

        Ok(result)
    }

    /// Send a prompt to Gemini, using cache if available.
    /// Returns cached response if the same prompt was sent within 30 minutes.
    pub fn generate_cached(&self, prompt: &str) -> Result<String, GeminiError> {
        // Check cache (recover from poison)
        if let Some(text) = self.cache.get(prompt) {
            return Ok(text);
        }

        // Not cached — generate
        let text = self.generate(prompt)?;
        self.cache.insert(prompt, text.clone());

        Ok(text)
    }

    /// Send a prompt to Gemini (no caching). Checks rate limits first.
    pub fn generate(&self, prompt: &str) -> Result<String, GeminiError> {
        let request = GenerateRequest {
            contents: vec![Content {
                role: Some("user".into()),
                parts: vec![Part::text(prompt)],
            }],
            tools: None,
        };

        let content = self.send_request(&request)?;
        content
            .parts
            .into_iter()
            .find_map(|p| p.text)
            .ok_or_else(|| GeminiError::Parse("No response text from Gemini".into()))
    }

    /// Multi-turn generation with function calling (tool use).
    /// Gemini can call tools to query game data and calculations.
    /// Returns the final text response after all tool calls are resolved.
    pub fn generate_with_tools(
        &self,
        prompt: &str,
        tools: Vec<Tool>,
        mut execute_tool: impl FnMut(&str, &serde_json::Value) -> serde_json::Value,
        max_turns: usize,
    ) -> Result<String, GeminiError> {
        self.generate_with_tools_progress(
            prompt,
            tools,
            &mut execute_tool,
            max_turns,
            &mut |_, _, _| {},
        )
    }

    /// Like `generate_with_tools` but calls `on_progress(turn, max_turns, tool_names)` after
    /// each tool-calling round, so the UI can show what Gemini is doing.
    pub fn generate_with_tools_progress(
        &self,
        prompt: &str,
        tools: Vec<Tool>,
        execute_tool: &mut dyn FnMut(&str, &serde_json::Value) -> serde_json::Value,
        max_turns: usize,
        on_progress: &mut dyn FnMut(usize, usize, &[String]),
    ) -> Result<String, GeminiError> {
        let mut contents = vec![Content {
            role: Some("user".into()),
            parts: vec![Part::text(prompt)],
        }];

        let mut last_text: Option<String> = None;

        for turn in 0..max_turns {
            trim_contents(&mut contents, crate::llm::trim::SAFE_PROMPT_BUDGET_TOKENS);
            let request = GenerateRequest {
                contents: contents.clone(),
                tools: Some(tools.clone()),
            };

            let response_content = self.send_request(&request)?;

            // Capture any text from this response (Gemini may send text + calls)
            if let Some(text) = response_content.parts.iter().find_map(|p| p.text.clone()) {
                last_text = Some(text);
            }

            // Check if model wants to call a function
            let function_calls: Vec<&FunctionCall> = response_content
                .parts
                .iter()
                .filter_map(|p| p.function_call.as_ref())
                .collect();

            if function_calls.is_empty() {
                // No function calls — return text response
                return last_text
                    .ok_or_else(|| GeminiError::Parse("No response text from Gemini".into()));
            }

            // Report progress: which tools are being called this turn
            let tool_names: Vec<String> = function_calls.iter().map(|fc| fc.name.clone()).collect();
            on_progress(turn + 1, max_turns, &tool_names);

            // Add model's response to conversation history
            contents.push(response_content.clone());

            // Execute each function call and build response parts
            let mut response_parts = Vec::new();
            for fc in &function_calls {
                let result = execute_tool(&fc.name, &fc.args);
                response_parts.push(Part::function_response(&fc.name, result));
            }

            // Send function responses back
            contents.push(Content {
                role: Some("user".into()),
                parts: response_parts,
            });
        }

        // If we exceeded max_turns but had text, return it rather than error
        last_text.ok_or_else(|| {
            GeminiError::Parse(format!(
                "Tool loop exceeded {} turns with no text response",
                max_turns
            ))
        })
    }

    /// Low-level: send a request and return the response Content.
    /// Retries up to 2 times on transient server errors (500/503).
    fn send_request(&self, request: &GenerateRequest) -> Result<Content, GeminiError> {
        const MAX_RETRIES: u32 = 3;

        // Atomically check rate limit and reserve a slot
        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .check_and_reserve()?;

        let url = self.stream_url();
        let mut last_error: Option<GeminiError> = None;
        let mut next_delay = std::time::Duration::from_secs(5);

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                std::thread::sleep(next_delay);
                next_delay *= 2;
            }

            let resp = match self
                .http
                .post(&url)
                .header("x-goog-api-key", &self.api_key)
                .json(request)
                .send()
            {
                Ok(r) => r,
                Err(e) => {
                    if attempt == MAX_RETRIES - 1 {
                        self.rate
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .undo_reserve();
                        return Err(GeminiError::Http(e));
                    }
                    last_error = Some(GeminiError::Http(e));
                    continue;
                }
            };

            let status = resp.status().as_u16();
            match status {
                200 => {
                    let content = read_gemini_stream(resp)?;

                    // Persist usage
                    {
                        let rate = self.rate.lock().unwrap_or_else(|e| e.into_inner());
                        self.persist_usage(&rate);
                    }

                    return Ok(content);
                }
                401 | 403 => {
                    self.rate
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .undo_reserve();
                    return Err(GeminiError::InvalidKey);
                }
                429 => {
                    self.rate
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .undo_reserve();
                    return Err(GeminiError::RateLimited);
                }
                // Retryable: server failures + 408/504 gateway timeouts.
                408 | 500 | 502 | 503 | 504 => {
                    if let Some(secs) = resp
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                    {
                        next_delay = std::time::Duration::from_secs(secs.min(60));
                    }
                    let body = resp.text().unwrap_or_default();
                    last_error = Some(GeminiError::Api {
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
                    return Err(GeminiError::Api {
                        status,
                        message: body,
                    });
                }
            }
        }
        // All retries exhausted
        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .undo_reserve();
        Err(last_error.unwrap_or_else(|| GeminiError::Api {
            status: 500,
            message: "Gemini server error after retries".into(),
        }))
    }

    /// Save rate tracker to disk if a persistence path is configured.
    ///
    /// Uses temp-write + atomic rename so a crash mid-write can't corrupt the
    /// usage file. A corrupted usage file makes the addon think it has zero
    /// quota left for the day and silently blocks LLM calls until the next
    /// daily rollover.
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

    /// Get remaining daily quota.
    pub fn remaining_quota(&self) -> u32 {
        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remaining_today()
    }

    /// Clear the response cache.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_tracker_rpm_limit() {
        let mut tracker = RateTracker::new();
        for _ in 0..10 {
            assert!(tracker.check_and_reserve().is_ok());
        }
        // 11th check should fail (10 RPM limit)
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_rate_tracker_daily_limit() {
        let mut tracker = RateTracker::new();
        tracker.requests_today = 240;
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_rate_tracker_persistence_roundtrip_same_day() {
        let mut tracker = RateTracker::new();
        for _ in 0..5 {
            tracker.check_and_reserve().unwrap();
        }
        assert_eq!(tracker.requests_today, 5);
        assert_eq!(tracker.requests_this_minute, 5);

        let persisted = tracker.to_persisted();
        let reloaded = RateTracker::from_persisted(persisted);

        assert_eq!(reloaded.requests_today, 5);
        assert_eq!(reloaded.requests_this_minute, 0);
    }

    #[test]
    fn test_rate_tracker_persistence_day_rollover_resets_daily() {
        let yesterday = current_epoch_day().saturating_sub(1);
        let persisted = PersistedUsage {
            day: yesterday,
            requests_today: 200,
        };
        let reloaded = RateTracker::from_persisted(persisted);

        assert_eq!(reloaded.requests_today, 0);
        assert_eq!(reloaded.requests_this_minute, 0);
        assert_eq!(reloaded.current_day, current_epoch_day());
    }

    #[test]
    fn test_rate_tracker_minute_rollover_preserves_daily() {
        let mut tracker = RateTracker::new();
        for _ in 0..3 {
            tracker.check_and_reserve().unwrap();
        }
        assert_eq!(tracker.requests_this_minute, 3);
        assert_eq!(tracker.requests_today, 3);

        tracker.minute_start = Instant::now() - std::time::Duration::from_secs(61);

        tracker.check_and_reserve().unwrap();

        assert_eq!(tracker.requests_this_minute, 1);
        assert_eq!(tracker.requests_today, 4);
    }

    #[test]
    fn test_rate_tracker_daily_limit_enforced_after_reload() {
        // Simulate a DLL reload mid-day with the daily budget nearly exhausted.
        let persisted = PersistedUsage {
            day: current_epoch_day(),
            requests_today: 240,
        };
        let mut reloaded = RateTracker::from_persisted(persisted);
        assert_eq!(reloaded.requests_today, 240);
        assert!(reloaded.check_and_reserve().is_err());
    }

    #[test]
    fn test_remaining_quota() {
        let client = GeminiClient::new("fake-key", "gemini-2.5-flash").unwrap();
        assert_eq!(client.remaining_quota(), 250);
    }

    #[test]
    fn test_function_call_name_round_trip() {
        // Gemini's protocol pairs functionCall/functionResponse by `name`
        // (no id field). The round-trip invariant: when we echo the call,
        // the name must survive serde → HTTP → serde untouched.
        let server_content_json = r#"{
            "role": "model",
            "parts": [
                {"functionCall": {"name": "square_number", "args": {"number": 7}}}
            ]
        }"#;

        let content: Content = serde_json::from_str(server_content_json).expect("parse Content");
        let call = content
            .parts
            .iter()
            .find_map(|p| p.function_call.as_ref())
            .expect("functionCall present");
        assert_eq!(call.name, "square_number");

        let result_part = Part::function_response(&call.name, serde_json::json!({"result": 49}));
        let wire = serde_json::to_value(&result_part).unwrap();
        assert_eq!(wire["functionResponse"]["name"], "square_number");
        assert!(wire["functionResponse"]["response"].is_object());
    }

    #[test]
    fn test_trim_contents_drops_oldest_turn() {
        fn user_text(s: &str) -> Content {
            Content {
                role: Some("user".into()),
                parts: vec![Part::text(s)],
            }
        }
        fn model_call(name: &str, payload: &str) -> Content {
            Content {
                role: Some("model".into()),
                parts: vec![Part {
                    text: None,
                    function_call: Some(FunctionCall {
                        name: name.into(),
                        args: serde_json::json!({ "q": payload }),
                    }),
                    function_response: None,
                }],
            }
        }
        fn user_fresponse(name: &str, payload: &str) -> Content {
            Content {
                role: Some("user".into()),
                parts: vec![Part::function_response(
                    name,
                    serde_json::Value::String(payload.into()),
                )],
            }
        }

        let filler = "x".repeat(400);
        let mut contents = vec![
            user_text("initial prompt"),
            model_call("get_trait_details", &filler),
            user_fresponse("get_trait_details", &filler),
            model_call("get_trait_details", &filler),
            user_fresponse("get_trait_details", &filler),
            model_call("get_skill_info", &filler),
            user_fresponse("get_skill_info", &filler),
        ];
        let original_len = contents.len();

        trim_contents(&mut contents, 200);

        assert!(
            contents.len() < original_len,
            "expected trim, got {}",
            contents.len()
        );
        // Initial prompt preserved.
        assert_eq!(contents[0].role.as_deref(), Some("user"));
        assert_eq!(contents[0].parts[0].text.as_deref(), Some("initial prompt"));
        // Last model call is still the most recent one (get_skill_info).
        let last_model = contents
            .iter()
            .rposition(|c| c.parts.iter().any(|p| p.function_call.is_some()))
            .expect("must retain a model call");
        let last_call_name = contents[last_model].parts[0]
            .function_call
            .as_ref()
            .map(|fc| fc.name.clone())
            .unwrap();
        assert_eq!(last_call_name, "get_skill_info");
        // Every function_response still has a preceding function_call (pair invariant).
        for (i, c) in contents.iter().enumerate() {
            if c.parts.iter().any(|p| p.function_response.is_some()) {
                assert!(
                    contents[..i]
                        .iter()
                        .any(|prev| prev.parts.iter().any(|p| p.function_call.is_some())),
                    "orphan function_response at index {}",
                    i
                );
            }
        }
    }

    #[test]
    fn test_trim_contents_noop_under_budget() {
        let mut contents = vec![Content {
            role: Some("user".into()),
            parts: vec![Part::text("short")],
        }];
        trim_contents(&mut contents, 10_000);
        assert_eq!(contents.len(), 1);
    }

    #[test]
    fn test_read_gemini_stream_merges_text_chunks() {
        let sse = r#"data: {"candidates":[{"content":{"parts":[{"text":"Hel"}],"role":"model"},"index":0}]}
data: {"candidates":[{"content":{"parts":[{"text":"lo"}],"role":"model"},"index":0}]}

data: {"candidates":[{"content":{"parts":[{"text":"!"}],"role":"model"},"index":0}]}
"#;
        let content = read_gemini_stream(sse.as_bytes()).expect("stream parses");
        assert_eq!(content.role.as_deref(), Some("model"));
        assert_eq!(content.parts.len(), 1);
        assert_eq!(content.parts[0].text.as_deref(), Some("Hello!"));
    }

    #[test]
    fn test_read_gemini_stream_passes_function_calls_through() {
        let sse = r#"data: {"candidates":[{"content":{"parts":[{"text":"Rolling"},{"functionCall":{"name":"pick","args":{"slot":"heal"}}}],"role":"model"},"index":0}]}
"#;
        let content = read_gemini_stream(sse.as_bytes()).expect("stream parses");
        assert_eq!(content.parts.len(), 2);
        assert_eq!(content.parts[0].text.as_deref(), Some("Rolling"));
        let call = content.parts[1]
            .function_call
            .as_ref()
            .expect("function call");
        assert_eq!(call.name, "pick");
        assert_eq!(call.args["slot"], "heal");
    }

    #[test]
    fn test_read_gemini_stream_error_payload_maps_to_api_error() {
        let sse = r#"data: {"error":{"code":429,"message":"Resource exhausted","status":"RESOURCE_EXHAUSTED"}}
"#;
        match read_gemini_stream(sse.as_bytes()) {
            Err(GeminiError::Api { status, message }) => {
                assert_eq!(status, 429);
                assert!(message.contains("exhausted"));
            }
            _other => panic!("expected Api error"),
        }
    }
}
