# Todo

- **Snapshot tests for every prompt builder** — `prompts.rs` has 11
  prompt-constructing functions. Tests cover parsing but not construction.
  Add `insta`-style snapshots for `new_build_prompt_with_tools`,
  `synergy_build_prompt`, `improve_build_prompt_with_tools`,
  `chat_refinement_prompt_with_tools`. Drift becomes diff-visible.

- **Tool-call ID round-trip test** — `ToolCall` flows provider →
  `gemini_tools::ToolContext` → back. A dropped / reassigned
  `tool_call_id` silently breaks OpenAI. Add a focused test per provider
  with a mock server.

- **Live-LLM smoke suite** — one command that runs `live_llm.rs` against
  all three providers with a canonical build request and asserts the
  response validates. Catches API-shape drift in a single check.

- **Validation diagnostic output** — on rejection, ensure the error says
  *why* precisely enough that the LLM can correct itself on retry. Extend
  `ChangeEntry` to carry a machine-readable reason.

- ~~**Unified retry/backoff shim**~~ — Done. `llm/retry.rs` provides
  generic `with_retries<F, T, E>`, `RetryOutcome<T, E>`, `MAX_RETRIES`,
  and `backoff_delay`. OpenAI `send_chat`, Anthropic `send_messages`,
  and Gemini `send_request` all share the loop; per-provider status
  classification stays inline (OpenAI 500/502/503, Anthropic adds 529,
  Gemini 500/503).

- ~~**Rate-tracker persistence fuzz**~~ — Done. Per-provider tests cover
  to_persisted → from_persisted same-day roundtrip (daily preserved,
  minute zeroed), day rollover on reload (daily reset), and minute
  rollover mid-operation (minute resets, daily preserved). Gemini adds
  a daily-limit-after-reload test.

- ~~**Chat-refinement memory bound**~~ — Done (revised interpretation).
  Growth lives in `generate_with_tools_progress`, not the prompt builder.
  `llm/trim.rs` supplies `estimate_tokens` + `SAFE_PROMPT_BUDGET_TOKENS =
  100_000`. Each provider has a local `trim_messages`/`trim_contents`
  that drops oldest turn atomically (keeping the initial prompt and the
  latest turn, preserving tool_call/tool_result pair balance).

- ~~**Extend billing-keyword detection**~~ — Done. `llm::has_billing_keyword`
  centralizes the substring list (`billing`, `quota`, `exceeded`,
  `payment`, `credit`, `insufficient`) plus language-neutral Google
  status codes (`resource_exhausted`, `failed_precondition`) for
  Gemini's non-English responses. All three providers' `validate_key_detailed`
  overrides call through it.
