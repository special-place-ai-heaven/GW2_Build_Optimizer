# Implementation Plan: Simulator trust (Sprint 1)

**Branch**: `004-simulator-trust` (main checkout, from `main`) | **Date**: 2026-09-07 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/004-simulator-trust/spec.md`; source plan `docs/simulator-trust-plan.md`; research in [research.md](research.md).

## Summary

Make the chain from a build to its verdict inspectable for one slice, Necromancer Reaper in WvW, and fix the first demonstrated break in it. Concretely: an audit document with a baseline and CONN findings; an 8 × 8 trace matrix; seven kinds of causal experiment on hand-authored Reaper fixtures; a bounded event trace on the WvW report for the timing experiments; the names of unmodeled effect sources carried through to one visible "Not simulated: …" line on the overlay and into Choya's evidence; and `score_build` extended so Choya can obtain the app's own full-build referee verdict. Research established that Reaper has no data behind it today and that two display paths never run the referee; those become findings, not silent assumptions.

## Technical Context

Rust 2021, stable toolchain; Windows MSVC Nexus cdylib. Crates touched: `gw2-optimizer` (rotation/wvw_timeline.rs, engine.rs, referee.rs, gemini_tools.rs, data/quality.rs), `gw2-build-optimizer` addon (ui/comparison.rs, ui/main_view/optimization.rs, ui/main_view/chat_flow.rs), `locales/*.json` (12 files), `docs/simulator-connection-audit.md`. No new dependencies; no `log`/`tracing`. Storage: none. Testing: `cargo test` unit modules with `GameDb::empty_for_tests()`; no live provider calls; CI runs fmt, clippy `-D warnings`, workspace tests on windows-latest. Performance: at most 10 s added to the optimizer lib test run (16.2 s at `1612f96`); trace capped at 512 events and off by default; `score_build` full-build mode capped at three evaluations per chat request. Constraints: do not touch `prompts.rs`, `llm/`, `examples/choya_live.rs` (owned by the latency work on `fix/wiki-timing-facts`); no calibrated formula or threshold changes; no push or release except with the in-game acceptance of the latency work. Scope: one slice, one profession, WvW plus a small PvE comparison.

## Constitution Check

The constitution file is an unfilled template, so it supplies no ratified gates. Project gates applied pre- and post-design: no secrets or account data in artifacts; SymForge for discovery and impact; Terminal Commander for long commands; no local Docker; every remedy preceded by a failing behavioural test; `Verified`/`Provisional`/`Blocked` used with their `DataQuality` meaning; the user tests in-game before any release; user merges PRs. All design choices below meet these gates. Post-design re-check: the only prompt-visible change is the `score_build` declaration text, sequenced last (R8).

## Project Structure

### Documentation (this feature)

```text
specs/004-simulator-trust/
├── plan.md              # this file
├── spec.md
├── research.md          # R1–R8 decisions
├── data-model.md        # Finding, matrix cell, experiment, coverage line, trace event, tool shape
├── contracts/
│   ├── score-build-tool.md
│   ├── coverage-line.md
│   └── audit-document.md
├── quickstart.md
└── tasks.md             # /speckit-tasks output
docs/simulator-connection-audit.md   # produced by CONN-00, updated through the sprint
```

### Source code (repository root)

```text
crates/optimizer/src/rotation/wvw_timeline.rs   # unmodeled_sources names; trace events; experiments (test module)
crates/optimizer/src/engine.rs                  # active_normalized_effects returns names; shared coverage_reason helper
crates/optimizer/src/referee.rs                 # coverage_reason use; parity + referee-level experiments (test module)
crates/optimizer/src/data/quality.rs            # unchanged types; helper may live here
crates/optimizer/src/gemini_tools.rs            # ToolContext.scenario; score_build full-build mode; simulate_rotation label
crates/addon/src/ui/comparison.rs               # BuildSuggestion.coverage_note; one-line render beside the marker
crates/addon/src/ui/main_view/optimization.rs   # synergy_result_to_suggestion / attach_chat_stats populate coverage_note
crates/addon/src/ui/main_view/chat_flow.rs      # Choya suggestion gets quality fields; concerns gain the line; per-turn tool cap
crates/addon/src/ui/main_view/optimize_flow.rs  # ToolContext.scenario at the advisor call site
locales/{en,de,es,fr,it,ja,ko,nl,pl,pt,ru,zh}.json   # quality.coverage_line
```

**Structure Decision**: existing crates and files only; experiments as `#[cfg(test)]` modules beside the code they exercise, matching the 45 timeline and 50 referee tests already there.

## Execution order

1. **CONN-00 (US1)**: write the audit Baseline and Inventory sections; record CONN-00-01 "Reaper has no records, profile, fixture or resource model" and the two "never runs the referee" paths (equipped display, published import) as hypotheses with evidence; author the Reaper `ValidatedBuild`, opener and `NormalizedEffect` test fixtures in a shared `#[cfg(test)]` helper.
2. **CONN-01 (US2)**: fill the trace matrix cell by cell from actual callers (the path map in research R5); answer the seven open questions; mark every cell.
3. **Diagnostics (US3 support)**: `WvwTimelineInput.trace` and `WvwCombatReport.trace` with the 512 cap; one test proves the cap and that search never requests it.
4. **CONN-02 (US3)**: the seven experiment kinds, each seen failing once under a disabling change and recorded; parity for referee vs Optimize suggestion first, the `score_build` leg added in step 6.
5. **Smallest remedy (US4)**: the coverage line end to end (names kept, shared helper, `coverage_note`, one render line, locale key, Choya suggestion quality fields). Its failing experiment is the unsupported-trigger control asserting the line at every projection.
6. **FR-018 (US3 scenario 5)**: `ToolContext.scenario`; `score_build` full-build mode; `simulate_rotation` scope label; per-turn cap; parity leg through the tool. Last, because its declaration text is prompt-visible.
7. Audit Findings/Experiments/Remedy sections finalised; quickstart gates run; version bump and DLL only if the release condition holds.

Steps 1 and 2 are documentation with fixtures; 3 to 6 each end with a local commit after the focused tests and clippy pass.

## Risks and implementation decisions

- Reaper fixtures are hand-authored, not sourced game data; the audit says so and no fixture value lands in `data/`. Chill uptime on the enemy cannot be measured (no enemy-cooldown model): a runtime-gap finding, not an experiment.
- Keeping unmodeled names changes two return types (`active_normalized_effects`, `WvwCombatReport`); `find_references` before each edit, then `analyze_file_impact`.
- The referee reason string is duplicated in `engine.rs`; the shared helper removes the copy so the line cannot drift.
- `synergy_result_to_suggestion` recomputes gates with a narrower gate set than the referee; parity asserts only referee-sourced fields and the discrepancy is a finding for CONN-03 B.
- `score_build` full-build mode has no cancellation inside the referee; the per-turn cap bounds it. Threading `is_cancelled` into `simulate_prepared` is a recorded follow-up.
- Passive double counting: the tool must derive modifiers only through the referee path, never through `extract_damage_modifiers` a second time; the parity test catches a divergence.
- Locale parity test (#21) will fail until all 12 files carry the new key; add them in one commit.

## Parallel opportunities

Audit document writing (steps 1–2) and the diagnostics change (step 3) are independent. Optimizer-side name plumbing and addon-side rendering can be designed together but must compile together; sequence them in one commit. The `score_build` extension is independent of the coverage line in code but sequenced last by policy. Cargo commands serialize on this checkout's target directory; the worktree's target directory is separate.

## Complexity Tracking

No constitution violations. No new crate, dependency, format or framework. Additions are two struct fields, one helper, one locale key, one tool argument and one capped vector.
