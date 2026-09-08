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

## prereq

## scope

## population
