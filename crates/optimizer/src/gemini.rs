//! Gemini API client for LLM-powered build reasoning.
//! Uses Google AI Studio's REST API (generativelanguage.googleapis.com).
//! API key is sent via x-goog-api-key header (not URL query) for security.
//! Includes response caching to minimize quota usage.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

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
}

struct CachedResponse {
    text: String,
    cached_at: Instant,
}

struct RateTracker {
    requests_this_minute: u32,
    minute_start: Instant,
    requests_today: u32,
}

impl RateTracker {
    fn new() -> Self {
        Self {
            requests_this_minute: 0,
            minute_start: Instant::now(),
            requests_today: 0,
        }
    }

    fn check(&mut self) -> Result<(), GeminiError> {
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
        Ok(())
    }

    fn record_success(&mut self) {
        self.requests_this_minute += 1;
        self.requests_today += 1;
    }

    fn remaining_today(&self) -> u32 {
        250u32.saturating_sub(self.requests_today)
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
        // Pre-check rate limit (don't increment yet)
        self.rate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .check()?;

        let request = GenerateRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
        };

        let resp = self
            .http
            .post(GEMINI_GENERATE_URL)
            .header("x-goog-api-key", &self.api_key)
            .json(&request)
            .send()?;

        let status = resp.status().as_u16();
        match status {
            200 => {
                // Only count successful requests against quota
                self.rate
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .record_success();
            }
            401 | 403 => return Err(GeminiError::InvalidKey),
            429 => return Err(GeminiError::RateLimited),
            _ => {
                let body = resp.text().unwrap_or_default();
                return Err(GeminiError::Api { status, message: body });
            }
        }

        let body: GenerateResponse = resp.json()?;
        body.candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|c| c.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| GeminiError::Parse("No response text from Gemini".into()))
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
            assert!(tracker.check().is_ok());
            tracker.record_success();
        }
        // 11th check should fail (10 RPM limit)
        assert!(tracker.check().is_err());
    }

    #[test]
    fn test_rate_tracker_daily_limit() {
        let mut tracker = RateTracker::new();
        tracker.requests_today = 240;
        assert!(tracker.check().is_err());
    }

    #[test]
    fn test_remaining_quota() {
        let client = GeminiClient::new("fake-key").unwrap();
        assert_eq!(client.remaining_quota(), 250);
    }
}
