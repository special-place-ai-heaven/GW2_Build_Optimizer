# Story 2.05: Per-Elite-Spec Condition Weight Presets

Status: done

## Story

As a player using a condition Scourge or Harbinger build,
I want the optimizer to use distinct condition weights for my elite spec rather than a shared necro-group preset,
so that Scourge (bleeding/torment heavy) and Harbinger (poison/torment rotations) receive appropriately differentiated scoring.

## Non-Goals

- **No changes to `necro_group()` values**: Scourge and base Necromancer continue to use the existing `necro_group()` preset (Bleeding=8.0, Burning=1.0, Poison=1.5, Torment=6.0, Confusion=0.1).
- **No changes to `firebrand_group()` or `default_pve()` values**: These presets are unchanged.
- **No new call sites**: Existing call sites of `condition_weights_for_profession()` currently pass base profession names (e.g., `"Necromancer"`). The `"Harbinger"` dispatch arm is forward-compatible for callers that may supply elite-spec names directly in the future. Only the dispatch match arm changes.
- **No UI changes**: Condition weights are internal scoring parameters, not user-visible.
- **No changes to normalization constants**: `STRIKE_DPS_NORM`, `CONDI_DPS_NORM` must not be changed.
- **No changes to `calculate_combat_performance()` signature or logic**: Only the weights passed in change.
- **No other elite-spec breakouts in this story**: Only Harbinger is split out. Further granularity (e.g., Willbender vs Firebrand, Reaper vs Scourge) is future work.

## Dependencies

- **P2-01 must be done** (it is: `ConditionWeights` struct and dispatch function exist).
- **P2-02 must be done** (it is: integration test `test_profession_dispatch_affects_condi_score` exists and passes). P2-05 extends this test.
- **Independent of P2-03, P2-06, P2-07** — no file overlap.

## Verification

```bash
# Run all combat tests (includes new and updated tests)
cargo test --package gw2-optimizer -- combat

# Run the specific new/updated tests
cargo test --package gw2-optimizer -- test_harbinger_weights_differ_from_necro
cargo test --package gw2-optimizer -- test_condition_weights_for_profession_dispatch
cargo test --package gw2-optimizer -- test_profession_dispatch_affects_condi_score

# Full workspace check — zero warnings
cargo check --workspace
cargo test --workspace --exclude gw2-build-optimizer
cargo test --package gw2-build-optimizer -- --test-threads=1
```

## Acceptance Criteria

1. A new `ConditionWeights::harbinger_preset()` associated function exists in `crates/optimizer/src/combat.rs` with values: `{ bleeding: 5.0, burning: 0.5, poison: 3.0, torment: 5.0, confusion: 0.1 }`.
   - **Rationale**: Harbinger pistol rotation applies more Poison than base Necromancer (3.0 vs 1.5), less Bleeding (5.0 vs 8.0 — Harbinger relies on elixir bursts not sustained shade pulses), and equal Torment weighting to Scourge adjusted down slightly (5.0 vs 6.0 — Harbinger lacks shade-torment application). Burning is incidental (0.5).
2. `condition_weights_for_profession()` dispatch updated: `"Harbinger"` now maps to `harbinger_preset()` instead of `necro_group()`. All other arms unchanged:
   - `"Necromancer" | "Scourge"` -> `necro_group()`
   - `"Guardian" | "Firebrand" | "Willbender"` -> `firebrand_group()`
   - `_` -> `default_pve()`
3. Existing test `test_condition_weights_for_profession_dispatch` updated: the Harbinger assertions now verify `harbinger_preset()` values (all 5 fields) instead of `necro_group()` values.
4. A new test `test_harbinger_weights_differ_from_necro` exists that:
   - Uses a condition-heavy `StatBlock` (condition_damage=2000, expertise=400) — same pattern as existing preset comparison tests.
   - Calls `calculate_combat_performance()` with `&ConditionWeights::harbinger_preset()` and with `&ConditionWeights::necro_group()`.
   - Asserts that the two `condition_dps_index` values are **different** (not equal), proving the presets produce meaningfully distinct scoring.
   - Asserts `strike_dps_index` difference < 0.01 (condition weights don't affect power damage).
5. P2-02's integration test `test_profession_dispatch_affects_condi_score` continues to pass without modification (it tests Necromancer vs Warrior, not Harbinger).
6. All pre-existing tests in `combat.rs` and `scoring.rs` continue to pass with identical expected values — no production code paths change for non-Harbinger professions.
7. `cargo check --workspace` exits with zero errors and zero warnings.

## Tasks / Subtasks

- [x] Add `harbinger_preset()` associated function to `ConditionWeights` (AC: 1)
  - [x] Add `pub fn harbinger_preset() -> Self` in the `impl ConditionWeights` block after `necro_group()`
  - [x] Values: `{ bleeding: 5.0, burning: 0.5, poison: 3.0, torment: 5.0, confusion: 0.1 }`
  - [x] Add doc comment explaining Harbinger rotation differences from Scourge
- [x] Update `condition_weights_for_profession()` dispatch (AC: 2)
  - [x] Change the first match arm from `"Necromancer" | "Scourge" | "Harbinger"` to `"Necromancer" | "Scourge"`
  - [x] Add new arm: `"Harbinger" => ConditionWeights::harbinger_preset()`
  - [x] Update the function's doc comment to note Harbinger now has its own preset
- [x] Update `test_condition_weights_for_profession_dispatch` (AC: 3)
  - [x] Find the Harbinger assertion block (currently asserts `bleeding=8.0, torment=6.0` from necro_group)
  - [x] Replace with full 5-field assertions for harbinger_preset: `bleeding=5.0, burning=0.5, poison=3.0, torment=5.0, confusion=0.1`
- [x] Add new test `test_harbinger_weights_differ_from_necro` (AC: 4)
  - [x] Copy construction pattern from `test_necro_weights_amplify_bleeding_torment_score` (StatBlock with condition_damage=2000, expertise=400)
  - [x] Call `calculate_combat_performance()` twice: once with `&ConditionWeights::harbinger_preset()`, once with `&ConditionWeights::necro_group()`
  - [x] Assert `harbinger_result.condition_dps_index != necro_result.condition_dps_index` (use `(a - b).abs() > 0.01`)
  - [x] Assert `strike_dps_index` difference < 0.01
- [x] Verify P2-02 integration test still passes (AC: 5)
  - [x] Run `cargo test --package gw2-optimizer -- test_profession_dispatch_affects_condi_score`
- [x] Run full verification suite (AC: 6, 7)
  - [x] `cargo test --package gw2-optimizer -- combat`
  - [x] `cargo check --workspace`
  - [x] `cargo test --workspace --exclude gw2-build-optimizer`
  - [x] `cargo test --package gw2-build-optimizer -- --test-threads=1`

## Dev Notes

- **Harbinger preset rationale**:
  - **Poison=3.0** (up from 1.5 in necro_group): Harbinger's pistol skills and elixirs apply Poison as a primary condition. Pistol 2 (Vile Blast) and Pistol 3 (Weeping Shots) both apply Poison stacks. Elixir utility skills apply Blight to self but Poison to enemies.
  - **Bleeding=5.0** (down from 8.0 in necro_group): Harbinger still applies Bleeding but less than Scourge's shade pulses. Scepter auto-attack chain applies Bleeding, but Harbinger rotations emphasize pistol skills more than shade torment.
  - **Torment=5.0** (down from 6.0 in necro_group): Harbinger lacks Scourge's shade-based Torment application (Sand Cascade, Desert Shroud ticks). Some Torment from shared Necromancer skills (Scepter 3 Grasping Dead with traits).
  - **Burning=0.5**: Incidental only — no primary Burning sources in Harbinger kit.
  - **Confusion=0.1**: Near-zero in PvE as with all presets (triggers on target skill activation).
  - These values are intentionally approximate — annotated as provisional estimates pending future rotation profiling (see Epic 3 P3-14).

- **Minimal blast radius**: Only `combat.rs` is modified. No changes to callers of `condition_weights_for_profession()` — they already pass profession strings that include "Harbinger". The dispatch simply routes to a different preset now.

- **Existing test patterns to follow**: Look at `test_firebrand_weights_amplify_burning_score` (lines ~1072-1108) and `test_necro_weights_amplify_bleeding_torment_score` (lines ~1110-1147) for identical construction and assertion patterns.

- **The dispatch match order matters**: Place the `"Harbinger"` arm **before** the `"Necromancer" | "Scourge"` arm, or split them into separate arms. Rust match arms are evaluated top-to-bottom; since Harbinger was previously in the combined arm, it must be separated.

- **No changes to engine.rs, synergy_pipeline.rs, gemini_tools.rs, stats.rs, scoring.rs**: All call sites already pass through `condition_weights_for_profession()` which handles the dispatch. Only the dispatch logic changes.

- **Forward compatibility note**: The epic-2-planning-seed AC draft suggested values `Poison=3.0, Bleeding=5.0, Torment=5.0, Burning=0.5, Confusion=0.1` — this story uses those exact values.

- **Future improvement**: Add a Harbinger end-to-end dispatch integration test mirroring P2-02's `test_profession_dispatch_affects_condi_score` pattern (calls `condition_weights_for_profession("Harbinger")` and feeds through `calculate_combat_performance()`). Not required for this story — the dispatch test (AC3) and preset comparison test (AC4) provide sufficient coverage.

### Project Structure Notes

- Modify: `crates/optimizer/src/combat.rs` — add `harbinger_preset()`, update dispatch match arm, update 1 existing test, add 1 new test

### References

- [Source: _bmad-output/implementation-artifacts/epic-2-planning-seed.md#P2-05] — original story spec and AC draft
- [Source: crates/optimizer/src/combat.rs:186-233] — `ConditionWeights` struct and existing presets (incl. `harbinger_preset()`)
- [Source: crates/optimizer/src/combat.rs:235-248] — `condition_weights_for_profession()` dispatch function
- [Source: crates/optimizer/src/combat.rs:1155-1209] — `test_condition_weights_for_profession_dispatch` (updated)
- [Source: crates/optimizer/src/combat.rs:1211-1249] — `test_harbinger_weights_differ_from_necro` (new)
- [Source: crates/optimizer/src/combat.rs:1251-1298] — `test_profession_dispatch_affects_condi_score` (P2-02, unchanged)
- [Source: crates/optimizer/src/combat.rs:1072-1153] — firebrand and necro preset comparison tests (pattern followed)
- [Source: docs/stories/P2-01-condition-weights.md] — foundation story, ConditionWeights architecture
- [Source: docs/stories/P2-02-condition-dispatch-integration-test.md] — integration test this story extends

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

None required — all changes compiled and tested cleanly on first pass.

### Completion Notes List

- Added `ConditionWeights::harbinger_preset()` with doc comment explaining Harbinger rotation differences (Poison-heavy pistol/elixir kit, less Bleeding than Scourge shade pulses, slightly less Torment).
- Updated `condition_weights_for_profession()` dispatch: `"Harbinger"` arm placed first (before `"Necromancer" | "Scourge"`) to route to `harbinger_preset()`. Updated doc comment.
- Updated `test_condition_weights_for_profession_dispatch`: Harbinger block now asserts all 5 fields against `harbinger_preset()` values (was 2-field check against `necro_group()`).
- Added `test_harbinger_weights_differ_from_necro`: confirms presets produce meaningfully different `condition_dps_index` values with identical stat block, and `strike_dps_index` is unaffected.
- Full verification: 221 tests pass (22 api + 8 core + 166 optimizer [incl. 26 combat] + 25 addon), zero warnings, zero regressions.

### Change Log

- 2026-03-06: Implemented P2-05 — Harbinger condition weight preset split from necro_group
- 2026-03-06: Code review — fixed stale `necro_group()` doc comment (M1), corrected Non-Goals call-site claim (M2), updated References line numbers (L1), fixed test count arithmetic (L2)

### File List

- Modified: `crates/optimizer/src/combat.rs` (added `harbinger_preset()`, updated dispatch, updated 1 test, added 1 new test)
