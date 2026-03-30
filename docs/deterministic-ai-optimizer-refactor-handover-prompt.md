# Deterministic AI-Assisted Optimizer Refactor Handover Prompt

Use this prompt with a fresh coding agent session in another terminal.

```text
You are the implementation agent for the deterministic AI-assisted optimizer refactor in the GW2 Build Optimizer repository.

Repository:
- C:\AI_STUFF\PROGRAMMING\GW2_Build_Optimizer

Mission:
- Refactor the optimizer so that:
  1. the game world model is grounded in deterministic, source-backed mechanics
  2. the AI model acts as an optimizer/search agent over that deterministic world
  3. every proposed build is validated, scored, and compared by deterministic code
  4. the final product can consistently produce builds that are at least on-par with elite human build creators, with the architecture capable of surpassing them

Product thesis:
- Guild Wars 2 build optimization is not magic.
- The mechanics layer is largely known:
  - items
  - itemstats
  - skills
  - traits
  - formulas
  - timers
  - stacking rules
  - patch-mode splits
- Therefore, the core problem is:
  - deterministic world modeling
  - deterministic evaluation
  - intelligent search over a very large constrained space
- The AI model is not the source of truth.
- The AI model is the optimizer/search brain operating against deterministic truth.

Non-negotiable architecture:
- Deterministic data/mechanics layer:
  - authoritative source-backed game data and formulas
  - legality rules
  - combat/effect calculations
  - scenario-aware evaluation primitives
- AI optimizer layer:
  - interprets user intent
  - proposes candidate builds or search neighborhoods
  - explores combinations creatively
  - identifies promising interactions and unusual build spaces
- Deterministic judge/referee layer:
  - validates every candidate
  - computes stats, combat metrics, and scenario metrics
  - ranks candidates
  - emits confidence/quality/provenance
- The model must not become the final ranking authority.
- The final build recommendation must be justified by deterministic evaluation.

Fresh-session instructions:
1. If SymForge and codebase-memory MCPs are available, use them consistently for code discovery, tracing, and inspection.
2. If indexes are stale, reindex before substantial work.
3. Read the mandatory files listed below before editing code.
4. Use the live code, not stale planning assumptions, as the implementation baseline.

Mandatory files to read first:
1. docs/optimizer-source-of-truth.md
2. docs/optimizer-data-schemas.md
3. docs/reports/p3-13-evidence-classification.md
4. docs/architecture-assessment.md
5. docs/deterministic-ai-optimizer-refactor-handover-prompt.md
6. crates/optimizer/src/genome.rs
7. crates/optimizer/src/scenario.rs
8. crates/optimizer/src/referee.rs
9. crates/optimizer/src/engine.rs
10. crates/optimizer/src/synergy_pipeline.rs
11. crates/addon/src/ui/main_view/optimization.rs
12. crates/optimizer/src/llm/mod.rs
13. crates/optimizer/src/gemini_tools.rs
14. crates/optimizer/src/validation.rs
15. crates/optimizer/src/combat.rs
16. crates/optimizer/src/search.rs
17. crates/optimizer/src/scoring.rs

Core implementation rules:
- Never add a new numeric constant without assigning it an evidence class.
- Never treat heuristic scoring logic as factual combat truth.
- Never put heuristics directly into the factual stat/combat engine.
- Never let the LLM provider determine the final winner directly.
- Never claim superiority without a deterministic benchmark harness.
- Never collapse PvE, PvP, and WvW into one scoring model.
- Never silently reuse PvE assumptions in WvW or PvP.
- Every mode-sensitive computation path must accept BalanceContext.
- Every heuristic must be named, documented, replaceable, and testable.

What is factual vs heuristic:
- Factual / derived and should remain deterministic:
  - raw item and itemstat data
  - skill and trait records
  - profession/spec legality
  - base formulas
  - duration caps
  - patch-mode splits backed by patch notes or wiki
  - slot budgets
  - legality/validation
- Heuristic and must be isolated:
  - objective weighting
  - build desirability under user intent
  - rotation assumptions when no exact execution trace exists
  - target behavior assumptions
  - proc/uptime assumptions where they are not directly factual
  - search policy

Current code reality:
- The repo already has a strong factual substrate.
- The current optimizer path is still heuristic-heavy and stagewise:
  - greedy trait/spec pruning
  - greedy rune/sigil/weapon/skill selection
  - proxy-heavy scoring
  - simplified rotation simulation
  - optional LLM build-selection path
- This is useful as a seed generator, but not strong enough to be the long-term authority.

Current refactor foundation already implemented:
- crates/optimizer/src/genome.rs
  - introduces BuildGenome, a canonical full-build state extracted from ValidatedBuild
- crates/optimizer/src/scenario.rs
  - introduces ScenarioSpec, making the optimization scenario explicit instead of implicit
- crates/optimizer/src/referee.rs
  - introduces RefereeReport and deterministic evaluation of a complete validated build
- crates/optimizer/src/lib.rs
  - exports the new modules

Current verification state:
- The current foundation slice compiles and tests cleanly.
- Use this exact command if cargo is not on PATH:
  - C:\Users\poslj\.cargo\bin\cargo.exe test -p gw2-optimizer

Refactor target state:
- The optimizer should become:
  1. a deterministic world model
  2. an AI-guided search system
  3. a deterministic referee
  4. a benchmarked optimizer

The desired behavior is:
- user specifies profession/mode/preferences/goals
- AI explores candidate builds against deterministic game rules
- deterministic evaluator scores each candidate under the declared scenario
- referee ranks them and emits the winner
- benchmark harness compares winners against elite human reference builds
- UI shows benchmark delta, patch target, quality/confidence, and explanation

What the AI model is allowed to do:
- generate candidate builds
- mutate candidates
- suggest overlooked synergies
- suggest search neighborhoods
- explain why a candidate should be explored
- critique weak finalists
- interpret flexible user intent into optimization targets

What the AI model is not allowed to do:
- bypass legality validation
- bypass deterministic scoring
- directly override the referee’s winner
- invent unsupported mechanics
- hide uncertainty in heuristic assumptions

Primary implementation objectives:
1. Create a new deterministic optimization path without breaking the existing UI flow.
2. Demote the current synergy pipeline into a seed-generation role.
3. Replace direct LLM build selection with advisor-driven candidate generation.
4. Add a benchmark corpus and deterministic benchmark comparison layer.
5. Separate factual mechanics, heuristic assumptions, search policy, and final ranking cleanly.

Concrete next phases:

Phase A: New engine entry point
- Add a new `optimize_v2()` path in `crates/optimizer/src/engine.rs`.
- It should:
  - accept user intent and scenario inputs
  - generate deterministic seed candidates
  - optionally ask the advisor model for candidate mutations or exploration hints
  - run deterministic evaluation on all finalists
  - return a deterministic report structure

Phase B: Search v2
- Add a new module such as `crates/optimizer/src/search_v2.rs`.
- Search over complete build states, not partial stage decisions.
- Use:
  - beam search, evolutionary search, or another bounded deterministic search strategy
  - memoized evaluation
  - dominance pruning
  - seed injection from the existing synergy pipeline
- Do not brute-force the full space blindly.

Phase C: Advisor layer
- Add a new module such as `crates/optimizer/src/advisor.rs`.
- Reuse the existing `LlmClient` and tool infrastructure where useful.
- The advisor should:
  - receive current top builds and search gaps
  - propose alternative candidates or mutations
  - produce explanations and critiques
- The advisor should not return the winner directly.

Phase D: Benchmark layer
- Add a new module such as `crates/optimizer/src/benchmarks.rs`.
- Add reference build data under `data/reference_builds/`.
- Each benchmark entry should include:
  - patch id
  - mode
  - scenario
  - build definition
  - source/provenance
  - expected metric envelope
- Add tests that fail when the optimizer loses to benchmark reference builds for the same scenario.

Phase E: Trust/UI integration
- Update addon UI flow only after `optimize_v2()` is functional.
- Show:
  - scenario
  - deterministic score/report
  - benchmark delta
  - data quality / confidence
  - AI explanation as advisory text, not authority text

Important design stance:
- The model’s role is closer to:
  - “intelligent search policy”
  - “candidate generator”
  - “research assistant”
  - “critic”
- The deterministic engine’s role is:
  - “physics”
  - “rulebook”
  - “judge”

Do not regress this foundation:
- Keep using ValidatedBuild as the legality-checked bridge object unless a clearly better canonical type is introduced intentionally.
- Keep the new BuildGenome and ScenarioSpec concepts central.
- Keep RefereeReport as the direction of travel for deterministic judging.
- Keep current paths alive until `optimize_v2()` is proven.

Expected code quality:
- Small, composable modules.
- Tests for every new deterministic boundary.
- No giant rewrite in one pass.
- Prefer additive migration:
  - new path first
  - prove it
  - then retire old path

Required verification:
- Run optimizer crate tests after each meaningful slice:
  - C:\Users\poslj\.cargo\bin\cargo.exe test -p gw2-optimizer
- Add focused unit tests for:
  - genome extraction
  - scenario derivation
  - referee determinism
  - search reproducibility
  - benchmark comparisons
- If changing math or evaluation semantics, document:
  - evidence class touched
  - source justification
  - modes affected
  - whether the change is factual or heuristic

First task to execute immediately in the fresh session:
- Inspect the new foundation modules (`genome.rs`, `scenario.rs`, `referee.rs`)
- Design and implement `optimize_v2()` as a parallel path
- Route current deterministic/synergy logic into seed generation only
- Define the next result/report type that the addon can consume with minimal breakage

Final success condition:
- The repo should evolve from a heuristic build suggester into a deterministic, benchmarked, AI-guided optimizer:
  - deterministic mechanics truth
  - AI-assisted exploration
  - deterministic evaluation and ranking
  - benchmark comparison against elite human references

Start by reading the mandatory files, then inspect the live code, then continue the refactor from the current foundation.
```

