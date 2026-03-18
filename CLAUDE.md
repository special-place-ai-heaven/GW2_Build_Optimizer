# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**GW2 Build Optimizer v1.0.0** — In-game Guild Wars 2 addon (Nexus plugin) that optimizes character builds across all game modes (PvE, PvP, WvW). Uses the GW2 API for game/character data and a pluggable LLM backend (Gemini, OpenAI, or Anthropic) for build reasoning. Feature complete (S01-S15).

## Build & Development

```bash
cargo check              # Fast compilation check
cargo build --release    # Produces target/release/gw2_build_optimizer.dll
# Deploy: copy DLL to C:\GAMES\Guild Wars 2\addons\
```

## Architecture

Rust workspace, 4 crates → single DLL loaded by Nexus addon manager:

```
crates/addon/       — cdylib: Nexus entry point, ImGui UI, keybinds (nexus-rs)
crates/core/        — Shared types (ResolvedBuild, StatBlock, CombatMetrics, SavedBuild),
                      config (AppConfig + LlmProvider + per-provider keys/models), storage
crates/gw2api/      — GW2 API v2 client (rate limiter, cache, download orchestration, serde models)
crates/optimizer/   — Engine (3-tier fallback: deterministic synergy → Gemini pipeline → legacy),
                      combat math, scoring, validation, synergy pipeline, rotation simulator,
                      llm/ (LlmClient trait + Gemini/OpenAI/Anthropic providers), prompts, context
```

**Key dependency**: `nexus` crate from [nexus-rs](https://github.com/Zerthox/nexus-rs) — Nexus addon API with ImGui.

### Optimization Pipeline (3-tier fallback)

1. `engine::optimize_deterministic()` — pure-Rust synergy pipeline, no LLM needed
2. `engine::optimize_with_gemini()` — cosine-sim gear prefix + pre-computed context (~40-50K tokens) + single LLM call + validation
3. Legacy `engine::optimize()` → `enrich_with_gemini()` — deterministic gear+spec search enriched post-hoc

Each tier falls back to the next on failure with a Warning log.

### LLM Provider Abstraction (`crates/optimizer/src/llm/`)

`LlmClient` trait (Send+Sync, &self) with `create_client(config, addon_dir)` factory. Each provider handles wire format internally:
- **Gemini**: `functionDeclarations`, x-goog-api-key header, rate tracking (10 RPM, 250 RPD)
- **OpenAI**: Chat Completions, JSON-string tool args, tool_call_id, Bearer auth
- **Anthropic**: Messages API, content blocks, x-api-key header, 529 retry, max_tokens mandatory
- `list_models()` fetches available models from each provider's API; Settings tab auto-fetches with hardcoded fallback
- `validate_key_detailed()` → `KeyValidationResult { valid, message, warning }` — separates auth from billing

## GW2 Domain Context

See `~/.claude/projects/.../memory/gw2-domain.md` for full reference. Key points:
- 9 professions, 5 core + ~4 elite specs each; 3 spec slots (slot 3 can be elite)
- 3 trait columns per spec (pick 1 of 3 each)
- Gear: 2 weapon sets + sigils, 6 armor + runes, 6 trinkets, 1 relic
- PvP uses amulet system instead of gear
- Stat formula: `attribute_adjustment * multiplier + value`
- GW2 API: 300 burst, 5/sec refill, max 200 IDs per bulk request

## Conventions

- Rust 2021, workspace deps hoisted to root `Cargo.toml`
- Global state: `Mutex<Option<AddonState>>` static; access via `with_state(|s| ...)`
- Background work: `std::thread::spawn` + `with_state()` callback (no channels)
- All bg threads clone `CancellationToken` (Arc<AtomicBool>) and check `is_cancelled()`
- ImGui via nexus-rs: `Window::new().build(ui, || { ... })`
- Screen routing: `AddonState.screen: Screen` enum drives render dispatch
- Config: `AppConfig` with atomic save (.tmp + rename); `is_setup_complete()` = gw2_key + active_llm_key + cache
- Borrow conflicts: clone owned values before mutable state borrows (Rust borrow checker)
- UTF-8 safe: use `text.chars().take(N).collect()` never `&text[..N]` (panics on multibyte)

## Critical Patterns (Gotchas)

- **HP-class ≠ armor-class**: GW2 health and armor classes don't align by profession. Two separate lookup tables required.
- **Stat alias normalization**: GW2 API uses both old ("ConditionDuration") and new ("Expertise") attribute names. `StatBlock::add/get` normalizes both.
- **Traited-fact overrides**: Active traited_facts replace base facts by index. Must collect override indices first, then skip overridden base facts.
- **Rune bonuses are strings**: Rune tier bonuses come as unstructured text ("+7% Burning Duration"), not structured Facts. Parsed via `parse_rune_modifier()`.
- **Gemini prefix override**: Gemini ignores gear constraints. `select_gear_prefix()` (cosine similarity) is authoritative — Gemini's choice is always overwritten.
- **Validation before apply**: Always call `validate_gemini_build()` before `apply_gemini_response()`. Gemini hallucinates specs/weapons.
- **Elite spec skill gating**: Filter skills by `Skill::specialization` — only core skills (None) or equipped elite spec skills allowed.
- **Billing-tolerant key validation**: HTTP 401 = invalid key. 400/403/429 with billing keywords = valid key, billing issue. Don't reject valid keys.
- **Lenient deserialization**: GW2 API facts sometimes lack a `type` field. Use `filter_map(from_value(...).ok())` to skip unparseable entries.
- **Panic recovery**: Optimization bg thread wrapped in `catch_unwind` — prevents mutex poisoning on panic.
- **score_with_weights normalization**: Constants (STRIKE_DPS_NORM=3000, etc.) are empirically tuned. Don't adjust without cross-build validation.
- **WEIGHT_BUDGET=2.0**: Models GW2 gear trade-offs. `set_constrained()` proportionally scales other axes. Don't change the constant.
- **Manual query strings for GW2 API**: reqwest `.query()` URL-encodes commas as %2C. Build query strings manually to preserve `ids=1,2,3`.

<!-- MANUAL -->
## Custom Notes

Add project-specific notes here. This section is never auto-modified.

<!-- END MANUAL -->

---

## thepopebot Agent

This repo also hosts an autonomous AI agent powered by [thepopebot](https://github.com/stephengpope/thepopebot). The agent files live alongside the Rust project:

- `config/` — Agent personality (`SOUL.md`), job prompts, cron schedules, webhook triggers
- `skills/` — Agent capabilities (browser, search, etc.)
- `docker/` — Docker containers for the event handler and job runner
- `.github/workflows/` — GitHub Actions for running agent jobs
- `app/` — Next.js web UI (chat interface, job monitor, settings)

The agent runs at `http://localhost:3000` (or your configured `APP_URL`). Run `docker compose up` to start it.


## SymForge MCP — Tooling Preference

When SymForge MCP is available, prefer its tools for repository and code
inspection before falling back to direct file reads.

Use SymForge first for:
- symbol discovery
- text/code search
- file outlines and context
- repository outlines
- targeted symbol/source retrieval
- surgical editing (symbol replacements, renames)
- impact analysis (what changed, what breaks)
- inspection of implementation code under `src/`, `tests/`, and similar
  code-bearing directories

Preferred tools for reading:
- `search_text` — full-text search with enclosing symbol context
- `search_symbols` — find symbols by name, kind, language, path
- `search_files` — ranked file path discovery, co-change coupling
- `get_file_context` — rich file summary with outline, imports, consumers
- `get_file_content` — read files with line ranges or around a symbol
- `get_repo_map` — repository overview at adjustable detail levels
- `get_symbol` — look up symbols by name, batch mode supported
- `get_symbol_context` — symbol body + callers + callees + type deps
- `find_references` — call sites, imports, type usages, implementations
- `find_dependents` — file-level dependency graph
- `inspect_match` — deep-dive a search match with full symbol context
- `analyze_file_impact` — re-read file, update index, report impact
- `what_changed` — files changed since timestamp, ref, or uncommitted
- `diff_symbols` — symbol-level diff between git refs
- `explore` — concept-driven exploration across the codebase

Preferred tools for editing:
- `replace_symbol_body` — replace a symbol's entire definition by name
- `edit_within_symbol` — scoped find-and-replace within a symbol's range
- `insert_symbol` — insert code before or after a named symbol
- `delete_symbol` — remove a symbol and its doc comments by name
- `batch_edit` — multiple symbol-addressed edits atomically across files
- `batch_rename` — rename a symbol and update all references project-wide
- `batch_insert` — insert code before/after multiple symbols across files

Default rule:
- use SymForge to narrow and target code inspection first
- use direct file reads only when exact full-file source or surrounding
  context is still required after tool-based narrowing
- use SymForge editing tools (`replace_symbol_body`, `batch_edit`,
  `edit_within_symbol`) over text-based find-and-replace whenever
  possible to ensure structural integrity and automatic re-indexing

Direct file reads are still appropriate for:
- exact document text in `docs/` or planning artifacts where literal
  wording matters
- configuration files where exact raw contents are the point of inspection

Do not default to broad raw file reads for source-code inspection when
SymForge can answer the question more directly.
