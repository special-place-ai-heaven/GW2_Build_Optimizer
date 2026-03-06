---
stepsCompleted: ["step-01-init", "step-02-discovery", "step-02b-vision", "step-02c-executive-summary", "step-03-success", "step-04-journeys", "step-05-domain", "step-06-innovation-skipped", "step-07-project-type", "step-08-scoping", "step-09-functional", "step-10-nonfunctional", "step-11-polish"]
vision:
  statement: "Replace fundamentally incorrect optimizer with factual, verifiable engine"
  differentiator: "Three technical docs define exactly what correct looks like — every calculation traceable to GW2 API or wiki"
  insight: "Factual foundation (Phase A) → data infrastructure (Phase B) → separated heuristics (Phase C)"
  nature: "Correctness refactor, not feature add"
inputDocuments:
  - docs/optimizer-source-of-truth.md
  - docs/optimizer-data-schemas.md
  - _bmad-output/planning-artifacts/epics.md
  - _bmad-output/planning-artifacts/prd.md (draft — loaded as ground truth reference)
documentCounts:
  briefs: 0
  research: 0
  brainstorming: 0
  projectDocs: 4
workflowType: 'prd'
classification:
  projectType: "Desktop Plugin/Addon"
  constraints: ["crash resilience", "graceful degradation"]
  domain: "Provenance-tracked game computation"
  domainDependency: "external authority (GW2 API + wiki)"
  context: "Brownfield foundation-layer overhaul"
  architectureModel: "Provenance-layered trust (source x layer -> DataQuality)"
  prdStructure: "Governance + cross-cutting + validation"
  authority: "Delegates to 3 technical docs (source-of-truth, data-schemas, epics)"
  writePolicy: "Write-once freeze; living artifacts extracted separately"
elicitationMethods: 12
prdOutline:
  - "1. Agent Reference + Glossary"
  - "2. Introduction & Authority"
  - "3. Architecture & Rationale"
  - "4. Verification Strategy"
  - "5. Constraints & Risk"
  - "6. Story Guidance (composable AC templates)"
---

# Product Requirements Document - GW2_Build_Optimizer

**Author:** Rob
**Date:** 2026-03-06

## Executive Summary

Epic 3 replaces the GW2 Build Optimizer's fundamentally incorrect optimization engine with a factual, verifiable implementation. The current engine produces wrong results — stat calculations, synergy scoring, and build recommendations are unreliable. Three technical specification documents (source-of-truth, data-schemas, epics) define exactly what correct behavior looks like, down to individual formulas and schema contracts.

The refactor proceeds in three phases: Phase A establishes factual engine correctness (universal formulas, condition/boon calculations, status effects), Phase B builds patch-aware data infrastructure (typed loaders, balance override ledger, normalized effects), and Phase C replaces heuristic layers with explicitly separated, provenance-tracked profiles (rotation profiles, objective scorer with 6-axis weighting). Every calculation becomes traceable to a GW2 API fact or wiki source.

This is not a feature addition. It is a correctness overhaul of the foundation that all future optimizer work depends on.

### What Makes This Special

The optimizer's correctness is defined by three machine-verifiable specification documents rather than ad-hoc implementation. A provenance-layered trust model (source provenance × layer provenance → DataQuality) ensures factual data is never contaminated by heuristic assumptions. Patch-aware data infrastructure means GW2 balance patches require data file updates, not code changes. The 8 known defects (D1–D8) in the current engine are resolved as side effects of building it correctly from specifications.

## Project Classification

- **Type**: Desktop Plugin/Addon (Nexus/ImGui) — crash resilience and graceful degradation required
- **Domain**: Provenance-tracked game computation with external authority dependency (GW2 API + wiki)
- **Context**: Brownfield foundation-layer overhaul — replacing internals of a feature-complete (S01–S15) shipping addon
- **Architecture**: Provenance-layered trust model delegating authority to three technical specification documents

## Success Criteria

### User Success

- Optimized builds produce correct stat totals verifiable against in-game character panel
- Balance patches handled via data file updates — no DLL rebuild required for users
- No regression in addon stability: mutex recovery, cancellation, graceful degradation all preserved
- Optimizer produces results at least as fast as current implementation (no perceptible latency increase)

### Business Success

Not applicable in the traditional sense — this is a personal/community addon, not a revenue product. Success is measured by **correctness confidence**: the ability to trust optimizer output and extend it without fear of compounding errors.

- All 8 known defects (D1–D8) resolved as side effects of correct implementation
- Foundation stable enough to build Epic 4 (UI/UX refactor) on top without rework
- Future profession/spec additions require only data files, not engine changes

### Technical Success

- Every stat calculation traceable to a GW2 API fact or accepted wiki source (GR-2, GR-3)
- Zero heuristic contamination in factual layers (GR-1) — `DataQuality` enum enforces provenance
- All 22 FRs pass acceptance criteria; 7 NFRs met
- 3 review gates passed: RG-1 (after P3-09), RG-2 (after P3-10a), RG-3 (after P3-10b)
- Typed loaders validate data files at load time — malformed data fails loud, not silent

### Measurable Outcomes

| Metric | Target |
|--------|--------|
| Defects resolved | 8/8 (D1–D8) |
| FR coverage | 22/22 passing |
| NFR compliance | 7/7 met |
| Heuristic contamination | 0 factual values with Evidence::Heuristic |
| Patch turnaround | Data file edit only, no recompile |
| MVD-1 gate | Phase A complete + typed loaders + D1–D5 resolved |

## User Journeys

### Journey 1: Player Optimizes a Build (Post-Epic 3)

A GW2 player opens the addon overlay, selects their Firebrand, and clicks Optimize. The engine loads `profession_profiles/guardian.json`, applies universal stat formulas from `data/universal_formulas.json`, calculates condition damage using per-condition formulas, evaluates boon uptime as first-class status effects, and returns a build where every stat is traceable to a GW2 API value. The player checks Power in the addon against their character panel — the numbers match. A balance patch drops next week; someone updates `data/balance_overrides.json` and the optimizer is correct again without a new DLL.

**Reveals**: FR1–FR7 (Phase A factual engine), FR8–FR12 (data infrastructure), FR17 (data-file loading), FR21–FR22 (patch manifest/ledger), NFR1 (performance), NFR3 (stability)

### Journey 2: Developer Adds a New Profession or Patch

A new elite spec releases. The developer creates a new entry in `profession_profiles/`, adds rotation data to `rotation_profiles/`, and updates the patch manifest. No Rust code changes. The typed loader validates the new files at startup — if a field is missing or malformed, the addon logs a clear error and falls back gracefully. The `DataQuality` enum ensures the new data starts as `Provisional` until verified.

**Reveals**: FR13–FR16 (Phase B/C data-driven architecture), FR18 (typed loaders), FR19 (save/load with profession context), FR20 (objective profiles), NFR2 (extensibility), NFR4 (data validation), GR-1 through GR-5

### Journey Requirements Summary

| Capability | Journey | Requirements |
|-----------|---------|-------------|
| Correct stat calculation | Player | FR1–FR7, FR17, D1–D5 |
| Patch-aware data loading | Both | FR8–FR12, FR21–FR22, NFR2 |
| Provenance tracking | Both | GR-1, GR-2, DataQuality enum |
| Typed validation | Developer | NFR4, FR13, FR18 |
| Graceful degradation | Both | NFR3, crash resilience constraint |
| Profile extensibility | Developer | FR14–FR16, data-file-only updates |
| Build persistence | Developer | FR19, crash-safe save/load |
| Objective scoring | Both | FR20, 6-axis scorer |

## Domain-Specific Requirements

### External Authority Dependency

- GW2 API v2 is the primary data source: 300 burst, 5/sec refill, max 200 IDs per bulk request
- Wiki (wiki.guildwars2.com) is the secondary source for values not exposed by API (e.g., condition formulas, some trait coefficients)
- Source hierarchy: API fact > wiki-verified > derived > heuristic (GR-2)
- Accepted wiki sources defined by GR-3

### Game Domain Constraints

- 9 professions × 5 core specs + ~4 elite specs each — combinatorial explosion in profile data
- Stat formula: `attribute_adjustment × multiplier + value` — universal, non-negotiable
- HP-class ≠ armor-class (D2) — two separate lookup tables required
- Stat alias normalization required ("ConditionDuration" ↔ "Expertise")
- Traited-fact overrides replace base facts by index — order-dependent (D3)
- Rune bonuses are unstructured strings, not typed facts (D5)
- PvP uses amulet system, completely different from PvE/WvW gear

### Provenance Constraints

- Factual values must never carry `Evidence::Heuristic` (GR-1)
- `DataQuality` enum (Verified/Provisional/Blocked) tracks trust level
- `FactualValue<T>` wraps every data point with source + evidence metadata
- Balance patches create temporal versioning — patch_id required on all override data

### Crash Resilience (Nexus Plugin)

- Addon runs inside game process — panic = game crash
- All optimization wrapped in `catch_unwind` to prevent mutex poisoning
- `CancellationToken` (Arc<AtomicBool>) on all background threads
- Graceful degradation: malformed data → log + skip, never crash

## Desktop Plugin/Addon Specific Requirements

### Project-Type Overview

Nexus plugin (cdylib DLL) loaded into the Guild Wars 2 game process by the Raidcore Nexus addon manager. Not a standalone desktop app — runs as an in-process library with no independent lifecycle.

### Technical Architecture Considerations

- **Platform**: Windows x86_64 only (GW2 is Windows-only; Nexus is Windows-only)
- **System integration**: Nexus addon API provides ImGui rendering context, keybind registration, and event hooks. No direct Win32 UI.
- **Auto-update**: Handled by Nexus addon manager, not by this addon. DLL is hot-swappable.
- **Offline**: GW2 API requires internet; optimizer must degrade gracefully when API is unreachable (cached data fallback). Data files (JSON) are fully offline.
- **Memory model**: Shared process space with game. `Mutex<Option<AddonState>>` global state. No IPC.

### Implementation Considerations

- **Build**: `cargo build --release` → single DLL, no installer
- **Deploy**: Copy DLL to `C:\GAMES\Guild Wars 2\addons\`
- **Data files**: JSON files in `{addon_dir}/data/` and `{addon_dir}/cache/`
- **No cross-platform**: Windows only, no Mac/Linux
- **No web/mobile**: Not applicable

## Project Scoping & Phased Development

### Out of Scope

Epic 3 does NOT change:
- **UI/UX**: No ImGui layout changes, no new screens, no visual redesign (deferred to Epic 4)
- **LLM provider integration**: No changes to Gemini/OpenAI/Anthropic clients, prompt templates, or model selection
- **Settings and configuration**: No changes to AppConfig, keybinds, API key management, or settings tab
- **GW2 API client**: No changes to rate limiter, cache strategy, download orchestration, or serde models
- **Addon lifecycle**: No changes to Nexus entry point, event hooks, or DLL loading
- **Build lock panel**: No changes to BuildLocks, lock UI, or lock constraint propagation
- **New features**: No new user-facing capabilities — this is a correctness refactor only

### MVP Strategy & Philosophy

**MVP Approach:** Problem-solving MVP — make the optimizer produce correct results. No new features, no UX improvements, no additional user-facing capabilities. Correctness is the product.

**Resource Requirements:** Solo developer. No external dependencies beyond GW2 API availability and wiki reference data.

### MVP Feature Set (Phase 1 — MVD-1)

**Core Journey Supported:** Player optimizes a build and gets verifiably correct stat totals.

**Must-Have Capabilities (P3-01 through P3-07 + typed loaders):**
- Universal stat formulas replacing hardcoded calculations
- Per-condition damage formulas (duration-aware)
- Boon value calculations as first-class status effects
- Status effect state machine (apply/stack/extend/strip)
- Profession profile dispatch with typed JSON loader
- Defects D1–D5 resolved

**Without these, the optimizer remains fundamentally incorrect.**

### Post-MVP Features

**Phase 2 — Data Infrastructure (P3-08 through P3-12):**
- Patch manifest + balance override ledger
- Normalized effect taxonomy (23 categories)
- StatusOperation pipeline
- Review gates RG-1, RG-2

**Phase 3 — Heuristic Layer (P3-13 through P3-16):**
- Rotation profiles (typed, per-profession/spec/objective)
- Objective scorer (6-axis weighting)
- LLM prompt context rebuild using factual data
- Defects D6–D8 resolved
- Review gate RG-3

### Risk Mitigation Strategy

**Technical Risks:**
- P3-02 (BalanceContext) could destabilize existing resolve pipeline — mitigate with feature flag, incremental integration
- P3-09, P3-10b, P3-14, P3-15 flagged as high-risk for sizing — monitor during sprint planning, split if needed
- Status effect state machine (P3-04) is novel complexity — prototype early in Phase A

**Data Risks:**
- Some GW2 values not in API, only on wiki — may require manual data entry with `Evidence::Derived` or `Evidence::Heuristic` tagging
- Balance patches can change formulas without notice — patch manifest + override ledger (Phase B) mitigates

**Resource Risks:**
- Solo developer — no parallelization of dependent stories
- If blocked, MVD-1 (Phase A) delivers standalone value even without Phase B/C
- Each phase is independently useful: A = correct, B = maintainable, C = intelligent

## Functional Requirements

### Profession & Attribute Foundation

- **FR1**: Engine can resolve correct per-profession `ProfessionProfile` (armor_weight, health_class, base_health, base_defense) with independent health/armor class dimensions
- **FR2**: Engine can derive `BalanceContext` (patch_id + game_mode) and thread it through all mode-sensitive computation paths
- **FR3**: Engine can calculate universal attribute formulas at level 80 (crit chance, crit damage, condition duration, boon duration with caps)
- **FR4**: Engine can calculate strike damage using canonical formula: `skill_fact_damage * (Power/1000) * (2597/target_armor)`

### Status Effect Calculations

- **FR5**: Engine can apply mode-aware boon values with correct stacking modes, counterpart metadata, and suppression effects (Slow/Chilled/Weakness)
- **FR6**: Engine can apply mode-aware condition damage formulas with per-condition base damage, stacking mode/cap, and boon/condition interaction notes
- **FR7**: Engine can calculate outgoing duration formulas for conditions and boons using expertise/concentration scaling + explicit modifiers

### Equipment & Gear System

- **FR8**: Engine can resolve canonical slot-budget values for level-80 ascended equipment per slot type and stat shape
- **FR9**: Engine can optimize PvP builds via distinct path (amulet+rune+sigils+relic+traits+skills only — no gear-prefix search)
- **FR17**: Engine can load all factual constants from canonical data files instead of hardcoded values
- **FR18**: Engine can validate data files at load time via typed loaders with typed validation errors (reject malformed enums, duplicate IDs, patch/mode mismatches)

### Patch-Aware Data Infrastructure

- **FR10**: Engine can apply per-mode trait/skill coefficient overrides from versioned balance data files
- **FR11**: Engine can resolve WvW-specific balance values without silently falling back to PvE assumptions
- **FR14**: Engine can represent Unknown values explicitly and block/degrade factual scoring when encountering them
- **FR21**: Engine can load patch manifests declaring patch_id, game_build_id, release_date, inheritance chain, and supported modes
- **FR22**: Engine can load patch ledgers with per-entity change records (field, old/new value, evidence level, source link)

### Effect Taxonomy & Normalization

- **FR12**: Engine can represent normalized effects across 23 categories (17 base + 6 boon/condition interaction ops) with stacking rules, trigger rules, uptime models, and timer/ICD/cap metadata
- **FR15**: Engine can resolve factorized dependency tables with cross-validation of stacking/cap/timer metadata
- **FR16**: Engine can classify every numeric rule as Factual, Derived, Heuristic, or Unknown with documentation and replaceability

### Heuristic Layer (Explicitly Separated)

- **FR13**: Engine can load rotation profiles as explicit heuristic data: per-profession/spec/mode condition rates, boon generation, target behavior, buff uptime assumptions
- **FR20**: Engine can score builds via objective profiles with 6-axis scorer (power, condition, boon_support, healing, sustain, control) and typed priority maps

### Persistence & Safety

- **FR19**: Engine can save/load builds with profession context for accurate metric reconstruction, using crash-safe persistence

## Non-Functional Requirements

### Data Integrity & Provenance

- **NFR1**: No balance-sensitive numeric value may exist without a `patch_id`. Balance datasets must be patch-versioned. Old patch data must not be overwritten in place. Verified by: unit test scanning all balance data structures for patch_id presence; integration test confirming old patch directories are preserved after new patch ingestion.
- **NFR2**: If a patch changes a value but the new number is not captured locally, record as `Unknown` and block/degrade factual scoring paths. Never silently continue with stale coefficients. Verified by: unit test feeding Unknown values and confirming scoring path blocks or returns labeled-heuristic result.
- **NFR3**: Source hierarchy precedence: (1) Official API/in-game data, (2) GW2 Wiki formulas, (3) Wiki patch notes, (4) Local curated balance snapshots, (5) Approved heuristics. Verified by: code review checklist on PRs modifying optimizer math; evidence level annotation on all data file entries.
- **NFR6**: Heuristics are allowed only when no authoritative value exists, and must be tagged, tested, and replaceable without breaking the engine. Verified by: unit test confirming heuristic-tagged values can be replaced without compilation errors; grep for `Heuristic` evidence level confirms all heuristics are tagged.

### Process & Review

- **NFR4**: Any PR changing optimizer math must state evidence level, source justification, factual vs heuristic classification, and affected modes. Verified by: PR template checklist requiring these four fields; CI check rejecting PRs without evidence annotation.
- **NFR5**: Every factual rule needs at least one source-backed test. Required test styles: exact formula tests, mode-split regression tests, profession profile regression tests, slot-budget total tests, save/load context preservation tests, snapshot tests for patch override datasets. Verified by: test coverage report mapping each factual rule to at least one test; CI gate requiring all factual tests pass.

### Mode Separation

- **NFR7**: PvE, PvP, and WvW must never be flattened into one coefficient table where balance data differs. Mode separation is non-negotiable. Verified by: regression test confirming Fury returns 25% in PvE and 20% in PvP/WvW; integration test loading mode-specific balance overrides and confirming distinct values per mode.

### Performance

- Optimization must complete within 120% of current implementation wall-clock time as measured by before/after benchmark on the same hardware
- Data file loading must not block addon startup — fall back to cached data if loading is incomplete
- No additional memory allocation beyond what typed data files require

### Stability (Inherited)

- See Domain-Specific Requirements → Crash Resilience for full constraints
- All new optimizer paths must maintain existing `catch_unwind` + `CancellationToken` + graceful degradation guarantees
