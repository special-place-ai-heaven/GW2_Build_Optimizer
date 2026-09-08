# Sprint 2 seen-failing evidence

One heading per control from `specs/005-wvw-proc-sites/tasks.md`. Each body is the
panic block and result line captured by `python docs/audit/disable_and_run.py <control>`
(or by running the control before its mechanism existed), pasted verbatim.

## US1 on-crit procs

### reaper_oncrit_positive_control_fires_from_crits

Before the firing site existed (commit after `4721463`, run 2026-09-08):

```
thread 'rotation::wvw_timeline::reaper_experiments::reaper_oncrit_positive_control_fires_from_crits' panicked at crates\optimizer\src\rotation\wvw_timeline.rs:4894:9:
the on-crit sigil fires from the opener's critical hits; trace: [TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Superior Sigil of Fire (on-crit)", detail: "no firing site" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "\"Chilled to the Bone!\" (no record)", detail: "no record" }, ...]
test result: FAILED. 2 passed; 3 failed
```

Disabled again after the edit with `python docs/audit/disable_and_run.py oncrit` (the OnCrit call in `apply_skill_effect` gated off): harness output (2026-09-08):

```
### oncrit
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: if crit > 0.0 {
                    for _ in 0..*hit_count {
test: reaper_oncrit_positive_control_fires_from_crits
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5086:9:
the on-crit sigil fires from the opener's critical hits; trace: [TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "\"Chilled to the Bone!\" (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "\"You Are All Weaklings!\" (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Death Spiral (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Death's Charge (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Dusk Strike (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Ghastly Claws (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Grasping Darkness (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Gravedigger (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Infusing Terror (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Life Rend (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Nightfall (no record)", detail: "no record" }, TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Reaper Adept Left (no record)", detail: "no record" }, TraceEvent { t_ms: 0, k
```

### reaper_oncrit_zero_precision_never_fires

Passes before and after: with no firing site nothing fires, and with the site a zero
crit chance still fires nothing. Negative control, kept as a regression guard
(`test result: ok` on 2026-09-08 before the edit).

### reaper_oncrit_icd_bounds_rate

```
thread 'rotation::wvw_timeline::reaper_experiments::reaper_oncrit_icd_bounds_rate' panicked at crates\optimizer\src\rotation\wvw_timeline.rs:4960:9:
assertion `left == right` failed: one fire inside one 5 s cooldown window; trace: [TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Superior Sigil of Fire (on-crit)", detail: "no firing site" }, TraceEvent { t_ms: 400, kind: HitLanded, source: "Gravedigger", detail: "1391.6" }, ... TraceEvent { t_ms: 4450, kind: HitLanded, source: "Soul Spiral", detail: "87.0" }]
  left: 0
 right: 1
```

### reaper_oncrit_trials_bracket_expected_value

```
thread 'rotation::wvw_timeline::reaper_experiments::reaper_oncrit_trials_bracket_expected_value' panicked at crates\optimizer\src\rotation\wvw_timeline.rs:4996:32:
trials exist for the sigil: []
```

## US2 weapon swap sigils

### reaper_swap_loads_set_two_sigils
Before set-2 sigils loaded (2026-09-08):

```
thread 'rotation::wvw_timeline::reaper_experiments::reaper_swap_loads_set_two_sigils' (1510468) panicked at crates\optimizer\src\rotation\wvw_timeline.rs:4653:9:
set-2 sigil fires only after the swap at 1350 ms: []
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Harness `python docs/audit/disable_and_run.py swap` after the edit (set 2 mapped to set 1):

```
### swap
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: spec.weapon_set = sigil_sets.get(&spec.source_id).copied().unwrap_or(0);
test: reaper_swap_loads_set_two_sigils
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:4679:9:
set-2 sigil fires only after the swap at 1350 ms: [TraceEvent { t_ms: 400, kind: ProcFired, source: "Superior Sigil of Fire", detail: "StrikeDamagePct �1.00" }]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
error: test failed, to rerun pass `-p gw2-optimizer --lib`
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1143 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical
```


### reaper_swap_keeps_icd_across_sets
Passes before and after: a sigil socketed on both sets already loaded through set 1
with one shared cooldown. Timing guard, not seen-failing (`test result: ok`, 2026-09-08).


### reaper_set_two_sigil_leaves_coverage_line
Before (the stowed sigil was neither named nor loaded, so the first assertion passed
by omission; the trial-entry assertion is the control):

```
thread 'rotation::wvw_timeline::reaper_experiments::reaper_set_two_sigil_leaves_coverage_line' panicked at crates\optimizer\srcotation\wvw_timeline.rs:4718:9:
the stowed sigil's record is loaded (it has a trial entry): []
```


## US3 conditional bonuses

### reaper_scholar_applies_only_above_threshold

Before the timeline had conditional specs (2026-09-08), all three US3 controls and the
PvE guard's first run:

```
thread 'rotation::wvw_timeline::reaper_experiments::reaper_scholar_applies_only_above_threshold' (1629080) panicked at crates\optimizer\src\rotation\wvw_timeline.rs:4916:9:
the threshold is true at the start of the fight: []

--
thread 'rotation::wvw_timeline::reaper_experiments::reaper_unresolved_conditional_stays_named' (1689712) panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5043:9:
named as unresolved: ["Superior Rune of the Scholar (on-health-threshold)"]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
--
thread 'rotation::wvw_timeline::reaper_experiments::reaper_thief_stacks_cap_and_expire' (1661240) panicked at crates\optimizer\src\rotation\wvw_timeline.rs:4994:9:
six qualifying weapon-skill hits gain or refresh: []

--
thread 'referee::tests::pve_output_unchanged_by_conditional_tagging' (1689748) panicked at crates\optimizer\src\referee.rs:2974:13:
PvE value 0 moved: got [0.01988142845871873, 0.0, 0.05594285714285714, 0.15, 0.4302897574123989, 0.03833333333333334, 0.09310281831080738], pinned [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
```

Harness after the edit (`threshold`: the threshold forced true; `stack`: the cap removed):

```
### threshold
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: let holds = if above {
test: reaper_scholar_applies_only_above_threshold
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5126:9:
the bonus expires at the crossing: []
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
error: test failed, to rerun pass `-p gw2-optimizer --lib`
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1147 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical

### stack
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: spec.stacks = (spec.stacks + 1).min(max);
test: reaper_thief_stacks_cap_and_expire
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5180:9:
never a sixth stack: [TraceEvent { t_ms: 400, kind: StackGained, source: "Relic of the Thief", detail: "1/5" }, TraceEvent { t_ms: 750, kind: StackGained, source: "Relic of the Thief", detail: "2/5" }, TraceEvent { t_ms: 1250, kind: StackGained, source: "Relic of the Thief", detail: "3/5" }, TraceEvent { t_ms: 1550, kind: StackGained, source: "Relic of the Thief", detail: "4/5" }, TraceEvent { t_ms: 1800, kind: StackGained, source: "Relic of the Thief", detail: "5/5" }, TraceEvent { t_ms: 3050, kind: StackGained, source: "Relic of the Thief", detail: "6/5" }, TraceEvent { t_ms: 9250, kind: StackGained, source: "Relic of the Thief", detail: "1/5" }, TraceEvent { t_ms: 9600, kind: StackGained, source: "Relic of the Thief", detail: "2/5" }]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
error: test failed, to rerun pass `-p gw2-optimizer --lib`
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1147 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical
```

### reaper_thief_stacks_cap_and_expire

See the block above (same runs).

### reaper_unresolved_conditional_stays_named

See the block above: before the edit the rune was named "(on-health-threshold)", an unsupported trigger, never "(unresolved value)".

## US4 dark combos

### reaper_dark_whirl_life_steals

Before the dark arm existed (2026-09-08):

```
test rotation::wvw_timeline::reaper_experiments::reaper_dark_whirl_life_steals ... FAILED
test rotation::wvw_timeline::reaper_experiments::reaper_expired_field_makes_no_combo ... FAILED

failures:
--
thread 'rotation::wvw_timeline::reaper_experiments::reaper_dark_whirl_life_steals' (1971316) panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5271:9:
the dark whirl combo resolves: []
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
--
thread 'rotation::wvw_timeline::reaper_experiments::reaper_expired_field_makes_no_combo' (1971452) panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5333:9:
the finisher comes after the field: 3200

--
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 1148 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p gw2-optimizer --lib`
```

Harness `dark` after the edit (arm returns to the degraded note):

```
### dark
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: let damage = 198.0 + 0.03 * self.params.power;
test: reaper_dark_whirl_life_steals
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5313:9:
the dark whirl combo resolves: []
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
error: test failed, to rerun pass `-p gw2-optimizer --lib`
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1149 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical
```

### reaper_expired_field_makes_no_combo (regression guard, not seen-failing)

First run failed on its own premise (the finisher landed at 3 200 ms, inside the 5 s field); the opener was lengthened so the finisher lands after 5 s. Passes before and after the dark arm.

## US6 life force and shroud

### reaper_shroud_refused_without_life_force

### reaper_life_force_gain_capped

### reaper_shroud_drains_and_exits

### shroud_bar_is_prepared_from_transform_skills

### resource_model_completeness_matches_previous_list
