# Contract: WvW report, trace and coverage line, Sprint 3

## `WvwCombatReport` additions

| Field | Type | Invariant |
|---|---|---|
| `cleave_damage` | `f64` | ≥ 0; already included in `total_damage` and, when protected, in `protected_damage` |
| `cleave_condition_stack_seconds` | `f64` | ≥ 0 |
| `ally_boon_stack_seconds` | `f64` | ≥ 0; 0 in every Solo-tier fight |
| `ally_healing` | `f64` | ≥ 0 |
| `ally_cleanses` | `u32` | 0 in every Solo-tier fight |
| `trait_fire_counts` | `BTreeMap<String, u32>` | one entry per trait record that fired at least once |
| `coverage` | `Vec<CoverageEntry { name, class: ReasonClass, detail: Option<String> }>` | one entry per skipped source; empty ⇔ `DataQuality::Verified` from the timeline |

`unmodeled_sources` keeps the rendered strings, now one per `coverage` entry, formatted `"{name} ({suffix})"`.

## Coverage line suffixes

| `ReasonClass` | Suffix |
|---|---|
| `NoRecord` | `no record` |
| `PassiveNoEffect` | `passive, no simulated effect` |
| `NeedsMechanic` | `needs: {mechanic}` |
| `UnresolvedValue` | `unresolved value` |
| `NoFiringSite` | `no firing site` |

Guarantees: no skill the builder produced a `SkillEffect` for appears; no trait the parser consumed a fact from appears unless it also carries a record that could not execute (then with that record's class); the rendered line is `Not simulated: a, b, c and N others` as before; an empty list yields no reason and the build is Verified.

## Trace kinds added

| Kind | Source | Detail |
|---|---|---|
| `TraitFired` | trait name | `{category} ×{weight}` plus `at entry` / `at exit ({why})` / `periodic` |
| `ProcSkippedPrerequisite` | trait name | `foe not Chilled` / `not in shroud` / `foe above 50%` / `prerequisite never met` (summary at end) |
| `PopulationApplied` | source name | `{applied_to} of {foes|allies} ({cap})` |
| `ShroudBonusActive` / `ShroudBonusEnded` | trait name | `×1.15 crit damage` |

The trace stays capped at 512 events and empty unless requested.

## Rank key (WvW only)

`search_rank` for `CombatKind::Support | Commander | Staller`: the output slot that is `sustain_margin.max(0.0)` today becomes `sustain_margin.max(0.0) + ally_boon_stack_seconds / 1000.0`. No other slot, gate or constant changes. PvE and PvP keys are untouched.

## `WvwTimelineInput` additions

`population: FightPopulation` (from the scenario's tier) and nothing else; `active_effects` carries the trait records as it carries every record.
