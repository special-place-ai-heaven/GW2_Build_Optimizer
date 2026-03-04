# GW2 Build Optimizer — Production-Readiness Backlog

_Updated: 2026-03-03 | Includes adversarial validation pass + re-rank with blast-radius/effort_

Items are grouped by **final Risk/Severity tier** and ordered by priority within each tier. Each item includes a code reference, blast-radius assessment, and effort estimate.

---

## Adversarial Validation: P0=0 Challenged

Before re-ranking, one adversarial pass was conducted against the P0=0 claim.

### Challenge 1 — Thread-Unjoined DLL Unload (`state.rs:274-280`)

`on_unload()` → `clear()` cancels the `CancellationToken` and drops state, but **does not join any background threads**. Thirteen `std::thread::spawn` calls exist across `main_view.rs` (lines 1020, 1083, 1501, 1584, 2168, 2199, 2279, 2312, 2408, 3233) and `setup.rs` (lines 91, 268, 369). On Windows, `std::thread::spawn` does **not** increment the DLL reference count, so Nexus calling `FreeLibrary` while threads execute DLL code could cause an access violation.

**Verdict: NOT P0.** Mitigated in practice because: (1) `is_cancelled()` is checked at thread entry and after each blocking op, so threads exit within one network-round-trip; (2) Nexus typically unloads only on user request during non-critical moments; (3) the pattern is standard across Nexus addon ecosystem. Noted as a **strengthening of P1-02** (addon tests should cover `clear()` + thread lifecycle).

### Challenge 2 — Stuck Loading Flags on Non-Optimizer Thread Panics (`main_view.rs:1501, 1584, 2168`)

The 9 non-optimizer background threads (character loading, model fetch, health check, etc.) have **no `catch_unwind`** — only the optimizer thread (`main_view.rs:2408-2410`) is wrapped. If these threads panic, mutex is recovered (poison recovery at `state.rs:199-207`) but `loading`/`validating` flags stay `true` permanently, requiring a GW2 restart to clear.

**Verdict: NOT P0.** Probability is low given the `Result`-based error handling throughout. But it's a systemic robustness gap that strengthens the case for P1-02 tests and for a future hardening task.

### Challenge 3 — WvW Silent Correctness Failure (`synergy_pipeline.rs:74`)

Users selecting WvW mode receive PvE-optimized builds with no warning. `_game_mode` parameter is present but unused.

**Verdict: Confirms P2-02.** Known limitation documented in README; not a regression. Stays P2.

### Challenge 4 — Spec Name Cross-Profession Validation (`validation.rs:93-126`)

Checked whether Gemini could hallucinate a spec name that passes cross-profession validation and causes a wrong build. Code review confirms profession-specific filtering is applied.

**Verdict: No issue.** Validation is sound.

### **Adversarial Conclusion: P0=0 stands.** No new P0 items discovered.

---

## P0 — Critical (Release Blockers)

_None identified. The codebase is functionally complete with no critical bugs or data-loss risks._

---

## P1 — High (Should Fix Before Stable/Public Release)

### P1-01 · CI Pipeline — Automated Test Runs

**Risk**: Regressions silently ship. Any code change is unverified unless manually run.
**Gap**: No `.github/workflows/`, no CI config of any kind.
**Evidence**: `CLAUDE.md`: "No CI pipeline — tests are run manually with `cargo test`."
**Fix**: Add GitHub Actions workflow running `cargo check` + `cargo test` on every push/PR.
**Blast-radius**: HIGH — affects entire test corpus (80+ tests never automatically verified).
**Effort**: ~1 hour
**Re-rank note**: Highest effort/risk ratio in the backlog. First priority.

---

### P1-02 · `addon` Crate — Zero Test Coverage

**Risk**: The most user-visible code (UI state machine, window routing, cancellation, config loading) is completely untested. Also: 9 non-optimizer background threads have no `catch_unwind` — a panic leaves `loading` flags stuck permanently (adversarial finding; requires GW2 restart to recover).
**Gap**: No `#[cfg(test)]` in any `crates/addon/src/*.rs` file.
**Evidence**: `state.rs:12` (static STATE), `state.rs:17-37` (CancellationToken), `state.rs:210-256` (init/screen routing), `state.rs:274-280` (clear — no thread join), `main_view.rs:1501, 1584, 2168, 2199, 2279, 2312` (unguarded thread spawns).
**Fix**: Add tests for:
  - `CancellationToken` propagation (`state.rs:17-37`)
  - `init()` → correct `Screen` routing based on config state (`state.rs:210-256`)
  - `MainState::default()` field initialization
  - `with_state` callback invocation when state is Some/None
  - `clear()` correctly cancels token and drops state
**Blast-radius**: HIGH — UI state machine is entire user-visible surface; stuck loading flags require game restart.
**Effort**: ~3-4 hours

---

### P1-03 · `main_view.rs` Decomposition

**Risk**: ~1400-line file with 13 thread spawns combines UI rendering, API threading, build resolution, and stats calculation. Any modification risks breaking unrelated functionality. Future UI changes are dangerous at this size.
**Gap**: The refactor proposed in `code_review_report.md §12.1.1` was not executed.
**Evidence**: `main_view.rs` lines 1020, 1083, 1501, 1584, 2168, 2199, 2279, 2312, 2408, 3233 (thread spawns spread across 1300 lines).
**Fix**: Extract into sub-modules per the proposed structure:
  ```
  main_view/
  ├── mod.rs           — tab routing + render dispatch
  ├── character.rs     — load_characters(), load_character_tabs()
  ├── resolution.rs    — resolve_build(), resolve_specs/skills/equipment
  ├── optimization.rs  — start_optimization(), enrich path
  └── stats.rs         — calculate_current_stats(), compute_3tier_combat()
  ```
**Blast-radius**: MEDIUM — current code works; this is a maintainability risk, not a runtime risk. Future UI work is risky without this.
**Effort**: ~4 hours

---

## P2 — Medium (Important for Quality, Not Hard Blockers)

### P2-01 · Condition Stack Weights Are Profession-Unaware

**Risk**: The `condition_dps_index` formula (`combat.rs:241-246`) applies identical stack weights for every profession: Bleeding=3.0, Burning=2.0, Poison=1.0, Torment=1.5, Confusion=0.5. A Necromancer Scourge (8-12 Bleeding stacks, 5-10 Torment) and a Firebrand (10+ Burning stacks) receive the same scoring weighting, producing inaccurate relative build scores.
**Gap**: No profession branching in `calculate_combat_performance()`.
**Evidence**: `combat.rs:241-246` — hardcoded weights with comment "typical condition application rates in a rotation".
**Fix**: Introduce `ConditionWeights` struct with per-archetype presets; pass from optimizer context. Minimum viable: 3 profession-group weight presets (necro-group, mesmer-group, firebrand-group).
**Blast-radius**: MEDIUM — affects accuracy of condition build optimization for all condition-damage professions. Correctness issue, not a crash risk.
**Effort**: ~2-3 hours
**Correctness-critical**: YES — selected for story drafting.

---

### P2-04 · Trait Fuzzy-Match False Positive Risk

**Risk**: `find_trait_by_name()` at `validation.rs:486-491` uses a forward contains-match with no minimum needle length. A short trait name returned by an LLM (e.g. "Force", "Swift") matches the first long trait whose name contains that substring, silently applying an incorrect trait to the validated build.
**Gap**: The forward contains-check has no minimum-length guard.
**Evidence**: `validation.rs:486-491` — `t.name.to_lowercase().contains(&needle)` with no `needle.len() >= N` guard.
**Fix**: Add `needle.len() >= 5` guard before the contains fallback, OR require `needle.len() >= (trait_name.len() * 60 / 100)`.
**Blast-radius**: MEDIUM — affects Tier 2 optimizer path (Gemini pipeline) whenever LLM returns short trait names. Wrong trait applied silently.
**Effort**: ~30 minutes
**Correctness-critical**: YES — selected for story drafting.

---

### P2-02 · WvW Numeric Modeling Gap

**Risk**: WvW mode is selected in UI but the synergy pipeline doesn't apply WvW-specific stat modifiers. Users optimize WvW builds as if they're PvE builds — no warning shown.
**Gap**: `synergy_pipeline.rs:74`: `_game_mode` parameter is unused (prefixed `_`).
**Evidence**: `synergy_pipeline.rs:74`, `README.md:67-69` "Known Limitations".
**Fix**: Implement WvW coefficient scaling (typically -33% to -50% on skill power coefficients). Minimum viable: WvW damage multiplier applied when `GameMode::WvW` is active.
**Blast-radius**: MEDIUM-HIGH for WvW users — wrong optimization output with no error. Niche use case vs PvE but materially misleading.
**Effort**: ~3-4 hours

---

### P2-03 · `GameDb::load` Uses `Result<T, String>` Pattern

**Risk**: String-typed errors lose context across call boundaries.
**Gap**: Several `optimizer` crate functions return `Result<T, String>` instead of typed error enums.
**Evidence**: `gamedb.rs` (GameDb::load), `code_review_report.md §8.2`.
**Fix**: Add `OptimizerError` enum with `thiserror` derivation.
**Blast-radius**: LOW — technical debt; no user-visible impact.
**Effort**: ~2 hours

---

## P3 — Low (Technical Debt / Polish)

### P3-01 · Logging Inconsistency — `eprintln!` in `storage.rs`

**Evidence**: `storage.rs:77`
**Fix**: Replace with `nexus::log::log(LogLevel::Warning, ...)`.
**Blast-radius**: LOW — corrupt save warnings invisible in-game.
**Effort**: 5 min

---

### P3-02 · `sanitize_filename` Allows Space Characters

**Evidence**: `storage.rs:104-113`: `c == ' '` allowed.
**Fix**: Remove `|| c == ' '`; replace spaces with underscores.
**Blast-radius**: LOW — filesystem compatibility edge case.
**Effort**: 5 min

---

### P3-04 · Hardcoded Sigil Recognition List (~15 entries)

**Evidence**: `combat.rs:501-560` — explicit name-contains chain.
**Fix**: Supplement with description-based parsing or data-driven table.
**Blast-radius**: LOW-MEDIUM — new GW2 sigils post-ship go unrecognized.
**Effort**: ~2 hours (ongoing maintenance concern)

---

### P3-05 · `_derived: &DerivedStats` Unused Parameter

**Evidence**: `combat.rs:195`
**Fix**: Remove parameter and update all callers.
**Blast-radius**: LOW — API surface noise.
**Effort**: ~30 min

---

### P3-03 · Speculative Model IDs in `GEMINI_MODELS` Fallback

**Evidence**: `config.rs:138-141` — "gemini-3-pro-preview" may not exist.
**Fix**: Audit against Gemini API and remove speculative IDs.
**Blast-radius**: LOW — affects users who can't reach model-list API.
**Effort**: 30 min (requires API check)

---

### P3-06 · No Default Keybind Documented in README

**Evidence**: `README.md:24`
**Fix**: Add actual default keybind (`Ctrl+Shift+O` per `lib.rs:33`).
**Blast-radius**: LOW — documentation only.
**Effort**: 5 min

---

## Re-Ranked Summary Table

| Rank | ID | Title | Severity | Blast-Radius | Effort | Code Reference |
|------|-----|-------|----------|--------------|--------|---------------|
| 1 | P1-01 | CI pipeline | High | HIGH | 1h | CLAUDE.md |
| 2 | P1-02 | addon crate tests | High | HIGH | 3-4h | state.rs:17-37, 210-280; main_view.rs:1501+ |
| 3 | P1-03 | Decompose main_view.rs | High | MEDIUM | 4h | main_view.rs (1400 lines, 13 thread spawns) |
| 4 | P2-01 | Condition stack weights profession-unaware ★ | Medium | MEDIUM | 2-3h | combat.rs:241-246 |
| 5 | P2-04 | Trait fuzzy-match guard ★ | Medium | MEDIUM | 30min | validation.rs:486-491 |
| 6 | P2-02 | WvW numeric modeling | Medium | MED-HIGH | 3-4h | synergy_pipeline.rs:74 |
| 7 | P2-03 | Typed OptimizerError enum | Medium | LOW | 2h | gamedb.rs |
| 8 | P3-01 | eprintln→nexus log in storage | Low | LOW | 5min | storage.rs:77 |
| 9 | P3-02 | sanitize_filename: no spaces | Low | LOW | 5min | storage.rs:104 |
| 10 | P3-04 | Sigil modifier list maintenance | Low | LOW-MED | 2h | combat.rs:501-560 |
| 11 | P3-05 | Remove `_derived` dead param | Low | LOW | 30min | combat.rs:195 |
| 12 | P3-03 | Verify Gemini fallback model IDs | Low | LOW | 30min | config.rs:138 |
| 13 | P3-06 | Document default keybind | Low | LOW | 5min | README.md:24 |

★ = Correctness-critical P2; selected for story drafting along with all P1 items.

---

_Adversarial pass: P0=0 confirmed. Two adversarial findings incorporated into P1-02 scope._
_Status: Stories drafted for P1-01, P1-02, P1-03, P2-01, P2-04. Awaiting user approval._
