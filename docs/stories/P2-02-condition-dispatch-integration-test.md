# Story P2-02: End-to-End Condition Dispatch Integration Test

Status: done

## Story

As a developer maintaining the condition scoring pipeline,
I want an integration test that verifies `condition_weights_for_profession()` dispatch flows through to a meaningfully different `condition_dps_index` output,
so that a future accidental reversion to `default_pve()` at any call site is immediately caught by CI.

## Non-Goals

- **No production code changes**: This story is test-only. No modification to `ConditionWeights`, `condition_weights_for_profession()`, or `calculate_combat_performance()`.
- **No per-elite-spec presets**: That is P2-05. This test uses the existing `necro_group()` preset.
- **No WvW or PvP paths**: The test is PvE-only (standard `BuffProfile::default_squad()` or similar).
- **No GameDb, API calls, or file I/O**: Test must be fully self-contained.

## Dependencies

- **P2-01 must be done** (it is: condition weights dispatch is implemented and all 164 tests pass).
- **P1-01 should be done** (it is: CI validates new tests automatically).
- **Independent of P2-03** — no file overlap.

## Verification

```bash
# Run the new integration test directly
cargo test --package gw2-optimizer -- test_profession_dispatch_affects_condi_score

# Confirm existing combat tests still pass
cargo test --package gw2-optimizer -- combat

# Full workspace check
cargo check --workspace
cargo test --workspace --exclude gw2-build-optimizer
cargo test --package gw2-build-optimizer -- --test-threads=1
```

## Acceptance Criteria

1. A new `#[test]` named exactly `test_profession_dispatch_affects_condi_score` exists in `crates/optimizer/src/combat.rs`.
2. The test constructs a condition-heavy `ConditionTicks` profile: `bleeding` and `torment` stacks both non-zero (e.g., `bleeding_stacks: 10.0, torment_stacks: 6.0`). All other fields may be zero.
3. The test calls `calculate_combat_performance()` twice using the same `StatBlock`, `DerivedStats`, `DamageModifiers`, and `BuffProfile`:
   - First call: `&ConditionWeights::necro_group()`, profession `"Necromancer"`
   - Second call: `&ConditionWeights::default_pve()`, profession `"Warrior"`
4. The test asserts: `necro_result.condition_dps_index > default_result.condition_dps_index`.
5. The test is entirely self-contained — uses only stack construction patterns already present in the `combat.rs` test module; no external fixtures, no API calls, no GameDb.
6. `cargo test --package gw2-optimizer -- test_profession_dispatch_affects_condi_score` exits 0 with the test passing (not merely filtered out).
7. All pre-existing `combat.rs` tests continue to pass with identical expected values — no production code is touched.

## Tasks / Subtasks

- [x] Add integration test (AC 1–6)
  - [x] Open `crates/optimizer/src/combat.rs` and locate the `#[cfg(test)]` module
  - [x] Copy an existing `StatBlock`/`DerivedStats`/`DamageModifiers`/`BuffProfile` construction pattern from a nearby test
  - [x] Set `condition_damage: 2000.0` and `expertise: 400.0` to create a condi-heavy profile (ticks computed internally)
  - [x] Call `calculate_combat_performance()` with `&ConditionWeights::necro_group()` and `"Necromancer"`; capture as `necro_result`
  - [x] Call `calculate_combat_performance()` with `&ConditionWeights::default_pve()` and `"Warrior"`; capture as `default_result`
  - [x] Assert `necro_result.condition_dps_index > default_result.condition_dps_index`
- [x] Verify pre-existing tests still pass (AC 7)
  - [x] Run `cargo test --package gw2-optimizer -- combat` — all existing tests must pass

## Dev Notes

- **Why this test is meaningful**: `ConditionWeights::necro_group()` sets Bleeding=8.0 and Torment=6.0 vs `default_pve()` Bleeding=3.0 and Torment=1.5. With `bleeding_stacks=10` and `torment_stacks=6`, the necro group scores ≈`8.0×10 + 6.0×6 = 116` weighted units vs `default_pve` ≈`3.0×10 + 1.5×6 = 39`. The difference is large enough that floating-point rounding cannot close the gap.
- **Existing test patterns**: Look at `test_firebrand_weights_amplify_burning_score` and `test_necro_weights_amplify_bleeding_torment_score` (added in P2-01) for identical construction patterns. This test is similar but uses `condition_weights_for_profession()` indirectly via explicit preset selection.
- **`condition_ticks` construction**: The `ConditionTicks` struct is created by `calculate_condition_ticks(condition_damage, modifiers)`. Use a `condition_damage` value large enough that ticks are non-trivial (e.g., 1200.0 or higher).
- **No changes to production code**: This is test-only. If any production code needs changing to make the test compile, that is a sign the test design is wrong.

### Project Structure Notes

- Modify: `crates/optimizer/src/combat.rs` — add one new `#[test]` to the existing test module

### References

- [Source: P2-01 story] — `test_necro_weights_amplify_bleeding_torment_score` is the direct precedent
- [Source: crates/optimizer/src/combat.rs:192–240] — `ConditionWeights` struct, presets, and `condition_weights_for_profession()`
- [Source: crates/optimizer/src/combat.rs:250–280] — `calculate_combat_performance()` signature
- [Source: epic-2-planning-seed.md] — acceptance criteria basis

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

None.

### Completion Notes List

- Added `test_profession_dispatch_affects_condi_score` to `crates/optimizer/src/combat.rs` test module
- Test uses identical construction pattern to `test_necro_weights_amplify_bleeding_torment_score` (StatBlock with condition_damage=2000, expertise=400)
- Calls `condition_weights_for_profession()` to dispatch weights, then `calculate_combat_performance()` — true end-to-end dispatch test
- Asserts `necro_result.condition_dps_index > default_result.condition_dps_index` + strike_dps_index invariant
- All 7 ACs satisfied: correct test name (AC1), bleeding+torment profile (AC2), two calls with different weights/professions (AC3), assertion (AC4), self-contained (AC5), test passes (AC6), no regressions (AC7)
- Full workspace: 185 tests pass (165 optimizer + 20 addon), 0 failures

### File List

- Modified: `crates/optimizer/src/combat.rs` — added 1 new `#[test]` function

## Change Log

- 2026-03-05: Added end-to-end integration test `test_profession_dispatch_affects_condi_score` verifying condition weight dispatch flows through to `condition_dps_index` output
- 2026-03-05: Code review fixes — H1: fixed package name in verification commands (`gw2_optimizer` → `gw2-optimizer`); M1+M2: rewrote test to call `condition_weights_for_profession()` for true end-to-end dispatch coverage (was bypassing dispatch function); L1: added strike_dps_index invariant assertion; L2: fixed task subtask description
