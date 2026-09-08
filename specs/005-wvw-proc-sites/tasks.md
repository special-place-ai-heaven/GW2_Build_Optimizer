# Tasks: WvW proc firing sites (Sprint 2, CONN-03 C)

**Input**: Design documents from `specs/005-wvw-proc-sites/`

**Prerequisites**: plan.md, spec.md, research.md (R1–R10), data-model.md, contracts/normalized-effect-schema.md, contracts/wvw-report.md, quickstart.md

**Tests**: Required by the spec. FR-006 demands a positive, a negative and a timing control for every mechanic, each **seen failing** under a scripted disable before the edit (Sprint 1 discipline). Test tasks therefore come first in every story phase and their failure must be observed and quoted before the implementation task is started.

**Organization**: Phases follow the spec's user stories in priority order. Every story is independently testable on the synthetic Reaper fixture; only the cached-build test needs `dev.cfg`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelisable (different files, no dependency on an unfinished task)
- **[Story]**: US1–US6 from spec.md
- Paths are repository-relative. Run `cargo test` filters with `MSYS_NO_PATHCONV=1` when they contain `::`. Python edits to `.rs`/`.json` use `io.open(..., encoding="utf-8", newline="")` for read and write, from the repo root.

## Standing rules (apply to every task)

- Never edit `crates/optimizer/src/prompts.rs`, `crates/optimizer/src/llm/`, `crates/optimizer/examples/choya_live.rs`. Never change a scoring constant, gate threshold, normalisation value or objective profile (FR-010, FR-013).
- Never copy a fixture id or value into `data/` (FR-009).
- No `git push`, no version bump, no release. Commit locally after each story checkpoint with `git add -f specs/005-wvw-proc-sites` when spec files changed.
- Before changing a type, run SymForge `find_references` on it and list the call sites in the commit message body.
- A "seen-failing" task ends with the quoted failure text saved to `docs/audit/sprint2-failures.md` (created by T003) under the control's name.

---

## Phase 1: Setup

**Purpose**: the disable/run/restore harness and the evidence file the controls write to.

- [X] T001 Generalise the Sprint 1 disable script into `docs/audit/disable_and_run.py`: arguments `<control-name>`; each control is a table entry of `(file, exact_anchor_text, replacement, test_filter)`; the script copies the file to a scratch path, applies the replacement in memory (assert count == 1), writes with `newline=""` and UTF-8, runs `cargo test -p gw2-optimizer --lib <filter> -- --nocapture`, restores the file from the copy (never `git checkout`), and prints the test's failure block. Entries are added by the story tasks below.
- [X] T002 [P] Add a fixture variant module in `crates/optimizer/src/rotation/reaper_fixture.rs`: `build_with_set_two_fire()` (Sigil of Fire on set 2 main seat, Force on set 1 main, via `set_sigil_seats`), `build_with_zero_precision()` (Marauder replaced by a precision-free stat set; add itemstat 99437 "Soldier" to `db()`), `records_with_threshold_and_stack()` (Scholar-shaped OnHealthThreshold record with `health_threshold {above, 90}` on synthetic rune id 99001 and a Thief-shaped stacking record on synthetic relic id 99002), and `opener_with_swap()` (Gravedigger, then a set-2 weapon skill; add Axe 2 "Ghastly Claws" 30040 with a "Life Force" Percent fact of 12). Keep every id synthetic; document the opener re-order (generators before shroud) in the module doc comment.
- [X] T003 [P] Create `docs/audit/sprint2-failures.md` with one heading per control named in this file (empty bodies) so seen-failing tasks have a fixed place to paste evidence.

---

## Phase 2: Foundational

**Purpose**: schema, report and trace additions every story reads; must be green before any story starts.

- [X] T004 Add `HealthThreshold { above: bool, percent: FactualValue<f64> }`, `TriggerScope { Any, WeaponSkillWithRecharge }` and the three optional `serde(default)` fields `health_threshold`, `proc_chance`, `trigger_scope` to `NormalizedEffect` in `crates/optimizer/src/data/normalized_effects.rs` per `contracts/normalized-effect-schema.md`; extend `minimal_effect()` and the full round-trip test; add `test_validation_health_threshold_required_for_on_health_threshold` and `test_validation_max_stacks_requires_duration` and the validation code they exercise; confirm every existing `test_validation_*` and `test_embedded_effects_load_successfully` still pass.
- [X] T005 [P] Add the eight `TraceKind` variants (`ConditionalActivated`, `ConditionalExpired`, `StackGained`, `ComboResolved`, `ShroudEntered`, `ShroudExited`, `ShroudRefused`, `LifeForceGained`) and the report fields `proc_trials: Vec<ProcTrial>` and `shroud_refusals: Vec<String>` (with `ProcTrial { source, mean, min, max }`) to `crates/optimizer/src/rotation/wvw_timeline.rs`; default both to empty in `report()`; run `find_references(WvwCombatReport)` and update every literal constructor (referee tests, engine tests).
- [X] T006 [P] Add `pub enum Bar { Weapon, Shroud, Slot }` and `pub bar: Bar` to `RotationSkill` in `crates/optimizer/src/rotation/mod.rs` with a `RotationSkill::bar_for(weapon_set: u8) -> Bar` helper (`0 → Slot`, else `Weapon`); run `find_references(RotationSkill)` and set the field in every literal constructor (builder, fixture, tests) using the helper, so behaviour is unchanged.
- [X] T007 [P] Create `data/formulas/shroud.json` exactly as in `contracts/normalized-effect-schema.md` (Death 10574, Reaper 30792 with WvW drain 5.0, Harbinger 62567 and Ritualist 77238 as `null`, read date 2026-09-08) and a loader `crate::data::shroud::table()` in a new `crates/optimizer/src/data/shroud.rs` (embedded with `include_str!`, parsed once, `null` → `None`); register the module in `crates/optimizer/src/data/mod.rs`; test that the four ids load and the two nulls are `None`.
- [X] T008 Add `ResourceKind::LifeForce` in `crates/optimizer/src/rotation/wvw_timeline.rs` with `resource_cap` and `initial_resources` arms (cap computed from `params.max_health × 0.69`, so `resource_cap` takes `&SimParams`; initial 0.0); extend `SkillResourceRule` with `gain_on_use: f64`, `entry_floor: f64`, `drain_per_second: f64`, `enters_shroud: bool`, `exits_shroud: bool` (all default); run `find_references(SkillResourceRule)` and update `wvw_resource_rules` literals in `crates/optimizer/src/engine.rs` and every test literal with `..Default::default()` (derive `Default`).
- [X] T009 Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p gw2-optimizer` and confirm counts equal Sprint 1 plus the new foundational tests; commit "Sprint 2 foundation: record fields, trace kinds, Bar, life force kind, shroud table".

**Checkpoint**: every existing test green; no behaviour changed yet.

---

## Phase 3: User Story 1 - An on-crit proc changes the fight (Priority: P1) 🎯 MVP

**Goal**: OnCrit records fire from landed hits as an expected-value mass process in ranking and as seeded trials in the trace; the on-crit sigil leaves the coverage line.

**Independent Test**: `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_oncrit` passes; the inverted Sprint 1 unsupported control passes; trials bracket the expected value.

### Seen-failing controls for US1

- [X] T010 [US1] Invert `reaper_unsupported_oncrit_is_named_not_zeroed` in `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) into `reaper_oncrit_positive_control_fires_from_crits`: assert at least one `ProcFired` event whose source is the fixture's Sigil of Fire, and that no `unmodeled_sources` entry contains "Sigil of Fire". Run it; paste the failure (today it fails on the first assertion) into `docs/audit/sprint2-failures.md`.
- [X] T011 [P] [US1] In `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) add `reaper_oncrit_zero_precision_never_fires` (fixture `build_with_zero_precision()`: zero `ProcFired` for the sigil, `total_damage` equals the no-sigil run within 1e-9) and `reaper_oncrit_icd_bounds_rate` (opener with a 2-hit Gravedigger inside 5 s: at most one full-mass fire per 5 s window, `ProcSkippedIcd` present) in that module. Both fail today because nothing fires; paste both failures.
- [X] T012 [P] [US1] In `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) add `reaper_oncrit_trials_bracket_expected_value`: with `trace: true`, `report.proc_trials` has an entry for the sigil whose `mean` is within 1.0 of the expected-value fire count derived from the trace's `ProcFired` weights (sum of `×p`), and `min ≤ mean ≤ max`. Fails today (`proc_trials` empty); paste the failure.

### Implementation for US1

- [X] T013 [US1] In `crates/optimizer/src/rotation/simulator.rs` add `pub(super) fn crit_chance_fraction(precision, crit_chance_bonus_pct) -> f64` extracted from `strike_crit_factor_with_bonus` (same formula, clamped), and make `strike_crit_factor_with_bonus` call it so numbers are unchanged.
- [X] T014 [US1] In `crates/optimizer/src/rotation/wvw_timeline.rs`: extend `ProcSpec` with `weapon_set: u8` (set 0 for now), `proc_chance: f64`, `mass: f64`, `scope: TriggerScope`; in `load_normalized_effects` accept `TriggerRule::OnCrit` as supported and read `proc_chance` (default 1.0) and `trigger_scope`; keep the "(on-crit)" unmodeled push only for records whose value is unresolved.
- [X] T015 [US1] In `crates/optimizer/src/rotation/wvw_timeline.rs::trigger_procs`, add a `weight: f64` parameter: `OnHit` callers pass 1.0; the new `OnCrit` call in `apply_skill_effect`'s `StrikeDamage` arm passes `crit_chance_fraction(...) × hit_count` (Fury bonus included as the damage line does). Implement the mass process from data-model.md: if ready, apply `p × effect` where `p = weight × proc_chance`, `mass += p`; when `mass ≥ 1.0` subtract 1.0 and set `next_ready_ms = now + icd`; trace `ProcFired` with detail `"{category:?} ×{p:.2}"`. Apply scaling to the `StrikeDamagePct`, healing and `apply_operation` arms (operation amounts scaled by `p` through `amount_value`; boon/condition stacks rounded to nearest, minimum 0, and documented as an approximation in a `// ponytail:` comment).
- [X] T016 [US1] Add the coefficient arm for `ProcEffect → StrikeDamagePct` with `value ≤ 2.0` in `crates/optimizer/src/rotation/wvw_timeline.rs::trigger_procs`: `damage = UNEQUIPPED_WEAPON_STRENGTH (690.5) × params.power / reference_armor() × value × params.strike_mult`, no crit factor, with the wiki comment and read date; values `> 2.0` keep the existing percent path.
- [X] T017 [US1] In `crates/optimizer/src/rotation/wvw_timeline.rs` add the trial pass: an in-module `struct XorShift64Star(u64)` PRNG; a `CritMode { Expected, Seeded(XorShift64Star) }` field on `Timeline`; in seeded mode `OnCrit` procs draw Bernoulli(`p`) and fire in full; in `evaluate_wvw_timeline`, when `input.trace` is true, run the timeline eight more times with seeds `0x9E37_79B9_7F4A_7C15 + k` and fill `proc_trials` (mean/min/max of `ProcFired` counts per source); `trace: false` never constructs a seeded timeline (assert in `trace_is_empty_unless_requested` that `proc_trials` is empty).
- [X] T018 [US1] Add entry `oncrit` to `docs/audit/disable_and_run.py` (anchor: the `self.trigger_procs(TriggerRule::OnCrit, …)` line → comment it out; filter `reaper_oncrit_positive_control_fires_from_crits`); run it and confirm the control fails when disabled and the file is restored byte-identical.
- [X] T019 [US1] Run `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_` and the full optimizer suite; confirm `unsupported_proc_category_counts_once_per_source` and every Sprint 1 experiment still pass; commit "WvW: on-crit procs fire as an expected-value process; seeded trials in the trace".

**Checkpoint**: US1 delivers SC-003 and SC-009 on the fixture.

---

## Phase 4: User Story 2 - Swapping weapons swaps sigils (Priority: P2)

**Goal**: records for all four sigil seats load; only the held set's sigils fire; cooldowns persist across swaps; set-2 sigils with records leave the coverage line.

**Independent Test**: `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_swap` passes.

### Seen-failing controls for US2

- [X] T020 [US2] Add `reaper_swap_loads_set_two_sigils` in `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) using `build_with_set_two_fire()` and `opener_with_swap()` with a synthetic OnHit record on the set-2 sigil: every `ProcFired` for that sigil has `t_ms` greater than the `WeaponSwap` event's `t_ms`; then with the same sigil moved to set 1 main, every such event precedes the swap. Fails today (no set-2 events at all); paste the failure.
- [X] T021 [P] [US2] In `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) add `reaper_swap_keeps_icd_across_sets` (same record on both sets, ICD 10 s, opener hits on set 1 then swaps and hits on set 2 within 10 s: exactly one `ProcFired` and at least one `ProcSkippedIcd` for it) and `reaper_set_two_sigil_leaves_coverage_line` (`build_with_set_two_fire()` through `referee::evaluate_validated_build_with`: no `unmodeled_sources` entry names the set-2 sigil). Both fail today; paste the failures.

### Implementation for US2

- [X] T022 [US2] In `crates/optimizer/src/validation.rs` add `ValidatedBuild::sigil_ids_by_set(&self) -> [Vec<u32>; 2]` (from `sigil_seats` when recorded; dense fallback: first two → set 1, rest → set 2) with a unit test beside `sigil_seats_keep_their_holes`; leave `active_sigil_ids` unchanged for stats.
- [X] T023 [US2] In `crates/optimizer/src/engine.rs::active_normalized_effects`, select records for both sets and return the seat set per sigil: change the return to `(Vec<(&NormalizedEffect, u8)>, Vec<String>)` where `u8` is the weapon set (0 for non-sigils); count both sets' sigils in `equipped` so a stowed sigil without a record is still named; update the WvW call site and `WvwTimelineInput.active_effects` to carry the set (`&[(&NormalizedEffect, u8)]`); run `find_references(active_normalized_effects)` and fix the Sprint 1 tests that read it.
- [X] T024 [US2] In `crates/optimizer/src/rotation/wvw_timeline.rs`: `load_normalized_effects` stores the set in `ProcSpec.weapon_set`; `trigger_procs` skips specs with `weapon_set != 0 && weapon_set != held_set` where `held_set` is a new parameter defaulting to `self.active_weapon_set`; `ScheduledHit` gains `weapon_set: u8` captured in `start_cast`, and `land_scheduled_hits` passes it as `held_set` so mid-channel swaps keep the cast's sigils.
- [X] T025 [US2] Add entry `swap` to `docs/audit/disable_and_run.py` (anchor: the `weapon_set != held_set` skip → replace with `false` so set 2 never loads; filter `reaper_swap_loads_set_two_sigils`); run and confirm.
- [X] T026 [US2] Run the `reaper_` suite, the Sprint 1 negative control `reaper_negative_control_wrong_mode_and_stowed_set` (its stowed-set branch must still pass: a stowed sigil still does not fire before the swap) and the full optimizer suite; commit "WvW: set-2 sigils load and fire only while held".

**Checkpoint**: US2 delivers SC-001's second name.

---

## Phase 5: User Story 3 - Conditional bonuses apply only when their condition holds (Priority: P3)

**Goal**: health-threshold and stacking bonuses apply per strike only while true, without double counting the parser's flattened value; PvE and PvP output unchanged.

**Independent Test**: `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_scholar reaper_thief reaper_unresolved_conditional pve_output_unchanged` passes.

### Seen-failing controls for US3

- [X] T027 [US3] Add `reaper_scholar_applies_only_above_threshold` in `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) with `records_with_threshold_and_stack()`: run A (no incoming damage) has a `ConditionalActivated` event at `t_ms == 0` and every `HitLanded` amount is the run-B amount × 1.05 within 1e-9; run B (enemy pressure event that takes the player to 80 percent at a known tick) has a `ConditionalExpired` event at that tick and later hits carry no bonus. Fails today (no such events, bonus flattened everywhere); paste.
- [X] T028 [P] [US3] In `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) add `reaper_thief_stacks_cap_and_expire` (five qualifying weapon-skill hits raise stacks to 5 with `StackGained` events, a sixth refreshes without a sixth stack, after 6 s of no qualifying hit the multiplier returns to 1.0) and `reaper_unresolved_conditional_stays_named` (threshold `percent` unresolved: the rune stays in `unmodeled_sources` as "(unresolved value)" and no conditional event appears). Both fail today; paste.
- [X] T029 [P] [US3] Add `pve_output_unchanged_by_conditional_tagging` in `crates/optimizer/src/referee.rs`: evaluate the fixture in `pve_scenario()` and assert `realized`, `stats` and the three combat tiers equal a pinned snapshot captured before T031 (record the numbers in the test with a comment naming the commit). This test must pass before and after; it is the FR-010 guard, not a seen-failing control.

### Implementation for US3

- [X] T030 [US3] In `crates/optimizer/src/combat.rs`: add `ConditionalClause { source_id: u32, value: f64, above: bool, percent: f64 }` and `DamageModifiers.conditional_strike: Vec<ConditionalClause>`; in `parse_percent_clauses` (and the rune/relic callers that know the source id) detect "above N%" / "below N%" health wording via `text_util::percent_clauses` and push the clause while still applying the flattened value exactly as today; unit test on the Scholar string.
- [X] T031 [US3] In `crates/optimizer/src/rotation/wvw_timeline.rs`: add `ConditionalSpec` per data-model.md and `conditional_specs: Vec<ConditionalSpec>` on `Timeline`; `load_normalized_effects` routes `OnHealthThreshold`/`Conditional` records with `health_threshold` and `TriggeredEffect → StrikeDamagePct` into threshold specs, and `OnHit` records with `max_stacks` into stacking specs (scope from the record); `strike_conditional_mult()` evaluated in the `StrikeDamage` arm; threshold state re-evaluated there and in `expire_timed_state` with `ConditionalActivated`/`ConditionalExpired` events; stacking gains on qualifying hits (`Bar::Weapon` skill with `cooldown_ms > 0` or a resource rule when scope is `WeaponSkillWithRecharge`), `StackGained` events, expiry in `expire_timed_state`.
- [X] T032 [US3] In `crates/optimizer/src/engine.rs` WvW branch of `simulate_prepared_with`: clone `params` for the timeline and, for each `conditional_strike` clause whose `source_id` has a loaded threshold record, divide `strike_mult` by `1 + value/100`; leave `params` for the PvE/PvP simulator untouched; add a unit test that the divide-out happens only when the record loaded.
- [X] T033 [US3] Add entries `threshold` (anchor: the threshold evaluation in `strike_conditional_mult` → always true; filter `reaper_scholar_applies_only_above_threshold`) and `stack` (anchor: the `min(max_stacks)` cap → remove; filter `reaper_thief_stacks_cap_and_expire`) to `docs/audit/disable_and_run.py`; run both and confirm.
- [X] T034 [US3] Run the `reaper_` suite, `reaper_parity_referee_matches_optimize_suggestion`, `pve_output_unchanged_by_conditional_tagging`, the `parser_consistency_tests` and the full optimizer suite; commit "WvW: health-threshold and stacking bonuses apply per strike; parser tags conditionals".

**Checkpoint**: US3 delivers SC-001's third name and the FR-010 guard.

---

## Phase 6: User Story 4 - A dark field combo produces its effect (Priority: P4)

**Goal**: dark field + whirl/leap/blast/projectile produce their outcomes; the "dark field" degraded note disappears.

**Independent Test**: `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_dark reaper_expired_field` passes.

### Seen-failing controls for US4

- [X] T035 [US4] Add `reaper_dark_whirl_life_steals` in `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`): opener Nightfall then Soul Spiral: a `ComboResolved` event with detail containing "dark field + whirl finisher", `report.healing > 0` attributable to it (compare with the field removed), and no `unmodeled_sources` entry containing "dark field". Fails today; paste.
- [X] T036 [P] [US4] In `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) add `reaper_expired_field_makes_no_combo`: Soul Spiral cast after Nightfall's `duration_ms` has elapsed (insert a filler skill): no `ComboResolved` event and `combo_activations` unchanged. Passes today by accident (no dark combo ever resolves), so record it as a regression guard, not seen-failing.

### Implementation for US4

- [X] T037 [US4] Re-read the wiki `Combo` page for the dark row (leeching bolts, dark aura, area blindness, life siphon) with crawl4ai `md`, note the read date, and add the numbers as constants with the wiki comment in `crates/optimizer/src/rotation/wvw_timeline.rs::resolve_combo` dark arm: whirl → `record_damage(per-bolt × bolts)` plus `heal(per-bolt heal × bolts, healing-power scaled like the water arm)`; leap → `apply_buff("Dark Aura", 1, duration, true)`; blast → `apply_defense(CoverKind::Blind, duration, 1, false)` as the smoke arm; projectile → single life siphon; each traces `ComboResolved` with `"{field} field + {finisher} finisher → {outcome}"`; unstated numbers keep `note_unmodeled`.
- [X] T038 [US4] Add entry `dark` to `docs/audit/disable_and_run.py` (anchor: the `field_type.contains("dark")` arm body → `note_unmodeled(unmodeled("dark field"))`; filter `reaper_dark_whirl_life_steals`); run and confirm.
- [X] T039 [US4] Run the `reaper_` suite and `reaper_fixture_evaluates_without_errors` (its `combo_activations ≥ 1` assertion still holds; update its degraded-count expectation to zero); commit "WvW: dark field combos resolve".

**Checkpoint**: US4 delivers SC-001's fourth name.

---

## Phase 7: User Story 5 - The first verified cases are real, not fixture values (Priority: P5)

**Goal**: the shipped WvW records for Sigil of Fire, Rune of the Scholar, Relic of the Thief and the cached Reaper build's triggered traits are correct in shape and sourced with dates; a real cached build executes them.

**Independent Test**: `test_embedded_effects_load_successfully` passes on the rewritten file; the `#[ignore]` cached-build test lists the three sources as executed.

- [ ] T040 [US5] Rewrite three records in `data/normalized_effects/2026-01-13/wvw.json` per `contracts/normalized-effect-schema.md`: `sigil:24548:0` (`value 0.85`, `evidence_level Factual`, `internal_cooldown 5.0`, `uptime_model Unknown`, source with read date), `rune:24836:1` (add `health_threshold {above: true, percent: 90.0}`), `relic:100916:0` (`trigger_rule OnHit`, `category TriggeredEffect`, `inner_category StrikeDamagePct`, `value 1.0`, `max_stacks 5`, `effect_duration 6.0`, `trigger_scope WeaponSkillWithRecharge`, source with read date). Use a Python script with UTF-8 and `newline=""`; keep key order and indentation; run the loader tests.
- [ ] T041 [US5] Add `#[ignore]` test `reaper_cached_build_has_recorded_sources` in `crates/optimizer/src/referee.rs`: through `gw2_api::dev_config`, load the cache, find a Necromancer character tab whose elite spec is Reaper and whose game mode tab is WvW (or report "no Reaper WvW tab in cache" and return), resolve it to a `ValidatedBuild`, evaluate in the WvW scenario, and assert that each of Sigil of Fire, Rune of the Scholar and Relic of the Thief that the build carries is absent from `unmodeled_sources` and present in the trace; print the list of equipped triggered traits without a record.
- [ ] T042 [US5] From T041's printed list, write records for the cached Reaper build's traits whose wiki page states an OnCrit, OnHit or health-gated trigger, into `data/normalized_effects/2026-01-13/wvw.json`, each with the wiki URL, read date and `Factual`/`Heuristic` as the page supports; leave traits without stated numbers unrecorded (they stay on the coverage line by design).
- [ ] T043 [US5] Add `records_this_sprint_carry_read_dates` in `crates/optimizer/src/data/normalized_effects.rs`: every WvW record with `health_threshold`, `proc_chance`, `trigger_scope` or a coefficient-form `ProcEffect` has a `source` containing "(read 20"; commit "Records: Fire, Scholar, Thief and the cached Reaper build's triggered traits, wiki-dated".

**Checkpoint**: US5 delivers SC-005 (or documents a vacuous pass if the cache has no Reaper WvW tab).

---

## Phase 8: User Story 6 - Life force governs shroud (Priority: P6)

**Goal**: life force is generated from facts, capped, required to enter shroud, drained in shroud, and ends shroud at zero; the shroud bar is prepared from `transform_skills`; refusals reach `quality_reasons`; completeness is derived, not listed.

**Independent Test**: `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_shroud reaper_life_force resource_model_completeness shroud_bar` passes.

### Seen-failing controls for US6

- [ ] T044 [US6] Add `reaper_shroud_refused_without_life_force` in `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`): opener that presses Reaper Shroud first with life force 0: a `ShroudRefused` event, no `ShroudEntered`, `report.shroud_refusals` contains "needs 10% life force", and the shroud skills never appear in `HitLanded`. Fails today (shroud is an ordinary skill); paste.
- [ ] T045 [P] [US6] In `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) add `reaper_life_force_gain_capped` (Ghastly Claws with a 12 percent fact raises the pool by `0.12 × cap` with a `LifeForceGained` event; repeated past the cap stays at cap) and `reaper_shroud_drains_and_exits` (enter at 30 percent with Reaper drain 5 percent/s: `ShroudExited` with detail "life force 0" at 6 s ± one tick; a channel pending at that tick is `CastInterrupted`; `heal()` during shroud does not change `player_health`; incoming damage in shroud reduces the pool at 50 percent and overflows to health) in that module. Both fail today; paste.
- [ ] T046 [P] [US6] Add `shroud_bar_is_prepared_from_transform_skills` in `crates/optimizer/src/rotation/builder.rs` tests: a test `GameDb` with Death Shroud (10574, `transform_skills` listing ids with slots `Downed_1..4` and `Weapon_5`, `specialization` 34 for five of them and `None` for five core ones), Reaper's Shroud 30792 (`flip_skill` 30961): preparing a Reaper build yields five `Bar::Shroud` skills with the Reaper ids in order 1..5, the entry and exit skills as `Bar::Slot`, and none of the core-shroud ids; preparing a core Necromancer yields the core five. Fails today (no shroud skills prepared); paste.
- [ ] T047 [P] [US6] Add `resource_model_completeness_matches_previous_list` in `crates/optimizer/src/engine.rs` tests: nine minimal builds (one per profession, each with its mechanic skill and a weapon skill) evaluated through `wvw_resource_rules`: Thief, Revenant, Warrior, Mesmer and Necromancer complete; Guardian, Elementalist, Engineer, Ranger incomplete. Fails today on Necromancer; paste.

### Implementation for US6

- [ ] T048 [US6] In `crates/optimizer/src/rotation/builder.rs`: after the `Profession_` slot pass, if the profession is Necromancer, find the entry skill (`Profession_1`, `specialization` matching the equipped elite or `None`), take Death Shroud's (10574) `transform_skills`, keep those whose `specialization` equals the equipped elite id (or `None` when no shroud-owning elite is equipped), map slot `Downed_1..4 → 1..4` and `Weapon_5 → 5`, and emit them as `RotationSkill { bar: Bar::Shroud, weapon_set: 0, .. }` with effects from facts via `extract_effects_for_context`; mark the entry skill and its `flip_skill` as `Bar::Slot`; Scourge (no entry skill) emits nothing extra.
- [ ] T049 [US6] In `crates/optimizer/src/engine.rs::wvw_resource_rules`: for Necromancer, read Percent facts whose `text` starts with "Life Force" ("Life Force" → `gain_on_use`, "Life Force Per Hit" → `gain_on_hit`, "per 3 Seconds"/"When Ending" → unresolved: no rule, source named "(unresolved value)") converting percent of pool to absolute with `0.69 × max_health`; emit the entry rule from `data::shroud::table()` (`entry_floor 10%`, `drain_per_second` for the mode, `enters_shroud`), the exit rule (`exits_shroud`), and for shroud skills with a `cost` a `LifeForce` cost rule; when the table row is `null`, emit no drain and name the shroud "(unresolved value)" so the model is incomplete.
- [ ] T050 [US6] In `crates/optimizer/src/engine.rs` replace the profession allowlist at the end of `wvw_resource_rules` with the derived rule from research R6: complete iff rules are non-empty and every rotation skill that names a resource (has `initiative`, has `cost` on a `Profession_` slot, has a Life Force fact, or is `Bar::Shroud`) has a rule; keep the four current professions' outcomes as T047 pins them.
- [ ] T051 [US6] In `crates/optimizer/src/rotation/wvw_timeline.rs`: add `ShroudState { entry_skill_id, entered_at_ms, drain_per_second, damage_reduction }` and `in_shroud: Option<ShroudState>`; `pick_skill` and the opener path admit `Bar::Shroud` only in shroud and `Bar::Weapon` only outside; `start_cast` on an `enters_shroud` rule checks `entry_floor`, refuses with a `ShroudRefused` event and a `shroud_refusals` line (`"{name} needs {floor}% life force, had {have}%"`) and skips the cast; on success `ShroudEntered`; `exits_shroud` rule → exit with detail "exit skill"; `regenerate_resources` applies the drain per tick and forces exit at zero (detail "life force 0") cancelling any pending cast the way `receive_control` does; incoming damage in shroud goes to the pool × (1 − reduction) with overflow to `player_health`; `heal()` returns early in shroud; `gain_on_use` credited in `resolve_pending_cast` with `LifeForceGained`; `gain_on_hit` through the existing path.
- [ ] T052 [US6] In `crates/optimizer/src/referee.rs::evaluate_validated_build_with`, append each `report.shroud_refusals` line to `quality_reasons` as a `DataQualityReason` with field `wvw_timeline.resources` and severity matching the existing resource-ledger reason; drop the "outside the bounded resource ledger" reason when `resource_model_complete` is true (already the rule) so the Necromancer no longer carries it (SC-008).
- [ ] T053 [US6] Update `crates/optimizer/src/rotation/reaper_fixture.rs`: give the fixture's generator skills Life Force Percent facts, give Reaper Shroud 30012 the `flip_skill` exit and the shroud skills `Bar::Shroud` through the builder path (or set `bar` directly where the fixture constructs `RotationSkill`s), re-order `opener()` to Gravedigger, Death Spiral, Well of Suffering, Reaper Shroud, Shroud 4, Shroud 1, and fix every Sprint 1 experiment expectation that the re-order changes (trace anchors in the audit are updated in T058).
- [ ] T054 [US6] Add entries `shroud_floor` (anchor: the `entry_floor` comparison → `true`; filter `reaper_shroud_refused_without_life_force`) and `drain` (anchor: the drain subtraction → `0.0`; filter `reaper_shroud_drains_and_exits`) to `docs/audit/disable_and_run.py`; run both and confirm.
- [ ] T055 [US6] Run the `reaper_` suite, `shroud_bar_is_prepared_from_transform_skills`, `resource_model_completeness_matches_previous_list`, the Sprint 1 experiments and the full workspace tests; if the diff exceeds 600 lines split into two commits ("Life force resource and rules" then "Shroud bar and state transitions"); otherwise commit "WvW: life force and one shroud shape for every Necromancer specialisation".

**Checkpoint**: US6 delivers SC-008; all six stories complete.

---

## Phase 9: Polish and cross-cutting

- [ ] T056 Add `reaper_results_repeat_identically` in `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`): ten evaluations of the fixture with `trace: true` give identical `WvwCombatReport` fields and identical `proc_trials` (SC-004); add `reaper_trace_fits_under_cap` asserting `trace_truncated == false` for the full fixture opener with every mechanic enabled.
- [ ] T057 [P] Time the default optimizer test run three times (`cargo test -p gw2-optimizer --lib 2>&1 | grep "test result"`), record the harness time beside the Sprint 1 baseline (16.80 s) in `docs/simulator-connection-audit.md` section 8, and mark any experiment over 1 s as `#[ignore]` with an on-demand note (FR-011).
- [ ] T058 Write `docs/simulator-connection-audit.md` section 8 "Sprint 2 experiments": one row per control (name, mechanism disabled, quoted failure from `docs/audit/sprint2-failures.md`, what passes now), the updated trace anchors for the re-ordered opener, the completeness-rule table, and cross-references closing CONN-00-06, CONN-00-07, CONN-00-09, CONN-01-01 (WvW), CONN-01-02 (WvW); note CONN-01-03 (PvE/PvP execution) and CONN-01-06 remain open.
- [ ] T059 [P] Add a "### What the simulator now simulates in WvW" subsection under the current unreleased version in `CHANGELOG.md` (five bullets: on-crit procs, set-2 sigils, conditional bonuses, dark combos, life force and shroud); keep the semver heading so `embedded_changelog_has_at_least_five_entries` passes; no version bump.
- [ ] T060 Sweep the diff for machine paths, `poslj`, `scratchpad`, `DEBUG`, `mock`, fixture ids inside `data/`, and any edit under `prompts.rs`, `llm/`, `choya_live.rs`; run quickstart §6 gates (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --release`) and quickstart §1–§5; record results in the completion report.
- [ ] T061 Update memory `project-004-simulator-trust.md` (or a new `project-005-wvw-proc-sites.md`) with the sprint state, the seen-failing evidence location, and the open findings; commit all spec changes with `git add -f specs/005-wvw-proc-sites`; no push.

---

## Dependencies and execution order

- **Phase 1 → Phase 2 → stories**: T004–T008 block every story. T004 (schema) is needed by US1/US3/US5; T005 (trace/report) by every story; T006 (`Bar`) by US3 scope and US6; T007/T008 by US6 only, but they are cheap and land together.
- **US1 (P1)** depends only on Phase 2. **US2** depends on US1's `ProcSpec` extension (T014) for the `weapon_set` field. **US3** is independent of US1/US2 in behaviour but shares `load_normalized_effects`; sequence after US2 to avoid merge churn. **US4** is independent of all. **US5** needs T004 and benefits from US1–US3 for the cached-build test to show executions. **US6** needs T006–T008 and is last because it touches the builder and re-orders the fixture opener.
- **Polish** after all stories; T058 needs every failure quote.

## Parallel opportunities

- Phase 1: T002 ∥ T003 while T001 is written.
- Phase 2: T005 ∥ T006 ∥ T007 after T004 lands (different files); T008 after T005.
- Within each story the seen-failing controls marked [P] are written together, then run in one `cargo test` invocation.
- US4 (T035–T039) can be drafted in parallel with US2/US3 by a second agent on a worktree with its own target directory; rebase before commit.
- Polish: T057 ∥ T059 while T058 is written.

## Parallel example: User Story 1

```text
Together: T011 "reaper_oncrit_zero_precision_never_fires + reaper_oncrit_icd_bounds_rate"
          T012 "reaper_oncrit_trials_bracket_expected_value"
Then run once: MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_oncrit
Paste all three failures, then T013 → T014 → T015 → T016 → T017 in order (same function chain).
```

## Implementation strategy

- **MVP = Phase 1 + Phase 2 + US1.** It closes CONN-00-06, is the plan's reference case, and proves the expected-value process against seeded trials (SC-003, SC-009).
- **Increment 2 = US2 + US3 + US4.** Three independent firing sites, each one commit, each with its controls. After this SC-001 holds on the fixture.
- **Increment 3 = US5.** Real records and the cached-build test make the improvement reach a player.
- **Increment 4 = US6.** Largest item; may split into two commits.
- Stop at any checkpoint: every earlier increment stays shippable behind the Sprint 1 release gate.

## Notes

- Every control's failure is quoted before its implementation task starts; a control that cannot be made to fail is a finding for the audit, not a test to keep.
- Disable scripts restore from copies, never from git, because the tree holds uncommitted work between tasks.
- `RotationSkill`, `SkillResourceRule`, `ProcSpec`, `ScheduledHit`, `WvwCombatReport`, `NormalizedEffect`, `DamageModifiers` all change shape: `find_references` first, `analyze_file_impact` after.
