# optimizer-engine

The interplay-modeling brain — must outperform human meta-build creators by rigorously modeling variable interactions.

## Agent Toolbelt (required)

- **Use SymForge MCP tools for all code work in this tentacle.** This is the
  churn-heaviest area of the repo (per SymForge git-temporal hotspots).
  `health` once per session, then `search_symbols`, `search_text`,
  `get_file_context` (prefer over raw Read), `get_symbol` for function
  bodies, `find_references` before every refactor, `edit_plan` +
  `edit_within_symbol` / `replace_symbol_body` for surgical edits,
  `analyze_file_impact` after each edit, `what_changed` on resume. Raw
  `Read`/`Grep`/`Glob` on source burns 70–95% more tokens.
- **Optional: Obsidian vault** for higher-level mission context.

## Scope

- `crates/optimizer/src/engine.rs` — 3-tier fallback: `optimize_deterministic`
  → `optimize_with_gemini` → legacy `optimize` + enrich
- `crates/optimizer/src/combat.rs` — damage math, condition ticks,
  rune/sigil/relic parsers, duration bonuses, caps
- `crates/optimizer/src/scoring.rs` — 6-axis `OptimizationWeights`,
  `ObjectiveScorer`, `select_gear_prefix` (cosine similarity),
  `WEIGHT_BUDGET = 2.0`
- `crates/optimizer/src/synergy.rs` + `synergy_pipeline.rs` — deterministic
  spec/trait/gear/rune/sigil/relic/weapon/skill co-selection
- `crates/optimizer/src/search.rs` + `search_v2.rs` — legacy + v2 search loops
- `crates/optimizer/src/rotation/` — `skill_timings`, `builder`, `simulator`,
  `SimulationResult`
- `crates/optimizer/src/gamedb.rs` — in-memory `GameDb` (O(1) lookups, no
  disk I/O during resolve)
- `crates/optimizer/src/stats.rs` — attribute adjustment formula resolver
- `crates/optimizer/src/validation.rs` — validates LLM build responses
  against `GameDb` (jointly shared with `llm-integration` — "LLM proposes,
  engine validates")
- `crates/optimizer/src/referee.rs` — WvW hard-viability gates (stunbreaks,
  cleanses, stability, EHP floors per sub-role)
- `crates/optimizer/src/balance.rs`, `benchmark.rs`, `scenario.rs`,
  `genome.rs`, `context.rs`
- `crates/optimizer/src/lib.rs` — public API + `enrich_with_gemini` re-export
- `crates/optimizer/tests/objective_profiles_integration.rs`

## Mission Link

This is where the project wins or loses its headline claim: outperform
human meta-build creators. Winning requires modeling **every interplay**
correctly:

- **Multiplicative damage modifiers** (`DamageModifiers::total_strike_mult`,
  `total_condi_mult`) stack multiplicatively, not additively.
- **Hard caps**: `CONDITION_DURATION_CAP = 1.0` (`combat.rs:231`),
  `BOON_DURATION_CAP = 1.0` (`combat.rs:236`), crit-chance 1.0.
- **Diminishing returns** through divisors: `EXPERTISE_DIVISOR = 1500`
  (`combat.rs:220`), `CONCENTRATION_DIVISOR = 1500` (`combat.rs:226`).
- **Per-condition duration bonuses** (trait/rune granted) stack *additively*
  *before* the cap — see `total_condi_duration_for`.
- **6-axis scoring with a hard budget** `WEIGHT_BUDGET = 2.0`. Players
  cannot specialize in everything; the optimizer honors that.

## Key Decisions

- **3-tier fallback semantics** (`engine.rs:115+`): each tier falls through
  on failure with a `Warning` log — never silent, never `Error`. The
  `release` skill's smoke test depends on this.
- **`select_gear_prefix` (cosine similarity) is authoritative.** Gemini's
  gear choice is always overwritten — Gemini ignores gear constraints.
  Do not "trust the LLM" on gear. (`scoring.rs:723-760`.)
- **`GameDb` is all in-memory HashMaps.** Build resolution hits it O(1);
  zero disk I/O on the resolve path.
- **Scoring constants are empirically tuned.**
  `STRIKE_DPS_NORM=3000`, `CONDI_DPS_NORM=3000`, `EFFECTIVE_HEALTH_NORM`,
  `HEALING_NORM`, `BOON_SUPPORT_NORM`, `CONTROL_NORM` (`scoring.rs:16-23`).
  `WEIGHT_BUDGET=2.0` is calibrated against known-good builds. **Do not
  adjust without a cross-build regression pass.** The `refactor` skill
  protects these explicitly.
- **`set_constrained` proportionally scales other axes** to stay under
  `WEIGHT_BUDGET`. That's how the UI sliders behave. (`scoring.rs:126-158`.)
- **Traited-fact override pattern**: when iterating `skill.facts`, collect
  override indices from `traited_facts` *first*, then skip overridden base
  facts. Naive iteration double-counts. Applies across `combat.rs`,
  `scoring.rs::score_fact`, and `synergy.rs` fact-walk sites.
- **Rune bonuses are unstructured strings** (e.g. `"+7% Burning Duration"`)
  parsed by `parse_rune_modifier` (`combat.rs:649-697`). Sigils and
  relics parsed by `parse_sigil_modifier` / `parse_relic_modifier`. Any
  code walking `Vec<Fact>` for rune/sigil/relic bonuses is a bug.
- **Elite spec skill gating.** Filters by `Skill::specialization`: only
  `None` (core) or the equipped elite spec are eligible.
- **`ObjectiveScorer::from_profile` reads the 6-axis scorer from a loaded
  `ObjectiveProfile`** (from `patch-aware-data`). `fallback` handles
  missing profiles. (`scoring.rs:363-411`.)
- **`referee::evaluate_viability_gates` enforces WvW hard floors**: min
  stunbreaks, min cleanses, stability access, EHP floor per WvW sub-role
  (Roam < Havoc < Zerg). PvE runs only the EHP gate.
  (`referee.rs:18-50, 105+`.)
- **Panic recovery.** The optimization background thread (in `addon-ui`)
  wraps the engine call in `catch_unwind`. Engine functions must be
  panic-safe-adjacent — prefer `?` over `unwrap`.

## Conventions

- **Profession condition weights go through `condition_weights_for_profession`**
  (`combat.rs:182-202`). Do not inline per-profession tables; extend
  dispatch.
- **`DamageModifiers::total_*` helpers are the only sanctioned way** to
  combine modifiers. `additive.iter().sum()` gets it wrong for crit-damage
  stacking.
- **Test files sit next to source**, heavy reliance on integration tests
  (`objective_profiles_integration.rs`). Addon-crate tests run
  `--test-threads=1` due to global `STATE` — *that's the addon*, not this
  crate. This crate runs in parallel.
- **Cancellation checks**: loops inside `optimize_*` must respect
  `CancellationToken::is_cancelled` at iteration boundaries — users can
  abort mid-run from the UI.

## Cross-Tentacle Contracts

- **core-domain** supplies input types; this crate returns `CombatMetrics`,
  `RotationBreakdown`, `SynergyResult`, `BuildCandidate`.
- **gw2-api-client** populates `GameDb` via `GameDb::load`.
- **patch-aware-data** provides `ObjectiveProfile`, `RotationProfile`, slot
  budgets, formulas — consumed by `ObjectiveScorer`,
  `calculate_combat_performance`, `calculate_condition_ticks`.
- **llm-integration** calls `optimize_with_gemini` / `enrich_with_gemini`;
  this crate validates and overrides what the LLM returned.

## Hotspots (SymForge git-temporal)

Per the live index: `engine.rs`, `synergy_pipeline.rs`, `scoring.rs`, and
`combat.rs` are top churn files. Changes here ripple — run the full
`cargo test` before declaring done.

<!-- octogent:suggested-skills:start -->
## Suggested Skills

You can use these skills if you need to.

- `code-review`
- `refactor`
<!-- octogent:suggested-skills:end -->
