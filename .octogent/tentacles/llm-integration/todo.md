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

- **Unified retry/backoff shim** — OpenAI and Anthropic each have
  near-identical `MAX_RETRIES = 3` loops with bespoke status matching.
  Extract `with_retries<F>(f: F)` shared helper. Keep billing-vs-auth
  distinction per provider.

- **Rate-tracker persistence fuzz** — `PersistedUsage` is loaded and
  flushed per reserve. Simulate DLL-reload mid-minute and confirm
  `used_this_minute` resets on minute rollover but `used_today` persists.

- **Chat-refinement memory bound** —
  `chat_refinement_prompt_with_tools` rebuilds the whole conversation into
  one prompt. Add a token-budget guard that trims oldest non-critical
  turns when approaching per-provider context limits.

- **Extend billing-keyword detection** — `validate_key_detailed` provider
  overrides each carry a substring set. Centralize the list (`"billing"`,
  `"quota"`, `"exceeded"`, `"payment"`, `"credit"`) and cover localized
  error messages (Gemini occasionally returns non-English text).
