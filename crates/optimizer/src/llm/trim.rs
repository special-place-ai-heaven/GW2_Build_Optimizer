//! Token-budget heuristics for chat-refinement conversations.
//!
//! Tool-call loops in `generate_with_tools_progress` grow a message list
//! across turns. Without a guard, pathological tool-call chains can exceed
//! the provider context window (OpenAI 128K being the binding constraint).
//! This module provides a cheap estimator, a conservative budget, and the
//! trimmer for the OpenAI-compatible message shape. Anthropic keeps its own
//! trimmer because its turns pair differently (assistant blocks + a user
//! message of tool results, not assistant + N tool-role messages).

/// Rough token count. ASCII text ≈ 4 characters per token; non-ASCII
/// scripts (CJK, Cyrillic, Hangul, …) run ≈ 1 token per character. Byte
/// length alone under-counts CJK by 1.3-2x (3 bytes per char, but real
/// cost ~1 token), which let oversized prompts reach the API and get
/// rejected instead of trimmed. Slight over-estimates are safe — we trim
/// early, never late.
pub(crate) fn estimate_tokens(text: &str) -> usize {
    let ascii = text.bytes().filter(|&b| b.is_ascii()).count();
    let non_ascii = text.len() - ascii;
    ascii / 4 + non_ascii
}

/// Conservative prompt budget in estimated tokens. Sized for OpenAI's 128K
/// window with ~24K reserved for the response plus safety margin. Anthropic
/// (200K) and Gemini (1M) have more headroom; this guardrail is intentionally
/// a no-op for them in normal use.
pub(crate) const SAFE_PROMPT_BUDGET_TOKENS: usize = 100_000;

/// Drop oldest tool-call turn(s) when the conversation exceeds the token
/// budget. A "turn" is one assistant message with `tool_calls` plus the
/// tool-role messages that reference its ids; these must be dropped as an
/// atomic unit so the remaining assistant/tool pairing stays valid. The
/// initial user prompt (messages[0]) and the most recent turn are always
/// preserved.
///
/// One copy: `openai.rs` and `openrouter.rs` carried byte-identical 43-line
/// versions over the same `openai_compat::Message` type, so every fix had to
/// land twice (GLM F20).
pub(crate) fn trim_openai_messages(
    messages: &mut Vec<super::openai_compat::Message>,
    budget_tokens: usize,
) {
    use super::openai_compat::Message;

    fn message_tokens(m: &Message) -> usize {
        let content = m.content.as_deref().map(estimate_tokens).unwrap_or(0);
        let tool_calls = m
            .tool_calls
            .as_ref()
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| estimate_tokens(&tc.function.arguments))
                    .sum::<usize>()
            })
            .unwrap_or(0);
        content + tool_calls
    }

    let mut total: usize = messages.iter().map(message_tokens).sum();
    if total <= budget_tokens {
        return;
    }

    loop {
        let turn_starts: Vec<usize> = messages
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, m)| m.role == "assistant" && m.tool_calls.is_some())
            .map(|(i, _)| i)
            .collect();

        // Keep at least the initial prompt + the most recent turn.
        if turn_starts.len() < 2 {
            return;
        }

        messages.drain(turn_starts[0]..turn_starts[1]);
        total = messages.iter().map(message_tokens).sum();
        if total <= budget_tokens {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    // Intentional invariant tripwires.
    #![allow(clippy::assertions_on_constants)]
    use super::super::openai_compat::{FunctionCallData, Message, ToolCallResponse};
    use super::*;

    #[test]
    fn estimate_tokens_counts_cjk_per_char_not_per_byte() {
        // 10 CJK chars = 30 bytes. The old bytes/4 estimator said 7 tokens;
        // the real cost is ~10. chars-based estimate must meet or beat it.
        let cjk = "龘龘龘龘龘龘龘龘龘龘";
        assert_eq!(cjk.len(), 30);
        assert!(estimate_tokens(cjk) >= 10);
    }

    #[test]
    fn estimate_tokens_divides_by_four() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens(&"a".repeat(4000)), 1000);
    }

    #[test]
    fn budget_is_sane() {
        // Guard against accidental edits: budget must leave real response room
        // for OpenAI's 128K window, and stay conservative for safety margin.
        assert!(SAFE_PROMPT_BUDGET_TOKENS < 128_000);
        assert!(SAFE_PROMPT_BUDGET_TOKENS >= 50_000);
    }

    fn user_msg(text: &str) -> Message {
        Message {
            role: "user".into(),
            content: Some(text.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn assistant_with_call(id: &str, args: &str) -> Message {
        Message {
            role: "assistant".into(),
            content: None,
            tool_calls: Some(vec![ToolCallResponse {
                id: id.into(),
                call_type: "function".into(),
                function: FunctionCallData {
                    name: "get_trait_details".into(),
                    arguments: args.into(),
                },
            }]),
            tool_call_id: None,
        }
    }

    fn tool_result(id: &str, body: &str) -> Message {
        Message {
            role: "tool".into(),
            content: Some(body.into()),
            tool_calls: None,
            tool_call_id: Some(id.into()),
        }
    }

    #[test]
    fn test_trim_messages_drops_oldest_turn() {
        // Big filler (~400 chars = ~100 tokens each)
        let filler = "x".repeat(400);

        let mut messages = vec![
            user_msg("initial prompt"),
            assistant_with_call("call_1", &format!(r#"{{"q":"{}"}}"#, filler)),
            tool_result("call_1", &filler),
            assistant_with_call("call_2", &format!(r#"{{"q":"{}"}}"#, filler)),
            tool_result("call_2", &filler),
            assistant_with_call("call_3", &format!(r#"{{"q":"{}"}}"#, filler)),
            tool_result("call_3", &filler),
        ];
        let original_len = messages.len();

        // Budget of 200 tokens = 800 chars. Each turn is >= 200 chars of tool
        // args + 400 chars of result.
        trim_openai_messages(&mut messages, 200);

        assert!(
            messages.len() < original_len,
            "expected trimming, got {} messages",
            messages.len()
        );
        // Initial user prompt preserved.
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content.as_deref(), Some("initial prompt"));
        // Most recent turn preserved (the call_3 pair).
        let last_assistant_idx = messages
            .iter()
            .rposition(|m| m.role == "assistant")
            .expect("must retain an assistant message");
        let last_tc = messages[last_assistant_idx]
            .tool_calls
            .as_ref()
            .expect("assistant has tool_calls")
            .first()
            .unwrap();
        assert_eq!(last_tc.id, "call_3");
        // Every tool message still refers to a tool_call_id present on a
        // preceding assistant.
        for (i, m) in messages.iter().enumerate() {
            if m.role == "tool" {
                let id = m.tool_call_id.as_deref().unwrap();
                let found = messages[..i].iter().any(|prev| {
                    prev.tool_calls
                        .as_ref()
                        .is_some_and(|tcs| tcs.iter().any(|tc| tc.id == id))
                });
                assert!(found, "orphaned tool_call_id {}", id);
            }
        }
    }

    #[test]
    fn test_trim_messages_noop_under_budget() {
        let mut messages = vec![
            user_msg("short prompt"),
            Message {
                role: "assistant".into(),
                content: Some("short reply".into()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let original_len = messages.len();
        trim_openai_messages(&mut messages, 10_000);
        assert_eq!(messages.len(), original_len);
    }
}
