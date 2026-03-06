# Story 3.07: Typed Loaders and Hardcoded Constant Replacement

Status: ready-for-dev

## Story

As a GW2 player,
I want the optimizer to load all factual data from validated data files at startup instead of using hardcoded constants scattered through the source code,
so that fixing a wrong value or supporting a balance patch requires editing a data file, not rebuilding the DLL.

## Non-Goals

- **No patch manifest loading** -- that is P3-08 scope.
- **No balance override loading** -- that is P3-09 scope.
- **No effect data loading** -- that is P3-10a scope.
- **No new data files** -- all Phase A data files already exist from P3-01 through P3-06.
- **No rotation profile loading** -- that is P3-14 scope.
- **No UI changes** -- this is engine-internal plumbing only.

## Dependencies

- **P3-01 through P3-06 (all done)** -- all Phase A data files and loaders exist.
- **Downstream**: P3-08 and later stories follow the loader infrastructure pattern established here.

## Acceptance Criteria

1. A unified `DataLoadError` typed enum exists in `crates/optimizer/src/data/` with variants for each failure mode (missing file, parse error, validation error, etc.). NOT `anyhow` or string errors.
2. Each Phase A loader (profession_profiles, universal_formulas, boon_condition_formulas, slot_budgets) already uses `include_str!` + `OnceLock` pattern. This story adds a `DataLoadError` return path and validation that integrates with a startup health check.
3. A `DataState` (or similar) struct exists that tracks the overall health of loaded data: `Ready`, `Degraded(reasons)`, `Disabled(errors)`.
4. All Phase A data is loaded once at startup (already the case via `OnceLock`). If any required Phase A data fails validation, the optimizer enters `Disabled` state with a user-visible error message.
5. If optional later-phase data (P3-08+ manifests, overrides) is missing, the optimizer enters `Degraded` state (not `Disabled`).
6. The gear search in `engine.rs` consumes slot-budget data from the loaded `SlotBudgets` snapshot instead of hardcoded `SLOT_ADJUSTMENTS` and `attribute_adjustment_for_slot()`.
7. The duplicate `SLOT_ADJUSTMENTS` constant in `gemini_tools.rs` is also replaced with loaded data.
8. `synergy_pipeline.rs` references to `engine::SLOT_ADJUSTMENTS` are replaced with loaded slot-budget data.
9. All hardcoded constants that duplicate Phase A data file values are removed from `engine.rs`, `gemini_tools.rs`, `synergy_pipeline.rs`. Test fixture values and heuristic tuning constants are NOT replaced.
10. D5 is fully resolved (slot budgets consumed at runtime).
11. At least one test per loader validates the error path (malformed data returns typed `DataLoadError`).
12. Public API is clean: downstream code accesses loaded data via `data::profiles()`, `data::formulas()`, `data::boons()`, `data::conditions()`, `data::slot_budgets()`.

## Technical Context

### Current State (Phase A loaders)

Each Phase A dataset already has a working loader in `crates/optimizer/src/data/`:
- `profession_profiles.rs` -- `profiles() -> &ProfessionProfiles` via `OnceLock`
- `universal_formulas.rs` -- `formulas() -> &UniversalFormulas` via `OnceLock`
- `boon_condition_formulas.rs` -- `boons() -> &BoonFormulas`, `conditions() -> &ConditionFormulas`
- `slot_budgets.rs` -- `slot_budgets() -> &SlotBudgets` via `OnceLock`

All use `include_str!` (compile-time embedding) + `OnceLock` (lazy init). They panic on parse failure currently. This story adds graceful error handling.

### Hardcoded Constants to Replace

**engine.rs:**
- `attribute_adjustment_for_slot()` function (lines ~379-400) -- replace with `data::slot_budgets().get(slot, shape)`
- `SLOT_ADJUSTMENTS` constant (lines ~572-590) -- remove, replace usages with slot budget lookups
- References in `approximate_total_stats()` and `search_gear_prefixes()`

**gemini_tools.rs:**
- `SLOT_ADJUSTMENTS` constant (lines ~1228-1241) -- duplicate of engine.rs, replace with `data::slot_budgets()`

**synergy_pipeline.rs:**
- `engine::SLOT_ADJUSTMENTS` reference (line ~819) -- replace with slot budget lookup

### DataLoadError Design

```rust
#[derive(Debug, Clone)]
pub enum DataLoadError {
    ParseError { source: String, detail: String },
    ValidationError { source: String, field: String, reason: String },
    MissingRequired { source: String },
}
```

### DataState Design

```rust
pub enum DataState {
    Ready,
    Degraded { reasons: Vec<String> },
    Disabled { errors: Vec<DataLoadError> },
}
```

### Slot Budget Integration Pattern

Current code in `engine.rs`:
```rust
fn attribute_adjustment_for_slot(slot: &str) -> f64 {
    match slot {
        "Helm" => 141.0, "Shoulders" => 141.0, "Coat" => 225.0, ...
    }
}
```

Replace with:
```rust
use crate::data;
// For ThreeStat prefix: major = slot_budgets().get("Helm", ThreeStat).major
// The attribute_adjustment IS the major value for a 3-stat prefix
let budgets = data::slot_budgets();
let budget = budgets.get(SlotType::Helm, StatShape::ThreeStat).unwrap();
let major = budget.major as f64;
```

## Tasks

- [ ] 1. Create `DataLoadError` enum in `data/mod.rs` (AC: 1)
- [ ] 2. Create `DataState` enum with `Ready`/`Degraded`/`Disabled` variants (AC: 3)
- [ ] 3. Add `try_load()` alternatives to each loader that return `Result<T, Vec<DataLoadError>>` instead of panicking (AC: 2, 11)
- [ ] 4. Add a `data::initialize()` function that loads all Phase A data and returns `DataState` (AC: 4, 5)
- [ ] 5. Replace `attribute_adjustment_for_slot()` in `engine.rs` with slot budget lookups (AC: 6)
- [ ] 6. Replace `SLOT_ADJUSTMENTS` in `engine.rs` with slot budget iteration (AC: 6)
- [ ] 7. Replace `SLOT_ADJUSTMENTS` in `gemini_tools.rs` with slot budget lookups (AC: 7)
- [ ] 8. Replace `engine::SLOT_ADJUSTMENTS` usage in `synergy_pipeline.rs` (AC: 8)
- [ ] 9. Remove dead `attribute_adjustment_for_slot()` function and `SLOT_ADJUSTMENTS` constant (AC: 9)
- [ ] 10. Add error-path tests for each loader (AC: 11)
- [ ] 11. Verify D5 resolution: all slot constants removed, runtime uses loaded data (AC: 10)

## Verification

```bash
cargo test --package gw2-optimizer -v
cargo check
```

## Dev Notes

- The `include_str!` + `OnceLock` pattern means data is embedded at compile time. The "startup load" is really "first access triggers parse + validate". The error handling wraps this parse step.
- `attribute_adjustment_for_slot()` returns a single f64. The slot budget dataset has `major`, `minor`, and optionally `celestial` values per slot+shape. The current gear search uses `attribute_adjustment` as the major value for 3-stat prefixes. Map accordingly.
- The `SLOT_ADJUSTMENTS` array is iterated to compute total stat budget across all slots. Replace with iterating `slot_budgets().all_entries()` or similar.
- Keep the `OnceLock` pattern for backward compat. The `try_load()` functions are for the health-check path; the existing `profiles()`, `formulas()`, etc. accessors continue to work (they'll panic only if data is truly corrupt, which `initialize()` would catch first).
- Test fixtures in `engine::tests` that use hardcoded values (e.g., `test_attribute_adjustment_slots`) should be updated to test against loaded data, not removed.

### References

- [Source: _bmad-output/planning-artifacts/epics.md, Story 3.7]
- [Source: crates/optimizer/src/engine.rs, attribute_adjustment_for_slot() and SLOT_ADJUSTMENTS]
- [Source: crates/optimizer/src/data/mod.rs, existing loader infrastructure]
- [Source: docs/optimizer-data-schemas.md]
