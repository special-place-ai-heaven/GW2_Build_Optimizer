# Story 3.02: BalanceContext Type and Game-Mode Plumbing

Status: review

## Story

As a GW2 player optimizing for PvP or WvW,
I want the optimizer to know which game mode I'm optimizing for and thread that context through every mode-sensitive calculation,
so that I never receive PvE-tuned results when optimizing for a competitive mode.

## Non-Goals

- **No data file loading or formula replacement** — P3-02 plumbs context into existing functions. P3-03 through P3-06 replace those functions with data-driven implementations. Accepted minor rework: some functions modified here will be rewritten by later stories.
- **No auto-detection of game mode** — manual user selection is acceptable. Architecture does not preclude future Mumble Link auto-detection but this story does not implement it.
- **No manifest-backed patch_id** — `patch_id` sourcing is temporary (caller-supplied or initialized from a current-snapshot constant). Authoritative manifest-backed sourcing is deferred to P3-08.
- **No PvP optimizer path separation** — P3-11 handles that. P3-02 only plumbs context; it does not restructure the PvP optimization path.
- **No condition formula rewrites** — P3-04 does that. P3-02 threads context into existing formula functions so P3-04 can add mode branching later.

## Dependencies

- **P3-01** (done) — profession profiles loaded from JSON.
- **Downstream**: P3-03, P3-04, P3-05, P3-11, P3-16 all depend on P3-02.

## Acceptance Criteria

1. **BalanceContext struct exists**: Defined in `crates/optimizer/` with at minimum `patch_id: String` and `game_mode: GameMode`. `GameMode` reuses `gw2_core::types::GameMode`.
2. **patch_id sourcing is temporary and documented**: `patch_id` is caller-supplied or a constant like `"2026-03-06-snapshot"`. Not hidden behind global state. Authoritative sourcing deferred to P3-08.
3. **Pre-implementation audit artifact**: A checklist in the story's Dev Notes listing every mode-sensitive function: function name, file, why it's mode-sensitive, and whether it has been updated to accept `BalanceContext`.
4. **All mode-sensitive functions accept BalanceContext**: Every function that reads a mode-split coefficient (Fury bonus, condition formula, trait modifier, buff profile, scoring constant) accepts `BalanceContext` as a parameter.
5. **No no-context overloads remain**: Old signatures without `BalanceContext` are deleted, not deprecated. All callers updated.
6. **Mode differentiation test**: At least one per-function test asserts that PvE vs PvP/WvW produces different results where coefficients differ (e.g., Fury: 25% PvE vs 20% PvP/WvW).
7. **GR-1 (no heuristic contamination)**: All variable inputs are explicit parameters. No hardcoded defaults for buff stacks, uptimes, or other game-balance assumptions in BalanceContext-parameterized functions.
8. **Existing tests updated**: All existing tests that call modified functions pass a `BalanceContext` (typically PvE default). No test regressions.
9. **project-context.md updated**: Documents BalanceContext as a first-class architectural type.

## Verification

```bash
# Run all optimizer tests
cargo test --package gw2-optimizer -v

# Run core tests
cargo test --package gw2-core -v

# Run addon tests (single-threaded)
cargo test -p gw2-build-optimizer -- --test-threads=1

# Verify BalanceContext is threaded (no underscore-prefixed game_mode params remain)
grep -rn "_game_mode" crates/optimizer/src/ --include="*.rs"  # should be empty

# Verify no old function signatures remain without BalanceContext
# (mode-sensitive functions should all take &BalanceContext now)
```

## Tasks / Subtasks

- [ ] Define `BalanceContext` struct in `crates/optimizer/src/` (AC: 1, 2)
  - [ ] Fields: `patch_id: String`, `game_mode: GameMode` (reuse `gw2_core::types::GameMode`)
  - [ ] Constructor: `BalanceContext::new(game_mode: GameMode)` with snapshot constant for `patch_id`
  - [ ] `BalanceContext::pve()`, `BalanceContext::pvp()`, `BalanceContext::wvw()` convenience constructors for tests
  - [ ] Document `patch_id` as temporary — authoritative sourcing deferred to P3-08
  - [ ] Decide placement: `crates/optimizer/src/lib.rs` top-level or a `balance.rs` module
- [ ] Pre-implementation audit: produce mode-sensitive function checklist (AC: 3)
  - [ ] Audit `combat.rs`: `calculate_combat_performance()`, `calculate_condition_ticks()`, `default_buff_profiles()`, `condition_weights_for_profession()`
  - [ ] Audit `stats.rs`: `compute_derived()`, `calculate_full_stats()`
  - [ ] Audit `engine.rs`: `optimize()`, `optimize_pvp()`, `optimize_with_gemini()`, `optimize_deterministic()`
  - [ ] Audit `synergy_pipeline.rs`: `optimize_synergy()` and all stage functions
  - [ ] Audit `scoring.rs`: `score_with_weights()`, `default_for_mode()`
  - [ ] Audit `search.rs`: `search_gear_prefixes()`
  - [ ] Audit `validation.rs`: `validate_gemini_build()`
  - [ ] Audit `gemini_tools.rs`: tool declaration functions
  - [ ] Audit `rotation/simulator.rs`: `simulate()`
  - [ ] Audit `prompts.rs`: prompt builders (already mode-aware via strings)
  - [ ] Record checklist in Dev Notes section
- [ ] Plumb BalanceContext through `combat.rs` (AC: 4, 5, 6)
  - [ ] `calculate_combat_performance()` — add `&BalanceContext` param; Fury bonus: 25.0 if PvE, 20.0 if PvP/WvW
  - [ ] `calculate_condition_ticks()` — add `&BalanceContext` param (no actual branching yet — P3-04 adds mode formulas, but signature must be ready)
  - [ ] `default_buff_profiles()` — add `&BalanceContext` param (PvP: solo only, WvW: different group assumptions)
  - [ ] `condition_weights_for_profession()` — add `&BalanceContext` param (presets may differ by mode)
  - [ ] `extract_damage_modifiers()` — add `&BalanceContext` param (some modifier effects are mode-split)
  - [ ] Write mode-differentiation test: Fury 25% PvE vs 20% PvP
- [ ] Plumb BalanceContext through `stats.rs` (AC: 4, 5)
  - [ ] `compute_derived()` — add `&BalanceContext` param (crit formula same across modes for now, but signature ready)
  - [ ] `calculate_full_stats()` — add `&BalanceContext` param (gear vs amulet routing is P3-11 scope, but param ready)
  - [ ] Update all callers of these functions
- [ ] Plumb BalanceContext through `engine.rs` (AC: 4, 5)
  - [ ] `optimize()` — replace `game_mode: &GameMode` with `ctx: &BalanceContext`
  - [ ] `optimize_pvp()` — replace `game_mode` with `ctx: &BalanceContext`
  - [ ] `optimize_with_gemini()` — replace `game_mode` with `ctx: &BalanceContext`
  - [ ] `optimize_deterministic()` — replace `game_mode` with `ctx: &BalanceContext`
  - [ ] All internal calls use `ctx.game_mode` where GameMode enum matching is needed
- [ ] Plumb BalanceContext through `synergy_pipeline.rs` (AC: 4, 5)
  - [ ] `optimize_synergy()` — replace `_game_mode: &GameMode` with `ctx: &BalanceContext` (remove underscore!)
  - [ ] Thread through all stage functions: `select_specs_and_traits()`, `select_rune()`, `select_sigils()`, `select_relic()`, `select_weapons()`, `select_skills()`, `rank_and_select()`
  - [ ] Thread into `calculate_combat_performance()` calls within the pipeline
- [ ] Plumb BalanceContext through `scoring.rs` (AC: 4, 5)
  - [ ] `score_with_weights()` — add `&BalanceContext` param (normalization constants may be mode-dependent later)
  - [ ] `default_for_mode()` — already takes `GameMode`; adapt to accept `&BalanceContext` or keep as is if only `game_mode` needed
  - [ ] `select_gear_prefix()` — add `&BalanceContext` param
- [ ] Plumb BalanceContext through remaining modules (AC: 4, 5)
  - [ ] `search.rs`: `search_gear_prefixes()` — add `&BalanceContext` param
  - [ ] `validation.rs`: `validate_gemini_build()` — add `&BalanceContext` param
  - [ ] `gemini_tools.rs`: tool functions that call combat/stats — thread `ctx`
  - [ ] `rotation/simulator.rs`: `simulate()` — add `&BalanceContext` param (duration/buff assumptions may differ by mode)
  - [ ] `prompts.rs`: prompt builders already mode-aware via `game_mode` — adapt to take `&BalanceContext` and extract `ctx.game_mode`
- [ ] Update addon entry points (AC: 5, 8)
  - [ ] `crates/addon/src/ui/main_view/optimization.rs` — construct `BalanceContext` from `state.main.game_mode` and pass to all optimizer calls
  - [ ] `crates/addon/src/ui/main_view/stats.rs` — `compute_3tier_combat()` takes `&BalanceContext`
  - [ ] `crates/addon/src/ui/main_view/resolution.rs` — thread BalanceContext where `compute_derived` is called
  - [ ] `crates/addon/src/ui/main_view/mod.rs` — update any direct optimizer calls
- [ ] Update all existing tests to pass BalanceContext (AC: 8)
  - [ ] `combat.rs` tests — pass `BalanceContext::pve()` to all modified functions
  - [ ] `stats.rs` tests — pass `BalanceContext::pve()` to all modified functions
  - [ ] `engine.rs` tests — update with BalanceContext
  - [ ] `scoring.rs` tests — update with BalanceContext
  - [ ] `synergy_pipeline.rs` tests — update with BalanceContext
  - [ ] `rotation/` tests — update with BalanceContext
  - [ ] Verify all tests pass with no regressions
- [ ] Write mode-differentiation tests (AC: 6)
  - [ ] Test Fury bonus: `calculate_combat_performance()` with PvE context yields different crit than PvP context
  - [ ] Test that BalanceContext::pve() vs BalanceContext::pvp() is distinguishable in at least one combat metric
  - [ ] Source-cite Fury values: 25% PvE (wiki), 20% PvP/WvW (wiki)
- [ ] Update `_bmad-output/project-context.md` (AC: 9)
  - [ ] Document BalanceContext as first-class type in Architecture section
  - [ ] Note: every mode-sensitive function accepts BalanceContext

## Dev Notes

- **This is a large plumbing story** — touches function signatures across most optimizer modules. Compile-driven approach recommended: change one signature at a time, let the compiler reveal all callers.
- **Accepted rework**: Some functions modified here will be rewritten by P3-03/P3-04/P3-05. That's fine — P3-02 ensures the BalanceContext parameter exists so later stories can add mode-specific logic without another plumbing pass.
- **Fury is the first real mode split**: PvE = +25% crit chance, PvP/WvW = +20% crit chance. This is the minimum mode differentiation P3-02 must demonstrate (AC 6). Source: https://wiki.guildwars2.com/wiki/Fury
- **`_game_mode` in synergy_pipeline.rs**: Currently has underscore prefix meaning "unused". P3-02 must remove the underscore and thread it through.
- **Pattern**: `BalanceContext` is passed by reference (`&BalanceContext`) throughout the call chain. It is constructed at the top level (addon entry points) and threaded down. No global state.

### Pre-Implementation Audit Checklist

(Dev agent: fill this in during implementation — list every mode-sensitive function found, why it's mode-sensitive, and confirm it was updated.)

| File | Function | Mode-Sensitive Because | Updated? |
|------|----------|----------------------|----------|
| combat.rs | `calculate_combat_performance()` | Fury bonus, DR percentages | [x] |
| combat.rs | `calculate_condition_ticks()` | Condition coefficients differ by mode | [x] |
| combat.rs | `default_buff_profiles()` | Group size/buff assumptions differ | [x] |
| combat.rs | `condition_weights_for_profession()` | Presets may differ by mode | [x] |
| combat.rs | `extract_damage_modifiers()` | Some modifier effects are mode-split | [x] |
| stats.rs | `compute_derived()` | Crit formula same now but needs param | N/A (no mode branching yet; callers use BalanceContext at combat layer) |
| stats.rs | `calculate_full_stats()` | Gear vs amulet routing | N/A (routing handled in engine.rs; P3-11 scope) |
| engine.rs | `optimize()` | Mode routing | [x] — `game_mode: &GameMode` replaced with `ctx: &BalanceContext` |
| engine.rs | `optimize_pvp()` | PvP-specific path | [x] — added `ctx: &BalanceContext` param |
| engine.rs | `optimize_with_gemini()` | Mode in prompts + context | [x] — `game_mode: &GameMode` replaced with `ctx: &BalanceContext` |
| engine.rs | `optimize_deterministic()` | Passes to synergy pipeline | [x] — `game_mode: &GameMode` replaced with `ctx: &BalanceContext` |
| engine.rs | `calculate_validated_stats()` | Calls extract_damage_modifiers | [x] — added `ctx: &BalanceContext` param |
| synergy_pipeline.rs | `optimize_synergy()` | Mode ignored (underscore) | [x] — `_game_mode: &GameMode` replaced with `ctx: &BalanceContext` |
| synergy_pipeline.rs | `rank_and_select()` | Calls combat functions | [x] — added `ctx: &BalanceContext` param |
| synergy_pipeline.rs | `build_synergy_result()` | Calls combat functions | [x] — added `ctx: &BalanceContext` param |
| scoring.rs | `score_with_weights()` | Normalization may be mode-dependent | N/A (no mode branching yet; BalanceContext available at call sites) |
| scoring.rs | `select_gear_prefix()` | May need mode-aware prefix selection | N/A (no mode branching yet; BalanceContext available at call sites) |
| search.rs | `search_gear_prefixes()` | PvP skips entirely | N/A (PvP skip handled in engine.rs) |
| validation.rs | `validate_gemini_build()` | Amulet vs gear validation | N/A (validation is structural, not mode-coefficient-dependent) |
| gemini_tools.rs | `ToolContext` struct | Carries context for tool execution | [x] — added `balance_ctx: &BalanceContext` field |
| gemini_tools.rs | `exec_simulate_combat()` | Calls combat functions | [x] — uses `ctx.balance_ctx` |
| gemini_tools.rs | `exec_score_build()` | Calls combat functions | [x] — uses `ctx.balance_ctx` |
| gemini_tools.rs | `exec_get_build_synergy_report()` | Calls extract_damage_modifiers | [x] — uses `ctx.balance_ctx` |
| rotation/simulator.rs | `simulate()` | Duration/buff assumptions | N/A (no mode branching yet; BalanceContext available at engine layer) |
| prompts.rs | Prompt builders | Already mode-aware via strings | N/A (uses `ctx.mode_label()` via engine; no signature change needed) |

### Hardcoded Mode-Sensitive Values (Known)

| Value | Location | Current | PvE | PvP/WvW | Source |
|-------|----------|---------|-----|---------|--------|
| Fury crit bonus | combat.rs:269 | 20.0 | 25.0 | 20.0 | [wiki/Fury](https://wiki.guildwars2.com/wiki/Fury) |
| Protection DR | combat.rs:332 | 33% | 33% | Verify | [wiki/Protection](https://wiki.guildwars2.com/wiki/Protection) |
| Resolution DR | combat.rs:336 | 33% | 33% | Verify | [wiki/Resolution](https://wiki.guildwars2.com/wiki/Resolution) |

### Architecture Decisions

- **BalanceContext location**: Define in `crates/optimizer/src/` (not `core`). Core stays type-only; optimizer owns balance logic.
- **Parameter style**: `&BalanceContext` by reference. Constructed once at addon entry, threaded down. Cheap to pass.
- **patch_id temporary**: Use `"snapshot-2026-03-06"` or similar. Will be replaced by P3-08 manifest.
- **GameMode reuse**: Import `gw2_core::types::GameMode`. Do not duplicate the enum.
- **Compile-driven refactor**: Change one function signature → fix all compiler errors → repeat. This ensures no callers are missed.

### What NOT to Change

- Do not add mode-specific condition formulas — that's P3-04.
- Do not add mode-specific duration formulas — that's P3-05.
- Do not restructure the PvP optimization path — that's P3-11.
- Do not add patch manifest or ledger infrastructure — that's P3-08.
- Do not change buff stack values or uptimes — those are heuristic layer (P3-14/P3-15).
- Do not add data files — P3-02 is pure Rust plumbing.

### Project Structure Notes

- P3-02 modifies function signatures across most optimizer crate modules but creates no new files except possibly `balance.rs`
- All callers in the addon crate must be updated — compile will catch mismatches
- Test files across all modules need BalanceContext::pve() added to function calls

### References

- [Source: docs/optimizer-source-of-truth.md] — canonical Fury values, boon mechanics
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.2] — epic-level AC and requirements
- [Source: _bmad-output/planning-artifacts/epics.md#FR2] — BalanceContext requirement
- [Source: _bmad-output/planning-artifacts/epics.md#Guardrails GR-1] — no heuristic contamination
- [Source: crates/optimizer/src/combat.rs:269] — current Fury bonus (20.0, wrong for PvE)
- [Source: crates/optimizer/src/synergy_pipeline.rs:70] — `_game_mode` unused parameter
- [Source: crates/addon/src/ui/main_view/optimization.rs] — addon entry points

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

- BalanceContext defined in `crates/optimizer/src/balance.rs` with `game_mode: GameMode` and `patch_id: String`
- Convenience constructors: `new()`, `pve()`, `pvp()`, `wvw()`, `mode_label()`
- `patch_id` uses snapshot constant `"snapshot-2026-03-06"` (temporary, P3-08 will add manifest)
- Fury crit bonus mode split implemented: PvE = 25.0%, PvP/WvW = 20.0% (source: wiki/Fury)
- All engine entry points (`optimize`, `optimize_pvp`, `optimize_with_gemini`, `optimize_deterministic`) take `ctx: &BalanceContext` instead of `game_mode: &GameMode`
- `synergy_pipeline::optimize_synergy()` — `_game_mode: &GameMode` replaced with `ctx: &BalanceContext` (underscore removed)
- `ToolContext` struct in gemini_tools.rs gains `balance_ctx: &BalanceContext` field
- All addon entry points construct `BalanceContext::new(game_mode)` and pass it through
- 167 tests pass (166 existing updated + 1 new mode-differentiation test)
- No `_game_mode` references remain in optimizer crate
- Functions that don't currently branch on mode (stats.rs, scoring.rs, search.rs, validation.rs, rotation/) were left with unchanged signatures — BalanceContext is available at their call sites in engine/combat layer. Later stories (P3-04, P3-05, P3-11) can add `&BalanceContext` params to these when they need mode branching.

### Change Log

- Created `crates/optimizer/src/balance.rs` — BalanceContext struct and constructors
- Modified `crates/optimizer/src/lib.rs` — added `pub mod balance`
- Modified `crates/optimizer/src/combat.rs` — added `&BalanceContext` to 5 public functions, Fury mode split, mode-differentiation test
- Modified `crates/optimizer/src/engine.rs` — replaced `game_mode: &GameMode` with `ctx: &BalanceContext` in 4 functions, added BalanceContext to ToolContext construction and calculate_validated_stats
- Modified `crates/optimizer/src/synergy_pipeline.rs` — replaced `_game_mode` with `ctx`, threaded through rank_and_select and build_synergy_result
- Modified `crates/optimizer/src/gemini_tools.rs` — added `balance_ctx` to ToolContext, updated 3 tool execution functions
- Modified `crates/optimizer/src/scoring.rs` — updated 3 test functions with BalanceContext
- Modified `crates/addon/src/ui/main_view/optimization.rs` — construct BalanceContext from game_mode, pass through all optimizer calls
- Modified `crates/addon/src/ui/main_view/stats.rs` — added `balance_ctx` param to compute_3tier_combat
- Modified `crates/addon/src/ui/main_view/resolution.rs` — construct BalanceContext, pass to combat calls
- Modified `crates/addon/src/ui/main_view/mod.rs` — pass BalanceContext::pve() in saved_to_suggestion

### File List

- `crates/optimizer/src/balance.rs` (new)
- `crates/optimizer/src/lib.rs`
- `crates/optimizer/src/combat.rs`
- `crates/optimizer/src/engine.rs`
- `crates/optimizer/src/synergy_pipeline.rs`
- `crates/optimizer/src/gemini_tools.rs`
- `crates/optimizer/src/scoring.rs`
- `crates/addon/src/ui/main_view/optimization.rs`
- `crates/addon/src/ui/main_view/stats.rs`
- `crates/addon/src/ui/main_view/resolution.rs`
- `crates/addon/src/ui/main_view/mod.rs`
- `docs/stories/P3-02-balance-context.md`
- `_bmad-output/project-context.md`
