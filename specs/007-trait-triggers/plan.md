# Implementation Plan: Trait triggers are the build (007)

**Branch**: `007-trait-triggers` (off `255371f`) | **Date**: 2026-09-08 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/007-trait-triggers/spec.md`; Sprint 2 audit `docs/simulator-connection-audit.md` section 8 (T042 finding, CONN-01-06); research in [research.md](research.md).

## Summary

Make trait trigger effects change the simulated WvW fight. Five new trigger kinds with one firing site each (shroud enter, shroud exit, condition applied, boon applied, boon stripped, plus a periodic tick), one `prerequisite` block (foe condition, in shroud, foe health), trait-owned skill-use filtered by skill category or slot, two effect kinds the Necromancer needs (`GainsLifeForce`, `Heal`), and a counted fight population per scale so every ally- and foe-facing effect lands on everyone it is meant for. The coverage line stops naming executed weapon skills and classifies what it does name with a fixed reason set carried in the records. The Necromancer's 111 traits are the first increment: every one ends up executed from facts, executed from a dated wiki record, or classified; the other eight professions follow one increment each on the same mechanism with a published GuildJen WvW build and a hand-authored opener per profession. Acceptance is automated: seen-failing experiments, ranking-direction tests, the trait-coverage audit table, the wiki-number check and the PvE/PvP pins. No screen changes.

## Technical Context

**Language/Version**: Rust 2021, stable toolchain; Windows MSVC Nexus cdylib. **Primary Dependencies**: none new; serde for the schema, the in-tree xorshift for trials, crawl4ai (already a dev tool) only inside an `#[ignore]` test that reads its URL from `dev.cfg`. **Storage**: JSON data files under `data/` (records, `fight_population.json`), Markdown audit output under `docs/audit/`. **Testing**: `cargo test` unit modules beside the code (`reaper_experiments` pattern, a `necro_experiments` module, one `<profession>_experiments` per later increment); two `#[ignore]` cache- and network-backed tests. **Target Platform**: the optimizer crate only; the addon consumes the same `RefereeReport` and coverage reasons it renders today. **Project Type**: Rust workspace library change. **Performance Goals**: optimizer lib harness within 10 percent of the Sprint 2 baseline (16.6–19.0 s); search never runs trials. **Constraints**: FR-009 PvE/PvP byte-identical; no calibrated constant, gate or threshold moves; owned files untouched; fixture values never in `data/`; every behaviour change behind a seen-failing test. **Scale/Scope**: 999 traits across nine professions, 111 per increment; Necromancer increment in this plan's execution order, eight repeats of the same shape.

## Constitution Check

The constitution file is an unfilled template and supplies no ratified gates. Project gates applied before research and re-checked after design:

- No secrets or account data in artifacts: the cache-backed tests print names only; `dev.cfg` is never read in a way that prints its contents.
- SymForge for discovery and impact: `find_references` before each changed type (`TriggerRule`, `TriggerScope`, `EffectCategory`, `NormalizedEffect`, `ProcSpec`, `ConditionalKind`, `RotationSkill`, `WvwCombatReport`, `search_rank`, `active_normalized_effects`).
- Every mechanism preceded by a failing behavioural test seen failing under a scripted disable and quoted in the audit (section 9).
- No scoring constant, gate or normalisation moves. The one rank-key change (ally boon output for support kinds) is ordering, pinned by a ranking test, and never touches PvE/PvP.
- PvE and PvP results byte-identical before and after (`pve_output_unchanged_by_conditional_tagging` plus a new pin on the set of trait facts the parser consumes).
- Fixture values never enter `data/`; records carry wiki URL and read date.
- Owned files untouched; commits local until the user says otherwise; the user merges PRs. No in-game test is asked for this sprint (spec clarification 2); a release per profession increment follows the automated gates.

Post-design re-check: every schema addition is optional and `serde(default)`, so all fourteen existing records load unchanged and `test_serde_roundtrip_*` stay green. `RotationSkill.categories` defaults empty, so every constructor compiles. The fight population counts do not add state to the tick loop. No violations.

## Project Structure

### Documentation (this feature)

```text
specs/007-trait-triggers/
├── plan.md                              # this file
├── spec.md
├── research.md                          # R1–R10
├── data-model.md
├── contracts/
│   ├── normalized-effect-schema.md      # trigger kinds, prerequisite, scope, categories, coverage block
│   ├── wvw-report.md                    # population totals, trace kinds, rank-key change
│   └── trait-coverage-audit.md          # audit table format and the wiki-number check
├── quickstart.md
└── tasks.md                             # /speckit-tasks output
docs/simulator-connection-audit.md       # section 9: Sprint 3 experiments and seen-failing evidence
docs/audit/disable_and_run.py            # new control entries
docs/audit/sprint3-failures.md           # quoted failures
docs/audit/trait-coverage.md             # generated: every trait's coverage state
```

### Source code (repository root)

```text
crates/optimizer/src/data/normalized_effects.rs   # TriggerRule +6, TriggerScope +3, EffectCategory +2, Prerequisite,
                                                  #   ScaleBy, CoverageBlock, healing_power_coefficient; validation rules
crates/optimizer/src/data/fight_population.rs     # loader for data/formulas/fight_population.json
crates/optimizer/src/data/quality.rs              # ReasonClass and its line suffixes
crates/optimizer/src/rotation/wvw_timeline.rs     # firing sites in enter_shroud / exit_shroud / apply_buff /
                                                  #   apply_outgoing_condition / remove_enemy_boons / run tick;
                                                  #   prerequisite checks; ConditionalKind::InShroud; GainsLifeForce and
                                                  #   Heal routes; population counting in apply_operation and
                                                  #   apply_skill_effect; new TraceKinds and report totals
crates/optimizer/src/rotation/mod.rs              # RotationSkill.categories, slot_name
crates/optimizer/src/rotation/builder.rs          # fill categories/slot; Number of Targets → SkillEffect.targets
crates/optimizer/src/engine.rs                    # executed-from-facts set; ReasonClass on the coverage list;
                                                  #   population from scenario into WvwTimelineInput
crates/optimizer/src/combat.rs                    # returns the trait ids whose facts it consumed (no numeric change)
crates/optimizer/src/referee.rs                   # search_rank support slot adds ally boon output; ranking tests;
                                                  #   trait_coverage_audit and records_match_their_wiki_pages (#[ignore])
crates/optimizer/src/rotation/reaper_fixture.rs   # necro opener variants that reach every trigger kind
crates/optimizer/src/rotation/necro_published.rs  # one GuildJen WvW Necromancer build as a fixture (URL + scrape date)
crates/optimizer/src/rotation/<prof>_fixture.rs   # one per later increment (opener + published)
data/normalized_effects/2026-01-13/wvw.json       # Necromancer trait records (111 states), then one profession per increment
data/formulas/fight_population.json               # foes/allies per CombatTier
```

**Structure Decision**: existing files plus one data loader, one data file, one fixture file per profession, and the generated audit table. Experiments stay `#[cfg(test)]` beside the code as in Sprints 1 and 2.

## Execution order

1. **Schema (US1, US2 groundwork; R2–R5, R7).** Add the enum variants, `Prerequisite`, `ScaleBy`, `CoverageBlock`, `healing_power_coefficient`, the new `TriggerScope` variants and the validation rules (empty prerequisite rejected; trait `OnSkillUse` needs a scope; a `coverage` block excludes a payload; `Periodic` needs `internal_cooldown`). Round-trip tests; all fourteen existing records still load. Commit.
2. **Coverage truth (US4; R7).** Seen-failing first: the fixture's coverage line names Gravedigger. Then the executed-from-facts set from the builder and the parser, `ReasonClass` suffixes, `active_normalized_effects` filters and classifies. Controls: no executed weapon skill on the line; a classified record shows its class; empty line ⇒ Verified. Commit. (Goes second so every later step's coverage assertions are meaningful.)
3. **Shroud triggers (US1; R2).** Seen-failing: a `OnShroudEnter` record never fires on the fixture's entry. Then the two firing sites, `ConditionalKind::InShroud`, the Scourge rule (Desert Shroud is the entry; Manifest Sand Shade is not: `is_shroud_entry` already excludes it, the test proves it). Controls: fires at entry, once at exit for each `why`, active only inside, removed-trait run differs, determinism. Commit.
4. **Prerequisites and trait-owned skill-use (US2; R3, R4).** Seen-failing: Chilling Nova's record fires on an unchilled foe. Then `prerequisite` evaluation in `trigger_procs` and `update_conditionals`, `foe_has_condition`, `RotationSkill.categories`, `scope_admits` on category/slot/status, `OnConditionApplied` / `OnBoonApplied` / `OnBoonStripped` / `Periodic` sites, `GainsLifeForce` and `Heal` routes, `ScaleBy::ConditionsRemoved`. Controls per trigger: positive, negative, cooldown, trace reason `prerequisite never met`. Commit.
5. **Fight population (FR-003a; R6).** Seen-failing: a five-target boon record in a Havoc scenario credits one target. Then `fight_population.json` and loader, `FightPopulation` on `WvwTimelineInput`, counting in `apply_operation` / `apply_skill_effect` / cleanse / heal, the four report totals, `search_rank` support slot, `cleave_damage` into damage totals. Controls: Roam credits one, Havoc credits five (player + 4) or the record's cap, Cloud caps at the record's `target_count`; two support builds differing in one ally-facing trait rank apart; PvE pin unchanged. Commit.
6. **Necromancer catalogue (US3; R1, R5, R8).** Write records for the 111 traits from the wiki (core lines first, then Reaper, Scourge, Harbinger, Ritualist), each with URL and read date, each with a firing test in `necro_experiments` or a `coverage` class. The trait-coverage audit test generates `docs/audit/trait-coverage.md`; SC-001 on the cached Reaper build (`#[ignore]`, dev.cfg); the wiki-number check (`#[ignore]`, needs `crawl_url` in `dev.cfg`). Published GuildJen Necromancer WvW build as a fixture; opener variants that reach every trigger kind. Two commits: core lines, elite lines.
7. **Evidence, gates, release of increment 1.** Audit section 9 with quoted failures for every control; `disable_and_run.py` entries; determinism and trace-cap tests re-run; timing recorded; CHANGELOG under Unreleased; version bump; the release checklist from `quickstart.md` §7. Push and PR only when the gates pass, as the spec's automated acceptance says.
8. **Increments 2–9 (US3, FR-011, FR-012).** For each remaining profession, in the order of the cached characters then alphabetical: read the profession's 111 trait pages, add any trigger or prerequisite the profession needs that Necromancer did not (expected: none for the trigger enum; new `NeedsMechanic` names are likely), write the records and the `coverage` classes, add `<prof>_fixture.rs` (opener + published GuildJen build), the experiments, the audit rows, the wiki check, and release. Each is its own branch off the previous increment's tip, its own PR, its own version.

Steps 1–5 are the mechanism and are Necromancer-independent; step 6 is the first catalogue; step 8 repeats step 6 eight times.

## Risks and implementation decisions

- **Population counting is arithmetic, not simulation.** Extra foes and allies have no state, so a record that reads an ally's conditions cannot be written this sprint; it takes `coverage: NeedsMechanic("ally state")`. Stated in the audit as a known approximation.
- **`OnConditionApplied` fan-out.** Every skill fact that applies a condition now passes through one helper; the helper is the only new call in the hot path and is a Vec push plus a `trigger_procs` scan, so SC-006 holds. Measured in step 7.
- **Trait facts already in `SimParams`.** A trait with both a stat line and a trigger keeps the stat line in the parser and the trigger in a record; the executed-from-facts set marks it executed either way and the record carries only the trigger half. The pin in step 2 guards double counting.
- **Wiki pages with split numbers.** Records carry the WvW number; a page without a WvW column leaves `value` unresolved and the trait sits on the line as `(unresolved value)`, never a guess.
- **The wiki-number check needs a network.** It is `#[ignore]` and part of the release checklist, not `cargo test`. The read date in `source` is the audit trail when the network is not there.
- **Rank-key change for support kinds.** Affects only WvW Support/Commander ordering; pinned by a direction test in both directions (trait present ranks above absent; two damage builds unchanged).
- **Shroud entry outranks weapon damage in `pick_skill`** (taken during implementation, kept). The improviser never entered shroud in the production profile, so every entry record of the cached build had no firing site; an affordable entry now carries a +600 000 priority. No `reaper_*` pin moved; `necro_shroud_enter_fires_once_at_entry` and the catalogue run depend on it.
- **WvW-only routing of non-damaging skill-fact conditions** (taken during implementation, kept). The builder emits Chilled, Crippled, Weakness, Vulnerability and the like as `ApplyBuff` and Fear as `CrowdControl`; the WvW timeline puts any `ApplyBuff` whose name is a known condition on the foe, and Fear / Taunt land as conditions as well as disables (wiki `Fear`: a condition that counts as a control effect). PvE and PvP builder output is untouched (`pve_output_unchanged_by_conditional_tagging`).
- **Death's Carapace as a self buff read at the strike** (convergence). The buff is a `TimedBuff` named `Death's Carapace`; `receive_strike` scales the strike by armor / (armor + 20 x stacks). Its 10 s runs through the boon-duration multiplier like every self buff (a known approximation). The threshold halves stay coverage classes with their own names.
