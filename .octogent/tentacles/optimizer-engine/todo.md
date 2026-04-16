# Todo

- **Regression harness for empirical constants** — commit a set of
  known-good builds (power DPS Dragonhunter, condi Harbinger, heal Druid,
  tank Chrono, WvW roam Daredevil) with expected score ranges. Any PR
  touching `STRIKE_DPS_NORM`, `CONDI_DPS_NORM`, `WEIGHT_BUDGET`, or tier
  constants must not shift these out of range. Wire into CI.

- **Audit every `Vec<Fact>` walker for traited-fact override handling** —
  grep for iteration patterns; confirm each site either (a) goes through a
  helper that applies overrides, or (b) has a unit test proving it doesn't
  double-count. Candidates: `score_fact`, `extract_damage_modifiers`,
  `calculate_combat_performance`.

- **Decompose `engine.rs::optimize_with_gemini`** — 200+ lines at
  `engine.rs:709-923`. Extract: prompt-context assembly, LLM call,
  validation, gear-prefix override, stat recalc, rotation simulate, synergy
  build. Each becomes independently testable. Keep public API and
  tier-fallback semantics.

- **Rotation simulator accuracy pass** — `SimulationResult` claims uptime
  percentages; there is no test that simulator uptime matches the rotation
  profile's `effective_boon_uptime` within tolerance. Add.

- **Synergy pipeline: prove weapon selection respects lock constraints** —
  `select_weapons` (`synergy_pipeline.rs:558-720`) is complex and not
  directly tested for the "locked elite spec → must equip its signature
  weapon" constraint. Add a focused test per profession.

- **Condition-tick coverage for Confusion + Torment mode dispatch** — tests
  exist (`test_confusion_mode_dispatch_in_combat`,
  `test_torment_mode_dispatch_in_combat`) but only for PvE/WvW. Add PvP
  cases — confusion damage curves differ by mode and this is an interplay
  the optimizer must get right.

- **Expand viability gates beyond WvW** — `referee::evaluate_viability_gates`
  runs only the EHP gate in PvE. Raid/strike encounters have their own
  floors (boon uptime thresholds, CC-bar contribution). Add PvE-specific
  gates driven from `data/objective_profiles/pve.json`, not hardcoded.

- **`select_gear_prefix` coverage extension** — tests cover common axes.
  Add cases for dual-stat power+condition hybrids produced by
  Celestial-adjacent weights at low budget — known edge case where cosine
  similarity can pick a wrong prefix.

- **Audit `optimize_*` inner loops for `is_cancelled`** — every loop
  boundary inside `optimize`, `optimize_deterministic`,
  `optimize_with_gemini`, and `synergy_pipeline::run` must check
  `CancellationToken::is_cancelled`. Add a test that injects a token,
  cancels mid-run, and asserts the function returns within N iterations.
  Companion to the `addon-ui` cancellation-coverage audit on spawn sites.
