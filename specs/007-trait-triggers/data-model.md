# Data model: Trait triggers are the build (007)

Entities from the spec mapped onto the types that exist. Field names are proposals for `/speckit-tasks`; the invariants are the contract. Every addition is optional and `serde(default)` so Sprint 2 records load unchanged.

## Trait record (`NormalizedEffect`, extended)

Existing: `effect_id`, `source_type`, `source_id`, `source_name`, `category`, `value`, `stacking_rule`, `trigger_rule`, `uptime_model`, `evidence_level`, `source`, `effect_duration`, `internal_cooldown`, `max_stacks`, `status_operation`, `inner_category`, `health_threshold`, `proc_chance`, `trigger_scope`.

| New field | Type | Meaning | Validation |
|---|---|---|---|
| `prerequisite` | `Prerequisite` | what must hold at the trigger instant | at least one member present |
| `healing_power_coefficient` | `FactualValue<f64>` | added to `value` × healing power for `Heal` | only with `Heal` |
| `scale_by` | `ScaleBy` | multiply the effect by a count known at the firing site | only with `GainsLifeForce` or `Heal` |
| `coverage` | `CoverageBlock` | this trait is classified, not executed | excludes `status_operation`, `inner_category`, a resolved `value` |

`TriggerRule` (extended): `Passive`, `OnCrit`, `OnHit`, `OnSkillUse`, `OnHealthThreshold`, `Conditional`, **`OnShroudEnter`, `OnShroudExit`, `OnConditionApplied`, `OnBoonApplied`, `OnBoonStripped`, `Periodic`**.

- `OnSkillUse` with `source_type: Trait` requires `trigger_scope` ≠ `Any`.
- `Periodic` requires `internal_cooldown` (the period, seconds).
- `Conditional` with `prerequisite.in_shroud` and a strike/crit-damage category becomes `ConditionalKind::InShroud`.

`TriggerScope` (extended): `Any`, `WeaponSkillWithRecharge`, **`Category(String)`** (an API `Skill.categories` entry), **`Slot(String)`** (`Heal`, `Utility`, `Elite`, `Profession`), **`Status(String)`** (a boon or condition name for `OnConditionApplied` / `OnBoonApplied` / `OnBoonStripped`).

`EffectCategory` (extended): the 22 existing plus **`GainsLifeForce`** (`value` = percent of the pool) and **`Heal`** (`value` = flat amount).

### `Prerequisite`

| Member | Type | Runtime read |
|---|---|---|
| `foe_condition` | `String` | the primary foe carries it now (`outgoing_conditions`, unexpired) |
| `in_shroud` | `bool` | `in_shroud.is_some()` equals it |
| `foe_health` | `HealthThreshold` | `enemy_health / target_health` versus `percent` with `above`; never met on the open dummy |

A failed prerequisite traces `ProcSkippedPrerequisite` with the member name; it is not a coverage entry.

### `ScaleBy`

`ConditionsRemoved`: the count the same firing's `RemovesCondition` operation removed (Unholy Martyr). One variant; more are added when a page needs them.

### `CoverageBlock`

```json
"coverage": { "class": "NeedsMechanic", "mechanic": "minions" }
```

`class` ∈ `PassiveNoEffect` | `NeedsMechanic`; `mechanic` required for `NeedsMechanic`, forbidden otherwise. A record with `coverage` has `trigger_rule: Passive`, `category: FlatStat`, `value` unresolved, and exists so the audit lists the trait's state from data.

## Runtime proc (`ProcSpec`, extended)

| New field | Type | Meaning |
|---|---|---|
| `prerequisite` | `Option<Prerequisite>` | checked in `trigger_procs` before the mass step |
| `scale_by` | `Option<ScaleBy>` | multiplies `value` at the firing site |
| `healing_power_coefficient` | `f64` | 0 when absent |

`ConditionalKind` (extended): `Threshold`, `Stacking`, **`InShroud`** (active iff in shroud; re-evaluated in `update_conditionals` every strike and tick).

## Rotation skill (`RotationSkill`, extended)

| New field | Type | Source |
|---|---|---|
| `categories` | `Vec<String>` | API `Skill.categories` |
| `slot_name` | `Option<String>` | API `Skill.slot` |

`SkillEffect::ApplyBuff` / `ApplyCondition` / `Healing` / `RemovesCondition` gain `targets: u32` (1 when the fact has no `Number of Targets`), read by the builder.

## Fight population (`FightPopulation`)

```json
{ "source": "scenario tiers, crates/optimizer/src/scenario.rs and referee.rs (Roam 1v1, Havoc 5-15, Cloud/Zerg)",
  "tiers": { "Solo": { "foes": 1, "allies": 0 }, "Party": { "foes": 5, "allies": 4 }, "Squad": { "foes": 10, "allies": 9 } } }
```

Loaded once (`data/fight_population.rs`, embedded like the other formula files), selected by `ScenarioSpec.combat_tier`, carried on `WvwTimelineInput.population`. Invariants: `foes ≥ 1`; `allies ≥ 0`; the player is not counted among allies.

Counting at a firing with `target_count = n` (record) or `targets = n` (skill fact):

```
foe-facing:  applied_to = min(n, foes);   primary gets the state change; (applied_to − 1) × amount → cleave totals
ally-facing: applied_to = 1 + min(n − 1, allies); player gets the buff; (applied_to − 1) × stacks × duration → ally_boon_stack_seconds
self-only:   applied_to = 1
```

## WvW report (`WvwCombatReport`, extended)

| New field | Type | Meaning |
|---|---|---|
| `cleave_damage` | `f64` | strike damage credited to secondary foes; also added to `total_damage` and `protected_damage` |
| `cleave_condition_stack_seconds` | `f64` | condition stacks × seconds on secondary foes |
| `ally_boon_stack_seconds` | `f64` | boon stacks × seconds on allies other than the player |
| `ally_healing` | `f64` | healing credited to allies |
| `ally_cleanses` | `u32` | conditions removed from allies |
| `trait_fire_counts` | `BTreeMap<String, u32>` | per trait record, how often it fired (weighted fires round to whole) |
| `coverage` | `Vec<CoverageEntry>` | `{ name, class, detail }` replacing the string-only `unmodeled_sources` for callers that classify; `unmodeled_sources` keeps the rendered strings |

`TraceKind` (extended): `TraitFired`, `ProcSkippedPrerequisite`, `PopulationApplied`, `ShroudBonusActive`, `ShroudBonusEnded`.

## Coverage state (`ReasonClass`, `CoverageEntry`)

`ReasonClass` ∈ `NoRecord` | `PassiveNoEffect` | `NeedsMechanic` | `UnresolvedValue` | `NoFiringSite`. Rendered suffix per `contracts/wvw-report.md`. A source is in exactly one of: executed from facts, executed from a record, `CoverageEntry` with a class. `DataQuality` is Verified iff the entry list is empty.

## Audit row (`docs/audit/trait-coverage.md`)

`| profession | line | trait id | trait | state | source |` where `state` ∈ `facts` | `record` | `facts+record` | one `ReasonClass`, and `source` is the record's wiki URL with its read date or `—`. One row per trait in the cache; the generating test fails on a trait with no row.

## State transitions

Shroud:

```
enter_shroud: in_shroud = Some → trigger OnShroudEnter → InShroud conditionals activate (trace ShroudBonusActive)
exit_shroud(why): trigger OnShroudExit (why in trace) → InShroud conditionals end (ShroudBonusEnded) → in_shroud = None
```

Proc with a prerequisite:

```
trigger matches ∧ scope admits ∧ set held
  → prerequisite fails: trace ProcSkippedPrerequisite(member); no mass
  → holds: Sprint 2 mass / cooldown step; fire; TraitFired for SourceType::Trait
```

Periodic: `next_ready_ms` starts at 0; fires on the first tick, then every `internal_cooldown`.
