# Story 3.09: Balance Override Datasets and Unknown-Value Handling

Status: ready-for-dev

## Story

As a GW2 player,
I want the optimizer to use patch-versioned, mode-specific balance overrides instead of single hardcoded coefficient tables,
so that when ArenaNet splits a skill coefficient between PvE and PvP/WvW, the optimizer reflects the correct mode-specific value -- and when a value is unknown after a patch, the optimizer tells me honestly instead of silently using stale data.

## Non-Goals

- **No NormalizedEffect types** -- that is P3-10a scope.
- **No effect extraction** -- that is P3-10b scope.
- **No WvW non-fallback audit** -- that is P3-12 scope (uses infrastructure from this story).
- **No comprehensive override data population** -- the initial production dataset is minimal/baseline.
- **No UI changes** -- DataQuality is surfaced through existing output types only.

## Dependencies

- **P3-02 (done)** -- BalanceContext for mode-specific lookup.
- **P3-07 (done)** -- Typed-loader pattern, DataLoadError enum.
- **P3-08 (done)** -- Patch manifest/ledger infrastructure for override references.
- **Downstream**: P3-10a (uses FactualValue), P3-12 (uses mode-specific override lookup).

## Acceptance Criteria

1. `DataQuality` enum with variants: `Verified`, `Provisional`, `Blocked`.
2. `DataQualityReason` type with: affected field/entity, affected mode(s), human-readable explanation.
3. `FactualValue<T>` wrapper: either `Resolved(T)` or `Unknown`. Unknown propagates through arithmetic (Unknown * 5 = Unknown, Unknown + 3 = Unknown).
4. Balance override data files at `data/balance_overrides/<patch_id>/<mode>.json` with: `patch_id`, `mode`, `entities` array.
5. Each entity has: `source_type`, `source_id`, `name`, `overrides` mapping field names to `{value, evidence_level}`.
6. Loader follows P3-07 pattern: `Result<T, Vec<DataLoadError>>`, strict deserialization.
7. Lookup semantics: `None` = no override (use base value, no quality degradation), `Unknown` = explicitly unresolved (degrades DataQuality).
8. No silent PvE fallback for missing WvW/PvP data -- `None` returned, caller decides.
9. Optimizer output type includes `DataQuality` field and `Vec<DataQualityReason>`.
10. Initial production baseline files may be empty. Test fixtures cover full load/lookup/quality path.

## Technical Context

### DataQuality (place in `crates/optimizer/src/data/mod.rs` or new file)

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum DataQuality {
    Verified,
    Provisional,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct DataQualityReason {
    pub field: String,
    pub entity: String,
    pub modes: Vec<String>,
    pub explanation: String,
}
```

### FactualValue<T>

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FactualValue<T> {
    Resolved(T),
    Unknown,
}

impl FactualValue<f64> {
    pub fn map_or_unknown(self, f: impl FnOnce(f64) -> f64) -> Self {
        match self {
            FactualValue::Resolved(v) => FactualValue::Resolved(f(v)),
            FactualValue::Unknown => FactualValue::Unknown,
        }
    }
}

// Arithmetic: Unknown propagates
impl std::ops::Mul<f64> for FactualValue<f64> {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        match self {
            FactualValue::Resolved(v) => FactualValue::Resolved(v * rhs),
            FactualValue::Unknown => FactualValue::Unknown,
        }
    }
}
// Similar for Add, Sub, Div
```

### Balance Override Data Schema

`data/balance_overrides/2026-01-13/pve.json`:
```json
{
  "patch_id": "2026-01-13",
  "mode": "PvE",
  "entities": []
}
```

Baseline files are empty (no overrides for baseline patch). Test fixtures use synthetic entries:
```json
{
  "patch_id": "test",
  "mode": "PvE",
  "entities": [
    {
      "source_type": "skill",
      "source_id": 12345,
      "name": "Fireball",
      "overrides": {
        "damage_coefficient": { "value": 0.9, "evidence_level": "Factual", "source": "https://wiki..." },
        "burning_duration": { "value": null, "evidence_level": "Unknown" }
      }
    }
  ]
}
```

### Override Loader Pattern

Use `include_str!` for baseline files. The loader should embed the initial empty baseline files at compile time. Future patch overrides would be loaded at runtime (but that's a future concern).

```rust
pub struct BalanceOverrides {
    entries: HashMap<(String, String), OverrideFile>,  // (patch_id, mode) -> file
}

impl BalanceOverrides {
    pub fn lookup(&self, patch_id: &str, mode: &str, source_type: &str, source_id: u32, field: &str) -> Option<OverrideEntry> {
        // Returns None if no override exists
        // Returns Some with value=null for Unknown entries
    }
}
```

### Integration with Optimizer Output

The optimizer's top-level result types (`SynergyResult`, `BuildCandidate`) need a `data_quality: DataQuality` field and `quality_reasons: Vec<DataQualityReason>`.

For this story, the field is always `Verified` (baseline has no overrides). The infrastructure is wired but only produces non-Verified when test fixtures are used or when a future story populates real overrides.

## Tasks

- [ ] 1. Implement `DataQuality` enum and `DataQualityReason` struct (AC: 1, 2)
- [ ] 2. Implement `FactualValue<T>` with Unknown propagation and arithmetic ops (AC: 3)
- [ ] 3. Create empty baseline override files `data/balance_overrides/2026-01-13/{pve,pvp,wvw}.json` (AC: 4)
- [ ] 4. Create `crates/optimizer/src/data/balance_overrides.rs` with types, loader, validation (AC: 5, 6)
- [ ] 5. Implement override lookup with None vs Unknown semantics (AC: 7, 8)
- [ ] 6. Register module in `data/mod.rs` (AC: 6)
- [ ] 7. Add `data_quality` and `quality_reasons` fields to optimizer output types (AC: 9)
- [ ] 8. Add test fixtures with synthetic override entries (AC: 10)
- [ ] 9. Add tests: FactualValue arithmetic, None vs Unknown lookup, DataQuality propagation, error paths (AC: 3, 7, 10)

## Verification

```bash
cargo test --package gw2-optimizer -v
cargo check
```

## Dev Notes

- `FactualValue<T>` is generic but arithmetic impls only needed for `f64` and maybe `u32` initially.
- The `None` vs `Unknown` distinction is critical: `None` = sparse data (normal), `Unknown` = explicitly unresolved (quality issue). Never conflate these.
- Initial production files are empty entities arrays. Non-trivial testing uses inline JSON strings in tests, not separate test fixture files.
- DataQuality is always Verified in production with the baseline. The infrastructure exists for future stories.
- Place DataQuality and FactualValue in `data/mod.rs` or a dedicated `data/quality.rs` for shared access.
- The optimizer output integration (Task 7) is minimal: add fields to SynergyResult/BuildCandidate, default to Verified.

### References

- [Source: _bmad-output/planning-artifacts/epics.md, Story 3.9]
- [Source: crates/optimizer/src/data/mod.rs, existing loader infrastructure]
- [Source: crates/optimizer/src/engine.rs, SynergyResult and BuildCandidate types]
