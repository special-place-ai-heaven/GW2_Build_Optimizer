//! Token-budget heuristics for chat-refinement conversations.
//!
//! Tool-call loops in `generate_with_tools_progress` grow a message list
//! across turns. Without a guard, pathological tool-call chains can exceed
//! the provider context window (OpenAI 128K being the binding constraint).
//! This module provides a cheap estimator and a conservative budget; each
//! provider implements turn-aware trimming locally because message shapes
//! differ.

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

#[cfg(test)]
mod tests {
    // Intentional invariant tripwires.
    #![allow(clippy::assertions_on_constants)]
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
}
