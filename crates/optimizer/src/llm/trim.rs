//! Token-budget heuristics for chat-refinement conversations.
//!
//! Tool-call loops in `generate_with_tools_progress` grow a message list
//! across turns. Without a guard, pathological tool-call chains can exceed
//! the provider context window (OpenAI 128K being the binding constraint).
//! This module provides a cheap estimator and a conservative budget; each
//! provider implements turn-aware trimming locally because message shapes
//! differ.

/// Rough token count: 1 token ≈ 4 characters. Good enough for budget
/// guarding; not suitable for billing or exact context accounting.
pub(crate) fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Conservative prompt budget in estimated tokens. Sized for OpenAI's 128K
/// window with ~24K reserved for the response plus safety margin. Anthropic
/// (200K) and Gemini (1M) have more headroom; this guardrail is intentionally
/// a no-op for them in normal use.
pub(crate) const SAFE_PROMPT_BUDGET_TOKENS: usize = 100_000;

#[cfg(test)]
mod tests {
    use super::*;

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
