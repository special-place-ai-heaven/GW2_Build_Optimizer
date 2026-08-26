//! Shared SSE parsing for streaming chat completions.
//!
//! OpenAI-compatible providers (OpenAI, OpenRouter) stream `data:` lines
//! with OpenAI-shaped deltas. Keep-alive comments and blank lines are
//! skipped, content and tool-call deltas accumulate (parallel calls merged
//! by index, fragmented JSON arguments stitched back together), `[DONE]`
//! ends the stream, and a top-level `error` payload is the mid-stream
//! failure channel (the HTTP status is already 200 by then — see
//! OpenRouter's errors-and-debugging docs). Anthropic's event-based SSE
//! will need an adapter on top of these primitives.

use super::openrouter::{FunctionCallData, Message, ToolCallResponse};
use super::LlmError;
use std::io::BufRead;

use serde::Deserialize;
use serde_json::Value;

/// One `data:` payload from a streamed chat completion.
#[derive(Deserialize)]
pub(crate) struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    /// Top-level error for mid-stream failures — the HTTP status is already
    /// 200 by then, so OpenRouter ships the failure in-band.
    #[serde(default)]
    error: Option<Value>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<StreamToolCallDelta>>,
}

#[derive(Deserialize)]
struct StreamToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<StreamFunctionDelta>,
}

#[derive(Deserialize)]
struct StreamFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

/// Accumulates one streamed assistant message from ordered deltas.
#[derive(Default)]
pub(crate) struct StreamAccumulator {
    content: String,
    tool_calls: Vec<StreamToolCall>,
}

#[derive(Default)]
struct StreamToolCall {
    id: String,
    name: String,
    arguments: String,
}

/// What a completed stream produced.
#[derive(Debug)]
pub(crate) enum StreamedMessage {
    Message(Message),
    /// HTTP 200 but nothing usable came out; carries the finish reason.
    Empty(String),
}

pub(crate) fn apply_chunk(
    acc: &mut StreamAccumulator,
    finish: &mut Option<String>,
    chunk: StreamChunk,
) {
    for choice in chunk.choices {
        if let Some(reason) = choice.finish_reason {
            *finish = Some(reason);
        }
        let Some(delta) = choice.delta else { continue };
        if let Some(text) = delta.content {
            acc.content.push_str(&text);
        }
        for call in delta.tool_calls.unwrap_or_default() {
            if acc.tool_calls.len() <= call.index {
                acc.tool_calls
                    .resize_with(call.index + 1, StreamToolCall::default);
            }
            let slot = &mut acc.tool_calls[call.index];
            if let Some(id) = call.id {
                slot.id.push_str(&id);
            }
            if let Some(function) = call.function {
                if let Some(name) = function.name {
                    slot.name.push_str(&name);
                }
                if let Some(args) = function.arguments {
                    slot.arguments.push_str(&args);
                }
            }
        }
    }
}

impl StreamAccumulator {
    /// `None` when the stream carried neither content nor tool calls.
    fn into_message(self) -> Option<Message> {
        let tool_calls = self
            .tool_calls
            .into_iter()
            .filter(|call| !call.name.is_empty())
            .map(|call| ToolCallResponse {
                id: call.id,
                call_type: "function".to_string(),
                function: FunctionCallData {
                    name: call.name,
                    arguments: call.arguments,
                },
            })
            .collect::<Vec<_>>();
        let content = (!self.content.is_empty()).then_some(self.content);
        if content.is_none() && tool_calls.is_empty() {
            return None;
        }
        Some(Message {
            role: "assistant".to_string(),
            content,
            tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            tool_call_id: None,
        })
    }
}

/// Reads an SSE chat-completions body into one assistant message.
pub(crate) fn read_stream<R: std::io::Read>(reader: R) -> Result<StreamedMessage, LlmError> {
    let mut acc = StreamAccumulator::default();
    let mut finish: Option<String> = None;

    for line in std::io::BufReader::new(reader).lines() {
        let line = line.map_err(|e| LlmError::Http(e.to_string()))?;
        let line = line.trim_end();
        if !line.starts_with("data: ") {
            continue;
        }
        let data = &line["data: ".len()..];
        if data == "[DONE]" {
            break;
        }
        let chunk: StreamChunk = match serde_json::from_str(data) {
            Ok(chunk) => chunk,
            Err(_) => continue,
        };
        if let Some(err) = chunk.error {
            let status = err.get("code").and_then(Value::as_u64).unwrap_or(502) as u16;
            return Err(LlmError::Api {
                status,
                message: err.to_string(),
            });
        }
        apply_chunk(&mut acc, &mut finish, chunk);
    }

    match acc.into_message() {
        Some(message) => Ok(StreamedMessage::Message(message)),
        None => Ok(StreamedMessage::Empty(
            finish.unwrap_or_else(|| "unknown".to_string()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_stream_accumulates_content_and_skips_keepalives() {
        let sse = concat!(
            ": OPENROUTER PROCESSING\n",
            "\n",
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n",
            ": OPENROUTER PROCESSING\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"total_tokens\":9}}\n",
            "data: [DONE]\n",
            "data: {\"ignored\":\"after done\"}\n",
        );
        match read_stream(sse.as_bytes()).expect("ok") {
            StreamedMessage::Message(m) => {
                assert_eq!(m.role, "assistant");
                assert_eq!(m.content.as_deref(), Some("Hello"));
                assert!(m.tool_calls.is_none());
            }
            StreamedMessage::Empty(finish) => panic!("expected content, got empty: {finish}"),
        }
    }

    #[test]
    fn test_read_stream_merges_fragmented_parallel_tool_calls() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"a\",\"arguments\":\"{\\\"x\\\":\"}}]}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"c2\",\"function\":{\"name\":\"b\",\"arguments\":\"{}\"}}]}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"1}\"}}]}}]}\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n",
            "data: [DONE]\n",
        );
        match read_stream(sse.as_bytes()).expect("ok") {
            StreamedMessage::Message(m) => {
                let calls = m.tool_calls.expect("tool calls");
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].id, "c1");
                assert_eq!(calls[0].function.name, "a");
                assert_eq!(calls[0].function.arguments, "{\"x\":1}");
                assert_eq!(calls[1].id, "c2");
                assert_eq!(calls[1].function.arguments, "{}");
            }
            StreamedMessage::Empty(_) => panic!("expected tool calls"),
        }
    }

    #[test]
    fn test_read_stream_mid_stream_error_maps_to_api_error() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n",
            "data: {\"id\":\"x\",\"error\":{\"code\":429,\"message\":\"Rate limit exceeded\",\"metadata\":{\"error_type\":\"rate_limit_exceeded\"}},\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":\"error\"}]}\n",
        );
        match read_stream(sse.as_bytes()).expect_err("mid-stream error must fail") {
            LlmError::Api { status, message } => {
                assert_eq!(status, 429);
                assert!(message.contains("Rate limit exceeded"));
            }
            other => panic!("expected Api error, got: {other}"),
        }
    }

    #[test]
    fn test_read_stream_empty_body_reports_finish_reason() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n",
            "data: [DONE]\n",
        );
        match read_stream(sse.as_bytes()).expect("ok") {
            StreamedMessage::Empty(finish) => assert_eq!(finish, "length"),
            StreamedMessage::Message(_) => panic!("expected empty"),
        }
    }
}
