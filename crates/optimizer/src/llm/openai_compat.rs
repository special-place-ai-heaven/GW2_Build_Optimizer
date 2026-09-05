//! Shared request core for OpenAI-compatible chat-completions providers.
//!
//! OpenAI and OpenRouter speak the same wire format; only the base URL,
//! identity headers, and OpenRouter-specific extras (reasoning caps,
//! provider routing preferences) differ. One streaming implementation, one
//! retry policy, one rate-tracker handshake — the wrappers add only what
//! makes them distinct.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::body::read_body_capped;
use super::cancel::{sleep_observing, CANCELLED};
use super::rate::RateTracker;
use super::sse::{read_stream, StreamedMessage};
use super::{LlmError, ToolDefinition};
use serde_json::Value;

/// Client-level ceiling. Streams flow continuously (OpenRouter interleaves
/// `: OPENROUTER PROCESSING` keep-alive comments), so a reasoning model that
/// thinks for minutes no longer trips a short wall clock that would abort
/// valid requests mid-generation. Every call sets a tighter per-request
/// timeout on top; this is only the outer bound.
pub(crate) const REQUEST_TIMEOUT_SECS: u64 = 900;
pub(crate) const CONNECT_TIMEOUT_SECS: u64 = 15;
/// Per-request wall clock for one streamed chat completion. Shared by every
/// provider so the worst-case unload wait does not depend on which provider
/// the user picked (Claude F32: 420 s / 420 s / 900 s / 180 s before).
///
/// This is a reqwest *total* deadline — connect through last body byte — not
/// an idle timeout. Keep-alives hold the server side open; they do not extend
/// the client deadline (GLM F14). 420 s is the budget for one completion.
pub(crate) const CHAT_REQUEST_TIMEOUT: Duration = Duration::from_secs(420);
/// Per-request wall clock for the metadata endpoints — key validation and the
/// model catalog. These are small, fast calls made from the Settings UI; they
/// used to ride the 900 s client default, so one hung endpoint stalled the
/// worker for fifteen minutes (GLM F14).
pub(crate) const METADATA_TIMEOUT: Duration = Duration::from_secs(20);
/// First retry backoff, doubled per attempt and clamped to
/// [`MAX_RETRY_DELAY`].
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(5);
/// Ceiling for a retry backoff, including a provider-supplied `Retry-After`.
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);
/// Completion ceiling per chat completion. Reasoning models spend the same
/// budget on hidden thinking, so the cap must cover both or the answer gets
/// truncated (or arrives empty) with finish_reason "length".
///
/// Raised from 16_384: a build is worth real tokens. The reasoning cap used
/// to be HALF of this, so a thinking model could spend the entire budget
/// deliberating and have nothing left to answer with. Designing a build from
/// live tool data — trait columns for three specs, skill facts, upgrade
/// ranking, then a rotation sim — is not a 16k job.
/// Whether a failure means "this model could not produce a usable function
/// call", rather than a transport, auth or quota problem.
///
/// Google answers a failed function call with HTTP 200 and
/// `native_finish_reason: MALFORMED_FUNCTION_CALL`, and OpenRouter attaches no
/// top-level `error` object because the provider itself succeeded. The stream
/// therefore ends empty and [`sse::read_stream`] reports it as a parse failure
/// carrying that reason. Measured 2026-09-05: gemini-3.8-flash failed this way
/// on every tool-carrying Choya request while answering toolless ones fine, and
/// glm-5.3-flash drove the same twenty declarations without trouble — so it is
/// the model, not the schema, and the loop can recover by dropping the tools.
pub(crate) fn is_function_call_failure(err: &LlmError) -> bool {
    let LlmError::Parse(message) = err else {
        return false;
    };
    message.to_ascii_uppercase().contains("MALFORMED_FUNCTION_CALL")
}

pub(crate) const MAX_COMPLETION_TOKENS: u32 = 65_536;
/// Upper bound on hidden reasoning tokens per request (OpenRouter
/// `reasoning.max_tokens`; ignored by providers without thinking support).
///
/// Deliberately well under [`MAX_COMPLETION_TOKENS`] so a long think always
/// leaves room for the build itself.
pub(crate) const REASONING_TOKEN_CAP: u32 = 32_768;

pub(crate) fn http_client() -> Result<reqwest::blocking::Client, LlmError> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .map_err(|e| LlmError::Http(e.to_string()))
}

/// Holds the rate slot taken by `check_and_reserve` and gives it back on drop
/// unless the request actually succeeded.
///
/// Every early return used to repeat the undo by hand, and the providers that
/// were copied from this one each missed a path — Anthropic and Gemini leak a
/// slot on mid-stream failure (GLM F16). A guard cannot miss a path.
pub(crate) struct RateReserve<'a> {
    rate: Option<&'a Mutex<RateTracker>>,
}

impl<'a> RateReserve<'a> {
    /// Wrap a slot that `check_and_reserve` has already taken.
    pub(crate) fn held(rate: &'a Mutex<RateTracker>) -> Self {
        Self { rate: Some(rate) }
    }

    /// The request succeeded: the slot stays spent.
    pub(crate) fn keep(&mut self) {
        self.rate = None;
    }
}

impl Drop for RateReserve<'_> {
    fn drop(&mut self) {
        if let Some(rate) = self.rate.take() {
            rate.lock()
                .unwrap_or_else(|e| e.into_inner())
                .undo_reserve();
        }
    }
}

/// `Retry-After` in delta-seconds, clamped to [`MAX_RETRY_DELAY`].
///
/// The HTTP-date form is not honored: none of these APIs send it, and a
/// mis-parsed date would be worse than the default backoff.
pub(crate) fn retry_after_delay(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let secs = headers
        .get("Retry-After")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(Duration::from_secs(secs).min(MAX_RETRY_DELAY))
}

/// Next backoff after one more failed attempt: doubled, never past the
/// ceiling. Shared so no provider grows an unbounded wait of its own.
pub(crate) fn doubled_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_RETRY_DELAY)
}

/// Whether an HTTP status is worth another attempt.
///
/// 429 is in the set: a rate limit with `Retry-After` is a normal traffic
/// shape on OpenRouter, and returning immediately turned one transient burst
/// into a user-visible failure (Grok F5, GLM F21). 408/504 are gateway
/// "upstream didn't respond in time"; 529 is Anthropic overloaded normalized
/// through the router.
pub(crate) fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 408 | 500 | 502 | 503 | 504 | 529)
}

/// Normalize a transport failure so a rate limit reads the same to the UI
/// whether it arrived as an HTTP status or inside a 200 body.
pub(crate) fn as_transport_error(status: u16, message: String) -> LlmError {
    if status == 429 {
        LlmError::RateLimited
    } else {
        LlmError::Api { status, message }
    }
}

// ─── OpenAI wire types ───

#[derive(Serialize)]
pub(crate) struct ChatRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens: Option<u32>,
    /// Always streamed: keep-alive comments hold the connection open while
    /// reasoning models think, and the first bytes land in seconds instead
    /// of after the whole generation.
    pub(crate) stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reasoning: Option<ReasoningConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider: Option<ProviderPrefs>,
}

/// OpenRouter `reasoning` parameter — caps hidden thinking so the completion
/// budget survives for the actual answer. Providers without thinking support
/// ignore unknown parameters (per OpenRouter's parameter docs).
#[derive(Serialize, Debug, Clone)]
pub(crate) struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens: Option<u32>,
}

/// OpenRouter `provider` routing preferences.
#[derive(Serialize, Debug, Clone)]
pub(crate) struct ProviderPrefs {
    /// Only route to endpoints that natively support every parameter in the
    /// request — never to one that fakes tools through a prompt template.
    pub(crate) require_parameters: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Message {
    pub(crate) role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<Vec<ToolCallResponse>>,
    /// For role="tool" messages: the ID of the tool call being responded to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub(crate) struct OpenAiTool {
    #[serde(rename = "type")]
    pub(crate) tool_type: String,
    pub(crate) function: OpenAiFunction,
}

#[derive(Serialize, Debug, Clone)]
pub(crate) struct OpenAiFunction {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) parameters: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct ToolCallResponse {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) call_type: String,
    pub(crate) function: FunctionCallData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct FunctionCallData {
    pub(crate) name: String,
    /// OpenAI sends arguments as a JSON *string*, not an object.
    pub(crate) arguments: String,
}

/// Everything the shared core needs to reach one provider.
pub(crate) struct ProviderCore<'a> {
    pub(crate) http: &'a reqwest::blocking::Client,
    pub(crate) rate: &'a Mutex<RateTracker>,
    pub(crate) api_key: &'a str,
    pub(crate) base_url: &'a str,
    pub(crate) model: &'a str,
    /// Static identity headers (OpenRouter's HTTP-Referer / X-Title).
    pub(crate) extra_headers: &'a [(&'static str, String)],
    /// Provider name for error strings ("OpenRouter", "OpenAI").
    pub(crate) label: &'a str,
    pub(crate) max_tokens: u32,
    /// OpenRouter `reasoning.max_tokens`. `None` omits the field.
    pub(crate) reasoning_max_tokens: Option<u32>,
    /// Whether this base URL understands the OpenRouter-only top-level
    /// `provider` block. `api.openai.com` rejects unknown top-level body
    /// arguments, so sending it there breaks the OpenAI provider outright
    /// (Claude F8). Capability, not a URL sniff: a self-hosted
    /// OpenAI-compatible gateway sets this from its own knowledge.
    pub(crate) supports_provider_prefs: bool,
    /// OpenRouter `provider.require_parameters` when tools are present.
    pub(crate) require_tool_endpoints: bool,
    /// Per-request wall-clock cap. This is a reqwest *total* deadline, not an
    /// idle timeout: provider keep-alives hold the connection open but do not
    /// extend it. See [`CHAT_REQUEST_TIMEOUT`].
    pub(crate) request_timeout: std::time::Duration,
    pub(crate) max_retries: u32,
    /// Polled between attempts, between backoff slices, and between stream
    /// lines so an unload does not have to wait out `request_timeout`.
    /// `&|| false` where cancellation is not meaningful.
    pub(crate) is_cancelled: &'a dyn Fn() -> bool,
}

/// One streamed chat completion with the shared retry policy.
///
/// Rate tracker handshake (reserve on entry, undo on every failure path)
/// lives here; response persistence stays with the caller on success.
pub(crate) fn send_chat(
    core: ProviderCore<'_>,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
) -> Result<Message, LlmError> {
    core.rate
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .check_and_reserve()?;

    // Every early return past this point owes the tracker an undo, so the
    // reserve is released exactly once by the guard's drop.
    let mut reserve = RateReserve::held(core.rate);

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
        model: core.model.to_string(),
        messages: messages.to_vec(),
        tools: openai_tools,
        max_tokens: Some(core.max_tokens),
        stream: Some(true),
        reasoning: core.reasoning_max_tokens.map(|t| ReasoningConfig {
            max_tokens: Some(t),
        }),
        // OpenRouter-only body field. `api.openai.com` rejects unknown
        // top-level arguments, so posting it to every OpenAI-compatible base
        // URL made the OpenAI provider fail outright (Claude F8).
        provider: core.supports_provider_prefs.then_some(ProviderPrefs {
            require_parameters: core.require_tool_endpoints,
        }),
    };

    let url = format!("{}/chat/completions", core.base_url);
    let mut last_error: Option<LlmError> = None;
    let mut next_delay = INITIAL_RETRY_DELAY;

    for attempt in 0..core.max_retries {
        if (core.is_cancelled)() {
            return Err(LlmError::Unavailable(CANCELLED.to_string()));
        }
        if attempt > 0 {
            if !sleep_observing(next_delay, core.is_cancelled) {
                return Err(LlmError::Unavailable(CANCELLED.to_string()));
            }
            next_delay = doubled_backoff(next_delay);
        }

        let mut req = core
            .http
            .post(&url)
            .timeout(core.request_timeout)
            .header("Authorization", format!("Bearer {}", core.api_key))
            .header("Content-Type", "application/json");
        for (name, value) in core.extra_headers {
            req = req.header(*name, value);
        }

        let resp = match req.json(&request).send() {
            Ok(r) => r,
            Err(e) => {
                if attempt == core.max_retries - 1 {
                    return Err(LlmError::Http(e.to_string()));
                }
                last_error = Some(LlmError::Http(e.to_string()));
                continue;
            }
        };

        let status = resp.status().as_u16();
        match status {
            200 => {
                match read_stream(resp, core.is_cancelled) {
                    Ok(StreamedMessage::Message(message)) => {
                        reserve.keep();
                        return Ok(message);
                    }
                    // Nothing usable in a 200 — the held reserve is released
                    // on drop so this dead trip doesn't count against quota.
                    Ok(StreamedMessage::Empty(finish)) => {
                        return Err(LlmError::Parse(format!(
                            "Empty response from {label} (finish_reason: {finish})",
                            label = core.label
                        )));
                    }
                    // Measured OpenRouter behaviour (2026-08-27): an upstream
                    // rate limit arrives as HTTP **200** carrying an unframed
                    // `{"error":{…,"code":429}}` body. Retrying only on the
                    // HTTP status would never fire on the provider that needs
                    // it most, so the in-band status gets the same policy.
                    Err(LlmError::Api { status, message })
                        if is_retryable_status(status) && attempt + 1 < core.max_retries =>
                    {
                        last_error = Some(as_transport_error(status, message));
                        continue;
                    }
                    Err(LlmError::Api { status, message }) => {
                        return Err(as_transport_error(status, message))
                    }
                    Err(e) => return Err(e),
                }
            }
            401 => return Err(LlmError::InvalidKey),
            status if is_retryable_status(status) => {
                if let Some(delay) = retry_after_delay(resp.headers()) {
                    next_delay = delay;
                }
                last_error = Some(as_transport_error(status, read_body_capped(resp)));
                continue;
            }
            _ => {
                return Err(LlmError::Api {
                    status,
                    message: read_body_capped(resp),
                })
            }
        }
    }

    Err(last_error.unwrap_or_else(|| LlmError::Api {
        status: 500,
        message: format!("{} server error after retries", core.label),
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn user(text: &str) -> Message {
        Message {
            role: "user".into(),
            content: Some(text.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Build one raw HTTP/1.1 response. `Connection: close` so each attempt
    /// gets its own connection and the script stays in lockstep with the
    /// retry loop.
    fn http_response(status_line: &str, headers: &[(&str, &str)], body: &str) -> String {
        let mut out = format!("HTTP/1.1 {status_line}\r\n");
        for (name, value) in headers {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
        out.push_str(&format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ));
        out
    }

    /// Read one request fully — headers plus body, `Content-Length` or
    /// chunked — before answering. A half-read request plus a closed socket
    /// is an RST that discards the response, which surfaces as a transport
    /// error instead of the status the script meant to send.
    /// Returns the request body, so a test can assert on what was actually
    /// posted rather than on a struct it built itself.
    fn drain_request(stream: &TcpStream) -> std::io::Result<String> {
        let mut reader = std::io::BufReader::new(stream.try_clone()?);
        let mut content_length = 0usize;
        let mut chunked = false;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Ok(String::new());
            }
            if line.trim_end_matches(['\r', '\n']).is_empty() {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(value) = lower.strip_prefix("content-length:") {
                content_length = value.trim().parse().unwrap_or(0);
            } else if let Some(value) = lower.strip_prefix("transfer-encoding:") {
                chunked = value.contains("chunked");
            }
        }
        let mut body = Vec::new();
        if chunked {
            loop {
                let mut size_line = String::new();
                if reader.read_line(&mut size_line)? == 0 {
                    break;
                }
                let size = usize::from_str_radix(size_line.trim(), 16).unwrap_or(0);
                // Chunk payload plus its trailing CRLF.
                let mut chunk = vec![0u8; size + 2];
                reader.read_exact(&mut chunk)?;
                if size == 0 {
                    break;
                }
                chunk.truncate(size);
                body.extend_from_slice(&chunk);
            }
        } else {
            body = vec![0u8; content_length];
            reader.read_exact(&mut body)?;
        }
        Ok(String::from_utf8_lossy(&body).into_owned())
    }

    /// Serve one scripted response on one connection.
    ///
    /// Nothing is read after the response: the request was already consumed in
    /// full, so half-closing is a clean FIN, and waiting for the client to
    /// close would park this thread while the next attempt is already
    /// connecting.
    fn serve_one(
        mut stream: TcpStream,
        response: &str,
        served: &AtomicUsize,
        bodies: &Mutex<Vec<String>>,
    ) -> std::io::Result<()> {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let body = drain_request(&stream)?;
        stream.write_all(response.as_bytes())?;
        stream.flush()?;
        // Record as soon as the response is on the wire — before the lingering
        // drain below — so `served()` and `bodies()` are already accurate by
        // the time the client returns and the test asserts on them.
        bodies.lock().unwrap_or_else(|e| e.into_inner()).push(body);
        served.fetch_add(1, Ordering::SeqCst);
        // Wait for the client to close before dropping the socket. Closing a
        // Windows socket that still has unread inbound data sends an RST, and
        // the client reports that as a send failure rather than the status we
        // just wrote (WSAECONNRESET, reproduced at ~20% without this). Safe to
        // park here: every connection has its own thread.
        let mut sink = Vec::new();
        let _ = (&stream).take(64 * 1024).read_to_end(&mut sink);
        let _ = stream.shutdown(Shutdown::Both);
        Ok(())
    }

    /// A scripted loopback HTTP server.
    ///
    /// Not a live API call and not a network dependency: it binds
    /// `127.0.0.1:0`, replays a fixed list of raw responses, and dies with
    /// the test process. `leaf-1.1.2` is removing real-network calls from the
    /// gw2api tests; this plants none.
    struct ScriptedServer {
        base_url: String,
        served: Arc<AtomicUsize>,
        /// Request bodies in the order they arrived.
        bodies: Arc<Mutex<Vec<String>>>,
        /// Held so the port stays bound for the whole test even after the
        /// script is exhausted. A refused connect would come back as a
        /// transport error and read like a retry that never happened.
        _listener: Arc<TcpListener>,
    }

    impl ScriptedServer {
        fn start(responses: Vec<String>) -> Self {
            let listener = Arc::new(TcpListener::bind("127.0.0.1:0").expect("bind loopback"));
            let port = listener.local_addr().expect("local addr").port();
            let served = Arc::new(AtomicUsize::new(0));
            let bodies = Arc::new(Mutex::new(Vec::new()));
            let counter = Arc::clone(&served);
            let recorder = Arc::clone(&bodies);
            let accepting = Arc::clone(&listener);
            // Detached: if the client stops early, the accept simply parks and
            // is reaped at process exit, so a failing assertion reports the
            // real problem instead of hanging the suite on a join.
            std::thread::spawn(move || {
                for response in responses {
                    let Ok((stream, _)) = accepting.accept() else {
                        return;
                    };
                    // One thread per connection: a client socket that lingers
                    // must never delay the accept for the next attempt.
                    let counter = Arc::clone(&counter);
                    let recorder = Arc::clone(&recorder);
                    std::thread::spawn(move || {
                        let _ = serve_one(stream, &response, &counter, &recorder);
                    });
                }
            });
            Self {
                base_url: format!("http://127.0.0.1:{port}"),
                served,
                bodies,
                _listener: listener,
            }
        }

        /// The JSON body of the Nth request that reached the server.
        fn posted_body(&self, index: usize) -> serde_json::Value {
            let bodies = self.bodies.lock().expect("bodies");
            let raw = bodies.get(index).expect("request was posted");
            serde_json::from_str(raw).expect("posted body is JSON")
        }

        fn served(&self) -> usize {
            self.served.load(Ordering::SeqCst)
        }
    }

    /// The exact body OpenRouter returns when an upstream provider rate-limits
    /// a stream it had already accepted. Measured 2026-08-27: HTTP **200**,
    /// `content-type: text/event-stream`, and a bare JSON error object with no
    /// `data:` framing anywhere in it. `metadata.raw` is a String carrying
    /// embedded JSON, not a nested object.
    const OPENROUTER_INBAND_429: &str = concat!(
        "{\"error\":{\"message\":\"Provider returned error\",\"code\":429,",
        "\"metadata\":{\"raw\":\"{\\\"detail\\\":\\\"temporarily rate-limited upstream\\\"}\",",
        "\"provider_name\":\"Io Net\",\"is_byok\":false,",
        "\"limit_source\":\"upstream_provider_shared_pool\"}}}"
    );

    fn inband_429() -> String {
        http_response(
            "200 OK",
            &[("Content-Type", "text/event-stream")],
            OPENROUTER_INBAND_429,
        )
    }

    /// The gateway-level shape: OpenRouter refuses before any stream starts,
    /// with a proper status and `application/json`. `Retry-After: 0` is what
    /// keeps this test instant — it pins the backoff at zero, and the
    /// doubling stays at zero from there.
    fn http_429() -> String {
        http_response(
            "429 Too Many Requests",
            &[("Retry-After", "0"), ("Content-Type", "application/json")],
            "{\"error\":{\"message\":\"rate limit exceeded\",\"code\":429}}",
        )
    }

    /// A healthy stream, including the keep-alive comments OpenRouter
    /// interleaves on every real response.
    fn good_stream() -> String {
        http_response(
            "200 OK",
            &[("Content-Type", "text/event-stream")],
            concat!(
                ": OPENROUTER PROCESSING\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"done\"}}]}\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n",
                "data: [DONE]\n",
            ),
        )
    }

    fn test_core<'a>(
        http: &'a reqwest::blocking::Client,
        rate: &'a Mutex<RateTracker>,
        base_url: &'a str,
        is_cancelled: &'a dyn Fn() -> bool,
    ) -> ProviderCore<'a> {
        ProviderCore {
            http,
            rate,
            api_key: "test-key",
            base_url,
            model: "test/model",
            extra_headers: &[],
            label: "OpenRouter",
            max_tokens: MAX_COMPLETION_TOKENS,
            reasoning_max_tokens: None,
            supports_provider_prefs: true,
            require_tool_endpoints: false,
            // Short: a hung mock must fail the test, not stall it for 420 s.
            request_timeout: Duration::from_secs(10),
            max_retries: 3,
            is_cancelled,
        }
    }

    /// Grok F5 / GLM F21 — 429 used to return immediately. Both real 429
    /// shapes must be retried, for the *same* provider: the gateway-level
    /// HTTP status, and the in-band error OpenRouter carries inside a 200
    /// `text/event-stream` after it has already accepted the stream.
    #[test]
    fn http_429_retries() {
        let http = http_client().expect("client");
        let no_cancel = || false;

        let server = ScriptedServer::start(vec![http_429(), inband_429(), good_stream()]);
        let rate = Mutex::new(RateTracker::new(60));
        let core = test_core(&http, &rate, &server.base_url, &no_cancel);
        let message = send_chat(core, &[user("hi")], None).expect("third attempt must succeed");

        assert_eq!(message.content.as_deref(), Some("done"));
        assert_eq!(
            server.served(),
            3,
            "both the HTTP 429 and the in-band 200/429 must have been retried"
        );
        assert_eq!(
            rate.lock().expect("rate").requests_this_minute(),
            1,
            "three round trips are still one logical request"
        );

        // Exhausting the attempts reports a rate limit, not a raw 502, and
        // hands the reserved slot back.
        let server = ScriptedServer::start(vec![http_429(), inband_429(), inband_429()]);
        let rate = Mutex::new(RateTracker::new(60));
        let core = test_core(&http, &rate, &server.base_url, &no_cancel);
        let error = send_chat(core, &[user("hi")], None).expect_err("all attempts rate limited");

        assert!(
            matches!(error, LlmError::RateLimited),
            "an in-band 429 must surface as RateLimited, got: {error}"
        );
        assert_eq!(server.served(), 3);
        assert_eq!(
            rate.lock().expect("rate").requests_this_minute(),
            0,
            "a failed request must not spend a rate slot"
        );
    }

    /// OpenRouter fails in two shapes and only one of them is worth another
    /// attempt. `403` (model access restricted), `404` (no endpoint matching
    /// the guardrails) and `400` (unknown model id) are permanent: retrying
    /// them spends the whole backoff window and reads to the user as a hang.
    /// Measured against `:free` models, where outright failure is common.
    #[test]
    fn permanent_failures_are_not_retried() {
        let http = http_client().expect("client");
        let no_cancel = || false;

        for (status_line, expected) in [
            ("403 Forbidden", 403u16),
            ("404 Not Found", 404),
            ("400 Bad Request", 400),
        ] {
            // Script three responses; only the first may be consumed.
            let refusal = http_response(
                status_line,
                &[("Content-Type", "application/json")],
                "{\"error\":{\"message\":\"No endpoints available\"}}",
            );
            let server = ScriptedServer::start(vec![refusal.clone(), refusal, good_stream()]);
            let rate = Mutex::new(RateTracker::new(60));
            let core = test_core(&http, &rate, &server.base_url, &no_cancel);

            let error = send_chat(core, &[user("hi")], None).expect_err("permanent failure");
            match error {
                LlmError::Api { status, .. } => assert_eq!(status, expected),
                other => panic!("expected Api {expected}, got: {other}"),
            }
            assert_eq!(
                server.served(),
                1,
                "{status_line} must fail on the first attempt, not burn the backoff"
            );
            assert_eq!(rate.lock().expect("rate").requests_this_minute(), 0);
        }
    }

    /// Claude F8, proved on the wire rather than on a struct: the flag has to
    /// change the bytes that actually leave the process. `OpenAiClient` sets
    /// `supports_provider_prefs: false`, `OpenRouterClient` sets `true`.
    #[test]
    fn provider_prefs_flag_controls_the_posted_body() {
        let http = http_client().expect("client");
        let no_cancel = || false;

        // OpenAI-shaped: no OpenRouter extensions may appear.
        let server = ScriptedServer::start(vec![good_stream()]);
        let rate = Mutex::new(RateTracker::new(60));
        let mut core = test_core(&http, &rate, &server.base_url, &no_cancel);
        core.supports_provider_prefs = false;
        core.reasoning_max_tokens = None;
        send_chat(core, &[user("hi")], None).expect("ok");

        let body = server.posted_body(0);
        assert!(
            body.get("provider").is_none(),
            "OpenAI must not receive the OpenRouter `provider` block: {body}"
        );
        assert!(
            body.get("reasoning").is_none(),
            "OpenAI must not receive the OpenRouter `reasoning` block: {body}"
        );
        assert_eq!(body["stream"], serde_json::json!(true));

        // OpenRouter-shaped: both extensions ride along.
        let server = ScriptedServer::start(vec![good_stream()]);
        let rate = Mutex::new(RateTracker::new(60));
        let mut core = test_core(&http, &rate, &server.base_url, &no_cancel);
        core.supports_provider_prefs = true;
        core.require_tool_endpoints = true;
        core.reasoning_max_tokens = Some(REASONING_TOKEN_CAP);
        send_chat(core, &[user("hi")], None).expect("ok");

        let body = server.posted_body(0);
        assert_eq!(
            body["provider"]["require_parameters"],
            serde_json::json!(true)
        );
        assert_eq!(
            body["reasoning"]["max_tokens"],
            serde_json::json!(REASONING_TOKEN_CAP)
        );
    }

    /// Claude F8 — the OpenRouter-only `provider` block was posted to
    /// `api.openai.com` too, where an unknown top-level argument is a 400.
    #[test]
    fn openai_request_omits_the_openrouter_provider_block() {
        let base = ChatRequest {
            model: "gpt-4o".into(),
            messages: vec![user("hi")],
            tools: None,
            max_tokens: Some(MAX_COMPLETION_TOKENS),
            stream: Some(true),
            reasoning: None,
            provider: None,
        };
        let body = serde_json::to_value(&base).expect("serializes");
        assert!(
            body.get("provider").is_none(),
            "OpenAI body must not carry `provider`: {body}"
        );
        assert!(body.get("reasoning").is_none());

        let routed = ChatRequest {
            provider: Some(ProviderPrefs {
                require_parameters: true,
            }),
            ..base
        };
        let body = serde_json::to_value(&routed).expect("serializes");
        assert_eq!(
            body["provider"]["require_parameters"],
            serde_json::json!(true),
            "OpenRouter still gets its routing preferences"
        );
    }

    #[test]
    fn retry_after_is_read_and_clamped() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert_eq!(retry_after_delay(&headers), None);

        headers.insert("Retry-After", "7".parse().expect("header"));
        assert_eq!(retry_after_delay(&headers), Some(Duration::from_secs(7)));

        headers.insert("Retry-After", "99999".parse().expect("header"));
        assert_eq!(retry_after_delay(&headers), Some(MAX_RETRY_DELAY));

        // HTTP-date form is not parsed; the default backoff beats a guess.
        headers.insert(
            "Retry-After",
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().expect("header"),
        );
        assert_eq!(retry_after_delay(&headers), None);
    }

    #[test]
    fn cancellation_aborts_before_the_request_is_sent() {
        let http = http_client().expect("client");
        let rate = Mutex::new(RateTracker::new(60));
        let cancelled = || true;
        // Port 1: reaching the socket at all would be the bug.
        let core = test_core(&http, &rate, "http://127.0.0.1:1", &cancelled);

        let error = send_chat(core, &[user("hi")], None).expect_err("cancel must abort");
        assert!(matches!(error, LlmError::Unavailable(ref m) if m == CANCELLED));
        assert_eq!(
            rate.lock().expect("rate").requests_this_minute(),
            0,
            "a cancelled request must give its rate slot back"
        );
    }

    /// Live repro of the in-game Choya hang: the real chat prompt builder at
    /// kitchen-brief scale, streamed with the production request shape.
    /// Ignored by default; run with OPENROUTER_API_KEY set:
    ///   cargo test -p gw2-optimizer live_hang_repro -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_hang_repro_big_prompt_streaming() {
        use std::time::Instant;

        let key = std::env::var("OPENROUTER_API_KEY").expect("set OPENROUTER_API_KEY");
        let rate = Mutex::new(RateTracker::new(60));
        let http = http_client().expect("client");

        let mut kitchen = String::from(
            "Mode: WvW \u{b7} Scale: Roam\nRole: Roamer \u{b7} Damage, Bruiser, Troll\nProfession: Druid\n\n",
        );
        for i in 0..200 {
            kitchen.push_str(&format!(
                "- Pantry item {i}: stat prefix notes, rune and sigil interactions,\n relic timing, trait synergy hints, rotation considerations.\n"
            ));
        }
        kitchen.push_str("\nRecent chat:\n- player: hello\n");
        let message = "I want to make a perfect druid roaming build which is a cross between a roamer and a troll build, prioritizing pure condition damage, lots of disable CC and great survivability/sustain.";
        let prompt = crate::prompts::chat_refinement_prompt_with_tools(
            "Druid", "WvW", message, &kitchen, "Choya",
        );
        println!("prompt bytes: {}", prompt.len());

        let messages = vec![user(&prompt)];
        let no_cancel = || false;
        let core = ProviderCore {
            http: &http,
            rate: &rate,
            api_key: &key,
            base_url: "https://openrouter.ai/api/v1",
            model: "z-ai/glm-5.3-flash",
            extra_headers: &[],
            label: "OpenRouter",
            max_tokens: MAX_COMPLETION_TOKENS,
            reasoning_max_tokens: Some(REASONING_TOKEN_CAP),
            supports_provider_prefs: true,
            require_tool_endpoints: false,
            request_timeout: CHAT_REQUEST_TIMEOUT,
            max_retries: 2,
            is_cancelled: &no_cancel,
        };

        let t0 = Instant::now();
        match send_chat(core, &messages, None) {
            Ok(msg) => println!(
                "OK in {:.1}s — content {} chars",
                t0.elapsed().as_secs_f64(),
                msg.content.as_deref().map(str::len).unwrap_or(0)
            ),
            Err(e) => {
                println!("ERR after {:.1}s: {e:?}", t0.elapsed().as_secs_f64());
            }
        }
    }
}
