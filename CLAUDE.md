# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**GW2 Build Optimizer v1.4.4** — In-game Guild Wars 2 addon (Nexus plugin) that optimizes character builds across all game modes (PvE, PvP, WvW). Uses the GW2 API for game/character data and a pluggable LLM backend (Gemini, OpenAI, Anthropic, or OpenRouter) for build reasoning. Feature complete (S01-S15). Overlay chrome is localized (Settings → Language). Skill/trait/item names follow official API `lang=` for Deutsch, Español, Français, and 简体中文.

## Build & Development

```bash
cargo check              # Fast compilation check
cargo build --release    # Produces target/release/gw2_build_optimizer.dll
# Deploy: copy DLL to C:\GAMES\Guild Wars 2\addons\
```

**Every code fix ships.** After any addon/optimizer/core/gw2api change: bump patch version, `cargo test`, `cargo build --release`, copy `gw2_build_optimizer.dll` to `C:\GAMES\Guild Wars 2\addons\`, commit, push, and `gh release create` with the DLL + `SHA256SUMS.txt`. README Download links stay on `/releases/latest` — do not paste a SHA into README.

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
- **Gemini prefix override**: Optimize overwrites Gemini with `select_gear_prefix()` (cosine). Choya chat does not — named prefix in the player's message wins, including after “not minstrel”.
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


