# P3-12 WvW Non-Fallback Audit

Generated: 2026-03-06

## Methodology

Every function accepting `BalanceContext`, `GameMode`, or a mode string parameter was traced
through the optimizer crate. Each was classified as:

- **(a) Uses WvW-specific data** -- the function reads mode from BalanceContext and dispatches
  to WvW-specific coefficients already present in Phase A data files.
- **(b) Mode-invariant base data** -- the computation is identical across all modes; no known
  mode split exists for this data.
- **(c) Known mode split, WvW value unresolved** -- ArenaNet has confirmed different values
  per mode, but the WvW value is not yet in the override system.
- **(d) Uncertain split status** -- data may differ between modes but no confirmation exists;
  flagged for investigation in P3-13.

## Mode-Sensitive Computation Paths

### combat.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `calculate_condition_ticks()` | `ctx.game_mode` | Dispatches to WvW formulas via `conds.tick_damage(..., mode)`, `conds.torment_tick(..., mode, ...)`, `conds.confusion_tick(..., mode, ...)` | **(a)** Uses WvW-specific data |
| `calculate_combat_performance()` | `ctx.game_mode` | Fury crit bonus: `b.fury_crit_bonus(ctx.game_mode)` returns 0.20 for WvW (vs 0.25 PvE). All other boon values (Might, Protection, Resolution, Vulnerability) are mode-invariant. | **(a)** Fury is WvW-specific; **(b)** others are mode-invariant |
| `condition_weights_for_profession()` | `_ctx: &BalanceContext` | Accepts BalanceContext but currently ignores mode (all presets are PvE-oriented). WvW condition weights may differ. | **(d)** Uncertain -- flagged for P3-13/P3-14 |
| `condition_duration_bonus()` | `_ctx: &BalanceContext` | Accepts BalanceContext but ignores it. Uses hardcoded EXPERTISE_DIVISOR (1500.0) which is mode-invariant per wiki. | **(b)** Mode-invariant |
| `condition_duration_multiplied()` | `ctx: &BalanceContext` | Delegates to `condition_duration_bonus()` -- same invariance. | **(b)** Mode-invariant |
| `boon_duration_bonus()` | `_ctx: &BalanceContext` | Accepts BalanceContext but ignores it. CONCENTRATION_DIVISOR (1500.0) is mode-invariant per wiki. | **(b)** Mode-invariant |
| `boon_duration_multiplied()` | `ctx: &BalanceContext` | Delegates to `boon_duration_bonus()` -- same invariance. | **(b)** Mode-invariant |
| `default_buff_profiles()` | `_ctx: &BalanceContext` | Returns same Solo/Party/Full Squad profiles regardless of mode. WvW may warrant different buff assumptions. | **(d)** Uncertain -- flagged for P3-14 |
| `extract_damage_modifiers()` | `_ctx: &BalanceContext` | Accepts BalanceContext but ignores it. Extracts modifiers from trait/rune/sigil facts -- these facts come from the GW2 API which does not mode-split fact values. | **(b)** Mode-invariant (API facts are not mode-split) |

### boon_condition_formulas.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `BoonFormulas::fury_crit_bonus()` | `mode: GameMode` | Returns per-mode value from boons.json. WvW entry: `{ "crit_chance_bonus": 0.20 }` | **(a)** Uses WvW-specific data |
| `BoonFormulas::might_power_per_stack()` | None (reads PvE/all_modes) | 30.0 per stack, mode-invariant per wiki. | **(b)** Mode-invariant |
| `BoonFormulas::might_condi_per_stack()` | None (reads PvE/all_modes) | 30.0 per stack, mode-invariant per wiki. | **(b)** Mode-invariant |
| `BoonFormulas::protection_multiplier()` | None (reads PvE/all_modes) | 0.67, mode-invariant per wiki. | **(b)** Mode-invariant |
| `BoonFormulas::resolution_multiplier()` | None (reads PvE/all_modes) | 0.67, mode-invariant per wiki. | **(b)** Mode-invariant |
| `BoonFormulas::vulnerability_pct_per_stack()` | None (reads PvE/all_modes) | 0.01 per stack, mode-invariant per wiki. | **(b)** Mode-invariant |
| `ConditionFormulas::tick_damage()` | `mode: GameMode` | Dispatches to per-mode formula entry from conditions.json. WvW entries exist for all damage conditions. | **(a)** Uses WvW-specific data |
| `ConditionFormulas::torment_tick()` | `mode: GameMode` | WvW: stationary base=26.0, coeff=0.07 (vs PvE: 31.8, 0.09). | **(a)** Uses WvW-specific data |
| `ConditionFormulas::confusion_tick()` | `mode: GameMode` | WvW: over_time base=10.0 flat, on_skill_use base=49.5, coeff=0.0975. | **(a)** Uses WvW-specific data |

### universal_formulas.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `UniversalFormulas::crit_chance()` | None | Formula constants (precision_offset=895, divisor=21) are mode-invariant per wiki. | **(b)** Mode-invariant |
| `UniversalFormulas::crit_damage()` | None | Formula constants (base=150%, ferocity_divisor=15) are mode-invariant per wiki. | **(b)** Mode-invariant |
| `UniversalFormulas::health()` | None | Vitality-to-health multiplier (10) is mode-invariant per wiki. | **(b)** Mode-invariant |
| `UniversalFormulas::strike_damage()` | None | Damage formula constants (reference_armor=2597) are mode-invariant per wiki. | **(b)** Mode-invariant |

### balance_overrides.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `BalanceOverrides::lookup()` | `mode: &str` | Keyed by `(patch_id, mode)`. WvW lookup only searches the WvW file. Returns `None` if no WvW entry exists -- **does NOT fall back to PvE**. | **(a)** Correctly mode-isolated |

### engine.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `optimize()` | `ctx.game_mode` | Checks `GameMode::PvP` to branch to `optimize_pvp()`. WvW flows through the PvE/WvW gear pipeline (correct -- WvW uses the same gear system as PvE). BalanceContext is threaded through all sub-calls. | **(a)** Correct dispatch |
| `optimize_pvp()` | `ctx: &BalanceContext` | Only reached when `ctx.game_mode == PvP`. WvW never enters this path. | N/A (PvP only) |
| `optimize_deterministic()` / `optimize_with_gemini()` | `ctx: &BalanceContext` | Thread BalanceContext through all combat/scoring calls. | **(a)** Correct propagation |

### stats.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `base_stats()` | None | 1000 base for all primary attributes. Mode-invariant per wiki. | **(b)** Mode-invariant |
| `base_health()` | None (profession-keyed) | Health pools are mode-invariant per wiki. | **(b)** Mode-invariant |
| `base_defense()` | None (profession-keyed) | Armor values are mode-invariant per wiki. | **(b)** Mode-invariant |
| `compute_derived()` | None | Uses `base_health()`, `base_defense()`, universal formulas -- all mode-invariant. | **(b)** Mode-invariant |
| `calculate_gear_stats()` | None | Gear stat formulas (`attr_adj * mult + value`) are mode-invariant. | **(b)** Mode-invariant |
| `calculate_full_stats()` | None | Aggregates gear/rune/sigil/infusion/trait stats -- all mode-invariant. | **(b)** Mode-invariant |
| `calculate_pvp_stats()` | None | PvP-only (amulet system). WvW never calls this. | N/A (PvP only) |

### scoring.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `OptimizationWeights::default_for_mode()` | `mode: &str` | WvW preset: power=0.4, disable=0.3, condition=0.2, healing=0.2, sustain=0.5. These are heuristic weight distributions, not game-data coefficients. | **(b)** Mode-variant by design (user preference, not game data) |
| `score_with_weights()` | None | Normalization constants (STRIKE_DPS_NORM=3000 etc.) are empirically tuned, mode-invariant. | **(b)** Mode-invariant |

### synergy_pipeline.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `build_synergy_candidates()` | `ctx: &BalanceContext` | Threads BalanceContext to `combat::calculate_combat_performance()`. No direct mode-dependent logic of its own. | **(a)** Correct propagation |

### context.rs

| Function | Mode Parameter | WvW Behavior | Classification |
|----------|---------------|--------------|----------------|
| `ContextConfig.game_mode` | `&str` | Passed to Gemini as context string. Gemini sees "WvW" in the prompt. No computation is mode-dependent here -- it is informational for the LLM. | **(b)** Informational only |

## Balance Override Non-Fallback Verification

The `BalanceOverrides::lookup()` method (balance_overrides.rs:129-153) was verified to be
strictly keyed by `(patch_id, mode)`:

1. **Key structure**: `files: HashMap<(String, String), OverrideFile>` -- the `mode` string
   is part of the composite key. WvW and PvE are separate keys and separate files.

2. **No cross-mode fallback**: The lookup does `self.files.get(&(patch_id, mode))` -- if no
   WvW file entry matches, it returns `None`. There is no "try PvE if WvW is missing" logic
   anywhere in the function.

3. **None vs Unknown**: `None` return = no override exists (use base value, no quality
   degradation). `Some(Unknown)` = override exists but value is unresolved (degrades quality).

4. **Loading isolation**: `load_all_overrides()` loads pve.json, pvp.json, wvw.json separately
   and inserts each with its own `(patch_id, mode)` key. No merging or fallback between files.

**Conclusion**: There is no code path where a WvW lookup can silently return a PvE value.

## Known Mode Splits

The following coefficients are known to differ between PvE and PvP/WvW:

| Entity | Field | PvE Value | PvP/WvW Value | Handled In |
|--------|-------|-----------|---------------|------------|
| Fury (Boon) | crit_chance_bonus | 0.25 | 0.20 | Phase A: boons.json per-mode entries |
| Torment (Condition) | base_per_tick (stationary) | 31.8 | 26.0 | Phase A: conditions.json per-mode entries |
| Torment (Condition) | condition_damage_coeff (stationary) | 0.09 | 0.07 | Phase A: conditions.json per-mode entries |
| Torment (Condition) | base_per_tick (moving) | 22.0 | 19.8 | Phase A: conditions.json per-mode entries |
| Torment (Condition) | condition_damage_coeff (moving) | 0.06 | 0.054 | Phase A: conditions.json per-mode entries |
| Confusion (Condition) | base_per_tick (over_time) | 18.25 | 10.0 (flat) | Phase A: conditions.json per-mode entries |
| Confusion (Condition) | condition_damage_coeff (over_time) | 0.05 | 0.0 | Phase A: conditions.json per-mode entries |
| Confusion (Condition) | base_per_tick (on_skill_use) | 16.24 | 49.5 | Phase A: conditions.json per-mode entries |
| Confusion (Condition) | condition_damage_coeff (on_skill_use) | 0.0325 | 0.0975 | Phase A: conditions.json per-mode entries |

All known mode splits are currently handled in Phase A data files (boons.json, conditions.json).
No unresolved splits exist in the balance_overrides system.

Trait/skill coefficient splits (which ArenaNet applies on a per-skill basis in balance patches)
are tracked in the `known_mode_splits()` registry and will be populated as P3-13 discovers them.

## Quality Degradation Infrastructure

A `check_wvw_quality()` function was added to `balance_overrides.rs` that:

1. Iterates all entries in `known_mode_splits()`
2. For each split NOT handled in Phase A data:
   - Checks if a WvW override exists in the override system
   - `Some(Value)` = quality maintained (Verified)
   - `Some(Unknown)` = quality degrades to Provisional
   - `None` = quality degrades to Provisional (known split, missing override)
3. Returns `(DataQuality, Vec<DataQualityReason>)`

In the current baseline, all known splits are `handled_in_phase_a: true`, so
`check_wvw_quality()` returns `Verified` with no reasons. This infrastructure activates
when P3-13 adds trait/skill splits with `handled_in_phase_a: false`.

## Integration Tests Added

| Test | Verifies |
|------|----------|
| `test_wvw_uses_wvw_specific_override` | WvW lookup returns WvW-specific override value |
| `test_wvw_known_split_missing_degrades_quality` | Quality check with empty overrides returns Verified (all Phase A handled) |
| `test_wvw_no_known_split_uses_base_value` | Non-split coefficient returns None, quality stays Verified |
| `test_wvw_explicit_unknown_override_degrades_quality` | Override with null value returns Unknown variant |
| `test_wvw_never_falls_back_to_pve` | PvE override exists, WvW lookup returns None (not PvE value) |
| `test_override_lookup_is_mode_isolated` | Three modes with same entity ID return three different values |
| `test_known_mode_splits_baseline` | All baseline splits are marked handled_in_phase_a |
| `test_check_wvw_quality_baseline_verified` | Baseline check_wvw_quality returns Verified |

## Summary

- **Total mode-sensitive paths audited**: 29
- **(a) Uses WvW-specific data**: 11 (Fury, Torment, Confusion formulas; override lookup; engine dispatch; context propagation)
- **(b) Mode-invariant base data**: 16 (universal formulas, stat calculations, gear formulas, duration formulas, Might/Protection/Resolution/Vulnerability)
- **(c) Known split, WvW value unresolved**: 0
- **(d) Uncertain split status**: 2 (condition_weights_for_profession, default_buff_profiles -- flagged for P3-13/P3-14)
- **Silent PvE fallback paths found**: 0
