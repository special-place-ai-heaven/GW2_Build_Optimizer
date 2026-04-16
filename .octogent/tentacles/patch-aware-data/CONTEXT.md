# patch-aware-data

Versioned, ground-truth game-data bundle and its typed loaders — the numeric substrate the engine reasons on.

## Agent Toolbelt (required)

- **Use SymForge MCP tools for all code work in this tentacle.** `health`
  once per session, then `search_symbols`, `search_text`,
  `get_file_context`, `get_symbol`, `find_references`, `edit_plan` +
  `edit_within_symbol` / `replace_symbol_body`, `analyze_file_impact`,
  `what_changed`. Raw `Read`/`Grep`/`Glob` on source burns 70–95% more
  tokens. (For editing the `data/*.json|yaml` data files themselves,
  `get_file_content` is the right tool — exact whitespace matters.)
- **Optional: Obsidian vault** for balance-patch context if present.

## Scope

- `crates/optimizer/src/data/mod.rs` — `DataState`, `EvidenceLevel`,
  `DataLoadError`, `initialize()` orchestrator
- `crates/optimizer/src/data/manifests.rs` — patch-manifest parsing + routing
- `crates/optimizer/src/data/patch_ledger.rs` — `PatchLedger`,
  `LedgerChange`, source-URL tracking per balance change
- `crates/optimizer/src/data/balance_overrides.rs` — per-mode deltas
  (PvE/PvP/WvW)
- `crates/optimizer/src/data/normalized_effects.rs` — normalized per-mode
  boon/condition/effect data (biggest dataset, ~730 symbols)
- `crates/optimizer/src/data/boon_condition_formulas.rs` — shared boon +
  condition formulas
- `crates/optimizer/src/data/universal_formulas.rs` — shared/universal
  formulas
- `crates/optimizer/src/data/objective_profiles.rs` — `ObjectiveProfile`
  + 6-axis `AxisWeights` + `NormalizationConstants`
- `crates/optimizer/src/data/rotation_profiles.rs` — `RotationProfile`,
  `ScenarioProfile`, `TargetBehavior`, `ConditionWeightsFromProfile`,
  `BuffProfileFromScenario`
- `crates/optimizer/src/data/slot_budgets.rs` — per-slot attribute
  budgets at level 80 ascended
- `crates/optimizer/src/data/profession_profiles.rs` — per-profession
  identity (HP class, armor class, condi weights)
- `crates/optimizer/src/data/quality.rs` — data-quality heuristics
- `crates/optimizer/src/data/consistency_tests.rs` — cross-dataset
  invariants
- `data/balance_overrides/2026-01-13/{pve,pvp,wvw}.json`
- `data/formulas/{boons,conditions,universal}.json`
- `data/manifests/2026-01-13.json`
- `data/normalized_effects/2026-01-13/{pve,pvp,wvw}.json`
- `data/objective_profiles/{pve,pvp,wvw}.json`
- `data/patch_ledgers/2026-01-13.yaml`
- `data/profession_profiles.json`
- `data/rotation_profiles/{pve,pvp,wvw}.json`
- `data/slot_budgets/level80_ascended.json`

## Mission Link

Interplay modeling is only as accurate as its inputs. If `EXPERTISE_DIVISOR`
is right but Firebrand's boon priorities are stale, the optimizer produces
scoring errors with no visible symptom. This layer keeps the numbers
honest through balance changes. Data is versioned by patch date so a
future "January 2026 balance" rollback is a one-line change.

## Key Decisions

- **All data files embedded via `include_str!`** — the DLL carries its
  data inside the binary. No runtime FS dependency, no addon-folder
  coupling. Pattern at top of every loader.
- **`EvidenceLevel` gates data quality.** Validators refuse to load
  non-heuristic rotation profiles — see
  `test_non_heuristic_evidence_rejected`. Quality is a load-time
  contract, not a runtime check.
- **Lazy-static `OnceCell` loading.** Loaders return `&'static Data` after
  first parse. `objective_profiles()`, `rotation_profiles()`, `ledgers()`
  all memoize — parse cost paid exactly once per DLL lifetime.
- **Fallback semantics for rotation profiles**:
  `RotationProfileData::lookup` tries exact (mode, profession, elite)
  first, then (mode, profession, generic). Missing generic fallback is
  rejected at load time (`test_missing_generic_fallback_rejected`).
- **`ObjectiveProfileData::default_for_mode` is contractual**: every mode
  has exactly one default profile (`test_each_mode_has_one_default`).
- **`ConditionWeightsFromProfile::from_profile`** converts loaded rotation
  profile → `ConditionWeights` the engine expects. Explicit constructors
  for known groups (`necro_group`, `firebrand_group`, `harbinger_preset`)
  and a PvE default.
- **Patch ledger = human-readable audit trail.** Every balance delta has
  a `source_url` (GW2 wiki patch notes, Hardstuck, etc.).
  `validate_ledger` rejects empty source URLs — every number in the
  engine is traceable to a primary source.
- **Pattern: `include_str!` → `serde_json::from_str` → validator fn →
  `OnceCell::set`.** All loaders follow this shape. Copy the pattern;
  don't invent a new one.
- **`populate_heuristic_uptimes`** (`rotation_profiles.rs:504-583`) fills
  derived uptime fields not in source JSON. Runs on first load.

## Conventions

- **One `include_str!` per file**, one validator per dataset, tests next
  to source.
- **New JSON/YAML files live under `data/<dataset>/<patch-date>/<mode>.{json,yaml}`**
  (mode omitted for mode-agnostic). Keep the patch-date segment even for
  single-patch datasets — future-proofs ledger inheritance.
- **`DataLoadError` variants cover Parse, Validation, Missing.**
  `std::fmt::Display` at `data/mod.rs:70-92` renders operator messages.
  New errors fit this shape or extend Display.
- **Validators return `Result<(), DataLoadError>`** and surface the first
  failure with location. Fail fast; no "list of all errors".
- **Tests favor real embedded data** over synthetic fixtures. `test_embedded_*`
  load the actual shipping JSON and assert invariants — they fail in CI
  if JSON is malformed, preventing a broken DLL from shipping.
- **Tests for malformed input** use inline-string fixtures, not temp files.

## Cross-Tentacle Contracts

- **core-domain** — imports `GameMode` only.
- **optimizer-engine** — primary consumer:
  - `ObjectiveProfile` → `ObjectiveScorer::from_profile`
  - `RotationProfile` + scenario → `BuffProfileFromScenario` →
    `default_buff_profiles`
  - `ConditionWeightsFromProfile` → `condition_weights_for_profession`
  - slot budgets → `calculate_candidate_stats` +
    `add_budget_stats_for_itemstat`
  - boon/condition formulas → `calculate_condition_ticks` + duration math
- **llm-integration** reads objective-profile descriptions for prompt
  construction; never reads raw JSON directly.

## Non-Goals

- **No HTTP.** This layer does not fetch data at runtime. Updates ship via
  new JSON in a new DLL build.
- **No UI surface.** The Settings tab displays "current patch: 2026-01-13"
  from `addon-ui`; it does not edit the bundled data.
- **No "mid-run" balance mutation.** A loaded `ObjectiveProfile` is
  immutable for the DLL's lifetime.

<!-- octogent:suggested-skills:start -->
## Suggested Skills

You can use these skills if you need to.

- `code-review`
- `refactor`
<!-- octogent:suggested-skills:end -->
