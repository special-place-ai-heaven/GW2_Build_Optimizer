# Story 3.01: Profession Profiles and Health/Armor Truth

Status: ready-for-dev

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
cargo test --package gw2-build-optimizer-engine -v

# Run core crate tests (compute_derived uses profiles)
cargo test --package gw2-build-optimizer-core -v

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

- [ ] Create `data/profession_profiles.json` with all 9 professions (AC: 1, 2, 3, 4, 5)
  - [ ] Each entry: profession, armor_weight, health_class, base_health_level_80, base_defense_level_80, evidence_level, sources
  - [ ] Guardian: Heavy armor, Low health (1645), defense 1271
  - [ ] Necromancer: Light armor, High health (9212), defense 967
  - [ ] Warrior: Heavy armor, High health (9212), defense 1271
  - [ ] Remaining 6 professions with correct values per source-of-truth
- [ ] Create `crates/optimizer/src/data/mod.rs` module root (AC: 6)
  - [ ] Define `ProfessionProfile` struct matching JSON schema
  - [ ] Define enums: `ArmorWeightClass` (Heavy/Medium/Light), `HealthClass` (High/Medium/Low), `EvidenceLevel` (Factual/Derived/Heuristic/Unknown)
  - [ ] Implement `load_profession_profiles(path) -> Result<Vec<ProfessionProfile>, ProfessionProfileError>`
  - [ ] Typed error enum `ProfessionProfileError`: IoError, ParseError, ValidationError(String)
  - [ ] Validation: exactly 9 entries, no duplicate professions, defense matches armor class, all enums valid
- [ ] Create `crates/optimizer/src/data/profession_profiles.rs` with lookup helpers (AC: 7)
  - [ ] `ProfessionProfiles` wrapper struct with `HashMap<String, ProfessionProfile>` for O(1) lookup
  - [ ] `fn base_health(&self, profession: &str) -> Option<f64>`
  - [ ] `fn base_defense(&self, profession: &str) -> Option<f64>`
  - [ ] `fn armor_weight(&self, profession: &str) -> Option<&str>`
  - [ ] Register `data` module in `crates/optimizer/src/lib.rs`
- [ ] Replace hardcoded lookups in `crates/optimizer/src/stats.rs` (AC: 7)
  - [ ] `base_health()` delegates to loaded profiles (keep function signature for backward compat or refactor callers)
  - [ ] `base_defense()` delegates to loaded profiles
  - [ ] `armor_weight()` delegates to loaded profiles
  - [ ] Update callers at lines ~468-469 (`compute_combat_performance`)
- [ ] Replace hardcoded lookups in `crates/optimizer/src/combat.rs` (AC: 7)
  - [ ] Lines ~324-325: `calculate_combat_performance()` uses loaded profiles
- [ ] Replace hardcoded match arms in `crates/core/src/types.rs` (AC: 8)
  - [ ] `StatBlock::compute_derived()` — remove inline `base_hp` and `base_defense` match arms
  - [ ] Design decision: either pass base values as parameters, or pass a profile reference
- [ ] Write tests with source citations (AC: 6, 10)
  - [ ] `test_guardian_health_class_is_low` — asserts 1645, cites wiki Health page
  - [ ] `test_necromancer_health_class_is_high` — asserts 9212, cites wiki Health page
  - [ ] `test_all_9_professions_present` — loader rejects < 9 or > 9
  - [ ] `test_duplicate_profession_rejected` — loader rejects duplicate entry
  - [ ] `test_malformed_enum_rejected` — loader rejects invalid armor_weight/health_class
  - [ ] `test_defense_matches_armor_class` — validates cross-field consistency
  - [ ] `test_base_health_values_match_source_of_truth` — all 9 professions correct
  - [ ] `test_base_defense_values_match_source_of_truth` — all 9 professions correct
- [ ] Update `_bmad-output/project-context.md` health class table (AC: 9)
  - [ ] Guardian: HIGH -> LOW in the HP class listing
  - [ ] Necromancer: MEDIUM -> HIGH in the HP class listing
  - [ ] Update base values line: HP = 9212 / 5922 / 1645 (confirm ordering)
- [ ] Update existing tests in `stats.rs` to expect corrected values
  - [ ] `test_base_health`: Guardian -> 1645.0, Necromancer -> 9212.0
  - [ ] Tests should cite source in comments

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

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List
