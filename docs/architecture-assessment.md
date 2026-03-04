# GW2 Build Optimizer — Current-State Architecture, Quality & Gap Assessment

_Generated: 2026-03-03 | Scan: deep | Primary source: `_bmad-output/project-context.md` + code_

---

## 1. Project Classification

| Property | Value |
|----------|-------|
| Type | Desktop addon (Nexus plugin, `cdylib` DLL) |
| Language | Rust 2021 edition |
| Structure | Monolith — 4-crate workspace → single DLL |
| Version | v1.0.0 (self-described as feature-complete, S01–S15 done) |
| Build output | `target/release/gw2_build_optimizer.dll` |
| Deploy target | `C:\GAMES\Guild Wars 2\addons\` |

---

## 2. Architecture

### 2.1 Workspace Structure

```
GW2_Build_Optimizer/
├── Cargo.toml              — workspace root, all deps hoisted
├── crates/
│   ├── addon/              — cdylib DLL entry point; Nexus lifecycle + ImGui UI
│   │   ├── src/lib.rs      — DLL exports: addon_load, addon_unload, addon_render
│   │   ├── src/state.rs    — AddonState, MainState, CancellationToken, with_state()
│   │   └── src/ui/         — UI panels: main_view.rs (1400 ln), setup.rs, comparison.rs,
│   │                           chat_bar.rs, build_display.rs, lock_panel.rs, gear_diff.rs,
│   │                           radar_chart.rs
│   ├── core/               — shared domain types + config + storage
│   │   ├── src/types.rs    — ResolvedBuild, StatBlock, CombatMetrics, BuildLocks, SavedBuild
│   │   ├── src/config.rs   — AppConfig, LlmProvider enum, atomic save, 5 unit tests
│   │   └── src/storage.rs  — BuildStorage (save/list/delete per-JSON-file), 2 unit tests
│   ├── gw2api/             — GW2 API v2 client
│   │   ├── src/client.rs   — rate limiter (300 burst/5 per sec), manual query strings
│   │   ├── src/cache.rs    — build-number-based cache invalidation
│   │   ├── src/download.rs — batch download orchestration (200-ID chunks, scoped threads)
│   │   └── src/models/     — API models: Profession, Specialization, Trait, Skill,
│   │                           Item, ItemStat, Fact (20+ variants), PvpAmulet, etc.
│   └── optimizer/          — core optimization logic
│       ├── src/engine.rs   — 3-tier pipeline orchestration (50 KB)
│       ├── src/scoring.rs  — 5-axis OptimizationWeights, cosine-similarity gear selection,
│       │                      tier tables, score_with_weights(), 30+ tests
│       ├── src/combat.rs   — CombatPerformance model, condition ticks, damage modifiers,
│       │                      rune/sigil/relic parser, buff profiles, 25+ tests
│       ├── src/synergy_pipeline.rs — deterministic greedy pipeline (gear→spec→rune→sigil→relic→weapon→skill)
│       ├── src/validation.rs — validate_gemini_build() against GameDb, 4 unit tests
│       ├── src/gamedb.rs   — O(1) HashMap lookups for all GW2 entities
│       ├── src/stats.rs    — StatBlock accumulation, trait facts, BuffConversion
│       ├── src/synergy.rs  — NormalizedEffect, SynergyLink, marginal synergy scoring
│       ├── src/search.rs   — gear prefix exhaustive search, spec combo search
│       ├── src/rotation/   — rotation simulator (mod.rs, simulator.rs, builder.rs, skill_timings.rs)
│       ├── src/prompts.rs  — LLM prompt templates, GeminiBuildResponse serde
│       ├── src/context.rs  — pre-computed context builder (40-50K token LLM context)
│       └── src/llm/        — LlmClient trait + Gemini/OpenAI/Anthropic providers
```

**Evidence**: `Cargo.toml:1-22`, `crates/addon/src/lib.rs`, `crates/optimizer/src/engine.rs:1-24`

### 2.2 Key Architectural Patterns

| Pattern | Implementation | Evidence |
|---------|---------------|----------|
| Global state | `static STATE: Mutex<Option<AddonState>>` | `state.rs:12` |
| State access | `with_state<F, R>(f: F)` — non-reentrant | `state.rs:283-288` |
| Background threads | `std::thread::spawn` + `CancellationToken` | `state.rs:17-37` |
| Cancellation | `Arc<AtomicBool>` cloned into each thread | `state.rs:17-37` |
| Mutex poison recovery | `.unwrap_or_else(|e| e.into_inner())` | `state.rs:199-207` |
| Config save | `.tmp` write + `fs::rename` (atomic) | `config.rs:212-221` |
| Screen routing | `AddonState.screen: Screen` enum | `state.rs:154-165` |
| LLM abstraction | `LlmClient` trait (Send+Sync, &self methods) | `llm/mod.rs:79-155` |
| Gear selection | Cosine similarity against `GEAR_PROFILES` | `scoring.rs:526-564` |
| 3-tier fallback | `optimize_deterministic` → Gemini → `optimize` | `engine.rs (tier logic)` |

### 2.3 3-Tier Optimization Pipeline

```
Tier 1: optimize_deterministic()     ← PRIMARY
  └── optimize_synergy()             ← greedy layered search
       ├── select_gear_prefix()      ← cosine similarity (authoritative, overrides LLM)
       ├── search_spec_combos()      ← exhaustive spec × trait cross-product
       ├── rune scoring              ← marginal synergy against accumulated effects
       ├── sigil scoring             ← greedy sequential
       ├── relic scoring             ← marginal synergy
       ├── weapon enumeration        ← profession + elite spec gates
       └── skill selection           ← greedy: heal → elite → 3× utility
          ↓ (on failure)
Tier 2: optimize_with_gemini()       ← cosine-sim gear + LLM reasoning + validate_gemini_build()
          ↓ (on validation failure)
Tier 3: legacy optimize()            ← deterministic gear+spec search + post-hoc LLM enrich
```

**Evidence**: `engine.rs (full pipeline)`, `synergy_pipeline.rs:70-79`, `validation.rs:93-126`

### 2.4 LLM Provider Abstraction

Three providers implement `LlmClient` trait:

| Provider | Wire format | Auth | Rate tracking |
|----------|-------------|------|---------------|
| Gemini | `functionDeclarations` | x-goog-api-key header | 10 RPM / 250 RPD, persisted |
| OpenAI | Chat Completions, JSON-string tool args | Bearer | persisted |
| Anthropic | Messages API, content blocks | x-api-key | persisted |

**Evidence**: `llm/mod.rs:79`, `llm/gemini.rs`, `llm/openai.rs`, `llm/anthropic.rs`

---

## 3. Quality Assessment

### 3.1 Strengths

| Area | Rating | Evidence |
|------|--------|---------|
| Type safety | Excellent | No `unsafe` blocks; typed enums throughout |
| Error handling | Strong | `thiserror` everywhere; `?` propagation |
| Mutex safety | Correct | Poison recovery at `state.rs:199-207` |
| Atomic saves | Correct | `.tmp` + rename in `config.rs:218-220` |
| Test coverage | Good | 80+ unit tests: `combat.rs` (25+), `scoring.rs` (30+), `config.rs` (5), `storage.rs` (2), `validation.rs` (4) |
| Domain modeling | Excellent | GW2 formulas accurate; HP≠armor-class separation at `types.rs:214-229` |
| GW2 formula accuracy | Verified | Bleeding: `0.06*CD+22`, Burning: `0.155*CD+131.75` at `combat.rs:146-172` |
| Thread safety | Correct | `Send+Sync` on LlmClient; `Arc<AtomicBool>` cancellation |
| Serde resilience | Good | `#[serde(default)]` on optional fields; `filter_map(from_value(...).ok())` |
| Workspace deps | Clean | All versions hoisted to root `Cargo.toml:15-22` |

### 3.2 Code Quality Metrics

| Metric | Value |
|--------|-------|
| Total Rust source files | 54 |
| `unsafe` blocks | 0 |
| Test functions | 80+ |
| TODO comments | 0 |
| `unwrap()` on fallible ops | 0 (tests only) |
| Largest single file | `main_view.rs` (~1400 ln) |
| Second-largest file | `engine.rs` (~50 KB) |
| Logging inconsistency | `eprintln!` in `storage.rs:77`, rest uses nexus log |

---

## 4. Gap Analysis

### 4.1 Functional Gaps

| Gap | Severity | Evidence |
|-----|----------|---------|
| WvW numeric modeling | Medium | `synergy_pipeline.rs:74`: `_game_mode` parameter unused; README "Known Limitations" |
| Revenant legend swapping optimization | Low | README "Known Limitations": "No support for Revenant legend swapping optimization yet" |
| Conditional modifier modeling | Low | README: "complex conditional modifiers (e.g. 'while above 90% HP') are noted but not dynamically modeled" |
| Condition stack weights profession-unaware | Medium | `combat.rs:241-246`: Bleeding=3.0, Burning=2.0 hardcoded regardless of profession/rotation archetype |

### 4.2 Quality Gaps

| Gap | Severity | Location | Evidence |
|-----|----------|----------|---------|
| `addon` crate: zero tests | High | `crates/addon/src/` | No `#[cfg(test)]` in any addon source file |
| No CI pipeline | High | project root | CLAUDE.md: "No CI pipeline"; `code_review_report.md §5.4` |
| `main_view.rs` too large | Medium | `crates/addon/src/ui/main_view.rs` | ~1400 lines; `code_review_report.md §1.1`, §12.1 |
| `GameDb::load` returns `Result<T, String>` | Low | `optimizer/src/gamedb.rs` | `code_review_report.md §8.2` |
| `eprintln!` in storage instead of nexus log | Low | `storage.rs:77` | Inconsistent with rest of codebase |
| `sanitize_filename` allows spaces | Low | `storage.rs:104-113` | Passes `c == ' '`; filesystem hazard on some targets |
| Trait fuzzy-match false positive risk | Low | `validation.rs:488-490` | Contains-match without minimum length guard |
| Hardcoded sigil list (~15 entries) | Low | `combat.rs:501-560` | New game sigils not auto-handled |
| Speculative model IDs in `GEMINI_MODELS` | Low | `config.rs:138-141` | "gemini-3-pro-preview" may not exist; fallback list only |
| `_derived: &DerivedStats` unused | Info | `combat.rs:195` | Parameter present but prefixed `_`; `DerivedStats` may be vestigial |

### 4.3 Documentation Gaps

| Gap | Severity | Notes |
|-----|----------|-------|
| No project-level `docs/` folder existed | Resolved | This run creates it |
| README.md exists but lacks keybind reference | Low | `README.md:24` says "configured in Nexus" without default |
| No architecture diagram outside CLAUDE.md | Low | Could benefit from Mermaid diagram |

---

## 5. Summary

**The project is production-ready for v1.0.0 as a personal/community addon.** All S01–S15 sprints are complete. The core optimizer, combat model, validation, LLM abstraction, and persistence are all well-implemented with 80+ tests.

**Primary risks before a broader release:**
1. No automated testing (CI) — regressions won't be caught automatically
2. The `addon` crate (UI/state) has zero test coverage — the most user-visible code is untested
3. `main_view.rs` at 1400+ lines is a maintainability liability — any future UI work is risky

**The GW2 domain modeling is accurate and well-tested.** The 3-tier optimizer fallback is robust. The LLM provider abstraction is clean and extensible.
