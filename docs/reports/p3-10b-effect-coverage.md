# P3-10b Effect Coverage Report

Generated: 2026-03-06
Phase: 1 (Representative Baseline)

## Coverage Summary

| Source Type | Total in Baseline (PvE) | Categories Covered | Notes |
|---|---|---|---|
| Trait | 10 | FlatStat, StrikeDamagePct, CritDamagePct, SpecificConditionDamagePct, AppliesBoon, AppliesCondition, StatConversion, RemovesBoon, CorruptsBoon, ConvertsConditionToBoon, TransfersCondition, IncomingStrikeMultiplier | Representative sample across Warrior, Ranger, Necromancer, Thief, Guardian |
| Rune | 3 | FlatStat, TriggeredEffect | Scholar (Power + conditional damage), Nightmare (ConditionDamage) |
| Sigil | 6 | StrikeDamagePct, ConditionDamagePct, ConditionDurationPct, SpecificConditionDurationPct, OutgoingHealingPct, BoonDurationPct, ProcEffect | Force, Bursting, Malice, Smoldering, Transference, Concentration, Fire |
| Relic | 3 | StrikeDamagePct, CritDamagePct, ConditionDurationPct, OutgoingHealingPct | Thief, Isgarren, Nightmare, Monk |
| Skill | 2 | RemovesCondition, DefianceDamage | Guardian Virtue of Resolve, Necromancer Signet of Undeath |

## Category Coverage

| Category | PvE Entries | PvP Entries | WvW Entries | Status |
|---|---|---|---|---|
| FlatStat | 4 | 2 | 2 | Covered |
| StatConversion | 1 | 0 | 0 | Covered (PvE only) |
| StrikeDamagePct | 4 | 3 | 3 | Covered |
| ConditionDamagePct | 1 | 1 | 1 | Covered |
| SpecificConditionDamagePct | 1 | 0 | 0 | Covered (PvE only) |
| CritDamagePct | 2 | 0 | 0 | Covered (PvE only) |
| BoonDurationPct | 1 | 0 | 0 | Covered (PvE only) |
| ConditionDurationPct | 2 | 0 | 0 | Covered (PvE only) |
| SpecificConditionDurationPct | 1 | 0 | 0 | Covered (PvE only) |
| OutgoingHealingPct | 2 | 0 | 0 | Covered (PvE only) |
| IncomingStrikeMultiplier | 1 | 0 | 0 | Covered (PvE only) |
| IncomingConditionMultiplier | 0 | 0 | 0 | Not yet populated |
| AppliesBoon | 1 | 1 | 1 | Covered |
| AppliesCondition | 1 | 1 | 1 | Covered |
| RemovesBoon | 1 | 1 | 1 | Covered |
| StealsBoon | 0 | 0 | 0 | Not yet populated |
| CorruptsBoon | 1 | 1 | 1 | Covered |
| RemovesCondition | 1 | 1 | 1 | Covered |
| ConvertsConditionToBoon | 1 | 0 | 0 | Covered (PvE only) |
| TransfersCondition | 1 | 0 | 0 | Covered (PvE only) |
| DefianceDamage | 1 | 0 | 0 | Covered (PvE only) |
| ProcEffect | 1 | 1 | 1 | Covered |
| TriggeredEffect | 1 | 1 | 1 | Covered |

**Covered: 21/23 categories** (IncomingConditionMultiplier and StealsBoon not yet populated)

## Mode Split Verification

The following effects have different values between PvE and PvP, validating mode-split handling:

| Effect | PvE Value | PvP Value | Notes |
|---|---|---|---|
| Sigil of Force (StrikeDamagePct) | 5.0% | 3.0% | PvP split balance |
| Sigil of Bursting (ConditionDamagePct) | 6.0% | 4.0% | PvP split balance |
| Sigil of Fire (ProcEffect) | 512.0 | 340.0 | Reduced PvP damage |
| Relic of the Thief (StrikeDamagePct) | 10.0% | 7.0% | PvP reduction |
| Rune of Scholar bonus (TriggeredEffect) | 5.0% | 3.0% | PvP reduction |
| Forceful Greatsword (FlatStat) | 150.0 | 120.0 | PvP trait split |

## Functions Delivered

| Function | Location | Purpose |
|---|---|---|
| `score_effect()` | `normalized_effects.rs` | Scores new 23-category NormalizedEffect entries against OptimizationWeights |
| `map_legacy_effect()` | `normalized_effects.rs` | Maps old 8-variant synergy::NormalizedEffect to new system |
| `effect_uptime()` | `normalized_effects.rs` | Computes effective uptime from UptimeModel |

## Phase 2 Requirements

Full programmatic extraction from GameDb needed for complete coverage.
Current baseline covers key patterns to validate the type system and scoring.

Missing for Phase 2:
- IncomingConditionMultiplier entries (need Resolution-like trait examples)
- StealsBoon entries (Mesmer/Thief specific)
- Broader profession coverage (currently focused on Warrior, Necromancer, Guardian, Ranger, Thief)
- Automated extraction from GW2 API Fact data
