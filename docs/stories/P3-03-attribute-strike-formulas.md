# Story 3.03: Universal Attribute and Strike Damage Formulas

Status: done

## Story

As a GW2 player,
I want the optimizer to calculate attributes, critical chance, critical damage, and strike damage using the exact wiki-documented formulas loaded from data,
so that the base math underlying every build comparison is verifiably correct.

## Non-Goals

- **No mode-specific formulas** -- universal.json contains ONLY mode-invariant constants. Mode-split formulas (Fury PvE vs PvP, Torment/Confusion mode variants, boon/condition mode coefficients) are P3-04.
- **No duration formula functions** -- expertise-to-condition-duration and concentration-to-boon-duration *conversion constants* are stored in universal.json, but the actual duration calculation functions are P3-05 scope.
- **No weapon-strength or skill-coefficient extraction** -- `skill_damage_term` is the already-normalized damage input (API tooltip fact or pre-computed). Decomposing weapon-strength * coefficient is a separate concern.
- **No generic loader infrastructure** -- this story delivers a concrete loader for universal formulas only. Generic loader traits are P3-07.
- **No crit-chance unit normalization to ratio** -- the epics doc mentions ratio-based representation (0.0-1.0). However, the current codebase consistently uses percentage points (0-100) for crit_chance, crit_damage, boon_duration, and condi_duration across `stats.rs`, `combat.rs`, `scoring.rs`, `types.rs`, and the UI. Changing the unit system is a cross-cutting refactor that would touch 20+ call sites and is out of scope. This story preserves the existing percentage-point convention. If a future story normalizes to ratios, it should be its own scoped effort.

## Dependencies

- **P3-01 (done)**: Provides `data/profession_profiles.json` loader pattern (`include_str!` + `OnceLock` + typed validation).
- **P3-02 (done)**: Provides `BalanceContext` in `crates/optimizer/src/balance.rs`. Formula functions should accept `&BalanceContext` for forward-compatibility, even though universal formulas are mode-invariant.
- **Downstream**: P3-04 (boon/condition formulas reference universal constants), P3-05 (duration functions use expertise/concentration divisors from universal.json), P3-07 (typed loaders may generalize the pattern).

## Acceptance Criteria

1. **Data file exists**: `data/formulas/universal.json` contains:
   - `base_primary_attribute`: 1000
   - `vitality_to_health`: 10
   - `precision_offset`: 895
   - `precision_per_crit_pct`: 21
   - `ferocity_per_crit_damage_pct`: 15
   - `base_crit_damage_pct`: 150
   - `expertise_per_condition_duration_pct`: 15
   - `concentration_per_boon_duration_pct`: 15
   - `condition_duration_cap`: 1.0
   - `boon_duration_cap`: 1.0
   - `tooltip_reference_armor`: 2597
   - `evidence_level`: "Factual"
   - `sources`: array of wiki URLs
2. **Critical chance formula**: `(Precision - precision_offset) / precision_per_crit_pct`, capped at 100.0 (percentage points). Constants loaded from data. Source: https://wiki.guildwars2.com/wiki/Critical_Chance
3. **Critical damage formula**: `base_crit_damage_pct + Ferocity / ferocity_per_crit_damage_pct`. Divisor from loaded data. Source: https://wiki.guildwars2.com/wiki/Ferocity
4. **Strike damage formula**: `skill_damage_term * (Power / base_primary_attribute) * (tooltip_reference_armor / target_armor)`. `tooltip_reference_armor` from loaded data (2597). Source: https://wiki.guildwars2.com/wiki/Damage
5. **Vitality-to-health**: `health = base_health + Vitality * vitality_to_health`. Multiplier from loaded data. Source: https://wiki.guildwars2.com/wiki/Health
6. **Duration cap values stored**: `condition_duration_cap` (1.0) and `boon_duration_cap` (1.0) present in data file for downstream P3-05 consumption. Not yet wired into runtime in this story.
7. **No mode-specific values**: Loader validates that no mode-specific fields exist in universal.json.
8. **All hardcoded constants in runtime paths replaced**: The magic numbers 895, 21, 15, 150, 2597, 10 (vitality multiplier), and 1000 (base primary attribute) are read from loaded data in `stats.rs` compute_derived, `combat.rs` calculate_combat_performance, and `rotation/simulator.rs` REFERENCE_ARMOR.
9. **GR-1**: All variable inputs to formula functions are explicit parameters. No hardcoded defaults for buff stacks, uptimes, or assumptions.
10. **GR-2**: Test expected values cite wiki sources in comments. At least one test per formula.

## Verification

```bash
# Run optimizer crate tests
cargo test --package gw2-optimizer -v

# Run core crate tests (compute_derived uses formulas)
cargo test --package gw2-core -v

# Verify data file exists and has required fields
cat data/formulas/universal.json | python -c "
import json, sys
d = json.load(sys.stdin)
required = ['base_primary_attribute','vitality_to_health','precision_offset',
  'precision_per_crit_pct','ferocity_per_crit_damage_pct','base_crit_damage_pct',
  'expertise_per_condition_duration_pct','concentration_per_boon_duration_pct',
  'condition_duration_cap','boon_duration_cap','tooltip_reference_armor',
  'evidence_level','sources']
missing = [k for k in required if k not in d]
assert not missing, f'Missing keys: {missing}'
print('All required fields present')
"

# Verify no hardcoded 895 or 2597 remain outside tests and data/
grep -rn '895\b' crates/optimizer/src/stats.rs crates/optimizer/src/combat.rs crates/optimizer/src/rotation/simulator.rs | grep -v '#\[cfg(test)\]' | grep -v 'mod tests' | grep -v '// '
# Should be zero matches in non-test, non-comment code

# Verify REFERENCE_ARMOR constant is removed from combat.rs and simulator.rs
grep -n 'const REFERENCE_ARMOR' crates/optimizer/src/combat.rs crates/optimizer/src/rotation/simulator.rs
# Should be zero matches
```

## Tasks / Subtasks

### Task 1: Create `data/formulas/universal.json` (AC: 1, 6, 7)

- [x] Create `data/formulas/` directory
- [x] Create `data/formulas/universal.json` with all constants:
  ```json
  {
    "base_primary_attribute": 1000,
    "vitality_to_health": 10,
    "precision_offset": 895,
    "precision_per_crit_pct": 21,
    "ferocity_per_crit_damage_pct": 15,
    "base_crit_damage_pct": 150,
    "expertise_per_condition_duration_pct": 15,
    "concentration_per_boon_duration_pct": 15,
    "condition_duration_cap": 1.0,
    "boon_duration_cap": 1.0,
    "tooltip_reference_armor": 2597,
    "evidence_level": "Factual",
    "sources": [
      "https://wiki.guildwars2.com/wiki/Attribute",
      "https://wiki.guildwars2.com/wiki/Critical_Chance",
      "https://wiki.guildwars2.com/wiki/Ferocity",
      "https://wiki.guildwars2.com/wiki/Damage",
      "https://wiki.guildwars2.com/wiki/Health",
      "https://wiki.guildwars2.com/wiki/Boon#Boon_duration",
      "https://wiki.guildwars2.com/wiki/Condition#Condition_duration"
    ]
  }
  ```
- [x] Verify no mode-specific fields are present (no `game_mode`, no `pve_*`/`pvp_*`/`wvw_*` keys)

### Task 2: Create `crates/optimizer/src/data/universal_formulas.rs` loader (AC: 1, 7, 8)

- [x] Create `crates/optimizer/src/data/universal_formulas.rs`
- [x] Follow the P3-01 pattern: `include_str!` + `OnceLock` + typed struct + validation
  - `const UNIVERSAL_FORMULAS_JSON: &str = include_str!("../../../../data/formulas/universal.json");`
  - `static FORMULAS: OnceLock<UniversalFormulas> = OnceLock::new();`
  - `pub fn formulas() -> &'static UniversalFormulas`
- [x] Define `UniversalFormulas` struct with typed fields:
  ```rust
  #[derive(Debug, Clone, Deserialize)]
  pub struct UniversalFormulas {
      pub base_primary_attribute: f64,
      pub vitality_to_health: f64,
      pub precision_offset: f64,
      pub precision_per_crit_pct: f64,
      pub ferocity_per_crit_damage_pct: f64,
      pub base_crit_damage_pct: f64,
      pub expertise_per_condition_duration_pct: f64,
      pub concentration_per_boon_duration_pct: f64,
      pub condition_duration_cap: f64,
      pub boon_duration_cap: f64,
      pub tooltip_reference_armor: f64,
      pub evidence_level: EvidenceLevel,
      pub sources: Vec<String>,
  }
  ```
- [x] Reuse `EvidenceLevel` enum from `profession_profiles.rs` (moved to `data/mod.rs` for shared use)
- [x] Define `UniversalFormulaError` typed error enum: `ParseError`, `ValidationError(String)`
- [x] Implement `load_universal_formulas(json: &str) -> Result<UniversalFormulas, UniversalFormulaError>`
- [x] Validation rules:
  - All numeric fields must be positive (> 0)
  - `condition_duration_cap` and `boon_duration_cap` must be exactly 1.0 (hard constraint for universal)
  - `evidence_level` must be `Factual` (no derived/heuristic/unknown allowed in universal.json)
  - No extra unexpected fields (use `#[serde(deny_unknown_fields)]` on a wrapper if needed, or validate manually)
- [x] Add convenience methods on `UniversalFormulas`:
  ```rust
  /// Critical chance from Precision (percentage points, 0-100).
  /// Formula: (precision - precision_offset) / precision_per_crit_pct
  pub fn crit_chance(&self, precision: f64) -> f64

  /// Critical damage from Ferocity (percentage, e.g. 170.0 for 170%).
  /// Formula: base_crit_damage_pct + ferocity / ferocity_per_crit_damage_pct
  pub fn crit_damage(&self, ferocity: f64) -> f64

  /// Health from Vitality + profession base health.
  /// Formula: base_health + vitality * vitality_to_health
  pub fn health(&self, vitality: f64, base_health: f64) -> f64

  /// Strike damage term.
  /// Formula: skill_damage * (power / base_primary_attribute) * (tooltip_reference_armor / target_armor)
  pub fn strike_damage(&self, skill_damage: f64, power: f64, target_armor: f64) -> f64
  ```

### Task 3: Register module in `crates/optimizer/src/data/mod.rs` (AC: 8)

- [x] Add `pub mod universal_formulas;` to `crates/optimizer/src/data/mod.rs`
- [x] Add `pub use universal_formulas::UniversalFormulas;` for convenience re-export

### Task 4: Replace hardcoded constants in `crates/optimizer/src/stats.rs` (AC: 2, 3, 5, 8)

Current hardcoded locations:
- **Line 117-124** (`base_stats()`): `power: 1000.0, precision: 1000.0, toughness: 1000.0, vitality: 1000.0` -- the 1000 is `base_primary_attribute`. Replace with loaded value.
- **Line 455** (`compute_derived()`): `(stats.precision - 895.0) / 21.0` -- precision_offset and precision_per_crit_pct. Replace with `formulas().crit_chance(stats.precision)`.
- **Line 456** (`compute_derived()`): `150.0 + stats.ferocity / 15.0` -- base_crit_damage_pct and ferocity_per_crit_damage_pct. Replace with `formulas().crit_damage(stats.ferocity)`.
- **Line 459** (`compute_derived()`): `stats.vitality * 10.0` -- vitality_to_health. Replace with `formulas().vitality_to_health`.

Changes:
- [x] `base_stats()`: replace `1000.0` literals with `formulas().base_primary_attribute`
- [x] `compute_derived()`: replace crit_chance calculation with `formulas().crit_chance(stats.precision).clamp(0.0, 100.0)`
- [x] `compute_derived()`: replace crit_damage calculation with `formulas().crit_damage(stats.ferocity)`
- [x] `compute_derived()`: replace `stats.vitality * 10.0` with `stats.vitality * formulas().vitality_to_health`

### Task 5: Replace hardcoded constants in `crates/optimizer/src/combat.rs` (AC: 2, 3, 4, 5, 8)

Current hardcoded locations:
- **Line 255** (`const REFERENCE_ARMOR: f64 = 2597.0;`): tooltip_reference_armor. Remove this constant.
- **Line 285** (`calculate_combat_performance()`): `(total_precision - 895.0) / 21.0` -- precision_offset and precision_per_crit_pct.
- **Line 287** (`calculate_combat_performance()`): `150.0 + stats.ferocity / 15.0` -- base_crit_damage_pct and ferocity_per_crit_damage_pct.
- **Line 298** (`calculate_combat_performance()`): `REFERENCE_WEAPON_STRENGTH / REFERENCE_ARMOR` -- tooltip_reference_armor.
- **Line 304** (`calculate_combat_performance()`): `stats.expertise / 15.0` -- expertise_per_condition_duration_pct. (NOTE: wiring this into a duration function is P3-05 scope, but the constant replacement happens here.)
- **Line 329** (`calculate_combat_performance()`): `stats.concentration / 15.0` -- concentration_per_boon_duration_pct. (Same note as above.)
- **Line 333** (`calculate_combat_performance()`): `stats.vitality * 10.0` -- vitality_to_health.
- **Line 343** (`calculate_combat_performance()`): `REFERENCE_ARMOR` in strike_ehp calculation.

Changes:
- [x] Remove `const REFERENCE_ARMOR: f64 = 2597.0;`
- [x] Replace crit_chance formula with `formulas().crit_chance(total_precision)` (fury added separately)
- [x] Replace crit_damage formula with `formulas().crit_damage(stats.ferocity) + modifiers.total_crit_damage_bonus()`
- [x] Replace `REFERENCE_ARMOR` usages with `f.tooltip_reference_armor`
- [x] Replace expertise divisor with `f.expertise_per_condition_duration_pct`
- [x] Replace concentration divisor with `f.concentration_per_boon_duration_pct`
- [x] Replace vitality multiplier with `f.vitality_to_health`

### Task 6: Replace hardcoded constant in `crates/optimizer/src/rotation/simulator.rs` (AC: 8)

Current hardcoded location:
- **Line 36** (`const REFERENCE_ARMOR: f64 = 2597.0;`): tooltip_reference_armor.

Changes:
- [x] Remove `const REFERENCE_ARMOR: f64 = 2597.0;`
- [x] Replace all uses of `REFERENCE_ARMOR` in the simulator with `reference_armor()` helper that loads from data

### Task 7: Update `crates/core/src/types.rs` compute_derived (AC: 2, 3, 5, 8)

Current hardcoded locations:
- **Line 214**: `(self.precision - 895) as f64 / 21.0` -- precision_offset and precision_per_crit_pct
- **Line 215**: `150.0 + self.ferocity as f64 / 15.0` -- base_crit_damage_pct and ferocity_per_crit_damage_pct
- **Line 216**: `self.vitality * 10` -- vitality_to_health

Design decision: `types.rs` is in the `core` crate which should not depend on the `optimizer` crate (circular dependency). Options:
- **(a) Pass constants as parameters** -- add `base_crit_damage_pct`, `precision_offset`, etc. as parameters to `compute_derived()`. This keeps core dependency-free but adds parameter clutter.
- **(b) Pass a reference to UniversalFormulas** -- requires moving the struct definition to `core` or making `core` depend on a shared types crate. Adds coupling.
- **(c) Leave as-is with a comment** -- `compute_derived()` in types.rs was identified as dead code (no callers) in P3-01. The active runtime paths are in `stats.rs` and `combat.rs` (both in optimizer crate, already fixed in Tasks 4-5).

Recommended: **(c) Leave types.rs compute_derived as-is** with an updated comment noting the constants are canonical in `data/formulas/universal.json` and this method should be updated if callers are added. The runtime paths in optimizer crate are the ones that matter and are fully data-driven after Tasks 4-6.

- [x] Add a doc comment to `StatBlock::compute_derived()` in `types.rs` noting that the canonical source for these constants is `data/formulas/universal.json` and that the active runtime paths in `crates/optimizer/src/{stats,combat}.rs` use loaded values
- [x] Confirmed: no active callers of `types.rs::compute_derived()` -- all callers use `stats::compute_derived()` in the optimizer crate

### Task 8: Write tests with source citations (AC: 2, 3, 4, 5, 9, 10)

Tests in `crates/optimizer/src/data/universal_formulas.rs`:
- [x] `test_embedded_formulas_load_successfully` -- validates the embedded JSON parses and passes validation
- [x] `test_crit_chance_base_precision` -- Precision 1000: `(1000 - 895) / 21 = 5.0%`. Source: https://wiki.guildwars2.com/wiki/Critical_Chance
- [x] `test_crit_chance_high_precision` -- Precision 2000: `(2000 - 895) / 21 = 52.619%`. Source: https://wiki.guildwars2.com/wiki/Critical_Chance
- [x] `test_crit_chance_capped` -- Precision 5000: raw value exceeds 100, result capped at 100.0
- [x] `test_crit_damage_zero_ferocity` -- Ferocity 0: `150.0 + 0/15 = 150.0%`. Source: https://wiki.guildwars2.com/wiki/Ferocity
- [x] `test_crit_damage_with_ferocity` -- Ferocity 300: `150.0 + 300/15 = 170.0%`. Source: https://wiki.guildwars2.com/wiki/Ferocity
- [x] `test_health_from_vitality` -- Vitality 1000, base_health 9212: `9212 + 1000*10 = 19212`. Source: https://wiki.guildwars2.com/wiki/Health
- [x] `test_strike_damage_formula` -- Power 2000, skill_damage 500, target_armor 2597: `500 * (2000/1000) * (2597/2597) = 1000.0`. Source: https://wiki.guildwars2.com/wiki/Damage
- [x] `test_strike_damage_with_different_armor` -- Power 2000, skill_damage 500, target_armor 2000: `500 * (2000/1000) * (2597/2000) = 1298.5`. Demonstrates armor scaling.
- [x] `test_tooltip_reference_armor_value` -- `formulas().tooltip_reference_armor == 2597.0`. Source: https://wiki.guildwars2.com/wiki/Damage
- [x] `test_base_primary_attribute_value` -- `formulas().base_primary_attribute == 1000.0`. Source: https://wiki.guildwars2.com/wiki/Attribute
- [x] `test_validation_rejects_negative_values` -- JSON with negative precision_offset is rejected
- [x] `test_validation_rejects_non_factual` -- JSON with `evidence_level: "Heuristic"` is rejected
- [x] `test_validation_rejects_wrong_caps` -- JSON with `condition_duration_cap: 2.0` is rejected

Tests in existing test modules (updated):
- [x] Update `stats.rs::test_derived_stats_no_gear` comment to cite wiki source for the formula constants
- [x] Update `combat.rs` test comments to cite wiki sources where they verify formula outputs (sources already present in combat.rs tests)

### Task 9: Verify existing tests still pass (AC: 8)

- [x] `cargo test --package gw2-optimizer -v` -- all 194 tests pass with loaded constants (values identical, just sourced differently)
- [x] `cargo test --package gw2-core -v` -- all 15 core tests pass, unaffected
- [x] `cargo test -p gw2-build-optimizer -- --test-threads=1` -- all 25 addon tests pass
- [x] Verify no behavior change -- every replaced constant has the same numeric value as before

## Dev Notes

### Current Hardcoded Constant Locations (Complete Audit)

| File | Line(s) | Constant | Value | Replacement Field |
|------|---------|----------|-------|-------------------|
| `stats.rs` | 118-121 | base primary attributes | 1000.0 | `base_primary_attribute` |
| `stats.rs` | 455 | precision offset | 895.0 | `precision_offset` |
| `stats.rs` | 455 | precision divisor | 21.0 | `precision_per_crit_pct` |
| `stats.rs` | 456 | base crit damage | 150.0 | `base_crit_damage_pct` |
| `stats.rs` | 456 | ferocity divisor | 15.0 | `ferocity_per_crit_damage_pct` |
| `stats.rs` | 459 | vitality multiplier | 10.0 | `vitality_to_health` |
| `combat.rs` | 255 | REFERENCE_ARMOR const | 2597.0 | `tooltip_reference_armor` |
| `combat.rs` | 285 | precision offset | 895.0 | `precision_offset` |
| `combat.rs` | 285 | precision divisor | 21.0 | `precision_per_crit_pct` |
| `combat.rs` | 287 | base crit damage | 150.0 | `base_crit_damage_pct` |
| `combat.rs` | 287 | ferocity divisor | 15.0 | `ferocity_per_crit_damage_pct` |
| `combat.rs` | 298 | REFERENCE_ARMOR usage | 2597.0 | `tooltip_reference_armor` |
| `combat.rs` | 304 | expertise divisor | 15.0 | `expertise_per_condition_duration_pct` |
| `combat.rs` | 329 | concentration divisor | 15.0 | `concentration_per_boon_duration_pct` |
| `combat.rs` | 333 | vitality multiplier | 10.0 | `vitality_to_health` |
| `combat.rs` | 343 | REFERENCE_ARMOR in EHP | 2597.0 | `tooltip_reference_armor` |
| `rotation/simulator.rs` | 36 | REFERENCE_ARMOR const | 2597.0 | `tooltip_reference_armor` |
| `core/types.rs` | 214 | precision offset | 895 | (leave as-is, dead code -- see Task 7) |
| `core/types.rs` | 214 | precision divisor | 21.0 | (leave as-is, dead code -- see Task 7) |
| `core/types.rs` | 215 | base crit damage | 150.0 | (leave as-is, dead code -- see Task 7) |
| `core/types.rs` | 215 | ferocity divisor | 15.0 | (leave as-is, dead code -- see Task 7) |
| `core/types.rs` | 216 | vitality multiplier | 10 | (leave as-is, dead code -- see Task 7) |

### Constants NOT in Scope (Appear Similar But Are Different)

| File | Line | Value | Why NOT replaced |
|------|------|-------|------------------|
| `combat.rs` | 257 | `REFERENCE_WEAPON_STRENGTH: 1100.0` | This is an empirical reference (Ascended greatsword avg), not a universal formula constant. It is a heuristic baseline for DPS index comparison, not a wiki-documented formula parameter. Leave as hardcoded. |
| `combat.rs` | 270-271 | `30.0` (Might power/condi per stack) | This is a boon effect value, P3-04 scope. |
| `combat.rs` | 274-277 | `25.0`/`20.0` (Fury crit bonus) | This is a mode-split boon value, P3-04 scope. |
| `combat.rs` | 341-342 | `0.33` (Protection/Resolution DR) | These are boon effect values, P3-04 scope. |
| `combat.rs` | 345-346 | `0.65`/`0.35` (strike/condi EHP blend) | This is a heuristic weighting, not a wiki formula constant. |
| `scoring.rs` | 23-26 | `STRIKE_DPS_NORM` etc. | These are scoring normalization constants, not attribute formulas. |
| `stats.rs` | 197 (in calculate_gear_stats) | `attribute_adjustment * multiplier + value` | This is the GW2 API itemstat formula, already data-driven from API responses. Not a hardcoded constant. |

### Architecture Decisions

- **Loader location**: `crates/optimizer/src/data/universal_formulas.rs`. Follows the P3-01 pattern established by `profession_profiles.rs`.
- **Data file location**: `data/formulas/universal.json`. The `formulas/` subdirectory groups formula files (universal, boons, conditions) for later stories.
- **Loading strategy**: Same as P3-01 (ADR-02): `include_str!` embeds JSON at compile time; `OnceLock` parses lazily on first access; typed struct provides O(1) field access.
- **EvidenceLevel reuse**: The `EvidenceLevel` enum is already defined in `profession_profiles.rs`. It should either be moved to `data/mod.rs` for shared use, or imported from `profession_profiles`. Moving to `mod.rs` is preferred since P3-04 will also need it.
- **Cross-crate boundary**: `types.rs::compute_derived()` is in `core` crate and cannot depend on `optimizer`. Since it was identified as dead code (no callers) in P3-01, it is left with hardcoded values and a documentation comment. All active runtime paths are in `optimizer` crate.
- **Convenience methods**: `UniversalFormulas` provides `crit_chance()`, `crit_damage()`, `health()`, `strike_damage()` convenience methods. Call sites use these instead of manually constructing formulas from fields, ensuring consistency.
- **Unit convention preserved**: Crit chance and crit damage continue to be expressed in percentage points (0-100), matching the existing codebase convention.

### Guardrails

- **GR-1 (no heuristic contamination)**: All formula functions take variable inputs as explicit parameters. No default buff stacks, uptimes, or assumptions baked in.
- **GR-2 (source verification)**: Every test expected value cites a wiki URL in a comment. The data file itself includes a `sources` array.
- **GR-5 (status state principle)**: Not applicable to this story (no boon/condition modeling).

### What NOT to Change

- Do not modify condition tick formulas (`bleeding_tick`, `burning_tick`, etc. in combat.rs lines 148-174) -- those are P3-04 scope.
- Do not modify boon effect values (Might stacks, Fury crit bonus, Protection DR) -- those are P3-04 scope.
- Do not modify scoring normalization constants (`STRIKE_DPS_NORM`, `CONDI_DPS_NORM`, etc.) -- those are scoring heuristics, not formula constants.
- Do not modify `REFERENCE_WEAPON_STRENGTH` (combat.rs line 257) -- it is an empirical reference, not a universal formula.
- Do not modify the `DamageModifiers` struct or its methods -- those are existing patterns that work correctly.
- Do not create generic loader traits or infrastructure -- that is P3-07.
- Do not change the unit convention (percentage points vs ratios) for crit_chance or crit_damage.
- Do not introduce duration calculation functions -- wiring expertise/concentration divisors into actual duration math is P3-05.

### Project Structure Notes

- `data/formulas/` directory is new -- needs to be created.
- `crates/optimizer/src/data/universal_formulas.rs` is a new file in the existing `data/` module.
- `crates/optimizer/src/data/mod.rs` needs one line added (currently has 3 lines).
- The `EvidenceLevel` enum may need to be moved from `profession_profiles.rs` to `mod.rs` for shared use. If so, update `profession_profiles.rs` to import from `mod.rs`.

## References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.3] -- epic-level AC and requirements
- [Source: _bmad-output/planning-artifacts/prd.md#FR3,FR4] -- functional requirements
- [Source: crates/optimizer/src/stats.rs:455-459] -- current hardcoded compute_derived()
- [Source: crates/optimizer/src/combat.rs:255,285-298,304,329,333,343] -- current hardcoded combat formulas
- [Source: crates/optimizer/src/rotation/simulator.rs:36] -- duplicate REFERENCE_ARMOR constant
- [Source: crates/core/src/types.rs:213-218] -- compute_derived in core crate (dead code)
- [Source: crates/optimizer/src/data/profession_profiles.rs] -- loader pattern to follow (include_str + OnceLock)
- [Source: crates/optimizer/src/balance.rs] -- BalanceContext from P3-02
- [Source: docs/stories/P3-01-profession-profiles.md] -- completed P3-01 story (pattern reference)
- [Source: https://wiki.guildwars2.com/wiki/Critical_Chance] -- crit chance formula
- [Source: https://wiki.guildwars2.com/wiki/Ferocity] -- crit damage formula
- [Source: https://wiki.guildwars2.com/wiki/Damage] -- strike damage formula, tooltip_reference_armor (2597)
- [Source: https://wiki.guildwars2.com/wiki/Health] -- vitality-to-health multiplier
- [Source: https://wiki.guildwars2.com/wiki/Attribute] -- base primary attribute (1000 at level 80)

## Dev Agent Record

**Implemented by**: Claude Opus 4.6
**Date**: 2026-03-06

### Changes Made

1. **Created `data/formulas/universal.json`** -- 11 numeric constants + evidence_level + sources array. `#[serde(deny_unknown_fields)]` enforced on the struct.

2. **Created `crates/optimizer/src/data/universal_formulas.rs`** -- Full P3-01 pattern: `include_str!` + `OnceLock` + typed `UniversalFormulas` struct + `load_universal_formulas()` + validation + 4 convenience methods (`crit_chance`, `crit_damage`, `health`, `strike_damage`). 14 tests with wiki source citations.

3. **Moved `EvidenceLevel` to `data/mod.rs`** -- Shared across `profession_profiles.rs` and `universal_formulas.rs`. Updated `profession_profiles.rs` to import via `use super::EvidenceLevel`.

4. **Updated `data/mod.rs`** -- Added `pub mod universal_formulas` and `pub use universal_formulas::UniversalFormulas`.

5. **Replaced hardcoded constants in `stats.rs`** -- `base_stats()` uses `formulas().base_primary_attribute`; `compute_derived()` uses `formulas().crit_chance()`, `formulas().crit_damage()`, `formulas().vitality_to_health`. Added wiki source comments to `test_derived_stats_no_gear`.

6. **Replaced hardcoded constants in `combat.rs`** -- Removed `const REFERENCE_ARMOR`. `calculate_combat_performance()` now uses a single `let f = formulas()` binding for crit_chance, crit_damage, tooltip_reference_armor, expertise divisor, concentration divisor, and vitality_to_health.

7. **Replaced hardcoded constant in `rotation/simulator.rs`** -- Replaced `const REFERENCE_ARMOR` with `fn reference_armor()` that loads from data. All 5 usages updated.

8. **Updated `core/types.rs`** -- Added doc comment to `StatBlock::compute_derived()` explaining that canonical constants are in `data/formulas/universal.json` and active runtime paths use loaded values. Confirmed zero active callers of this method.

### Test Results

- `gw2-optimizer`: 194 passed, 0 failed, 9 ignored (live API tests)
- `gw2-core`: 15 passed, 0 failed
- `gw2-build-optimizer`: 25 passed, 0 failed
- Full workspace `cargo check`: clean compilation
