# Architecture Document — Epic 3

**Project**: GW2 Build Optimizer
**Epic**: Optimizer Correctness Overhaul and Patch-Aware Data-Driven Architecture
**Status**: Draft for readiness review
**Date**: 2026-03-06

**Authority delegation**: This document extracts architectural decisions from two authoritative source documents. When in doubt, defer to:
- `docs/optimizer-source-of-truth.md` — architecture model, invariants, implementation rules
- `docs/optimizer-data-schemas.md` — file layouts, schemas, validation, loader rules

**Historical context only** (not primary authority):
- `docs/architecture-assessment.md` — pre-Epic 3 assessment

---

## Core Position

There is not one GW2 optimizer. There are three optimization contexts sharing a common stat engine:
1. PvE
2. PvP
3. WvW

They share universal attribute math, but do not share all skill, trait, boon, condition, or scoring behavior.

---

## 5-Layer Architecture Model

```
Layer 1: Universal Stat & Combat Foundation
  - Attribute formulas, strike damage, condition formulas
  - Factual, mode-invariant (except where patch notes split)
  - Source: wiki, API

Layer 2: Mode-Aware Balance Layer
  - BalanceContext (patch_id + game_mode)
  - Mode-split boon values, condition coefficients, trait modifiers
  - Patch-versioned override datasets
  - Source: wiki patch notes, balance snapshots

Layer 3: Profession/Spec-Aware Effect Layer
  - NormalizedEffect (23 categories)
  - Stacking rules, trigger rules, uptime models
  - Boon/condition interaction operations
  - Source: API facts + wiki (factual); uptime estimates (heuristic, from Layer 4)

Layer 4: Explicit Heuristic Layer
  - Rotation profiles (condition application rates, boon generation, target behavior)
  - Labeled Heuristic evidence level
  - Replaceable without breaking factual engine
  - Source: curated estimates, community benchmarks

Layer 5: Objective Scorer (per optimization context)
  - 6-axis scoring: power, condition, boon_support, healing, sustain, control
  - Typed priority maps: boon_priorities, condition_priorities, interaction_priorities
  - Mode-specific, use-case-specific profiles
  - Explicitly heuristic — separated from factual engine
```

---

## Factorized Dependency Matrix

Do not build one giant matrix. Use factorized tables:

| Table | Inputs | Output | Evidence |
|-------|--------|--------|----------|
| `profession_profiles` | profession | armor class, health class, base HP, base defense | Factual |
| `slot_budgets` | slot, rarity, stat shape | flat attribute values | Factual |
| `attribute_formulas` | attributes | derived stats (crit chance, crit dmg, durations) | Factual |
| `condition_formulas` | mode, condition, movement state | tick/activation damage | Factual |
| `balance_overrides` | patch, mode, source_id | split trait/skill values | Factual |
| `normalized_effects` | traits, skills, upgrades | effect descriptors (23 categories) | Factual/Derived |
| `buff_profiles` | mode, scenario | active boons and buffs | Heuristic/explicit |
| `rotation_profiles` | patch, mode, profession, spec, objective | uptime and trigger assumptions | Heuristic |
| `objective_profiles` | mode, use case | scoring priorities (6-axis) | Heuristic |
| `scoring_rules` | objective profile, combat metrics | final ranking score | Heuristic |

---

## Key Type Contracts

### ProfessionProfile
```rust
pub struct ProfessionProfile {
    pub profession: ProfessionId,
    pub armor_weight: ArmorWeightClass,   // Heavy, Medium, Light
    pub health_class: HealthClass,         // High, Medium, Low
    pub base_health_level_80: i32,
    pub base_defense_level_80: i32,
}
```
Invariant: armor class and health class are independent. Guardian = Heavy + Low. Necromancer = Light + High.

### BalanceContext
```rust
pub struct BalanceContext {
    pub patch_id: String,
    pub game_mode: GameMode,  // PvE, PvP, WvW
}
```
All mode-sensitive lookups depend on BalanceContext.

### NormalizedEffect
```rust
pub struct NormalizedEffect {
    pub effect_id: String,
    pub source_type: SourceType,
    pub source_id: u32,
    pub source_name: String,
    pub category: EffectCategory,         // 23 variants
    pub stacking_rule: StackingRule,
    pub trigger_rule: TriggerRule,
    pub uptime_model: UptimeModel,
    pub effect_duration: Option<FactualValue<f64>>,
    pub internal_cooldown: Option<FactualValue<f64>>,
    pub max_stacks: Option<FactualValue<u32>>,
    pub evidence_level: EvidenceLevel,
    pub source: Option<String>,
}
```

### EffectCategory (23 variants)
- **Modifier (1-12)**: FlatStat, StatConversion, StrikeDamagePct, ConditionDamagePct, SpecificConditionDamagePct, CritDamagePct, BoonDurationPct, ConditionDurationPct, SpecificConditionDurationPct, OutgoingHealingPct, IncomingStrikeMultiplier, IncomingConditionMultiplier
- **Application (13-14)**: AppliesBoon, AppliesCondition
- **Interaction (15-20)**: RemovesBoon, StealsBoon, CorruptsBoon, RemovesCondition, ConvertsConditionToBoon, TransfersCondition
- **Control/Proc/Meta (21-23)**: DefianceDamage, ProcEffect, TriggeredEffect

### FactualValue<T>
```rust
pub enum FactualValue<T> {
    Resolved(T),
    Unknown,
}
```
Arithmetic on Unknown propagates Unknown. Single numeric uncertainty model across the codebase.

### DataQuality
```rust
pub enum DataQuality {
    Verified,      // all required factual values resolved
    Provisional,   // stale/heuristic fallback used, output allowed
    Blocked,       // required value missing, no safe fallback
}
```
Carries `Vec<DataQualityReason>` for UI-surfaceable explanations.

### ObjectiveScorer
```rust
pub struct ObjectiveScorer {
    pub objective_profile_id: String,
    pub patch_id: String,
    pub game_mode: GameMode,
    pub axis_weights: AxisWeights,          // 6-axis: power, condition, boon_support, healing, sustain, control
    pub boon_priorities: BoonPriorityMap,
    pub condition_priorities: ConditionPriorityMap,
    pub interaction_priorities: InteractionPriorityMap,
}
```

### RotationProfile
```rust
pub struct RotationProfile {
    pub profile_id: String,
    pub profession: ProfessionId,
    pub elite_spec: Option<SpecId>,
    pub objective_profile_id: String,
    pub boon_generation: BoonGenerationMap,
    pub boon_uptime: BoonUptimeMap,
    pub condition_application: ConditionApplicationMap,
    pub incoming_suppression: SuppressionMap,
    pub target_behavior: TargetBehavior,
    pub scenarios: Vec<ScenarioVariation>,
    pub evidence_level: EvidenceLevel,  // always Heuristic
}
```

---

## Data Directory Layout

```
data/
  manifests/
    2026-01-13.json              # Patch manifest (patch_id, game_build_id, sources)
  profession_profiles.json       # 9 professions, factual
  slot_budgets/
    level80_ascended.json        # Per-slot attribute budgets
  formulas/
    universal.json               # Mode-invariant formulas and constants
    conditions.json              # Per-mode condition formulas
    boons.json                   # Per-mode boon values + StatusDefinition metadata
  balance_overrides/
    2026-01-13/
      pve.json                   # Mode-specific trait/skill overrides
      pvp.json
      wvw.json
  patch_ledgers/
    2026-01-13.yaml              # Machine-readable patch diff
  normalized_effects/
    2026-01-13/
      pve.json                   # Classified effects per mode
      pvp.json
      wvw.json
  rotation_profiles/
    pve.json                     # Heuristic rotation assumptions
    pvp.json
    wvw.json
  objective_profiles/
    pve.json                     # 6-axis scoring profiles
    pvp.json
    wvw.json
```

---

## Loader Module Layout

```
crates/optimizer/src/data/
  mod.rs                         # Public API, startup orchestration
  profession_profiles.rs         # ProfessionProfile loader
  formulas.rs                    # Universal + condition + boon formula loaders
  slot_budgets.rs                # Slot budget loader
  balance_overrides.rs           # Balance override loader
  manifests.rs                   # Patch manifest loader
  patch_ledgers.rs               # Patch ledger loader (YAML)
  normalized_effects.rs          # NormalizedEffect loader
  rotation_profiles.rs           # RotationProfile loader
  objective_profiles.rs          # ObjectiveProfile loader
```

### Loader Behavior Rules
1. Return `Result<T, Vec<DataLoadError>>` — typed errors, never String
2. Strict deserialization — no `#[serde(default)]` on required fields
3. Reject malformed enums, duplicate IDs, patch/mode mismatches
4. Preserve `Unknown` explicitly — never silently default to zero
5. Data loaded once at startup into immutable in-memory snapshots
6. No mid-run hot reload; addon restart picks up changed files
7. Missing required data → optimizer disabled state (not crash)
8. Missing optional data → optimizer degraded state

---

## Startup and Data Lifecycle

```
Addon Load
  |
  v
Load Phase A data files (required)
  - profession_profiles.json
  - formulas/universal.json
  - formulas/conditions.json
  - formulas/boons.json
  - slot_budgets/level80_ascended.json
  |
  +-- Any missing/corrupt? --> Optimizer DISABLED (explicit error)
  |
  v
Load Phase B data files (optional)
  - manifests/, patch_ledgers/
  - balance_overrides/
  - normalized_effects/
  |
  +-- Any missing? --> Optimizer DEGRADED (reduced DataQuality)
  |
  v
Load Phase C data files (optional)
  - rotation_profiles/
  - objective_profiles/
  |
  +-- Any missing? --> Optimizer DEGRADED (heuristic layer unavailable)
  |
  v
Compare live /v2/build against manifest game_build_id
  |
  +-- Mismatch? --> Informational indicator (not automatic Provisional)
  |
  v
Ready: immutable in-memory data snapshot
  |
  v
Optimization runs use snapshot captured at run start
  - No mid-run reload
  - FactualValue::Unknown in path --> DataQuality degrades
```

---

## Evidence Classification System

| Level | Meaning | Allowed Source |
|-------|---------|----------------|
| Factual | Direct from API, wiki formula, or patch notes | API, wiki |
| Derived | Computed from factual inputs by deterministic formula | Factual inputs |
| Heuristic | Approved estimate, no authoritative source | Must be tagged, tested, replaceable |
| Unknown | Explicitly unresolved after patch change | Blocks or degrades factual paths |

### Source Hierarchy (precedence order)
1. Official API or in-game structured data
2. GW2 Wiki gameplay formulas
3. GW2 Wiki patch notes for mode splits
4. Local curated balance snapshots
5. Approved heuristics declared in source-of-truth

---

## Validation Matrix

### Per-Dataset Validation
1. Parse tests (malformed data rejected)
2. Schema invariant tests (required fields, valid enums)
3. Cross-file consistency tests

### Cross-File Consistency Checks
1. Every profession in code exists in `profession_profiles.json`
2. Every patch override references a valid patch manifest
3. Every objective profile references a valid mode
4. Every rotation profile references a valid objective profile
5. Every normalized effect field belongs to the allowed category set

---

## Phased Rollout

### Phase A — Factual Engine (P3-01 through P3-06)
Fix combat math. Deliverables: profession profiles, universal formulas, boon/condition formulas, duration formulas, slot budgets. Each with data file + loader + integration + tests.

### MVD-1 Milestone (after P3-07, optionally P3-16)
Releasable: all combat formulas correct and mode-aware. Defects D1-D5 fixed. Typed loaders in place.

### Phase B — Data Infrastructure (P3-07 through P3-13)
Patch versioning, balance overrides, effect system, PvP/WvW path separation.

### Phase C — Heuristic Layer (P3-14, P3-15)
Rotation profiles, objective profiles, scorer isolation. All explicitly heuristic.

### Standalone (P3-16)
Save/load persistence + crash safety. Can execute after P3-01.

---

## Review Gates

| Gate | After | Before | Criteria |
|------|-------|--------|----------|
| RG-1 | P3-09 | P3-10a, P3-12 | DataQuality design reviewed. Unknown handling tested. Override data audited. |
| RG-2 | P3-10a | P3-10b | NormalizedEffect types validated. 23 categories confirmed. Interaction payloads reviewed. |
| RG-3 | P3-10b | P3-13 | Effect data coverage reviewed. Evidence levels assigned. Cross-file consistency validated. |

---

## Existing Crate Architecture (Unchanged)

```
crates/addon/       — cdylib: Nexus entry point, ImGui UI, keybinds
crates/core/        — Shared types, config, storage
crates/gw2api/      — GW2 API v2 client, rate limiter, cache
crates/optimizer/   — Engine, combat math, scoring, search, LLM providers
```

Epic 3 primarily modifies `crates/optimizer/` (new `data/` module, formula refactors, effect system) and `crates/core/` (shared types like DataQuality, BalanceContext). The `crates/addon/` UI layer is not modified by Epic 3.

---

## Key Architectural Decisions

**ADR-01**: Data files are JSON (YAML for patch ledgers only). Loaded once at startup.

**ADR-02**: Canonical values in JSON, loaded into typed in-memory structs. No repeated file I/O on hot paths.

**ADR-03**: No schema versioning in data files. Schema changes update all files in same commit. Acceptable for solo-maintained addon.

**ADR-04**: Restart-based data reload. No manual "reload data" action in v1. Architecture permits adding it later.

**ADR-05**: `FactualValue<T>` is the single numeric uncertainty model. No parallel systems.

**ADR-06**: PvP amulet stats sourced from existing API/GameDb cache (`/v2/pvp/amulets`), not duplicated into data files.

**ADR-07**: Patch staleness detection is informational only. `DataQuality::Provisional` triggers only on specific stale/Unknown values consumed in computation, not on every build-ID mismatch.
