# Story 3.01: Profession Profiles and Health/Armor Truth

Status: done

## Story

As a GW2 player,
I want the optimizer to use correct base health and defense values for every profession,
so that all derived stats (effective HP, toughness, damage mitigation) are accurate and I can trust the optimizer's survivability comparisons.

## Non-Goals

- **No generic loader infrastructure** — this story delivers a concrete loader for profession profiles only. Generic loader infrastructure (`crates/optimizer/src/data/mod.rs` with shared traits, error types, etc.) is deferred to P3-07.
- **No BalanceContext plumbing** — profession profiles are mode-invariant (same health/armor across PvE/PvP/WvW). BalanceContext is P3-02.
- **No other data files** — only `data/profession_profiles.json`. Formulas, slot budgets, etc. are later stories.
- **No runtime reload UI** — data loads once at startup. Reload capability is deferred.

## Dependencies

- **None** — P3-01 is the first Epic 3 story and has no prerequisites.
- **Downstream**: P3-02 (BalanceContext), P3-07 (typed loaders), P3-16 (save/load) all depend on P3-01.

## Acceptance Criteria

1. **Guardian health is Low (1645)**: `base_health("Guardian")` returns `1645.0`, not `9212.0`. Value loaded from `data/profession_profiles.json`.
2. **Necromancer health is High (9212)**: `base_health("Necromancer")` returns `9212.0`, not `5922.0`. Value loaded from `data/profession_profiles.json`.
3. **All 9 professions present**: `data/profession_profiles.json` contains exactly 9 entries. Each has independent `armor_weight` and `health_class` fields never inferred from each other.
4. **Defense matches armor class**: `base_defense_level_80` = Heavy:1271, Medium:1118, Light:967 per entry.
5. **Evidence and sources**: Each entry includes `evidence_level: "Factual"` and `sources` array citing wiki URLs.
6. **Loader validates**: Malformed enum, duplicate profession, or fewer than 9 entries returns a typed validation error — no silent defaults.
7. **All hardcoded lookups replaced**: Every code path resolving profession base health or defense routes through the loaded profile. No duplicate hardcoded tables remain.
8. **`compute_derived()` uses profiles**: `StatBlock::compute_derived()` in `crates/core/src/types.rs` reads from loaded profiles, not inline match arms.
9. **project-context.md updated**: Health class table corrected (Guardian=Low, Necromancer=High).
10. **Source-cited tests**: Test expected values cite their wiki source in comments (GR-2).

## Verification

```bash
# Run all tests (optimizer crate has the loader + integration tests)
cargo test --package gw2-optimizer -v

# Run core crate tests (compute_derived uses profiles)
cargo test --package gw2-core -v

# Verify data file exists and has 9 entries
cat data/profession_profiles.json | python -c "import json,sys; d=json.load(sys.stdin); assert len(d)==9, f'Expected 9, got {len(d)}'"

# Verify no hardcoded health/defense match arms remain in stats.rs or types.rs
# (the functions should delegate to loaded profiles, not contain match arms)
grep -n "9212\|5922\|1645" crates/optimizer/src/stats.rs  # should only appear in tests
grep -n "9212\|5922\|1645" crates/core/src/types.rs       # should be gone

# Verify no .rs files outside scope were modified
git diff --name-only -- '*.rs' | grep -v 'stats.rs\|types.rs\|lib.rs\|data/\|combat.rs'  # should be empty or minimal
```

## Tasks / Subtasks

- [x] Create `data/profession_profiles.json` with all 9 professions (AC: 1, 2, 3, 4, 5)
  - [x] Each entry: profession, armor_weight, health_class, base_health_level_80, base_defense_level_80, evidence_level, sources
  - [x] Guardian: Heavy armor, Low health (1645), defense 1271
  - [x] Necromancer: Light armor, High health (9212), defense 967
  - [x] Warrior: Heavy armor, High health (9212), defense 1271
  - [x] Remaining 6 professions with correct values per source-of-truth
- [x] Create `crates/optimizer/src/data/mod.rs` module root (AC: 6)
  - [x] Define `ProfessionProfile` struct matching JSON schema
  - [x] Define enums: `ArmorWeightClass` (Heavy/Medium/Light), `HealthClass` (High/Medium/Low), `EvidenceLevel` (Factual/Derived/Heuristic/Unknown)
  - [x] Implement `load_profession_profiles(json) -> Result<ProfessionProfiles, ProfessionProfileError>`
  - [x] Typed error enum `ProfessionProfileError`: ParseError, ValidationError(String)
  - [x] Validation: exactly 9 entries, no duplicate professions, defense matches armor class, health matches health class, all enums valid
- [x] Create `crates/optimizer/src/data/profession_profiles.rs` with lookup helpers (AC: 7)
  - [x] `ProfessionProfiles` wrapper struct with `HashMap<String, ProfessionProfile>` for O(1) lookup
  - [x] `fn base_health(&self, profession: &str) -> Option<f64>`
  - [x] `fn base_defense(&self, profession: &str) -> Option<f64>`
  - [x] `fn armor_weight(&self, profession: &str) -> Option<&str>`
  - [x] Register `data` module in `crates/optimizer/src/lib.rs`
  - [x] `OnceLock<ProfessionProfiles>` with `include_str!` for compile-time embedding
  - [x] `profiles()` function for global access
- [x] Replace hardcoded lookups in `crates/optimizer/src/stats.rs` (AC: 7)
  - [x] `base_health()` delegates to loaded profiles (signature preserved for backward compat)
  - [x] `base_defense()` delegates to loaded profiles
  - [x] `armor_weight()` delegates to loaded profiles
  - [x] Callers at lines ~468-469 unchanged (they call base_health/base_defense which now delegate)
- [x] Replace hardcoded lookups in `crates/optimizer/src/combat.rs` (AC: 7)
  - [x] Lines ~324-325: already call `stats::base_health`/`stats::base_defense` — automatically use profiles
- [x] Replace hardcoded match arms in `crates/core/src/types.rs` (AC: 8)
  - [x] `StatBlock::compute_derived()` — changed to accept `base_health: i32, base_defense: i32` parameters
  - [x] Design decision: pass base values as parameters (avoids cross-crate data dependency)
  - [x] Method was dead code (no callers found) — kept with new signature for future use
- [x] Write tests with source citations (AC: 6, 10)
  - [x] `test_guardian_health_class_is_low` — asserts 1645, cites wiki Health page
  - [x] `test_necromancer_health_class_is_high` — asserts 9212, cites wiki Health page
  - [x] `test_all_9_professions_present` — all 9 professions present in loaded data
  - [x] `test_duplicate_profession_rejected` — loader rejects duplicate entry (9 entries, one duplicated, exercises HashSet path)
  - [x] `test_malformed_enum_rejected` — loader rejects invalid armor_weight
  - [x] `test_defense_armor_class_mismatch_rejected` — validates cross-field consistency
  - [x] `test_base_health_values_match_source_of_truth` — all 9 professions correct
  - [x] `test_base_defense_values_match_source_of_truth` — all 9 professions correct
  - [x] `test_armor_weight_values` — all 9 professions correct
  - [x] `test_unknown_profession_returns_none` — unknown names return None
  - [x] `test_health_class_mismatch_rejected` — validates cross-field health/class consistency
  - [x] `test_fewer_than_9_rejected` — loader rejects < 9 entries
  - [x] `test_embedded_profiles_load_successfully` — validates the embedded JSON
- [x] Update `_bmad-output/project-context.md` health class table (AC: 9)
  - [x] Guardian: HIGH -> LOW in the HP class listing
  - [x] Necromancer: MEDIUM -> HIGH in the HP class listing
  - [x] Added note: values loaded from `data/profession_profiles.json`
- [x] Update existing tests in `stats.rs` to expect corrected values
  - [x] `test_base_health`: Guardian -> 1645.0, Necromancer -> 9212.0
  - [x] Tests cite source in comments

## Dev Notes

- **This story fixes confirmed defect D1**: Guardian and Necromancer health classes are wrong in the current codebase. Guardian is HIGH health (9212) but should be LOW (1645). Necromancer is MEDIUM health (5922) but should be HIGH (9212). This is a ~5.6x error for Guardian survivability calculations.

### Hardcoded Lookup Sites (4 total — ALL must be replaced)

| Location | Function | Bug? | Lines |
|----------|----------|------|-------|
| `crates/optimizer/src/stats.rs` | `base_health()` | YES: Guardian=9212 (wrong), Necro=5922 (wrong) | 128-134 |
| `crates/optimizer/src/stats.rs` | `base_defense()` | No (armor classes are correct) | 144-151 |
| `crates/optimizer/src/stats.rs` | `armor_weight()` | No (armor classes are correct) | 154-161 |
| `crates/core/src/types.rs` | `StatBlock::compute_derived()` | YES: same Guardian/Necro bug, inline match | 215-229 |

### Call Sites (functions that consume the lookups)

| Location | Function | Lines |
|----------|----------|-------|
| `crates/optimizer/src/stats.rs` | `compute_combat_performance()` | ~468-469 |
| `crates/optimizer/src/combat.rs` | `calculate_combat_performance()` | ~324-325 |
| `crates/core/src/types.rs` | `StatBlock::compute_derived()` | ~211-230 |

### Correct Values (from `docs/optimizer-source-of-truth.md` Section 1)

| Profession | Armor Weight | Health Class | base_health_80 | base_defense_80 |
|------------|-------------|-------------|-----------------|-----------------|
| Warrior | Heavy | High | 9212 | 1271 |
| Guardian | Heavy | **Low** | **1645** | 1271 |
| Revenant | Heavy | Medium | 5922 | 1271 |
| Engineer | Medium | Medium | 5922 | 1118 |
| Ranger | Medium | Medium | 5922 | 1118 |
| Thief | Medium | Low | 1645 | 1118 |
| Elementalist | Light | Low | 1645 | 967 |
| Mesmer | Light | Medium | 5922 | 967 |
| Necromancer | Light | **High** | **9212** | 967 |

Sources:
- https://wiki.guildwars2.com/wiki/Health
- https://wiki.guildwars2.com/wiki/Armor

### Data File Schema (from `docs/optimizer-data-schemas.md` Schema 2)

```json
[
  {
    "profession": "Guardian",
    "armor_weight": "Heavy",
    "health_class": "Low",
    "base_health_level_80": 1645,
    "base_defense_level_80": 1271,
    "evidence_level": "Factual",
    "sources": [
      "https://wiki.guildwars2.com/wiki/Health",
      "https://wiki.guildwars2.com/wiki/Armor"
    ]
  }
]
```

### Architecture Decisions

- **Loader location**: `crates/optimizer/src/data/profession_profiles.rs` with `mod.rs` parent. This is the first file in `data/` — P3-07 will generalize the module later.
- **Data file location**: `data/profession_profiles.json` at repo root (per data-schemas doc directory layout).
- **Loading strategy (ADR-02)**: Parse JSON once at startup into typed in-memory struct. No file I/O on hot paths. The `ProfessionProfiles` struct provides O(1) `HashMap` lookups.
- **Cross-crate access**: The loader lives in `optimizer` crate. `core::StatBlock::compute_derived()` currently has inline match arms. Options: (a) pass `base_health` and `base_defense` as parameters to `compute_derived()`, or (b) move the profile lookup into `core`. Option (a) is simpler and avoids adding data-loading responsibility to `core`.
- **Scope boundary**: This story's loader is profession-profiles-only. It does NOT create shared loader traits, generic error types, or startup lifecycle. Those are P3-07 scope.
- **Backward compatibility**: `stats::base_health()` etc. can either be refactored to take a profiles reference, or can be thin wrappers that access a module-level `OnceLock<ProfessionProfiles>`. The dev agent should choose the approach that minimizes caller changes while avoiding global mutable state.

### Guardrails

- **GR-1 (no heuristic contamination)**: This story is purely factual. No buff stacks, uptimes, or assumptions.
- **GR-2 (source verification)**: All test expected values must cite wiki URLs in comments.
- **GR-5 (status state principle)**: Not applicable to this story (no boon/condition modeling).

### What NOT to Change

- Do not modify condition formulas, boon values, or scoring logic — those are later stories.
- Do not create generic loader traits or infrastructure — that's P3-07.
- Do not add BalanceContext or GameMode — that's P3-02.
- Do not modify files outside the scope listed in File List below unless absolutely necessary for compilation.

### Project Structure Notes

- Story follows existing module pattern: directory + `mod.rs` for multi-file modules
- `data/` directory at repo root is new — needs to be created
- `crates/optimizer/src/data/` module is new — needs `mod.rs` + `profession_profiles.rs`
- Workspace deps are hoisted to root `Cargo.toml` — if `serde` features need changing, change there

### References

- [Source: docs/optimizer-source-of-truth.md#Section 1 — Profession Profile] — canonical health/armor values
- [Source: docs/optimizer-data-schemas.md#Schema 2 — Profession Profiles] — JSON schema and validation rules
- [Source: docs/optimizer-data-schemas.md#Loader Rules] — loader behavior requirements
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.1] — epic-level AC and requirements
- [Source: _bmad-output/planning-artifacts/epics.md#Guardrails GR-1, GR-2] — implementation guardrails
- [Source: crates/optimizer/src/stats.rs:128-161] — current hardcoded functions to replace
- [Source: crates/core/src/types.rs:211-230] — compute_derived inline match arms to replace
- [Source: crates/optimizer/src/combat.rs:324-325] — caller site using stats::base_health/base_defense

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

None — clean implementation, no debugging required.

### Completion Notes List

- Created `data/profession_profiles.json` with all 9 professions, correct Guardian=Low (1645) and Necromancer=High (9212) values
- Created `crates/optimizer/src/data/` module: `mod.rs` + `profession_profiles.rs`
- Used `include_str!` + `OnceLock` pattern: JSON embedded at compile time, parsed lazily, O(1) HashMap lookups
- Replaced 3 hardcoded match-arm functions in `stats.rs` with delegations to loaded profiles
- Changed `StatBlock::compute_derived()` in `types.rs` to accept `base_health`/`base_defense` as parameters (method was dead code — no callers)
- `combat.rs` already called `stats::base_health`/`stats::base_defense` — no changes needed there
- Existing `stats.rs` tests updated to expect corrected values
- 13 new tests in `profession_profiles.rs` with wiki source citations (12 original + 1 added in review)
- All tests pass across all 4 crates (179 optimizer, 25 addon, 8 core)
- project-context.md health class table corrected

### Change Log

- 2026-03-06: P3-01 implemented — profession profiles loaded from JSON, D1 fixed (Guardian/Necromancer health classes)
- 2026-03-06: Code review fixes — H1: rewrote duplicate test to exercise HashSet path (9 entries); H2: fixed Verification package names; M1: added Engineer to stats::test_base_health; M2: added test_health_class_mismatch_rejected

### File List

- `data/profession_profiles.json` — NEW: canonical profession profile data (9 entries)
- `crates/optimizer/src/data/mod.rs` — NEW: data module root
- `crates/optimizer/src/data/profession_profiles.rs` — NEW: loader, types, OnceLock, 12 tests
- `crates/optimizer/src/lib.rs` — MODIFIED: added `pub mod data`
- `crates/optimizer/src/stats.rs` — MODIFIED: base_health/base_defense/armor_weight delegate to profiles; test_base_health corrected
- `crates/core/src/types.rs` — MODIFIED: compute_derived() takes base_health/base_defense params instead of inline match
- `_bmad-output/project-context.md` — MODIFIED: health class table corrected
- `_bmad-output/implementation-artifacts/sprint-status.yaml` — MODIFIED: p3-01 status tracking
