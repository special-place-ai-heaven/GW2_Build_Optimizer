//! Gemini API client for LLM-powered build reasoning.
//! Uses Google AI Studio's REST API (generativelanguage.googleapis.com).
//! API key is sent via x-goog-api-key header (not URL query) for security.
//! Includes response caching to minimize quota usage.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const GEMINI_GENERATE_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";
const GEMINI_MODELS_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models";

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

#[derive(Serialize)]
struct GenerateRequest {
    contents: Vec<Content>,
}

#[derive(Serialize, Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Deserialize)]
struct GenerateResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<Content>,
}

impl GeminiClient {
    pub fn new(api_key: &str) -> Result<Self, GeminiError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self {
            api_key: api_key.to_string(),
            http,
            cache: Mutex::new(HashMap::new()),
            rate: Mutex::new(RateTracker::new()),
            usage_path: None,
        })
    }

    /// Create a client with persistent rate tracking.
    /// Loads existing usage from `usage_path` and saves after each request.
    pub fn with_persistence(api_key: &str, usage_path: PathBuf) -> Result<Self, GeminiError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
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
        // Atomically check rate limit and reserve a slot
        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .check_and_reserve()?;

        let request = GenerateRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
        };

        let resp = match self
            .http
            .post(GEMINI_GENERATE_URL)
            .header("x-goog-api-key", &self.api_key)
            .json(&request)
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                // Request never reached server — release reserved slot
                self.rate.lock().unwrap_or_else(|e| e.into_inner()).undo_reserve();
                return Err(GeminiError::Http(e));
            }
        };

        let status = resp.status().as_u16();
        match status {
            200 => {} // parse body below
            401 | 403 => return Err(GeminiError::InvalidKey),
            429 => return Err(GeminiError::RateLimited),
            _ => {
                let body = resp.text().unwrap_or_default();
                return Err(GeminiError::Api { status, message: body });
            }
        }

        let body: GenerateResponse = resp.json()?;
        let text = body
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|c| c.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| GeminiError::Parse("No response text from Gemini".into()))?;

        // Persist usage after successful parse
        {
            let rate = self.rate.lock().unwrap_or_else(|e| e.into_inner());
            self.persist_usage(&rate);
        }

        Ok(text)
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
        let client = GeminiClient::new("fake-key").unwrap();
        assert_eq!(client.remaining_quota(), 250);
    }
}
