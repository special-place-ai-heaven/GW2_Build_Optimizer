# Data model: WvW proc firing sites

Entities from the spec, mapped onto the types that already exist. Field names are proposals for `/speckit-tasks`; the invariants are the contract.

## Proc record (`NormalizedEffect`, extended)

Existing: `effect_id`, `source_type`, `source_id`, `source_name`, `category`, `value`, `stacking_rule`, `trigger_rule`, `uptime_model`, `evidence_level`, `source`, `effect_duration`, `internal_cooldown`, `max_stacks`, `status_operation`, `inner_category`.

New, all optional and `serde(default)`:

| Field | Type | Meaning | Validation |
|---|---|---|---|
| `health_threshold` | `{ above: bool, percent: FactualValue<f64> }` | prerequisite on the regular health pool, percent of max | required when `trigger_rule` is `OnHealthThreshold`; `percent` in (0, 100] |
| `proc_chance` | `FactualValue<f64>` | fraction in (0, 1]; absent means certain | only with `OnCrit` or `OnHit` |
| `trigger_scope` | `Any` \| `WeaponSkillWithRecharge` | which activating skills count | only with `OnHit` or `OnSkillUse` |

Existing rule strengthened: `max_stacks` present ⇒ `effect_duration` present.

Unresolved `FactualValue` anywhere in a record that is otherwise executable keeps the source on the coverage line as "(unresolved value)" (today's behaviour in `load_normalized_effects`).

## Runtime proc (`ProcSpec`, extended)

Existing: `source_type`, `source_id`, `source_name`, `trigger`, `category`, `value`, `duration_ms`, `internal_cooldown_ms`, `next_ready_ms`, `operation`.

| New field | Type | Meaning |
|---|---|---|
| `weapon_set` | `u8` | 0 not a sigil; 1 or 2 from the sigil seat; skipped by `trigger_procs` when not the held set |
| `proc_chance` | `f64` | record value or 1.0 |
| `mass` | `f64` | accumulated expected-value probability; cooldown starts when it reaches 1.0 |
| `scope` | `TriggerScope` | from the record |

State transitions for the expected-value process on a landed hit with crit chance `c`:

```
p = c × proc_chance            (OnCrit)        p = proc_chance (OnHit)
if now ≥ next_ready: apply p × effect; mass += p
   if mass ≥ 1: mass -= 1; next_ready = now + icd; trace ProcFired
   else trace ProcFired with weight p
else trace ProcSkippedIcd
```

Seeded trial mode (trace only): `p` is replaced by a Bernoulli draw from the trial's xorshift64* stream; on success the effect applies in full and the cooldown starts.

## Conditional bonus (`ConditionalSpec`, new)

| Field | Type | Meaning |
|---|---|---|
| `source_id`, `source_name`, `source_type` | as records | identity for trace and coverage |
| `kind` | `Threshold { above, percent }` \| `Stacking { max, duration_ms, scope }` | one of the two shapes this sprint executes |
| `value` | `f64` | strike multiplier per activation or per stack (fraction) |
| `active` | `bool` | threshold state as of the last evaluation; drives Activated/Expired trace events |
| `stacks`, `expires_at_ms` | `u32`, `u32` | stacking state |

Per-strike multiplier: `Π (1 + value)` over active thresholds × `Π (1 + stacks × value)` over stacking specs. Threshold state is re-evaluated at every strike and in `expire_timed_state`; re-activation after healing above the line is a normal transition. A stack gained past `max` refreshes `expires_at_ms` without changing `stacks`.

## Active set

Derived, not stored: `Timeline.active_weapon_set` plus `ProcSpec.weapon_set`. `ScheduledHit` gains `weapon_set: u8` captured at cast start so a mid-channel swap lands the remaining hits with the set they were cast from.

## Life force (`ResourceKind::LifeForce`, `SkillResourceRule` extended, `ShroudState` new)

`resource_cap(LifeForce) = 0.69 × params.max_health`; `initial_resources` starts at 0 (a fight starts empty unless the scenario says otherwise; the fixture opener builds it first).

`SkillResourceRule` new fields (all default 0 / false):

| Field | Meaning |
|---|---|
| `gain_on_use` | absolute life force added when the cast resolves (from Percent fact "Life Force") |
| `gain_on_hit` | existing; used for "Life Force Per Hit" |
| `entry_floor` | minimum pool to start the cast (10% of cap for shroud entry skills) |
| `drain_per_second` | pool lost per second while this skill's shroud is active |
| `enters_shroud` / `exits_shroud` | flags on the entry skill and its flip skill |

`ShroudState { entry_skill_id, entered_at_ms, drain_per_second }` on the timeline (`Option`). Transitions:

```
enter:  pool ≥ entry_floor ⇒ in_shroud = Some; trace ShroudEntered
        else refuse; reason "Reaper's Shroud needs 10% life force, had 4%"; trace ShroudRefused
tick:   pool -= drain × tick; pool ≤ 0 ⇒ exit (forced): cancel pending cast, trace ShroudExited
exit skill / opener exit ⇒ exit; entry skill recharge 10 s starts (already a Recharge fact)
damage while in shroud: pool -= dmg × 0.5 (WvW); overflow to player_health
heal while in shroud: no-op
```

`RotationSkill.bar: Bar { Weapon, Shroud, Slot }` (default `Weapon` for weapon_set 1/2, `Slot` for weapon_set 0). `pick_skill` filter: `Shroud` only while in shroud, `Weapon` only while not, `Slot` always.

Shroud table (source-dated, in `data/`, keyed by entry skill id): entry floor percent, drain percent per second per mode, damage reduction per mode. Rows: Death Shroud 10574, Reaper's Shroud 30792, Harbinger Shroud 62567, Ritualist's Shroud 77238. Unstated numbers are unresolved and mark the build's resource model incomplete.

## Trace event (`TraceEvent`, `TraceKind` extended)

New kinds: `ConditionalActivated`, `ConditionalExpired`, `StackGained`, `ComboResolved`, `ShroudEntered`, `ShroudExited`, `ShroudRefused`, `LifeForceGained`. Cap unchanged at 512, `trace_truncated` unchanged.

## Report (`WvwCombatReport`, extended)

| Field | Type | Notes |
|---|---|---|
| `proc_trials` | `Vec<ProcTrial { source: String, mean: f64, min: u32, max: u32 }>` | empty unless `trace`; 8 seeds |
| `shroud_refusals` | `Vec<String>` | readable reasons; the referee appends them to `quality_reasons` |

## Coverage line

Unchanged shape (`data::quality::coverage_reason`). Semantics this sprint: a source whose record loaded into `proc_specs` or `ConditionalSpec` is modeled and leaves the line; a sigil in the stowed set with a record is modeled (it will fire after a swap); unresolved values and unsupported triggers stay.

## Damage modifiers (`DamageModifiers`, extended)

`conditional_strike: Vec<ConditionalClause { source_id: u32, value: f64, above: bool, percent: f64 }>` recorded by `parse_percent_clauses` beside the existing flattened contribution. Only the WvW branch of `simulate_prepared_with` reads it, to divide the flattened factor out of `strike_mult` for sources whose record loaded.
