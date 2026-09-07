# Implementation Plan: WvW proc firing sites (Sprint 2, CONN-03 C)

**Branch**: `005-wvw-proc-sites` (off the Sprint 1 tip `6e4820c` on `004-simulator-trust`) | **Date**: 2026-09-08 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/005-wvw-proc-sites/spec.md`; audit `docs/simulator-connection-audit.md` (CONN-00-06, CONN-00-07, CONN-00-09, CONN-01-01, CONN-01-02); research in [research.md](research.md).

## Summary

Make the four source kinds Sprint 1 proved inert in WvW actually execute: on-crit procs through an expected-value process with seeded trials in the trace (R1), set-2 sigils after a weapon swap (R3), health-threshold and stacking rune/relic bonuses applied per strike without double counting the parser's flattened value (R4), and the dark-field combo arm (R5). Add life force as a first-class resource with one shroud shape for every Necromancer specialisation, prepared from the API's `transform_skills` (R6), and replace the profession allowlist with a derived completeness rule. Ship the first real records (Sigil of Fire, Scholar, Thief, the cached Reaper build's triggered traits) with dated wiki sources (R7). Every mechanic lands behind a positive, negative and timing control that was seen failing first (R8).

## Technical Context

Rust 2021, stable; Windows MSVC Nexus cdylib. Crates touched: `gw2-optimizer` only for code (`rotation/wvw_timeline.rs`, `rotation/builder.rs`, `rotation/mod.rs`, `engine.rs`, `combat.rs`, `data/normalized_effects.rs`, `referee.rs`, `rotation/reaper_fixture.rs`), plus `data/normalized_effects/2026-01-13/wvw.json`, a new `data/formulas/shroud.json`, `docs/simulator-connection-audit.md` section 8, and `CHANGELOG.md`. No addon change: the coverage line, quality reasons and `score_build` already carry everything (the referee appends `shroud_refusals` to `quality_reasons`, which the UI renders today). No new dependency: the trial RNG is an in-module xorshift64*. Storage: none. Testing: `cargo test` unit modules on the synthetic Reaper fixture; one `#[ignore]` test reads the real cache through `dev.cfg`. Performance: default optimizer harness within +10 s of 16.8 s; search never runs trials. Constraints: FR-010 (no calibrated constants), FR-013 (owned files untouched, no push, no release).

## Constitution Check

The constitution file is an unfilled template and supplies no ratified gates. Project gates applied before research and re-checked after design:

- No secrets or account data in artifacts: the cache-backed test prints names, never keys.
- SymForge for discovery and impact; `find_references` before each changed type (`ProcSpec`, `ScheduledHit`, `SkillResourceRule`, `RotationSkill`, `TraceKind`, `WvwCombatReport`, `DamageModifiers`, `NormalizedEffect`).
- Every remedy preceded by a failing behavioural test, seen failing under a scripted disable and quoted in the audit.
- No scoring constant, gate or normalisation moves; PvE and PvP results are byte-identical before and after (a regression test pins `synergy_result_from_validated` output for the fixture in PvE).
- Fixture values never enter `data/`; records carry wiki URL and read date.
- Owned files untouched; commits local; the user tests in-game before any release; the user merges PRs.

Post-design re-check: the schema additions are optional and default, so every existing record loads unchanged; the parser still emits its flattened value so non-WvW callers see no change; the `Bar` field on `RotationSkill` defaults from `weapon_set`, so every constructor compiles without edits beyond the builder. No violations.

## Project Structure

### Documentation (this feature)

```text
specs/005-wvw-proc-sites/
├── plan.md                              # this file
├── spec.md
├── research.md                          # R1–R10
├── data-model.md
├── contracts/
│   ├── normalized-effect-schema.md      # new record fields, Fire value semantics, shroud table
│   └── wvw-report.md                    # report and trace additions
├── quickstart.md
└── tasks.md                             # /speckit-tasks output
docs/simulator-connection-audit.md       # section 8: Sprint 2 experiments and seen-failing evidence
docs/audit/disable_and_run.py            # scripted disable / run / restore (Sprint 1 pattern, generalised)
```

### Source code (repository root)

```text
crates/optimizer/src/data/normalized_effects.rs   # health_threshold, proc_chance, trigger_scope; two validation rules
crates/optimizer/src/rotation/wvw_timeline.rs     # OnCrit site + mass process; ProcSpec.weapon_set; ConditionalSpec;
                                                  #   dark combo arm; LifeForce resource + ShroudState; new TraceKinds;
                                                  #   proc_trials (trace only); xorshift64*; reaper_experiments additions
crates/optimizer/src/rotation/mod.rs              # RotationSkill.bar: Bar
crates/optimizer/src/rotation/builder.rs          # shroud bar from transform_skills; Life Force facts → resource rules input
crates/optimizer/src/engine.rs                    # all four sigil seats selected; conditional divide-out for WvW params;
                                                  #   life force rules from facts + shroud table; derived completeness rule
crates/optimizer/src/combat.rs                    # parse_percent_clauses tags health-gated clauses (flattened value unchanged)
crates/optimizer/src/referee.rs                   # shroud_refusals → quality_reasons; parity and PvE-unchanged tests
crates/optimizer/src/rotation/reaper_fixture.rs   # life force facts, set-2 sigil variant, threshold/stack records, shroud bar
data/normalized_effects/2026-01-13/wvw.json       # Fire, Scholar, Thief rewritten; Reaper build traits added
data/formulas/shroud.json                         # entry floor, drain, reduction per shroud and mode (nulls allowed)
```

**Structure Decision**: existing files only, one new data file, one new audit script. Experiments stay `#[cfg(test)]` beside the code, as in Sprint 1.

## Execution order

1. **Schema and records (US5, R7).** Add the three optional fields and two validation rules with round-trip tests; rewrite the Fire, Scholar and Thief records; add `data/formulas/shroud.json` and its loader. Existing loader tests must stay green with the old files. Commit.
2. **On-crit site (US1, R1, R2).** Seen-failing first: the Sprint 1 unsupported control inverted (sigil must leave the coverage line and appear in the trace). Then the crit-chance helper, the mass process in `trigger_procs`, the Fire coefficient arm, trace weights, and the trial pass under `trace`. Controls: positive, zero-precision negative, ICD timing, trials bracket the expected value. Commit.
3. **Weapon swap (US2, R3).** Seen-failing: set-2 sigil never fires. Then four-seat selection in `active_normalized_effects`, `ProcSpec.weapon_set`, `ScheduledHit.weapon_set`, the held-set filter. Controls: fires only after the swap, only before when moved, cooldown persists across sets, set-2 sigil off the coverage line. Commit.
4. **Conditional bonuses (US3, R4).** Seen-failing: the Scholar bonus applies below the threshold. Then `DamageModifiers.conditional_strike` tagging, the WvW divide-out, `ConditionalSpec` threshold and stacking, trace events. Controls: above/below, crossing time, stacks cap and expire, unresolved stays named, PvE output byte-identical. Commit.
5. **Dark combos (US4, R5).** Seen-failing: degraded count non-zero for Soul Spiral in Nightfall. Then the dark arm with wiki numbers. Controls: whirl life-steals, expired field makes no combo. Commit.
6. **Life force and shroud (US6, R6).** Seen-failing: the fixture enters shroud with zero life force. Then `ResourceKind::LifeForce`, rule fields, the shroud table loader, the builder's shroud bar from `transform_skills`, `Bar` on `RotationSkill`, `pick_skill` filtering, `ShroudState` transitions, damage-to-pool, no-heal, forced exit cancelling a channel, refusal reasons into `quality_reasons`, the derived completeness rule with its nine-profession pin. Largest step; split into two commits (resource and rules; shroud bar and transitions) if the diff passes 600 lines.
7. **Evidence and closing.** Audit section 8 with the quoted failures for every control; `reaper_results_repeat_identically`; the `#[ignore]` cached-build test; gates from quickstart §6; CHANGELOG entry under the unreleased version. No bump, no push.

Steps 2 to 5 are independent of 6 and can be reviewed separately; 6 depends on 1 (shroud table) and touches the builder, so it goes last among the code steps.

## Risks and implementation decisions

- **Expected-value mass process vs cooldown semantics.** The mass scheme matches the true proc rate in expectation but front-loads the first cycle (fires scaled on every ready hit). The trials in the trace are the honesty check: SC-009 requires the trial mean within one proc of the expected count on the fixture opener. If the fixture shows a larger gap, record it in the audit and prefer the trials' number in the trace, not in the score.
- **Double counting conditionals.** The divide-out in the WvW branch must run only for sources whose record actually loaded; the parity test (referee vs Optimize) already asserts `strike_mult` agreement and will catch a divide applied on one side only.
- **`transform_skills` quirks.** Shroud skills carry `Downed_*` and `Weapon_5` slots; the mapping is explicit and tested against the cached ids (29442, 29458, 30278, 30825, 29958, 30504, 30557). Scourge has no entry skill and must not get a `ShroudState`. Harbinger and Ritualist numbers are `null` until read; their builds report the resource incomplete rather than guessing.
- **Life force starting at zero** makes the fixture opener refuse shroud until it has cast the generators; the opener is re-ordered in the fixture (Gravedigger, Death Spiral, Well, then shroud) and that ordering is stated in the fixture doc comment. The scenario may later expose a starting pool; not this sprint.
- **`RotationSkill` constructors.** `Bar` defaults from `weapon_set` through a constructor helper so the dozens of literal constructions in tests do not change; `find_references(RotationSkill)` before deciding whether a `Default` impl or a helper is cheaper.
- **Trace cap.** The fixture opener with every new kind must fit under 512 events; a test asserts `trace_truncated == false`.
- **Test budget.** Trials only under `trace`; the 8-seed pass is one extra timeline run each, well under the 10 s cap. The cached-build test is `#[ignore]`.
- **Locale parity.** No new UI string; `shroud_refusals` flow through existing `quality_reasons` rendering.

## Parallel opportunities

Steps 2–5 touch different functions of the same file; they are sequenced by commit but can be drafted in parallel and rebased. Step 1 (data and schema) and the audit script generalisation are independent of everything. Step 6 is serial after 1. Cargo runs serialise on the target directory; use the worktree's separate target for a parallel drafting agent only if one is used.

## Complexity Tracking

No constitution violations. Additions: three optional record fields, one small data file, one enum variant on `ResourceKind`, one `Bar` field, one `ConditionalSpec` list, one `ShroudState`, eight trace kinds, two report fields, one in-module PRNG. No new crate, dependency, format or framework.
