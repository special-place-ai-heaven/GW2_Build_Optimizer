---
stepsCompleted: ["step-01-validate-prerequisites", "step-02-design-epics", "step-03-create-stories", "step-04-final-validation"]
validationStatus: PASS
validationDate: 2026-03-06
sprintPlanningReady: true
highRiskStories: [P3-09, P3-10b, P3-14, P3-15]
inputDocuments:
  - docs/optimizer-source-of-truth.md
  - docs/optimizer-data-schemas.md
  - docs/optimizer-coding-agent-handover-prompt.md (execution guidance only)
  - _bmad-output/project-context.md (repo constraints)
  - docs/stories/P2-07-saved-build-profession-and-crash-safety.md (overlap awareness)
  - docs/stories/P2-03-catch-unwind-hardening.md (dependency awareness)
---

# GW2_Build_Optimizer - Epic Breakdown

## Overview

This document provides the complete epic and story breakdown for GW2_Build_Optimizer Epic 3: Optimizer Correctness Overhaul and Patch-Aware Data-Driven Architecture. It decomposes the requirements from the optimizer source-of-truth and data schemas documents into implementable stories.

## Requirements Inventory

### Functional Requirements

FR1: Implement canonical ProfessionProfile with correct armor_weight (Heavy/Medium/Light), health_class (High/Medium/Low), base_health_level_80, and base_defense_level_80 per profession. Guardian must be Low health, Necromancer must be High health. Armor class and health class must be modeled as independent dimensions and must never be inferred from each other.

FR2: Implement BalanceContext type carrying patch_id and game_mode (PvE/PvP/WvW). Every mode-sensitive and patch-sensitive computation path must accept or derive from BalanceContext. This is a first-class architectural type, not an implementation detail.

FR3: Implement universal attribute formulas at level 80: base primary=1000, 1 Vitality=10 HP, crit chance=(Precision-895)/21, 15 Ferocity=1% crit damage, 15 Expertise=1% condition duration, 15 Concentration=1% boon duration, caps at 100%.

FR4: Implement strike damage foundation: damage = skill_fact_damage * (Power/1000) * (2597/target_armor), where 2597 is tooltip reference armor.

FR5: Implement mode-aware boon values. Fury: +25% crit chance in PvE, +20% in PvP/WvW. Might: +30 Power and +30 Condition Damage per stack. Vulnerability: +1% damage per stack (cap 25). Protection: -33% incoming strike. Resolution: -33% incoming condition.

FR6: Implement mode-aware condition formulas at level 80 with the general form `coefficient * ConditionDamage + base`. Each damaging condition must declare per-mode coefficients and base constants, wiki-verified during implementation (not frozen at the FR layer — known discrepancies exist for Burning base, Torment, and Confusion). Multi-state conditions (Torment: stationary/moving, Confusion: over-time/on-skill-use) must declare their state dimensions explicitly per mode. Condition data must also include `StatusDefinition`-style stacking metadata (stacking mode, max stacks, effect class, suppression effects) per condition.

FR7: Implement duration formulas: outgoing_duration = base_duration * (1 + duration_bonus), where condition_duration_bonus = expertise/1500 + explicit modifiers, boon_duration_bonus = concentration/1500 + explicit modifiers.

FR8: Implement canonical slot-budget dataset for level-80 ascended equipment with explicit values per slot type (1H weapon, 2H weapon, amulet, accessory, ring, back, armor) and stat shape (ThreeStat, FourStat, CelestialLike).

FR9: Implement PvP optimizer path separation. PvP must bypass normal gear-prefix optimization entirely. PvP optimization uses amulet+rune+sigils+relic+traits+skills only. The PvP path is a distinct optimization route, not a mode flag on the gear search.

FR10: Implement patch-aware balance override system with per-mode trait/skill coefficient overrides stored in versioned data files.

FR11: Implement WvW non-fallback behavior. WvW must not silently reuse PvE numeric assumptions when split data is required. Where balance data differs between WvW and PvE, WvW must use its own values or explicitly degrade with a labeled heuristic fallback.

FR12: Implement NormalizedEffect representation with 23 effect categories (17 from source-of-truth section 11 + 6 boon/condition interaction categories: RemovesBoon, StealsBoon, CorruptsBoon, RemovesCondition, ConvertsConditionToBoon, TransfersCondition), explicit stacking rules, trigger rules, uptime models, timer/ICD/cap metadata, and evidence level per effect.

FR13: Implement RotationProfile as explicit heuristic layer with per-profession/spec/mode condition application rates, boon generation/sustain assumptions (distinct from achieved boon uptime), target behavior assumptions (including suppression effects like Slow/Chilled/Weakness that reduce output channels), and buff uptime assumptions.

FR14: Implement Unknown-value handling as runtime behavior. Unresolved changed values must be represented explicitly (not silently zeroed or defaulted). Factual scoring paths must block or degrade into labeled heuristic mode when encountering Unknown values. Stale values must never be silently reused after a patch changes them.

FR15: Implement factorized dependency tables: profession_profiles, slot_budgets, attribute_formulas, condition_formulas, balance_overrides, normalized_effects, buff_profiles, rotation_profiles, objective_profiles, scoring_rules.

FR16: Classify every numeric rule as Factual, Derived, Heuristic, or Unknown. Heuristics must be named, documented, tested, and replaceable.

FR17: Replace hardcoded factual constants in Rust source with values loaded from canonical data files.

FR18: Implement typed loaders for each dataset with typed validation errors. Loaders must reject malformed enums, reject duplicate IDs, reject patch/mode mismatches, preserve Unknown explicitly, and surface typed errors (not String). Silent failure is not acceptable.

FR19: Save/load must persist profession context for accurate combat metric reconstruction on reload AND use crash-safe persistence (temp-write + atomic rename). Both the profession-awareness defect and the non-atomic save behavior are in scope.

FR20: Implement objective-profile datasets and scorer isolation with 6 scoring axes: power, condition, boon_support, healing, sustain, control. Scoring logic must be separated from the factual combat engine. Boon contribution must be a first-class scoring axis (boon_support) with typed per-boon priority weights (`boon_priorities`), not implicit inside other axes or collapsed into one generic support number. Condition contribution must include typed per-condition priority weights (`condition_priorities`) so that damage conditions, debuffs, and suppression conditions can be valued independently. Interaction operations (boon strip/steal/corrupt, condition cleanse/convert/transfer) must include typed per-operation priority weights (`interaction_priorities`) so that denial and sustain operations can be valued by the control and boon_support axes. Objective profiles are mode-specific and use-case-specific (e.g., PvE_Power_DPS, PvP_Burst_Duelist, WvW_Zerg_BoonSupport, WvW_Roaming_Disruptor). Without explicit separation, implementation will drift back into mixed factual/heuristic logic.

FR21: Implement patch manifest infrastructure. Each patch snapshot requires a manifest declaring patch_id, game_build_id (the GW2 API `/v2/build` integer), release_date, inheritance chain, source links, supported modes, and status. Manifests are first-class versioning artifacts. The `game_build_id` enables runtime staleness detection by comparing against the live `/v2/build` endpoint.

FR22: Implement patch ledger infrastructure. Each patch requires a machine-readable change ledger with entity id, entity type, mode, field changed, old value, new value, evidence level, and source link. Ledgers are the bridge between patch notes and normalized data.

### NonFunctional Requirements

NFR1: Patch governance - no balance-sensitive numeric value may exist without a patch_id. Balance datasets must be patch-versioned. Old patch data must not be overwritten in place.

NFR2: Unknown-value rule - if a patch changes a value but the new number is not captured locally, record as Unknown and block or degrade factual scoring paths. Never silently continue with stale coefficients.

NFR3: Source hierarchy precedence: (1) Official API/in-game data, (2) GW2 Wiki formulas, (3) Wiki patch notes, (4) Local curated balance snapshots, (5) Approved heuristics.

NFR4: Review rule - any PR changing optimizer math must state evidence level, source justification, factual vs heuristic, and affected modes.

NFR5: Every factual rule needs at least one source-backed test. Required test styles: exact formula tests, mode-split regression tests, profession profile regression tests, slot-budget total tests, save/load context preservation tests, snapshot tests for patch override datasets.

NFR6: Heuristics are allowed only when no authoritative value exists, and must be tagged, tested, and replaceable without breaking the engine.

NFR7: Mode separation is non-negotiable - PvE, PvP, and WvW must never be flattened into one coefficient table where balance differs.

### Additional Requirements

From Architecture / Data Schemas:

- Data files live under `data/` directory with explicit subdirectory structure: manifests/, formulas/, slot_budgets/, balance_overrides/, patch_ledgers/, normalized_effects/, rotation_profiles/, objective_profiles/
- Percent-like values stored as decimal ratios (0.25 not 25)
- Patch IDs use ISO date format (YYYY-MM-DD)
- Evidence level enum: Factual, Derived, Heuristic, Unknown
- Null for structurally optional values; Unknown evidence state for known-but-unresolved numbers
- Loader modules live under `crates/optimizer/src/data/`
- Cross-file consistency checks required: every profession in code exists in profession_profiles.json, every patch override references valid manifest, every objective profile references valid mode
- Each story delivers data file + loader + code integration + tests as one unit. No orphaned data files or loaders without immediate integration.
- Implementation note (ADR-02): canonical values live in JSON data files, but are loaded once into typed in-memory structures at startup or cache-refresh time. No repeated JSON parsing or file I/O on hot paths. Loaders parse → validate → return typed structs; runtime code operates only on the in-memory structs.
- Data reload lifecycle (R6): data loads into typed, immutable in-memory snapshot(s). Each optimization run uses the snapshot captured at run start — no mid-run hot reload. Refresh policy: data is loaded at addon startup. If no manual reload UI is in scope, updates to data files take effect on next addon restart. If a manual "reload data" action is added later, it replaces the in-memory snapshot atomically between runs.
- Schema evolution tradeoff (R5): data files are patch-versioned (via manifest patch_id / game_build_id) but not schema-versioned. Schema changes (new fields, new enum variants) are handled by updating all data files in the same commit as the loader change. This is acceptable for a solo-maintained addon. If schema churn becomes a real problem, introduce a `schema_version` field in data files later — but do not pre-build the migration machinery now.

From P2-07 Overlap:

- P3-16 (save/load) explicitly supersedes and absorbs P2-07. P2-07 must not remain as a parallel active story. All P2-07 scope is subsumed by P3-16.

From Project-Context Conflicts:

- CONFLICT: project-context.md has Guardian=HIGH health, Necromancer=MEDIUM health. Source-of-truth corrects to Guardian=LOW, Necromancer=HIGH. project-context.md must be updated after FR1 is implemented.
- CONFLICT: project-context.md torment/confusion formulas differ from source-of-truth values. Source-of-truth takes precedence per established hierarchy.
- CONFLICT: current code and historical docs treat Fury as one global value. Source-of-truth requires PvE (+25%) vs PvP/WvW (+20%) split.
- Relevant factual stories must include cleanup of stale project-context and historical docs in their acceptance criteria.

### Known Defects (Confirmed)

D1: Guardian and Necromancer health classes are wrong in current code.
D2: Burning base value is off from canonical level-80 wiki value.
D3: Torment and confusion formulas are mode-incomplete and numerically outdated.
D4: Fury is treated as one global value instead of PvE vs PvP/WvW split.
D5: Gear search uses local slot constants instead of canonical slot-budget dataset.
D6: Disable scoring is a proxy, not a factual CC/defiance model.
D7: Condition weighting is still a heuristic preset system, not a spec-aware rotation model.
D8: Save/load loses profession-aware combat reconstruction and uses non-atomic writes.

### FR Coverage Map

| FR | Phase | Story | Description |
|----|-------|-------|-------------|
| FR1 | A | P3-01 | Profession profiles with correct health/armor classes |
| FR2 | A | P3-02 | BalanceContext type + game_mode plumbing |
| FR3 | A | P3-03 | Universal attribute formulas |
| FR4 | A | P3-03 | Strike damage foundation |
| FR5 | A | P3-04 | Mode-aware boon values + stacking modes + counterpart/suppression metadata (Slow/Chilled/Weakness) |
| FR6 | A | P3-04 | Mode-aware condition formulas + stacking mode/cap metadata per boon and condition + boon/condition interaction notes |
| FR7 | A | P3-05 | Duration formulas |
| FR8 | A | P3-06 | Slot-budget dataset (data file + loader) |
| FR9 | B | P3-11 | PvP optimizer path separation |
| FR10 | B | P3-09 | Balance override datasets |
| FR11 | B | P3-12 | WvW non-fallback behavior |
| FR12 | B | P3-10a/10b | NormalizedEffect representation (23 categories incl. boon/condition interaction ops) — split: types+schema+timer/ICD/cap fields (10a), extraction+population+timer+interaction metadata (10b) |
| FR13 | C | P3-14 | Rotation profiles (heuristic) — typed sections: boon_generation, boon_uptime, condition_application, incoming_suppression, target_behavior, scenarios |
| FR14 | B | P3-09 | Unknown-value runtime handling |
| FR15 | B | P3-13 | Factorized dependency tables (including stacking/cap/timer metadata cross-validation) |
| FR16 | B | P3-13 | Evidence classification (including timer/ICD/cap evidence levels) |
| FR17 | B | P3-07 | Replace hardcoded constants with data file reads |
| FR18 | B | P3-07 | Typed loaders with typed validation errors |
| FR19 | - | P3-16 | Save/load persistence + crash safety (standalone, absorbs P2-07) |
| FR20 | C | P3-15 | Objective profiles + scorer isolation (6-axis: power, condition, boon_support, healing, sustain, control + per-boon/per-condition/per-interaction priorities) |
| FR21 | B | P3-08 | Patch manifest infrastructure |
| FR22 | B | P3-08 | Patch ledger infrastructure |

### Defect Resolution Map

| Defect | Resolved At | Notes |
|--------|-------------|-------|
| D1 | P3-01 | Profession profiles fix Guardian/Necromancer health classes |
| D2 | P3-04 | Burning formula coefficient and base constant verified against cited wiki source during implementation (L1 verification required — do not assume 131 is correct; wiki may yield 131.75 or other value) |
| D3 | P3-04 | Torment/confusion formulas mode-complete with correct values |
| D4 | P3-04 | Fury split: 25% PvE, 20% PvP/WvW |
| D5 | P3-07 | Fully closed only when runtime code consumes slot-budget data via loaders (not at P3-06 dataset creation alone) |
| D6 | P3-15 | Reframed as heuristic in scorer isolation — not a factual CC model |
| D7 | P3-14 | Reframed as heuristic rotation profiles replacing preset system |
| D8 | P3-16 | Save/load profession persistence + crash-safe writes |

## Epic List

### Epic 3: Optimizer Correctness Overhaul and Patch-Aware Data-Driven Architecture

**User outcome**: GW2 players get accurate, mode-aware build optimization backed by verifiable game data. Calculations are correct for all 9 professions across PvE, PvP, and WvW. The optimizer can be updated for future balance patches without code changes. Scoring objectives are explicit and tunable. Saved builds reconstruct accurately.

**Design principles applied**:
- Stories are explicitly split between factual engine work, data infrastructure, and heuristic/scoring work
- Each story delivers data file + loader + code integration + tests as one unit
- Factual stories include cleanup of conflicting project-context and historical docs in ACs
- Phase C contains ONLY genuinely heuristic work (rotation profiles, objective profiles, scoring)

**Source document authority scope (L1/L3)**:
- `docs/optimizer-source-of-truth.md` is authoritative for architecture, classification, factual/heuristic boundaries, and structural requirements.
- Where live discrepancies exist between the source-of-truth document's numeric coefficients and the cited wiki/API sources, formula values must be **re-verified against the cited sources during implementation**. The source-of-truth doc has confirmed arithmetic discrepancies (e.g., Burning base constant, Confusion/Torment formulas). Implementers must not blindly transcribe — verify from wiki, then commit verified values.
- After epic implementation, `docs/optimizer-source-of-truth.md` and `docs/optimizer-data-schemas.md` must be updated to reflect verified values and any schema additions (e.g., `game_build_id`) so the documents stay aligned with the epic and code.

**Epic-wide implementation guardrails**:
- **No heuristic contamination in factual code (GR-1)**: No combat math function in Phase A or Phase B may contain a literal numeric game-balance assumption (e.g., "average Might stacks = 15", "typical buff uptime = 80%"). All variable inputs must be explicit function parameters. If a function needs buff stacks, it takes `might_stacks: u32` — never `Option<u32>` with a hardcoded default inside. Test fixtures may use explicit constants for testing, but production code paths must not embed heuristic assumptions. Violation = heuristic contamination; must be caught in review.
- **Source verification tiering (GR-2)**: API-native structured values (e.g., stat values returned by `/v2/items`) may use one authoritative API source. Hand-transcribed formulas, patch-split coefficients, and non-trivial derived constants require two independent verification paths where feasible (e.g., wiki formula + in-game tooltip, or wiki + API cross-check). Test expected values must cite their source in comments.
  - *Wiki-only formula note (S2)*: Condition formulas and some combat coefficients have no API source — wiki is the sole authority. For these values, document a manual spot-check protocol (equip known gear, read in-game tooltip, compare against formula output) as a secondary verification path. Community theorycrafting resources (Snow Crows, Lucky Noobs benchmarks) may serve as tertiary cross-references. Accept wiki as best-available with documented risk.
- **Accepted combat wiki sources (GR-3)**: The following wiki pages are valid primary sources for combat math, stacking rules, and duration behavior:
  - `Boon` — boon list, stacking modes, effects
  - `Condition` — condition list, stacking modes, base damage formulas
  - `Effect_stacking` — intensity vs duration stacking rules
  - `Boon_Duration` — concentration formula, boon duration cap, boon-specific duration modifiers
  - `Damage` — strike and condition damage formulas
  - `Attribute` — attribute definitions, scaling formulas
  - The `Diminishing_returns` page (https://wiki.guildwars2.com/wiki/Diminishing_returns) is NOT a valid source for combat math — it documents reward/loot diminishing returns, not combat stacking or damage formulas. No "condition diminishing returns" formula exists in GW2 combat. Do not cite this page for any combat-related data.
- **Boons and conditions are first-class optimization outputs (GR-4)**: The optimizer does not just maximize passive stats. Real build value often comes from: which boons the build generates and at what rate, how reliably it sustains those boons, which conditions it applies and at what pressure, whether those conditions suppress the opponent's output channels, and whether the build's boon package neutralizes incoming condition pressure. Boon generation/sustain/denial and condition application/cleanse/conversion are core GW2 build mechanics that must be modeled in the effect system (P3-10a/10b), the heuristic profile layer (P3-14), and the scoring system (P3-15). The 6-axis scoring model (power, condition, boon_support, healing, sustain, control) reflects this: boon contribution is a first-class axis, not a side effect buried in other axes.
- **Status state schema principle (GR-5)**: Model boons and conditions as typed status state, not generic modifiers. Factual layers define what statuses do and how they stack; heuristic layers define how often a given build creates, sustains, denies, or suffers those statuses. The schema contracts are: `StatusDefinition` (factual stack/cap/consumption/semantics metadata in P3-04), `StatusOperation` (typed interaction payloads on NormalizedEffect in P3-10a/10b), typed rotation profile sections (boon_generation, boon_uptime, condition_application, incoming_suppression in P3-14), and typed objective profiles with per-boon/per-condition priority maps (P3-15). This schema is implementation-safe: typed enough for GW2's real mechanics, but not so broad that Epic 3 becomes a frame-by-frame simulator.

---

#### Phase A — Factual Engine Correctness

Fix the combat math foundation so every calculation the player sees is source-backed and mode-aware. Each story delivers a data file, its typed loader, integration into the engine, and source-backed tests.

**P3-01: Profession Profiles and Health/Armor Truth**
FRs: FR1 | Fixes: D1
Delivers: `data/profession_profiles.json` + loader + integration + tests
AC includes: update project-context.md health class table
AC (GR-2): source verification per GR-2 tiering — health/armor values cite source in test comments

**P3-02: BalanceContext Type and Game-Mode Plumbing**
FRs: FR2
Delivers: BalanceContext struct, threaded through all mode-sensitive computation paths
Depends on: P3-01 (profession profiles exist for context validation)
Note: Large-scope story — touches many function signatures across crates/optimizer. May need sub-tasks.
Note (R1): some functions modified by P3-02 for BalanceContext plumbing will be rewritten by P3-03/P3-04/P3-05 with data-driven implementations. This is accepted minor rework — each story remains self-contained and the double-touch is preferable to merging concerns across stories.
AC (R4): before changing any signatures, produce an explicit pre-implementation audit checklist of all mode-sensitive paths. The checklist must include: function name, why it is mode-sensitive (reads formulas / coefficients / data files / validation rules / scoring inputs), and whether it has been updated to accept BalanceContext. This is a deliverable artifact, not implied review work.
AC (FM-02): audit all call sites reading mode-split coefficients. Remove no-context overloads entirely (delete, not deprecate). Add per-function test asserting PvE ≠ PvP output where coefficients differ.
AC (GR-1): no heuristic contamination — BalanceContext-parameterized functions must take all variable inputs as explicit parameters.
AC (S6): P3-02 must explicitly define how `BalanceContext.game_mode` is sourced at runtime. Acceptable initial approach: manual user selection. Auto-detection (e.g., via Mumble Link map ID) is out of scope for P3-02 but the architecture must not preclude it. If auto-detection is deferred, document this explicitly so the system does not imply it already exists.

**P3-03: Universal Attribute and Strike Damage Formulas**
FRs: FR3, FR4
Delivers: `data/formulas/universal.json` + loader + integration + tests
AC (GR-1): no heuristic contamination — formula functions take all variable inputs as explicit parameters.
AC (GR-2): source verification per GR-2 tiering — hand-transcribed formulas require two independent verification paths where feasible; test expected values cite source.
Note (R3, optional): the GW2 API returns skill `facts` with damage values. At least one test could pull a known skill's damage fact and verify the strike formula reproduces it (`skill_fact_damage * Power/1000 * 2597/target_armor`). This is an automated cross-check against API data — useful but not a blocker if the API data path is awkward.

**P3-04: Mode-Aware Boon Values and Condition Formulas**
FRs: FR5, FR6 | Fixes: D2, D3, D4
Delivers: `data/formulas/boons.json` (with `StatusDefinition`-style factual metadata per boon: stacking mode, max stacks, max duration, consumption mode, effect class, special mechanics, effect semantics) + `data/formulas/conditions.json` (with `StatusDefinition`-style metadata per condition: stacking mode, max stacks, effect class, secondary effects, suppression effects) + loaders + integration + tests + boon↔condition counterpart reference data
Depends on: P3-02 (BalanceContext required for mode dispatch)
AC includes: update project-context.md condition formula table and Fury documentation
AC (GR-1): no heuristic contamination — condition/boon functions take all variable inputs as explicit parameters.
AC (GR-2): mode-split coefficients (Fury, Torment, Confusion) require two independent verification paths where feasible; test expected values cite source.
AC (L1): verify and correct Burning level-80 base constant against the cited wiki source (known discrepancy between source-of-truth doc and wiki-derived value).
AC (L2): Torment formulas (PvE stationary/moving, PvP/WvW stationary/moving) must be re-verified against wiki before implementation. The source-of-truth values and live code values differ significantly — treat both as unverified until wiki-confirmed.
AC (L3): Confusion formulas (PvE over-time, PvE on-skill-use, PvP/WvW over-time, PvP/WvW on-skill-use) must be explicitly wiki-cross-checked before implementation. The source-of-truth values and live code values differ dramatically — neither should be assumed correct.
AC (L9): define and document `all_modes` vs per-mode precedence rule for the boon/modifier schema. Rule must specify: per-mode entry overrides `all_modes` when both exist for the same boon.

**P3-05: Duration Formulas**
FRs: FR7
Delivers: duration formula integration + tests
Depends on: P3-02 (BalanceContext for mode-aware duration modifiers)
AC (GR-1): no heuristic contamination — duration functions take all variable inputs as explicit parameters.

**P3-06: Canonical Slot-Budget Dataset**
FRs: FR8
Delivers: `data/slot_budgets/level80_ascended.json` + loader + tests
Note: D5 is NOT fully resolved here — D5 closes at P3-07 when runtime code consumes this dataset
AC (GR-2): slot-budget values are API-native structured data — one authoritative API source is acceptable; test expected values cite source.

---

#### Phase B — Data Infrastructure and Mode Paths

Build the patch-versioned, mode-aware data layer so the optimizer is resilient to balance patches and correctly separates PvE/PvP/WvW. Each story delivers end-to-end: data artifacts + loaders + engine wiring + tests.

**P3-07: Typed Loaders and Hardcoded Constant Replacement**
FRs: FR17, FR18 | Resolves: D5 (fully, by wiring slot-budget data into runtime)
Delivers: typed loader module infrastructure under `crates/optimizer/src/data/`, replaces all hardcoded factual constants with data file reads
Depends on: P3-01 through P3-06 (data files must exist to load)
AC (FM-05): all data enums use strict deserialization (no `#[serde(default)]` on required fields). Loaders return `Result<T, Vec<DataLoadError>>`, never panic or silently skip. Each loader must have a test feeding malformed data and asserting a specific typed error.
AC (R2): must define startup behavior when required data files are missing, corrupt, or fail validation:
- The addon itself must still start — do not crash or prevent the addon from loading.
- The optimizer subsystem enters a disabled/degraded state with an explicit user-visible error surfaced (e.g., "Optimizer data failed to load: [specific error]").
- No silent fallback to stale, fabricated, or hardcoded values.
- Test: simulate missing/corrupt data file at startup and assert the optimizer reports the error and refuses to produce results until data is available.

---

**MVD-1: Minimum Viable Delivery Milestone** (after P3-07, with P3-16 ideally complete or in progress)

At this point the optimizer is materially better than today and releasable:
- Phase A complete: all combat formulas correct, mode-aware, source-backed
- Typed loaders and schema-backed reads in place (D5 fully resolved)
- Defects resolved: D1, D2, D3, D4, D5 + D8 (if P3-16 complete)
- 6 of 8 confirmed defects fixed; remaining D6/D7 are heuristic-layer concerns addressed in Phase C
- Phase B/C work is valuable but not blocking a release — the optimizer can ship here with correct math and honest mode separation

---

**P3-08: Patch Manifest and Patch Ledger Infrastructure**
FRs: FR21, FR22
Delivers: `data/manifests/` + `data/patch_ledgers/` + loaders + validation + initial 2026-01-13 snapshot
AC (FM-07, refined by S4): patch manifest must include a `game_build_id` field (the GW2 API `/v2/build` integer). At runtime, compare `/v2/build` against the latest manifest's `game_build_id` (not `patch_id`, which is a date string). Staleness behavior is tiered:
- `game_build_id` mismatch alone = **informational indicator** ("Newer game build detected — balance data not yet verified for this build"). Does NOT automatically downgrade to `DataQuality::Provisional`.
- `DataQuality::Provisional` triggers only when: (a) a specific value is identified as stale via ledger entry or override state, OR (b) an `Unknown` value is actually consumed in the computation path.
- `DataQuality::Blocked` triggers only when no safe degraded path exists (e.g., formula shape changed, required value missing with no stale fallback).
Rationale: not every GW2 build update is a balance patch. Automatic Provisional on every build bump creates false alarms for a solo-maintained addon.
AC (L7): update `docs/optimizer-data-schemas.md` Schema 1 (Patch Manifest) to include `game_build_id` field. The schema doc must stay aligned with the epic's manifest requirements.

**P3-09: Balance Override Datasets and Unknown-Value Handling**
FRs: FR10, FR14
Delivers: `data/balance_overrides/<patch>/<mode>.json` + Unknown-value runtime behavior (block/degrade factual paths)
Depends on: P3-08 (patch manifests and ledgers must exist for override references)
AC must include:
- `DataQuality` enum on optimizer output: `Verified | Provisional | Blocked`
  - Verified: all required factual values resolved for the path used
  - Provisional: stale or heuristic fallback used, but output still allowed
  - Blocked: required value missing and no safe fallback exists, or formula shape changed
- Structured `Vec<DataQualityReason>` (or equivalent) explaining why output is Provisional or Blocked, with affected fields/entities/modes
- UI-surfaceable: the quality indicator and reasons must be available for display
AC (FM-03): data lookup layer must not have a PvE default fallback. When WvW/PvP data is missing, lookup returns `None` or `Unknown`, forcing explicit caller handling. Pattern: `fn get_coefficient(ctx: &BalanceContext, key: &str) -> Option<f64>` — never `fn get_coefficient_or_pve(...)`.
AC (FM-04): `Unknown` must be a distinct type in the value system, not representable as bare `f64`. Arithmetic on Unknown must propagate Unknown state. Test that an Unknown coefficient in the formula chain produces `DataQuality::Provisional`, not a numeric result with `DataQuality::Verified`.
HIGH RISK: Unknown-value handling adds non-trivial architectural scope (DataQuality + DataQualityReason). Review gate required before P3-10a and P3-12 proceed.

**P3-10a: NormalizedEffect Types, Schema, and Contracts**
FRs: FR12 (partial — type system and schema)
Delivers: NormalizedEffect struct with 23 effect categories (17 from source-of-truth + 6 boon/condition interaction operations) + `StatusOperation`-style structured payloads on interaction categories (operation_type, target_side, status_kind, amount_mode, amount_value, base_duration_ms, target_scope, target_count, internal_cooldown_ms) + timer/ICD/cap metadata fields + stacking rules + trigger rules + uptime model slots + evidence level per effect; JSON schema under `data/normalized_effects/`; typed loader
Depends on: P3-04 (StatusDefinition metadata), P3-08 (patch plumbing), P3-09 (balance overrides and Unknown handling)
Note: effect categories are factual/derived; uptime model slots are defined here but populated with heuristic values in P3-14; boon/condition interaction categories (RemovesBoon, StealsBoon, CorruptsBoon, RemovesCondition, ConvertsConditionToBoon, TransfersCondition) are first-class because boon denial/corruption and condition cleanse/conversion are core GW2 build mechanics
AC (C4): any numeric field in NormalizedEffect that can be unresolved must use the P3-09 Unknown/FactualValue type system. Do not use raw `f64` + `evidence_level: Unknown` as a parallel system. One numeric uncertainty model only.
HIGH RISK: largest analysis scope in the epic (23 effect categories across traits/skills/upgrades including boon/condition interactions). Review gate required before P3-10b proceeds.

**P3-10b: Effect Extraction and Population (Factual/Derived Only)**
FRs: FR12 (partial — population of factual/derived effect data)
Delivers: populated `data/normalized_effects/<patch>/<mode>.json` files by extracting factual/derived effects from traits, skills, and upgrades; integration into engine. Uptime model slots are left empty or null — heuristic uptime values are deferred to P3-14.
Depends on: P3-10a (types and schema must be stable before population)
AC (FM-08): coverage report listing every effect-source entity with numeric effects, mapped to NormalizedEffect category. Must cover all source types in scope: traits, skills, runes, sigils, relics. Any entity with a numeric effect that has no mapping must be flagged. Count cross-referenced against GW2 API endpoints for each source type. If P3-10b is later split by source type, scope the coverage report to the sources included in that sub-story.
AC (C3): P3-10b populates factual/derived effect data only (categories, stacking rules, trigger rules, evidence levels). Heuristic uptime estimates are NOT populated here — they are deferred to P3-14 (rotation profiles) to preserve the Phase B / Phase C boundary.
HIGH RISK: data completeness uncertain — extracting and classifying effects across all professions is significant research. Review gate required before downstream stories proceed.

**P3-11: PvP Optimizer Path Separation**
FRs: FR9
Delivers: distinct PvP optimization route bypassing gear-prefix search, using amulet system
Depends on: P3-02 (BalanceContext for mode dispatch)
Conditional parallel: can start after P3-02 IF scoped to route separation + entry-point split + gear-prefix bypass only. If the story also requires schema-backed PvP data integration (e.g., amulet stat tables loaded from data files), then it depends on P3-07.
AC (FM-09): PvP optimization entry point must be gated behind a data-readiness check. Invoking PvP optimization without loaded amulet stat data must return `DataQuality::Blocked`, not a result.
AC (L10): P3-11 must define and deliver a PvP amulet stat schema or dataset. Without this, PvP route separation ships without usable PvP stat data. Either add a PvP amulet schema to `docs/optimizer-data-schemas.md` or deliver the amulet stat dataset as a P3-11 artifact.

**P3-12: WvW Non-Fallback Behavior**
FRs: FR11
Delivers: WvW-specific balance data consumption, explicit degradation when split data is missing
Depends on: P3-09 (balance overrides provide the mode-specific data)
AC (FM-03): test that requests a WvW-only coefficient with no WvW data file present and asserts the result is `DataQuality::Provisional` or `DataQuality::Blocked`, not a PvE number.

**P3-13: Factorized Dependency Tables and Evidence Classification**
FRs: FR15, FR16
Delivers: evidence level classification across all tables, cross-file consistency validation
Depends on: all Phase A + P3-07 through P3-10b (tables must exist to classify). Blocked by Review Gate RG-3.

---

#### Phase C — Heuristic Layer and Scoring Isolation

Separate heuristic logic from factual math. Every story in this phase is explicitly heuristic — no factual engine changes.

**P3-14: Rotation Profiles and Heuristic Uptime Population**
FRs: FR13 | Addresses: D7 (replaces preset condition weighting)
Delivers: `data/rotation_profiles/<mode>.json` + loader + typed RotationProfile with top-level sections: `boon_generation` (by boon kind), `boon_uptime` (by boon kind), `condition_application` (by condition kind), `incoming_suppression` (by condition/control kind), `target_behavior`, `scenarios` (buff environment variations). Also populates heuristic uptime values in NormalizedEffect data (deferred from P3-10b). Runtime enforces factual caps/stacking/timers from P3-04/P3-10b.
Depends on: P3-04 (StatusDefinition metadata for cap/stacking enforcement), P3-07 (typed loaders), P3-10b (populated factual/derived effect data to attach uptime estimates to)
Evidence level: Heuristic (explicitly labeled)
AC must include: end-to-end integration smoke test exercising the full factual+heuristic pipeline

**P3-15: Objective Profiles and Typed State-Aware Scorer Isolation (6-axis)**
FRs: FR20 | Addresses: D6 (replaces fake disable proxy with heuristic control axis)
Delivers: `data/objective_profiles/<mode>.json` + 6-axis ObjectiveScorer (power, condition, boon_support, healing, sustain, control) + typed `boon_priorities` + typed `condition_priorities` + typed `interaction_priorities` + separation from factual engine
Depends on: P3-14 (rotation profiles), P3-10b (typed effect data), P3-13 (evidence/cross-file validation)
Evidence level: Heuristic (explicitly labeled)
AC must include: end-to-end integration smoke test exercising the full optimization pipeline (factual engine → rotation profile → objective scorer → ranked output)

---

#### Standalone

**P3-16: Save/Load Profession Persistence and Crash Safety**
FRs: FR19 | Fixes: D8
Delivers: profession-aware combat metric reconstruction + crash-safe atomic writes
Supersedes: P2-07 (all P2-07 scope is absorbed; P2-07 is retired as a separate story)
Can be executed at any point after P3-01 (needs profession profiles)
Scheduling recommendation: start early, in parallel with P3-02 or soon after P3-01. Scores highest of any non-Phase-A story (self-contained, no wiki research, fixes confirmed defect).
AC (FM-06): recalculate combat metrics on load if engine version OR active balance manifest version (patch snapshot) has changed since save. Both are invalidation boundaries — saved data includes both version keys.

---

### Parallelization Notes

Hard architecture dependencies always override weighted scoring. The following parallelization opportunities are available within that constraint:

| Track | Stories | Condition |
|-------|---------|-----------|
| After P3-01 completes | P3-02 + P3-16 in parallel | P3-16 only needs profession profiles; P3-02 is the architecture gate for mode-aware work |
| After Phase A completes | P3-07 + P3-08 in parallel | P3-08 (patch manifests/ledgers) has no hard dependency on P3-07 (typed loaders). Both depend on Phase A data files existing. |
| After P3-02 completes | P3-11 (conditionally) | Only if P3-11 is scoped to route separation, entry-point split, and gear-prefix bypass. If it needs schema-backed PvP data integration, it depends on P3-07. |
| P3-03, P3-05, P3-06 | Mutually independent | No inter-dependencies; can overlap if capacity allows. P3-04 depends on P3-02. |

P3-02 remains early despite lower weighted score than P3-16 because it is the structural gate for all mode-aware correctness (P3-04, P3-05, P3-11, P3-12).

### Review Gates

P3-09 and P3-10a/10b are the two main uncertainty stories in the epic. Both carry data-completeness and architectural-scope risk.

| Gate | After | Before proceeding to | Gate criteria |
|------|-------|---------------------|---------------|
| **RG-1** | P3-09 completes | P3-10a, P3-12 | DataQuality/DataQualityReason design reviewed and accepted. Unknown-value handling tested. Balance override data audited for completeness. |
| **RG-2** | P3-10a completes | P3-10b | NormalizedEffect type system and schema reviewed. 23 effect categories validated (17 from source-of-truth + 6 boon/condition interaction categories from wiki). Interaction payloads reviewed. Stacking/trigger rules confirmed. |
| **RG-3** | P3-10b completes | P3-13 | Populated effect data reviewed for coverage and accuracy. Evidence levels assigned. Cross-file consistency validated. |

No downstream story may proceed past a review gate without explicit sign-off.

### Follow-Up Backlog (Outside Epic 3 Scope)

Items identified during epic design that are valuable but not blockers for Epic 3:

- **FU-1: Formula correction UX notification (S5)**: When Phase A formula fixes change DPS numbers on saved builds, the user needs some indication that the change is an intentional correction, not a bug. Options: one-time "Engine updated" notification, version note in saved build data, or in-addon changelog. This is a UI/UX concern, not engine correctness — defer to a separate story.
- **FU-2: Source document sync (L5/L7)**: After epic implementation, update `docs/optimizer-source-of-truth.md` and `docs/optimizer-data-schemas.md` to reflect: verified formula coefficients, `game_build_id` in manifest schema, PvP amulet schema, `all_modes`/per-mode precedence rule, corrected effect category count (23: 17 original + 6 boon/condition interaction categories), boon/condition stacking mode and cap metadata contract (intensity vs duration, max_stacks per boon/condition), NormalizedEffect timer/ICD/cap fields (`effect_duration`, `internal_cooldown`, `max_stacks`), boon/condition interaction operation categories (RemovesBoon, StealsBoon, CorruptsBoon, RemovesCondition, ConvertsConditionToBoon, TransfersCondition) with structured payloads, 6-axis scoring model (power, condition, boon_support, healing, sustain, control) replacing the original 5-axis model, per-boon priority weights (`boon_priorities`), per-condition priority weights (`condition_priorities`), and per-interaction priority weights (`interaction_priorities`) in objective profiles, `StatusDefinition`-style factual metadata contract (stacking mode, caps, consumption mode, effect semantics, duration bonus), `StatusOperation`-style typed interaction payloads for NormalizedEffect, rotation profile typed sections (boon_generation, boon_uptime, condition_application, incoming_suppression, target_behavior, scenarios), suppression effect metadata (Slow/Chilled/Weakness/Blind/Fear/Taunt/Immobile/Crippled), accepted combat wiki sources (`Boon`, `Condition`, `Effect_stacking`, `Boon_Duration`, `Damage`, `Attribute`), exclusion of `Diminishing_returns` as a combat source, and any other changes discovered during implementation. The epic must not become more correct than the source documents — that inverts the authority hierarchy.

### Dependency Convention

In this epic, **Depends on** means a strict DAG edge — the listed story must be complete before this one starts. **Recommended order** means sequencing preference but not a hard gate. Stories use whichever applies.

---

## Story Details

### Story 3.1: P3-01 — Profession Profiles and Health/Armor Truth

As a **GW2 player**,
I want the optimizer to use correct base health and defense values for every profession,
So that all derived stats (effective HP, toughness, damage mitigation) are accurate and I can trust the optimizer's survivability comparisons.

**Acceptance Criteria:**

**Given** optimizer data is initialized
**When** it resolves a Guardian's base health at level 80
**Then** it returns `1645` (Low health class), not `9212`
**And** the value is loaded from `data/profession_profiles.json`, not hardcoded in Rust source

**Given** optimizer data is initialized
**When** it resolves a Necromancer's base health at level 80
**Then** it returns `9212` (High health class), not `5922`
**And** the value is loaded from `data/profession_profiles.json`, not hardcoded in Rust source

**Given** `data/profession_profiles.json` exists
**When** the loader validates the file
**Then** it contains exactly 9 entries (one per profession)
**And** each entry has independent `armor_weight` and `health_class` fields that are never inferred from each other
**And** `base_defense_level_80` matches the armor class (Heavy=1271, Medium=1118, Light=967)
**And** each entry includes `evidence_level: "Factual"` and `sources` array citing wiki URLs

**Given** `profession_profiles.json` contains a malformed enum, duplicate profession, or fewer than 9 professions
**When** the loader runs
**Then** it returns a typed validation error and does not silently default

**Given** any code path resolves profession base health or base defense
**When** the story is complete
**Then** all such lookups route through the loaded profession profile source
**And** no duplicate hardcoded tables remain in `crates/optimizer/src/stats.rs` or `crates/core/src/types.rs`

**Given** the profession profiles are loaded successfully
**When** any combat math function needs a profession's base health or defense
**Then** it reads from the loaded in-memory profile struct, not from any hardcoded match arm

**Given** the story is complete
**When** `_bmad-output/project-context.md` is reviewed
**Then** the health class table has been updated to reflect Guardian=Low, Necromancer=High

**Given** GR-2 source verification
**When** test expected values are written
**Then** each profession's health and defense values cite their wiki source in test comments

**Requirements**: FR1 | **Fixes**: D1 | **Delivers**: `data/profession_profiles.json` + loader (profession-profiles-only, not generic infrastructure) + integration replacing all hardcoded lookup sites + tests + project-context.md cleanup

**Scope boundary**: This story introduces a concrete data loader for profession profiles only. Generic loader infrastructure and full startup lifecycle are deferred to P3-07.

---

### Story 3.2: P3-02 — BalanceContext Type and Game-Mode Plumbing

As a **GW2 player optimizing for PvP or WvW**,
I want the optimizer to know which game mode I'm optimizing for and thread that context through every mode-sensitive calculation,
So that I never receive PvE-tuned results when optimizing for a competitive mode.

**Acceptance Criteria:**

**Given** the `BalanceContext` type is defined
**When** inspected
**Then** it contains at minimum `patch_id: String` and `game_mode: GameMode` where `GameMode` is an enum of `PvE`, `PvP`, `WvW`

**Given** `patch_id` is part of `BalanceContext` but manifest infrastructure is not yet implemented
**When** P3-02 completes
**Then** `patch_id` sourcing is explicitly documented as temporary (caller-supplied or initialized from a current-snapshot constant)
**And** it is not hidden behind global state
**And** authoritative manifest-backed sourcing is deferred to P3-08

**Given** P3-02 implementation begins
**When** the pre-implementation audit is performed (R4)
**Then** a checklist artifact is produced listing every mode-sensitive function: function name, why it is mode-sensitive (reads formulas / coefficients / data files / validation rules / scoring inputs), and whether it has been updated to accept `BalanceContext`
**And** the checklist includes all mode-sensitive paths across combat math, validation, search/routing, and optimization entry pipelines — not only formula functions

**Given** a function that reads a mode-split coefficient (e.g., Fury bonus, condition formula, trait modifier)
**When** the story is complete
**Then** every such function accepts `BalanceContext` as a parameter
**And** no no-context overloads remain (deleted, not deprecated)

**Given** a mode-sensitive formula function is called with a PvE context and then with a PvP context
**When** the coefficients differ between modes (e.g., Fury: 25% vs 20%)
**Then** the function produces different results for PvE vs PvP
**And** at least one per-function test asserts this mode differentiation

**Given** the story must define how `game_mode` is sourced at runtime (S6)
**When** the implementation is reviewed
**Then** the initial approach is documented: manual user selection is acceptable
**And** the architecture does not preclude future auto-detection (e.g., via Mumble Link map ID)
**And** if auto-detection is deferred, this is stated explicitly so the system does not imply it already exists

**Given** GR-1 (no heuristic contamination)
**When** BalanceContext-parameterized functions are implemented
**Then** all variable inputs are explicit parameters — no hardcoded defaults for buff stacks, uptimes, or other game-balance assumptions

**Given** some functions modified by P3-02 will be rewritten by P3-03/P3-04/P3-05 (R1)
**When** the story is scoped
**Then** this accepted minor rework is acknowledged — P3-02 plumbs context into existing functions; later stories replace those functions with data-driven implementations

**Requirements**: FR2 | **Recommended order**: after P3-01 | **Delivers**: `BalanceContext` struct + `GameMode` enum + pre-implementation audit checklist + plumbing through all mode-sensitive computation paths (combat math, validation, search/routing, optimization entry) + mode-differentiation tests

**Scope boundary**: Large-scope story — touches many function signatures across `crates/optimizer/`. May need sub-tasks. Does NOT introduce data file loading or formula replacement — those are P3-03 through P3-06. Does NOT implement auto-detection of game mode or manifest-backed patch_id sourcing — those are follow-ups (P3-08).

---

### Story 3.3: P3-03 — Universal Attribute and Strike Damage Formulas

As a **GW2 player**,
I want the optimizer to calculate attributes, critical chance, critical damage, and strike damage using the exact wiki-documented formulas loaded from data,
So that the base math underlying every build comparison is verifiably correct.

**Acceptance Criteria:**

**Given** `data/formulas/universal.json` exists and is loaded during optimizer data initialization
**When** the loader validates the file
**Then** it contains: `base_primary_attribute` (1000), `vitality_to_health` (10), `precision_offset` (895), `precision_per_crit_pct` (21), `ferocity_per_crit_damage_pct` (15), `expertise_per_condition_duration_pct` (15), `concentration_per_boon_duration_pct` (15), `condition_duration_cap` (1.0), `boon_duration_cap` (1.0), `tooltip_reference_armor` (2597)
**And** the file includes `evidence_level: "Factual"` and `sources` citing wiki URLs

**Given** a character has Precision of 2000
**When** critical chance is calculated
**Then** the result is `(2000 - 895) / 21 = 52.619...%`
**And** the formula reads `precision_offset` and `precision_per_crit_pct` from the loaded data, not from hardcoded constants

**Given** Precision produces a raw critical chance above 100%
**When** effective critical chance is used in combat math
**Then** it is capped at 1.0 (ratio-based)
**And** all crit-chance values in the optimizer are represented as ratios (0.0–1.0), not percent points, to avoid unit ambiguity

**Given** a character has Ferocity of 300
**When** critical damage bonus is calculated
**Then** the result is `300 / 15 = 20%` bonus (total 170% with base 150%)
**And** the divisor is read from the loaded `ferocity_per_crit_damage_pct`

**Given** strike damage is calculated
**When** the formula is applied
**Then** the result is `skill_damage_term * (Power / 1000) * (tooltip_reference_armor / target_armor)`
**And** `tooltip_reference_armor` is read from the loaded data (2597), not hardcoded
**And** `skill_damage_term` is the already-normalized damage input used by the optimizer (API-reported tooltip damage fact or equivalent pre-computed term) — P3-03 does not model weapon-strength or skill-coefficient extraction; if that decomposition is needed, it is a separate concern

**Given** boon duration or condition duration bonus is calculated
**When** the result exceeds 100%
**Then** it is capped at `1.0` (the cap value from loaded data)

**Given** `universal.json` contains a mode-specific field
**When** the loader validates
**Then** validation fails — no mode-specific values are allowed in this file

**Given** all universal formula constants used by optimizer runtime paths
**When** the story is complete
**Then** they are read from the loaded universal formula source
**And** duplicate hardcoded constants are removed from affected code paths

**Given** GR-1 (no heuristic contamination)
**When** formula functions are implemented
**Then** all variable inputs (Power, Precision, Ferocity, target armor, skill damage, etc.) are explicit parameters

**Given** GR-2 (source verification)
**When** test expected values are written
**Then** hand-transcribed formulas cite two independent verification paths where feasible; test comments cite wiki sources

**Given** R3 (optional API cross-check)
**When** feasible
**Then** at least one test pulls a known skill's damage fact from the GW2 API and verifies the strike formula reproduces it — useful but not a blocker if the API data path is awkward

**Requirements**: FR3, FR4 | **Recommended order**: after P3-01 (no hard dependency on P3-02 — universal formulas are mode-invariant) | **Delivers**: `data/formulas/universal.json` + loader + formula functions replacing hardcoded constants + tests

**Scope boundary**: Universal, mode-invariant formulas only. Mode-split formulas (boons, conditions) are P3-04. Duration formulas are P3-05. Weapon-strength/skill-coefficient extraction is out of scope unless already modeled in existing code.

---

### Story 3.4: P3-04 — Mode-Aware Boon Values and Condition Formulas

As a **GW2 player optimizing for PvP or WvW**,
I want boon effects and condition damage formulas to use the correct mode-specific values,
So that my build's DPS and support calculations reflect the actual game balance for my chosen mode.

**Acceptance Criteria:**

**Given** `data/formulas/boons.json` exists and is loaded during optimizer data initialization
**When** its scope is reviewed
**Then** it contains boon effects (Fury, Might, Protection, Resolution) plus shared combat debuff modifiers (Vulnerability)
**And** this mixed scope is explicitly documented in the file header or schema — the file is not misrepresented as boon-only

**Given** the schema for `boons.json` entries
**When** reviewed
**Then** each entry's value fields are explicitly typed as one of: `flat_additive` (e.g., Might: +30 Power per stack), `ratio` (e.g., Protection: 0.67 multiplier), `ratio_per_stack` (e.g., Vulnerability: 0.01 per stack), or `max_stacks` (integer cap)
**And** flat additive values and ratio values are never ambiguously mixed — the field name or type tag distinguishes them

**Given** Fury crit chance bonus is looked up with a PvE `BalanceContext`
**When** the value is resolved
**Then** it returns `0.25` (25%)
**When** looked up with a PvP or WvW `BalanceContext`
**Then** it returns `0.20` (20%)
**And** these values are loaded from the data file, not hardcoded

**Given** `boons.json` contains both `all_modes` and per-mode entries for the same boon
**When** the loader resolves the value
**Then** per-mode entry takes precedence over `all_modes`
**And** this precedence rule is documented in the schema and tested

**Given** Might, Vulnerability, Protection, and Resolution are looked up
**When** any `BalanceContext` is provided
**Then** Might returns `+30 Power` and `+30 Condition Damage` per stack (flat additive, per-stack)
**And** Vulnerability returns `+0.01 damage per stack` (ratio per-stack), max 25 stacks
**And** Protection returns `0.67` incoming strike multiplier (ratio)
**And** Resolution returns `0.67` incoming condition multiplier (ratio)

**Given** `data/formulas/conditions.json` exists and is loaded during optimizer data initialization
**When** Burning tick damage is calculated at 0 Condition Damage
**Then** the base constant matches the wiki-verified level-80 value (verify against cited wiki source — known discrepancy between source-of-truth doc value `131` and wiki-derived `131.75`)
**And** the coefficient is `0.155`

**Given** Torment tick damage is calculated
**When** the `BalanceContext` is PvE
**Then** stationary and moving formulas are separate: both coefficients and bases are loaded from `conditions.json` and wiki-verified before implementation (L2)
**When** the `BalanceContext` is PvP or WvW
**Then** PvP/WvW-specific stationary and moving formulas are used, also wiki-verified

**Given** Confusion damage is calculated
**When** the `BalanceContext` is PvE
**Then** over-time and on-skill-use formulas are separate, wiki-verified before implementation (L3)
**When** the `BalanceContext` is PvP or WvW
**Then** PvP/WvW-specific over-time and on-skill-use formulas are used, also wiki-verified

**Given** Torment is calculated with identical inputs
**When** the `BalanceContext` switches between PvE and PvP
**Then** the results differ where coefficients differ
**When** the movement state switches between stationary and moving under the same mode
**Then** the results differ
**And** tests assert both mode-dispatch and state-dispatch produce different outputs

**Given** Confusion is calculated with identical inputs
**When** the `BalanceContext` switches between PvE and PvP
**Then** the results differ where coefficients differ
**When** the trigger state switches between over-time and on-skill-use under the same mode
**Then** the results differ
**And** tests assert both mode-dispatch and trigger-dispatch produce different outputs

**Given** Bleeding and Poison are calculated
**When** any `BalanceContext` is provided
**Then** Bleeding uses `0.06 * CD + 22` and Poison uses `0.06 * CD + 33.5`
**And** these are currently the same across all modes but are still stored per-mode in the data file (future-proofing for potential splits)

**Given** condition formulas are loaded from `conditions.json`
**When** the file is validated
**Then** every condition declares all three modes (PvE, PvP, WvW)
**And** multi-state conditions (Torment, Confusion) declare their state dimensions explicitly (stationary/moving, over-time/on-skill-use)
**And** mode-specific splits are not collapsed

**Given** the boon and condition data files
**When** each entry is reviewed
**Then** every boon entry includes factual stacking metadata:
  - `stacking_mode`: either `"intensity"` (stacks in number, e.g., Might, Stability) or `"duration"` (stacks in duration, e.g., Fury, Protection, Resolution, Quickness, Alacrity)
  - `max_stacks`: integer cap for intensity boons (Might: 25, Stability: 25); for duration boons this is typically 1 (single instance, extended by duration stacking)
  - `max_duration`: maximum stacked duration in seconds where factually known (most boons: 30s; Swiftness: 60s; per https://wiki.guildwars2.com/wiki/Effect_stacking)
  - `base_duration`: null or omitted at this layer — base durations are per-source (skill/trait), not per-boon-type
  - `effect_class`: categorization of the boon's functional role: `"offensive_throughput"` (Quickness, Alacrity — changes action/recharge rate, not a stat modifier), `"offensive_stat"` (Might, Fury — stat or chance modifier), `"defensive"` (Protection, Resolution, Aegis, Stability, Resistance), `"sustain"` (Regeneration, Vigor), `"utility"` (Swiftness)
  - `special_mechanics`: optional field for boons with non-standard behavior:
    - Aegis: blocks next incoming attack, consumed on block (not duration-stacking in the normal sense — single-use per application, https://wiki.guildwars2.com/wiki/Aegis)
    - Resistance: nondamaging conditions on you become ineffective while active (does not remove them, https://wiki.guildwars2.com/wiki/Resistance)
    - Quickness: increases action speed — this changes cast throughput, not a flat stat bonus (https://wiki.guildwars2.com/wiki/Quickness)
    - Alacrity: reduces skill recharge time — this changes skill frequency, not a flat stat bonus (https://wiki.guildwars2.com/wiki/Alacrity, https://wiki.guildwars2.com/wiki/Recharge)
**And** every condition entry includes factual stacking metadata:
  - `stacking_mode`: `"intensity"` (most damaging conditions: Bleeding, Burning, Torment, Confusion, Poisoned) or `"duration"` (most control/suppression conditions: Fear, Taunt, Daze, Blind, Chilled, Immobile, Crippled, Slow, Weakness)
  - `max_stacks`: integer cap for intensity conditions (Vulnerability: 25; most intensity damaging conditions: effective cap of 1500)
  - `effect_class`: categorization of the condition's functional role: `"damage"` (Bleeding, Burning, Torment, Confusion, Poisoned), `"debuff"` (Vulnerability — multiplicative damage taken increase), `"suppression"` (Weakness, Chilled, Slow, Blind — reduces the target's output channels), `"control"` (Fear, Taunt, Daze, Immobile, Crippled — restricts target actions/positioning)
  - `secondary_effects`: optional field for conditions with effects beyond their primary damage/control:
    - Poisoned: also reduces healing effectiveness by 33% (https://wiki.guildwars2.com/wiki/Poisoned)
    - Vulnerability: +1% strike and condition damage taken per stack, max 25 (https://wiki.guildwars2.com/wiki/Vulnerability)
    - Blind: next outgoing attack misses — suppresses the next action, not continuous (https://wiki.guildwars2.com/wiki/Blinded)
**And** these metadata fields are factual game data, not heuristic — they describe the game's stacking rules and functional roles, not assumptions about uptime or application

**Given** conditions that suppress or deny output channels
**When** suppression and control conditions are modeled in `conditions.json`
**Then** each includes a factual `suppression_effects` metadata field documenting the output channel it reduces:
  - `Slow`: reduces skill activation speed — suppresses action throughput (cast rate), therefore boon generation rate and DPS output rate (https://wiki.guildwars2.com/wiki/Slow)
  - `Chilled`: -66% movement speed, increases skill cooldowns by 66% — suppresses skill frequency and therefore boon/condition application rate (https://wiki.guildwars2.com/wiki/Chilled)
  - `Weakness`: -50% endurance regeneration and 50% of non-critical hits become glancing blows — suppresses strike DPS output and dodge economy (https://wiki.guildwars2.com/wiki/Weakness)
  - `Blind`: next outgoing attack misses — suppresses one action's output entirely, per-hit basis (https://wiki.guildwars2.com/wiki/Blinded)
  - `Fear`: involuntary retreat, unable to act — complete action denial for duration (https://wiki.guildwars2.com/wiki/Fear)
  - `Taunt`: forced to attack the source — action override, denies skill choice (https://wiki.guildwars2.com/wiki/Taunt)
  - `Immobile`: prevents movement — positioning denial, affects melee range and escape (https://wiki.guildwars2.com/wiki/Immobile)
  - `Crippled`: -50% movement speed — positioning suppression, weaker than Immobile (https://wiki.guildwars2.com/wiki/Crippled)
**And** these are factual game mechanics sourced from the individual condition wiki pages, not heuristic assumptions
**And** the suppression metadata documents which output channels each condition affects: `action_throughput` (Slow, Fear, Taunt), `skill_frequency` (Chilled), `strike_output` (Weakness, Blind), `dodge_economy` (Weakness), `positioning` (Immobile, Crippled, Chilled, Fear)
**And** this metadata is informational for P3-14 (heuristic runtime) — rotation profiles use it to model how incoming suppression conditions reduce effective build output in each scenario
**And** this does NOT model boon removal, corruption, or conversion — those are effect-level operations modeled in P3-10a/P3-10b

**Given** boon/condition counterpart interactions exist in GW2
**When** the data files are authored
**Then** a reference metadata section (in `boons.json` or a separate reference doc) documents the factual boon↔condition counterpart relationships:
  - Certain traits/skills corrupt boons into specific conditions (e.g., Might → Weakness, Fury → Blind, Protection → Vulnerability)
  - Certain traits/skills convert conditions into specific boons (e.g., the reverse)
  - Boon removal, boon steal, condition cleanse, and condition transfer are distinct first-class operations
**And** this counterpart mapping is factual reference data from `wiki.guildwars2.com/wiki/Boon` and `wiki.guildwars2.com/wiki/Condition`
**And** the actual effect-level modeling of these operations (who applies them, when, how often) is P3-10a/P3-10b scope, not P3-04

**Given** the source citations for boon/condition stacking metadata
**When** the data is authored
**Then** accepted wiki sources for combat stacking and duration rules include:
  - `wiki.guildwars2.com/wiki/Boon` (boon list, stacking modes, caps)
  - `wiki.guildwars2.com/wiki/Condition` (condition list, stacking modes, caps)
  - `wiki.guildwars2.com/wiki/Effect_stacking` (intensity vs duration stacking rules)
  - `wiki.guildwars2.com/wiki/Boon_Duration` (boon duration caps, concentration formula, boon-specific duration modifiers)
  - `wiki.guildwars2.com/wiki/Damage` (strike/condition damage formulas)
  - `wiki.guildwars2.com/wiki/Attribute` (attribute definitions and formulas)
**And** the `wiki.guildwars2.com/wiki/Diminishing_returns` page is NOT a valid source for combat math — it covers reward/loot DR, not combat stacking or damage formulas
**And** no "condition diminishing returns" formula is invented or implied — GW2 conditions do not have diminishing returns on damage; they stack by intensity or duration per the Effect Stacking rules

**Given** all boon and condition formula constants used by optimizer runtime paths
**When** the story is complete
**Then** they are read from the loaded data sources
**And** duplicate hardcoded boon/condition constants are removed from affected code paths

**Given** the story is complete
**When** `_bmad-output/project-context.md` is reviewed
**Then** the condition formula table and Fury documentation have been updated to reflect verified values

**Given** GR-1 (no heuristic contamination)
**When** condition/boon functions are implemented
**Then** all variable inputs (Condition Damage, stacks, mode, movement state, skill activation) are explicit parameters

**Given** GR-2 (source verification)
**When** test expected values are written for Fury, Torment, Confusion, and Burning
**Then** mode-split coefficients cite two independent verification paths where feasible; test comments cite wiki sources

**Requirements**: FR5, FR6 | **Fixes**: D2, D3, D4 | **Depends on**: P3-02 (BalanceContext required for mode dispatch) | **Delivers**: `data/formulas/boons.json` (boon effects + shared combat debuff modifiers + `StatusDefinition`-style factual metadata per boon: stacking mode, max stacks, max duration, consumption mode, effect class, special mechanics, effect semantics) + `data/formulas/conditions.json` (condition formulas + `StatusDefinition`-style factual metadata per condition: stacking mode, max stacks, effect class, secondary effects, suppression effects) + loaders + mode-aware formula functions with mode-dispatch and state-dispatch tests + project-context.md cleanup

**Scope boundary**: Boon effects, combat debuff modifiers, condition tick formulas, and factual `StatusDefinition` metadata only. Each boon/condition entry carries typed status state metadata: stacking mode, caps, consumption mode (for consumable boons like Aegis), effect semantics (what the status does), and effect class (functional role). Duration formulas (how long conditions/boons last) are P3-05. This story does not model condition application rates or uptime — those are heuristic concerns in P3-14. Status metadata is factual game data delivered here; later stories (P3-10a, P3-14, P3-15) use it for type constraints, runtime cap enforcement, and typed priority valuation. The `Diminishing_returns` wiki page is not a valid combat source — GW2 conditions do not have diminishing returns on damage.

---

### Story 3.5: P3-05 — Duration Formulas

As a **GW2 player**,
I want condition and boon durations to be calculated correctly using expertise, concentration, and explicit duration modifiers,
So that builds investing in duration stats see accurate uptime projections.

**Acceptance Criteria:**

**Given** a condition with base duration of 3 seconds and a character with 450 Expertise plus a +20% Burning Duration modifier
**When** outgoing Burning duration is calculated
**Then** the result is `3 * (1 + 450/1500 + 0.20) = 3 * 1.50 = 4.5 seconds`
**And** the divisor `1500` is read from the loaded universal formula data (derived from `expertise_per_condition_duration_pct: 15` → 15/100 per 15 expertise → 1/1500 per 1 expertise), not hardcoded separately

**Given** a boon with base duration of 5 seconds and a character with 600 Concentration
**When** outgoing boon duration is calculated
**Then** the result is `5 * (1 + 600/1500) = 5 * 1.40 = 7.0 seconds`

**Given** total condition duration bonus exceeds 100%
**When** the bonus is applied
**Then** it is capped at 1.0 (100% bonus = double base duration) before multiplication
**And** the cap is read from loaded data (`condition_duration_cap` from `universal.json`)

**Given** total boon duration bonus exceeds 100%
**When** the bonus is applied
**Then** it is capped at 1.0 before multiplication
**And** the cap is read from loaded data (`boon_duration_cap` from `universal.json`)

**Given** a condition has both a global duration bonus (from Expertise) and a condition-specific duration modifier (e.g., +20% Burning Duration from a rune)
**When** total duration bonus is calculated
**Then** they stack additively: `total_bonus = expertise/1500 + specific_modifier`
**And** the sum is then capped and applied multiplicatively to base duration

**Given** a `BalanceContext` is available
**When** duration formulas are evaluated
**Then** the context is accepted as a parameter (even if duration formulas are currently mode-invariant) to ensure the signature is future-proof for potential mode-split duration caps

**Given** all duration formula logic used by optimizer runtime paths
**When** the story is complete
**Then** duration calculations use the loaded formula constants
**And** any duplicate hardcoded duration divisors or caps are removed from affected code paths

**Given** GR-1 (no heuristic contamination)
**When** duration functions are implemented
**Then** all variable inputs (base duration, expertise, concentration, specific modifiers) are explicit parameters — no default uptime assumptions

**Given** GR-2 (source verification) for duration formulas
**When** boon duration cap and concentration-driven boon duration formulas are implemented
**Then** the primary source for boon duration behavior is `wiki.guildwars2.com/wiki/Boon_Duration` — this page documents the concentration → boon duration formula, the 100% boon duration cap, and boon-specific duration modifiers
**And** the `Boon` wiki page provides supplementary context (boon list, which boons stack by duration)
**And** condition duration formula sourcing uses `wiki.guildwars2.com/wiki/Condition` and `wiki.guildwars2.com/wiki/Expertise` (or the universal attribute reference)
**And** test expected values cite these sources in comments

**Requirements**: FR7 | **Recommended order**: after P3-02 and P3-03 (BalanceContext for future-proof signatures; universal formula data provides expertise/concentration constants) | **Delivers**: duration formula functions + integration into existing duration calculation paths + tests

**Scope boundary**: Duration calculation formulas only. Does not model actual uptime (what fraction of a fight a condition is active) — that is heuristic work in P3-14. Does not introduce new data files if duration constants are already covered by `universal.json`.

---

### Story 3.6: P3-06 — Canonical Slot-Budget Dataset

As a **GW2 player**,
I want the optimizer's gear search to use verified stat budgets per equipment slot,
So that stat comparisons between gear prefixes are based on real item data, not fabricated constants.

**Acceptance Criteria:**

**Given** `data/slot_budgets/level80_ascended.json` exists and is loaded during optimizer data initialization
**When** the loader validates the file
**Then** it contains entries for every equipment slot type: helm, shoulders, coat, gloves, leggings, boots, one-handed weapon, two-handed weapon, amulet, accessory, ring, back item
**And** each slot entry declares stat values for all stat-shape families currently supported by the optimizer search (ThreeStat and FourStat at minimum; CelestialLike or other families if the current search supports them)
**And** the file includes `evidence_level: "Factual"` and `sources` citing concrete API item IDs used for derivation

**Given** the dataset stores stat values
**When** the format is reviewed
**Then** values are final integer stat modifiers per slot and stat-shape, as verified on representative ascended items — not raw `attribute_adjustment` values or abstract budget floats
**And** this avoids reintroducing rounding bugs or approximation logic downstream

**Given** the canonical derivation path for slot-budget values
**When** values are sourced
**Then** primary source is concrete `API:2/items` item IDs (e.g., Zojja's Blade for 1H Berserker, The Queen's Necklace for amulet Berserker), optionally corroborated by `API:2/itemstats`
**And** wiki pages serve as secondary human-readable confirmation, not primary source
**And** the data file records the specific item IDs used for derivation in a `derivation_items` field or equivalent

**Given** a ThreeStat ascended one-handed weapon
**When** its slot budget is looked up
**Then** major stat returns `125`, minor stats return `90` each

**Given** a ThreeStat ascended two-handed weapon
**When** its slot budget is looked up
**Then** major stat returns `251`, minor stats return `179` each

**Given** a ThreeStat ascended amulet
**When** its slot budget is looked up
**Then** major stat returns `157`, minor stats return `108` each

**Given** slot budgets for armor slots (helm, shoulders, coat, gloves, leggings, boots)
**When** looked up
**Then** each has explicit stat values per stat shape
**And** armor stat budgets are the same across armor weight classes (Heavy/Medium/Light) — armor weight only affects defense, not attribute bonuses

**Given** the slot-budget data file contains a missing slot, duplicate slot, or zero-value entry
**When** the loader validates
**Then** it returns a typed validation error and does not silently skip or default

**Given** the gear search currently uses local slot constants
**When** the story is complete
**Then** the slot-budget data file and loader are in place and tested
**And** D5 is NOT yet fully resolved — runtime consumption of this dataset happens in P3-07

**Given** GR-2 (source verification)
**When** test expected values are written
**Then** each slot/shape combination cites the specific `API:2/items` item ID used for verification in test comments

**Requirements**: FR8 | **Recommended order**: after P3-01 (no hard dependency on P3-02 — slot budgets are mode-invariant) | **Delivers**: `data/slot_budgets/level80_ascended.json` + loader + validation + tests

**Scope boundary**: Data file and loader only. This story does NOT wire slot budgets into the gear search runtime — that integration is P3-07 (which fully resolves D5). PvP amulet stat data is out of scope — that is P3-11. Exotic-rarity slot budgets are out of scope unless the current optimizer search uses them.

---

### Story 3.7: P3-07 — Typed Loaders and Hardcoded Constant Replacement

As a **GW2 player**,
I want the optimizer to load all factual data from validated data files at startup instead of using hardcoded constants scattered through the source code,
So that fixing a wrong value or supporting a balance patch requires editing a data file, not rebuilding the DLL.

**Acceptance Criteria:**

**Given** the typed loader infrastructure is implemented
**When** inspected
**Then** it lives under `crates/optimizer/src/data/` (or equivalent module)
**And** provides one typed loader per Phase A dataset: profession profiles, universal formulas, boon/condition formulas, duration formulas, slot budgets
**And** each loader returns `Result<T, Vec<DataLoadError>>` where `DataLoadError` is a typed enum (not `anyhow` or string errors)
**And** each loader uses strict deserialization — no `#[serde(default)]` on required fields, no silent skipping of unknown keys in required structures

**Given** Phase A data files exist (`data/profession_profiles.json`, `data/formulas/universal.json`, `data/formulas/boons.json`, `data/formulas/conditions.json`, `data/slot_budgets/level80_ascended.json`)
**When** the optimizer initializes at addon startup
**Then** all Phase A data files are loaded, validated, and stored as immutable in-memory snapshots
**And** loading happens once at startup; data is not re-read mid-optimization-run
**And** to pick up changed data files, the user must restart the addon (restart-based reload)

**Given** a required Phase A data file is missing, corrupt, or fails validation at startup
**When** the addon starts
**Then** the addon itself still loads successfully (no crash, no hang)
**And** the optimizer subsystem enters a **disabled** state with an explicit user-visible error (e.g., "Optimizer data failed to load: [specific error]")
**And** the optimizer refuses to produce results until data is available
**And** no silent fallback to stale, fabricated, or hardcoded values occurs

**Given** an optional later-phase data file (e.g., balance overrides from P3-09, rotation profiles from P3-14) is missing at startup
**When** the addon starts
**Then** the optimizer enters a **degraded** state (not disabled)
**And** the user-visible indicator distinguishes degraded from disabled
**And** the optimizer can still produce results using Phase A factual data, with `DataQuality` reflecting the missing optional data

**Given** the gear search currently uses local slot constants (D5)
**When** the story is complete
**Then** the gear search consumes slot-budget data from the loaded `level80_ascended.json` snapshot
**And** all local slot constant definitions in the gear search code are removed
**And** D5 is fully resolved

**Given** production factual constants corresponding to Phase A datasets exist as hardcoded values in `crates/optimizer/`
**When** the story is complete
**Then** each such constant is replaced by a read from the corresponding loaded data file
**And** the replacement scope is limited to: profession base health/defense, universal formula constants, boon/condition formula coefficients, duration formula constants, and slot-budget values
**And** explicitly excluded from replacement: test fixture values, constants that will be addressed by later stories (P3-08 through P3-15), UI-only display values, and non-factual heuristic tuning constants

**Given** each Phase A loader
**When** fed a malformed data file (wrong types, missing required fields, duplicate keys, extra unknown required fields)
**Then** it returns a specific typed `DataLoadError` variant
**And** at least one test per loader asserts this error path

**Given** the story is complete
**When** the loader module is reviewed
**Then** it exposes a clean public API for downstream code to access loaded data (e.g., `data.profession_profiles().get(Profession::Guardian)`)
**And** the in-memory data structures are immutable after load — no mutation during optimizer runs

**Requirements**: FR17, FR18 | **Resolves**: D5 (fully) | **Depends on**: P3-01 through P3-06 (data files must exist to load) | **Delivers**: typed loader module infrastructure under `crates/optimizer/src/data/` + runtime wiring replacing all Phase A hardcoded constants + disabled/degraded startup states + per-loader error-path tests

**Scope boundary**: Loaders and constant replacement for Phase A datasets only. Patch manifest loading (P3-08), balance override loading (P3-09), and effect data loading (P3-10a) each introduce their own loaders when those stories are implemented. This story defines the loader infrastructure pattern that those stories follow.

---

### Story 3.8: P3-08 — Patch Manifest and Patch Ledger Infrastructure

As a **GW2 player**,
I want the optimizer's data layer to track which game patch its balance data targets,
So that when a new GW2 patch lands, the addon can detect staleness and tell me whether its numbers are still verified for the current game build.

**Acceptance Criteria:**

**Given** `data/manifests/2026-01-13.json` exists as the initial patch manifest
**When** the loader validates the file
**Then** it contains at minimum: `patch_id` (ISO date string, must equal filename stem), `game_build_id` (integer — the GW2 API `/v2/build` value at that patch), `release_date`, `inherits_from` (nullable — null only for the earliest snapshot), `sources` (array, at least one entry with `kind` and `url`), `supported_modes` (array of `PvE`/`PvP`/`WvW`), and `status` (e.g., `"active"`)
**And** the manifest schema matches `docs/optimizer-data-schemas.md` Schema 1, with the addition of the `game_build_id` field

**Given** the manifest schema currently in `docs/optimizer-data-schemas.md` lacks a `game_build_id` field
**When** the story is complete
**Then** `docs/optimizer-data-schemas.md` Schema 1 has been updated to include `game_build_id: integer` with documentation stating it is the GW2 API `/v2/build` value
**And** this update is committed in the same PR as the implementation (FU-2/L7)

**Given** `data/patch_ledgers/2026-01-13.yaml` exists as the initial patch ledger
**When** the loader validates the file
**Then** it contains: `patch_id`, `inherits_from`, and a `changes` array
**And** each change entry has: `source_type` (skill/trait/rune/sigil/relic), `source_id` (integer), `source_name`, `mode` (PvE/PvP/WvW), `field` (string), `old_value`, `new_value`, `evidence_level` (Factual/Derived/Heuristic/Unknown), and `source` (URL)
**And** the initial ledger may have an empty `changes` array (it represents the baseline snapshot, not a diff)

**Given** the initial 2026-01-13 snapshot is the first patch
**When** the manifest and ledger data is populated
**Then** the initial manifest's `game_build_id` is either:
  (a) the exact historical build number at the 2026-01-13 patch, from a cited source (e.g., wiki patch notes, community archive, or locally captured API response), OR
  (b) if the exact historical value is unavailable: the first locally verified build number at authoring time, with a documented baseline policy recorded in the manifest's `sources` array or an adjacent `authoring_notes` field — stating that this manifest anchors to the first locally verified supported build and the `game_build_id` may not match the original 2026-01-13 patch build exactly
**And** silent approximation or uncited guessing is not acceptable
**And** the initial ledger has `inherits_from: null` and `changes: []` (baseline — no prior diff)

**Given** the typed loader infrastructure from P3-07 exists
**When** manifest and ledger loaders are implemented
**Then** they follow the same P3-07 loader pattern: `Result<T, Vec<DataLoadError>>`, strict deserialization, typed error variants
**And** manifest loader validates: `patch_id` matches filename stem, at least one source entry, `game_build_id` is a positive integer
**And** ledger loader validates: `patch_id` matches filename stem, every change entry has a non-empty `source` URL, every `evidence_level` is a valid enum variant

**Given** multiple manifest files exist under `data/manifests/`
**When** the loader initializes
**Then** it loads all available manifests and identifies the latest by `patch_id` (date ordering via `release_date`)
**And** `game_build_id` is metadata used for live-build mismatch detection — it does NOT participate in manifest ordering
**And** the inheritance chain (`inherits_from`) is validated: no circular references, every referenced parent manifest exists

**Given** multiple manifest files exist under `data/manifests/`
**When** the loader validates the manifest set
**Then** no two manifests share the same `patch_id`
**And** no two manifests are both marked `status: "active"` within the same inheritance lineage
**And** every manifest's filename stem matches its embedded `patch_id` (unique and consistent)
**And** violations return typed `DataLoadError` variants, not silent skipping

**Given** the addon starts and fetches the live `/v2/build` integer (already done by `check_api_health`)
**When** the live `game_build_id` differs from the latest manifest's `game_build_id`
**Then** the system emits an **informational indicator** (e.g., "Newer game build detected — balance data not yet verified for this build")
**And** this mismatch alone does NOT automatically downgrade any output to `DataQuality::Provisional`

**Given** the `DataQuality` tiering from P3-09 is not yet implemented
**When** P3-08 is complete
**Then** the staleness detection is plumbed but the `DataQuality` response (Provisional/Blocked) is deferred to P3-09
**And** P3-08 delivers the detection mechanism and informational indicator only — not the quality-downgrade behavior

**Given** a manifest or ledger file is missing, corrupt, or fails validation
**When** the loader runs
**Then** it returns a typed `DataLoadError` (not panic or string error)
**And** missing manifest/ledger files result in the optimizer entering **degraded** state (not disabled — Phase A factual data is still usable)
**And** the user-visible indicator states which manifest/ledger failed and why

**Given** a ledger entry references a change with `evidence_level: "Unknown"`
**When** the ledger is loaded
**Then** the Unknown entry is preserved as-is in the in-memory representation
**And** it is available for P3-09 to consume when implementing Unknown-value propagation

**Given** GR-2 (source verification)
**When** the initial 2026-01-13 manifest is authored
**Then** the `game_build_id` value cites its source in the manifest's `sources` array
**And** the manifest `sources` array includes at least one concrete URL

**Requirements**: FR21, FR22 | **Can run in parallel with**: P3-07 (no hard dependency — both depend on Phase A data files existing but not on each other) | **Delivers**: `data/manifests/2026-01-13.json` + `data/patch_ledgers/2026-01-13.yaml` + manifest loader + ledger loader + manifest-set validation (uniqueness, lineage, consistency) + inheritance chain validation + staleness detection mechanism + informational indicator on build mismatch + `docs/optimizer-data-schemas.md` Schema 1 update

**Scope boundary**: Delivers manifest/ledger data files, loaders, manifest-set validation, and the staleness detection mechanism (compare live `/v2/build` against manifest `game_build_id`). Does NOT deliver the `DataQuality` enum, `DataQualityReason`, Unknown-value propagation, or quality-downgrade behavior — those are P3-09. Does NOT deliver balance override data files — those are P3-09. The initial 2026-01-13 ledger has empty changes (baseline snapshot); actual populated ledger entries are a P3-09+ concern when real patch diffs occur.

---

### Story 3.9: P3-09 — Balance Override Datasets and Unknown-Value Handling

As a **GW2 player**,
I want the optimizer to use patch-versioned, mode-specific balance overrides instead of single hardcoded coefficient tables,
So that when ArenaNet splits a skill coefficient between PvE and PvP/WvW, the optimizer reflects the correct mode-specific value — and when a value is unknown after a patch, the optimizer tells me honestly instead of silently using stale data.

**Acceptance Criteria:**

**Given** the `DataQuality` enum is implemented
**When** inspected
**Then** it has exactly three variants: `Verified`, `Provisional`, `Blocked`
**And** `Verified` means all required factual values were resolved for the computation path used
**And** `Provisional` means at least one stale value or heuristic fallback was consumed, but output is still allowed
**And** `Blocked` means a required value is missing with no safe fallback, or a formula shape has changed

**Given** the `DataQualityReason` type is implemented
**When** an optimizer output is `Provisional` or `Blocked`
**Then** it carries a `Vec<DataQualityReason>` explaining each contributing cause
**And** each reason includes: affected field or entity name, affected mode(s), and a human-readable explanation
**And** the reasons are UI-surfaceable — the addon UI can display them without parsing internal types

**Given** the `FactualValue<T>` type (or equivalent wrapper) is implemented
**When** inspected
**Then** it represents either a resolved value of type `T` or an `Unknown` state
**And** `Unknown` is NOT representable as a bare `f64` (e.g., not `f64::NAN` or a sentinel value)
**And** arithmetic operations on `Unknown` propagate `Unknown` (e.g., `Unknown * 5.0 = Unknown`, `Unknown + 3.0 = Unknown`)
**And** this is the single numeric uncertainty model used across the entire codebase — P3-10a and all downstream stories must use this same type, not a parallel system (C4)

**Given** the distinction between `None` and `Unknown` in override lookups
**When** a balance override lookup is performed for an entity/field that has no override entry
**Then** the result is `None`, meaning "no override exists — caller should use the base Phase A factual value if present"
**And** `None` does NOT degrade `DataQuality` — it is a normal sparse-data path, not a data quality issue

**Given** the distinction between `None` and `Unknown` in override lookups
**When** a balance override lookup is performed for an entity/field that has an explicit entry with `evidence_level: "Unknown"` and `value: null`
**Then** the result is `FactualValue::Unknown`, meaning "this value is known to have changed or be unresolved"
**And** downstream consumption of this `Unknown` value degrades `DataQuality` to `Provisional` or `Blocked`
**And** the two semantics (`None` = no override, `Unknown` = explicitly unresolved) are never conflated

**Given** a formula chain encounters an `Unknown` coefficient
**When** the computation completes
**Then** the optimizer output includes `DataQuality::Provisional` (if a stale fallback was available and used) or `DataQuality::Blocked` (if no fallback exists)
**And** the output does NOT carry `DataQuality::Verified` when any `Unknown` value was consumed in the computation path

**Given** `data/balance_overrides/<patch_id>/<mode>.json` files exist
**When** the loader validates a balance override file
**Then** it contains: `patch_id` (must match directory name), `mode` (must match filename stem — `pve.json`, `pvp.json`, or `wvw.json`), and an `entities` array
**And** each entity has: `source_type` (skill/trait/rune/sigil/relic), `source_id` (integer), `name` (string), and `overrides` (object mapping field names to override entries)
**And** each override entry has: `value` (number or null) and `evidence_level` (Factual/Derived/Heuristic/Unknown)
**And** Factual/Derived entries must include a `source` URL; Unknown entries have `value: null`

**Given** the balance override loaders are implemented
**When** inspected
**Then** they follow the P3-07 typed-loader pattern: `Result<T, Vec<DataLoadError>>`, strict deserialization, typed error variants, no silent skipping of malformed entries
**And** the loader rejects files where `mode` does not match filename path, or `patch_id` does not match directory name, returning typed `DataLoadError`

**Given** the initial 2026-01-13 patch snapshot is the baseline
**When** balance override files are authored for this story
**Then** the production baseline files under `data/balance_overrides/2026-01-13/` may be empty or minimal (the baseline has no prior diff to override against)
**And** non-trivial test override entries (including at least one `Unknown` entry) exist in dedicated test fixtures or test-only datasets — NOT in production data files
**And** these test fixtures verify the full load → lookup → None-vs-Unknown → DataQuality path

**Given** a balance override lookup is performed with a `BalanceContext` specifying WvW mode
**When** no WvW override file exists for the requested patch, or the requested entity/field has no WvW entry
**Then** the lookup returns `None` (no override — base value usable), NOT a PvE value (FM-03)
**And** there is no `get_coefficient_or_pve()` or equivalent silent-PvE-fallback function
**And** the caller must explicitly handle missing mode-specific data

**Given** a balance override lookup is performed
**When** the requested entity/field exists in the override data with a resolved value
**Then** the override value takes precedence over any base value from Phase A data files
**And** the evidence level from the override entry is preserved and participates in `DataQuality` determination

**Given** the P3-08 staleness detection has identified a `game_build_id` mismatch
**When** the mismatch is informational only (no specific stale values identified in overrides or ledger)
**Then** `DataQuality` remains `Verified` unless specific override entries or ledger changes trigger Provisional/Blocked
**And** the informational indicator from P3-08 is displayed independently of DataQuality

**Given** a specific override value is identified as stale via a ledger entry (the ledger says the value changed but the new value is Unknown)
**When** that value is consumed in a computation
**Then** `DataQuality::Provisional` is produced with a `DataQualityReason` citing the stale field, entity, and mode

**Given** the story is complete
**When** the `DataQuality`, `DataQualityReason`, and `FactualValue<T>` types are reviewed
**Then** they are defined in a shared location accessible to all crates that need them (e.g., `crates/core/` or `crates/optimizer/src/data/`)
**And** the optimizer's top-level output type includes a `DataQuality` field and associated `Vec<DataQualityReason>`

**Given** GR-1 (no heuristic contamination)
**When** balance override lookups and Unknown propagation are implemented
**Then** no default coefficient values are injected by the lookup layer — missing data returns `None`, unresolved data returns `Unknown`, and callers decide how to handle it

**Requirements**: FR10, FR14 | **Depends on**: P3-02 (BalanceContext for mode-specific lookup), P3-07 (typed-loader pattern infrastructure), P3-08 (patch manifests and ledgers for override references) | **Delivers**: `DataQuality` enum + `DataQualityReason` type + `FactualValue<T>` wrapper with Unknown propagation + balance override data files + loader (P3-07 pattern) + mode-specific lookup (no PvE fallback, None ≠ Unknown) + integration with P3-08 staleness detection + optimizer output DataQuality field + test fixtures with synthetic override entries + tests

**Scope boundary**: Delivers the balance override data layer, DataQuality/DataQualityReason types, FactualValue<T> with Unknown propagation, and mode-specific lookup with explicit None-vs-Unknown semantics. Does NOT populate comprehensive override data for all skills/traits — the initial production dataset is minimal/baseline. Does NOT implement NormalizedEffect types (P3-10a) or effect extraction (P3-10b). Does NOT implement the full WvW non-fallback audit (P3-12) — P3-09 provides the infrastructure that P3-12 uses to enforce WvW correctness.

**Review Gate**: RG-1 triggers after P3-09 completes. P3-10a and P3-12 may not proceed until DataQuality/DataQualityReason design is reviewed and accepted, None-vs-Unknown semantics are validated, Unknown-value handling is tested, and balance override data is audited for completeness.

---

### Story 3.10a: P3-10a — NormalizedEffect Types, Schema, and Contracts

As a **GW2 player**,
I want the optimizer to have a structured type system for every effect that traits, skills, runes, sigils, and relics produce,
So that the optimizer can reason about stacking rules, trigger conditions, and evidence levels per effect instead of treating all modifiers as a single multiplied blob.

**Acceptance Criteria:**

**Given** the `NormalizedEffect` struct is implemented
**When** inspected
**Then** it contains at minimum: `effect_id` (unique string, e.g., `"trait:12345:0"`), `source_type` (trait/skill/rune/sigil/relic), `source_id` (integer), `source_name` (string), `category` (EffectCategory enum), `stacking_rule` (StackingRule enum), `trigger_rule` (TriggerRule enum), `uptime_model` (UptimeModel struct with `kind` field), `evidence_level` (EvidenceLevel enum), and `source` (URL string, optional for Unknown-evidence entries)
**And** any numeric field that can be unresolved uses `FactualValue<T>` from P3-09 — not raw `f64` with a separate evidence_level marker (C4: one numeric uncertainty model only)
**And** the struct includes optional timer/cap metadata fields for factually known constraints:
  - `effect_duration`: `Option<FactualValue<f64>>` — the base duration in seconds of the effect or buff/condition it applies, where factually known from skill/trait facts (null when not applicable or not yet sourced)
  - `internal_cooldown`: `Option<FactualValue<f64>>` — internal cooldown (ICD) in seconds between activations, where factually known (e.g., sigil proc cooldowns, trait proc cooldowns; null when passive or no ICD)
  - `max_stacks`: `Option<FactualValue<u32>>` — maximum concurrent stacks, where applicable (references the boon/condition stacking cap from P3-04 data for AppliesBoon/AppliesCondition; specific to the effect for other categories; null when not applicable)
**And** these timer/cap fields are factual metadata — they describe known game constraints, not heuristic estimates
**And** the fields use `Option<FactualValue<T>>` so that: `None` = field not applicable for this effect; `Some(FactualValue::Unknown)` = field is applicable but value not yet sourced; `Some(FactualValue::Resolved(v))` = factually known value
**And** this is NOT a generic global cooldown system — each field is independently optional and sourced per-effect from API facts, wiki, or tooltips

**Given** the `EffectCategory` enum is implemented
**When** inspected
**Then** it contains exactly 23 variants — the original 17 from source-of-truth section 11 plus 6 boon/condition interaction categories:

**Modifier categories (1–12):**
1. `FlatStat` — flat stat bonus
2. `StatConversion` — stat conversion (e.g., X% of Power → Condition Damage)
3. `StrikeDamagePct` — strike damage modifier
4. `ConditionDamagePct` — condition damage modifier
5. `SpecificConditionDamagePct` — specific-condition damage modifier (e.g., +X% Burning damage)
6. `CritDamagePct` — crit damage modifier
7. `BoonDurationPct` — boon duration modifier
8. `ConditionDurationPct` — condition duration modifier
9. `SpecificConditionDurationPct` — specific-condition duration modifier
10. `OutgoingHealingPct` — outgoing healing modifier
11. `IncomingStrikeMultiplier` — incoming strike damage multiplier (e.g., Protection's 0.67×)
12. `IncomingConditionMultiplier` — incoming condition damage multiplier (e.g., Resolution's 0.67×)

**Boon/condition application categories (13–14):**
13. `AppliesBoon` — boon application (payload: boon kind, stacks/pulses, base duration, stacking mode reference, cap reference, target scope/count where factually known, source-specific duration modifier where factually known, boon `effect_class` reference from P3-04 data — important because throughput boons like Quickness/Alacrity change action/recharge rate and their value to a build is qualitatively different from stat-modifier boons like Might/Fury)
14. `AppliesCondition` — condition application (payload: condition kind, stacks/pulses, base duration, stacking mode reference, cap reference, target scope/count where factually known, condition `effect_class` reference from P3-04 data — important because the condition's functional role (damage vs debuff vs suppression vs control) determines how it is valued in scoring)

**Boon/condition interaction categories (15–20):**
15. `RemovesBoon` — strips boon(s) from target (payload: target boon kind or `Any`, count of boons removed per activation, target scope)
16. `StealsBoon` — removes boon from target and applies it to self (payload: target boon kind or `Any`, count, target scope)
17. `CorruptsBoon` — converts boon(s) on target into condition(s) (payload: source boon kind or `Any`, resulting condition kind, count, target scope)
18. `RemovesCondition` — cleanses condition(s) from self or ally (payload: target condition kind or `Any`, count, target scope)
19. `ConvertsConditionToBoon` — converts condition(s) on self/ally into boon(s) (payload: source condition kind or `Any`, resulting boon kind, count, target scope)
20. `TransfersCondition` — moves condition(s) from self to target (payload: target condition kind or `Any`, count, target scope)

**Control, proc, and meta categories (21–23):**
21. `DefianceDamage` — defiance damage / crowd control contribution
22. `ProcEffect` — direct damage or utility output triggered by an event (the proc *is* the effect output; e.g., Superior Sigil of Fire deals X damage on critical hit, Rune of the Scholar deals X damage on hit above 90% HP)
23. `TriggeredEffect` — conditional modifier/buff that gates another effect category (the trigger *activates* a modifier on the build; e.g., "gain +10% strike damage when above 90% health" is a TriggeredEffect wrapping a StrikeDamagePct, "gain Fury for 5s on weapon swap" is a TriggeredEffect wrapping an AppliesBoon)

**And** there are no catch-all "Other" or "Misc" variants — every effect must map to one of the 23 categories
**And** the 6 new boon/condition interaction categories (15–20) represent first-class operations because boon denial/steal/corruption and condition cleanse/convert/transfer are core GW2 build mechanics — they are not secondary modifiers or edge cases
**And** each interaction category carries a `StatusOperation`-style structured payload with required fields: `operation_type` (one of the 8 operation types: applies_boon, removes_boon, steals_boon, corrupts_boon, applies_condition, removes_condition, converts_condition_to_boon, transfers_condition), `target_side` (self/ally/enemy), `status_kind` (specific boon/condition kind), `amount_mode` (stacks/duration_ms/charges/count), `amount_value` (f64), `base_duration_ms` (nullable), `target_scope` (self/single_target/nearby_allies/party/squad/area), `target_count` (nullable), `internal_cooldown_ms` (nullable), `source_specific_duration_multiplier` (nullable)
**And** this typed payload enables the scorer to value these operations quantitatively — it gives first-class support for boon windows, boon strip/corrupt/steal, cleanse/convert/transfer, target scope, and timers/ICDs

**Given** the boundary between `ProcEffect` and `TriggeredEffect`
**When** classifying an effect
**Then** the disjoint rule is: if the triggered event directly produces a damage number, healing amount, or utility output (the proc *is* the final effect), it is `ProcEffect`; if the triggered event activates a modifier, buff, or conditional state that modifies other effects or stats (the trigger *gates* another effect), it is `TriggeredEffect`
**And** a `TriggeredEffect` references which inner effect category it gates via a first-class `inner_category: EffectCategory` field (e.g., `inner_category: StrikeDamagePct`) — this is a structural field on the type, not just descriptive text
**And** P3-10b must use this boundary consistently — classification tests should include at least one borderline example from each side
**And** boon/condition interaction categories (15–20) are distinct from ProcEffect/TriggeredEffect: a boon steal is a `StealsBoon`, not a `ProcEffect` that happens to steal a boon; a corruption trait is a `CorruptsBoon`, not a `TriggeredEffect` wrapping a condition application

**Given** the `StackingRule` enum is implemented
**When** inspected
**Then** it includes at minimum: `Multiplicative` (multiplied together), `Additive` (summed before application), `Highest` (only best value applies), `NonStacking` (does not stack, single instance only)
**And** the enum is explicitly chosen per effect — no global assumption that all modifiers of the same category multiply together

**Given** the `TriggerRule` enum is implemented
**When** inspected
**Then** it includes at minimum: `Passive` (always active when equipped/traited), `OnCrit` (triggers on critical hit), `OnHit` (triggers on any hit), `OnSkillUse` (triggers on skill activation), `OnHealthThreshold` (triggers at HP threshold), `Conditional` (custom condition, described in metadata)
**And** the trigger rule determines when an effect is active — the uptime model determines how often

**Given** the `UptimeModel` struct is implemented
**When** inspected
**Then** it has a `kind` field with at minimum: `AlwaysOn` (100% uptime, passive effects), `Estimated` (heuristic uptime percentage), `Derived` (uptime computed from rotation/profile logic, not stored here), `Unknown` (uptime not yet determined)
**And** when `kind` is `AlwaysOn`, no uptime value is needed (implied 100%)
**And** when `kind` is `Estimated`, the struct includes an uptime value using `FactualValue<f64>`, and the effect's `evidence_level` must be `Heuristic` — `Factual` and `Derived` are incompatible with estimated uptime
**And** when `kind` is `Derived`, no uptime value is stored — the value is computed at runtime from rotation profile logic (P3-14)
**And** when `kind` is `Unknown`, no uptime value is stored — it represents an effect whose activation frequency has not been determined
**And** P3-10a defines the structural shape only; no `Estimated` uptime values are populated in this story — heuristic population is P3-14

**Given** the JSON schema under `data/normalized_effects/<patch_id>/<mode>.json`
**When** the loader validates a file
**Then** it contains: `patch_id` (must match directory name), `mode` (must match filename stem), and an `effects` array
**And** each effect entry maps to the `NormalizedEffect` struct including the optional timer/cap metadata fields
**And** the loader follows the P3-07 typed-loader pattern: `Result<T, Vec<DataLoadError>>`, strict deserialization, typed error variants, no silent skipping
**And** the loader validates timer/cap metadata consistency: e.g., `internal_cooldown` on a `Passive` trigger rule should be `None` (passive effects have no ICD); `max_stacks` on `AppliesBoon`/`AppliesCondition` should reference a valid boon/condition stacking cap from P3-04 data

**Given** the loader validates effect entries
**When** an effect has `uptime_model.kind: "Estimated"` and `evidence_level` is anything other than `"Heuristic"`
**Then** the loader returns a typed `DataLoadError` — only Heuristic evidence is compatible with estimated uptime

**Given** the loader validates effect entries
**When** an effect has a duplicate `effect_id` within the same file
**Then** the loader returns a typed `DataLoadError` — duplicate effect IDs are not allowed within a single mode file

**Given** the `NormalizedEffect` type uses `FactualValue<T>` for numeric fields
**When** a numeric field is `Unknown`
**Then** it integrates with the P3-09 Unknown propagation system — computations consuming the Unknown value produce appropriate `DataQuality` degradation
**And** there is no parallel `evidence_level: Unknown` + raw numeric value system — the `FactualValue<T>` wrapper is the single source of truth for numeric uncertainty (C4)

**Given** the data files for the initial 2026-01-13 snapshot
**When** P3-10a is complete
**Then** the schema files under `data/normalized_effects/2026-01-13/` may exist as empty or minimal stubs (e.g., `{"patch_id": "2026-01-13", "mode": "PvE", "effects": []}`)
**And** actual population of effect data is P3-10b's scope — P3-10a delivers types, schema, loader, and validation only

**Given** the `docs/optimizer-data-schemas.md` Schema 9 lists 16 recommended categories and uses `IncomingStrikeMultiplier`/`IncomingConditionMultiplier` naming
**When** P3-10a is complete
**Then** the implementation uses the schema doc's naming convention (`IncomingStrikeMultiplier`, `IncomingConditionMultiplier`) as the canonical enum variant names
**And** the implementation includes all 23 categories (the schema doc is missing `TriggeredEffect` and the 6 boon/condition interaction categories)
**And** this discrepancy is noted for FU-2 (doc sync backlog) — the schema doc should be updated to list all 23 categories, add the ProcEffect/TriggeredEffect boundary definition, and add the 6 boon/condition interaction category definitions with structured payloads

**Given** GR-1 (no heuristic contamination)
**When** NormalizedEffect types are implemented
**Then** all type definitions are factual/structural — no heuristic uptime values, no default stack counts, no assumed trigger frequencies are embedded in the type system itself

**Requirements**: FR12 (partial — type system and schema) | **Depends on**: P3-04 (StatusDefinition metadata + boon/condition counterpart reference data), P3-07 (typed-loader pattern infrastructure), P3-08 (patch plumbing for patch_id/mode directory structure), P3-09 (FactualValue<T> and DataQuality for Unknown-value integration) | **Delivers**: `NormalizedEffect` struct (including optional `effect_duration`, `internal_cooldown`, `max_stacks` timer/cap fields) + `EffectCategory` enum (23 variants: 12 modifier + 2 application + 6 boon/condition interaction + 3 control/proc/meta, with disjoint ProcEffect/TriggeredEffect boundary and distinct interaction categories) + `StatusOperation`-style structured payloads on interaction categories (operation_type, target_side, status_kind, amount_mode, amount_value, base_duration_ms, target_scope, target_count, internal_cooldown_ms) + `StackingRule` enum + `TriggerRule` enum + `UptimeModel` struct (with Estimated→Heuristic-only constraint) + JSON schema under `data/normalized_effects/` + typed loader + timer/cap consistency validation + interaction payload validation + validation rules + minimal stub data files + tests

**Scope boundary**: Delivers the type system, schema, loader, and validation for NormalizedEffect — including the timer/cap metadata fields (`effect_duration`, `internal_cooldown`, `max_stacks`) and the 6 boon/condition interaction categories with structured payloads. Does NOT populate effect data, timer/cap values, or interaction metadata — that is P3-10b. Does NOT assign heuristic uptime values — that is P3-14. Stub data files are empty or minimal; the loader and types are the deliverables. The 23 categories extend the original 17 from the source-of-truth with 6 interaction operations that are factual game mechanics (wiki-sourced: boon removal/steal/corruption, condition cleanse/convert/transfer). Category naming follows the schema doc vocabulary where applicable. The timer/cap fields are optional per-effect factual metadata, not a generic global cooldown system. FU-2 must sync the source-of-truth doc to reflect 23 categories.

**Review Gate**: RG-2 triggers after P3-10a completes. P3-10b may not proceed until the NormalizedEffect type system and schema are reviewed, all 23 effect categories are validated (17 from source-of-truth + 6 interaction categories from wiki boon/condition mechanics), the ProcEffect/TriggeredEffect boundary is confirmed, interaction category payloads are reviewed, and stacking/trigger rules are accepted.

---

### Story 3.10b: P3-10b — Effect Extraction and Population (Factual/Derived Only)

As a **GW2 player**,
I want the optimizer to have populated effect data for every trait, skill, rune, sigil, and relic that produces a numeric effect,
So that the optimizer's scoring and synergy evaluation is based on classified, structured effect data instead of ad-hoc extraction logic scattered across the codebase.

**Acceptance Criteria:**

**Given** the P3-10a NormalizedEffect type system exists
**When** effect data is populated for the 2026-01-13 patch snapshot
**Then** `data/normalized_effects/2026-01-13/pve.json`, `pvp.json`, and `wvw.json` files exist with populated effect entries
**And** each effect entry maps to exactly one of the 23 `EffectCategory` variants using the category classification rules from P3-10a (including disjoint ProcEffect/TriggeredEffect boundary and distinct boon/condition interaction categories)
**And** effects that differ between game modes have mode-specific entries (not a single entry with `all_modes`)

**Given** the population scope for P3-10b
**When** the story is complete
**Then** only factual and derived effect data is populated: categories, stacking rules, trigger rules, evidence levels, and numeric values where factually known
**And** heuristic uptime estimates are NOT populated — uptime model slots are set to `AlwaysOn` (for passive effects), `Derived` (for effects whose uptime depends on rotation), or `Unknown` (for effects whose activation frequency is not yet determined) (C3)
**And** no `Estimated` uptime values appear in the populated data — that is Phase C / P3-14 scope

**Given** the timer/cap metadata fields from P3-10a
**When** effect entries are populated
**Then** each effect entry includes factually known timer/cap values where applicable:
  - `effect_duration`: populated from skill/trait API facts (`duration` field on buff/condition application facts), wiki tooltips, or in-game verification — for traits, skills, runes, sigils, and relics that apply time-limited effects
  - `internal_cooldown`: populated from API facts, wiki, or in-game verification — for proc-based effects (sigil procs, trait procs, relic procs) that have documented ICDs
  - `max_stacks`: populated from P3-04 boon/condition stacking caps for `AppliesBoon`/`AppliesCondition` effects; from API/wiki for effect-specific stack limits (e.g., trait-specific stack caps)
**And** where a timer/cap field is applicable but the value is not yet sourced, it is set to `Some(FactualValue::Unknown)` — not `None` (which means "not applicable")
**And** where a timer/cap field is not applicable for the effect category (e.g., ICD on a passive flat stat buff), it is `None`

**Given** each populated effect entry
**When** inspected
**Then** it includes the category-specific payload fields required by engine scoring — not just a category tag:
  - `FlatStat`: affected stat (e.g., `Power`, `Precision`) + magnitude
  - `StatConversion`: source stat + target stat + conversion ratio
  - `StrikeDamagePct` / `ConditionDamagePct` / `SpecificConditionDamagePct` / `CritDamagePct`: modifier value (ratio) + target condition name (for specific-condition variants)
  - `BoonDurationPct` / `ConditionDurationPct` / `SpecificConditionDurationPct`: modifier value (ratio) + target boon/condition name (for specific variants)
  - `OutgoingHealingPct`: modifier value (ratio)
  - `IncomingStrikeMultiplier` / `IncomingConditionMultiplier`: multiplier value
  - `AppliesBoon`: boon kind + `amount_mode` (stacks/duration_ms/charges per P3-04 stacking mode) + `amount_value` + `base_duration_ms` (nullable) + `target_scope` (self/single_target/nearby_allies/party/squad/area) + `target_count` (nullable) + stacking mode reference + cap reference + `effect_class` reference from P3-04
  - `AppliesCondition`: condition kind + `amount_mode` (stacks/duration_ms per P3-04 stacking mode) + `amount_value` + `base_duration_ms` (nullable) + `target_scope` + `target_count` (nullable) + stacking mode reference + cap reference + `effect_class` reference from P3-04
  - `DefianceDamage`: damage value per activation
  - `ProcEffect`: output kind (damage/heal/utility) + magnitude + trigger (from TriggerRule)
  - `TriggeredEffect`: full inner effect payload (not just `inner_category` — the complete inner NormalizedEffect with its own category-specific fields) + trigger condition
  - `RemovesBoon`: target boon kind (specific or `Any`) + count removed per activation + target scope
  - `StealsBoon`: target boon kind (specific or `Any`) + count stolen + target scope
  - `CorruptsBoon`: source boon kind (specific or `Any`) + resulting condition kind + count + target scope
  - `RemovesCondition`: target condition kind (specific or `Any`) + count cleansed + target scope (self/ally/area)
  - `ConvertsConditionToBoon`: source condition kind (specific or `Any`) + resulting boon kind + count + target scope
  - `TransfersCondition`: target condition kind (specific or `Any`) + count transferred + target scope
**And** an effect entry that is missing its required payload fields fails loader validation with a typed `DataLoadError`
**Implementation note**: TriggeredEffect's inner payload may use an embedded payload/reference model with explicit non-recursive depth constraints rather than literal recursive NormalizedEffect nesting — the requirement is that the inner effect carries enough data for scoring, not that it is structurally recursive.

**Given** the existing extraction functions in `crates/optimizer/src/synergy.rs`
**When** the story is implemented
**Then** the existing `extract_trait_effects`, `extract_rune_effects`, `extract_sigil_effects`, `extract_relic_effects`, and `extract_skill_effects` functions are either:
  (a) replaced by the new data-driven effect lookup from populated data files, OR
  (b) retained temporarily as the extraction source that generates/validates the data files, with a clear migration path documented
**And** the old 8-variant `NormalizedEffect` enum in `synergy.rs` is replaced by or mapped to the new 23-category P3-10a type system
**And** the synergy pipeline (`synergy_pipeline.rs`) consumes the new type — no parallel old/new effect representations at runtime

**Given** the internal sequencing of this story
**When** work is organized
**Then** the story is executed in two phases:
  Phase 1: populate data files + generate coverage report + validate data completeness
  Phase 2: migrate runtime engine to consume populated data files, replacing `extract_*` → `score_normalized_effect` flow with data-lookup → score flow
**And** RG-3 reviews both dataset completeness (Phase 1) AND runtime cutover correctness (Phase 2)
**And** if implementation capacity requires, the story may be split at this boundary — Phase 1 is independently valuable as a data deliverable

**Given** the coverage report requirement (FM-08)
**When** the story is complete
**Then** a coverage report artifact is committed at `docs/reports/p3-10b-effect-coverage.md` (or equivalent committed path)
**And** the report is generated by a deterministic script or command that can be re-run (committed under `scripts/` or as a `cargo test` that outputs the report)
**And** the report contains stable columns: source type, entity ID, entity name, mapped category, evidence level, mapped/unmapped status, unmapped reason (if applicable), timer/cap metadata status (complete/partial/missing)
**And** entity counts are cross-referenced against GW2 API endpoints for each source type (e.g., total traits from `/v2/traits`, total items of type Rune from `/v2/items`)
**And** the report states the coverage percentage per source type (mapped / total with numeric effects)
**And** the report includes a timer/cap metadata completeness section: for each source type, count of effects with applicable timer/cap fields that are populated (`Resolved`) vs `Unknown` vs `None` (not applicable)
**And** missing timer/cap metadata on effects where timers/ICDs/caps are applicable is flagged as a gap — not silently ignored
**And** the report includes a boon/condition interaction section: for each source type, count of effects that perform boon removal/steal/corruption or condition cleanse/convert/transfer, classified by interaction category (15–20), with missing interaction payload metadata flagged as gaps

**Given** the coverage report identifies unmapped entities
**When** the report is reviewed
**Then** each unmapped entity has a documented reason: either (a) effect is non-numeric and out of scope, (b) effect category mapping is ambiguous and deferred, or (c) API data is insufficient to classify
**And** unmapped entities do not silently degrade optimizer accuracy — their absence is visible

**Given** a trait or skill effect that differs between PvE and PvP/WvW
**When** the effect is populated
**Then** separate entries exist in the mode-specific data files with the correct mode-specific values
**And** at least one test asserts that a known mode-split effect (e.g., a trait with different coefficients in PvP) produces different NormalizedEffect entries per mode file

**Given** the ProcEffect/TriggeredEffect boundary from P3-10a
**When** effects are classified
**Then** at least two borderline classification tests exist: one effect that is ProcEffect (direct output) and one that is TriggeredEffect (gates another modifier)
**And** the classification is consistent with the disjoint rule: proc *is* the output vs. trigger *gates* a modifier

**Given** the populated data files
**When** the loader validates them
**Then** all P3-10a validation rules apply: no duplicate `effect_id` per file, `uptime_model.kind: "Estimated"` requires `evidence_level: "Heuristic"`, `patch_id`/`mode` match directory/filename
**And** category-specific payload completeness is validated per the payload requirements above
**And** the loader follows the P3-07 typed-loader pattern

**Given** the populated effect data is loaded at runtime (Phase 2)
**When** the optimizer engine needs to evaluate effects for a build
**Then** it reads from the loaded in-memory effect data, not by re-running extraction logic against raw GW2 API item/trait/skill objects at optimization time
**And** the engine integration replaces the current `extract_*` → `score_normalized_effect` flow with a data-lookup → score flow

**Given** GR-1 (no heuristic contamination)
**When** effect data is populated
**Then** all numeric values in the data files are factual (from API facts, wiki) or derived (computed from factual inputs) — no assumed uptimes, no estimated stack counts, no guessed proc rates

**Given** GR-2 (source verification)
**When** effect values are populated
**Then** each effect entry with `evidence_level: "Factual"` includes a `source` URL (wiki page or API endpoint)
**And** entries with `evidence_level: "Derived"` document the derivation method (e.g., "computed from API trait fact index 2")

**Requirements**: FR12 (partial — population of factual/derived effect data) | **Depends on**: P3-10a (types and schema must be stable before population) | **Delivers**: populated `data/normalized_effects/2026-01-13/{pve,pvp,wvw}.json` (including factually known timer/ICD/cap metadata per effect + boon/condition interaction payloads) + coverage report artifact at committed path (including timer/cap completeness section + interaction category section) + deterministic report generation script + engine integration replacing `extract_*` flow + migration of old 8-variant NormalizedEffect enum to 23-category type + tests

**Scope boundary**: Populates factual/derived effect data and integrates into engine. Does NOT populate heuristic uptime values (P3-14). Does NOT perform the full WvW non-fallback audit (P3-12). Does NOT create rotation profiles or objective profiles (P3-14, P3-15). The coverage report is a point-in-time artifact — it does not need to auto-update on API changes.

**Review Gate**: RG-3 triggers after P3-10b completes. P3-13 may not proceed until: (1) populated effect data is reviewed for coverage and accuracy, (2) category-specific payloads are validated as complete, (3) evidence levels are assigned, (4) cross-file consistency is validated, and (5) runtime cutover from old extraction flow to data-lookup flow is confirmed working.

---

### Story 3.11: P3-11 — PvP Optimizer Path Separation

As a **GW2 PvP player**,
I want the optimizer to use the PvP amulet system instead of gear-prefix optimization when I'm optimizing for PvP,
So that my PvP build recommendations are based on the actual PvP stat system and not impossible PvE gear combinations.

**Acceptance Criteria:**

**Given** the optimizer receives a request with `BalanceContext.game_mode == PvP`
**When** optimization begins
**Then** it routes to a distinct PvP optimization path that bypasses the gear-prefix search entirely
**And** the PvP path optimizes over: PvP amulet selection + existing specialization/trait evaluation path
**And** current/locked rune, sigil, relic, and skill selections are carried through as inputs only — their optimization is deferred to stories with populated effect data (P3-10b+)
**And** the PvP path does NOT evaluate gear prefixes, slot budgets, or armor stat combinations

**Given** the distinction between PvE/WvW amulets and PvP amulets
**When** the PvP optimization path is reviewed
**Then** PvP amulets (`PvpAmulet`) are modeled as a distinct domain concept from PvE/WvW amulets (normal trinket slot)
**And** the PvP path must not reuse the normal trinket / slot-budget amulet representation — `PvpAmulet` is not a trinket slot, it is the full structured-PvP attribute package
**And** normal amulet slot budgets (from P3-06) must not participate in PvP optimization
**And** the existing `ResolvedPvpAmulet` type in `crates/core/src/types.rs` is the PvP domain type; it must never be conflated with normal amulet/trinket types
**And** naming stays explicit throughout: `amulet` for the PvE/WvW trinket slot, `pvp_amulet` for the PvP attribute package

**Given** the PvP amulet stat data
**When** the data source is reviewed
**Then** PvP amulet stats come from the existing cached API / GameDb path (`/v2/pvp/amulets` → `GameDb.pvp_amulets`)
**And** this is a separate API endpoint and data type from `/v2/items` amulets used by the PvE/WvW gear system
**And** a separate `data/pvp_amulets/` data file is NOT created unless a later patch-governance story explicitly requires versioned PvP amulet snapshots
**And** rationale: PvP amulet stats are API-native structured data already cached by the existing infrastructure — duplicating them into a data file would create dual truth without a patch-governance need

**Given** GR-2 (source verification)
**When** PvP amulet data sourcing is reviewed
**Then** the GW2 API `/v2/pvp/amulets` is the single authoritative source — API-native structured data satisfies GR-2 without secondary wiki verification

**Given** the PvP optimization entry point
**When** invoked without loaded or cached PvP amulet data (e.g., GameDb has no pvp_amulets)
**Then** it returns `DataQuality::Blocked` with a `DataQualityReason` stating that PvP amulet data is unavailable (FM-09)
**And** it does NOT return a result computed with empty/zero stats
**And** it does NOT fall back to PvE gear-based optimization

**Given** the existing `optimize_pvp` function in `crates/optimizer/src/engine.rs`
**When** P3-11 is implemented
**Then** the existing function is replaced or upgraded to consume actual PvP amulet stat data from GameDb instead of using an empty `GearCandidate` with zero stats
**And** each PvP amulet candidate contributes its stat attributes to the build's `StatBlock` for scoring
**And** the PvP path evaluates all available PvP amulets and selects the best-scoring one (or top N)

**Given** a PvP optimization run
**When** the optimizer evaluates PvP amulet candidates
**Then** each PvP amulet's stats are applied to the build's base stats (replacing gear stats, not adding to them)
**And** the scoring uses the same `BalanceContext`-parameterized formula functions from P3-02/P3-03/P3-04 with `game_mode: PvP`
**And** PvP-specific coefficient values (e.g., Fury +20% instead of +25%) are used correctly

**Given** the PvP optimization result
**When** returned to the caller
**Then** it includes: selected PvP amulet (name + stat attributes) + specializations/traits + computed stat block + combat metrics + score
**And** the result is compatible with the existing `BuildCandidate` output type — if `BuildCandidate` needs extension to carry the selected PvP amulet, the field is named `pvp_amulet` (not `amulet`) to preserve the domain distinction
**And** rune/sigil/relic/skill selections are passed through from input, not optimized — the result reflects carried selections, not PvP-optimized upgrades

**Given** the PvP optimizer path is a distinct route
**When** the codebase is reviewed
**Then** the dispatch between PvE/WvW gear search and PvP amulet search is driven by `BalanceContext.game_mode`, not by a flag on the gear search
**And** no shared code path assumes gear slots exist when running in PvP mode
**And** slot-budget data (from P3-06/P3-07) is not loaded or consulted during PvP optimization

**Given** `docs/optimizer-data-schemas.md` does not currently include a PvP amulet schema (L10)
**When** P3-11 is complete
**Then** the schema doc is updated to reference the API source (`/v2/pvp/amulets` via GameDb cache) as the canonical PvP amulet data path, noted for FU-2
**And** no new schema is added unless the data file approach is chosen in a future story

**Given** GR-1 (no heuristic contamination)
**When** PvP optimization is implemented
**Then** the PvP path uses factual PvP amulet stat values and PvP-mode formula coefficients — no assumed buff stacks, no estimated uptimes, no PvE assumptions leaked into PvP scoring

**Requirements**: FR9 | **Depends on**: P3-02 (BalanceContext for mode dispatch), P3-07 (DataQuality integration for Blocked gate) | **Delivers**: distinct PvP optimization route + PvP amulet selection from GameDb + PvP-specific stat/scoring path + minimal BuildCandidate extension (`pvp_amulet` field) + DataQuality::Blocked gate when data unavailable + schema doc sourcing note + tests

**Scope boundary**: Delivers PvP route separation and PvP amulet-based stat optimization. Rune, sigil, relic, and skill optimization within the PvP path is deferred — this story carries them as inputs only. Full PvP upgrade optimization requires populated effect data (P3-10b) and likely rotation profiles (P3-14). Does NOT deliver PvP-specific balance overrides (P3-09 infrastructure handles those). Does NOT deliver WvW-specific behavior (P3-12). The existing `optimize_pvp` function is the migration target — this story upgrades it from empty-gear placeholder to real PvP amulet-based optimization. PvE/WvW normal amulet (trinket slot) is entirely out of scope — that is covered by P3-06 slot budgets.

---

### Story 3.12: P3-12 — WvW Non-Fallback Behavior

As a **GW2 WvW player**,
I want the optimizer to use WvW-specific balance data when it exists and explicitly degrade when it doesn't,
So that I never receive PvE-tuned results silently presented as WvW recommendations.

**Acceptance Criteria:**

**Given** the optimizer runs with `BalanceContext.game_mode == WvW`
**When** a mode-sensitive coefficient is looked up (via P3-09 balance override infrastructure)
**Then** the lookup checks for WvW-specific data first
**And** if WvW data exists, it is used
**And** if no WvW override entry exists (`None` from P3-09) AND no known mode split exists for that coefficient, the base Phase A factual value is used (this is the normal sparse-data path, not a fallback)
**And** at no point does the lookup silently substitute a PvE-specific override value for a missing WvW-specific override

**Given** a WvW optimization run where a coefficient has NO known mode split
**When** no WvW override entry exists for that coefficient
**Then** the lookup returns `None` and the base Phase A value is used
**And** `DataQuality` may remain `Verified` — the base value is mode-invariant and not PvE-biased

**Given** a WvW optimization run where a coefficient IS known to differ between PvE and WvW (known mode split) but the WvW-specific value is missing or unresolved
**When** that coefficient is looked up
**Then** the lookup does NOT treat this as ordinary `None` (safe to use base value)
**And** the result surfaces as unresolved — either via a ledger-backed `Unknown` entry or via the audit artifact flagging the coefficient as PvE-biased
**And** computation consuming this value produces `DataQuality::Provisional` or `DataQuality::Blocked` — never `Verified`
**And** the `DataQualityReason` cites the specific coefficient, entity, and the fact that a known WvW split is unresolved

**Given** a WvW optimization run where a coefficient has an explicit WvW override with `evidence_level: "Unknown"` (value changed but unresolved)
**When** that coefficient is consumed in the computation
**Then** the result carries `DataQuality::Provisional` or `DataQuality::Blocked` with a `DataQualityReason` citing the unresolved WvW coefficient
**And** the result does NOT carry `DataQuality::Verified`

**Given** the audit requirement for WvW non-fallback compliance
**When** P3-12 is complete
**Then** an audit artifact is committed at `docs/reports/p3-12-wvw-non-fallback-audit.md` (or equivalent committed path)
**And** the audit is generated by a deterministic script or command that can be re-run (committed under `scripts/` or as a `cargo test` that outputs the report)
**And** the audit lists every mode-sensitive computation path that can be reached during WvW optimization
**And** each path is classified as:
  (a) uses WvW-specific data (resolved)
  (b) uses mode-invariant base data — no known mode split (safe)
  (c) known mode split exists but WvW value is unresolved (must degrade DataQuality)
  (d) PvE-biased base value with uncertain split status (flagged for P3-13 evidence classification)
**And** no path is classified as "silently uses PvE override as WvW fallback"

**Given** the P3-09 infrastructure already enforces `None`-not-PvE for missing mode-specific overrides (FM-03)
**When** P3-12 is reviewed
**Then** P3-12 does NOT re-implement the lookup semantics — it validates that the existing infrastructure produces correct WvW behavior end-to-end
**And** the story's primary deliverable is the audit, integration tests, the known-split enforcement mechanism, and any code fixes discovered during the audit

**Given** a WvW-specific integration test
**When** a coefficient that is known to differ between PvE and WvW is looked up with `game_mode: WvW`
**Then** the test asserts the WvW value is returned (not the PvE value)
**And** at least one test uses a real GW2 example (e.g., a trait or skill with a confirmed PvE/WvW split from wiki patch notes)

**Given** a WvW-specific integration test for the known-split-but-missing case
**When** a coefficient with a known WvW split has its WvW override deliberately removed
**Then** the test asserts the computation result is `DataQuality::Provisional` or `Blocked` — not `Verified`
**And** the `DataQualityReason` identifies the unresolved WvW split

**Given** a WvW-specific integration test for the no-known-split case
**When** a coefficient with no known mode split has no WvW override
**Then** the test asserts the base Phase A value is used and `DataQuality` may be `Verified`

**Given** the current codebase treats WvW identically to PvE in the gear search path
**When** P3-12 is complete
**Then** all WvW computation paths go through `BalanceContext`-parameterized functions (from P3-02)
**And** the WvW path uses the same gear-prefix search as PvE (WvW uses the same gear system) but with WvW-specific coefficients where available
**And** no code path silently assumes PvE coefficients when running in WvW mode

**Given** GR-1 (no heuristic contamination)
**When** WvW non-fallback behavior is implemented
**Then** no heuristic WvW-specific values are introduced in this story — all WvW coefficients come from balance override data or mode-invariant base data

**Requirements**: FR11 | **Depends on**: P3-02 (BalanceContext plumbing), P3-09 (balance override infrastructure with None-vs-Unknown semantics) | **Delivers**: WvW non-fallback audit artifact (committed, reproducible) + known-split enforcement mechanism + WvW-specific integration tests (resolved, known-split-missing, no-known-split cases) + code fixes for any silent PvE fallback paths discovered during audit + documentation of PvE-biased base values for P3-13

**Scope boundary**: Validates and enforces WvW non-fallback behavior using the P3-09 infrastructure. Does NOT create new WvW-specific balance data files — those are authored as part of the ongoing patch data maintenance. Does NOT create WvW rotation profiles (P3-14) or objective profiles (P3-15). The primary deliverable is confidence that the WvW path is honest: it uses WvW data when available, mode-invariant data when safe, degrades explicitly when a known split is unresolved, and flags uncertain cases for P3-13.

---

### Story 3.13: P3-13 — Factorized Dependency Tables and Evidence Classification

As a **GW2 player**,
I want every numeric rule in the optimizer to be classified by evidence level and every data table to be cross-validated against the others,
So that I can trust that the optimizer knows the difference between verified facts, derived computations, and heuristic guesses — and that no data file references an entity that doesn't exist.

**Acceptance Criteria:**

**Given** all Phase A data files (profession profiles, universal formulas, boon/condition formulas, slot budgets) and Phase B data files (patch manifests, patch ledgers, balance overrides, normalized effects) are in place
**When** P3-13 runs an evidence classification pass
**Then** every numeric rule across all data tables is classified as one of: `Factual`, `Derived`, `Heuristic`, or `Unknown`
**And** the classification is stored in each data file's `evidence_level` fields (already present from earlier stories)
**And** a summary report is committed at `docs/reports/p3-13-evidence-classification.md` listing: table name, total entries, count per evidence level, and any entries that were previously unclassified or inconsistently classified

**Given** the evidence classification report
**When** reviewed
**Then** every `Heuristic` entry is named (has a human-readable label or description), documented (states the assumption), and replaceable (can be swapped for a factual value without restructuring the data format)
**And** every `Unknown` entry documents why it is unknown and what would resolve it
**And** no entry has a missing or empty evidence level — the classification is exhaustive

**Given** the cross-file consistency validation requirement
**When** P3-13 implements consistency checks
**Then** the following checks are implemented as automated tests (not manual review):
  1. Every profession referenced in any data file exists in `profession_profiles.json`
  2. Every patch override file references a valid patch manifest (manifest exists with matching `patch_id`)
  3. Every normalized effect file references a valid patch and mode (manifest exists, mode is valid enum)
  4. Every normalized effect entry's `category` is a valid `EffectCategory` variant (covered by loader, but cross-validated here)
  5. Every balance override entity's `source_type`/`source_id` resolves to a real entity in the corresponding loaded dataset or cache (traits in traits cache, skills in skills cache, items in items cache, etc.), and the resolved entity's type matches the declared `source_type`
**And** the checks run as `cargo test` — not a separate script that must be remembered

**Given** the distinction between blocking failures and non-blocking findings
**When** cross-file consistency checks discover issues
**Then** the following are **blocking failures** that must fail tests and be fixed before the story completes:
  - Missing manifest referenced by an override or effect file
  - Invalid patch_id or mode reference (does not match any manifest/valid enum)
  - Nonexistent source entity (source_type/source_id does not resolve)
  - Source entity type mismatch (resolved entity type ≠ declared source_type)
  - Invalid evidence chain (e.g., `Factual` entry with no source citation)
  - Schema structural mismatch (required fields missing, wrong types)
**And** the following are **non-blocking findings** that are documented in the report and become follow-up items:
  - Evidence-quality issues (entry could be upgraded from Unknown/Heuristic with more research)
  - Deferred classification cleanup (Phase C tables not yet classified)
  - Audit gaps from P3-12 that require additional WvW research
  - Coverage gaps from P3-10b report that remain unresolved

**Given** the cross-file consistency checks from P3-12's WvW audit
**When** P3-13 integrates with P3-12 findings
**Then** any coefficients flagged as "PvE-biased with uncertain split status" (P3-12 audit category d) are classified with appropriate evidence levels
**And** if the split status cannot be resolved, the coefficient is classified as `Unknown` (not `Heuristic`) — missing facts are not mislabeled as estimates
**And** a coefficient may only be classified as `Heuristic` if an explicit heuristic substitute is being used in place of the missing factual value, with the substitute documented

**Given** the factorized dependency tables listed in FR15
**When** P3-13 reviews table coverage
**Then** the following tables exist and are loaded: `profession_profiles`, `slot_budgets`, `attribute_formulas` (universal.json), `condition_formulas` (conditions.json + boons.json), `balance_overrides`, `normalized_effects`
**And** the following tables are noted as deferred to Phase C: `rotation_profiles` (P3-14), `objective_profiles` (P3-15), `scoring_rules` (P3-15)
**And** `buff_profiles` are noted as either covered by boons.json or deferred, with explicit documentation of which

**Given** the evidence classification summary
**When** the report is generated
**Then** it is generated by a deterministic command (committed script or `cargo test` output)
**And** the report includes a per-table breakdown and a cross-table summary
**And** the report flags any evidence level mismatches: e.g., a `Factual` normalized effect referencing an `Unknown` balance override value

**Given** GR-2 (source verification)
**When** evidence levels are reviewed
**Then** every `Factual` entry has at least one source citation (URL or API reference)
**And** every `Derived` entry documents its derivation method
**And** entries classified as `Factual` that lack a source citation are downgraded to `Derived` or `Unknown` with a reason — this is a blocking failure

**Requirements**: FR15, FR16 | **Depends on**: all Phase A (P3-01 through P3-06), P3-07 (typed loaders), P3-08 (manifests/ledgers), P3-09 (balance overrides), P3-10a (effect types), P3-10b (populated effects), P3-12 (WvW audit artifact). Blocked by Review Gate RG-3. | **Delivers**: evidence classification pass across all existing data tables + evidence classification report (committed, reproducible) + automated cross-file consistency tests (blocking failures fail tests) + integration of P3-12 WvW audit findings + documentation of deferred tables + follow-up items for non-blocking findings

**Scope boundary**: Classifies and cross-validates existing data tables from Phase A and Phase B. Does NOT create new data tables — rotation profiles, objective profiles, and scoring rules are Phase C deliverables. Does NOT change any numeric values — only classifies, validates, and flags. Blocking referential-integrity failures must be fixed in this story. Non-blocking evidence-quality findings are documented as follow-up items.

---

### Story 3.14: P3-14 — Rotation Profiles and Heuristic Uptime Population

As a **GW2 player**,
I want the optimizer to use explicit, per-profession/spec rotation profiles instead of hardcoded preset condition weights,
So that condition application rates, buff uptimes, and target behavior assumptions are transparent, tunable, and replaceable — not buried in match arms.

**Acceptance Criteria:**

**Given** `data/rotation_profiles/pve.json`, `pvp.json`, and `wvw.json` exist
**When** the loader validates a rotation profile file
**Then** it contains: `mode` (must match filename stem) and a `profiles` array
**And** each profile has the following top-level fields:
  - `profile_id` (unique string, e.g., `"pve-guardian-firebrand-support"`)
  - `profession` (string)
  - `elite_spec` (nullable — null for core profession)
  - `objective_profile_id` (references P3-15, nullable until P3-15 delivers)
  - `boon_generation` (typed by boon kind — this build's boon output; see below)
  - `boon_uptime` (typed by boon kind — achieved boon uptimes this build benefits from; see below)
  - `condition_application` (typed by condition kind — this build's condition output; see below)
  - `incoming_suppression` (typed by condition/control kind — suppression this build suffers; see below)
  - `target_behavior` (target-specific heuristic inputs; see below)
  - `scenarios` (scenario-specific buff environment variations; see below)
  - `evidence_level` (must be `"Heuristic"`)
  - `notes` (human-readable documentation of assumptions, must state "Average-state heuristic, not event simulation.")
**And** the loader follows the P3-07 typed-loader pattern: `Result<T, Vec<DataLoadError>>`, strict deserialization, typed error variants

**Given** the design intent of rotation profiles
**When** the model is reviewed
**Then** a rotation profile is an **average-state heuristic model** — it describes time-averaged expected stack counts, application rates, and buff uptimes for a typical combat encounter
**And** it is NOT an event-by-event simulation, a rotation script, or a timeline of discrete actions
**And** all values in a rotation profile represent expected averages over a sustained combat window (e.g., 30+ seconds), not per-tick or per-skill values
**And** this design choice is documented in the profile schema's header or documentation field

**Given** the `condition_application` top-level field in each rotation profile
**When** inspected
**Then** it is a map of condition kind → typed application metrics object
**And** each application metrics object uses one of these modes, matching the condition's `stacking_mode` from P3-04 `StatusDefinition` metadata:
  - `avg_stacks_per_second` (f64) — for intensity-stacking damaging conditions (Bleeding, Burning, Torment, Confusion, Poisoned): average new stacks applied per second, steady-state rate
  - `avg_duration_ms_per_second` (f64) — for duration-stacking conditions (Fear, Taunt, Daze, Blind, Chilled, Immobile, Crippled, Slow, Weakness): average milliseconds of condition applied per second, representing control/suppression pressure
  - `expected_procs_per_second` (f64) — for consumable/on-hit conditions (Blind when treated as per-hit suppression): average proc frequency
  - `avg_stacks` (f64) — for debuff conditions maintained at a steady-state average (Vulnerability): average active stacks maintained on target, not application rate
**And** the mode type must match the condition's stacking behavior — using `avg_stacks_per_second` for a duration-stacking condition like Fear would misrepresent the application; using `avg_duration_ms_per_second` for an intensity-stacking condition like Bleeding would misrepresent the output
**And** this replaces the current `ConditionWeights` per-profession preset system
**And** conditions not present in the map are assumed to have zero application

**Given** the `boon_generation` top-level field in each rotation profile
**When** inspected
**Then** it is a map of boon kind → generation metrics object
**And** each generation metrics object may contain:
  - `avg_stacks_per_second` (f64, for intensity-stacking boons like Might, Stability — average new stacks produced per second)
  - `avg_duration_ms_per_second` (f64, for duration-stacking boons like Quickness, Alacrity — average milliseconds of boon produced per second)
  - `expected_procs_per_second` (f64, for consumable boons like Aegis — average proc frequency)
**And** the metric type matches the boon's `stacking_mode` from P3-04 `StatusDefinition` metadata
**And** boons not present in the map are assumed to have zero generation
**And** boon generation is this build's boon output contribution — it feeds the `boon_support` scoring axis in P3-15

**Given** the `boon_uptime` top-level field in each rotation profile
**When** inspected
**Then** it is a map of boon kind → f64 uptime fraction (0.0–1.0)
**And** uptime represents the net achieved boon uptime this build benefits from, after accounting for all sources and removals
**And** uptime feeds the combat metric calculations (e.g., Fury uptime → effective crit chance, Might stacks → effective power)
**And** boon generation and boon uptime are NOT the same thing — a Quickness Firebrand generates Quickness at a high rate but may receive Quickness from another source; a solo DPS player generates zero boon support but may assume self-buffed Might from food
**And** this separation is required so that Support/Boon builds are scored on their boon output (generation), not just on the uptimes they benefit from

**Given** the `incoming_suppression` top-level field in each rotation profile
**When** inspected
**Then** it is a map of condition/control kind → f64 uptime fraction (0.0–1.0)
**And** it models the average suppression this build suffers from enemy pressure in the scenario
**And** at minimum the following suppression types are supported: Weakness, Chilled, Slow, Blind, Fear, Taunt, Immobile, Crippled, and a generic `control_pressure` (f64, average fraction of time under any hard CC)
**And** when present, the runtime uses these to discount effective output rates (e.g., if Slow uptime is 0.3, effective action speed is reduced by the documented Slow penalty × 0.3)
**And** when absent or zero for a given condition, no suppression is applied for that condition
**And** suppression modeling is explicitly heuristic — it estimates the average impact of output-channel conditions on this build's performance

**Given** the `target_behavior` field in each rotation profile
**When** inspected
**Then** it contains at minimum:
  - `movement_fraction` (f64, 0.0–1.0 — fraction of time target is moving; affects Torment/Confusion state-conditional damage)
  - `skill_use_frequency_per_second` (f64 — target's skill activation rate; affects Confusion on-skill-use damage)
**And** these are the heuristic inputs that feed into mode-aware condition formulas from P3-04

**Given** the `scenarios` field in each rotation profile
**When** inspected
**Then** it is an ordered array of `ScenarioProfile` entries, each containing:
  - `scenario_id` (string, e.g., `"solo"`, `"party"`, `"full_squad"`)
  - `label` (human-readable display name)
  - `might_stacks` (f64, 0.0–25.0 — average stacks, not necessarily integer)
  - `vulnerability_stacks` (f64, 0.0–25.0 — average stacks on target)
  - additional scenario-specific boon overrides (optional map of boon kind → uptime or stacks, for scenario variations beyond Might/Vulnerability)
**And** scenarios describe the buff environment this build operates in — they override the top-level `boon_uptime` values where the scenario changes assumptions (e.g., solo has 8 Might stacks, full squad has 25)
**And** the top-level `boon_generation`, `condition_application`, and `incoming_suppression` are profile-wide — they represent this build's average output/input regardless of scenario; scenarios only vary the external buff environment
**And** all numeric fields use f64 (average values, not discrete booleans) — this is a deliberate upgrade from the current boolean/integer `BuffProfile` model
**And** at minimum three scenarios exist per profile: solo, party, full squad

**Given** the design decision on multi-scenario outputs
**When** P3-14 is complete
**Then** solo / party / full squad remain first-class scenario outputs — the optimizer computes combat metrics for each scenario in the profile's `scenarios` array
**And** the engine's existing multi-scenario computation pattern (solo/party/full squad in `combat.rs`, `engine.rs`, `synergy_pipeline.rs`) is preserved but driven by scenario data from the rotation profile instead of `default_buff_profiles()`
**And** the number and names of scenarios are data-driven (from the profile), not hardcoded — a profile could define two scenarios or four if needed

**Given** the existing `condition_weights_for_profession()` and `default_buff_profiles()` functions
**When** P3-14 is complete
**Then** both are fully replaced by rotation profile data lookups
**And** `ConditionWeights`, `condition_weights_for_profession()`, and `default_buff_profiles()` are deleted from the codebase
**And** all call sites in `combat.rs`, `engine.rs`, and `synergy_pipeline.rs` consume the new `RotationProfile` and `ScenarioProfile` types

**Given** rotation profile evidence levels
**When** any profile is reviewed
**Then** `evidence_level` is `"Heuristic"` for all profiles — no rotation profile may claim `"Factual"` unless backed by simulation data or log analysis (which is out of scope for this story)
**And** every profile's `notes` field documents the key assumptions (e.g., "assumes full party buffs, golem-like target, no downtime")

**Given** the runtime rule: heuristic profiles must respect factual metadata
**When** rotation profile values are consumed at runtime
**Then** heuristic profiles may shape expected state, but must respect factual metadata from earlier stories:
  - **Stacking mode**: intensity-stacking boons/conditions accumulate stacks up to their cap; duration-stacking boons/conditions extend duration (not stacks) — from P3-04 `StatusDefinition` metadata
  - **Stack caps**: from P3-04 metadata (e.g., Might max 25, Vulnerability max 25) — `condition_application` rates that would exceed the cap are clamped; `boon_uptime` values that exceed 1.0 are clamped
  - **Duration caps**: conditions/boons have per-source duration limits — from P3-04 `max_duration_ms` metadata
  - **ICDs/timers**: from P3-10b effect data — if a proc-based trait or sigil has a known ICD, the effective application rate is capped at `1/ICD` regardless of the rotation profile's nominal rate
**And** these constraints are factual game rules loaded from data — the rotation profile provides heuristic rates, but the factual cap/timer/stacking layer prevents impossible states (e.g., >25 Might stacks, proc faster than ICD, >100% uptime)
**And** the heuristic assumptions (application rates, uptimes, suppression) remain Phase C only — the factual constraints (caps, stacking modes, timers) are Phase A/B data consumed here

**Given** a specific rotation profile lookup for a profession/spec/mode
**When** a matching profile exists
**Then** the profile's condition application rates, scenarios, and target behavior assumptions are used for scoring
**And** no hardcoded fallback values are mixed in — the profile is the complete heuristic input

**Given** a specific rotation profile lookup for a profession/spec/mode combination with no matching profile
**When** the specific lookup returns `None`
**Then** a separate explicit fallback resolver selects a documented generic profile (e.g., `"generic-pve-dps"`, `"generic-pvp"`) if one exists in the same mode file
**And** this fallback resolution is a distinct code path from the specific lookup — not a silent default inside the lookup function
**And** if a generic fallback profile is used, `DataQuality` is `Provisional` with a `DataQualityReason` stating the non-specific profile was substituted
**And** if no generic fallback exists either, the optimizer enters degraded mode with `DataQuality::Provisional` and a `DataQualityReason` stating the missing profile

**Given** the heuristic uptime population deferred from P3-10b
**When** P3-14 is complete
**Then** NormalizedEffect entries with `uptime_model.kind: "Derived"` or `"Unknown"` are updated with heuristic uptime estimates where applicable
**And** updated entries have their `uptime_model.kind` changed to `"Estimated"` with a concrete uptime value
**And** the `evidence_level` of any effect whose uptime is now `"Estimated"` is set to `"Heuristic"` (per P3-10a validation rule)
**And** effects that remain `"Unknown"` (no reasonable heuristic available) are left unchanged — they continue to degrade DataQuality when consumed

**Given** the initial rotation profile dataset
**When** profiles are authored
**Then** at minimum one profile exists per core profession per mode (9 × 3 = 27 minimum)
**And** additional per-elite-spec profiles are included where the elite spec significantly changes the rotation (e.g., Scourge vs base Necromancer condition application differs substantially)
**And** at least one generic fallback profile exists per mode (e.g., `"generic-pve-dps"`)
**And** the initial values are explicitly documented as provisional estimates — the `notes` field states the source of each assumption

**Given** the end-to-end integration smoke test requirement
**When** P3-14 is complete
**Then** at least one integration test exercises the full factual + heuristic pipeline: load Phase A factual data → load rotation profile → compute combat metrics across all scenarios → produce scored output with `DataQuality` reflecting the heuristic rotation input
**And** the test asserts that `DataQuality` is `Provisional` (heuristic input used), not `Verified`
**And** the test asserts that changing the rotation profile's condition application rates changes the scored output
**And** the test asserts that different scenarios produce different combat metric values (solo < party < full squad for offensive metrics)

**Given** GR-1 is inverted for Phase C
**When** rotation profiles are implemented
**Then** all values in rotation profiles are explicitly `Heuristic` — the Phase C boundary means no factual engine changes occur in this story
**And** the rotation profile data is cleanly separated from the factual combat math (Phase A) and the factual effect data (Phase B) — heuristic assumptions flow in through the profile, not through modifications to formulas or effect entries

**Requirements**: FR13 | **Addresses**: D7 (replaces preset condition weighting) | **Depends on**: P3-04 (StatusDefinition metadata — stacking mode/caps/consumption for clamping), P3-07 (typed loaders), P3-10b (populated factual/derived effect data including timer/ICD/cap metadata + boon/condition interaction data to attach uptime estimates to) | **Delivers**: `data/rotation_profiles/{pve,pvp,wvw}.json` + `RotationProfile` type (with typed top-level sections: `boon_generation`, `boon_uptime`, `condition_application`, `incoming_suppression`, `target_behavior`) + `ScenarioProfile` type (buff environment variations) + typed loader + runtime replacement of `condition_weights_for_profession()` and `default_buff_profiles()` (both deleted) + data-driven multi-scenario computation with factual cap/timer/stacking enforcement + explicit fallback resolver + heuristic uptime population for NormalizedEffect entries + end-to-end integration test + tests

**Scope boundary**: Delivers heuristic rotation profiles and uptime population. Does NOT change factual combat formulas (Phase A) or factual effect data (Phase B) — only adds heuristic uptime/application rates on top. Does NOT deliver objective profiles or scorer isolation (P3-15). Does NOT deliver simulation-backed rotation data — all profiles are heuristic estimates. The initial dataset is explicitly provisional and replaceable. The multi-scenario model (solo/party/squad) is preserved but made data-driven rather than hardcoded.

---

### Story 3.15: P3-15 — Objective Profiles and Typed State-Aware Scorer Isolation

As a **GW2 player**,
I want the optimizer's scoring logic to be separated from the factual combat engine and driven by explicit, mode-specific objective profiles with typed boon and condition priorities,
So that build rankings reflect intentional heuristic choices about damage, sustain, boon support, and control pressure — not hardcoded assumptions pretending to be math.

**Acceptance Criteria:**

**Given** `data/objective_profiles/pve.json`, `pvp.json`, and `wvw.json` exist
**When** the loader validates an objective profile file
**Then** it contains: `mode` (must match filename stem) and a `profiles` array
**And** each profile has:
  - `objective_profile_id` (unique string, e.g., `"PvE_Power_DPS"`, `"WvW_Roaming_Disruptor"`, `"PvP_Boon_Duelist"`)
  - `axis_weights` (object with exactly 6 axes: `power`, `condition`, `boon_support`, `healing`, `sustain`, `control` — each 0.0–1.0)
  - `weight_budget` (f64 — total weight constraint, stored in data)
  - `normalization_constants` (object with per-axis normalization values: `strike_dps_norm`, `condi_dps_norm`, `boon_support_norm`, `healing_power_norm`, `effective_health_norm`, `control_norm`)
  - `boon_priorities` (map of boon type → relative priority/value)
  - `condition_priorities` (map of condition type → relative priority/value)
  - `interaction_priorities` (optional map of interaction operation type → relative priority/value — values boon/condition interaction operations for the `control` and `boon_support` axes; see below)
  - `is_mode_default` (boolean — exactly one profile per mode must be `true`)
  - `notes` (human-readable documentation of scoring intent and assumptions)
  - `evidence_level` (must be `"Heuristic"`)
**And** the loader follows the P3-07 typed-loader pattern
**And** the loader validates that exactly one profile per mode has `is_mode_default: true`

**Given** the canonical relationship between `OptimizationWeights` and `ObjectiveScorer`
**When** P3-15 is complete
**Then** `OptimizationWeights` remains the user-editable runtime weight vector
**And** `OptimizationWeights` is revised to exactly 6 axes: `power`, `condition`, `boon_support`, `healing`, `sustain`, `control`
**And** the old `disable` axis is removed and replaced by `control`
**And** `ObjectiveScorer` is constructed from:
  - the active `OptimizationWeights`
  - the selected objective profile's normalization constants
  - the selected objective profile's `boon_priorities`
  - the selected objective profile's `condition_priorities`
  - the selected objective profile's `interaction_priorities`
  - the selected objective profile's `weight_budget`
**And** `ObjectiveScorer` is the single entry point for scoring
**And** the old `score_with_weights` free function is removed or made private behind `ObjectiveScorer`
**And** no parallel scoring system remains at runtime

**Given** the existing `WEIGHT_BUDGET` and normalization constants in `scoring.rs`
**When** P3-15 is complete
**Then** these values are loaded from objective profile data, not hardcoded
**And** the hardcoded constants are removed from `scoring.rs`
**And** different objective profiles may use different normalization constants and weight budgets

**Given** boon support is a first-class optimization target
**When** an objective profile is inspected
**Then** `boon_priorities` is required and typed by specific boon kind
**And** the scorer uses `boon_support` axis weight plus `boon_priorities` to value:
  - boon generation
  - boon sustain/uptime
  - boon package correctness for the selected objective
**And** the story does not treat all boons as interchangeable or collapse them into one generic support number

**Given** condition/control pressure is a first-class optimization target
**When** an objective profile is inspected
**Then** `condition_priorities` is required and typed by specific condition kind
**And** the scorer uses `condition` and `control` axis weights plus `condition_priorities` to value:
  - offensive condition pressure by type
  - suppression/control pressure by type
  - target-state amplification where modeled
**And** the story does not treat all conditions as interchangeable or collapse them into one generic condi number

**Given** the `interaction_priorities` field in objective profiles
**When** inspected
**Then** it is an optional map of interaction operation type → f64 value weight (0.0–1.0 scale, relative importance)
**And** the keys are the 6 interaction operation types from P3-10a: `removes_boon`, `steals_boon`, `corrupts_boon`, `removes_condition`, `converts_condition_to_boon`, `transfers_condition`
**And** when present, it defines how much each interaction operation contributes to the `control` axis (for denial operations: removes_boon, steals_boon, corrupts_boon, transfers_condition) and to the `boon_support` axis (for sustain operations: removes_condition, converts_condition_to_boon)
**And** a WvW Disruptor profile may weight `corrupts_boon` and `steals_boon` higher than `removes_boon` because corruption creates pressure (applies a condition) while removal only denies a boon
**And** a PvP Sustain profile may weight `removes_condition` and `converts_condition_to_boon` higher because cleanse/convert sustain value matters more than denial in that context
**And** when absent, interaction operations are weighted equally within their axis (the default scalar model)
**And** `interaction_priorities` is heuristic data — different objective profiles may value the same operation differently depending on the build's role

**Given** the `control` axis replaces the old `disable` axis
**When** P3-15 is reviewed
**Then** `control` is explicitly documented as a heuristic valuation of control/denial pressure, not a factual CC simulator
**And** `control` may include, where modeled by earlier stories:
  - control application
  - output suppression
  - boon denial/corruption pressure
  - mobility denial
**And** this is the story's resolution of D6 — the fake duration-proxy framing is removed

**Given** the existing `OptimizationWeights::default_for_mode()` and `OptimizationWeights::PRESETS`
**When** P3-15 is complete
**Then** mode-specific defaults come from the mode-default objective profile's `axis_weights`, not hardcoded match arms
**And** the old preset system (power DPS, condi DPS, healer, support, etc.) is replaced by named objective profiles in data files
**And** `OptimizationWeights::default_for_mode()` resolves through objective-profile data (or equivalent scorer/profile service), not embedded constants

**Given** the addon UI radar chart and budget UI currently depend on scoring constants
**When** P3-15 is complete
**Then** radar chart normalization and budget display are sourced from the active `ObjectiveScorer` or selected objective profile
**And** the radar chart renders all 6 axes: `power`, `condition`, `boon_support`, `healing`, `sustain`, `control`
**And** if the active objective profile changes, the radar chart updates its normalization and budget accordingly

**Given** the `set_constrained` budget enforcement logic
**When** the user adjusts weights via the UI
**Then** the weight budget from the active objective profile is used
**And** the proportional-scaling enforcement math is preserved as UI interaction logic
**And** if no objective profile is loaded, the UI falls back to current behavior with `DataQuality::Provisional`

**Given** a rotation profile's `objective_profile_id` field is null
**When** runtime resolves the objective profile for scoring
**Then** null resolves explicitly to the mode's default objective profile
**And** this resolution is not a hidden default inside low-level lookup code

**Given** a rotation profile's `objective_profile_id` references a non-existent profile
**When** runtime resolves the objective profile
**Then** the result is `DataQuality::Provisional` with a `DataQualityReason` stating that the requested objective profile was not found and the mode default was used instead
**And** if no mode default exists either, the result is `DataQuality::Blocked` with a reason stating no objective profile is available

**Given** typed boon and condition priorities affect ranking
**When** two objective profiles differ mainly by `boon_priorities` or `condition_priorities`
**Then** the scorer can produce different rankings for the same build set based on typed state emphasis
**And** at least one integration test asserts that changing boon priorities changes ranking
**And** at least one integration test asserts that changing condition priorities changes ranking

**Given** the end-to-end integration smoke test requirement
**When** P3-15 is complete
**Then** at least one integration test exercises the full pipeline:
  - load Phase A factual data
  - load Phase B typed effect data
  - load Phase C rotation profile
  - load objective profile
  - construct `ObjectiveScorer`
  - compute combat metrics
  - apply typed scoring
  - produce ranked output with `DataQuality` reflecting heuristic scoring inputs
**And** the test asserts that two different objective profiles produce different build rankings for the same profession/build set
**And** the test asserts that `DataQuality` is `Provisional` because heuristic objective/priority inputs are used

**Given** the initial objective profile dataset
**When** profiles are authored
**Then** at minimum the following profiles exist per mode:
  - PvE: Power DPS, Condi DPS, Boon Support, Healer, Hybrid Support
  - PvP: Burst, Sustain, Boon Pressure, Control/Disruptor
  - WvW: Zerg DPS, Zerg Support, Roamer, Disruptor
**And** each profile defines typed `boon_priorities` and typed `condition_priorities` appropriate to that role
**And** all profiles have `evidence_level: "Heuristic"` with rationale documented in `notes`
**And** initial values are migration-equivalent to the current presets where applicable, but extended to typed boon/condition valuation

**Given** GR-1 is inverted for Phase C
**When** objective profiles and scorer isolation are implemented
**Then** all axis weights, normalization constants, `boon_priorities`, `condition_priorities`, and `interaction_priorities` are explicitly `Heuristic`
**And** no factual combat formula changes occur in this story
**And** the factual combat engine produces raw `CombatPerformance` and typed effect/state outputs
**And** `ObjectiveScorer` consumes those outputs as heuristic ranking inputs through an explicit structural boundary

**Requirements**: FR20 | **Addresses**: D6 (replaces fake disable proxy with heuristic control axis in isolated scorer) | **Depends on**: P3-14 (rotation profiles), P3-10b (typed effect data), P3-13 (evidence/cross-file validation) | **Delivers**: `data/objective_profiles/{pve,pvp,wvw}.json` + revised 6-axis `OptimizationWeights` + `ObjectiveScorer` type + typed loader + runtime replacement of hardcoded weights/norms/presets + typed boon/condition/interaction priority scoring + addon radar chart integration + objective-profile resolution logic + end-to-end integration test + tests

**Scope boundary**: Delivers heuristic objective profiles and scorer isolation from the factual engine. Does NOT change factual combat formulas (Phase A). Does NOT change factual effect definitions (Phase B). Does NOT build a factual CC simulator. Instead, it externalizes scoring into a 6-axis heuristic model with typed boon, condition, and interaction priorities. Passive stats remain enabling inputs; typed boon/condition/control packages are first-class scoring targets.

---

### Story 3.16: P3-16 — Save/Load Profession Persistence and Crash Safety

As a **GW2 player who saves and loads builds**,
I want saved builds to remember my profession, use crash-safe writes, and recalculate combat metrics correctly on load,
So that I never see wrong health/armor values from a missing profession, never lose a save to a mid-write crash, and never see stale combat numbers after an optimizer update.

**Acceptance Criteria:**

**Given** the `SavedBuild` struct in `crates/core/src/types.rs`
**When** P3-16 is complete
**Then** `SavedBuild` includes a `profession: String` field
**And** the field uses `#[serde(default)]` so existing saves without the field deserialize with `profession = ""` (backward compatibility)
**And** empty profession is treated as `"Warrior"` fallback at load time (matching current implicit behavior) — this fallback is documented as a backward-compat shim, not a default assumption

**Given** a build is saved via the addon UI
**When** the save is executed
**Then** the `profession` field is populated from the active character's profession (from optimization context or character data)
**And** the profession is never left empty for new saves

**Given** a saved build is loaded
**When** combat metrics are computed for display
**Then** combat metrics (solo/party/full squad) are always recomputed from the saved build configuration using the current engine and balance data
**And** this is the only path — there is no "reuse saved combat metrics" shortcut, because `SavedBuild` does not persist combat metric fields
**And** the recomputation uses the saved profession's correct base health and base defense values (from profession profiles loaded by P3-01/P3-07)

**Given** load-time combat recomputation
**When** `DamageModifiers` are needed for combat metric calculation
**Then** modifiers are reconstructed from the saved build configuration, NOT from `DamageModifiers::default()`
**And** reconstruction resolves the saved specializations/traits, rune, sigils, and relic against current game data (GameDb) to rebuild modifiers from those resolved entities
**And** if any saved entity cannot be resolved against current game data (e.g., name changed, entity removed), the unresolvable modifier is skipped with a warning — it does not silently zero out or use defaults for the entire modifier set

**Given** the current `SavedBuild` stores build components by name (strings), not by ID
**When** load-time modifier reconstruction resolves entities
**Then** resolution uses name-based lookup against GameDb (consistent with how components are currently persisted)
**And** if name-based resolution proves insufficient for reliable reconstruction (e.g., name collisions, ambiguous rune names), the story must either:
  (a) add stable ID fields to `SavedBuild` alongside names for reliable resolution, OR
  (b) document the limitation explicitly and accept best-effort reconstruction with appropriate logging
**And** the choice is documented in the implementation — this is not left to silent degradation

**Given** load-time combat recomputation
**When** the helper computes combat metrics
**Then** it uses a mode-aware helper or equivalent API that accepts the saved `game_mode` (or `BalanceContext` if available from P3-02)
**And** the current mode-agnostic `compute_3tier_combat` helper (which ignores game_mode and uses legacy heuristic hooks) is NOT reused unchanged
**And** the mode-aware path ensures PvP and WvW saved builds use correct mode-specific coefficients (e.g., Fury +20% in PvP, not +25%)

**Given** `SavedBuild` gains version tracking fields
**When** P3-16 is complete
**Then** `SavedBuild` includes:
  - `engine_version: String` (the optimizer engine version at save time)
  - `balance_manifest_version: Option<String>` (the `patch_id` of the active balance manifest at save time, if P3-08 is available; `None` otherwise)
**And** both fields use `#[serde(default)]` for backward compatibility
**And** these fields are informational metadata — they do NOT gate a reuse-vs-recompute decision (combat metrics are always recomputed)
**And** they enable future features (e.g., notifying the user that a saved build was created under a different balance patch) without being load-blocking now

**Given** `BuildStorage` in `crates/core/src/storage.rs`
**When** P3-16 is complete
**Then** the storage API explicitly distinguishes new saves from overwrites:
  - `save_new(build)` — creates a new save file; fails with a descriptive error if a file with the same derived filename already exists
  - `save_overwrite(build)` — replaces an existing save file; fails if the target file does not exist
  - (or equivalent API: `save(build, overwrite: bool)`)
**And** both paths use crash-safe temp-write + atomic rename

**Given** the crash-safe write pattern
**When** any save operation is performed (new or overwrite)
**Then** the build is serialized to `{filename}.tmp` first
**And** then `std::fs::rename("{filename}.tmp", "{filename}.json")` performs the atomic replacement
**And** if the rename fails, the `.tmp` file is cleaned up (best-effort) and an error is returned
**And** a `.json` file is never left in a partially-written state
**And** this matches the existing pattern in `config.rs` for `AppConfig` atomic saves

**Given** `save_new` is called
**When** a file with the same derived filename already exists
**Then** the operation fails with a descriptive error
**And** no `.tmp` file is left behind

**Given** `save_overwrite` is called
**When** the target `.json` file exists
**Then** the `.tmp` → `.json` rename atomically replaces the old file
**And** the old content is not lost if the write to `.tmp` fails

**Given** a backward-compatibility test
**When** a JSON string WITHOUT the `profession`, `engine_version`, or `balance_manifest_version` fields is deserialized
**Then** it produces a valid `SavedBuild` with `profession = ""`, `engine_version = ""`, `balance_manifest_version = None`
**And** the load path handles these gracefully (profession falls back to Warrior, modifiers are best-effort from available data, metrics are recomputed)

**Given** a round-trip test
**When** a `SavedBuild` with `profession = "Necromancer"` is serialized and deserialized
**Then** `profession == "Necromancer"` is preserved
**And** `engine_version` and `balance_manifest_version` are preserved

**Given** a modifier-reconstruction test
**When** a `SavedBuild` with known specializations/traits/rune/sigils is loaded against a GameDb
**Then** the reconstructed `DamageModifiers` reflect the saved build's trait and upgrade bonuses — not `DamageModifiers::default()`
**And** the test asserts that combat metrics differ from what `DamageModifiers::default()` would produce

**Given** a crash-safety test
**When** `save_new` and `save_overwrite` are exercised
**Then** the test verifies the `.tmp` → `.json` rename flow for both paths

**Given** P2-07 scope absorption
**When** P3-16 is complete
**Then** all P2-07 acceptance criteria are satisfied by this story
**And** `docs/stories/P2-07-saved-build-profession-and-crash-safety.md` is updated to note it is superseded by P3-16

**Requirements**: FR19 | **Fixes**: D8 | **Supersedes**: P2-07 | **Depends on**: P3-01 (profession profiles for correct base health/defense on load), P3-02 (BalanceContext for mode-aware recomputation) | **Recommended order**: start after P3-01 and P3-02 | **Delivers**: `SavedBuild.profession` field + `engine_version` + `balance_manifest_version` (informational) + mode-aware combat metric recomputation on load with correct profession and reconstructed modifiers + crash-safe temp-write + atomic rename + explicit `save_new`/`save_overwrite` API + backward compatibility + P2-07 retirement + tests

**Scope boundary**: Delivers profession persistence, modifier-aware mode-aware combat metric recomputation on load, and crash-safe writes. Does NOT re-optimize builds on load — only recomputes combat metrics from the saved build configuration. Does NOT migrate existing save files — old saves work via `#[serde(default)]` backward compatibility. Does NOT add persisted combat metric fields to `SavedBuild` — metrics are always recomputed. Does NOT change the save file format beyond adding the three new fields (`profession`, `engine_version`, `balance_manifest_version`). If P3-08 is not yet complete when P3-16 is implemented, `balance_manifest_version` is set to `None` on save. Scheduling note: P3-02 dependency means P3-16 starts after both P3-01 and P3-02 complete.
