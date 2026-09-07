# Contract: WvW combat report and trace, Sprint 2 additions

Consumers: the referee (`evaluate_validated_build_with`), `synergy_result_from_validated`, the `score_build` tool, and the experiments. Existing fields keep their meaning; additions are append-only.

## `WvwCombatReport`

| Field | Type | Contract |
|---|---|---|
| `proc_trials` | `Vec<ProcTrial>` | empty unless the input had `trace: true`; one entry per proc source that fired or could have fired; `mean` is over exactly 8 fixed seeds; `min`/`max` are the extreme counts |
| `shroud_refusals` | `Vec<String>` | one readable line per refused entry, e.g. `Reaper's Shroud needs 10% life force, had 4%`; the referee appends each to `quality_reasons` with field `wvw_timeline.resources` |
| `unmodeled_sources` | unchanged | a source that loaded into a proc or conditional spec is absent; unresolved values and unsupported triggers remain |

`ProcTrial { source: String, mean: f64, min: u32, max: u32 }`.

## `TraceKind`

Existing: `HitLanded`, `ProcFired`, `ProcSkippedIcd`, `ProcUnmodeled`, `CastInterrupted`, `WeaponSwap`.

Added: `ConditionalActivated`, `ConditionalExpired`, `StackGained`, `ComboResolved`, `ShroudEntered`, `ShroudExited`, `ShroudRefused`, `LifeForceGained`.

`detail` conventions: `ProcFired` carries `"{category:?} ×{p:.2}"` in expected-value mode and `"{category:?} (trial {seed})"` in trial mode; `ComboResolved` carries `"{field} field + {finisher} finisher → {outcome}"`; `ShroudExited` carries `"life force 0"`, `"exit skill"` or `"opener"`; `LifeForceGained` carries the new pool in percent.

Cap and truncation unchanged: 512 events, `trace_truncated` set when exceeded. The fixture opener must fit under the cap with every new kind enabled.

## `WvwTimelineInput`

Unchanged shape. `active_effects` now includes records for all four sigil seats; `resource_rules` may carry life force rules; `trace` also enables the 8-seed trial pass.

## `SimulationResult` / referee

`RealizedAxes`, `viability` and every score are computed exactly as today from the report's totals. No constant, gate or normalisation changes.
