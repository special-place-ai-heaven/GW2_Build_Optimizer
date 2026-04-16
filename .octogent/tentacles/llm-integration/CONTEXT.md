# llm-integration

Multi-provider LLM reasoning layer — the creative force that can propose builds beyond the meta.

## Agent Toolbelt (required)

- **Use SymForge MCP tools for all code work in this tentacle.** `health`
  once per session, then `search_symbols`, `search_text` (especially for
  prompt string fragments), `get_file_context`, `get_symbol`,
  `find_references`, `edit_plan` + `edit_within_symbol` /
  `replace_symbol_body`, `analyze_file_impact`, `what_changed`. Raw
  `Read`/`Grep`/`Glob` burns 70–95% more tokens.
- **Optional: Obsidian vault** for broader LLM/prompt-eng context if it
  exists.

## Scope

- `crates/optimizer/src/llm/mod.rs` — `LlmClient` trait, `LlmError`,
  `KeyValidationResult`, `ToolDefinition`, `ToolCall`, `create_client`
  factory, `generate_with_tools` shim
- `crates/optimizer/src/llm/gemini.rs` — Gemini provider
- `crates/optimizer/src/llm/openai.rs` — OpenAI Chat Completions + tools
- `crates/optimizer/src/llm/anthropic.rs` — Anthropic Messages API + tool use
- `crates/optimizer/src/llm/tools.rs` — shared tool-schema helpers
- `crates/optimizer/src/gemini.rs` — legacy Gemini wire-format client (the
  one `GeminiLlmClient` thinly wraps)
- `crates/optimizer/src/gemini_tools.rs` — `ToolContext` + tool
  implementations the LLM calls during generation
- `crates/optimizer/src/prompts.rs` — all prompt construction and response
  parsing (`new_build_prompt_with_tools`, `synergy_build_prompt`,
  `chat_refinement_prompt_with_tools`, `parse_gemini_build`, etc.)
- `crates/optimizer/src/validation.rs` — validates LLM build responses
  against `GameDb` (shared ownership with `optimizer-engine` — "LLM
  proposes, engine validates" is the whole architecture)
- `crates/optimizer/tests/live_llm.rs` — `#[ignore]` live-LLM integration
  tests

## Mission Link

The engine alone cannot outperform meta creators — it can only score what
already exists. The LLM is where novel combinations come from. But the LLM
hallucinates specs, weapons, and stat combos it has never seen. The layer
works only because every proposal is **validated and corrected** before
shipping to the user.

## Key Decisions

- **`LlmClient` trait is `Send + Sync + &self`.** Interior mutability
  (`Mutex` / `RwLock`) for provider state. Callers hold one
  `Arc<dyn LlmClient>` and share freely across threads. (`llm/mod.rs:79-168`.)
- **`create_client(config, addon_dir)` factory** switches on
  `AppConfig::active_llm_provider`. `addon_dir` carries persistent
  rate-limit state (per-provider `.usage.json`).
- **Per-provider wire formats:**
  - **Gemini**: `functionDeclarations`, `x-goog-api-key` header,
    rate-tracking 10 RPM / 250 RPD.
  - **OpenAI**: Chat Completions, `tool_calls` array, JSON-*string*
    arguments (not objects), `tool_call_id` in responses, Bearer auth.
    (`llm/openai.rs:122-170`.)
  - **Anthropic**: Messages API with content blocks (`text`, `tool_use`,
    `tool_result`), `x-anthropic-version` header, `x-api-key`,
    `max_tokens` is **required**, 529 retries with exponential backoff.
    (`llm/anthropic.rs:125-181`.)
- **`validate_key_detailed` returns `KeyValidationResult { valid, message, warning }`.**
  HTTP 401 = invalid (reject). 400/403/429 containing billing keywords
  (`"billing"`, `"quota"`, `"exceeded"`, `"payment"`) = **valid key,
  billing issue** (accept with warning). Never return `valid: false` on a
  billing error. (`llm/mod.rs:89-135` and each provider's override.)
- **`list_models` hits each provider's `/v1/models`** and falls back to
  hardcoded `GEMINI_MODELS` / `OPENAI_MODELS` / `ANTHROPIC_MODELS` from
  `gw2_core::config` on failure.
- **Rate trackers persist to disk** via `PersistedUsage { day_epoch, used_today, used_this_minute, minute_epoch }`.
  Loaded on `with_persistence`, flushed after every reserve — survives a
  DLL reload without resetting daily quota.
- **`validate_gemini_build` + `apply_gemini_response` ordering is mandatory.**
  Any code path taking an LLM JSON response must validate it against
  `GameDb` first. Unchecked apply = hallucinated spec/weapon/skill in user
  builds. (`validation.rs:93-126`.)
- **Prompts pre-bake the entire relevant context (~40–50K tokens).**
  Single LLM call, no multi-turn tool dance for base "new build" path.
  Tool-call path is only for refinement / chat.
  (`prompts.rs::build_game_context`, `new_build_prompt_with_tools`.)
- **`sanitize_build_summary`** strips fields before re-showing build JSON
  to the LLM in refinement — prevents prompt bloat. (`prompts.rs:490-495`.)

## Conventions

- **Wire-format types (`MessagesRequest`, `ChatRequest`, etc.) stay
  provider-local**, never leak into the engine. Engine sees only
  `LlmError`, `ToolCall`, `KeyValidationResult`.
- **Every provider impl has a `test_provider_name` +
  `test_remaining_quota_default` test** — minimal contract check that the
  trait is wired up.
- **Tool-schema conversion is provider-local**: `to_gemini_tools` in
  `llm/gemini.rs`, inline in `openai.rs`, inline in `anthropic.rs`. Do not
  hoist to `llm/tools.rs` unless three providers need the same shape.
- **Retry policy**: OpenAI/Anthropic wrap `send_chat` / `send_messages`
  with `MAX_RETRIES = 3` exponential backoff for 5xx and specifically
  Anthropic 529 overload. 4xx errors do not retry.
- **Live LLM tests `#[ignore]`-gated**. Never run in CI.

## Cross-Tentacle Contracts

- **core-domain** supplies `AppConfig`, `LlmProvider`, `BuildLocks`,
  `ResolvedBuild`, `GameMode`.
- **optimizer-engine** calls in via `optimize_with_gemini` and
  `enrich_with_gemini`; receives `ValidatedBuild` or `LlmError`.
- **patch-aware-data** provides objective-profile descriptions feeding
  prompt context (`weights_context`, `build_game_context`).
- **addon-ui** renders chat bar, model picker, key-validation warnings.
  `KeyValidationResult.warning` surfaces in-situ.

## Hotspots & Risks

- **Prompt drift is the silent killer.** Editing `prompts.rs` without
  regenerating snapshot tests means LLM output shape can change unnoticed.
  Run `cargo test -p gw2-optimizer prompts::` after any edit.
- **Provider API changes without notice.** The only warning is a failing
  live test. Keep live tests in sync; run monthly.

## Non-Goals

- **No game logic here.** If a prompt helper needs to know what
  "Berserker's" means, it reads from `patch-aware-data`, not an inline
  table.
- **No model fine-tuning, no embeddings, no RAG.** Design is
  pre-computed-context + single call. If you're about to add a vector
  store, stop and discuss.

<!-- octogent:suggested-skills:start -->
## Suggested Skills

You can use these skills if you need to.

- `code-review`
- `refactor`
<!-- octogent:suggested-skills:end -->
