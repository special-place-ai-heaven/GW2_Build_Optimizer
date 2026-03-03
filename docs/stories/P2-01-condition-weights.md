# Story P2-01: Profession-Aware Condition Stack Weights

Status: done

## Story

As a player using the optimizer for a condition-damage build,
I want the condition DPS score to reflect my profession's actual rotation stack counts,
so that the optimizer correctly differentiates a Burning-focused Firebrand from a Bleeding-focused Scourge.

## Non-Goals

- **No per-elite-spec presets**: Only profession-group presets (necro-group, firebrand-group, default-pve). Per-elite-spec granularity (e.g., separate Scourge vs. Harbinger presets) is a future improvement.
- **No UI controls**: Players cannot configure condition weights in the UI. The weights are selected programmatically from profession context.
- **No WvW-specific weights**: WvW condition modifier scaling is a separate concern (P2-02). This story addresses PvE condition stack counts only.
- **No changes to normalization constants**: `STRIKE_DPS_NORM`, `CONDI_DPS_NORM` are empirically tuned and must not be changed.
- **No changes to `strike_dps_index`, `effective_health`, or `healing_power_index`** calculations.
- **No changes to `ConditionTicks`**: The per-tick damage formulas are correct; only the stack-count multipliers change.

## Dependencies

- **P1-01 should be complete** so CI validates this change (existing 25+ combat tests must still pass).
- **P1-02 and P1-03 are independent** — no file overlap. Can be worked in parallel.
- **P2-04 is independent** — different file (`validation.rs` vs `combat.rs`). Can be worked in parallel.

## Verification

```bash
# Step 1 — existing combat tests pass with identical expected values
cargo test --package gw2_optimizer -- combat

# Step 2 — new differentiation tests exist and pass
cargo test --package gw2_optimizer -- test_firebrand_weights_amplify_burning_score
cargo test --package gw2_optimizer -- test_necro_weights_amplify_bleeding_torment_score

# Step 3 — full workspace check
cargo check --workspace
cargo test --workspace

# Step 4 — verify profession dispatch is wired (not just default_pve everywhere)
grep -rn "condition_weights_for_profession\|necro_group\|firebrand_group" crates/optimizer/src/
# Must show at least one use of necro_group() or firebrand_group() outside of their definitions
```

## Acceptance Criteria

1. A `ConditionWeights` struct exists in `crates/optimizer/src/combat.rs` (or a new `weights.rs`) with five `f64` fields: `bleeding`, `burning`, `poison`, `torment`, `confusion`.
2. `ConditionWeights` provides at minimum three named presets:
   - `ConditionWeights::necro_group()` — Bleeding=8.0, Burning=1.0, Poison=1.5, Torment=6.0, Confusion=0.1 (Scourge, Harbinger)
   - `ConditionWeights::firebrand_group()` — Bleeding=1.0, Burning=8.0, Poison=0.5, Torment=1.0, Confusion=0.0 (Firebrand)
   - `ConditionWeights::default_pve()` — the current values: Bleeding=3.0, Burning=2.0, Poison=1.0, Torment=1.5, Confusion=0.5 (generic fallback)
3. `calculate_combat_performance()` at `combat.rs:200` accepts a `condition_weights: &ConditionWeights` parameter and uses it in place of the hardcoded literals at `combat.rs:241-245`.
4. Callers of `calculate_combat_performance()` are updated: (a) all call sites compile with the new signature; (b) call sites in `synergy_pipeline.rs` and `engine.rs` that receive a `profession: &str` parameter pass the result of `condition_weights_for_profession(profession)` — NOT `default_pve()` — so profession dispatch is actually wired in; (c) call sites with no profession context (e.g., tests, generic helpers) may pass `&ConditionWeights::default_pve()`.
5. The existing tests in `combat.rs` that assert on `condition_dps_index` are updated to pass `&ConditionWeights::default_pve()` so they continue to pass with identical expected values.
6. At least 3 new tests:
   - `test_firebrand_weights_amplify_burning_score` — same inputs, firebrand preset produces higher `condition_dps_index` than default_pve when burning tick is dominant.
   - `test_necro_weights_amplify_bleeding_torment_score` — same inputs, necro preset produces higher `condition_dps_index` than default_pve when bleeding+torment ticks are dominant.
   - `test_condition_weights_for_profession_dispatch` — `condition_weights_for_profession("Necromancer")` returns `necro_group()` weights; `condition_weights_for_profession("Guardian")` returns `firebrand_group()` weights; `condition_weights_for_profession("Warrior")` returns `default_pve()` weights.
7. No other behavior changes — `strike_dps_index`, `effective_health`, and `healing_power_index` are unaffected.

## Tasks / Subtasks

- [x] Define `ConditionWeights` struct and presets (AC: 1, 2)
  - [x] Add `ConditionWeights { bleeding, burning, poison, torment, confusion: f64 }` to `combat.rs`
  - [x] Implement `default_pve()`, `necro_group()`, `firebrand_group()` associated functions
- [x] Update `calculate_combat_performance()` signature (AC: 3)
  - [x] Add `condition_weights: &ConditionWeights` parameter after `buffs: &BuffProfile`
  - [x] Replace `combat.rs:241-245` hardcoded literals with `condition_weights.bleeding`, etc.
- [x] Implement `condition_weights_for_profession()` helper (AC: 4, 6)
  - [x] Add `pub fn condition_weights_for_profession(profession: &str) -> ConditionWeights` in `combat.rs`
  - [x] Map: `"Necromancer"` | `"Harbinger"` | `"Scourge"` → `necro_group()`
  - [x] Map: `"Guardian"` | `"Firebrand"` | `"Willbender"` → `firebrand_group()`
  - [x] All others → `default_pve()`
- [x] Update all callers of `calculate_combat_performance()` (AC: 4)
  - [x] Grep for all call sites: `calculate_combat_performance(`
  - [x] For call sites with no profession context: pass `&ConditionWeights::default_pve()`
  - [x] For call sites in `synergy_pipeline.rs` and `engine.rs` that have `profession: &str`: pass `&condition_weights_for_profession(profession)`
- [x] Update existing tests to compile and pass (AC: 5)
  - [x] Add `&ConditionWeights::default_pve()` arg to all test calls of `calculate_combat_performance()`
- [x] Add new differentiation tests (AC: 6)
  - [x] `test_firebrand_weights_amplify_burning_score`
  - [x] `test_necro_weights_amplify_bleeding_torment_score`

## Dev Notes

- **Preset values rationale**:
  - **`necro_group`**: Scourge maintains ~8-12 Bleeding and ~5-8 Torment stacks in a full rotation. Burning is rare (0-2 stacks). Confusion is 0.1 (essentially zero in PvE).
  - **`firebrand_group`**: Firebrand's Tome of Justice can sustain 8-10 Burning stacks. Bleeding/Torment are incidental (1-2 stacks). Confusion is 0.0.
  - **`default_pve`**: Original values unchanged — used as fallback when profession isn't condition-focused or context is unavailable. Keep the current 3.0/2.0/1.0/1.5/0.5 values exactly.
  - These preset values are approximations and should be annotated with a comment citing their basis. They are intentionally conservative; future improvement is documented in the backlog.

- **Backward-compatible approach**: Add the parameter as a new required argument (not optional with default). The compiler will find all call sites. The initial pass should use `default_pve()` everywhere and then switch specific call sites to appropriate presets.

- **Where to apply profession-specific presets**:
  - In `synergy_pipeline.rs`: `optimize_synergy()` already receives `profession: &str`. Add a helper `fn condition_weights_for_profession(profession: &str) -> ConditionWeights` that maps profession name to preset (Necromancer/Harbinger → necro_group, Guardian/Willbender/Firebrand → firebrand_group, else default_pve).
  - In `engine.rs` / `scoring.rs`: anywhere combat performance is calculated for a specific profession.

- **SCREAMING_SNAKE_CASE for constants**: if you add `const DEFAULT_BLEEDING_WEIGHT: f64 = 3.0`, follow the project constant naming convention. Alternatively, embed the values directly in the associated functions — no external constants needed.

- **Do NOT change normalization constants** (`STRIKE_DPS_NORM`, `CONDI_DPS_NORM`) — these are empirically tuned and must not be changed per project rules.

- **Confusion in PvE**: The current `0.5` default is already conservative (Confusion triggers on target skill activation). For `necro_group` and `firebrand_group`, set Confusion to `0.1` or `0.0` in PvE contexts — it almost never fires in PvE auto-attack rotations.

### Project Structure Notes

- Modify: `crates/optimizer/src/combat.rs` — add struct + presets, update function signature
- Modify: all call sites of `calculate_combat_performance()` (search `crates/` for the function name)
- Possibly modify: `crates/optimizer/src/synergy_pipeline.rs` — pass profession-aware preset

### References

- [Source: docs/production-readiness-backlog.md#P2-01] — blast-radius assessment
- [Source: crates/optimizer/src/combat.rs:241-246] — hardcoded condition weights
- [Source: crates/optimizer/src/combat.rs:88-125] — ConditionTicks struct, CombatPerformance struct
- [Source: _bmad-output/project-context.md#GW2 Domain Correctness] — Confusion tick formula note
- [Source: _bmad-output/project-context.md#Critical Don't-Miss Rules] — "Never adjust normalization constants without cross-build validation"

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6

### Debug Log References

None.

### Completion Notes List

- Added `ConditionWeights` struct with `default_pve()`, `necro_group()`, `firebrand_group()` presets to `combat.rs`.
- Added `condition_weights_for_profession(profession: &str)` dispatch function — routes Necromancer/Scourge/Harbinger → necro_group, Guardian/Firebrand/Willbender → firebrand_group, all others → default_pve.
- Updated `calculate_combat_performance()` signature to accept `condition_weights: &ConditionWeights` between `buffs` and `profession`.
- Wired profession-aware dispatch in `engine.rs` (3 call sites: gear scoring, spec scoring, PvP, + 3-tier final), `synergy_pipeline.rs` (ranking + final build result), `gemini_tools.rs` (2 call sites), and `addon/stats.rs`.
- Updated all 6 scoring.rs test call sites and 5 combat.rs test call sites to pass `&ConditionWeights::default_pve()` — all expected values unchanged (backward-compatible).
- Added 3 new tests: `test_firebrand_weights_amplify_burning_score`, `test_necro_weights_amplify_bleeding_torment_score`, `test_condition_weights_for_profession_dispatch`.
- Full workspace: 164 tests pass, 0 failures, 0 regressions.

### File List

- `crates/optimizer/src/combat.rs` (modified — struct, presets, dispatch fn, signature, implementation, 5 existing test updates, 3 new tests)
- `crates/optimizer/src/engine.rs` (modified — 4 call sites wired with profession-aware dispatch; cw hoisted before both scoring loops)
- `crates/optimizer/src/synergy_pipeline.rs` (modified — 4 call sites wired with profession-aware dispatch)
- `crates/optimizer/src/gemini_tools.rs` (modified — 2 call sites wired with profession-aware dispatch)
- `crates/optimizer/src/scoring.rs` (modified — 6 test call sites updated to pass default_pve)
- `crates/addon/src/ui/main_view/stats.rs` (modified — 1 call site wired with profession-aware dispatch)
- `_bmad-output/implementation-artifacts/sprint-status.yaml` (modified — p2-01-condition-weights status updated)

## Change Log

- 2026-03-04 (claude-sonnet-4-6): Implemented P2-01 — added `ConditionWeights` struct + 3 presets, `condition_weights_for_profession()` dispatch, updated `calculate_combat_performance()` signature, wired profession-aware dispatch across engine/synergy_pipeline/gemini_tools/addon, updated all call sites, added 3 new tests. 164 tests pass.
- 2026-03-04 (claude-sonnet-4-6): Code review fixes — hoisted `cw` before both scoring loops in engine.rs (M-1), corrected dispatch function doc comment to clarify elite-spec arm intent (M-2), expanded `test_condition_weights_for_profession_dispatch` to assert all 5 fields for representative paths (L-1), added sprint-status.yaml to File List (L-2).
