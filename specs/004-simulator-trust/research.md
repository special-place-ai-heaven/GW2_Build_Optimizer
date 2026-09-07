# Research: Simulator trust (Sprint 1)

Date: 2026-09-07 | Feature: 004-simulator-trust | Source tree inspected: worktree `.claude/worktrees/choya-work` at `09740ce` (the plan's target checkout; line numbers cited there and will move). Five read-only investigations; every claim below was verified against the worktree file, not the SymForge index of the main checkout.

## R1. Full-build tool for Choya: extend `score_build`

Decision: extend the existing `score_build` tool with an optional `build` object shaped exactly like the plate the model already emits (`prompts::GeminiBuildResponse`), fed through `validation::validate_gemini_build` and then `referee::evaluate_validated_build_with`. No new tool name.

Rationale: none of the seven Choya tools reaches the referee. `score_build` (gemini_tools.rs ~1072) is prefix-only, ignores traits (`DamageModifiers::default()`), and uses the pre-referee closed-form `score_with_weights`; its name already promises a verdict and nothing depends on its output shape. `get_build_synergy_report` is descriptive only; `simulate_rotation` answers a different question (skill list on an open dummy) and stays, labelled. The plate parser and the referee are the exact pair chat_flow.rs already runs at acceptance (~1196-1207), so the tool reuses the app's path rather than a second stat sheet.

Plumbing: `ToolContext` (gemini_tools.rs ~133) gains `scenario: &ScenarioSpec`; both builders (chat_flow.rs ~396, optimize_flow.rs ~686) have a scenario in scope. Omitted `build` returns an explicit error, never the equipped loadout. Validation fills (`complete_major_trait_columns`, `fill_revenant_legends`) are surfaced as `warnings` in the result. Scenario mode comes from `ctx.balance_ctx`, not from the argument, so a PvP amulet cannot be scored against WvW.

Bounding: the referee runs a gate sim plus a 60 s flow sim with no cancellation probe. Smallest bound: a per-turn cap of three full-build evaluations per chat request (counter in the tool callback next to the existing stale-epoch check at chat_flow.rs ~480), returning `{"error":"evaluation budget spent"}` after that; the 600 s rotation clamp precedent (gemini_tools.rs ~1609) is the model. Threading `is_cancelled` into `simulate_prepared` is deferred; recorded as a finding.

Alternatives rejected: a new `evaluate_build` tool (duplicates `score_build`'s stated purpose, widens the tool list the latency experiment is measuring); extending `simulate_combat` (its `extract_damage_modifiers` path would double-apply rune/sigil bonuses next to the referee's own modifier derivation).

## R2. Coverage qualification: carry names, render one line

Decision: keep the names of unmodeled effect sources instead of a bare count, build one shared reason string, and show it as one visible line beside the existing quality marker plus in Choya's evidence.

Rationale: `engine::active_normalized_effects` computes `equipped − modeled` and keeps only `.count()` (~1642); `wvw_timeline::load_normalized_effects` does `+= 1` for OnCrit / OnHealthThreshold / Conditional / non-skill OnSkillUse (~639-645). The referee turns that into `DataQualityReason { field: "wvw_timeline.effects", explanation: "{n} equipped or triggered effect sources are not yet represented by timed rules" }` (referee.rs ~1227-1239), duplicated verbatim in engine.rs ~1880-1892. The overlay draws quality only as a one-word coloured marker with reasons inside a hover tooltip (comparison.rs ~1081-1113). chat_flow.rs never reads `quality`/`quality_reasons`; a Choya plate's suggestion is built with `..Default::default()` (~924) and renders "Verified" regardless.

Change set: `WvwCombatReport.unmodeled_sources: Vec<String>` (count becomes `.len()`); `active_normalized_effects` returns the difference set resolved to names; one helper `coverage_reason(...) -> Option<DataQualityReason>` used by both referee.rs and engine.rs; `BuildSuggestion.coverage_note: Option<String>` populated from the `wvw_timeline.effects` reason; `render_data_quality_badge` prints it on the same line in muted text; new locale key `quality.coverage_line` ("Not simulated: {detail}") in all 12 locales; chat_flow.rs sets `data_quality`, `quality_reasons`, `coverage_note` on the Choya suggestion and appends the line to the plate concerns. Reason categories, stable effect identity and layout remain CONN-03 B.

## R3. The Reaper slice must be hand-authored

Finding, recorded as CONN-00-01 before any experiment: Reaper has no data behind it today.

- `data/normalized_effects/2026-01-13/{pve,pvp,wvw}.json` hold no Reaper trait record (Chilling Nova, Cold Shoulder, Soul Eater, Reaper's Onslaught, Decimate Defenses absent), no Necromancer rune, no Necro-specific relic. The only Necromancer proc executed in WvW is `Path of Corruption` (Curses trait, OnHit, ICD 10 s).
- The only on-crit sigil record is Superior Sigil of Fire (`OnCrit`, ICD 5 s) and `TriggerRule::OnCrit` has no firing site in production code; Air, Blood and Intelligence have no records.
- `data/rotation_profiles/wvw.json` has one Necromancer row, `elite_spec: null`, `evidence_level: "Heuristic"`; no published Reaper fixture (timeline fixtures are Spellbreaker, Mirage, Virtuoso).
- Life force is not a `ResourceKind`; `resource_model_complete` is a hardcoded allowlist (Thief, Revenant, Warrior, Mesmer) so Necromancer reads as complete when it is not.
- Chill on the enemy has no effect: only incoming chill on the player is modeled (`CHILLED_RECHARGE_PERCENT = 60`). Dark-field combos (Shroud 4, Well of Darkness) are counted unmodeled.

Decision: keep Reaper. Experiments use a hand-authored Reaper `ValidatedBuild` (Spite, Soul Reaping, Reaper; greatsword + axe/focus; Sigil of Fire and a passive sigil), a hand-authored opener id list, and hand-authored `NormalizedEffect` values living in the test module, not in `data/`. The chill-uptime mechanic is not an experiment; it is finding CONN-01-xx (runtime gap: no enemy-cooldown model). The seven experiment kinds map as follows:

| Kind | Mechanic | Entry point |
|---|---|---|
| Positive control | Path of Corruption (OnHit, ICD 10 s) equipped vs not, pinned opener | `evaluate_wvw_timeline` via `referee::evaluate_validated_build_with(..., opener)`; assert corrupt events and `unmodeled_sources` |
| Negative control | same record in the PvE mode file only, WvW scenario: no effect; same record on the stowed weapon set's sigil slot | same; assert zero change |
| Timing | Gravedigger multi-hit schedule vs `EnemyEvent::Control` mid-cast; Path of Corruption ICD respected; Might applied after a landed hit does not raise it | pinned opener + control event; compare `damage_events` |
| Ablation | complete mechanism (OnHit proc + its enabler skill) vs missing enabler vs missing payoff | `evaluate_wvw_timeline` with/without records |
| Interaction pair | Might stacks × strike modifier, 2×2 | `simulate_with` and `evaluate_wvw_timeline` |
| Unsupported control | Sigil of Fire (OnCrit) on a 100 % crit Reaper: zero proc damage, name appears in `unmodeled_sources`, line reaches suggestion and Choya evidence | `load_normalized_effects`, referee, `synergy_result_to_suggestion` |
| Cross-path parity | one Reaper build + WvW scenario through referee, Optimize suggestion, Choya `score_build` | see R5 |

Alternatives rejected: switching the slice to Spellbreaker or Mirage because fixtures exist (contradicts the user's choice and would not surface the Necromancer resource gap); adding Reaper records to `data/` in this sprint (data changes need sourced values and dates; that is CONN-03 C).

## R4. Diagnostics: bounded event trace on the WvW report, off by default

Decision: `WvwTimelineInput.trace: bool` (default false) and `WvwCombatReport.trace: Vec<TraceEvent>` capped at 512 events with `trace_truncated: bool`. `TraceEvent { t_ms, kind, source, detail }` for proc fired, proc skipped (ICD or unmodeled), hit landed, cast interrupted, swap. Filled only when `trace` is set; search and Choya never set it.

Rationale: neither simulator emits an event log; tests assert only aggregates, so no timing experiment can say "the proc fired at t". The optimizer has no `log`/`tracing` crate and the plan forbids serialising every tick. A capped vector on the existing report struct is the smallest inspectable surface, needs no dependency, and stays out of prompts because `ToolContext` never requests it.

Alternatives rejected: an env var (unreadable inside the injected DLL, and CI has no way to set it per test); adding `log`/`tracing` (new dependency, invisible in-game anyway).

## R5. Parity: assert where the referee flows, record where it does not

Findings:
- Equipped-character display (`resolution.rs`) never runs the referee: different stats function (`stats::calculate_full_stats` over the raw equipment tab vs `engine::calculate_validated_stats` over a `ValidatedBuild`), "Generic" buff profiles vs profession profiles, no gates, no realized axes. Recorded as CONN finding (reporting gap), not fixed in this sprint.
- Imported published builds (`provider_picks::adopt_provider_pick`) never run the referee; viability and rotation stay empty. Recorded as finding.
- `synergy_result_to_suggestion` recomputes gates with `evaluate_viability_gates` plus off-bar stability only, not the referee's objective-profile gate set, so the displayed gate list can differ from the referee's. Recorded; the parity experiment asserts on the fields that come straight from the referee.
- All canonical metrics are `f64` until the display boundary rounds `CombatMetrics` to `i32`.

Decision: the parity experiment evaluates one Reaper build and WvW scenario through (1) `evaluate_validated_build_with` directly, (2) `engine::synergy_result_from_validated` → `synergy_result_to_suggestion` (the Optimize exposure), (3) `score_build` with the same plate. Tolerance: `1e-9` on `f64` fields (same inputs, same code), `1.0` on rounded `CombatMetrics` indices. Path (1) vs (2) vs (3) must agree on `user_intent_score`, the six realized axes, `viability.is_viable`, and `quality`.

## R6. Test placement and budget

Decision: experiments are `#[cfg(test)]` modules in `wvw_timeline.rs` and `referee.rs` using `GameDb::empty_for_tests()` and hand-built `ValidatedBuild`s, matching the 45 and 50 tests already there. The default `cargo test -p gw2-optimizer --lib` run (1112 tests, 16.2 s at `1612f96`) may grow by at most 10 s; a pinned 15 s timeline runs in well under a second, so seven kinds with controls fit. Anything heavier is `#[ignore = "slow: run on demand"]`.

## R7. Audit document shape

Decision: `docs/simulator-connection-audit.md` uses the `CONN-*` ids and the two-column "Existing component | Evidence" table of `docs/simulator-trust-plan.md`, plus a findings table with the columns FR-002 requires. `docs/audit/` does not exist in this tree; the 003 ledger's `W###` six-column form is not copied. `Verified` / `Provisional` / `Blocked` are the `DataQuality` enum values (data/quality.rs) and are used with that meaning only.

## R8. Ownership and sequencing with the latency work

Decision: this feature lives on branch `004-simulator-trust` from `main`. The worktree branch `fix/wiki-timing-facts` holds the unreleased Choya latency commits and an active autoresearch loop editing `prompts.rs` and `choya_live.rs`. This feature does not touch `prompts.rs`, `llm/`, or `examples/choya_live.rs`. The `score_build` extension changes `gemini_tools.rs` tool declarations (a prompt-visible surface) and therefore lands last and merges only after the latency loop has closed and its DLL has been accepted in-game.
