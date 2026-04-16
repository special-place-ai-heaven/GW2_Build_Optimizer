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
/// Upper bound on honoring a server-sent `Retry-After`. Beyond this we
/// short-circuit to `ApiError::RateLimited` so background threads don't block
/// for minutes on an uninterruptible `thread::sleep`.
const RETRY_AFTER_CAP: Duration = Duration::from_secs(30);

/// Parse `Retry-After` as integer seconds (RFC 7231 delta-seconds form).
/// HTTP-date is intentionally unsupported — GW2 API returns integer seconds.
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let secs: u64 = headers.get("retry-after")?.to_str().ok()?.trim().parse().ok()?;
    Some(Duration::from_secs(secs))
}

/// UTF-8-safe truncation of response bodies to a short, loggable snippet.
/// Used by `ApiError::Api` to bound the message size; GW2 error payloads
/// are already short, but HTML from intermediaries can be arbitrarily large
/// (and is noise — we drop it entirely).
fn body_snippet(body: &str) -> String {
    const MAX: usize = 200;
    if body.contains('<') {
        return String::new();
    }
    body.chars().take(MAX).collect()
}

/// Build the comma-separated `ids` query value used by GW2 API bulk endpoints.
///
/// Produces output bit-for-bit identical to the previous inline construction
/// (`ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")`).
/// Does NOT enforce the 200-ID API cap — callers are responsible for chunking
/// (see `MAX_BULK_IDS` and `slice::chunks`).
pub(crate) fn build_bulk_ids_query(ids: &[u32]) -> String {
    let mut out = String::new();
    let mut first = true;
    for id in ids {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&id.to_string());
    }
    out
}

/// Convert a `serde_json::Value` ID into a `u32`, accepting either JSON numbers
/// or numeric JSON strings. Mirrors the previous `id.to_string().replace('"', "")`
/// behavior for the GW2 endpoints used by `fetch_by_ids` (all of which return
/// integer IDs in practice). Returns `None` for non-numeric values, which lets
/// callers `filter_map` and skip silently — matching prior behavior where a
/// non-numeric ID would have been sent verbatim and rejected by the API.
fn value_to_u32(v: &serde_json::Value) -> Option<u32> {
    if let Some(n) = v.as_u64() {
        u32::try_from(n).ok()
    } else if let Some(s) = v.as_str() {
        s.parse::<u32>().ok()
    } else {
        None
    }
}

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

/// Errors returned by the GW2 API client.
///
/// Variant conventions (see `code-review` skill for the binding rule):
/// - `Api` — GW2 API returned a non-2xx response. Always populates
///   `url_path` (the relative endpoint, e.g. `"items"`) and `body_snippet`
///   (≤200 chars, UTF-8 safe). Do NOT use for non-HTTP failures.
/// - `RateLimited` — 429 retries exhausted or `Retry-After` exceeded the
///   cap. Carries the endpoint that tripped the limit.
/// - `Cache` — on-disk cache read/write failure.
/// - `Internal` — panics, invalid config, unrecoverable client state.
///   Never a sentinel for HTTP errors.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("API error {status} on {url_path}: {body_snippet}")]
    Api {
        status: u16,
        url_path: String,
        body_snippet: String,
    },
    #[error("Rate limited after {retries} retries on {url_path}")]
    RateLimited { retries: u32, url_path: String },
    #[error("Missing required API scopes: {0:?}")]
    MissingScopes(Vec<String>),
    #[error("Cache error: {0}")]
    Cache(String),
    #[error("Internal error: {0}")]
    Internal(String),
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
        let http = Client::builder().timeout(Duration::from_secs(30)).build()?;
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
        let url_path = endpoint.to_string();

        // Build query string manually — reqwest's .query() encodes commas as %2C,
        // which triples separator length and can exceed URL limits for bulk ID requests.
        // We URL-encode values for safety but preserve commas (GW2 API uses them as
        // list separators in bulk ID requests).
        let url = if params.is_empty() {
            base_url
        } else {
            let query = params
                .iter()
                .map(|(k, v)| {
                    let encoded = urlencoding::encode(v).into_owned().replace("%2C", ",");
                    format!("{}={}", k, encoded)
                })
                .collect::<Vec<_>>()
                .join("&");
            format!("{}?{}", base_url, query)
        };

        let mut last_error: Option<ApiError> = None;
        // When Some, the next retry waits this duration instead of exponential
        // backoff — set by a 429 response with a `Retry-After` header.
        let mut suggested_wait: Option<Duration> = None;

        for attempt in 0..MAX_RETRIES {
            // Backoff before retries (not before first attempt)
            if attempt > 0 {
                let wait = suggested_wait.take().unwrap_or_else(|| {
                    Duration::from_millis(
                        (2000u64.saturating_mul(2u64.saturating_pow(attempt - 1))).min(30_000),
                    )
                });
                std::thread::sleep(wait);
            }

            // Take a token — sleep OUTSIDE the lock to allow concurrent threads.
            // Loop until we actually acquire a token (sleep may not refill enough).
            loop {
                let sleep_dur = self.bucket.lock().unwrap_or_else(|e| e.into_inner()).take();
                match sleep_dur {
                    None => break,
                    Some(wait) => std::thread::sleep(wait),
                }
            }

            let mut headers = HeaderMap::new();
            headers.insert(
                USER_AGENT,
                HeaderValue::from_static("GW2BuildOptimizer/0.1"),
            );
            if let Some(ref key) = self.api_key {
                let header_val = match HeaderValue::from_str(&format!("Bearer {}", key)) {
                    Ok(v) => v,
                    Err(_) => {
                        return Err(ApiError::Internal(
                            "API key contains invalid characters".into(),
                        ))
                    }
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

            // Retry on rate limit and server errors (502/503/504).
            // For 429, honor `Retry-After` when present; 5xx stays on exponential backoff.
            if status == 429 || status == 502 || status == 503 || status == 504 {
                if status == 429 {
                    let retry_after = parse_retry_after(resp.headers());
                    if let Some(wait) = retry_after {
                        if wait > RETRY_AFTER_CAP {
                            return Err(ApiError::RateLimited {
                                retries: attempt + 1,
                                url_path,
                            });
                        }
                        suggested_wait = Some(wait);
                    }
                    last_error = Some(ApiError::RateLimited {
                        retries: attempt + 1,
                        url_path: url_path.clone(),
                    });
                    continue;
                }
                let body = resp.text().unwrap_or_default();
                last_error = Some(ApiError::Api {
                    status,
                    url_path: url_path.clone(),
                    body_snippet: body_snippet(&body),
                });
                continue; // retry
            }

            if !resp.status().is_success() {
                let body = resp.text().unwrap_or_default();
                return Err(ApiError::Api {
                    status,
                    url_path,
                    body_snippet: body_snippet(&body),
                });
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
        Err(last_error.unwrap_or_else(|| {
            ApiError::Internal(format!(
                "GW2 API unavailable after {} retries on {}",
                MAX_RETRIES, url_path
            ))
        }))
    }

    /// Fetch all IDs from an endpoint root, then bulk-fetch in batches of 200.
    pub fn fetch_all<T: DeserializeOwned + Send>(
        &self,
        endpoint: &str,
    ) -> Result<Vec<T>, ApiError> {
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
            let group_results: Vec<Result<Vec<T>, ApiError>> = std::thread::scope(|s| {
                let handles: Vec<_> = group
                    .iter()
                    .map(|chunk| {
                        s.spawn(|| {
                            let numeric_ids: Vec<u32> =
                                chunk.iter().filter_map(value_to_u32).collect();
                            let joined = build_bulk_ids_query(&numeric_ids);
                            self.get_with_params::<Vec<T>>(endpoint, &[("ids", &joined)])
                        })
                    })
                    .collect();

                handles
                    .into_iter()
                    .map(|h| {
                        h.join().unwrap_or_else(|_| {
                            Err(ApiError::Internal(format!(
                                "Batch fetch thread panicked on {}",
                                endpoint
                            )))
                        })
                    })
                    .collect()
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
            let group_results: Vec<Result<Vec<T>, ApiError>> = std::thread::scope(|s| {
                let handles: Vec<_> = group
                    .iter()
                    .map(|chunk| {
                        s.spawn(|| {
                            let numeric_ids: Vec<u32> =
                                chunk.iter().filter_map(value_to_u32).collect();
                            let joined = build_bulk_ids_query(&numeric_ids);
                            self.get_with_params::<Vec<T>>(endpoint, &[("ids", &joined)])
                        })
                    })
                    .collect();

                handles
                    .into_iter()
                    .map(|h| {
                        h.join().unwrap_or_else(|_| {
                            Err(ApiError::Internal(format!(
                                "Batch fetch thread panicked on {}",
                                endpoint
                            )))
                        })
                    })
                    .collect()
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
        let endpoint = format!("characters/{}/buildtabs", urlencoding::encode(character));
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
    fn build_bulk_ids_query_empty_slice() {
        // Preserves prior behavior: empty Vec joined with "," → empty string.
        assert_eq!(build_bulk_ids_query(&[]), "");
    }

    #[test]
    fn build_bulk_ids_query_single_id() {
        assert_eq!(build_bulk_ids_query(&[42]), "42");
    }

    #[test]
    fn build_bulk_ids_query_exactly_200_ids() {
        // GW2 API max per bulk request. The helper itself does NOT enforce this
        // cap (chunking is upstream via `ids.chunks(MAX_BULK_IDS)`); it must
        // faithfully serialize whatever it's given.
        let ids: Vec<u32> = (1..=200).collect();
        let out = build_bulk_ids_query(&ids);
        let parts: Vec<&str> = out.split(',').collect();
        assert_eq!(parts.len(), 200);
        assert_eq!(parts[0], "1");
        assert_eq!(parts[199], "200");
        // Exactly 199 separators, no trailing comma.
        assert_eq!(out.matches(',').count(), 199);
        assert!(!out.starts_with(','));
        assert!(!out.ends_with(','));
    }

    #[test]
    fn build_bulk_ids_query_201_ids_passes_through() {
        // One over the GW2 API cap. The helper deliberately does NOT split or
        // error — chunking is the caller's responsibility (current call sites
        // chunk via `ids.chunks(MAX_BULK_IDS)` BEFORE invoking the helper).
        let ids: Vec<u32> = (1..=201).collect();
        let out = build_bulk_ids_query(&ids);
        assert_eq!(out.split(',').count(), 201);
        assert!(out.ends_with(",201"));
    }

    #[test]
    fn build_bulk_ids_query_u32_max() {
        assert_eq!(build_bulk_ids_query(&[u32::MAX]), "4294967295");
    }

    #[test]
    fn build_bulk_ids_query_matches_legacy_format_for_numeric_values() {
        // Bit-for-bit equivalence check against the previous inline construction
        // for the JSON-number IDs that real GW2 endpoints return.
        let values: Vec<serde_json::Value> =
            vec![1u32.into(), 42u32.into(), 100u32.into(), u32::MAX.into()];
        let legacy: String = values
            .iter()
            .map(|id| id.to_string().replace('"', ""))
            .collect::<Vec<_>>()
            .join(",");
        let numeric: Vec<u32> = values.iter().filter_map(value_to_u32).collect();
        assert_eq!(build_bulk_ids_query(&numeric), legacy);
    }

    #[test]
    fn value_to_u32_accepts_numbers_and_numeric_strings() {
        assert_eq!(value_to_u32(&serde_json::json!(42)), Some(42));
        assert_eq!(value_to_u32(&serde_json::json!("42")), Some(42));
        assert_eq!(value_to_u32(&serde_json::json!(u32::MAX)), Some(u32::MAX));
        assert_eq!(
            value_to_u32(&serde_json::json!((u32::MAX as u64) + 1)),
            None
        );
        assert_eq!(value_to_u32(&serde_json::json!(null)), None);
        assert_eq!(value_to_u32(&serde_json::json!("not-a-number")), None);
    }

    #[test]
    fn test_token_bucket_starts_full() {
        let mut bucket = TokenBucket::new();
        // Should not need to sleep when bucket is full
        let wait = bucket.take();
        assert!(wait.is_none(), "Full bucket should not require sleeping");
    }

    fn headers_with_retry_after(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("retry-after", HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn parse_retry_after_missing_header_is_none() {
        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }

    #[test]
    fn parse_retry_after_integer_seconds() {
        assert_eq!(
            parse_retry_after(&headers_with_retry_after("10")),
            Some(Duration::from_secs(10))
        );
    }

    #[test]
    fn parse_retry_after_zero_is_zero_duration() {
        assert_eq!(
            parse_retry_after(&headers_with_retry_after("0")),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn parse_retry_after_tolerates_whitespace() {
        assert_eq!(
            parse_retry_after(&headers_with_retry_after("  7  ")),
            Some(Duration::from_secs(7))
        );
    }

    #[test]
    fn parse_retry_after_rejects_http_date() {
        // HTTP-date form is valid per RFC 7231 but unsupported here.
        assert_eq!(
            parse_retry_after(&headers_with_retry_after("Wed, 21 Oct 2025 07:28:00 GMT")),
            None
        );
    }

    #[test]
    fn parse_retry_after_rejects_negative() {
        assert_eq!(parse_retry_after(&headers_with_retry_after("-1")), None);
    }

    #[test]
    fn parse_retry_after_rejects_garbage() {
        assert_eq!(parse_retry_after(&headers_with_retry_after("abc")), None);
    }

    #[test]
    fn body_snippet_truncates_long_text_utf8_safely() {
        // 300 four-byte chars × "💀" — naive byte slice at 200 would panic; chars().take must cap.
        let input: String = std::iter::repeat('💀').take(300).collect();
        let out = body_snippet(&input);
        assert_eq!(out.chars().count(), 200);
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn body_snippet_strips_html_payloads() {
        // Intermediary HTML error pages are noise — keep them out of ApiError.
        let html = "<html><body>Gateway timeout</body></html>";
        assert_eq!(body_snippet(html), "");
    }

    #[test]
    fn body_snippet_preserves_short_plain_text() {
        assert_eq!(body_snippet("not found"), "not found");
        assert_eq!(body_snippet(""), "");
    }

    #[test]
    fn get_with_params_429_then_200_succeeds_with_retry_after() {
        // Mock server: first GET /v2/mock returns 429 + Retry-After: 1, second returns 200.
        // We assert the retry path completes successfully, not precise timing — the
        // proactive token bucket and OS sleep granularity make timing assertions flaky.
        let mut server = mockito::Server::new();
        let m1 = server
            .mock("GET", "/mock")
            .with_status(429)
            .with_header("retry-after", "1")
            .with_body("rate limited")
            .expect(1)
            .create();
        let m2 = server
            .mock("GET", "/mock")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{\"ok\":true}")
            .expect(1)
            .create();

        let client = Gw2Client::without_key().unwrap();
        let url = format!("{}/mock", server.url());
        let resp: serde_json::Value = client.get_with_params(&url, &[]).unwrap();
        assert_eq!(resp["ok"], true);
        m1.assert();
        m2.assert();
    }

    #[test]
    fn get_with_params_429_over_cap_short_circuits_to_rate_limited() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/mock")
            .with_status(429)
            .with_header("retry-after", "3600") // 1h, well over RETRY_AFTER_CAP
            .with_body("rate limited")
            .expect(1) // must NOT retry
            .create();

        let client = Gw2Client::without_key().unwrap();
        let url = format!("{}/mock", server.url());
        let err = client
            .get_with_params::<serde_json::Value>(&url, &[])
            .unwrap_err();
        match err {
            ApiError::RateLimited { retries, url_path } => {
                assert_eq!(retries, 1);
                assert_eq!(url_path, url);
            }
            other => panic!("expected RateLimited, got {:?}", other),
        }
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
        let stats: Vec<super::super::models::ItemStat> = client
            .get_with_params("itemstats", &[("ids", "584")])
            .unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].name, "Berserker's");
    }

    #[test]
    #[ignore] // Requires network
    fn test_live_fetch_pvp_amulets_all() {
        // Shape smoke-test for the PvP amulet endpoint. Fetches every amulet
        // in one bulk request and asserts the list deserializes and at least
        // one amulet exposes stat attributes — catches drift in a low-traffic
        // endpoint that ItemStat tests do not cover.
        let client = Gw2Client::without_key().unwrap();
        let amulets: Vec<super::super::models::PvpAmulet> = client
            .get_with_params("pvp/amulets", &[("ids", "all")])
            .unwrap();
        assert!(!amulets.is_empty(), "expected non-empty amulet list");
        assert!(
            amulets.iter().any(|a| !a.attributes.is_empty()),
            "expected at least one amulet with attributes populated"
        );
    }

    #[test]
    #[ignore] // Requires network
    fn test_live_fetch_legendary_dual_stat_weapon() {
        // Sunrise (id=30704) — canonical legendary greatsword from launch.
        // Legendary weapons expose `stat_choices` in place of a fixed
        // `infix_upgrade`; this guards the shape optimizer gear resolution
        // relies on.
        let client = Gw2Client::without_key().unwrap();
        let items: Vec<super::super::models::Item> = client
            .get_with_params("items", &[("ids", "30704")])
            .unwrap();
        assert_eq!(items.len(), 1);
        let sunrise = &items[0];
        assert_eq!(sunrise.rarity, "Legendary");
        assert_eq!(sunrise.item_type, "Weapon");
        let details = sunrise
            .details
            .as_ref()
            .expect("legendary weapon must carry details");
        assert!(
            !details.stat_choices.is_empty(),
            "legendary weapon must expose stat_choices"
        );
    }

    #[test]
    #[ignore] // Requires network
    fn test_live_fetch_relic() {
        // SotO-era relics replaced the 7th rune bonus. 100947 is Relic of the
        // Thief. If Anet ever retires this exact ID, replace it — the test
        // still earns its keep by guarding the `Relic` item_type variant,
        // which only showed up in the API post-2023.
        let client = Gw2Client::without_key().unwrap();
        let items: Vec<super::super::models::Item> = client
            .get_with_params("items", &[("ids", "100947")])
            .unwrap();
        assert_eq!(items.len(), 1);
        let relic = &items[0];
        assert_eq!(relic.item_type, "Relic");
        assert!(!relic.name.is_empty());
    }
}
