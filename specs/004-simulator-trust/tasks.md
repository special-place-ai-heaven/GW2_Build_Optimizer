# Tasks: Simulator trust (Sprint 1)

**Input**: Design documents from `/specs/004-simulator-trust/` (plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md)

**Tests**: Required. The spec's experiments are behavioural tests (FR-008) and every remedy must be preceded by its failing experiment (FR-014). Each experiment is written first, seen failing under a disabling change, and the failing assertion text is recorded in the audit document.

**Organization**: grouped by user story. Branch `004-simulator-trust` in the main checkout. Never edit `crates/optimizer/src/prompts.rs`, `crates/optimizer/src/llm/`, or `crates/optimizer/examples/choya_live.rs` (owned by the latency work). Line numbers cited in research.md are from the worktree at `09740ce`; re-resolve every symbol with SymForge before editing and run `analyze_file_impact` after.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependencies)
- **[Story]**: US1 audit, US2 trace, US3 experiments and tool, US4 remedy

## Path Conventions

Rust workspace: `crates/optimizer/src/...`, `crates/addon/src/...`, `locales/*.json`, `docs/...`, `specs/004-simulator-trust/...`.

---

## Phase 1: Setup

**Purpose**: baseline recorded, ownership fenced, budget measured.

- [X] T001 Record the baseline into `docs/simulator-connection-audit.md` section 1 per `specs/004-simulator-trust/contracts/audit-document.md`: commit id of `004-simulator-trust` HEAD, branch, workspace version from `Cargo.toml`, active data manifest id from `crates/optimizer/data/normalized_effects/` (directory name), scenario `WvW Solo/StrikeSpike`, and the dirty/owned paths of the latency work (`crates/optimizer/src/prompts.rs`, `crates/optimizer/src/llm/`, `crates/optimizer/examples/choya_live.rs`); no credentials, no character names
- [X] T002 [P] Measure the test-time budget baseline: run `cargo test -p gw2-optimizer --lib` twice on this checkout, record both wall times and the passed/ignored counts in `docs/simulator-connection-audit.md` section 1 under "Test budget baseline"
- [X] T003 [P] Write `docs/simulator-connection-audit.md` section 2 (Inventory) listing, with paths: `crates/optimizer/tests/{scoring_regression,math_permutations,objective_profiles_integration}.rs`, `crates/optimizer/examples/{calibrate_viability,flow_calibration,necro_holes_check,scourge_support_check}.rs`, `crates/optimizer/src/data/consistency_tests.rs`, the timeline published fixtures (Spellbreaker, Mirage, Virtuoso) in `crates/optimizer/src/rotation/wvw_timeline.rs`, `crates/optimizer/data/rotation_profiles/{wvw,pve}.json` Necromancer rows, and the normalized-effect files; state explicitly that no Reaper record, profile or fixture exists

---

## Phase 2: Foundational

**Purpose**: the hand-authored Reaper slice every experiment uses.

**⚠️ CRITICAL**: no experiment can be written until T004–T006 compile.

- [X] T004 Add a `#[cfg(test)] pub(crate) mod reaper_fixture` in `crates/optimizer/src/rotation/mod.rs` (or a new `crates/optimizer/src/rotation/reaper_fixture.rs` declared there) exposing `fn db() -> GameDb` built from `GameDb::empty_for_tests()` with the Necromancer profession, specializations Spite, Soul Reaping, Reaper with their trait ids and names, the skills Gravedigger, Death Spiral, Grasping Darkness, Reaper Shroud 1–4, Signet of Vampirism, Well of Suffering, Well of Darkness, "You Are All Weaklings!", "Chilled to the Bone!", weapons Greatsword and Axe+Focus, items Superior Sigil of Fire, Superior Sigil of Force, Superior Rune of the Scholar, Relic of the Thief; ids may be synthetic but must be unique and documented in a comment as fixture-only
- [X] T005 In the same module add `fn build() -> ValidatedBuild` (greatsword set 1, axe/focus set 2, Sigil of Fire + Force on set 1, Scholar rune, Thief relic, Marauder prefix on every slot, the three specializations with fixed trait choices) and `fn opener() -> Vec<u32>` (press order: Well of Suffering, Gravedigger, Death Spiral, Reaper Shroud, Shroud 4, Shroud 1) following the `make_minimal_validated` pattern in `crates/optimizer/src/referee.rs`
- [X] T006 In the same module add `fn records() -> Vec<NormalizedEffect>` with hand-authored values: `Path of Corruption` (Trait, OnHit, CorruptsBoon, ICD 10 s, WvW), `Superior Sigil of Fire` (Sigil, OnCrit, StrikeDamagePct, ICD 5 s, WvW), and one `Passive` sigil record for Force; document in a comment that these are fixture values, not sourced data, and must never be copied into `crates/optimizer/data/`
- [X] T007 Add `fn scenario() -> (BalanceContext, ScenarioSpec)` for `GameMode::WvW`, `CombatTier::Solo`, `CombatKind::StrikeSpike` and `fn pve_scenario()` for `GameMode::PvE`, using `ScenarioSpec::from_balance_context` in `crates/optimizer/src/scenario.rs`
- [X] T008 Verify the fixture compiles and evaluates: add `reaper_fixture_evaluates_without_errors` in `crates/optimizer/src/referee.rs` tests calling `evaluate_validated_build_with(&build(), &db(), "Necromancer", &weights, &ctx, &scenario, &opener())` and asserting `report.viability` is computed and `validated.errors` is empty

**Checkpoint**: `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_fixture` passes.

---

## Phase 3: User Story 1 - A maintainer can see what the simulator really does (Priority: P1) 🎯 MVP

**Goal**: the audit document holds the baseline, inventory and the first findings with reproductions.

**Independent Test**: open `docs/simulator-connection-audit.md`, pick any finding, follow its reproduction on the baseline commit, observe the stated behaviour.

- [X] T009 [US1] Write `docs/simulator-connection-audit.md` section 3 (Findings) table header with the columns from `specs/004-simulator-trust/data-model.md` "Finding" and add CONN-00-01 "Reaper has no normalized-effect records, rotation profile, published fixture, or life-force resource model" with evidence paths (`crates/optimizer/data/normalized_effects/*/wvw.json`, `crates/optimizer/data/rotation_profiles/wvw.json`, `crates/optimizer/src/rotation/wvw_timeline.rs` `ResourceKind`, `crates/optimizer/src/engine.rs` resource allowlist), classification `data gap`, status `demonstrated` once T003 is written
- [X] T010 [P] [US1] Add hypotheses CONN-00-02 "equipped-character display never runs the referee" (evidence `crates/addon/src/ui/main_view/resolution.rs` → `stats::calculate_full_stats`, Generic buff profiles in `crates/optimizer/src/combat.rs`) and CONN-00-03 "imported published builds never run the referee" (evidence `crates/addon/src/ui/main_view/provider_picks.rs` `adopt_provider_pick`), both classification `reporting gap`, status `open`
- [X] T011 [P] [US1] Add hypotheses CONN-00-04 "unmodeled effect sources are counted, names discarded" (evidence `engine.rs::active_normalized_effects` `.count()`, `wvw_timeline.rs::load_normalized_effects` `+= 1`), CONN-00-05 "a Choya plate's suggestion is always Verified" (evidence `chat_flow.rs` suggestion built with `..Default::default()`), CONN-00-06 "OnCrit has no firing site" (evidence `wvw_timeline.rs` `trigger_procs` call sites), CONN-00-07 "weapon swap does not refresh the active sigil set" (evidence `try_weapon_swap`, `Timeline::new` proc load), CONN-00-08 "chill on the enemy has no effect" (evidence `CHILLED_RECHARGE_PERCENT` applies to the player only), CONN-00-09 "`resource_model_complete` is a profession allowlist that includes no Necromancer entry yet reads complete" (evidence `engine.rs` allowlist), CONN-00-10 "suggestion gates recomputed with a narrower gate set than the referee" (evidence `optimization.rs::synergy_result_to_suggestion` `evaluate_viability_gates` + off-bar stability only), CONN-00-11 "referee and tool have no cancellation probe" (evidence `referee.rs`, `gemini_tools.rs` staleness check only)
- [X] T012 [US1] Link each finding to related `W###` ids from `specs/003-audit-remediation/ledger.md` in the `links` column (at minimum B001 for CONN-00-05 and W011 for diagnostics) without renumbering anything
- [X] T013 [US1] Write the reproduction column for every finding as a test name (planned in Phase 5) or a command, and mark every entry that has no test yet as `hypothesis`

**Checkpoint**: audit sections 1–3 complete; every finding has evidence, reproduction, classification, status.

---

## Phase 4: User Story 2 - One build's mechanics are traced end to end (Priority: P2)

**Goal**: the 8 × 8 trace matrix for the Reaper slice, every cell classified, every open question answered or left open with reason.

**Independent Test**: any `exact`/`approximate` cell points to a function or event; any `missing` cell states a reason.

- [X] T014 [US2] Write `docs/simulator-connection-audit.md` section 4 (Trace matrix) skeleton: rows Gravedigger, Path of Corruption, Superior Sigil of Fire, Superior Sigil of Force, Superior Rune of the Scholar, Relic of the Thief, life force, Reaper Shroud 4 dark field + finisher; columns intent, parsing/validation, data selection, derived parameters, rotation preparation, runtime, evaluation, exposure; every cell initially `not-yet-checked`
- [X] T015 [P] [US2] Fill columns intent, parsing/validation, data selection from `crates/optimizer/src/validation.rs::validate_gemini_build`, `crates/optimizer/src/scenario.rs`, `crates/optimizer/src/engine.rs::active_normalized_effects` (worn-set sigils only), `crates/optimizer/src/data/normalized_effects.rs::effects_for_mode`, naming the actual caller per cell
- [X] T016 [P] [US2] Fill columns derived parameters and rotation preparation from `crates/optimizer/src/engine.rs::calculate_validated_stats`, `apply_validated_gear_stats`, `prepare_validated_rotation`, `crates/optimizer/src/combat.rs::buff_profiles_for_profession`, noting where a passive is folded into `SimParams` and whether any runtime record re-applies it
- [X] T017 [P] [US2] Fill columns runtime and evaluation from `crates/optimizer/src/rotation/wvw_timeline.rs` (`load_normalized_effects`, `trigger_procs`, `resolve_combo`, `try_weapon_swap`, `tick_recharge_rate`) and `crates/optimizer/src/referee.rs::evaluate_validated_build_with` (gate sim vs flow sim, which fields feed gates, realized axes, quality)
- [X] T018 [US2] Fill column exposure for each of the five paths: equipped display (`crates/addon/src/ui/main_view/resolution.rs`), Optimize (`crates/addon/src/ui/main_view/optimize_flow.rs` → `optimization.rs::synergy_result_to_suggestion` → `crates/addon/src/ui/comparison.rs`), Choya (`crates/addon/src/ui/main_view/chat_flow.rs` `plate_shortfall`, suggestion build, `attach_chat_stats`), published import (`provider_picks.rs`), search (`crates/optimizer/src/search_v2.rs` evaluation entry); state what each path retains of `RefereeReport`
- [X] T019 [US2] Write section 5 (Open questions answered) with one line each: weapon swap and stowed sigils (answer from CONN-00-07), passive double counting (answer from T016), on-hit semantics (strikes only, condition ticks never fire procs, cite `apply_skill_effect` StrikeDamage arm), support boons vs self uptime (buff profiles are assumed external; cite), starved non-damage skills (DPCT scheduler in `simulator.rs::pick_skill`; opener in timeline), gate/flow/tooltip/tool parity (cite the two scenarios and CONN-00-02/-10), warning survival (cite CONN-00-05); anything unproven is `left open:` with reason
- [X] T020 [US2] Replace every remaining `not-yet-checked` cell or record its reason under the matrix; add findings CONN-01-NN for each `missing` cell that is not already a CONN-00 finding

**Checkpoint**: matrix complete; SC-002 and SC-007 satisfied for the slice.

---

## Phase 5: User Story 3 - Causal experiments separate real gaps from noise (Priority: P3)

**Goal**: seven experiment kinds on the Reaper slice, a bounded trace for the timing ones, cross-path parity, and Choya's full-build tool.

**Independent Test**: `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_` lists every kind; each was seen failing once under a disabling change recorded in the audit.

### Diagnostics (needed by the timing experiments)

- [ ] T021 [US3] In `crates/optimizer/src/rotation/wvw_timeline.rs` add `pub trace: bool` (default false) to `WvwTimelineInput`, `pub trace: Vec<TraceEvent>` and `pub trace_truncated: bool` to `WvwCombatReport`, and `pub struct TraceEvent { pub t_ms: u32, pub kind: TraceKind, pub source: String, pub detail: String }` with `pub enum TraceKind { HitLanded, ProcFired, ProcSkippedIcd, ProcUnmodeled, CastInterrupted, WeaponSwap }`; push events at the landing site in `apply_skill_effect`, in `trigger_procs` (fired and ICD-skipped), in `note_unmodeled_proc`, at the interrupt site, and in `try_weapon_swap`; cap at 512 with the 513th setting `trace_truncated`
- [ ] T022 [US3] Add tests in `crates/optimizer/src/rotation/wvw_timeline.rs`: `trace_is_empty_unless_requested` (default input yields empty `trace`), `trace_caps_at_512_and_flags_truncation`; and in `crates/optimizer/src/search_v2.rs` assert via `find_references` that no production caller sets `trace = true` (add a comment on `WvwTimelineInput.trace` naming this rule)
- [ ] T023 [US3] Update `crates/optimizer/src/engine.rs::simulate_prepared` to pass `trace: false` and add `pub fn evaluate_wvw_timeline_traced(...)` (or a `trace` parameter on the existing helper) used only by tests, so experiments can request a trace without changing production calls

### Experiments (write each, see it fail under the disabling change, record the assertion text in audit section 6)

- [ ] T024 [P] [US3] Positive control `reaper_positive_control_onhit_proc_changes_events` in `crates/optimizer/src/rotation/wvw_timeline.rs`: fixture build + opener with `Path of Corruption` record vs without; assert `ProcFired` events with `source == "Path of Corruption"` exist only with the record and that `unmodeled_sources` does not name it; disabling change: remove the record
- [ ] T025 [P] [US3] Negative control `reaper_negative_control_wrong_mode_and_stowed_set` in `crates/optimizer/src/rotation/wvw_timeline.rs`: (a) the same record tagged PvE-only under the WvW scenario yields no `ProcFired`; (b) Sigil of Fire moved to weapon set 2 while set 1 is worn is absent from both `proc_specs` and `unmodeled_sources`; disabling change: tag the record WvW / move the sigil back
- [ ] T026 [P] [US3] Timing `reaper_timing_icd_interrupt_and_late_buff` in `crates/optimizer/src/rotation/wvw_timeline.rs`: assert consecutive `ProcFired` for Path of Corruption are ≥ 10 000 ms apart; an `EnemyEvent::Control` during Gravedigger's cast yields `CastInterrupted` and fewer `HitLanded` for that skill than the uninterrupted run; Might applied at t+1 does not change the `HitLanded.detail` damage of the hit at t; disabling changes: set ICD to 0, remove the control event, apply Might before the hit
- [ ] T027 [P] [US3] Ablation `reaper_ablation_enabler_and_payoff` in `crates/optimizer/src/rotation/wvw_timeline.rs`: complete (proc record + the skill that lands hits), missing enabler (skill removed from opener), missing payoff (record removed); state the expected event difference in a comment before asserting `ProcFired` counts complete > missing-enabler and complete > missing-payoff; allow equal when the ICD saturates
- [ ] T028 [P] [US3] Interaction pair `reaper_interaction_might_times_strike_modifier` in `crates/optimizer/src/rotation/simulator.rs` tests: 2 × 2 over Might stacks {0, 25} and `strike_mult` {1.0, 1.1}; compute `f(A+B) − f(A) − f(B) + f(base)` on total strike damage and assert its sign matches the multiplicative model, with eps 1.0
- [ ] T029 [P] [US3] Unsupported control `reaper_unsupported_oncrit_is_named_not_zeroed` in `crates/optimizer/src/rotation/wvw_timeline.rs`: Sigil of Fire record on a build with precision forced to 100 % crit; assert zero `ProcFired` for it, `ProcUnmodeled` present, `unmodeled_sources` contains `"Superior Sigil of Fire (on-crit)"` (this assertion FAILS until Phase 6 lands and is the remedy's regression test), and `total_strike_damage` equals the run without the sigil
- [ ] T030 [US3] Parity part 1 `reaper_parity_referee_matches_optimize_suggestion` in `crates/optimizer/src/referee.rs` tests: run `evaluate_validated_build_with` and `engine::synergy_result_from_validated` on the fixture; assert `user_intent_score`, the six `realized` axes, `viability.is_viable` and `quality` equal within 1e-9; document in the audit that suggestion gate lists may differ (CONN-00-10) and are not asserted
- [ ] T031 [US3] Add the PvE comparison `reaper_pve_comparison_uses_adaptive_scheduler_not_opener` in `crates/optimizer/src/referee.rs` tests: same build under `pve_scenario()`; assert `report.rotation.wvw.is_none()` and record in audit section 5 that PvE sequencing is adaptive, so PvE-vs-WvW totals are not comparable on ordering
- [ ] T032 [US3] Record every experiment in `docs/simulator-connection-audit.md` section 6 with kind, test name, file, the disabling change used, the failing assertion text observed, and the finding it serves; move findings from `hypothesis` to `demonstrated` or `refuted`

### Choya full-build tool (FR-018; executes LAST, after Phase 6 and after the latency loop on `fix/wiki-timing-facts` has closed)

- [ ] T033 [US3] Add `pub scenario: &'a ScenarioSpec` to `ToolContext` in `crates/optimizer/src/gemini_tools.rs` and set it at both construction sites: `crates/addon/src/ui/main_view/chat_flow.rs` (the chat worker, where `scenario` is already in scope) and `crates/addon/src/ui/main_view/optimize_flow.rs` (the advisor call); update `crates/optimizer/examples/choya_live.rs` ONLY if it fails to compile, with the minimal field addition and nothing else
- [ ] T034 [US3] Extend `decl_score_build` in `crates/optimizer/src/gemini_tools.rs` with the optional `build` object per `specs/004-simulator-trust/contracts/score-build-tool.md` (properties mirroring `prompts::GeminiBuildResponse`), keep `gear_prefix` optional, and update the description to say the full-build form returns the app's referee verdict
- [ ] T035 [US3] Implement full-build mode in `exec_score_build` in `crates/optimizer/src/gemini_tools.rs`: deserialize `build` into `GeminiBuildResponse`, call `validation::validate_gemini_build(&plate, ctx.db, ctx.profession_name)`, return `{"mode":"full_build","errors":[...]}` when `validated.errors` is non-empty, else `referee::evaluate_validated_build_with(&validated, ctx.db, ctx.profession_name, &ctx.weights, ctx.balance_ctx, ctx.scenario, &[])` and emit `viable`, `gates[{gate,passed,note}]`, `user_intent_score`, `realized{...}`, `quality`, `quality_reasons[]`, `coverage_note`, `warnings[]` from `validated.warnings`; return `{"error":"no build supplied"}` when both `gear_prefix` and `build` are absent; prefix-only mode gains `"mode":"prefix_only"` and `"scope"` text
- [ ] T036 [US3] Add the per-request evaluation cap: a counter captured by the tool callback closure in `crates/addon/src/ui/main_view/chat_flow.rs` next to the existing stale-epoch check; after three full-build `score_build` calls return `{"error":"evaluation budget spent"}`; unit test the counter logic where it is testable
- [ ] T037 [US3] Update `decl_simulate_rotation` description in `crates/optimizer/src/gemini_tools.rs` to add "estimates a skill list on an open dummy; not a full-build verdict; use score_build with a build for that"; keep its behaviour unchanged
- [ ] T038 [US3] Parity part 2 `score_build_full_mode_matches_referee` in `crates/optimizer/src/gemini_tools.rs` tests: build a `ToolContext` from the fixture, call `execute_tool("score_build", {"build": plate_json}, &ctx)`, and assert the returned `user_intent_score`, six axes, `viable`, `quality` equal `evaluate_validated_build_with` within 1e-9; plus `score_build_without_build_or_prefix_errors` and `score_build_never_substitutes_equipped_build` (context has a `current_build_summary`, tool called with no `build`: error, not a verdict)

**Checkpoint**: all seven kinds present; SC-003 budget checked (T044); FR-018 contract tests pass.

---

## Phase 6: User Story 4 - The smallest demonstrated break is fixed (Priority: P4)

**Goal**: the coverage qualification carries names end to end and shows as one line on the overlay and in Choya's evidence. Its regression test is T029.

**Independent Test**: T029 passes; `cargo test -p gw2-build-optimizer --lib coverage_note` passes; the locale parity test passes.

- [ ] T039 [US4] In `crates/optimizer/src/rotation/wvw_timeline.rs` replace `unmodeled_effect_sources: u32` on `WvwCombatReport` (and the intermediate state) with `unmodeled_sources: Vec<String>` formatted `"{name} ({trigger})"` where trigger is `on-crit`, `on-skill-use`, `on-health-threshold`, `conditional`, `unresolved value`, `partial combo`, `dark field`; push names in `load_normalized_effects`, `note_unmodeled_proc` (dedupe by the existing key set) and `resolve_combo`; update every reader found by `find_references` to use `.len()` where a count is still wanted
- [ ] T040 [US4] In `crates/optimizer/src/engine.rs::active_normalized_effects` return `(Vec<&NormalizedEffect>, Vec<String>)`: keep `equipped.difference(&modeled)`, resolve each `(source_type, id)` to a name via the db trait/skill/item maps, format `"{name} (no record)"`; update callers (`simulate_prepared`, `synergy_result_from_validated`) and merge these names into the timeline input so one list reaches the report
- [ ] T041 [US4] Add `pub fn coverage_reason(profession: &str, mode: GameMode, unmodeled: &[String]) -> Option<DataQualityReason>` in `crates/optimizer/src/data/quality.rs` producing `field = "wvw_timeline.effects"`, explanation `"Not simulated: {up to 3 names} and N others"`; replace the duplicated blocks in `crates/optimizer/src/referee.rs` (`evaluate_validated_build_with` quality section) and `crates/optimizer/src/engine.rs` (`synergy_result_from_validated`) with calls to it; unit test the 0, 1, 3 and 5-name renderings
- [ ] T042 [US4] Add `pub coverage_note: Option<String>` to `BuildSuggestion` in `crates/addon/src/ui/comparison.rs`; populate it in `crates/addon/src/ui/main_view/optimization.rs::synergy_result_to_suggestion` and `attach_chat_stats` from the reason whose `field == "wvw_timeline.effects"`; in `render_data_quality_badge` draw the line on the same row in muted text via `tf("quality.coverage_line", &[("detail", note)])` while keeping the tooltip; test `coverage_note_is_set_from_wvw_effects_reason` in `optimization.rs`
- [ ] T043 [US4] Add `"quality.coverage_line": "Not simulated: {detail}"` to `locales/en.json` and translated equivalents to `locales/{de,es,fr,it,ja,ko,nl,pl,pt,ru,zh}.json` in one commit so the locale parity test (#21) passes
- [ ] T044 [US4] In `crates/addon/src/ui/main_view/chat_flow.rs`: when the Choya suggestion is built, set `data_quality`, `quality_reasons` (stringified) and `coverage_note` from the `plate_shortfall` referee report instead of `..Default::default()`; append the coverage line to `concerns` before the `fmt.plate_concern` join; test `choya_suggestion_carries_referee_quality` where the flow is unit-testable, otherwise document the in-game check in quickstart step
- [ ] T045 [US4] Confirm T029 now passes unchanged, record the remedy in `docs/simulator-connection-audit.md` section 7 with the commit id, and set CONN-00-04 and CONN-00-05 to `remedied`

**Checkpoint**: SC-004 satisfied at all three projections; no calibrated constant changed.

---

## Phase 7: Polish & Cross-Cutting

- [ ] T046 Measure `cargo test -p gw2-optimizer --lib` wall time against T002; if the delta exceeds 10 s, mark the heaviest `reaper_*` tests `#[ignore = "slow: run on demand"]` until it does not, and record the final delta in the audit section 1
- [ ] T047 Run the workspace gates from `specs/004-simulator-trust/quickstart.md` step 7 (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --release`) and fix anything they raise within the files this feature owns
- [ ] T048 [P] Add a `CHANGELOG.md` entry under the next unreleased version describing the coverage line and the full-build `score_build` mode; do not bump `Cargo.toml` unless the release condition in spec FR-014 / Story 4 scenario 3 holds
- [ ] T049 [P] Final pass on `docs/simulator-connection-audit.md`: every finding has a status, every `missing` cell a reason, no percentage of mechanics implemented, `Verified`/`Provisional`/`Blocked` used only with their `DataQuality` meaning, no account or character names
- [ ] T050 Only if the release condition holds (free models answer and a player produced a build with Choya in-game on the latency DLL): bump patch version, build, copy `gw2_build_optimizer.dll` to the addons dir from `dev.cfg`, and stop for the user's in-game check of the coverage line per quickstart; otherwise leave commits local and state so in the handoff

---

## Dependencies & Execution Order

- **Setup (Phase 1)**: none. T002 and T003 parallel with T001.
- **Foundational (Phase 2)**: after Phase 1; T004 → T005 → T006 → T007 → T008 sequential (same module).
- **US1 (Phase 3)**: after Phase 2 (T009 cites the fixture); T010, T011 parallel; T012, T013 after them.
- **US2 (Phase 4)**: after Phase 2; T015–T017 parallel after T014; T018–T020 sequential.
- **US3 (Phase 5)**: T021 → T022 → T023, then T024–T029 parallel (different test functions, same file: land in one commit), T030–T031 after T023, T032 after all experiments. **T033–T038 are sequenced last of the whole sprint**, after Phase 6 and after the latency experiment on `fix/wiki-timing-facts` has closed, because they change prompt-visible tool declarations.
- **US4 (Phase 6)**: after T029 exists and fails; T039 → T040 → T041 sequential (optimizer), then T042 → T043 → T044 (addon), then T045.
- **Polish (Phase 7)**: after all desired phases; T050 only under its stated condition.

### Parallel Example: User Story 3 experiments

```text
Task: "reaper_positive_control_onhit_proc_changes_events in wvw_timeline.rs"
Task: "reaper_negative_control_wrong_mode_and_stowed_set in wvw_timeline.rs"
Task: "reaper_timing_icd_interrupt_and_late_buff in wvw_timeline.rs"
Task: "reaper_ablation_enabler_and_payoff in wvw_timeline.rs"
Task: "reaper_interaction_might_times_strike_modifier in simulator.rs"
Task: "reaper_unsupported_oncrit_is_named_not_zeroed in wvw_timeline.rs"
```
Same-file tasks are drafted in parallel but committed together; Cargo runs serialize on this checkout's target directory.

---

## Implementation Strategy

### MVP first (User Story 1)

1. Phase 1 and Phase 2: baseline, budget, fixture.
2. Phase 3: audit with findings and reproductions.
3. Stop and validate: a second reader follows two reproductions.

### Incremental delivery

1. Phase 4 (trace matrix) → audit sections 4–5 complete.
2. Phase 5 diagnostics + experiments → seven kinds, T029 failing on purpose.
3. Phase 6 remedy → T029 passes, line visible in the overlay and in Choya's concerns.
4. Phase 5 tool tasks T033–T038 → parity through Choya; merge after the latency loop closes.
5. Phase 7 → gates, changelog, conditional DLL.

### Commit policy

One local commit per checkpoint after focused tests and clippy pass. No push, no release, no in-game acceptance claim except under the spec's release condition.

---

## Notes

- Every `[P]` task touches different files or independent test functions.
- Re-resolve symbols with SymForge before each edit; run `analyze_file_impact` after changing `WvwCombatReport`, `active_normalized_effects` or `ToolContext`.
- Never copy fixture values into `crates/optimizer/data/`.
- If a hypothesis is refuted by its experiment, record the refutation and move on; do not manufacture a fix.
