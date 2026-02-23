//! Gemini API client for LLM-powered build reasoning.
//! Uses Google AI Studio's REST API (generativelanguage.googleapis.com).
//! API key is sent via x-goog-api-key header (not URL query) for security.
//! Includes response caching to minimize quota usage.

use std::collections::HashMap;
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
    cache: Mutex<HashMap<String, CachedResponse>>,
    rate: Mutex<RateTracker>,
    usage_path: Option<PathBuf>,
}

impl GeminiClient {
    fn generate_url(&self) -> String {
        format!("{}/{}:generateContent", GEMINI_API_BASE, self.model)
    }
}

struct CachedResponse {
    text: String,
    cached_at: Instant,
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
        Part { text: Some(s.into()), function_call: None, function_response: None }
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

// ─── Response Types ───

#[derive(Deserialize)]
struct GenerateResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<Content>,
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
            cache: Mutex::new(HashMap::new()),
            rate: Mutex::new(RateTracker::new()),
            usage_path: None,
        })
    }

    /// Create a client with persistent rate tracking.
    /// Loads existing usage from `usage_path` and saves after each request.
    pub fn with_persistence(api_key: &str, model: &str, usage_path: PathBuf) -> Result<Self, GeminiError> {
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
            cache: Mutex::new(HashMap::new()),
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
                Err(GeminiError::Api { status, message: body })
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
                return Err(GeminiError::Api { status, message: body });
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

        let body: ModelsResponse = resp
            .json()
            .map_err(|e| GeminiError::Parse(e.to_string()))?;

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
        let key = prompt.to_string();

        // Check cache (recover from poison)
        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = cache.get(&key) {
                if cached.cached_at.elapsed().as_secs() < 1800 {
                    return Ok(cached.text.clone());
                }
            }
        }

        // Not cached — generate
        let text = self.generate(prompt)?;

        // Store in cache
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.insert(key, CachedResponse {
                text: text.clone(),
                cached_at: Instant::now(),
            });
        }

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
        content.parts.into_iter()
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
        self.generate_with_tools_progress(prompt, tools, &mut execute_tool, max_turns, &mut |_, _, _| {})
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
            let function_calls: Vec<&FunctionCall> = response_content.parts.iter()
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
            GeminiError::Parse(format!("Tool loop exceeded {} turns with no text response", max_turns))
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

        let url = self.generate_url();
        let mut last_error: Option<GeminiError> = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                // Exponential backoff: 5s, 15s
                let delay = std::time::Duration::from_secs(5 * (1 << (attempt - 1)));
                std::thread::sleep(delay);
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
                        self.rate.lock().unwrap_or_else(|e| e.into_inner()).undo_reserve();
                        return Err(GeminiError::Http(e));
                    }
                    last_error = Some(GeminiError::Http(e));
                    continue;
                }
            };

            let status = resp.status().as_u16();
            match status {
                200 => {
                    let body: GenerateResponse = match resp.json() {
                        Ok(b) => b,
                        Err(e) => {
                            self.rate.lock().unwrap_or_else(|e| e.into_inner()).undo_reserve();
                            return Err(GeminiError::Http(e));
                        }
                    };
                    let content = body
                        .candidates
                        .and_then(|c| c.into_iter().next())
                        .and_then(|c| c.content)
                        .ok_or_else(|| GeminiError::Parse("No response content from Gemini".into()))?;

                    // Persist usage
                    {
                        let rate = self.rate.lock().unwrap_or_else(|e| e.into_inner());
                        self.persist_usage(&rate);
                    }

                    return Ok(content);
                }
                401 | 403 => {
                    self.rate.lock().unwrap_or_else(|e| e.into_inner()).undo_reserve();
                    return Err(GeminiError::InvalidKey);
                }
                429 => {
                    self.rate.lock().unwrap_or_else(|e| e.into_inner()).undo_reserve();
                    return Err(GeminiError::RateLimited);
                }
                500 | 503 => {
                    // Transient server error (ErrTimeout, overloaded) — retry
                    let body = resp.text().unwrap_or_default();
                    last_error = Some(GeminiError::Api { status, message: body });
                    continue;
                }
                _ => {
                    let body = resp.text().unwrap_or_default();
                    return Err(GeminiError::Api { status, message: body });
                }
            }
        }

        // All retries exhausted
        self.rate.lock().unwrap_or_else(|e| e.into_inner()).undo_reserve();
        Err(last_error.unwrap_or_else(|| GeminiError::Api {
            status: 500,
            message: "Gemini server error after retries".into(),
        }))
    }

    /// Save rate tracker to disk if a persistence path is configured.
    fn persist_usage(&self, rate: &RateTracker) {
        if let Some(ref path) = self.usage_path {
            let persisted = rate.to_persisted();
            if let Ok(json) = serde_json::to_string(&persisted) {
                let _ = std::fs::write(path, json);
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
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
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
    fn test_remaining_quota() {
        let client = GeminiClient::new("fake-key", "gemini-2.5-flash").unwrap();
        assert_eq!(client.remaining_quota(), 250);
    }
}
