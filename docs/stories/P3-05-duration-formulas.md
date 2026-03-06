# Story 3.05: Duration Formulas

Status: ready-for-dev

## Story

As a GW2 player,
I want condition and boon durations to be calculated correctly using expertise, concentration, and explicit duration modifiers,
so that builds investing in duration stats see accurate uptime projections.

## Non-Goals

- **No uptime modeling** -- this story calculates outgoing duration multipliers only. Actual uptime (what fraction of a fight a condition/boon is active) is heuristic work in P3-14.
- **No new data files** -- duration constants (`expertise_per_condition_duration_pct`, `concentration_per_boon_duration_pct`, `condition_duration_cap`, `boon_duration_cap`) already live in `data/formulas/universal.json` (Schema 3 in `docs/optimizer-data-schemas.md`). P3-03 creates and loads this file; P3-05 consumes it.
- **No mode-split duration caps** -- caps are currently mode-invariant (both 1.0). The `BalanceContext` parameter is accepted for future-proofing but not branched on.
- **No rotation or scoring changes** -- this story replaces hardcoded divisors/caps in duration math, not the scoring pipeline or condition weight system.

## Dependencies

- **P3-02** (done) -- `BalanceContext` type and plumbing. Duration functions accept `&BalanceContext`.
- **P3-03** (blocked) -- creates `data/formulas/universal.json` and its loader. P3-05 reads `expertise_per_condition_duration_pct` (15), `concentration_per_boon_duration_pct` (15), `condition_duration_cap` (1.0), and `boon_duration_cap` (1.0) from the loaded data.
- **Downstream**: P3-14 (rotation profiles) will call these duration functions to project condition/boon uptime.

**Implementation note**: The duration formula functions themselves can be written and tested immediately by accepting the constants as explicit parameters. Integration with P3-03's loaded data (replacing parameter pass-through with reads from the universal formula struct) is gated on P3-03 completion.

## Acceptance Criteria

1. **Condition duration formula**: `base_duration * (1 + expertise/1500 + specific_modifier)`, where the divisor 1500 is derived from `expertise_per_condition_duration_pct: 15` (i.e., `15 expertise = 1% = 0.01`, so `1 expertise = 1/1500`). Source: [wiki/Expertise](https://wiki.guildwars2.com/wiki/Expertise).
2. **Boon duration formula**: `base_duration * (1 + concentration/1500)`, where the divisor 1500 is derived from `concentration_per_boon_duration_pct: 15`. Source: [wiki/Boon_Duration](https://wiki.guildwars2.com/wiki/Boon_Duration).
3. **Condition duration cap**: Total condition duration bonus capped at `condition_duration_cap` (1.0 = 100% bonus = double base duration) from universal.json, applied before multiplying base duration.
4. **Boon duration cap**: Total boon duration bonus capped at `boon_duration_cap` (1.0 = 100% bonus) from universal.json, applied before multiplying base duration.
5. **Additive stacking, then cap, then multiply**: Global duration bonus (from Expertise/Concentration) + condition-specific/boon-specific modifiers stack additively. The sum is capped. Then: `outgoing = base * (1 + capped_bonus)`.
6. **BalanceContext accepted as parameter**: Duration functions accept `&BalanceContext` even though formulas are currently mode-invariant. Future-proofs for potential mode-split caps.
7. **Hardcoded divisors/caps replaced**: The hardcoded `/ 15.0` divisors and `.clamp(0.0, 100.0)` caps in `combat.rs` duration calculations are replaced with calls to the new duration functions (or at minimum read from loaded constants).
8. **GR-1 (no heuristic contamination)**: All variable inputs (base duration, expertise, concentration, specific modifiers) are explicit parameters. No default uptime assumptions.
9. **GR-2 (source verification)**: Test expected values cite wiki sources in comments.
10. **Sources cited**: wiki/Boon_Duration, wiki/Condition, wiki/Expertise, wiki/Concentration.

## Verification

```bash
# Run optimizer tests
cargo test --package gw2-optimizer -v

# Verify no hardcoded /15.0 for duration remains in combat.rs duration paths
# (ferocity /15.0 in crit damage is separate and stays)
grep -n "expertise / 15\.0\|concentration / 15\.0" crates/optimizer/src/combat.rs  # should be empty

# Verify duration cap not hardcoded as 100.0 in duration paths
grep -n "clamp(0.0, 100.0)" crates/optimizer/src/combat.rs  # should only remain for crit chance
```

## Tasks / Subtasks

### 1. Create duration formula functions (AC: 1, 2, 3, 4, 5, 6, 8)

- [ ] Add duration formula functions in `crates/optimizer/src/combat.rs` (or a new `duration.rs` if preferred)
  - [ ] `fn condition_duration_multiplier(base_duration: f64, expertise: f64, global_condi_dur_bonus: f64, specific_condi_dur_bonus: f64, cap: f64, _ctx: &BalanceContext) -> f64`
    - Computes: `base_duration * (1.0 + ((expertise / 1500.0) + global_condi_dur_bonus + specific_condi_dur_bonus).min(cap))`
    - The divisor 1500.0 is derived from the expertise constant (15 expertise per 1% = 15/0.01 = 1500)
  - [ ] `fn boon_duration_multiplier(base_duration: f64, concentration: f64, global_boon_dur_bonus: f64, cap: f64, _ctx: &BalanceContext) -> f64`
    - Computes: `base_duration * (1.0 + ((concentration / 1500.0) + global_boon_dur_bonus).min(cap))`
  - [ ] `fn condition_duration_bonus(expertise: f64, global_condi_pct: f64, specific_condi_pct: f64, cap: f64) -> f64`
    - Returns the capped bonus ratio: `((expertise / 1500.0) + global_condi_pct + specific_condi_pct).min(cap)`
    - Useful for the existing `condi_duration_pct` / per-condition `*_dur` calculation in `calculate_combat_performance()`
  - [ ] `fn boon_duration_bonus(concentration: f64, global_boon_pct: f64, cap: f64) -> f64`
    - Returns the capped bonus ratio: `((concentration / 1500.0) + global_boon_pct).min(cap)`

### 2. Replace hardcoded duration math in `calculate_combat_performance()` (AC: 7)

File: `crates/optimizer/src/combat.rs`, lines ~303-330

- [ ] Replace `(stats.expertise / 15.0).clamp(0.0, 100.0)` (line ~304) with a call to the new duration bonus function
  - Current code works in percentage-point space (0-100); new code should work in ratio space (0.0-1.0) for consistency with `universal.json` cap convention
  - **Decision point**: The existing code uses percentage points throughout `DamageModifiers` and `CombatPerformance`. The dev agent must decide whether to:
    - (a) Convert duration internals to ratio space (0.0-1.0) and update `DamageModifiers` methods + `CombatPerformance` fields, OR
    - (b) Keep percentage-point space and derive the cap as `condition_duration_cap * 100.0` from loaded data
  - Option (b) is lower risk for this story; option (a) is cleaner long-term but has wider blast radius
- [ ] Replace `(stats.concentration / 15.0).clamp(0.0, 100.0)` (line ~329) similarly
- [ ] Replace per-condition duration lines (~310-314): use the new `condition_duration_bonus()` function with per-condition specific modifiers
- [ ] Ensure `condi_duration_pct` and `boon_duration_pct` fields on `CombatPerformance` still report correct values for UI display

### 3. Integrate with P3-03 loaded data (blocked on P3-03) (AC: 7)

- [ ] Once P3-03 delivers the universal formula loader, replace any remaining literal `1500.0` with a value derived from the loaded `expertise_per_condition_duration_pct` field: `divisor = 100.0 / expertise_per_condition_duration_pct` (i.e., `100.0 / 15.0 = 6.667` per 1%, so `1 / (15.0 / 100.0 / 15.0)` = 1500)
  - Simpler: `divisor = 100.0 * expertise_per_condition_duration_pct` (since 15 expertise = 1% = 0.01, so per 1 expertise = 0.01/15 = 1/1500)
  - Actually: `expertise_per_condition_duration_pct: 15` means "15 Expertise = 1 percentage point". So bonus ratio = `expertise / (expertise_per_condition_duration_pct * 100)` = `expertise / 1500`
- [ ] Replace literal cap values with reads from loaded `condition_duration_cap` and `boon_duration_cap`
- [ ] Same for `concentration_per_boon_duration_pct`

### 4. Write tests with source citations (AC: 9, 10)

- [ ] `test_condition_duration_basic` -- 450 Expertise + 20% Burning Duration modifier: `3.0 * (1 + 450/1500 + 0.20) = 3.0 * 1.50 = 4.5s`. Source: wiki/Expertise
- [ ] `test_boon_duration_basic` -- 600 Concentration: `5.0 * (1 + 600/1500) = 5.0 * 1.40 = 7.0s`. Source: wiki/Boon_Duration
- [ ] `test_condition_duration_cap` -- 1800 Expertise + 30% modifier: raw bonus = 1800/1500 + 0.30 = 1.50, capped at 1.0. Result: `3.0 * (1 + 1.0) = 6.0s`. Source: wiki/Condition_Duration ("maximum 100%")
- [ ] `test_boon_duration_cap` -- 2000 Concentration: raw bonus = 2000/1500 = 1.333, capped at 1.0. Result: `5.0 * 2.0 = 10.0s`. Source: wiki/Boon_Duration ("maximum 100%")
- [ ] `test_condition_duration_additive_stacking` -- global 10% + specific Burning 20% + Expertise 300: bonus = 300/1500 + 0.10 + 0.20 = 0.50. Result: `4.0 * 1.50 = 6.0s`
- [ ] `test_zero_expertise_zero_modifiers` -- 0 Expertise, no modifiers: bonus = 0.0. `base * 1.0 = base`
- [ ] `test_duration_bonus_ratio_values` -- verify `condition_duration_bonus()` and `boon_duration_bonus()` return correct ratio values for known inputs
- [ ] `test_combat_performance_condi_duration_matches` -- existing test `test_condi_build_has_high_condi_dps` (expertise 600, expects ~40% duration) still passes with new implementation
- [ ] `test_balance_context_accepted` -- verify duration functions compile and run with `BalanceContext::pve()` parameter (signature future-proofing)

### 5. Verify existing tests still pass (AC: 7)

- [ ] Run full `cargo test --package gw2-optimizer` -- no regressions
- [ ] Specifically verify `test_condi_build_has_high_condi_dps` (combat.rs ~888) still expects `condi_duration_pct` ~40.0 for 600 Expertise
- [ ] Verify `test_parse_rune_modifier_*` tests still pass (these test modifier extraction, not duration math)

## Dev Notes

### Current State of Duration Calculations

Duration math currently lives inline in `calculate_combat_performance()` in `crates/optimizer/src/combat.rs` (lines ~303-330). Key observations:

**Condition duration (lines 303-314):**
```rust
// Hardcoded /15.0 divisor, hardcoded 100.0 cap (percentage points)
let base_condi_duration = (stats.expertise / 15.0).clamp(0.0, 100.0);
let total_condi_duration = (base_condi_duration + modifiers.total_condi_duration_bonus()).clamp(0.0, 100.0);

// Per-condition: adds base + per-condition-specific modifiers, clamps, converts to multiplier
let bleed_dur = 1.0 + (base_condi_duration + modifiers.total_condi_duration_for("Bleeding")).clamp(0.0, 100.0) / 100.0;
// ... same for Burning, Poison, Torment, Confusion
```

**Boon duration (lines 328-330):**
```rust
let base_boon_duration = (stats.concentration / 15.0).clamp(0.0, 100.0);
let boon_duration_pct = (base_boon_duration + modifiers.total_boon_duration_bonus()).clamp(0.0, 100.0);
```

**Issues to fix:**
1. `/ 15.0` is a hardcoded magic number -- should derive from loaded `expertise_per_condition_duration_pct` (value: 15)
2. `.clamp(0.0, 100.0)` is a hardcoded cap in percentage-point space -- should derive from loaded `condition_duration_cap` (value: 1.0, ratio space)
3. Duration logic is inline rather than in reusable functions

### Unit Conventions (Important)

The existing codebase works in **percentage-point space** for duration:
- `DamageModifiers.condi_duration_pct` stores values as decimal ratios (e.g., 0.10 for 10%)
- `DamageModifiers.total_condi_duration_bonus()` returns percentage points (multiplies by 100.0): e.g., 10.0 for 10%
- `CombatPerformance.condi_duration_pct` and `boon_duration_pct` are percentage points (0-100)
- Expertise `/ 15.0` yields percentage points directly (600 expertise / 15 = 40 percentage points)

The `universal.json` schema stores caps as **ratios** (1.0 = 100%).

The dev agent must bridge this gap carefully. The recommended approach (option (b) from Task 2) is to keep percentage-point space internally and convert the loaded cap: `effective_cap = condition_duration_cap * 100.0`.

### DamageModifiers Duration Methods

`DamageModifiers` (combat.rs lines 14-88) already aggregates duration modifiers:
- `condi_duration_pct: Vec<f64>` -- global condition duration percentages (stored as decimal ratios, e.g. 0.10)
- `specific_condi_duration: HashMap<String, Vec<f64>>` -- per-condition duration (same format)
- `boon_duration_pct: Vec<f64>` -- global boon duration percentages
- `total_condi_duration_bonus() -> f64` -- sums `condi_duration_pct` and multiplies by 100.0 (returns percentage points)
- `total_condi_duration_for(condition) -> f64` -- global + specific for one condition (percentage points)
- `total_boon_duration_bonus() -> f64` -- sums `boon_duration_pct` and multiplies by 100.0 (percentage points)

These methods are correct for aggregating modifiers. The new duration functions should accept these aggregated values as inputs.

### Modifier Sources (Already Working)

Duration modifiers are already correctly extracted from:
- **Runes**: `parse_rune_modifier()` (combat.rs ~526) -- parses "+7% Burning Duration", "+10% Condition Duration" etc.
- **Sigils**: `parse_sigil_modifier()` (combat.rs ~577) -- hardcoded known sigils (Malice +10% condi duration, Concentration +3.3% boon duration, Smoldering +10% Burning, etc.)
- **Sigil descriptions**: `parse_sigil_from_description()` (combat.rs ~638) -- fallback regex-like parsing
- **Traits**: `extract_trait_modifiers()` (combat.rs ~410) -- parses Percent facts containing "duration" keywords

No changes needed to modifier extraction -- this story only changes how the extracted values are combined and capped.

### P3-03 Dependency: universal.json

P3-03 creates `data/formulas/universal.json` with this schema (from `docs/optimizer-data-schemas.md` Schema 3):

```json
{
  "level_80": {
    "expertise_per_condition_duration_pct": 15.0,
    "concentration_per_boon_duration_pct": 15.0,
    "condition_duration_cap": 1.0,
    "boon_duration_cap": 1.0
  }
}
```

Until P3-03 lands, the formula functions can accept these as explicit parameters (or use the literal values 1500.0 and 1.0 with a `// TODO: read from loaded universal formulas (P3-03)` comment). The integration task (Task 3) wires up the loaded data.

### P3-01 Data Loader Pattern (Reference)

P3-01 established the data loading pattern in `crates/optimizer/src/data/profession_profiles.rs`:
- `include_str!` embeds JSON at compile time
- `OnceLock<T>` for lazy-init singleton
- `profiles()` function returns `&'static ProfessionProfiles`
- Typed validation on load

P3-03 will follow this same pattern for `data/formulas/universal.json`. P3-05's integration (Task 3) will call into P3-03's accessor to get the constants.

### Architecture Decisions

- **Function location**: Duration functions should live in `crates/optimizer/src/combat.rs` alongside the existing combat performance calculation, or in a new `duration.rs` module if the dev agent prefers separation. Either is acceptable.
- **No new data files**: All constants are already specified in `universal.json` (P3-03 scope). P3-05 does not create data files.
- **Parameter-first design**: Functions accept constants as parameters for testability. P3-03 integration wires up the loaded values at call sites.
- **Minimal blast radius**: Keep the `DamageModifiers` percentage-point convention. Convert loaded ratio caps at the call site (`cap * 100.0`).

### Guardrails

- **GR-1 (no heuristic contamination)**: Duration functions take explicit inputs. No default uptimes, no assumed rotation lengths.
- **GR-2 (source verification)**: All test expected values cite wiki URLs in comments.
- **GR-5 (status state principle)**: Not applicable to this story (no boon/condition status modeling).

### What NOT to Change

- Do not modify condition tick damage formulas -- that is P3-04.
- Do not modify scoring weights or normalization constants -- that is P3-15.
- Do not model actual uptime or rotation -- that is P3-14.
- Do not create `data/formulas/universal.json` -- that is P3-03.
- Do not change modifier extraction logic (`parse_rune_modifier`, `parse_sigil_modifier`, `extract_trait_modifiers`) -- those are working correctly.
- Do not change `DamageModifiers` field types or method signatures unless necessary for correctness.

### Hardcoded Duration Sites (ALL must be replaced)

| Location | Code | What It Does | Line(s) |
|----------|------|-------------|---------|
| `combat.rs` | `stats.expertise / 15.0` | Expertise to condi duration % | ~304 |
| `combat.rs` | `.clamp(0.0, 100.0)` after expertise | Condi duration cap | ~304 |
| `combat.rs` | `base_condi_duration + modifiers.total_condi_duration_for(...)` `.clamp(0.0, 100.0)` | Per-condition duration cap | ~310-314 |
| `combat.rs` | `stats.concentration / 15.0` | Concentration to boon duration % | ~329 |
| `combat.rs` | `.clamp(0.0, 100.0)` after concentration | Boon duration cap | ~329-330 |

Note: `stats.ferocity / 15.0` (line ~287, ~456) is the crit damage formula, NOT duration -- leave it alone. It will be addressed by P3-03.

### References

- [Source: docs/optimizer-source-of-truth.md#Section 6 -- Duration Formulas] -- canonical formula definitions
- [Source: docs/optimizer-data-schemas.md#Schema 3 -- Universal Formulas] -- JSON schema for constants
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.5] -- epic-level AC and requirements
- [Source: _bmad-output/planning-artifacts/epics.md#FR7] -- duration formula requirement
- [Source: _bmad-output/planning-artifacts/epics.md#Guardrails GR-1, GR-2] -- implementation guardrails
- [Source: crates/optimizer/src/combat.rs:303-330] -- current hardcoded duration calculations
- [Source: crates/optimizer/src/combat.rs:14-88] -- DamageModifiers struct and duration aggregation methods
- [Source: crates/optimizer/src/balance.rs] -- BalanceContext type (P3-02)
- [Source: crates/optimizer/src/data/profession_profiles.rs] -- P3-01 data loader pattern (OnceLock + include_str)
- [Wiki: https://wiki.guildwars2.com/wiki/Boon_Duration] -- boon duration formula and 100% cap
- [Wiki: https://wiki.guildwars2.com/wiki/Condition] -- condition overview
- [Wiki: https://wiki.guildwars2.com/wiki/Expertise] -- expertise to condition duration conversion (15:1%)
- [Wiki: https://wiki.guildwars2.com/wiki/Concentration] -- concentration to boon duration conversion (15:1%)
