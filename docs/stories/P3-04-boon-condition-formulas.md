# Story 3.04: Mode-Aware Boon Values and Condition Formulas

Status: ready-for-dev

## Story

As a GW2 player optimizing for PvP or WvW,
I want boon effects and condition damage formulas to use the correct mode-specific values,
so that my build's DPS and support calculations reflect the actual game balance for my chosen mode.

## Non-Goals

- **No duration formulas** -- how long boons/conditions last is P3-05 scope. This story covers what they do (effect values, tick formulas), not how long they last.
- **No application rates or uptimes** -- those are heuristic concerns in P3-14 (rotation profiles). This story delivers factual formulas and metadata only.
- **No NormalizedEffect types** -- those are P3-10a/P3-10b. This story delivers StatusDefinition-style metadata that P3-10a will consume.
- **No boon removal, corruption, or conversion modeling** -- those are effect-level operations modeled in P3-10a/P3-10b. This story only documents the factual counterpart mapping.
- **No generic loader infrastructure** -- that is P3-07. This story delivers concrete loaders for boons and conditions following the P3-01 pattern.
- **No condition stack weighting changes** -- ConditionWeights presets (P2-01) remain as-is. This story replaces tick formulas, not application assumptions.

## Dependencies

- **P3-02** (done) -- BalanceContext required for mode dispatch.
- **P3-01** (done) -- data loader pattern established in `crates/optimizer/src/data/`.
- **Downstream**: P3-05 (duration formulas), P3-07 (typed loaders), P3-10a (StatusDefinition metadata consumed), P3-14 (rotation profiles use cap/stacking metadata), P3-15 (objective profiles use metadata).

## Acceptance Criteria

1. **`data/formulas/boons.json` exists** with boon effects (Fury, Might, Protection, Resolution, Vulnerability) with mode-specific values. Fury: PvE 0.25, PvP/WvW 0.20. Might: +30 Power / +30 Condition Damage per stack. Each entry has typed value fields (`flat_additive`, `ratio`, `ratio_per_stack`, `max_stacks`).
2. **`data/formulas/conditions.json` exists** with condition damage formulas per mode. Burning base ~131, coeff 0.155 (verify L1 against wiki). Torment has stationary/moving formulas, different PvE vs PvP/WvW (verify L2). Confusion has over-time/on-skill-use, different per mode (verify L3).
3. **Bleeding and Poison same across modes**: Bleeding: `0.06*CD+22`, Poison: `0.06*CD+33.5`. Same across modes but stored per-mode in the data file for future-proofing.
4. **Per-mode precedence**: Per-mode entry takes precedence over `all_modes` when both exist. Precedence rule documented and tested (L9).
5. **All 3 modes declared**: Every condition declares all 3 modes (PvE, PvP, WvW). Multi-state conditions (Torment, Confusion) declare state dimensions explicitly.
6. **StatusDefinition metadata**: Boon/condition entries include factual stacking metadata: `stacking_mode` (intensity/duration), `max_stacks`, `max_duration`, `effect_class`, `special_mechanics` (optional), `suppression_effects` (for control/suppression conditions).
7. **Mode-differentiation tests**: Torment PvE vs PvP differs, stationary vs moving differs. Confusion PvE vs PvP differs, over-time vs on-skill-use differs. Tests assert both mode-dispatch and state-dispatch produce different outputs.
8. **All hardcoded boon/condition constants replaced**: Fury 25.0/20.0, Protection 0.33, Resolution 0.33, Vulnerability 0.01, Might 30.0 in `combat.rs` are read from loaded data. Condition tick functions delegate to loaded formulas.
9. **GR-1, GR-2 compliance**: No heuristic contamination. Mode-split coefficients (Fury, Torment, Confusion) cite wiki sources in test comments. Burning base constant wiki-verified (L1).
10. **project-context.md updated**: Condition formula table and Fury documentation updated to reflect verified values.

### Additional ACs from Epics

- **(L1)**: Verify and correct Burning level-80 base constant against cited wiki source. Known discrepancy: source-of-truth doc says `131`, current code uses `131.75`. Resolve before implementation.
- **(L2)**: Torment formulas (PvE stationary/moving, PvP/WvW stationary/moving) must be re-verified against wiki before implementation. Source-of-truth values and live code values differ significantly.
- **(L3)**: Confusion formulas (PvE over-time, PvE on-skill-use, PvP/WvW over-time, PvP/WvW on-skill-use) must be wiki-cross-checked before implementation. Source-of-truth values and live code values differ dramatically.
- **(L9)**: Define and document `all_modes` vs per-mode precedence rule.
- **Boon/condition counterpart reference**: Document factual boon-to-condition corruption mappings (e.g., Might -> Weakness, Fury -> Blind, Protection -> Vulnerability) as reference metadata. Actual effect-level modeling is P3-10a/P3-10b.
- **Suppression effects metadata**: Control/suppression conditions include `suppression_effects` documenting which output channels they affect (action_throughput, skill_frequency, strike_output, dodge_economy, positioning).

## Verification

```bash
# Run optimizer crate tests (includes new boon/condition loader + formula tests)
cargo test --package gw2-optimizer -v

# Verify data files exist
cat data/formulas/boons.json | python -c "import json,sys; d=json.load(sys.stdin); print(f'Boons: {len(d)} entries')"
cat data/formulas/conditions.json | python -c "import json,sys; d=json.load(sys.stdin); print(f'Conditions: {len(d)} entries')"

# Verify no hardcoded boon constants remain in combat.rs production code
# (0.33 for Protection/Resolution, 30.0 for Might, 25.0/20.0 for Fury should be gone from non-test code)
grep -n "0\.33\|fury_crit_bonus\|might_power.*30\|might_condi.*30\|0\.01" crates/optimizer/src/combat.rs | grep -v "^.*#\[test\]\|^.*fn test_\|^.*assert\|^.*// "

# Verify no hardcoded condition formulas remain in combat.rs
grep -n "0\.06 \* condition_damage\|0\.155 \* condition_damage\|0\.0375 \* condition_damage\|0\.0175 \* condition_damage" crates/optimizer/src/combat.rs  # should be zero

# Verify rotation/simulator.rs formulas also replaced
grep -n "0\.06 \* condition_damage\|0\.155 \* condition_damage\|0\.0375 \* condition_damage\|0\.0175 \* condition_damage" crates/optimizer/src/rotation/simulator.rs  # should be zero

# Run full workspace check
cargo check
```

## WARNING: Story Size

This is a large story. It creates two data files with extensive metadata schemas, a loader module, formula functions, and replaces hardcoded values across multiple files. **Recommended execution order**:

1. **Phase 1: Data files** -- Create `data/formulas/boons.json` and `data/formulas/conditions.json` with all entries and metadata. Wiki-verify L1/L2/L3 formulas.
2. **Phase 2: Loader** -- Create `crates/optimizer/src/data/boon_condition_formulas.rs` with types, loader, OnceLock, and lookup helpers.
3. **Phase 3: Formula functions** -- Replace hardcoded condition tick functions in `combat.rs` and `rotation/simulator.rs` with data-driven lookups. Replace hardcoded boon constants.
4. **Phase 4: Tests** -- Write comprehensive mode-dispatch, state-dispatch, and regression tests.
5. **Phase 5: Cleanup** -- Update `project-context.md`, verify no leftover hardcoded constants.

## Tasks / Subtasks

### Task 1: Create `data/formulas/boons.json` (AC: 1, 4, 6)

- [ ] Create `data/formulas/` directory (may already exist if P3-03 ran first)
- [ ] Author `data/formulas/boons.json` with entries for all GW2 boons plus Vulnerability
  - [ ] **Fury**: PvE `crit_chance_bonus: 0.25`, PvP `crit_chance_bonus: 0.20`, WvW `crit_chance_bonus: 0.20`. `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 30`, `effect_class: "offensive_stat"`. Source: https://wiki.guildwars2.com/wiki/Fury
  - [ ] **Might**: `all_modes` with `power_per_stack: 30`, `condition_damage_per_stack: 30`. `stacking_mode: "intensity"`, `max_stacks: 25`, `max_duration: 30`, `effect_class: "offensive_stat"`. Source: https://wiki.guildwars2.com/wiki/Might
  - [ ] **Protection**: `all_modes` with `incoming_strike_multiplier: 0.67`. `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 30`, `effect_class: "defensive"`. Source: https://wiki.guildwars2.com/wiki/Protection
  - [ ] **Resolution**: `all_modes` with `incoming_condition_multiplier: 0.67`. `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 30`, `effect_class: "defensive"`. Source: https://wiki.guildwars2.com/wiki/Resolution
  - [ ] **Vulnerability**: `all_modes` with `incoming_damage_pct_per_stack: 0.01`, `max_stacks: 25`. `stacking_mode: "intensity"`, `effect_class: "debuff"`, `secondary_effects: "Increases strike and condition damage taken"`. Source: https://wiki.guildwars2.com/wiki/Vulnerability
  - [ ] **Quickness**: `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 30`, `effect_class: "offensive_throughput"`, `special_mechanics: "Increases action speed; changes cast throughput, not a stat modifier"`. Source: https://wiki.guildwars2.com/wiki/Quickness
  - [ ] **Alacrity**: `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 30`, `effect_class: "offensive_throughput"`, `special_mechanics: "Reduces skill recharge time; changes skill frequency, not a stat modifier"`. Source: https://wiki.guildwars2.com/wiki/Alacrity
  - [ ] **Aegis**: `stacking_mode: "duration"`, `max_stacks: 1`, `effect_class: "defensive"`, `special_mechanics: "Blocks next incoming attack; consumed on block"`. Source: https://wiki.guildwars2.com/wiki/Aegis
  - [ ] **Stability**: `stacking_mode: "intensity"`, `max_stacks: 25`, `max_duration: 30`, `effect_class: "defensive"`. Source: https://wiki.guildwars2.com/wiki/Stability
  - [ ] **Resistance**: `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 30`, `effect_class: "defensive"`, `special_mechanics: "Nondamaging conditions ineffective while active; does not remove them"`. Source: https://wiki.guildwars2.com/wiki/Resistance
  - [ ] **Regeneration**: `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 30`, `effect_class: "sustain"`. Source: https://wiki.guildwars2.com/wiki/Regeneration
  - [ ] **Vigor**: `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 30`, `effect_class: "sustain"`. Source: https://wiki.guildwars2.com/wiki/Vigor
  - [ ] **Swiftness**: `stacking_mode: "duration"`, `max_stacks: 1`, `max_duration: 60`, `effect_class: "utility"`. Source: https://wiki.guildwars2.com/wiki/Swiftness
  - [ ] Document `all_modes` vs per-mode precedence rule in file header/comment (L9)
  - [ ] Include `evidence_level: "Factual"` and `sources` array on each entry
  - [ ] Include boon-to-condition corruption counterpart reference section (e.g., Might -> Weakness, Fury -> Blind, Protection -> Vulnerability, Regeneration -> Poison, Stability -> Fear, Vigor -> Crippled, Swiftness -> Crippled, Resistance -> Chilled, Aegis -> Blind, Resolution -> Confusion, Quickness -> Slow, Alacrity -> Slow). Source: https://wiki.guildwars2.com/wiki/Boon#Boon_corruption

### Task 2: Create `data/formulas/conditions.json` (AC: 2, 3, 5, 6)

- [ ] Author `data/formulas/conditions.json` with entries for all GW2 conditions
  - [ ] **Bleeding**: PvE/PvP/WvW each: `base_per_tick: 22.0`, `condition_damage_coeff: 0.06`, `delivery: "tick"`. `stacking_mode: "intensity"`, `max_stacks: 1500`, `effect_class: "damage"`. Source: https://wiki.guildwars2.com/wiki/Bleeding
  - [ ] **Burning**: PvE/PvP/WvW each: `condition_damage_coeff: 0.155`, `delivery: "tick"`. Base: wiki-verify L1 -- resolve 131 vs 131.75 discrepancy. `stacking_mode: "intensity"`, `max_stacks: 1500`, `effect_class: "damage"`. Source: https://wiki.guildwars2.com/wiki/Burning
  - [ ] **Poison (Poisoned)**: PvE/PvP/WvW each: `base_per_tick: 33.5`, `condition_damage_coeff: 0.06`, `delivery: "tick"`. `stacking_mode: "intensity"`, `max_stacks: 1500`, `effect_class: "damage"`, `secondary_effects: "Reduces healing effectiveness by 33%"`. Source: https://wiki.guildwars2.com/wiki/Poisoned
  - [ ] **Torment**: Multi-state condition with `stationary` and `moving` sub-entries per mode:
    - PvE stationary: wiki-verify L2 (source-of-truth says `0.09*CD+31.8`, code says `0.0375*CD+31.875`)
    - PvE moving: wiki-verify L2 (source-of-truth says `0.06*CD+22`, code has no moving formula)
    - PvP/WvW stationary: wiki-verify L2 (source-of-truth says `0.07*CD+26`)
    - PvP/WvW moving: wiki-verify L2 (source-of-truth says `0.054*CD+19.8`)
    - `stacking_mode: "intensity"`, `max_stacks: 1500`, `effect_class: "damage"`. Source: https://wiki.guildwars2.com/wiki/Torment
  - [ ] **Confusion**: Multi-state condition with `over_time` and `on_skill_use` sub-entries per mode:
    - PvE over-time: wiki-verify L3 (source-of-truth says `0.05*CD+18.25`, code has no over-time formula)
    - PvE on-skill-use: wiki-verify L3 (source-of-truth says `0.0325*CD+16.24`, code says `0.0175*CD+11`)
    - PvP/WvW over-time: wiki-verify L3 (source-of-truth says `10` flat)
    - PvP/WvW on-skill-use: wiki-verify L3 (source-of-truth says `0.0975*CD+49.5`)
    - `stacking_mode: "intensity"`, `max_stacks: 1500`, `effect_class: "damage"`. Source: https://wiki.guildwars2.com/wiki/Confusion
  - [ ] **Vulnerability**: PvE/PvP/WvW each: `incoming_damage_pct_per_stack: 0.01`, `max_stacks: 25`. `stacking_mode: "intensity"`, `effect_class: "debuff"`. Source: https://wiki.guildwars2.com/wiki/Vulnerability
  - [ ] **Weakness**: `stacking_mode: "duration"`, `effect_class: "suppression"`, `suppression_effects: { "strike_output": "-50% endurance regen, glancing blows", "dodge_economy": "-50% endurance regeneration", "channels": ["strike_output", "dodge_economy"] }`. Source: https://wiki.guildwars2.com/wiki/Weakness
  - [ ] **Blind (Blinded)**: `stacking_mode: "duration"`, `effect_class: "suppression"`, `suppression_effects: { "channels": ["strike_output"] }`, `special_mechanics: "Next outgoing attack misses; consumed per hit"`. Source: https://wiki.guildwars2.com/wiki/Blinded
  - [ ] **Slow**: `stacking_mode: "duration"`, `effect_class: "suppression"`, `suppression_effects: { "channels": ["action_throughput"] }`. Source: https://wiki.guildwars2.com/wiki/Slow
  - [ ] **Chilled**: `stacking_mode: "duration"`, `effect_class: "suppression"`, `suppression_effects: { "channels": ["skill_frequency", "positioning"] }`. Source: https://wiki.guildwars2.com/wiki/Chilled
  - [ ] **Immobile**: `stacking_mode: "duration"`, `effect_class: "control"`, `suppression_effects: { "channels": ["positioning"] }`. Source: https://wiki.guildwars2.com/wiki/Immobile
  - [ ] **Crippled**: `stacking_mode: "duration"`, `effect_class: "control"`, `suppression_effects: { "channels": ["positioning"] }`. Source: https://wiki.guildwars2.com/wiki/Crippled
  - [ ] **Fear**: `stacking_mode: "duration"`, `effect_class: "control"`, `suppression_effects: { "channels": ["action_throughput", "positioning"] }`. Source: https://wiki.guildwars2.com/wiki/Fear
  - [ ] **Taunt**: `stacking_mode: "duration"`, `effect_class: "control"`, `suppression_effects: { "channels": ["action_throughput"] }`. Source: https://wiki.guildwars2.com/wiki/Taunt
  - [ ] **Daze**: `stacking_mode: "duration"`, `effect_class: "control"`, `suppression_effects: { "channels": ["action_throughput"] }`. Source: https://wiki.guildwars2.com/wiki/Daze
  - [ ] Include `evidence_level: "Factual"` and `sources` array on each entry

### Task 3: Create loader module `crates/optimizer/src/data/boon_condition_formulas.rs` (AC: 1, 2, 4, 6, 8)

- [ ] Create `crates/optimizer/src/data/boon_condition_formulas.rs`
- [ ] Define Rust types matching JSON schemas:
  - [ ] `BoonDefinition` struct: name, stacking_mode, max_stacks, max_duration, effect_class, special_mechanics (Option), effects (mode-keyed map), evidence_level, sources, counterpart_condition (Option)
  - [ ] `ConditionDefinition` struct: name, stacking_mode, max_stacks, effect_class, secondary_effects (Option), suppression_effects (Option), formulas (mode-keyed map), evidence_level, sources
  - [ ] `ConditionFormula` struct: base_per_tick, condition_damage_coeff, delivery
  - [ ] `MultiStateConditionFormula` struct: variant with sub-entries for stationary/moving or over_time/on_skill_use
  - [ ] `BoonEffect` enum/struct: typed variants for `flat_additive`, `ratio`, `ratio_per_stack`
  - [ ] `StackingMode` enum: `Intensity`, `Duration`
  - [ ] `EffectClass` enum: `OffensiveThroughput`, `OffensiveStat`, `Defensive`, `Sustain`, `Utility`, `Damage`, `Debuff`, `Suppression`, `Control`
  - [ ] `BoonFormulas` wrapper struct with `HashMap<String, BoonDefinition>` for O(1) lookup
  - [ ] `ConditionFormulas` wrapper struct with `HashMap<String, ConditionDefinition>` for O(1) lookup
- [ ] Implement `load_boon_formulas(json: &str) -> Result<BoonFormulas, FormulaLoadError>` with validation:
  - [ ] Reject duplicate boon names
  - [ ] Reject invalid enum values (strict deserialization)
  - [ ] Validate per-mode precedence: if both `all_modes` and per-mode exist, per-mode wins
- [ ] Implement `load_condition_formulas(json: &str) -> Result<ConditionFormulas, FormulaLoadError>` with validation:
  - [ ] Reject duplicate condition names
  - [ ] Validate every condition declares all 3 modes
  - [ ] Validate multi-state conditions declare state dimensions
- [ ] Use `include_str!` + `OnceLock` pattern (matching P3-01 `profession_profiles.rs` pattern):
  - [ ] `const BOON_FORMULAS_JSON: &str = include_str!("../../../../data/formulas/boons.json");`
  - [ ] `const CONDITION_FORMULAS_JSON: &str = include_str!("../../../../data/formulas/conditions.json");`
  - [ ] `static BOONS: OnceLock<BoonFormulas>` with `boons()` global accessor
  - [ ] `static CONDITIONS: OnceLock<ConditionFormulas>` with `conditions()` global accessor
- [ ] Implement lookup helpers on `BoonFormulas`:
  - [ ] `fn fury_crit_bonus(&self, mode: GameMode) -> f64` -- returns 0.25 PvE, 0.20 PvP/WvW
  - [ ] `fn might_power_per_stack(&self) -> f64` -- returns 30.0
  - [ ] `fn might_condi_per_stack(&self) -> f64` -- returns 30.0
  - [ ] `fn protection_multiplier(&self) -> f64` -- returns 0.67 (caller computes DR as `1 - 0.67 = 0.33`)
  - [ ] `fn resolution_multiplier(&self) -> f64` -- returns 0.67
  - [ ] `fn vulnerability_pct_per_stack(&self) -> f64` -- returns 0.01
  - [ ] `fn vulnerability_max_stacks(&self) -> u32` -- returns 25
  - [ ] `fn get(&self, boon: &str) -> Option<&BoonDefinition>` -- generic accessor
- [ ] Implement lookup helpers on `ConditionFormulas`:
  - [ ] `fn tick_damage(&self, condition: &str, condition_damage: f64, mode: GameMode) -> f64` -- for simple conditions (Bleeding, Burning, Poison)
  - [ ] `fn torment_tick(&self, condition_damage: f64, mode: GameMode, moving: bool) -> f64` -- mode + movement state dispatch
  - [ ] `fn confusion_tick(&self, condition_damage: f64, mode: GameMode, on_skill_use: bool) -> f64` -- mode + trigger state dispatch
  - [ ] `fn get(&self, condition: &str) -> Option<&ConditionDefinition>` -- generic accessor
- [ ] Register module in `crates/optimizer/src/data/mod.rs`:
  - [ ] `pub mod boon_condition_formulas;`
  - [ ] `pub use boon_condition_formulas::{BoonFormulas, ConditionFormulas, boons, conditions};`

### Task 4: Replace hardcoded condition tick functions in `combat.rs` (AC: 2, 3, 7, 8)

Current hardcoded functions to replace (lines 148-174):

| Function | Line | Current Formula | Correct Action |
|----------|------|-----------------|----------------|
| `bleeding_tick()` | 148-150 | `0.06 * CD + 22.0` | Delegate to loaded data |
| `burning_tick()` | 153-155 | `0.155 * CD + 131.75` | Delegate; verify L1 base value |
| `poison_tick()` | 158-160 | `0.06 * CD + 33.5` | Delegate to loaded data |
| `torment_tick()` | 162-167 | `0.0375 * CD + 31.875` (stationary only) | Delegate; add moving variant; mode-split per L2 |
| `confusion_tick()` | 169-174 | `0.0175 * CD + 11.0` (on-use only) | Delegate; add over-time variant; mode-split per L3 |

- [ ] Replace `bleeding_tick()`, `burning_tick()`, `poison_tick()` to delegate to `conditions().tick_damage()`
- [ ] Replace `torment_tick()` with `conditions().torment_tick(cd, mode, moving)` -- requires `BalanceContext` and movement state parameter
- [ ] Replace `confusion_tick()` with `conditions().confusion_tick(cd, mode, on_skill_use)` -- requires `BalanceContext` and trigger state parameter
- [ ] Update `calculate_condition_ticks()` (line 177) to use `BalanceContext` for mode dispatch (currently `_ctx` is unused):
  - [ ] Change `_ctx` to `ctx` and pass `ctx.game_mode` to formula lookups
  - [ ] Torment: use stationary as default (conservative baseline, matching current behavior). Document that movement_fraction weighting is P3-14 scope.
  - [ ] Confusion: use on-skill-use as default (matching current behavior). Document that trigger frequency is P3-14 scope.
- [ ] Update `ConditionTicks` struct if needed to support multi-state reporting (optional: may keep single value with documented default state, since multi-state weighting is P3-14)

### Task 5: Replace hardcoded boon constants in `combat.rs` (AC: 1, 8)

Current hardcoded values to replace in `calculate_combat_performance()` (lines 260-361):

| Value | Line | Current Code | Replacement |
|-------|------|-------------|-------------|
| Might +30 Power | 270 | `buffs.might_stacks as f64 * 30.0` | `buffs.might_stacks as f64 * boons().might_power_per_stack()` |
| Might +30 Condi | 271 | `buffs.might_stacks as f64 * 30.0` | `buffs.might_stacks as f64 * boons().might_condi_per_stack()` |
| Fury 25/20 | 274-277 | `match ctx.game_mode { PvE => 25.0, ... }` | `boons().fury_crit_bonus(ctx.game_mode) * 100.0` (data stores as ratio) |
| Vulnerability +1% | 294 | `buffs.vulnerability_stacks as f64 * 0.01` | `buffs.vulnerability_stacks as f64 * boons().vulnerability_pct_per_stack()` |
| Protection 0.33 | 341 | `if buffs.protection { 0.33 }` | `if buffs.protection { 1.0 - boons().protection_multiplier() }` |
| Resolution 0.33 | 342 | `if buffs.resolution { 0.33 }` | `if buffs.resolution { 1.0 - boons().resolution_multiplier() }` |

- [ ] Replace each hardcoded value with the corresponding `boons()` lookup call
- [ ] Verify that `BuffProfile` doc comments (lines 103-112) are updated to say "loaded from data" rather than citing specific numbers

### Task 6: Replace hardcoded formulas in `rotation/simulator.rs` (AC: 8)

Current duplicated formulas at line 535-543:

```rust
fn condition_tick_damage(condition: &str, condition_damage: f64) -> f64 {
    match condition {
        "Bleeding" => 0.06 * condition_damage + 22.0,
        "Burning" => 0.155 * condition_damage + 131.75,
        "Poison" => 0.06 * condition_damage + 33.5,
        "Torment" => 0.0375 * condition_damage + 31.875,
        "Confusion" => 0.0175 * condition_damage + 11.0,
        _ => 0.0,
    }
}
```

- [ ] Replace `condition_tick_damage()` in `rotation/simulator.rs` to delegate to `conditions()` lookups
- [ ] The simulator currently does not carry a `BalanceContext` -- either thread it through or use PvE default with a `// TODO: P3-XX thread BalanceContext` comment
- [ ] Also replace hardcoded Might (+30 Power/CD, line 510) and Fury (+20% crit, line 520) references in the simulator if present in production code paths (check if these are only in doc comments)

### Task 7: Write tests (AC: 7, 9)

- [ ] **Loader tests** (in `boon_condition_formulas.rs`):
  - [ ] `test_embedded_boon_formulas_load_successfully` -- validates embedded JSON
  - [ ] `test_embedded_condition_formulas_load_successfully` -- validates embedded JSON
  - [ ] `test_duplicate_boon_rejected` -- loader rejects duplicate boon name
  - [ ] `test_duplicate_condition_rejected` -- loader rejects duplicate condition name
  - [ ] `test_malformed_stacking_mode_rejected` -- invalid enum rejected
  - [ ] `test_condition_missing_mode_rejected` -- condition without all 3 modes fails validation
  - [ ] `test_all_modes_vs_per_mode_precedence` -- per-mode entry overrides all_modes (AC 4)

- [ ] **Boon value tests** (cite wiki sources in comments):
  - [ ] `test_fury_pve_is_25_pct` -- `boons().fury_crit_bonus(GameMode::PvE) == 0.25`. Source: https://wiki.guildwars2.com/wiki/Fury
  - [ ] `test_fury_pvp_is_20_pct` -- `boons().fury_crit_bonus(GameMode::PvP) == 0.20`. Source: https://wiki.guildwars2.com/wiki/Fury
  - [ ] `test_fury_wvw_is_20_pct` -- `boons().fury_crit_bonus(GameMode::WvW) == 0.20`. Source: https://wiki.guildwars2.com/wiki/Fury
  - [ ] `test_might_per_stack_values` -- power=30, condi=30. Source: https://wiki.guildwars2.com/wiki/Might
  - [ ] `test_protection_multiplier` -- 0.67 (33% DR). Source: https://wiki.guildwars2.com/wiki/Protection
  - [ ] `test_resolution_multiplier` -- 0.67. Source: https://wiki.guildwars2.com/wiki/Resolution
  - [ ] `test_vulnerability_per_stack` -- 0.01, max 25. Source: https://wiki.guildwars2.com/wiki/Vulnerability

- [ ] **Condition formula tests** (cite wiki sources in comments):
  - [ ] `test_bleeding_formula_all_modes` -- `0.06*1000+22 = 82.0` same PvE/PvP/WvW. Source: https://wiki.guildwars2.com/wiki/Bleeding
  - [ ] `test_burning_formula` -- `0.155*1000+base = expected`. Verify base matches wiki (L1). Source: https://wiki.guildwars2.com/wiki/Burning
  - [ ] `test_poison_formula_all_modes` -- `0.06*1000+33.5 = 93.5`. Source: https://wiki.guildwars2.com/wiki/Poisoned
  - [ ] `test_torment_pve_stationary_vs_moving` -- different results for same CD. Source: https://wiki.guildwars2.com/wiki/Torment
  - [ ] `test_torment_pve_vs_pvp` -- different results for same CD and state. Source: https://wiki.guildwars2.com/wiki/Torment
  - [ ] `test_confusion_pve_overtime_vs_on_skill_use` -- different results. Source: https://wiki.guildwars2.com/wiki/Confusion
  - [ ] `test_confusion_pve_vs_pvp` -- different results for same state. Source: https://wiki.guildwars2.com/wiki/Confusion
  - [ ] `test_confusion_pvp_overtime_is_flat_10` -- PvP/WvW over-time is flat 10 damage. Source: https://wiki.guildwars2.com/wiki/Confusion

- [ ] **Integration tests** (in `combat.rs` tests):
  - [ ] `test_combat_performance_uses_loaded_boon_values` -- verify calculate_combat_performance results match loaded data (not hardcoded)
  - [ ] `test_torment_mode_dispatch_in_combat` -- PvE vs PvP Torment differs in condition_ticks
  - [ ] `test_confusion_mode_dispatch_in_combat` -- PvE vs PvP Confusion differs

- [ ] **StatusDefinition metadata tests**:
  - [ ] `test_boon_stacking_modes` -- Might is intensity, Fury is duration, etc.
  - [ ] `test_condition_stacking_modes` -- Bleeding is intensity, Weakness is duration, etc.
  - [ ] `test_boon_max_stacks` -- Might max 25, Stability max 25
  - [ ] `test_condition_effect_classes` -- Bleeding is damage, Weakness is suppression, Fear is control

### Task 8: Update project-context.md (AC: 10)

- [ ] Update condition formula table in `_bmad-output/project-context.md`:
  - [ ] Torment: document PvE stationary/moving + PvP/WvW stationary/moving (4 formulas)
  - [ ] Confusion: document PvE over-time/on-skill-use + PvP/WvW over-time/on-skill-use (4 formulas)
  - [ ] Burning: update base constant if corrected by L1
  - [ ] Add note: values loaded from `data/formulas/conditions.json`
- [ ] Update Fury documentation to reflect mode-split: PvE 25%, PvP/WvW 20%
- [ ] Add note: boon values loaded from `data/formulas/boons.json`

## Dev Notes

### Hardcoded Value Inventory (ALL must be replaced)

#### Boon constants in `crates/optimizer/src/combat.rs`:

| Value | Line(s) | Context | Notes |
|-------|---------|---------|-------|
| `30.0` (Might Power) | 270 | `buffs.might_stacks as f64 * 30.0` | Replace with `boons().might_power_per_stack()` |
| `30.0` (Might Condi) | 271 | `buffs.might_stacks as f64 * 30.0` | Replace with `boons().might_condi_per_stack()` |
| `25.0` (Fury PvE) | 275 | `GameMode::PvE => 25.0` | Replace with `boons().fury_crit_bonus(mode) * 100.0` |
| `20.0` (Fury PvP/WvW) | 276 | `GameMode::PvP \| GameMode::WvW => 20.0` | Same loader call |
| `0.01` (Vulnerability) | 294 | `buffs.vulnerability_stacks as f64 * 0.01` | Replace with `boons().vulnerability_pct_per_stack()` |
| `0.33` (Protection DR) | 341 | `if buffs.protection { 0.33 }` | Replace with `1.0 - boons().protection_multiplier()` |
| `0.33` (Resolution DR) | 342 | `if buffs.resolution { 0.33 }` | Replace with `1.0 - boons().resolution_multiplier()` |

#### Condition formulas in `crates/optimizer/src/combat.rs`:

| Function | Line(s) | Formula | Replace with |
|----------|---------|---------|-------------|
| `bleeding_tick()` | 148-150 | `0.06*CD+22.0` | `conditions().tick_damage("Bleeding", cd, mode)` |
| `burning_tick()` | 153-155 | `0.155*CD+131.75` | `conditions().tick_damage("Burning", cd, mode)` |
| `poison_tick()` | 158-160 | `0.06*CD+33.5` | `conditions().tick_damage("Poison", cd, mode)` |
| `torment_tick()` | 162-167 | `0.0375*CD+31.875` | `conditions().torment_tick(cd, mode, moving)` |
| `confusion_tick()` | 169-174 | `0.0175*CD+11.0` | `conditions().confusion_tick(cd, mode, on_skill_use)` |

#### Duplicated condition formulas in `crates/optimizer/src/rotation/simulator.rs`:

| Function | Line(s) | Notes |
|----------|---------|-------|
| `condition_tick_damage()` | 535-543 | Exact duplicate of combat.rs formulas. Replace with shared `conditions()` lookups. |

#### Doc-only references (update comments, not production logic):

| File | Line(s) | Content |
|------|---------|---------|
| `combat.rs` | 103 | `"Each stack = +30 Power, +30 Condition Damage"` -- update to "loaded from data" |
| `combat.rs` | 105 | `"+20% critical chance"` -- update to "mode-dependent, loaded from data" |
| `combat.rs` | 107-109 | Protection/Resolution doc comments |
| `combat.rs` | 111 | Vulnerability doc comment |
| `rotation/simulator.rs` | 510 | Might comment |
| `rotation/simulator.rs` | 520 | Fury comment |

### Formula Verification Requirements (L1, L2, L3)

**CRITICAL**: Before writing data files, verify all formulas against wiki. Known discrepancies:

**L1 -- Burning base constant**:
- Source-of-truth doc: `131`
- Current code: `131.75`
- Wiki: https://wiki.guildwars2.com/wiki/Burning -- check level 80 formula
- Resolution: use whichever the wiki says at implementation time. Document the verification.

**L2 -- Torment formulas**:
- Source-of-truth doc: PvE stationary `0.09*CD+31.8`, PvE moving `0.06*CD+22`
- Current code: stationary only, `0.0375*CD+31.875` (dramatically different)
- Wiki: https://wiki.guildwars2.com/wiki/Torment -- check all 4 formulas (PvE stat/mov, PvP stat/mov)
- Resolution: wiki is authoritative. Neither source-of-truth doc nor current code should be assumed correct.

**L3 -- Confusion formulas**:
- Source-of-truth doc: PvE over-time `0.05*CD+18.25`, PvE on-skill-use `0.0325*CD+16.24`, PvP over-time `10` flat, PvP on-skill-use `0.0975*CD+49.5`
- Current code: on-use only, `0.0175*CD+11.0` (dramatically different from source-of-truth)
- Wiki: https://wiki.guildwars2.com/wiki/Confusion -- check all 4 formulas
- Resolution: wiki is authoritative. Both are suspect.

### Schema Design Decisions

- **`all_modes` vs per-mode**: Data files support both. Per-mode entry takes absolute precedence when both exist. Loader resolves at load time, not at lookup time. This means the in-memory representation stores resolved per-mode values -- no runtime precedence logic needed after loading.
- **Protection/Resolution stored as multiplier (0.67), not DR (0.33)**: Matches Schema 5 in `docs/optimizer-data-schemas.md`. Caller computes DR as `1.0 - multiplier`. This is more numerically stable and avoids double-negation confusion.
- **Condition formula storage**: Simple conditions store `{ base_per_tick, condition_damage_coeff, delivery }`. Multi-state conditions nest by state dimension. Both are per-mode.
- **StatusDefinition is informational for P3-04**: The metadata (stacking_mode, max_stacks, etc.) is shipped as data-file fields. Typed Rust structs exist for deserialization, but runtime cap enforcement and typed operation payloads are P3-10a/P3-14 scope.
- **Boon counterpart mapping**: Stored as a reference field on each boon definition, not as a separate lookup table. The actual corruption/conversion logic is P3-10a/P3-10b.

### Module Structure

Following P3-01 pattern exactly:
```
crates/optimizer/src/data/
  mod.rs                           -- add: pub mod boon_condition_formulas; pub use ...
  profession_profiles.rs           -- existing (P3-01)
  boon_condition_formulas.rs       -- NEW: types, loaders, OnceLock, lookups, tests
data/formulas/
  boons.json                       -- NEW: boon effects + StatusDefinition metadata
  conditions.json                  -- NEW: condition formulas + StatusDefinition metadata
```

### What NOT to Change

- Do not modify `ConditionWeights` presets or `condition_weights_for_profession()` -- those are heuristic application rates, not tick formulas. They remain as-is until P3-14.
- Do not modify `DamageModifiers` struct -- it captures trait/rune/sigil modifiers, not base boon/condition values.
- Do not modify scoring constants (`STRIKE_DPS_NORM`, `WEIGHT_BUDGET`, etc.) -- those are heuristic scoring parameters.
- Do not add duration formulas -- that is P3-05 scope.
- Do not create generic loader traits/infrastructure -- that is P3-07 scope.
- Do not change the `BuffProfile` struct fields -- only update how their values are interpreted in `calculate_combat_performance()`.
- Do not change files outside the listed scope unless absolutely necessary for compilation.

### Guardrails

- **GR-1 (no heuristic contamination)**: All formula functions take explicit parameters (condition_damage, mode, movement_state). No default stack counts, uptimes, or assumptions embedded.
- **GR-2 (source verification)**: Mode-split values (Fury, Torment, Confusion) cite wiki sources. Burning base constant verified (L1). Two verification paths where feasible.
- **GR-3 (accepted sources)**: Valid wiki sources: Boon, Condition, Effect_stacking, Boon_Duration, Damage, Attribute, and individual boon/condition pages. NOT valid: Diminishing_returns page.
- **GR-4 (boons/conditions first-class)**: StatusDefinition metadata makes boons and conditions typed status state, not generic modifiers.
- **GR-5 (status state schema)**: Factual metadata delivered here (stacking, caps, effect semantics). Heuristic layers (P3-14) define how often; this story defines what.

### References

- [Source: docs/optimizer-source-of-truth.md#Section 4] -- canonical boon values
- [Source: docs/optimizer-source-of-truth.md#Section 5] -- canonical condition formulas
- [Source: docs/optimizer-data-schemas.md#Schema 4] -- condition formula JSON schema
- [Source: docs/optimizer-data-schemas.md#Schema 5] -- boon formula JSON schema
- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.4] -- epic-level AC, FR5, FR6, D2/D3/D4
- [Source: _bmad-output/planning-artifacts/epics.md#GR-1 through GR-5] -- implementation guardrails
- [Source: crates/optimizer/src/combat.rs:148-174] -- current hardcoded condition tick functions
- [Source: crates/optimizer/src/combat.rs:270-342] -- current hardcoded boon constants
- [Source: crates/optimizer/src/rotation/simulator.rs:535-543] -- duplicated condition formulas
- [Source: crates/optimizer/src/data/profession_profiles.rs] -- P3-01 loader pattern to follow
- [Source: crates/optimizer/src/balance.rs] -- BalanceContext type (P3-02)
- [Source: https://wiki.guildwars2.com/wiki/Fury] -- Fury mode split
- [Source: https://wiki.guildwars2.com/wiki/Might] -- Might per-stack values
- [Source: https://wiki.guildwars2.com/wiki/Protection] -- Protection DR
- [Source: https://wiki.guildwars2.com/wiki/Resolution] -- Resolution DR
- [Source: https://wiki.guildwars2.com/wiki/Vulnerability] -- Vulnerability per-stack
- [Source: https://wiki.guildwars2.com/wiki/Bleeding] -- Bleeding formula
- [Source: https://wiki.guildwars2.com/wiki/Burning] -- Burning formula (L1 verification)
- [Source: https://wiki.guildwars2.com/wiki/Poisoned] -- Poison formula
- [Source: https://wiki.guildwars2.com/wiki/Torment] -- Torment formula (L2 verification)
- [Source: https://wiki.guildwars2.com/wiki/Confusion] -- Confusion formula (L3 verification)
- [Source: https://wiki.guildwars2.com/wiki/Effect_stacking] -- stacking rules
- [Source: https://wiki.guildwars2.com/wiki/Boon] -- boon list, stacking, corruption mapping
- [Source: https://wiki.guildwars2.com/wiki/Condition] -- condition list, stacking
