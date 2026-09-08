# Contract: normalized effect record, Sprint 3 additions

Applies to every file under `data/normalized_effects/<patch>/{pve,pvp,wvw}.json`. Sprint 1 and 2 fields and their validation are unchanged; the loader keeps accepting every record that loads today (the fourteen in `2026-01-13/wvw.json` are the regression set).

## New trigger kinds

`trigger_rule` accepts, in addition to the six existing values: `OnShroudEnter`, `OnShroudExit`, `OnConditionApplied`, `OnConditionRemoved`, `OnBoonApplied`, `OnBoonStripped`, `Periodic`.

| Value | Fires when | Required companions |
|---|---|---|
| `OnShroudEnter` | the player's shroud entry skill resolves (Death, Reaper's, Harbinger, Ritualist's Shroud, Desert Shroud); never on Manifest Sand Shade | — |
| `OnShroudExit` | the shroud ends by skill, opener, drain or damage | — |
| `OnConditionApplied` | the player puts a condition on a foe (skill fact, record, corrupt) | `trigger_scope: {"Status": "<condition>"}` or `Any` |
| `OnConditionRemoved` | a cleanse removed at least one condition from the player (never a cleanse of nothing) | `trigger_scope: {"Status": "<condition>"}` (the last one removed) or `Any` |
| `OnBoonApplied` | a boon lands on the player | `trigger_scope: {"Status": "<boon>"}` or `Any` |
| `OnBoonStripped` | the player removes or corrupts a boon on a foe | `trigger_scope: {"Status": "<boon>"}` or `Any` |
| `Periodic` | every `internal_cooldown` seconds from the fight's start | `internal_cooldown` |

## New trigger scopes

`trigger_scope` accepts, in addition to `"Any"` and `"WeaponSkillWithRecharge"`:

```json
{ "Category": "Shout" }     // matches Skill.categories
{ "Slot": "Elite" }         // matches Skill.slot: Heal, Utility, Elite, Profession
{ "Status": "Fear" }        // a boon or condition name, for the three status triggers
```

Rule: a record with `source_type: Trait` and `trigger_rule: OnSkillUse` MUST carry a scope other than `Any` to execute. Validation rejects an explicit `Any`; an absent scope is accepted (three Sprint 1 records: Vicious Expression, Plague Sending, Pure of Voice) and the record stays on the coverage line as `(on-skill-use)` until it gets a scope.

## Prerequisite

```json
"prerequisite": {
  "foe_condition": "Chilled",
  "in_shroud": true,
  "foe_health": { "above": false, "percent": 50.0 }
}
```

Members are optional; an empty object is rejected. Allowed with any trigger. The existing `health_threshold` stays the player's own health gate.

## New categories

| `category` (or `inner_category`) | `value` | Extra fields |
|---|---|---|
| `GainsLifeForce` | percent of the life force pool | optional `scale_by` |
| `Heal` | flat healing | optional `healing_power_coefficient` (multiplies healing power, added to `value`), optional `scale_by` |

`scale_by` accepts `"ConditionsRemoved"`.

## Coverage block

```json
{
  "effect_id": "trait:1234:coverage",
  "source_type": "Trait", "source_id": 1234, "source_name": "Flesh of the Master",
  "category": "FlatStat", "value": { "unresolved": "classified, not executed" },
  "stacking_rule": "Additive", "trigger_rule": "Passive",
  "uptime_model": { "kind": "Unknown" }, "evidence_level": "Factual",
  "source": "https://wiki.guildwars2.com/wiki/Flesh_of_the_Master (read 2026-09-08)",
  "coverage": { "class": "NeedsMechanic", "mechanic": "minions" }
}
```

`derived_from` (optional, a list of numbers): the page numbers a derived `value` is computed from; the wiki check verifies these instead of `value`. Reaper's Onslaught carries `[300]` for its +20 % critical damage (300 ferocity at 15 per point).

`class` ∈ `PassiveNoEffect`, `NeedsMechanic`. `mechanic` is required for `NeedsMechanic` and names the game mechanism the simulator lacks, as the audit table lists it; the Necromancer increment uses `spirits`, `carapace threshold`, `carapace stat scaling`, `protection condition reduction`, `blight`, `shades`, `life siphon`, `barrier`, `ally state`, `recharge`, `trait skill`, `weapon-scoped duration`, `downed`, `minions`, `incoming damage reduction`, `life force scaling`, `dodge`, `revive`, `disable trigger`, `condition damage heal`, `crit chance per stack`, `fear damage`, `kill`, `percent heal`, `elixir`, `incoming condition duration`, `damage-scaled heal`, `marks`, `incoming healing`, `life force threshold`; later increments add theirs (`pet`, `clone`, `bundle`, `attunement`, `legend`, `transform`). A name is retired when the mechanism lands (Sprint 3 convergence retired `carapace`). A record with `coverage` MUST NOT carry `status_operation`, `inner_category`, `prerequisite` or a resolved `value`.

## Worked examples (Necromancer, WvW column, wiki read 2026-09-08)

Speed of Shadows (888): `OnShroudEnter`, `AppliesBoon` Swiftness 10 s self, plus a second record `RemovesCondition` scoped to movement-impairing conditions (Crippled, Chilled, Immobile) as three `status_kind` entries under one `effect_id` suffix each.

Soul Barbs (894): two records, `OnShroudEnter` and `OnShroudExit`, each `TriggeredEffect → StrikeDamagePct` value 10, `effect_duration` per the mode column (the page splits 15 s / 10 s; the WvW number is the one the record carries).

Death Perception (893): the +10 % critical chance stays a `Passive` `FlatStat`-style percent the parser already reads; the in-shroud half is `Conditional`, `prerequisite.in_shroud: true`, `TriggeredEffect → CritDamagePct` 15.

Chilling Nova (2020): `OnCrit`, `prerequisite.foe_condition: "Chilled"`, `internal_cooldown` 3, `TriggeredEffect → StrikeDamagePct` 1.125 (coefficient form) with `status_operation` AppliesCondition Chilled 2 s to enemies, `target_count` 5.

Dread (919): `OnConditionApplied`, `trigger_scope: {"Status": "Fear"}`, `internal_cooldown` 1, the boons and the strike bonus as the page states them.

Spiteful Fortitude (829): `OnHit`, `prerequisite.foe_health: {"above": false, "percent": 50}`, `GainsLifeForce` 1.

Blighter's Boon (1932): two records, `OnBoonApplied` (scope `Any`) and `OnBoonStripped` (scope `Any`), each `GainsLifeForce` 1 and `Heal` 133 with `healing_power_coefficient` per the page.

Unholy Martyr (1692): `OnShroudEnter` transfers 5 conditions from allies (`NeedsMechanic("ally state")` for the transfer half this sprint, stated in the record's `source` note); `OnShroudExit` `RemovesCondition` 3 self plus `GainsLifeForce` 7 `scale_by: ConditionsRemoved`.

## Source field

Unchanged: the wiki URL followed by ` (read YYYY-MM-DD[: note])`. Every record written this sprint carries it; `records_this_sprint_carry_read_dates` is extended to every record with a Sprint 3 field or a Sprint 3 trigger.
