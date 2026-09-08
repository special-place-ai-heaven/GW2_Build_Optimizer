# Tasks: Trait triggers are the build (007)

**Input**: Design documents from `specs/007-trait-triggers/` (plan.md, spec.md, research.md R1–R10, data-model.md, contracts/, quickstart.md)

**Tests**: Required. FR-010 makes every behaviour change land behind an experiment seen failing under `docs/audit/disable_and_run.py`. Each mechanism task below is preceded by its test task; the test is written, run, seen failing, then the mechanism lands and the test passes.

**Organization**: Phases follow the plan's execution order. US4 (coverage truth) runs before US1 because US1's independent test ("the trait leaves the Not simulated line") is only meaningful once executed sources are dropped from that line.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different files, no dependency on an unfinished task)
- **[Story]**: US1 shroud triggers, US2 prerequisites/scopes, US3 every trait executed or explained, US4 coverage line

## Path Conventions

Rust workspace. Optimizer crate at `crates/optimizer/src/`; data at `data/`; evidence under `docs/audit/`. Test filters containing `::` need `MSYS_NO_PATHCONV=1` in Git Bash.

---

## Phase 1: Setup

**Purpose**: discovery and the evidence scaffolding every later phase writes into.

- [X] T001 Call SymForge `status`, then `find_references` for `TriggerRule`, `TriggerScope`, `EffectCategory`, `NormalizedEffect`, `ProcSpec`, `ConditionalKind`, `RotationSkill`, `WvwCombatReport`, `search_rank`, `active_normalized_effects`; record the consumer list per symbol at the top of `docs/audit/sprint3-failures.md` (new file, "Consumers touched" section)
- [X] T002 [P] Add a `## Sprint 3 controls` block to `docs/audit/disable_and_run.py` with empty control entries named `shroud_enter`, `prereq`, `scope`, `population`, `coverage` (each entry: the source line to comment out, the test filter, the expected failure fragment); wire them into `--list`
- [X] T003 [P] Add section 9 heading "Sprint 3: trait triggers" with subsections 9.1–9.6 (Coverage truth, Shroud, Prerequisites and scopes, Population, Necromancer catalogue, Timing) to `docs/simulator-connection-audit.md`
- [X] T004 [P] Record the Sprint 2 timing baseline: run `cargo test -p gw2-optimizer --lib 2>&1 | tail -3` three times and write the wall-clock values under 9.6 in `docs/simulator-connection-audit.md`

---

## Phase 2: Foundational (schema)

**Purpose**: the record schema every story reads. All additions optional and `serde(default)`; the fourteen records in `data/normalized_effects/2026-01-13/wvw.json` must load unchanged.

- [X] T005 Add `TriggerRule::{OnShroudEnter, OnShroudExit, OnConditionApplied, OnBoonApplied, OnBoonStripped, Periodic}` in `crates/optimizer/src/data/normalized_effects.rs`; extend every exhaustive match found by T001 (`same_trigger`, `trigger_label`, loader arms) with a compile-clean arm that keeps unknown-to-runtime kinds on the coverage path
- [X] T006 Add `TriggerScope::{Category(String), Slot(String), Status(String)}` in `crates/optimizer/src/data/normalized_effects.rs` with serde forms `{"Category":"Shout"}`, `{"Slot":"Elite"}`, `{"Status":"Fear"}` per `contracts/normalized-effect-schema.md`
- [X] T007 Add `EffectCategory::{GainsLifeForce, Heal}` and the `healing_power_coefficient: Option<FactualValue<f64>>` field in `crates/optimizer/src/data/normalized_effects.rs`
- [X] T008 Add `Prerequisite { foe_condition: Option<String>, in_shroud: Option<bool>, foe_health: Option<HealthThreshold> }`, `ScaleBy::ConditionsRemoved`, `CoverageBlock { class: CoverageClass, mechanic: Option<String> }` and the `prerequisite`, `scale_by`, `coverage` fields on `NormalizedEffect` in `crates/optimizer/src/data/normalized_effects.rs`
- [X] T009 Add validation in `crates/optimizer/src/data/normalized_effects.rs` (`load_normalized_effects` / the existing validate fn): empty `prerequisite` rejected; `source_type: Trait` + `OnSkillUse` requires scope ≠ `Any`; `Periodic` requires `internal_cooldown`; `coverage` block forbids `status_operation`, `inner_category`, `prerequisite` and a resolved `value`; `NeedsMechanic` requires `mechanic`, other classes forbid it; `healing_power_coefficient` only with `Heal`; `scale_by` only with `GainsLifeForce` or `Heal`
- [X] T010 Add round-trip and validation tests in `crates/optimizer/src/data/normalized_effects.rs` (`#[cfg(test)]`): one record per new trigger kind and scope, one per validation rule (each rejected case asserts the error text), and `sprint2_records_still_load` asserting all fourteen existing records load
- [X] T011 Add `ReasonClass { NoRecord, PassiveNoEffect, NeedsMechanic(String), UnresolvedValue, NoFiringSite }` with `suffix()` returning the strings in `contracts/wvw-report.md` and `CoverageEntry { name, class, detail }` in `crates/optimizer/src/data/quality.rs`; unit test the five suffixes
- [X] T012 Commit: `schema: trigger kinds, scopes, prerequisite, coverage block (007 step 1)` with the session footers

**Checkpoint**: `cargo test -p gw2-optimizer --lib normalized_effects` green; fourteen records load.

---

## Phase 3: User Story 4 - Coverage line says what was skipped and why (Priority: P4, moved first)

**Goal**: the "Not simulated" line drops every source executed from facts and classifies the rest.

**Independent Test**: score the Reaper fixture; no executed weapon skill appears; each remaining name carries a suffix from the fixed set; an empty list yields `DataQuality::Verified`.

### Tests (seen-failing first)

- [X] T013 [US4] Write `coverage_line_never_names_executed_weapon_skills` in `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`): score the fixture, assert "Gravedigger" is absent from `unmodeled_sources`; run it, paste the failure into `docs/audit/sprint3-failures.md` under `coverage`
- [X] T014 [P] [US4] Write `coverage_entry_carries_its_class` and `nothing_skipped_is_verified` in `crates/optimizer/src/rotation/wvw_timeline.rs`: a record with `coverage: NeedsMechanic("minions")` renders `(needs: minions)`; an input whose sources are all executed yields empty `coverage` and Verified

### Implementation

- [X] T015 [US4] Make the trait-fact walk in `crates/optimizer/src/combat.rs` (around the line-740 region found by T001) return the set of trait ids whose facts it consumed; no numeric change; keep the existing signature via a wrapper if callers are many
- [X] T016 [US4] Record in `crates/optimizer/src/rotation/builder.rs` the set of skill ids for which at least one `SkillEffect` was produced (a field on the built rotation or a returned set) — no builder change needed: `RotationSkill.effects` already carries it; the engine reads `!effects.is_empty()`
- [X] T017 [US4] Rewrite the "(no record)" list in `engine::active_normalized_effects` (`crates/optimizer/src/engine.rs`): subtract the T015 and T016 sets; classify each remainder into a `CoverageEntry` (record with `coverage` block → its class; record with unresolved value → `UnresolvedValue`; record whose trigger has no runtime site → `NoFiringSite`; else `NoRecord`); render `unmodeled_sources` as `"{name} ({suffix})"`
- [X] T018 [US4] Add `coverage: Vec<CoverageEntry>` to `WvwCombatReport` in `crates/optimizer/src/rotation/wvw_timeline.rs`, fill it from the engine's list, and make `DataQuality::Verified` iff it is empty in `crates/optimizer/src/data/quality.rs` (`coverage_reason`)
- [X] T019 [US4] Add a PvE pin `pve_trait_fact_consumption_set_unchanged` in `crates/optimizer/src/combat.rs` asserting the T015 set for the fixture build equals a hard-coded list of trait ids (guards double counting when records arrive)
- [X] T020 [US4] Fill the `coverage` control in `docs/audit/disable_and_run.py` (disable the T017 subtraction), run it, quote the failure in `docs/audit/sprint3-failures.md` and audit 9.1; commit `coverage: only skipped mechanics, each with its class (007 step 2)`

**Checkpoint**: T013, T014 green; PvE guard `pve_output_unchanged_by_conditional_tagging` green.

---

## Phase 4: User Story 1 - Shroud traits fire on enter, hold, exit (Priority: P1) 🎯 MVP

**Goal**: `OnShroudEnter`, `OnShroudExit` and in-shroud conditional records change the simulated fight at the shroud transitions.

**Independent Test**: run the Reaper opener with a Speed of Shadows-shaped record equipped and removed; the trace carries `TraitFired` at the entry instant; totals differ; the trait is absent from the coverage line.

### Tests (seen-failing first)

- [X] T021 [US1] Write `necro_shroud_enter_fires_once_at_entry` in a new `necro_experiments` module in `crates/optimizer/src/rotation/wvw_timeline.rs`: an `OnShroudEnter` `AppliesBoon` Swiftness record (test-local, not in `data/`) applied on the fixture opener produces exactly one `TraitFired` trace at the entry tick and Swiftness on the player; run, paste failure under `shroud_enter` in `docs/audit/sprint3-failures.md`
- [X] T022 [P] [US1] Write `necro_shroud_exit_fires_for_every_why` (exit by skill, by drain, by damage each fire once), `necro_in_shroud_bonus_active_only_inside` (a `Conditional` + `prerequisite.in_shroud` CritDamagePct record raises shroud-skill strikes and not out-of-shroud strikes; `ShroudBonusActive`/`Ended` traces bracket it), `necro_desert_shroud_is_the_scourge_entry` (Desert Shroud fires `OnShroudEnter`, Manifest Sand Shade never does), `necro_removed_trait_changes_results` and `necro_results_repeat_identically` in `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T023 [P] [US1] Write `necro_shroud_trigger_without_shroud_floor_never_fires` in `crates/optimizer/src/rotation/wvw_timeline.rs`: a build that cannot enter shroud leaves the record on the coverage line with the shroud-floor reason (spec edge case)

### Implementation

- [X] T024 [US1] Add `TraceKind::{TraitFired, ShroudBonusActive, ShroudBonusEnded, ProcSkippedPrerequisite, PopulationApplied}` in `crates/optimizer/src/rotation/wvw_timeline.rs` with the detail strings in `contracts/wvw-report.md`; emit `TraitFired` from `trigger_procs` when the proc's `source_type` is `Trait`; add `trait_fire_counts: BTreeMap<String, u32>` to `WvwCombatReport`
- [X] T025 [US1] Call `trigger_procs(TriggerRule::OnShroudEnter, Some(entry_skill), false, 1.0)` at the end of `enter_shroud` and `trigger_procs(TriggerRule::OnShroudExit, ...)` at the start of `exit_shroud(why)` (before the state drops, `why` in the trace) in `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T026 [US1] Add `ConditionalKind::InShroud` in `crates/optimizer/src/rotation/wvw_timeline.rs`; build it in the record→`ConditionalSpec` conversion when `trigger_rule: Conditional` and `prerequisite.in_shroud == Some(true)`; activate/deactivate in `update_conditionals` from `in_shroud.is_some()`, tracing `ShroudBonusActive`/`Ended`
- [X] T027 [US1] Add `prerequisite: Option<Prerequisite>` to `ProcSpec` and carry it through the record→`ProcSpec` conversion in `crates/optimizer/src/rotation/wvw_timeline.rs` (full evaluation lands in Phase 5; here only `in_shroud` is checked so the US1 tests can use it)
- [X] T028 [US1] Fill the `shroud_enter` control in `docs/audit/disable_and_run.py` (comment out the T025 enter call), run, quote the failure in `docs/audit/sprint3-failures.md` and audit 9.2; commit `shroud triggers: enter, exit, in-shroud bonus (007 step 3)`

**Checkpoint**: all `necro_shroud_*` and `necro_in_shroud_*` green; `reaper_results_repeat_identically` and `reaper_trace_fits_under_cap` green.

---

## Phase 5: User Story 2 - Prerequisites on the foe and trait-owned skill triggers (Priority: P2)

**Goal**: records fire only when their prerequisite holds; trait `OnSkillUse` fires on the named skill kind; status triggers, periodic ticks, life force and heal routes exist.

**Independent Test**: a Chilling Nova-shaped record fires on a chilled foe and not on an unchilled one, with `ProcSkippedPrerequisite` in the trace; a shout-scoped record fires on the shout only.

### Tests (seen-failing first)

- [X] T029 [US2] Write `necro_chilled_prerequisite_gates_chilling_nova` in `crates/optimizer/src/rotation/wvw_timeline.rs`: an `OnCrit` record with `prerequisite.foe_condition: "Chilled"`, `internal_cooldown` 3, fires on the fixture only after the opener's chill lands and never before; the pre-chill attempt traces `ProcSkippedPrerequisite("foe not Chilled")`; run, paste failure under `prereq`
- [X] T030 [P] [US2] Write `necro_shout_scope_fires_on_shouts_only` (trait `OnSkillUse` + `Category("Shout")` fires once per shout cast, honours cooldown, ignores other skills), `necro_slot_scope_fires_on_elite_only`, `necro_fear_applied_fires_dread` (`OnConditionApplied` + `Status("Fear")`), `necro_boon_applied_and_stripped_fire` (`OnBoonApplied` any; `OnBoonStripped` on the opener's corrupt), `necro_periodic_and_exit_life_force` (a `Periodic` `GainsLifeForce` record credits the pool at t=0 and every period; an `OnShroudExit` `GainsLifeForce` with `scale_by: ConditionsRemoved` credits 7 × removed), `necro_heal_route_uses_healing_power` (`Heal` 133 + 0.1 × healing power) in `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T031 [P] [US2] Write `necro_prerequisite_never_met_is_traced_not_listed` (a `foe_health` prerequisite on the open dummy never fires, ends with a `prerequisite never met` summary trace, and is absent from `coverage`) and `necro_long_cooldown_fires_once_and_traces_refusal` in `crates/optimizer/src/rotation/wvw_timeline.rs`; run, paste the scope failure under `scope`

### Implementation

- [X] T032 [US2] Add `categories: Vec<String>` and `slot_name: Option<String>` to `RotationSkill` in `crates/optimizer/src/rotation/mod.rs` (defaults empty/None so every constructor compiles) and fill them from `Skill.categories` / `Skill.slot` in `crates/optimizer/src/rotation/builder.rs`
- [X] T033 [US2] Extend `scope_admits` in `crates/optimizer/src/rotation/wvw_timeline.rs` for `Category`, `Slot` (against the T032 fields) and `Status` (against the status name passed by the three status sites); lift the `SourceType::Skill`-only guard on `OnSkillUse` when the record's scope is not `Any`
- [X] T034 [US2] Implement `prerequisite_holds(&Prerequisite) -> Result<(), &'static str>` in `crates/optimizer/src/rotation/wvw_timeline.rs`: `foe_condition` reads unexpired `outgoing_conditions`; `in_shroud` reads `self.in_shroud.is_some()`; `foe_health` reads `enemy_health / target_health` (never met when `target_health` is `None`); call it in `trigger_procs` before the mass step and in `update_conditionals`, tracing `ProcSkippedPrerequisite(reason)`; at fight end emit one `prerequisite never met` summary per proc that never held
- [X] T035 [US2] Add the helper `apply_outgoing_condition(name, stacks, duration, source)` in `crates/optimizer/src/rotation/wvw_timeline.rs` replacing the three inline `outgoing_conditions` pushes found by grep (skill fact, corrupt, record operation) plus the two WvW-only routings for non-damaging conditions published as buffs and for Fear/Taunt control; it pushes, then calls `trigger_procs(OnConditionApplied, ...)` with the status name for scope matching
- [X] T036 [US2] Add the `OnBoonApplied` call at the end of `apply_buff`, the `OnBoonStripped` call per boon removed in `remove_enemy_boons`, and the `Periodic` call each tick of `run` (period = `internal_cooldown`, `next_ready_ms` starts at 0) in `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T037 [US2] Route `EffectCategory::GainsLifeForce` to the life force ledger (existing cap and `LifeForceGained` trace) and `EffectCategory::Heal` to `heal()` with `value + healing_power_coefficient × healing_power` in `trigger_procs`; add `scale_by` and `healing_power_coefficient` to `ProcSpec`; multiply by the same firing's `RemovesCondition` count for `ScaleBy::ConditionsRemoved` in `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T038 [US2] Fill the `prereq` and `scope` controls in `docs/audit/disable_and_run.py` (disable T034's check; disable T033's category arm), run both, quote failures in `docs/audit/sprint3-failures.md` and audit 9.3; commit `prerequisites, trait skill-use scopes, status and periodic sites (007 step 4)`

**Checkpoint**: every `necro_*` test green; `reaper_*` unchanged; `cargo test -p gw2-optimizer --lib` timing within 10 % of T004.

---

## Phase 6: Fight population (FR-003a, serves US3 SC-003)

**Goal**: every target-facing effect is credited to every foe and ally present for the scale, within the record's cap; support builds rank apart on an ally-facing trait.

**Independent Test**: a five-target boon record in a Havoc scenario credits the player plus four allies; in Roam only the player; in Cloud the record's `target_count` caps it.

### Tests (seen-failing first)

- [X] T039 Write `population_havoc_credits_five_or_cap` in `crates/optimizer/src/rotation/wvw_timeline.rs`: an `AppliesBoon` record with `target_count` 5, `target_side: Ally`, in a `CombatTier::Party` input yields `ally_boon_stack_seconds == 4 × stacks × duration`; run, paste failure under `population`
- [X] T040 [P] Write `population_roam_credits_one` (Solo tier: ally totals 0, cleave totals 0), `population_cloud_caps_at_record` (Squad tier with `target_count` 5 credits 4 allies, not 9; a foe-facing condition with `target_count` 5 credits 4 secondary foes), `population_cleave_damage_joins_totals` (`total_damage` and `protected_damage` include `cleave_damage`), `population_skill_fact_targets_feed_the_same_path` (a skill fact with `Number of Targets` 5 credits cleave) in `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T041 [P] Write `support_builds_rank_apart_on_ally_trait` in `crates/optimizer/src/referee.rs`: two `CombatKind::Support` WvW builds identical except one ally-facing boon record rank with the record above; and `damage_builds_unchanged_by_ally_slot`: two Damage builds' order is unchanged by the same record

### Implementation

- [X] T042 [P] Create `data/formulas/fight_population.json` with the `source` note and tiers Solo {1,0}, Party {5,4}, Squad {10,9} per `data-model.md`
- [X] T043 [P] Create `crates/optimizer/src/data/fight_population.rs` embedding the JSON like the other formula files, `FightPopulation { foes: u32, allies: u32 }`, `FightPopulation::for_tier(CombatTier)`, validation `foes ≥ 1`; register in `crates/optimizer/src/data/mod.rs`; unit test the three tiers
- [X] T044 Add `population: FightPopulation` to `WvwTimelineInput` and fill it from `ScenarioSpec.combat_tier` at the construction site in `crates/optimizer/src/engine.rs`; every other constructor (tests, fixtures) gets the Solo value
- [X] T045 Add `targets: u32` (default 1) to `SkillEffect::{ApplyBuff, ApplyCondition, Healing, RemovesCondition}` in `crates/optimizer/src/rotation/mod.rs`, read `Number of Targets` in `crates/optimizer/src/rotation/builder.rs` — landed as one `RotationSkill.targets` field (the fact is per skill, not per effect); the builder fills it from the `Number of Targets` fact
- [X] T046 Implement the counting in `apply_operation` and `apply_skill_effect` in `crates/optimizer/src/rotation/wvw_timeline.rs`: foe-facing `applied_to = min(n, foes)`, primary gets the state, `(applied_to−1) × amount` into `cleave_damage` / `cleave_condition_stack_seconds`; ally-facing `applied_to = 1 + min(n−1, allies)`, player gets the buff, `(applied_to−1) × stacks × duration` into `ally_boon_stack_seconds`, heals into `ally_healing`, cleanses into `ally_cleanses`; trace `PopulationApplied`; add the five totals to `WvwCombatReport`; add `cleave_damage` to `total_damage` and `protected_damage`
- [X] T047 Change the Support/Commander/Staller output slot in `referee::search_rank` (`crates/optimizer/src/referee.rs`) from `sustain_margin.max(0.0)` to `sustain_margin.max(0.0) + ally_boon_stack_seconds / 1000.0`; no other slot, gate or constant
- [X] T048 Fill the `population` control in `docs/audit/disable_and_run.py` (force `applied_to = 1`), run, quote in `docs/audit/sprint3-failures.md` and audit 9.4; run `pve_output_unchanged_by_conditional_tagging` and `cargo test -p gw2-optimizer --test scoring_regression`; commit `fight population: counted foes and allies per scale (007 step 5)`

**Checkpoint**: population tests and ranking tests green; PvE/PvP pins unchanged to the last digit.

---

## Phase 7: User Story 3 - Necromancer: every trait executed or explained (Priority: P3)

**Goal**: 111 Necromancer trait states in data; the cached Reaper build's "Not simulated" trait entries fall from nine to at most two; the audit table and the wiki-number check exist.

**Independent Test**: `reaper_cached_build_traits_are_simulated` (`--ignored`) prints nine traits with at most two on the line, each classed; `trait_coverage_audit_lists_every_trait` writes 999 rows with `NoRecord 0` for Necromancer.

### Fixtures

- [X] T049 [P] [US3] Create `crates/optimizer/src/rotation/necro_published.rs`: one GuildJen WvW Necromancer build read from `<addons_dir>/gw2_build_optimizer/benchmarks/guildjen_necromancer_wvw.json` at authoring time; hard-code its spec, trait, skill, gear, rune, sigil and relic ids as a `ValidatedBuild` fixture with the page URL and scrape date in a doc comment; register in `crates/optimizer/src/rotation/mod.rs` under `#[cfg(test)]`
- [X] T050 [P] [US3] Add opener variants to `crates/optimizer/src/rotation/reaper_fixture.rs` that reach every trigger kind the Necromancer records use: a shout, a Fear application, a corrupt (boon strip), a chill before a crit, a full shroud enter/exit, a Desert Shroud variant for Scourge, a Harbinger Shroud variant
- [X] T051 [P] [US3] Extend `records_this_sprint_carry_read_dates` in `crates/optimizer/src/data/normalized_effects.rs`: every record with a Sprint 3 field or trigger has `source` matching `https://wiki.guildwars2.com/... (read YYYY-MM-DD...)`

### Audit and wiki check (tests)

- [X] T052 [US3] Write `#[ignore]` `trait_coverage_audit_lists_every_trait` in `crates/optimizer/src/referee.rs`: read `cache/traits.json` and `cache/specializations.json` via `gw2_api::dev_config::cache_dir()` (unwrap the `{build, fetched_at, data}` envelope), derive each trait's state (facts / record / facts+record / class / NoRecord) from the T015 set, the records and the coverage blocks, write `docs/audit/trait-coverage.md` in the format of `contracts/trait-coverage-audit.md`, fail on any trait without a row or on `NoRecord > 0` for a profession in the shipped list (`["Necromancer"]` now)
- [X] T053 [P] [US3] Write `#[ignore]` `records_match_their_wiki_pages` in `crates/optimizer/src/referee.rs`: read `crawl_url` from `dev.cfg` (skip with a printed reason when absent), fetch each distinct record `source` URL once through the crawl4ai `md` endpoint with reqwest blocking, pick the WvW column when present, assert every factual number listed in `contracts/trait-coverage-audit.md` step 3 appears as a number token, print `ok`/`MISMATCH` per record; commit no page text
- [X] T054 [P] [US3] Write `#[ignore]` `reaper_cached_build_traits_are_simulated` in `crates/optimizer/src/referee.rs`: load the player's cached Reaper build from `char_*_buildtabs.json`, score it in WvW, print the nine traits with their state, assert at most two are in `coverage` and each has a class (SC-001)

### Records (core lines: Spite, Curses, Death Magic, Blood Magic, Soul Reaping)

- [X] T055 [US3] Read the 15 Spite trait pages on the wiki via crawl4ai `md`, write their records or `coverage` blocks into `data/normalized_effects/2026-01-13/wvw.json` with URL and read date (worked examples: Spiteful Fortitude 829 `OnHit` + `foe_health`, Dread 919 `OnConditionApplied` Fear); add one `necro_*` firing test per executable record in `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T056 [US3] Same for the 15 Curses traits in `data/normalized_effects/2026-01-13/wvw.json` and `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T057 [US3] Same for the 15 Death Magic traits (minion traits take `coverage: NeedsMechanic("minions")`) in `data/normalized_effects/2026-01-13/wvw.json` and `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T058 [US3] Same for the 15 Blood Magic traits (Unholy Martyr 1692 exit half per contract; ally-state half `NeedsMechanic("ally state")`) in `data/normalized_effects/2026-01-13/wvw.json` and `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T059 [US3] Same for the 15 Soul Reaping traits (Speed of Shadows 888, Soul Barbs 894, Death Perception 893 per contract) in `data/normalized_effects/2026-01-13/wvw.json` and `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T060 [US3] Run T052 (`--ignored`), confirm core-line rows have `NoRecord 0`; run T053 on the new records and paste the summary line into audit 9.5; commit `necromancer core lines: 75 trait states (007 step 6a)` — landed with 6b in one commit (one authoring script wrote the file); the audit's summary lines are in 9.5

### Records (elite lines: Reaper, Scourge, Harbinger, Ritualist)

- [X] T061 [US3] Read and record the 9 Reaper traits (Chilling Nova 2020, Cold Shoulder, Chilling Victory, Reaper's Onslaught in-shroud, Blighter's Boon 1932 two records) in `data/normalized_effects/2026-01-13/wvw.json` with `necro_*` tests in `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T062 [US3] Same for the 9 Scourge traits (shade traits `NeedsMechanic("shades")` unless the wiki states a Desert Shroud trigger) in `data/normalized_effects/2026-01-13/wvw.json` and `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T063 [US3] Same for the 9 Harbinger traits (blight traits `NeedsMechanic("blight")`; shroud triggers execute) in `data/normalized_effects/2026-01-13/wvw.json` and `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T064 [US3] Same for the 9 Ritualist traits in `data/normalized_effects/2026-01-13/wvw.json` and `crates/optimizer/src/rotation/wvw_timeline.rs`
- [X] T065 [US3] Run T052, T053 and T054 (`--ignored`); regenerate `docs/audit/trait-coverage.md`; assert Necromancer `NoRecord 0` (met) and SC-001 at most two (NOT met: 3, all Death's Carapace; the test keeps the assertion and fails until carapace lands); paste both summaries into audit 9.5; commit `necromancer elite lines: 36 trait states, audit table (007 step 6b)`
- [X] T066 [US3] Score `necro_published` (T049) and the same build with its recorded trigger traits swapped for their neighbours in `crates/optimizer/src/referee.rs` test `necro_published_ranks_by_its_triggers`: every listed pair ranks in the direction the audit table records (SC-003) — the key moves; its direction is the simulation's and is recorded in 9.5

**Checkpoint**: US3 independent test passes for Necromancer; every `necro_*` and `reaper_*` test green.

---

## Phase 8: Evidence, gates and release of increment 1

**Purpose**: the automated acceptance from `quickstart.md`, then the release the spec asks for.

- [ ] T067 Run `python docs/audit/disable_and_run.py shroud_enter prereq scope population coverage`; confirm each fails with the quoted block and the tree is byte-identical afterwards; note the run in audit 9.1–9.4
- [ ] T068 [P] Run `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib wvw_timeline::reaper_experiments::reaper_results_repeat_identically` and `..::reaper_trace_fits_under_cap`; run the lib harness three times and write the timing beside the T004 baseline in audit 9.6 (SC-006: within 10 %)
- [ ] T069 [P] Run `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib pve_output_unchanged` and `cargo test -p gw2-optimizer --test scoring_regression`; confirm identical to the last digit (SC-004)
- [ ] T070 Sweep the diff for machine paths, `poslj`, `scratchpad`, `DEBUG`, `mock`, fixture ids inside `data/`, and any edit under `crates/optimizer/src/prompts.rs`, `crates/optimizer/src/llm/`, `crates/optimizer/examples/choya_live.rs`; fix anything found
- [ ] T071 Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --release`
- [ ] T072 Add the CHANGELOG entry under Unreleased naming Necromancer and the six trigger kinds; bump the patch version in the workspace `Cargo.toml` files; commit `release: x.y.z` with the session footers
- [ ] T073 Push `007-trait-triggers`, open the PR against `main` (body ends with the Claude Code footer), `gh release create` with the DLL and `SHA256SUMS.txt`; stop: the user merges

---

## Phase 9: Increments 2–9 (US3, FR-011, FR-012; one branch, PR and version each)

**Purpose**: repeat Phase 7 and Phase 8 for the remaining eight professions on the shipped mechanism.

- [ ] T074 [US3] Determine the profession order: read the `profession` field of each entry in `<addons_dir>/gw2_build_optimizer/cache/characters.json` (fall back to alphabetical if the field is absent), then the remaining professions alphabetically; write the order at the top of audit section 10 in `docs/simulator-connection-audit.md`
- [ ] T075 [US3] For profession 2: branch `007-trait-triggers-<prof>` off the increment-1 tip; create `crates/optimizer/src/rotation/<prof>_fixture.rs` (opener reaching every trigger kind the profession's traits use + GuildJen WvW published build with URL and scrape date); read the 111 trait pages; write records and `coverage` blocks into `data/normalized_effects/2026-01-13/wvw.json`; add any new `NeedsMechanic` name to the contract list; add `<prof>_experiments` tests in `crates/optimizer/src/rotation/wvw_timeline.rs`; add the profession to the shipped list in T052; run T052 and T053; ranking-direction test; run Phase 8 gates T067–T073
- [ ] T076 [US3] Profession 3: same as T075
- [ ] T077 [US3] Profession 4: same as T075
- [ ] T078 [US3] Profession 5: same as T075
- [ ] T079 [US3] Profession 6: same as T075
- [ ] T080 [US3] Profession 7: same as T075
- [ ] T081 [US3] Profession 8: same as T075
- [ ] T082 [US3] Profession 9: same as T075; final `docs/audit/trait-coverage.md` shows `NoRecord 0` for all nine and 999 rows (SC-002)

---

## Dependencies

- Phase 1 → Phase 2 → Phase 3 (US4) → Phase 4 (US1) → Phase 5 (US2) → Phase 6 (population) → Phase 7 (US3 Necromancer) → Phase 8 → Phase 9.
- US4 precedes US1 because US1's independent test asserts the trait leaves the coverage line (needs T017).
- US2 depends on US1 only for `ProcSpec.prerequisite` (T027) and `TraitFired` (T024).
- Phase 6 depends on US2's `apply_outgoing_condition` helper (T035) so cleave conditions pass through one site.
- Phase 7 depends on Phases 2–6 complete: records use every trigger kind, prerequisite and category.
- Phase 9 depends on Phase 8's release; each increment on the previous increment's tip.

## Parallel Execution Examples

- Phase 1: T002, T003, T004 together after T001.
- Phase 3: T013 and T014 together; T015 and T016 together (different files).
- Phase 4: T022 and T023 together after T021 is seen failing.
- Phase 5: T030 and T031 together after T029.
- Phase 6: T040 and T041 together; T042 and T043 together.
- Phase 7: T049, T050, T051 together; T053 and T054 together; record tasks T055–T059 are sequential (same JSON file) but each line's wiki reads can be batched with one crawl4ai `crawl` call.
- Phase 8: T068 and T069 together.

## Implementation Strategy

- **MVP** = Phases 1–4: schema, truthful coverage line, shroud triggers proven on the Reaper opener. Reviewable on its own; no release yet.
- **Increment 1** = Phases 5–8: full Necromancer catalogue and release, the first profession where "optimized" is judged on trait triggers.
- **Increments 2–9** = Phase 9, one PR each, same shape, no new mechanism expected.
- Every mechanism task has a control in `docs/audit/disable_and_run.py`; a step is not committed until its failure is quoted in `docs/audit/sprint3-failures.md`.
