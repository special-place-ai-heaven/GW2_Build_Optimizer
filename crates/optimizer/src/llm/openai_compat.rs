//! Shared request core for OpenAI-compatible chat-completions providers.
//!
//! OpenAI and OpenRouter speak the same wire format; only the base URL,
//! identity headers, and OpenRouter-specific extras (reasoning caps,
//! provider routing preferences) differ. One streaming implementation, one
//! retry policy, one rate-tracker handshake — the wrappers add only what
//! makes them distinct.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::rate::RateTracker;
use super::sse::{read_stream, StreamedMessage};
use super::{LlmError, ToolDefinition};
use serde_json::Value;

/// Whole-request ceiling for streamed completions. Streams flow continuously
/// (OpenRouter interleaves `: OPENROUTER PROCESSING` keep-alive comments), so
/// a reasoning model that thinks for minutes no longer trips a short wall
/// clock that would abort valid requests mid-generation.
pub(crate) const REQUEST_TIMEOUT_SECS: u64 = 900;
pub(crate) const CONNECT_TIMEOUT_SECS: u64 = 15;
/// Completion ceiling per chat completion. Reasoning models spend the same
/// budget on hidden thinking, so the cap must cover both or the answer gets
/// truncated (or arrives empty) with finish_reason "length".
pub(crate) const MAX_COMPLETION_TOKENS: u32 = 16_384;
/// Upper bound on hidden reasoning tokens per request (OpenRouter
/// `reasoning.max_tokens`; ignored by providers without thinking support).
pub(crate) const REASONING_TOKEN_CAP: u32 = 8_192;

pub(crate) fn http_client() -> Result<reqwest::blocking::Client, LlmError> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .map_err(|e| LlmError::Http(e.to_string()))
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
    pub(crate) reasoning_max_tokens: Option<u32>,
    /// OpenRouter `provider.require_parameters` when tools are present.
    pub(crate) require_tool_endpoints: bool,
    /// Per-request wall-clock cap. Streams are keep-alived by the provider,
    /// so this bounds a dead/silent request, not a healthy generation.
    pub(crate) request_timeout: std::time::Duration,
    pub(crate) max_retries: u32,
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
        provider: Some(ProviderPrefs {
            require_parameters: core.require_tool_endpoints,
        }),
    };

    let url = format!("{}/chat/completions", core.base_url);
    let mut last_error: Option<LlmError> = None;
    let mut next_delay = std::time::Duration::from_secs(5);

    for attempt in 0..core.max_retries {
        if attempt > 0 {
            std::thread::sleep(next_delay);
            next_delay *= 2;
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
                    core.rate
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
                match read_stream(resp) {
                    Ok(StreamedMessage::Message(message)) => return Ok(message),
                    Ok(StreamedMessage::Empty(finish)) => {
                        // Nothing usable in a 200 — release the rate slot so
                        // this dead trip doesn't count against the daily cap.
                        core.rate
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .undo_reserve();
                        return Err(LlmError::Parse(format!(
                            "Empty response from {label} (finish_reason: {finish})",
                            label = core.label
                        )));
                    }
                    Err(e) => {
                        core.rate
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .undo_reserve();
                        return Err(e);
                    }
                }
            }
            401 => {
                core.rate
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .undo_reserve();
                return Err(LlmError::InvalidKey);
            }
            429 => {
                core.rate
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .undo_reserve();
                return Err(LlmError::RateLimited);
            }
            // Retryable: server + gateway + provider failures. 408/504 are
            // OpenRouter's "upstream didn't respond in time"; 529 is the
            // Anthropic overloaded signal normalized through their router.
            408 | 500 | 502 | 503 | 504 | 529 => {
                if let Some(secs) = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    next_delay = std::time::Duration::from_secs(secs.min(60));
                }
                let body = resp.text().unwrap_or_default();
                last_error = Some(LlmError::Api {
                    status,
                    message: body,
                });
                continue;
            }
            _ => {
                let body = resp.text().unwrap_or_default();
                core.rate
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

    core.rate
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .undo_reserve();
    Err(last_error.unwrap_or_else(|| LlmError::Api {
        status: 500,
        message: format!("{} server error after retries", core.label),
    }))
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Live repro of the in-game Choya hang: the real chat prompt builder at
    /// kitchen-brief scale, streamed with the production request shape.
    /// Ignored by default; run with OPENROUTER_API_KEY set:
    ///   cargo test -p gw2-optimizer live_hang_repro -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_hang_repro_big_prompt_streaming() {
        use std::sync::Mutex;
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

        let messages = vec![Message {
            role: "user".into(),
            content: Some(prompt),
            tool_calls: None,
            tool_call_id: None,
        }];
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
            require_tool_endpoints: false,
            request_timeout: std::time::Duration::from_secs(420),
            max_retries: 2,
        };

        let t0 = Instant::now();
        match send_chat(core, &messages, None) {
            Ok(msg) => println!(
                "OK in {:.1}s — content {} chars",
                t0.elapsed().as_secs_f64(),
                msg.content.as_deref().map(str::len).unwrap_or(0)
            ),
            Err(e) => {
                println!("ERR after {:.1}s: {e}", t0.elapsed().as_secs_f64());
                println!("DEBUG chain: {e:?}");
            }
        }
    }
}
