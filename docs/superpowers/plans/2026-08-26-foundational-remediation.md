# Foundational Remediation Plan — LLM transport bedrock → provider unification → optimizer → addon

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix every known deferred issue by first hardening the verification bedrock, then extracting the shared LLM transport layer, then unifying providers on it, then the optimizer's determinism/warning surfaces, then addon-layer polish — in strict dependency order, each layer green before the next starts.

**Architecture:** A `llm::transport` + `llm::sse` + `llm::ResponseCache` bedrock gives all providers one streaming reader, one capped-body reader, one retry policy, and one cache. The OpenAI-compatible clients (OpenAI, OpenRouter) collapse onto a single streaming core; Anthropic and Gemini adopt the shared pieces without changing their wire formats. Optimizer determinism and user-facing warnings ride existing pipelines. The addon keeps its compute-before-lock discipline; the abandoned card renderers go away.

**Tech Stack:** Rust workspace (2021), reqwest 0.12 blocking, std-only SSE parsing (BufRead), GitHub Actions on windows-latest.

**Spec:** Session decisions 2026-08-26: streaming-first chat completions (v1.6.1), compute-before-lock state discipline (v1.6.2), adversarial-review findings closed in v1.6.3; deferred items listed in the v1.6.3 release notes are the input backlog for this plan.

## Global Constraints

- Every task ends with `cargo test --workspace` fully green and `cargo build --workspace` warning-free.
- No behavior change unless the task says so; the one intended behavior change is T2.1 (OpenAI provider gains streaming, matching OpenRouter's v1.6.1 semantics).
- Line endings: repo is LF (`eol=lf`); never commit CRLF churn.
- Secrets never appear in code, logs, or tests — mock servers and env vars only.
- Each task is its own checkpoint commit on branch `fix/foundations`; commit only after its verification command passes.
- Release procedure stays per `release` skill: local DLL build + copy to `C:\GAMES\Guild Wars 2\addons\`, tag + `gh release create` with DLL + SHA256SUMS.txt.

## Dependency graph

```
L0 (verification bedrock)
 └── L1 (llm transport bedrock: sse module, ResponseCache, capped read, retry)
      └── L2.1 (OpenAI-compat unification: OpenRouter + OpenAI on the shared core)
           └── L2.2 (Anthropic SSE port — separate follow-up plan)
      └── L2.3 (Gemini adopts shared cache/caps — partial)
 └── L3.2 (determinism sweep — independent of L1)
 └── L3.1 (warnings channel — after L3.2)
 └── L4 (addon polish — independent; T4.3 after L1 uses the same capped reader)
```

---

### Task L0.1: Line-ending bedrock

**Files:**
- Create: `.gitattributes`

**Interfaces:** none (repo meta).

- [x] **Step 1:** Create `.gitattributes`:

```
* text=auto
*.rs text eol=lf
*.toml text eol=lf
*.md text eol=lf
*.json text eol=lf
*.yml text eol=lf
*.yaml text eol=lf
*.png binary
*.jpg binary
*.ttf binary
*.dll binary
```

- [x] **Step 2:** Verify no mass renormalization leaks into the commit: `git add .gitattributes && git status --short` — expect only `.gitattributes` staged.

- [x] **Step 3:** Commit: `git commit -m "chore: pin LF line endings via .gitattributes"`

### Task L0.2: CI verification bedrock

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:** Produces the gate every later task relies on: fmt check, clippy `-D warnings`, `cargo test --workspace`.

- [x] **Step 1:** Create `.github/workflows/ci.yml`:

```yaml
name: ci
on:
  push:
    branches: [main]
  pull_request:

jobs:
  verify:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [x] **Step 2:** Make the gate pass locally before committing: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Fix whatever the gate flags (expected: 2 warnings in `crates/optimizer/tests/live_llm.rs`, 1 in `gw2-core`, 1 in `gw2-api`, 4 unused `render_build_*` functions in `crates/addon/src/ui/main_view/build_display.rs` if resurfaced by fmt — delete per the v1.6.3 dead-code rules).

- [x] **Step 3:** Commit: `git commit -m "ci: fmt + clippy -D warnings + workspace tests on windows-latest"`

### Task L1.1: Extract the SSE streaming module

**Files:**
- Create: `crates/optimizer/src/llm/sse.rs`
- Modify: `crates/optimizer/src/llm/mod.rs` (add `pub mod sse;`)
- Modify: `crates/optimizer/src/llm/openrouter.rs` (delete moved items, `use super::sse::*`)

**Interfaces:**
- Produces: `sse::read_stream<R: std::io::Read>(reader: R) -> Result<sse::StreamedMessage, LlmError>`, `sse::StreamedMessage::{Message(Message), Empty(String)}`, `sse::apply_chunk`, `sse::StreamAccumulator` — byte-identical behavior to today's `openrouter.rs` implementations.
- Consumes: `super::Message`, `ToolCallResponse`, `FunctionCallData`, `LlmError` from `llm/mod.rs`.

- [x] **Step 1:** Move `StreamChunk`, `StreamChoice`, `StreamDelta`, `StreamToolCallDelta`, `StreamFunctionDelta`, `StreamAccumulator`, `StreamToolCall`, `StreamedMessage`, `apply_chunk`, `StreamAccumulator::into_message`, `read_stream` and the five `read_stream` tests from `openrouter.rs` into `llm/sse.rs` unchanged.
- [x] **Step 2:** In `openrouter.rs` replace the moved block with `use super::sse::{read_stream, StreamedMessage};` and delete the moved tests (they live in `sse.rs` now).
- [x] **Step 3:** Verify: `cargo test -p gw2-optimizer llm::sse` — all moved tests pass; `cargo test -p gw2-optimizer` green.
- [x] **Step 4:** Commit: `git commit -m "refactor(llm): extract shared SSE streaming module from the OpenRouter client"`

### Task L1.2: Shared response cache with eviction

**Files:**
- Create: `crates/optimizer/src/llm/response_cache.rs`
- Modify: `crates/optimizer/src/llm/mod.rs` (`pub mod response_cache;`)
- Modify: `llm/openrouter.rs`, `llm/openai.rs`, `llm/anthropic.rs`, `gemini.rs` — replace `cache: Mutex<HashMap<String, CachedResponse>>` with `cache: response_cache::ResponseCache` and delete each file's private `CachedResponse`.

**Interfaces:**
- Produces: `response_cache::ResponseCache::new(ttl_secs: u64, cap: usize)`, `.get(prompt: &str) -> Option<String>`, `.insert(prompt: &str, text: String)` (evicts expired, then clears to cap when `len >= cap`).
- Consumes: `std::time::Instant`.

- [x] **Step 1:** Implement `ResponseCache` (Mutex-internal, same 1800s/64 semantics the four providers inline today) plus unit tests: ttl expiry, cap clears, get-hit.
- [x] **Step 2:** Swap all four providers over; delete their inline eviction blocks and `CachedResponse` structs.
- [x] **Step 3:** Verify: `cargo test -p gw2-optimizer llm` green; `cargo build --workspace` warning-free.
- [x] **Step 4:** Commit: `git commit -m "refactor(llm): one ResponseCache with ttl+cap for all providers"`

### Task L1.3: Shared capped-body reader

**Files:**
- Create: `crates/optimizer/src/llm/transport.rs` (module also hosts the retry helper for L2.1)
- Modify: `crates/optimizer/src/scraper.rs` (`fetch_html` uses `transport::read_body_capped`), `crates/gw2api/src/client.rs`, `crates/addon/src/feedback/client.rs`

**Interfaces:**
- Produces: `transport::read_body_capped(resp: reqwest::blocking::Response, max_bytes: u64) -> Result<Vec<u8>, String>` (uses `std::io::Read::take`, maps io errors to strings).
- Consumes: `reqwest::blocking::Response: Read`.

- [x] **Step 1:** Implement + unit-test with `std::io::Cursor` bodies (over-cap truncates at cap, under-cap passes through).
- [x] **Step 2:** Convert `fetch_html` (cap 2 MiB), gw2api `get_with_params` JSON reads (cap 8 MiB), feedback client text reads (cap 1 MiB).
- [x] **Step 3:** Verify: `cargo test -p gw2-optimizer -p gw2-api -p gw2-build-optimizer` green.
- [x] **Step 4:** Commit: `git commit -m "refactor: shared capped-body reader across scraper, gw2api, feedback"`

### Task L2.1: Unify OpenAI-compatible providers on one streaming core

**Files:**
- Create: `crates/optimizer/src/llm/openai_compat.rs`
- Modify: `llm/openrouter.rs`, `llm/openai.rs` — both become thin wrappers (auth headers, base URL, model defaults, request extras)

**Interfaces:**
- Produces: `openai_compat::ChatRequest`, `openai_compat::send_chat(client, key, base, extra_headers, model, messages, tools, stream: bool, reasoning: Option<ReasoningConfig>, provider_prefs: Option<ProviderPrefs>) -> Result<Message, LlmError>` — the retry/backoff/Retry-After loop lives here once.
- Consumes: `sse::read_stream`, `transport::read_body_capped` (non-stream fallback paths), `LlmError`.

- [x] **Step 1:** Move `send_chat`'s body from `openrouter.rs` into `openai_compat::send_chat` with the parameterization above; keep status handling, rate-tracker reserve/undo at the wrapper level (wrappers own `RateTracker`).
- [x] **Step 2:** `OpenRouterClient::send_chat` = builds request extras (reasoning caps, provider prefs, referer headers) and delegates. `OpenAiClient::send_chat` = same core with `stream: true`, no reasoning/prefs — this is the intended behavior change: OpenAI chats stream now, killing its timeout class.
- [x] **Step 3:** Verify: `cargo test -p gw2-optimizer llm` (SSE + round-trip tests) green; live smoke: `OPENROUTER_API_KEY=… cargo test -p gw2-optimizer --test live_llm -- --ignored test_openrouter_validate` and, if a key exists, `test_openai_validate`.
- [x] **Step 4:** Commit: `git commit -m "refactor(llm): unify OpenAI + OpenRouter on one streaming chat core"`

### Task L3.2: Determinism sweep

**Files:**
- Modify: sites found by `grep -n "iter()" crates/optimizer/src --include="*.rs" -r | xargs -I{} echo` filtered to f64 accumulation into `StatBlock`/scores from HashMap/HashMap-iter sources (the amulet fix in `engine.rs` is the template: collect + sort + accumulate).

**Interfaces:** none new.

- [x] **Step 1:** Audit every `for (k, &v) in map` style accumulation feeding `stats.add`/score f64s; fix by sorted iteration; leave display-order maps alone.
- [x] **Step 2:** Verify: `cargo test -p gw2-optimizer` green; run the same optimize twice comparing serialized suggestions byte-for-byte if a fixture exists (`crates/optimizer/tests/`), else note in commit.
- [x] **Step 3:** Commit: `git commit -m "fix(optimizer): deterministic ordering for remaining map-fed f64 accumulations"`

### Task L4.2: Clipboard retry

**Files:**
- Modify: `crates/addon/src/clipboard.rs`

**Interfaces:** same `copy_text(&str) -> bool`.

- [x] **Step 1:** Wrap OpenClipboard in up to 3 attempts spaced 20 ms (Windows clipboard contention is transient); keep returning bool.
- [x] **Step 2:** Verify: `cargo test -p gw2-build-optimizer clipboard` green.
- [x] **Step 3:** Commit: `git commit -m "fix(addon): retry clipboard open to survive transient contention"`

### Deferred stages (each gets its own plan next session)

- **L2.2 Anthropic SSE port**: Anthropic stream uses `event:`/`data:` lines with `content_block_delta` and `message_delta` stops — `sse::read_stream` needs an Anthropic adapter; tool_use blocks accumulate differently (input_json_delta). Estimate M.
- **L2.3 Gemini streaming**: `generateContent` + `streamGenerateContent?alt=sse` — different auth (`x-goog-api-key`) and chunk shape (`candidates[0].content.parts[].text`). Estimate M.
- **L3.1 Warnings channel**: thread `Vec<String>` through `optimize_v2`/`optimize_deterministic` progress into `BuildSuggestion.quality_reasons` so stale trait locks surface. Estimate M; touches the beam-search hot path — needs its own review cycle.
- **L3.3 Mock-server test flake**: `get_with_params_429_then_200_succeeds_with_retry_after` raced once (13 s suite run, retry sleeps overlap port reuse). Investigate binding strategy; estimate S.
- **L4.1 Split `main_view/optimization.rs`** (2,645 lines) into `chat.rs`/`optimize.rs`/`suggestion_builders.rs` — pure moves. Estimate M.
- **L4.3 Chat history save off the render thread** (dirty-gated flush at `main_view/mod.rs:144` → bg flush). Estimate S.

## Status (2026-08-26)

All tasks complete on branch `fix/foundations`. L2.2/L2.3 executed in this
campaign (not deferred); L3.3 root-caused (mockito 1.7.2 accept-task
scheduling race — RST 10054 on early connections under load, reproduces on
unchanged code) with a readiness-probe gate as mitigation. Gates: clippy
`-D warnings` clean workspace-wide, 1,397 tests passing, OpenRouter live
suite verified the unified core end-to-end.
