# Contract: normalized effect record, Sprint 2 additions

Applies to every file under `data/normalized_effects/<patch>/{pve,pvp,wvw}.json`. Existing fields and their validation are unchanged; the loader must keep accepting every record that loads today.

## New optional fields

```json
{
  "effect_id": "rune:24836:1",
  "source_type": "Rune",
  "source_id": 24836,
  "source_name": "Superior Rune of the Scholar",
  "category": "TriggeredEffect",
  "inner_category": "StrikeDamagePct",
  "value": 5.0,
  "stacking_rule": "Multiplicative",
  "trigger_rule": "OnHealthThreshold",
  "health_threshold": { "above": true, "percent": 90.0 },
  "uptime_model": { "kind": "Unknown" },
  "evidence_level": "Factual",
  "source": "https://wiki.guildwars2.com/wiki/Superior_Rune_of_the_Scholar (read 2026-09-08)"
}
```

| Field | Present when | Rule |
|---|---|---|
| `health_threshold` | `trigger_rule` is `OnHealthThreshold` or `Conditional` | required for `OnHealthThreshold`; `percent` is a `FactualValue<f64>` in (0, 100]; `above` is a bool |
| `proc_chance` | `trigger_rule` is `OnCrit` or `OnHit` | `FactualValue<f64>` in (0, 1]; absent means 1.0 |
| `trigger_scope` | `trigger_rule` is `OnHit` or `OnSkillUse` | `"Any"` (default) or `"WeaponSkillWithRecharge"` |

Strengthened rule: `max_stacks` present ⇒ `effect_duration` present.

## Value semantics for `ProcEffect → StrikeDamagePct`

`value ≤ 2.0` is a damage coefficient applied to the unequipped weapon strength (690.5) and the player's power; the proc cannot crit. `value > 2.0` keeps today's meaning (percent of the strike). Only the Sigil of Fire record uses the coefficient form this sprint.

## Source field

`source` carries the wiki URL followed by ` (read YYYY-MM-DD)`. Records written this sprint: Sigil of Fire 24548, Rune of the Scholar 24836 (`:1`), Relic of the Thief 100916 (rewritten as `OnHit`, `TriggeredEffect → StrikeDamagePct`, `value 1.0`, `max_stacks 5`, `effect_duration 6.0`, `trigger_scope WeaponSkillWithRecharge`), and the triggered traits of the cached Reaper WvW build. No fixture value is copied.

## Shroud table (new file `data/formulas/shroud.json`)

```json
{
  "source": "https://wiki.guildwars2.com/wiki/Death_Shroud, https://wiki.guildwars2.com/wiki/Reaper%27s_Shroud (read 2026-09-08)",
  "life_force_pool_pct_of_health": 69.0,
  "entry_floor_pct": 10.0,
  "recharge_on_exit_s": 10.0,
  "shrouds": {
    "10574": { "name": "Death Shroud",     "drain_pct_per_s": { "pve": 3.0, "wvw": 3.0, "pvp": 5.0 }, "damage_reduction_pct": { "pve": 33.0, "wvw": 50.0, "pvp": 50.0 } },
    "30792": { "name": "Reaper's Shroud",  "drain_pct_per_s": { "pve": 4.0, "wvw": 5.0, "pvp": 5.0 }, "damage_reduction_pct": { "pve": 33.0, "wvw": 50.0, "pvp": 50.0 } },
    "62567": { "name": "Harbinger Shroud", "drain_pct_per_s": null, "damage_reduction_pct": null },
    "77238": { "name": "Ritualist's Shroud", "drain_pct_per_s": null, "damage_reduction_pct": null }
  }
}
```

`null` means unresolved: the build's resource model is reported incomplete and no number is invented. Values are re-read from the wiki at implementation and the read date updated.
