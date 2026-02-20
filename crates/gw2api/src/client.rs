//! GW2 API v2 HTTP client with rate limiting.
//! Rate limit: 300 burst, 5 tokens/sec refill. Max 200 IDs per bulk request.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use serde::de::DeserializeOwned;

const BASE_URL: &str = "https://api.guildwars2.com/v2";
const MAX_BULK_IDS: usize = 200;
const BUCKET_CAPACITY: u32 = 300;
const REFILL_RATE: f64 = 5.0; // tokens per second
const MAX_RETRIES: u32 = 3;

/// Rate-limited GW2 API client.
pub struct Gw2Client {
    http: Client,
    api_key: Option<String>,
    bucket: Mutex<TokenBucket>,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new() -> Self {
        Self {
            tokens: BUCKET_CAPACITY as f64,
            last_refill: Instant::now(),
        }
    }

    fn take(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * REFILL_RATE).min(BUCKET_CAPACITY as f64);
        self.last_refill = now;

        if self.tokens < 1.0 {
            let wait = Duration::from_secs_f64((1.0 - self.tokens) / REFILL_RATE);
            std::thread::sleep(wait);
            self.last_refill = Instant::now();
            self.tokens = 0.0;
        } else {
            self.tokens -= 1.0;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("Rate limited after {0} retries")]
    RateLimited(u32),
    #[error("Missing required API scopes: {0:?}")]
    MissingScopes(Vec<String>),
}

/// Token info returned by /v2/tokeninfo.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub permissions: Vec<String>,
}

/// Game build number from /v2/build.
#[derive(Debug, Clone, serde::Deserialize)]
struct BuildInfo {
    id: u32,
}

impl Gw2Client {
    pub fn new(api_key: Option<String>) -> Result<Self, ApiError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            api_key,
            bucket: Mutex::new(TokenBucket::new()),
        })
    }

    pub fn with_key(api_key: &str) -> Result<Self, ApiError> {
        Self::new(Some(api_key.to_string()))
    }

    pub fn without_key() -> Result<Self, ApiError> {
        Self::new(None)
    }

    /// Make a GET request to the API with rate limiting and retries.
    pub fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T, ApiError> {
        self.get_with_params(endpoint, &[])
    }

    /// Make a GET request with query parameters.
    pub fn get_with_params<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<T, ApiError> {
        let url = if endpoint.starts_with("http") {
            endpoint.to_string()
        } else {
            format!("{}/{}", BASE_URL, endpoint.trim_start_matches('/'))
        };

        for attempt in 0..MAX_RETRIES {
            self.bucket.lock().unwrap_or_else(|e| e.into_inner()).take();

            let mut headers = HeaderMap::new();
            headers.insert(
                USER_AGENT,
                HeaderValue::from_static("GW2BuildOptimizer/0.1"),
            );
            if let Some(ref key) = self.api_key {
                let header_val = HeaderValue::from_str(&format!("Bearer {}", key))
                    .map_err(|_| ApiError::Api {
                        status: 0,
                        message: "API key contains invalid characters".into(),
                    })?;
                headers.insert(AUTHORIZATION, header_val);
            }

            let resp = self
                .http
                .get(&url)
                .headers(headers)
                .query(params)
                .send()?;

            let status = resp.status().as_u16();

            if status == 429 {
                // Rate limited — exponential backoff
                let wait = Duration::from_millis(1000 * 2u64.pow(attempt));
                std::thread::sleep(wait);
                continue;
            }

            if !resp.status().is_success() {
                let body = resp.text().unwrap_or_default();
                return Err(ApiError::Api {
                    status,
                    message: body,
                });
            }

            let text = resp.text()?;
            let parsed: T = serde_json::from_str(&text)?;
            return Ok(parsed);
        }

        Err(ApiError::RateLimited(MAX_RETRIES))
    }

    /// Fetch all IDs from an endpoint root, then bulk-fetch in batches of 200.
    pub fn fetch_all<T: DeserializeOwned>(&self, endpoint: &str) -> Result<Vec<T>, ApiError> {
        // First get all IDs
        let ids: Vec<serde_json::Value> = self.get(endpoint)?;
        self.fetch_by_ids(endpoint, &ids)
    }

    /// Fetch items by a list of IDs in batches of 200.
    pub fn fetch_by_ids<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        ids: &[serde_json::Value],
    ) -> Result<Vec<T>, ApiError> {
        let mut results = Vec::with_capacity(ids.len());

        for chunk in ids.chunks(MAX_BULK_IDS) {
            let ids_str: Vec<String> = chunk.iter().map(|id| id.to_string().replace('"', "")).collect();
            let joined = ids_str.join(",");
            let batch: Vec<T> = self.get_with_params(endpoint, &[("ids", &joined)])?;
            results.extend(batch);
        }

        Ok(results)
    }

    /// Get the current game build number (used for cache invalidation).
    pub fn get_build_number(&self) -> Result<u32, ApiError> {
        let info: BuildInfo = self.get("build")?;
        Ok(info.id)
    }

    /// Validate the client's API key and return token info.
    /// Checks that required scopes (account, characters, builds) are present.
    pub fn validate_api_key(&self) -> Result<TokenInfo, ApiError> {
        let info: TokenInfo = self.get("tokeninfo")?;

        let required = ["account", "characters", "builds"];
        let missing: Vec<String> = required
            .iter()
            .filter(|s| !info.permissions.contains(&s.to_string()))
            .map(|s| s.to_string())
            .collect();

        if !missing.is_empty() {
            return Err(ApiError::MissingScopes(missing));
        }

        Ok(info)
    }

    /// Fetch character names (requires authenticated client).
    pub fn fetch_characters(&self) -> Result<Vec<String>, ApiError> {
        self.get("characters")
    }

    /// Fetch build tabs for a character.
    pub fn fetch_build_tabs(
        &self,
        character: &str,
    ) -> Result<Vec<super::models::BuildTab>, ApiError> {
        let endpoint = format!(
            "characters/{}/buildtabs",
            urlencoding::encode(character)
        );
        self.get_with_params(&endpoint, &[("tabs", "all")])
    }

    /// Fetch equipment tabs for a character.
    pub fn fetch_equipment_tabs(
        &self,
        character: &str,
    ) -> Result<Vec<super::models::EquipmentTab>, ApiError> {
        let endpoint = format!(
            "characters/{}/equipmenttabs",
            urlencoding::encode(character)
        );
        self.get_with_params(&endpoint, &[("tabs", "all")])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_starts_full() {
        let mut bucket = TokenBucket::new();
        // Should not sleep when bucket is full
        let start = Instant::now();
        bucket.take();
        assert!(start.elapsed() < Duration::from_millis(10));
    }

    #[test]
    #[ignore] // Requires network
    fn test_live_fetch_build_number() {
        let client = Gw2Client::without_key().unwrap();
        let build = client.get_build_number().unwrap();
        assert!(build > 100000); // Build numbers are large
    }

    #[test]
    #[ignore] // Requires network
    fn test_live_fetch_berserkers_itemstat() {
        let client = Gw2Client::without_key().unwrap();
        let stats: Vec<super::super::models::ItemStat> =
            client.get_with_params("itemstats", &[("ids", "584")]).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "Berserker's");
    }
}
