# Story P2-07: SavedBuild Profession Persistence & Crash-Safe Writes

Status: superseded

> **Superseded by P3-16.** All scope from P2-07 has been absorbed into Story 3.16
> (Save/Load Profession Persistence and Crash Safety). P3-16 extends the original
> P2-07 scope with engine_version/balance_manifest_version fields, save_new/save_overwrite
> API split, and DamageModifiers reconstruction from saved build config.

## Story

As a GW2 player who saves and loads builds,
I want saved builds to remember my profession and produce accurate combat metrics on reload,
so that the comparison panel shows correct health/armor/condition values instead of defaulting everything to Warrior assumptions.

Additionally, I want saved build files to survive a mid-write crash without corruption,
so that I don't silently lose saved builds if the game crashes during a save operation.

## Non-Goals

- **No changes to the optimizer or LLM pipeline**: This story is purely save/load persistence and display.
- **No migration tool**: Existing saves without `profession` will use `#[serde(default)]` and fall back to `"Warrior"` (current behavior). Users can re-save to capture the profession.
- **No changes to the build suggestion generation flow**: `BuildSuggestion` itself is unchanged.
- **No UI changes**: The comparison panel already renders combat metrics — it just needs correct data.

## Dependencies

- **Independent of P2-03** (thread hardening) — no file overlap.
- **Independent of P2-05** (elite spec condition presets) — no shared types.

## Verification

```bash
# Unit tests for SavedBuild serialization round-trip
cargo test --package gw2-core -- saved_build

# Unit tests for crash-safe storage
cargo test --package gw2-core -- storage

# Addon tests (saved_to_suggestion correctness)
cargo test --package gw2-build-optimizer -- saved_to_suggestion --test-threads=1

# Full workspace check
cargo check --workspace
cargo test --workspace --exclude gw2-build-optimizer
cargo test --package gw2-build-optimizer -- --test-threads=1
```

## Acceptance Criteria

1. `SavedBuild` (in `crates/core/src/types.rs`) gains a `profession: String` field with `#[serde(default)]` so existing saves deserialize with `profession = ""` (empty string, treated as "Warrior" fallback).
2. When a build is saved, the `profession` field is populated from the active character's profession (or the optimization context's profession).
3. `saved_to_suggestion()` (in `crates/addon/src/ui/main_view/mod.rs`) uses `saved.profession` (falling back to `"Warrior"` if empty) instead of hardcoded `"Warrior"` for both `compute_derived()` and `compute_3tier_combat()`.
4. `BuildStorage::save()` (in `crates/core/src/storage.rs`) uses the temp-write + rename pattern: serialize to `{filename}.tmp`, then `std::fs::rename()` to `{filename}.json`. This matches the existing pattern in `config.rs:212`.
5. A round-trip test verifies: create `SavedBuild` with `profession = "Necromancer"` -> serialize -> deserialize -> assert `profession == "Necromancer"`.
6. A backward-compat test verifies: deserialize a JSON string WITHOUT the `profession` field -> assert `profession == ""` (empty default).
7. A crash-safety test verifies: `BuildStorage::save()` does not leave a `.json` file in a corrupt state (test can verify the `.tmp` -> rename flow).
8. All pre-existing storage and addon tests continue to pass.

## Tasks / Subtasks

- [ ] Add `profession` field to `SavedBuild` (AC 1)
  - [ ] Add `#[serde(default)] pub profession: String` to `SavedBuild` struct in `types.rs`
  - [ ] Update the existing `test_save_and_list` test to include `profession: "Necromancer".into()`
- [ ] Populate `profession` on save (AC 2)
  - [ ] Find where `SavedBuild` is constructed (likely in the save/compare UI code)
  - [ ] Set `profession` from the active character or optimization context
- [ ] Fix `saved_to_suggestion()` to use saved profession (AC 3)
  - [ ] Replace hardcoded `"Warrior"` with `let prof = if saved.profession.is_empty() { "Warrior" } else { &saved.profession };`
  - [ ] Use `prof` in both `compute_derived()` and `compute_3tier_combat()` calls
- [ ] Make `BuildStorage::save()` crash-safe (AC 4)
  - [ ] Serialize to `{filename}.tmp` first
  - [ ] `std::fs::rename("{filename}.tmp", "{filename}.json")`
  - [ ] Keep `create_new` collision check on the final `.json` path (check existence before writing .tmp)
- [ ] Add round-trip serialization test (AC 5)
- [ ] Add backward-compat deserialization test (AC 6)
- [ ] Add crash-safety test (AC 7)
- [ ] Run full verification commands (AC 8)

## Dev Notes

- **Why "Warrior" is wrong**: `compute_derived()` calls `stats::base_health(profession)` and `stats::base_defense(profession)`. Warrior has 9212 base health and 1211 defense. Necromancer has 15922 health and 920 defense. Using Warrior for a Necromancer build shows ~42% less health and ~32% more armor than reality.
- **`DamageModifiers::default()` is also lossy**: The current code uses `DamageModifiers::default()` which has all modifiers at zero. Persisting the full `DamageModifiers` is complex (it depends on traits/runes/sigils/relic). For now, defaulting is acceptable — the profession fix alone is the highest-impact correction. A future story could persist pre-computed `CombatPerformance` directly.
- **Collision check with temp-write**: The current `create_new` on the `.json` path prevents overwriting. With temp-write + rename, check `path.exists()` before writing `.tmp`, then rename atomically. The rename itself is atomic on most filesystems.
- **Where is SavedBuild constructed?**: Search for `SavedBuild {` in the addon crate to find the construction site. The profession should come from `state.main.selected_character` or `state.main.resolved_build.profession`.
- **Backward compat**: `#[serde(default)]` on `String` gives `""`. The code should treat empty string same as missing — fall back to "Warrior".

### Project Structure Notes

- Modify: `crates/core/src/types.rs` — add `profession` field to `SavedBuild`
- Modify: `crates/core/src/storage.rs` — temp-write + rename in `save()`
- Modify: `crates/addon/src/ui/main_view/mod.rs` — fix `saved_to_suggestion()` profession usage

### References

- [Source: crates/core/src/types.rs:276-297] — `SavedBuild` struct
- [Source: crates/addon/src/ui/main_view/mod.rs:1679-1724] — `saved_to_suggestion()` function
- [Source: crates/core/src/storage.rs:22-55] — `BuildStorage::save()` method
- [Source: crates/core/src/config.rs:212-220] — existing temp-write + rename pattern to follow
- [Source: Code review finding from external AI review, 2026-03-05]

## Dev Agent Record

### Agent Model Used

_to be filled_

### Debug Log References

None.

### Completion Notes List

_to be filled_

### File List

_to be filled_

## Change Log

- 2026-03-05: Story created from external code review findings (F1: Save/Load correctness + F4: crash-safe writes). Bundled as both touch save/load persistence.
