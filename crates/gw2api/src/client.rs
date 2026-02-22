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
const MAX_RETRIES: u32 = 5;

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

    /// Take a token, returning the duration to sleep if the bucket was empty.
    /// The caller must sleep AFTER releasing the mutex lock to avoid blocking
    /// other threads that want to check/take tokens concurrently.
    fn take(&mut self) -> Option<Duration> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * REFILL_RATE).min(BUCKET_CAPACITY as f64);
        self.last_refill = now;

        if self.tokens < 1.0 {
            let wait = Duration::from_secs_f64((1.0 - self.tokens) / REFILL_RATE);
            self.tokens = 0.0;
            Some(wait)
        } else {
            self.tokens -= 1.0;
            None
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
    /// Builds query string manually to avoid URL-encoding commas in bulk ID requests.
    /// Retries on connection errors (timeouts) AND server errors (502/503/504).
    pub fn get_with_params<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<T, ApiError> {
        let base_url = if endpoint.starts_with("http") {
            endpoint.to_string()
        } else {
            format!("{}/{}", BASE_URL, endpoint.trim_start_matches('/'))
        };

        // Build query string manually — reqwest's .query() encodes commas as %2C,
        // which triples separator length and can exceed URL limits for bulk ID requests.
        let url = if params.is_empty() {
            base_url
        } else {
            let query = params.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            format!("{}?{}", base_url, query)
        };

        let mut last_error: Option<ApiError> = None;

        for attempt in 0..MAX_RETRIES {
            // Backoff before retries (not before first attempt)
            if attempt > 0 {
                let wait = Duration::from_millis(2000 * 2u64.pow(attempt - 1));
                std::thread::sleep(wait);
            }

            // Take a token — sleep OUTSIDE the lock to allow concurrent threads
            let sleep_dur = self.bucket.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(wait) = sleep_dur {
                std::thread::sleep(wait);
            }

            let mut headers = HeaderMap::new();
            headers.insert(
                USER_AGENT,
                HeaderValue::from_static("GW2BuildOptimizer/0.1"),
            );
            if let Some(ref key) = self.api_key {
                let header_val = match HeaderValue::from_str(&format!("Bearer {}", key)) {
                    Ok(v) => v,
                    Err(_) => return Err(ApiError::Api {
                        status: 0,
                        message: "API key contains invalid characters".into(),
                    }),
                };
                headers.insert(AUTHORIZATION, header_val);
            }

            // Connection errors (timeout, DNS, reset) are retryable — do NOT use `?`
            let resp = match self.http.get(&url).headers(headers).send() {
                Ok(resp) => resp,
                Err(e) => {
                    last_error = Some(ApiError::Http(e));
                    continue; // retry on connection failure
                }
            };

            let status = resp.status().as_u16();

            // Retry on rate limit and server errors (502/503/504)
            if status == 429 || status == 502 || status == 503 || status == 504 {
                let body = resp.text().unwrap_or_default();
                let clean_msg = if body.contains('<') {
                    format!("Server error (HTTP {})", status)
                } else if body.is_empty() {
                    format!("HTTP {}", status)
                } else {
                    body
                };
                last_error = Some(ApiError::Api { status, message: clean_msg });
                continue; // retry
            }

            if !resp.status().is_success() {
                let body = resp.text().unwrap_or_default();
                let clean_msg = if body.contains('<') {
                    format!("Server error (HTTP {})", status)
                } else {
                    body
                };
                return Err(ApiError::Api { status, message: clean_msg });
            }

            // Read body — connection can fail here too
            let text = match resp.text() {
                Ok(t) => t,
                Err(e) => {
                    last_error = Some(ApiError::Http(e));
                    continue; // retry on read failure
                }
            };

            let parsed: T = serde_json::from_str(&text)?;
            return Ok(parsed);
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| ApiError::Api {
            status: 0,
            message: "GW2 API unavailable after retries. Try again later.".into(),
        }))
    }

    /// Fetch all IDs from an endpoint root, then bulk-fetch in batches of 200.
    pub fn fetch_all<T: DeserializeOwned + Send>(&self, endpoint: &str) -> Result<Vec<T>, ApiError> {
        // First get all IDs
        let ids: Vec<serde_json::Value> = self.get(endpoint)?;
        self.fetch_by_ids(endpoint, &ids)
    }

    /// Fetch items by a list of IDs in batches of 200, up to 5 concurrent.
    pub fn fetch_by_ids<T: DeserializeOwned + Send>(
        &self,
        endpoint: &str,
        ids: &[serde_json::Value],
    ) -> Result<Vec<T>, ApiError> {
        let batches: Vec<&[serde_json::Value]> = ids.chunks(MAX_BULK_IDS).collect();
        let mut results = Vec::with_capacity(ids.len());

        // Process in groups of 5 concurrent fetches
        for group in batches.chunks(5) {
            let group_results: Vec<Result<Vec<T>, ApiError>> =
                std::thread::scope(|s| {
                    let handles: Vec<_> = group.iter().map(|chunk| {
                        s.spawn(|| {
                            let ids_str: Vec<String> = chunk.iter()
                                .map(|id| id.to_string().replace('"', ""))
                                .collect();
                            let joined = ids_str.join(",");
                            self.get_with_params::<Vec<T>>(endpoint, &[("ids", &joined)])
                        })
                    }).collect();

                    handles.into_iter().map(|h| {
                        h.join().unwrap_or_else(|_| Err(ApiError::Api {
                            status: 0,
                            message: "Batch fetch thread panicked".into(),
                        }))
                    }).collect()
                });

            for batch_result in group_results {
                results.extend(batch_result?);
            }
        }

        Ok(results)
    }

    /// Like `fetch_by_ids` but calls `on_progress(fetched_so_far, total)` after each batch group.
    pub fn fetch_by_ids_with_progress<T: DeserializeOwned + Send>(
        &self,
        endpoint: &str,
        ids: &[serde_json::Value],
        mut on_progress: impl FnMut(usize, usize),
    ) -> Result<Vec<T>, ApiError> {
        let batches: Vec<&[serde_json::Value]> = ids.chunks(MAX_BULK_IDS).collect();
        let total = ids.len();
        let mut results = Vec::with_capacity(total);

        for group in batches.chunks(5) {
            let group_results: Vec<Result<Vec<T>, ApiError>> =
                std::thread::scope(|s| {
                    let handles: Vec<_> = group.iter().map(|chunk| {
                        s.spawn(|| {
                            let ids_str: Vec<String> = chunk.iter()
                                .map(|id| id.to_string().replace('"', ""))
                                .collect();
                            let joined = ids_str.join(",");
                            self.get_with_params::<Vec<T>>(endpoint, &[("ids", &joined)])
                        })
                    }).collect();

                    handles.into_iter().map(|h| {
                        h.join().unwrap_or_else(|_| Err(ApiError::Api {
                            status: 0,
                            message: "Batch fetch thread panicked".into(),
                        }))
                    }).collect()
                });

            for batch_result in group_results {
                results.extend(batch_result?);
            }

            on_progress(results.len(), total);
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
        // Should not need to sleep when bucket is full
        let wait = bucket.take();
        assert!(wait.is_none(), "Full bucket should not require sleeping");
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
