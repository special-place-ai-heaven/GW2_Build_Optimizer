---
name: code-review
description: Project-specific code review checklist for GW2 Build Optimizer. Invoke before merging changes to crates/{addon,core,gw2api,optimizer}, especially anything touching the LLM clients, GW2 API client, optimization pipeline, scoring constants, or AddonState lifecycle. Catches the gotchas listed in CLAUDE.md that generic Rust review misses.
---

# Code Review — GW2 Build Optimizer

A generic Rust review will not catch the project-specific traps in this codebase. Use this as a checklist on top of normal Rust review.

## Domain Correctness

- **HP-class is NOT armor-class.** Two separate per-profession lookup tables. Reviewing anything that derives HP or armor: confirm the right table is consulted. Cross-class confusion silently produces wrong combat metrics.
- **Stat alias normalization.** GW2 API mixes old (`ConditionDuration`) and new (`Expertise`) attribute names. New code that adds/reads stats must go through `StatBlock::add` / `StatBlock::get`, never raw map lookups.
- **Traited-fact overrides.** When iterating skill facts, collect override indices from `traited_facts` first, then skip the corresponding base facts. Naive iteration double-counts.
- **Rune bonuses are unstructured strings** (e.g. `"+7% Burning Duration"`). They are NOT structured `Fact` entries. Parse via `parse_rune_modifier()` — if a reviewer sees rune logic that walks `facts: Vec<Fact>`, that is a bug.
- **Elite spec skill gating.** Skill availability filter must check `Skill::specialization`: allow only `None` (core) or the equipped elite spec.

## LLM Pipeline (`crates/optimizer/src/llm/` + `engine.rs`)

- **Gemini's gear prefix is always overwritten.** `select_gear_prefix()` (cosine similarity) is authoritative. A diff that "trusts Gemini's prefix choice" is a regression — Gemini ignores gear constraints.
- **Validate before apply.** Any code path producing a Gemini build response must call `validate_gemini_build()` before `apply_gemini_response()`. Gemini hallucinates specs and weapons.
- **Billing-tolerant key validation.** In `validate_key_detailed()`: HTTP 401 = invalid key (reject). 400/403/429 with billing keywords (e.g. "billing", "quota", "exceeded") = valid key with billing problem (accept, surface as warning). Never return `valid: false` on a billing error.
- **Tier fallback semantics.** Each of the three optimizer tiers (`optimize_deterministic` -> `optimize_with_gemini` -> legacy `optimize` + `enrich_with_gemini`) must fall through on failure with a **Warning** log (not Error, not silent).
- **Lenient deserialization.** GW2 fact entries sometimes lack a `type` field. Use `filter_map(|v| from_value(v).ok())`, never `collect::<Result<_,_>>()` — one bad entry must not kill the whole skill.

## State, Threads, Panics (`crates/addon/`)

- **Global state access** must go through `with_state(|s| ...)` on the `Mutex<Option<AddonState>>` static. Direct lock acquisition is a smell.
- **Background threads** spawned via `std::thread::spawn` must clone a `CancellationToken` and check `is_cancelled()` at loop boundaries.
- **`catch_unwind`** wraps the optimization background thread to prevent mutex poisoning. Removing it is a regression.
- **Borrow-conflict pattern**: clone owned values *before* taking a mutable borrow on `AddonState`. New code that fights the borrow checker probably needs this.

## UI / Strings

- **UTF-8 truncation**: `text.chars().take(N).collect::<String>()`, never `&text[..N]`. The latter panics on multibyte (player names, skill descriptions in non-English clients).

## GW2 API Client (`crates/gw2api/`)

- **Manual query strings.** `reqwest::Client::query()` URL-encodes commas as `%2C` and breaks `ids=1,2,3` bulk requests. Build the query string manually. Any new endpoint helper using `.query()` for ID lists is a bug.
- **Bulk limit = 200 IDs/request.** Larger batches must chunk.
- **Rate limiter**: 300 burst, 5/sec refill. New code that bypasses the limiter (raw `reqwest::get`) is a regression.
- **`ApiError` variant discipline.** `ApiError::Api` is for GW2 non-2xx responses only — it must populate `url_path` (the relative endpoint string) and `body_snippet` (≤200 chars, UTF-8 safe via `body_snippet()` helper). `RateLimited` must carry both `retries` and `url_path`. Cache I/O failures use `ApiError::Cache`; panics, invalid keys, and retry exhaustion use `ApiError::Internal`. Any construction with `status: 0` is a regression — that sentinel no longer exists.

## Empirically Tuned Constants — DO NOT ADJUST IN A REVIEW

These are calibrated against real builds. Changing them silently invalidates scoring across all archetypes.

- `STRIKE_DPS_NORM = 3000` (and other `*_NORM` constants in `scoring.rs`)
- `WEIGHT_BUDGET = 2.0` — models GW2 gear trade-offs in `set_constrained()`
- Any `*_NORM` or `*_BUDGET` literal in `scoring.rs` / `combat.rs`

If the diff changes one of these, the PR description must include cross-build validation evidence (scores for all 7 archetypes before/after). Otherwise reject.

## Config / Storage (`crates/core/`)

- **Atomic save** uses `.tmp` + `rename`. Diffs that write directly to the target path corrupt config on crash.
- **`is_setup_complete()`** requires `gw2_key && active_llm_key && cache`. Any feature that runs before setup completion is a UX bug.
