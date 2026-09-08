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

## shroud_enter

## prereq

## scope

## population
