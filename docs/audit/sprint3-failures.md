# Sprint 3 seen-failing evidence

One heading per control from `specs/007-trait-triggers/tasks.md`. Each body is the
panic block and result line captured by `python docs/audit/disable_and_run.py <control>`
(or by running the control before its mechanism existed), pasted verbatim.

## Consumers touched

SymForge `status` 2026-09-08: index ready, 271 files. `find_references` per symbol the
plan changes (test modules omitted; counts are the index's, recall best-effort):

| symbol | consumers outside its own file |
|---|---|
| `TriggerRule` | `data/mod.rs` re-export; `engine.rs` `wvw_params_without_executed_conditionals`; `rotation/builder.rs` `cleanse_ne` (test); `rotation/reaper_fixture.rs` record builders; `rotation/wvw_timeline.rs` `ProcSpec`, `resolve_pending_cast`, `try_stunbreak`, `apply_skill_effect`, `trigger_procs`, `trigger_label`, `same_trigger` (exhaustive matches to extend) |
| `TriggerScope` | `rotation/reaper_fixture.rs` `records_with_threshold_and_stack`; `rotation/wvw_timeline.rs` `ProcSpec.scope`, `ConditionalKind`, `scope_admits` |
| `EffectCategory` | `data/consistency_tests.rs`; `rotation/builder.rs` `enrich_with_cleanse`; `rotation/reaper_fixture.rs`; `rotation/wvw_timeline.rs` `ProcSpec.category`, `load_normalized_effects` (~911), `skill_directly_models_effect` (~2247), `trigger_procs` (~2520) |
| `NormalizedEffect` (data) | `engine.rs` `wvw_params_without_executed_conditionals`, `active_normalized_effects`; `rotation/builder.rs` `enrich_with_cleanse`; `rotation/reaper_fixture.rs`; `rotation/wvw_timeline.rs` `WvwTimelineInput.effects`, `load_normalized_effects`, `skill_directly_models_effect`. (`synergy.rs` has an unrelated enum of the same name.) |
| `ProcSpec` | `rotation/wvw_timeline.rs` only: `Timeline.procs`, `load_normalized_effects` |
| `ConditionalKind` | `rotation/wvw_timeline.rs` only: `ConditionalSpec`, loader (~926, 951), `update_conditionals` (~2311-2379) |
| `RotationSkill` | `addon/ui/main_view/optimization.rs` `simulate_suggestion_rotation`; `engine.rs` `PreparedRotation`, `active_normalized_effects`, `resource_model_complete`, `wvw_resource_rules`, `rotation_skill`; `rotation/builder.rs` (constructors `skill_to_rotation_for_context`, `merge_weapon_sets`); `rotation/combat_model.rs` kit predicates; `rotation/simulator.rs`; `rotation/wvw_timeline.rs`; `tests/math_permutations.rs` (struct literals) |
| `WvwCombatReport` | `grouped_sheet.rs` `dummy_rotation`; `referee.rs` `make_viable_rotation` (test); `rotation/mod.rs` `SimulationResult`; `rotation/wvw_timeline.rs` `evaluate_wvw_timeline`, `report`, `run_report` |
| `search_rank` | `addon/ui/main_view/chat_flow.rs` `plate_shortfall`; `addon/ui/main_view/optimize_flow.rs` `apply_improve_baseline_gate`; `engine.rs` `llm_advisor`; `search_v2.rs` `refine_piece_swaps_within`, `repair_seed`, `optimize_v2_search`; `grouped_sheet.rs`, `referee.rs` tests; examples `necro_holes_check`, `nudge_druid_check`, `scourge_support_check` |
| `active_normalized_effects` | `engine.rs` `simulate_prepared_with` only |

## coverage

### coverage_line_never_names_executed_weapon_skills

Before the executed-source subtraction existed (`1e760db`, run 2026-09-08): the production coverage line named all 40 equipped sources as "(no record)", Gravedigger among them.

```
thread 'rotation::wvw_timeline::reaper_experiments::coverage_line_never_names_executed_weapon_skills' (155492) panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5088:9:
executed weapon skills leave the coverage line: ["\"Chilled to the Bone!\" (no record)", "\"You Are All Weaklings!\" (no record)", "Death Spiral (no record)", "Death's Charge (no record)", "Dusk Strike (no record)", "Ghastly Claws (no record)", "Grasping Darkness (no record)", "Gravedigger (no record)", "Infusing Terror (no record)", "Life Rend (no record)", "Nightfall (no record)", "Reaper Adept Left (no record)", "Reaper Adept Minor (no record)", "Reaper Grandmaster Left (no record)", "Reaper Grandmaster Minor (no record)", "Reaper Master Left (no record)", "Reaper Master Minor (no record)", "Reaper's Shroud (no record)", "Reaper's Touch (no record)", "Relic of the Thief (no record)", "Rending Claws (no record)", "Signet of Vampirism (no record)", "Soul Reaping Adept Left (no record)", "Soul Reaping Adept Minor (no record)", "Soul Reaping Grandmaster Left (no record)", "Soul Reaping Gr
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1166 filtered out; finished in 0.01s
```

Disabled again after the edit with `python docs/audit/disable_and_run.py coverage` (the `executed_from_facts` skip in `engine::active_normalized_effects` gated off), 2026-09-08:

```
### coverage
file: crates/optimizer/src/engine.rs
disabled: if executed_from_facts {
            continue;
        }
test: coverage_line_never_names_executed_weapon_skills
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:5099:9:
executed weapon skills leave the coverage line: ["\"Chilled to the Bone!\" (no record)", "\"You Are All Weaklings!\" (no record)", "Death Spiral (no record)", "Death's Charge (no record)", "Dusk Strike (no record)", "Ghastly Claws (no record)", "Grasping Darkness (no record)", "Gravedigger (no record)", "Infusing Terror (no record)", "Life Rend (no record)", "Nightfall (no record)", "Reaper Adept Left (no record)", "Reaper Adept Minor (no record)", "Reaper Grandmaster Left (no record)", "Reaper Grandmaster Minor (no record)", "Reaper Master Left (no record)", "Reaper Master Minor (no record)", "Reaper's Shroud (no record)", "Reaper's Touch (no record)", "Relic of the Thief (no record)", "Ren
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
error: test failed, to rerun pass `-p gw2-optimizer --lib`
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1169 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical
```

With the mechanism on: the fixture's line drops every weapon skill and every fixture trait whose facts the parser consumed; `coverage_entry_carries_its_class` renders a `NeedsMechanic("minions")` record as `Flesh of the Master (needs: minions)`; `nothing_skipped_is_verified` gets an empty list and no coverage reason; `pve_trait_fact_consumption_set_unchanged` pins the consumed set (empty for the synthetic fixture traits; a percent-fact trait is consumed, a bare one is not).

## shroud_enter

### necro_shroud_enter_fires_once_at_entry

Before the entry site existed (loader still refused `OnShroudEnter` as `no firing site`; `309d769` plus the Phase 4 test, run 2026-09-08):

```
thread 'rotation::wvw_timeline::necro_experiments::necro_shroud_enter_fires_once_at_entry' (791960) panicked at crates\optimizer\src\rotation\wvw_timeline.rs:6785:9:
assertion `left == right` failed: the entry record fires once at the entry; trace: [TraceEvent { t_ms: 0, kind: ProcUnmodeled, source: "Speed of Shadows (on-shroud-enter)", detail: "no firing site" }, TraceEvent { t_ms: 400, kind: HitLanded, source: "Gravedigger", detail: "1120.5" }, TraceEvent { t_ms: 750, kind: HitLanded, source: "Gravedigger", detail: "1120.5" }, TraceEvent { t_ms: 750, kind: LifeForceGained, source: "Gravedigger", detail: "8% → 8%" }, TraceEvent { t_ms: 1250, kind: HitLanded, source: "Death Spiral", detail: "342.4" }, TraceEvent { t_ms: 1550, kind: HitLanded, source: "Death Spiral", detail: "342.4" }, TraceEvent { t_ms: 1800, kind: HitLanded, source: "Death Spiral", de
  left: 0
 right: 1
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1176 filtered out; finished in 0.01s
```

Disabled again after the edit with `python docs/audit/disable_and_run.py shroud_enter` (the `OnShroudEnter` call at the end of `enter_shroud` replaced by a no-op), 2026-09-08:

```
### shroud_enter
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: self.trigger_procs(TriggerRule::OnShroudEnter, Some(skill_id), false, 1.0);
test: necro_shroud_enter_fires_once_at_entry
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:6802:9:
assertion `left == right` failed: the entry record fires once at the entry; trace: [TraceEvent { t_ms: 400, kind: HitLanded, source: "Gravedigger", detail: "1120.5" }, TraceEvent { t_ms: 750, kind: HitLanded, source: "Gravedigger", detail: "1120.5" }, TraceEvent { t_ms: 750, kind: LifeForceGained, source: "Gravedigger", detail: "8% → 8%" }, TraceEvent { t_ms: 1250, kind: HitLanded, source: "Death Spiral", detail: "342.4" }, TraceEvent { t_ms: 1550, kind: HitLanded, source: "Death Spiral", detail: "342.4" }, TraceEvent { t_ms: 1800, kind: HitLanded, source: "Death Spiral", detail: "342.4" }, 
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1176 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical
```

With the site on: one `TraitFired` at the `ShroudEntered` tick, `AppliesBoon ×1.00 at entry`, Swiftness on the player, `trait_fire_counts["Speed of Shadows"] == 1`, the trait off the coverage line. `necro_shroud_exit_fires_for_every_why` (exit skill, drain, damage: one fire each, `at exit (exit skill)` / `at exit (life force 0)`), `necro_in_shroud_bonus_active_only_inside` (`×1.15 crit damage` bracketed by `ShroudBonusActive`/`Ended` at the entry and exit ticks; Gravedigger unchanged, shroud strikes higher), `necro_desert_shroud_is_the_scourge_entry`, `necro_removed_trait_changes_results`, `necro_results_repeat_identically` and `necro_shroud_trigger_without_shroud_floor_never_fires` (`Speed of Shadows (shroud never entered)`, class `NoFiringSite`) pass; `reaper_*` unchanged (1 172 passed).

## prereq

### necro_chilled_prerequisite_gates_chilling_nova

Before `prerequisite_holds` read the foe's conditions (`2307f2c` plus the Phase 5 tests): the record was refused on every crit with `foe prerequisite not evaluated` and never fired. Disabled again after the edit with `python docs/audit/disable_and_run.py prereq` (the `foe not Chilled` refusal gated off, so the record fires from the first crit at 400 ms), 2026-09-08:

```
### prereq
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: if !carried {
                return Err(format!("foe not {condition}"));
test: necro_chilled_prerequisite_gates_chilling_nova
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:6973:9:
the pre-chill crits are refused with the reason: [TraceEvent { t_ms: 400, kind: HitLanded, source: "Gravedigger", detail: "1120.5" }, TraceEvent { t_ms: 400, kind: ProcFired, source: "Chilling Nova", detail: "AppliesCondition ×0.61" }, TraceEvent { t_ms: 400, kind: TraitFired, source: "Chilling Nova", detail: "AppliesCondition ×0.61" }, TraceEvent { t_ms: 750, kind: HitLanded, source: "Gravedigger", detail: "1120.5" }, TraceEvent { t_ms: 750, kind: ProcFired, source: "Chilling Nova", detail: "
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
error: test failed, to rerun pass `-p gw2-optimizer --lib`
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1185 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical
```

With the check on: `ProcSkippedPrerequisite "foe not Chilled"` at 400, 750 and 2 000 ms (Grasping Darkness lands its strike before its chill resolves), `TraitFired AppliesCondition ×0.61` at 2 500 and 2 800 ms from Death Spiral's crits, refused again from 7 100 ms once the chill has expired; the unchilled opener never fires.

## scope

### necro_shout_scope_fires_on_shouts_only

Before `scope_admits` knew categories, a trait `OnSkillUse` record was refused by the loader as `(on-skill-use)` (Sprint 2 rule: skill-owned only). Disabled again after the edit with `python docs/audit/disable_and_run.py scope` (the `Category` arm forced false), 2026-09-08:

```
### scope
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: crate::data::normalized_effects::TriggerScope::Category(category) => skill_id
test: necro_shout_scope_fires_on_shouts_only
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:7022:9:
assertion `left == right` failed: one fire inside the 30 s cooldown: [TraceEvent { t_ms: 1100, kind: HitLanded, source: "Gravedigger", detail: "1163.5" }, TraceEvent { t_ms: 1450, kind: HitLanded, source: "Gravedigger", detail: "1163.5" }, TraceEvent { t_ms: 1450, kind: LifeForceGained, source: "Gravedigger", detail: "8% → 8%" }, TraceEvent { t_ms: 3350, kind: ShroudRefused, source: "Reaper's Shroud", detail: "Reaper's Shroud needs 10% life force, had 8%" }, TraceEvent { t_ms: 3450, kind: HitL
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1185 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical
```

With the arm on: one `TraitFired` on the first shout, `ProcSkippedIcd` on the second inside the 30 s cooldown, nothing on Gravedigger, nothing when the skills carry no `Shout` category.

## population

### population_havoc_credits_five_or_cap

Before the counting existed the report had no ally totals (the test could not compile); the record landed on the player alone. Disabled again after the edit with `python docs/audit/disable_and_run.py population` (`ally_fan_out` forced to 1), 2026-09-08:

```
### population
file: crates/optimizer/src/rotation/wvw_timeline.rs
disabled: let applied = 1 + (n.max(1) - 1).min(self.population.allies);
test: population_havoc_credits_five_or_cap
panicked at crates\optimizer\src\rotation\wvw_timeline.rs:7551:9:
four allies × 1 stack × 10 s: got 0
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
error: test failed, to rerun pass `-p gw2-optimizer --lib`
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1193 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p gw2-optimizer --lib`
restored: byte-identical
```

With the counting on: a five-target ally boon record in a Party fight credits `4 × stacks × duration` (`PopulationApplied "5 of allies (5)"`); Solo credits nothing beyond the player and the one foe; Squad caps at the record's five (four allies, four secondary foes), not nine; a five-target Gravedigger's cleave is `4 × the primary strike` and `total_damage = solo total + cleave_damage`; a skill fact's `Number of Targets` fills `RotationSkill.targets` and reaches the same path.
