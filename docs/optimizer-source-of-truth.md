# GW2 Optimizer Source Of Truth

Status: Draft for adoption
Owner: Project maintainers
Audience: Human maintainers, AI coding agents, reviewers
Last updated: 2026-08-23

## Purpose

This document is the single implementation-grade source of truth for the Guild Wars 2 optimizer.

Its purpose is to:

1. Define which formulas and constants are factual.
2. Define which parts of the optimizer are heuristics.
3. Define the exact data model required to support PvE, PvP, and WvW correctly.
4. Give coding agents a hard boundary between "can implement now" and "must not invent."

This document supersedes any prior assumption that the optimizer can be modeled with one generic set of formulas or one mode-agnostic scoring pipeline.

## Core Position

There is not one GW2 optimizer.

There are three optimization contexts sharing a common stat engine:

1. PvE
2. PvP
3. WvW

They share universal attribute math, but they do not share all skill, trait, boon, condition, or scoring behavior.

The correct architecture is:

- one universal stat and combat foundation
- one mode-aware balance layer
- one profession/spec-aware effect layer
- one explicit heuristic layer
- one objective scorer per optimization context

## Patch Governance

Large balance patches can and do invalidate optimizer coefficients.

Therefore:

1. this document is versioned by patch reality, not by code convenience
2. balance datasets must be patch-versioned
3. old patch data must not be overwritten in place
4. every coefficient table must declare the patch it targets

Required rule:

- no balance-sensitive numeric value may exist without a `patch_id`

### Patch update policy

When a large patch lands:

1. create a new patch dataset directory
2. ingest the official patch notes
3. diff every affected skill, trait, boon, condition, and coefficient
4. mark unchanged data as inherited from the prior patch snapshot
5. mark changed data as overridden in the new patch snapshot
6. run factual regression tests
7. run mode-split regression tests
8. mark unresolved values as `Unknown` instead of silently keeping stale numbers

### Patch storage policy

Do this:

```text
data/
  balance_overrides/
    2026-01-13/
      pve.json
      pvp.json
      wvw.json
    2026-03-xx/
      pve.json
      pvp.json
      wvw.json
```

Do not do this:

- one mutable `current_balance.json`
- one mode-agnostic coefficient table
- one undocumented pile of hardcoded changes in Rust source

### Unknown-value rule

If a patch note confirms a split or coefficient change but the final numeric value is not yet captured locally:

1. record the entity as changed
2. set its value state to `Unknown`
3. block factual scoring paths that depend on it, or degrade explicitly to a labeled heuristic path
4. never silently continue with stale coefficients while claiming source-backed accuracy

## Source Hierarchy

When sources disagree, resolve them in this order:

1. Official API or in-game structured data for raw item, skill, trait, profession, and specialization records.
2. GW2 Wiki for gameplay formulas, derived-stat formulas, condition formulas, boon effects, and documented slot values.
3. GW2 Wiki patch notes for balance splits by mode and patch date.
4. Local curated balance snapshot files committed in this repository.
5. Approved heuristics declared in this document.

Coding agents must not invent numeric constants when the hierarchy does not provide one.

## Evidence Levels

Every numeric rule used by the optimizer must be classified:

- `Factual`: direct from API, wiki formula, or patch notes.
- `Derived`: computed from factual inputs by a deterministic formula.
- `Heuristic`: estimate used where no authoritative value exists.

Heuristics are allowed only when:

1. no authoritative value is available in a usable structured form
2. the heuristic is tagged
3. the heuristic is tested
4. the heuristic is replaceable later without breaking the rest of the engine

## Non-Negotiable Modeling Dimensions

Any correct optimizer result is a function of at least these dimensions:

1. `patch_id`
2. `game_mode` (`PvE`, `PvP`, `WvW`)
3. `profession`
4. `elite_spec`
5. `armor_weight_class`
6. `health_class`
7. `equipment_slot`
8. `item_rarity`
9. `attribute_combination`
10. `active_traits`
11. `active_skills`
12. `active_upgrades` (rune, sigil, relic, infusions, food, utility)
13. `buff_profile`
14. `target_profile`
15. `rotation_profile`
16. `objective_profile`

Any system that drops one of these dimensions must do so explicitly and with a documented tradeoff.

## Canonical Domain Model

### 1. Profession Profile

Every profession must resolve to a canonical `ProfessionProfile`.

Required fields:

```rust
pub struct ProfessionProfile {
    pub profession: ProfessionId,
    pub armor_weight: ArmorWeightClass,
    pub health_class: HealthClass,
    pub base_health_level_80: i32,
    pub base_defense_level_80: i32,
}
```

Canonical mappings:

#### Armor class

- Heavy: `Warrior`, `Guardian`, `Revenant`
- Medium: `Engineer`, `Ranger`, `Thief`
- Light: `Elementalist`, `Mesmer`, `Necromancer`

#### Base health class at level 80

- High (`9212`): `Warrior`, `Necromancer`
- Medium (`5922`): `Revenant`, `Engineer`, `Ranger`, `Mesmer`
- Low (`1645`): `Guardian`, `Thief`, `Elementalist`

#### Base defense at level 80

- Heavy: `1271`
- Medium: `1118`
- Light: `967`

Important:

- armor class and health class are separate
- they must never be inferred from each other
- `Guardian` is heavy armor but low health
- `Necromancer` is light armor but high health

## Universal Formulas

These formulas are factual and shared across modes unless patch notes state otherwise.

### 2. Base attributes at level 80

- Base primary attributes at level 80: `1000`
- `1 Vitality = 10 Health`
- Critical Chance at level 80:

```text
Critical Chance (%) = 5 + ((Precision - 1000) / 21)
Critical Chance (%) = (Precision - 895) / 21
```

- `15 Ferocity = 1% Critical Damage`
- `15 Expertise = 1% Condition Duration`
- `15 Concentration = 1% Boon Duration`
- Boon Duration cap: `100%`
- Condition Duration cap: `100%`

### 3. Strike damage foundation

At level 80:

```text
Damage done = skill_fact_damage * (Power / 1000) * (2597 / target_armor)
```

Notes:

- `2597` is the level-80 tooltip/reference armor.
- Armor is `Toughness + Defense`.
- Critical damage is applied through critical chance and critical damage multipliers.

### 4. Boons and common combat modifiers

Canonical values:

- Might: `+30 Power` and `+30 Condition Damage` per stack
- Vulnerability: `+1% damage taken` per stack, cap `25`
- Protection: `-33% incoming strike damage`
- Resolution: `-33% incoming condition damage`
- Fury:
  - PvE: `+25% critical chance`
  - PvP and WvW: `+20% critical chance`

Important:

- mode-specific boon values must not be flattened into one global value
- Fury is split by mode and must be modeled as such

### 5. Condition formulas at level 80

These are factual and must be represented explicitly by mode where split.

#### Bleeding

```text
0.06 * Condition Damage + 22
```

#### Burning

```text
0.155 * Condition Damage + 131
```

#### Poison

```text
0.06 * Condition Damage + 33.5
```

#### Torment

PvE, moving:

```text
0.06 * Condition Damage + 22
```

PvE, stationary:

```text
0.09 * Condition Damage + 31.8
```

PvP and WvW, moving:

```text
0.054 * Condition Damage + 19.8
```

PvP and WvW, stationary:

```text
0.07 * Condition Damage + 26
```

#### Confusion

PvE, over time:

```text
0.05 * Condition Damage + 18.25
```

PvE, on skill use:

```text
0.0325 * Condition Damage + 16.24
```

PvP and WvW, over time:

```text
10
```

PvP and WvW, on skill use:

```text
0.0975 * Condition Damage + 49.5
```

Important:

- confusion must not be modeled as a simple timer-only condition
- torment must distinguish moving vs stationary behavior
- confusion and torment must distinguish PvE from PvP/WvW

### 6. Duration formulas

Condition duration:

```text
outgoing_duration = base_duration * (1 + condition_duration_bonus)
```

Boon duration:

```text
outgoing_duration = base_duration * (1 + boon_duration_bonus)
```

Where:

- `condition_duration_bonus = (expertise / 1500) + explicit_duration_modifiers`
- `boon_duration_bonus = (concentration / 1500) + explicit_duration_modifiers`

Duration bonuses stack additively, then apply multiplicatively to base duration.

## Item And Slot Truth

### 7. Raw item and stat source

The authoritative source for raw item and stat composition is:

- `/v2/items`
- `/v2/itemstats`
- wiki slot tables when the API record alone is not enough to build a normalized slot budget

### 8. Slot attribute budgets

The optimizer must not use fabricated slot constants if authoritative slot values are available.

Current requirement:

- gear search must be driven by a canonical slot budget table for level-80 ascended equipment
- the table must be explicit and versioned
- the table must be separate for:
  - armor slots
  - one-handed weapons
  - two-handed weapons
  - trinkets
  - back item
  - PvP amulets

Examples confirmed from authoritative sources:

- Ascended one-handed weapon, 3-stat: `125 / 90 / 90`
- Ascended two-handed weapon, 3-stat: `251 / 179 / 179`
- Ascended amulet, 3-stat: `157 / 108 / 108`
- Ascended accessory, 3-stat: `110 / 74 / 74`
- Ascended ring, 3-stat: `126 / 85 / 85`

Important:

- armor weights do not change attribute bonuses for the same slot and rarity
- armor weights only change defense

### 9. PvP equipment model

PvP is not gear-optimized through normal armor/weapon/trinket prefixes.

PvP optimizer input must use:

- amulet
- rune
- sigils
- relic
- traits
- skills

All gear-based prefix search must be bypassed in PvP mode.

## Balance Split Layer

### 10. Patch-aware balance model

Mode split is not optional.

Traits and skills can differ across:

- PvE
- PvP
- WvW

The optimizer must support a patch-aware and mode-aware `BalanceContext`.

Required type:

```rust
pub struct BalanceContext {
    pub patch_id: String,
    pub game_mode: GameMode,
}
```

All mode-sensitive lookups must depend on `BalanceContext`.

Examples of split-sensitive values:

- Fury strength
- trait damage modifiers
- skill coefficients
- healing coefficients
- duration values
- defiance damage
- condition application counts

The January 13, 2026 patch notes confirm this split-balance requirement and must be treated as evidence that one global coefficient table is invalid.

### 10.1 Required patch-diff workflow

For each patch:

1. Build a machine-readable change ledger:
   - entity id
   - entity type
   - mode
   - field changed
   - old value
   - new value
   - source link
2. Update normalized effect records only from that ledger.
3. Recompute any derived caches.
4. Re-run all affected formula and balance tests.

Recommended file:

```text
data/
  patch_ledgers/
    2026-01-13.yaml
```

Example fields:

```yaml
- source_type: trait
  source_name: Glass Cannon
  mode: WvW
  field: strike_damage_pct
  old: 0.10
  new: 0.05
  source: https://wiki.guildwars2.com/wiki/Game_updates/2026-01-13
```

## Effect System

### 11. Effect representation

Traits, skills, runes, sigils, relics, food, and utilities must be normalized into explicit effects.

Required categories:

1. Flat stat bonus
2. Stat conversion
3. Strike damage modifier
4. Condition damage modifier
5. Specific-condition damage modifier
6. Crit damage modifier
7. Boon duration modifier
8. Condition duration modifier
9. Specific-condition duration modifier
10. Outgoing healing modifier
11. Incoming strike mitigation
12. Incoming condition mitigation
13. Boon application
14. Condition application
15. Defiance damage / crowd control contribution
16. Proc effect
17. Triggered effect with condition or uptime rules

Every effect must carry:

```rust
pub struct NormalizedEffect {
    pub source_id: SourceId,
    pub mode: GameMode,
    pub patch_id: String,
    pub category: EffectCategory,
    pub stacking_rule: StackingRule,
    pub trigger_rule: TriggerRule,
    pub uptime_model: UptimeModel,
    pub confidence: EvidenceLevel,
}
```

The current global assumption that all strike or condition modifiers simply multiply together is too coarse. Stacking behavior must be explicit per effect category.

## Rotation And Encounter Assumptions

### 12. This is the main heuristic layer

The optimizer cannot derive a perfect final score from raw stats alone.

The following are heuristics unless a stronger source is available:

- average condition stack counts
- condition uptime assumptions
- boon uptime assumptions
- target movement assumptions
- target skill activation frequency
- proc uptime
- trait trigger frequency
- rotation cast frequency
- whether a condition build is burning-heavy, torment-heavy, bleeding-heavy, or mixed

These heuristics must not be mixed into the factual engine.

Required separation:

```rust
pub struct RotationProfile {
    pub patch_id: String,
    pub game_mode: GameMode,
    pub profession: ProfessionId,
    pub elite_spec: Option<SpecId>,
    pub objective: ObjectiveProfileId,
    pub assumptions: RotationAssumptions,
}
```

### 13. Profession/spec condition profiles

The current approach of a few broad `ConditionWeights` presets is an acceptable temporary heuristic, not a final model.

Final target:

- one rotation profile per profession/spec/mode/objective cluster
- explicit condition application and expected uptime assumptions
- explicit target movement assumption for torment
- explicit target skill-use assumption for confusion

Approved temporary heuristic:

- use named condition profiles per spec or spec group
- store them as local data
- mark them `Heuristic`
- test them

## Objective Scoring Layer

### 14. Scoring is not factual

Raw stat math can be factual.
Final "this build is better" scoring is objective-dependent and therefore heuristic by design.

This means:

- score normalization constants are heuristics
- gear prefix profile cosine similarity is heuristic
- trait fact text scoring is heuristic
- disable score proxies are heuristic

Therefore, scoring must be explicitly isolated from the factual combat engine.

Required separation:

```rust
pub struct ObjectiveScorer {
    pub objective: ObjectiveProfileId,
    pub patch_id: String,
    pub game_mode: GameMode,
}
```

Example objective profiles:

- `PvE_Power_DPS`
- `PvE_Condi_DPS`
- `PvE_Quickness_Support`
- `PvE_Alacrity_Healer`
- `PvP_Burst_Duelist`
- `PvP_SideNode_Sustain`
- `WvW_Zerg_BoonSupport`
- `WvW_Roaming_Condi`

## Factorized Dependency Matrix

Do not build one giant matrix.

Use a factorized dependency system with these explicit matrices/tables:

| Table | Inputs | Output | Evidence level |
| --- | --- | --- | --- |
| `profession_profiles` | profession | armor class, health class, base health, base defense | Factual |
| `slot_budgets` | slot, rarity, item category, stat shape | flat attribute values | Factual |
| `attribute_formulas` | attributes | derived stats | Factual |
| `condition_formulas` | mode, condition, movement state | tick/activation formulas | Factual |
| `balance_overrides` | patch, mode, source_id | split trait/skill values | Factual |
| `normalized_effects` | traits, skills, upgrades | effect descriptors | Factual/Derived |
| `buff_profiles` | mode, scenario | active boons and buffs | Heuristic or explicit |
| `rotation_profiles` | patch, mode, profession, spec, objective | uptime and trigger assumptions | Heuristic |
| `objective_profiles` | mode, use case | scoring priorities | Heuristic |
| `scoring_rules` | objective profile, combat metrics | final ranking score | Heuristic |

This is the correct replacement for a single giant codependency matrix.

## Implementation Rules For Coding Agents

### 15. Mandatory rules

1. Never add a new numeric constant without placing it in one of the evidence classes.
2. Never implement a mode-agnostic trait or skill coefficient if wiki or patch notes show split behavior.
3. Never infer health class from armor class.
4. Never infer attribute bonuses from armor weight.
5. Never put heuristics directly into the stat engine.
6. Every heuristic must be named, documented, and replaceable.
7. Every code path that depends on patch or mode must accept `BalanceContext`.
8. PvP must not run the normal gear-prefix search path.
9. WvW must not silently reuse PvE balance data.
10. Save/load must persist enough context to avoid recomputing metrics from fake defaults.

### 16. Review rule

Any PR that changes optimizer math must state:

1. which evidence level is being modified
2. what source justifies the change
3. whether the change touches factual logic or heuristic logic
4. which modes are affected

## Current Gaps To Correct

v1.5.0 resolves the earlier profession health tiers, armor-weight baselines,
mode-aware condition formulas, Fury split, and canonical ascended slot budgets.
The remaining gaps are explicit:

1. Mode-specific timing and coefficient coverage is still incremental. Facts not
   present in the active snapshot remain `Unknown`, unmodeled, or `Provisional`.
2. Health-threshold skill coefficients are stored by mode, but the initial-target
   rotation model uses only the above-50% value. Dynamic threshold transitions
   require target-health-aware effect representation.
3. Disable and secured-sequence scoring is a timeline model, not a complete factual
   representation of every profession mechanic and opponent response.
4. Condition weighting remains a named heuristic profile system rather than a
   complete profession/spec rotation library.
5. Attunements, legends, shroud, heat, life force, pets, kits, and several other
   profession state machines are not yet modeled end to end.

## Recommended Repository Structure

The optimal implementation is data-first.

Recommended files:

```text
docs/
  optimizer-source-of-truth.md
  balance/
    sources.md
    patch-2026-01-13.md
data/
  profession_profiles.json
  slot_budgets_level80_ascended.json
  condition_formulas.json
  balance_overrides/
    2026-01-13/
      pve.json
      pvp.json
      wvw.json
  rotation_profiles/
    pve/
    pvp/
    wvw/
src/
  balance/
  effects/
  scoring/
```

## Recommended Rollout Order

This is the optimal implementation sequence.

### Phase 1: Freeze factual core

Implement first:

1. `ProfessionProfile`
2. `BalanceContext`
3. universal attribute formulas
4. mode-aware boon values
5. mode-aware condition formulas
6. canonical slot-budget dataset

Do not touch scoring yet.

### Phase 2: Normalize effect system

Implement:

1. trait normalization
2. skill normalization
3. rune/sigil/relic normalization
4. explicit stacking and trigger rules

### Phase 3: Replace fake search inputs

Implement:

1. gear search from slot budgets
2. PvP amulet-specific optimization path
3. WvW mode-aware balance overrides

### Phase 4: Build heuristic layer properly

Implement:

1. rotation profiles
2. target profiles
3. objective profiles
4. scorer separation

### Phase 5: Verification

Add tests for:

1. profession profile correctness
2. universal formulas
3. mode-split formulas
4. slot budget totals
5. effect extraction
6. heuristic profile loading
7. scoring isolation

## Test Requirements

Every factual rule needs at least one source-backed test.

Required test styles:

1. exact formula tests
2. mode-split regression tests
3. profession profile regression tests
4. slot-budget total tests
5. save/load context preservation tests
6. snapshot tests for patch override datasets

Recommended naming:

- `test_guardian_health_class_is_low`
- `test_necromancer_health_class_is_high`
- `test_fury_bonus_is_25_in_pve_and_20_in_competitive_modes`
- `test_torment_formula_is_mode_and_movement_aware`
- `test_pvp_optimizer_bypasses_gear_prefix_search`

## What Must Remain Heuristic For Now

Until a stronger system exists, these are explicitly heuristic:

1. condition stack weighting by profession/spec
2. target skill activation frequency for confusion
3. target movement fraction for torment
4. proc uptime estimates
5. objective scoring normalization ceilings
6. gear search pruning strategy
7. trait scoring from text facts

These are allowed, but only if:

- they live outside the factual engine
- they are named and documented
- they are tested
- they can be swapped out later

## Minimal Acceptance Standard

The optimizer may claim "source-backed" only if:

1. all universal formulas are factual
2. all profession base profiles are factual
3. all mode splits that materially affect output are represented
4. all heuristics are tagged as heuristics
5. no fake defaults are used where source-backed values exist

## References

Universal formulas and attributes:

- https://wiki.guildwars2.com/wiki/Attribute
- https://wiki.guildwars2.com/wiki/Precision
- https://wiki.guildwars2.com/wiki/Critical_Chance
- https://wiki.guildwars2.com/wiki/Ferocity
- https://wiki.guildwars2.com/wiki/Critical_Damage
- https://wiki.guildwars2.com/wiki/Concentration
- https://wiki.guildwars2.com/wiki/Boon_Duration
- https://wiki.guildwars2.com/wiki/Expertise
- https://wiki.guildwars2.com/wiki/Condition_Duration
- https://wiki.guildwars2.com/wiki/Vitality
- https://wiki.guildwars2.com/wiki/Health
- https://wiki.guildwars2.com/wiki/Armor
- https://wiki.guildwars2.com/wiki/Damage
- https://wiki.guildwars2.com/wiki/Damage_calculation

Condition formulas:

- https://wiki.guildwars2.com/wiki/Bleeding
- https://wiki.guildwars2.com/wiki/Burning
- https://wiki.guildwars2.com/wiki/Poison
- https://wiki.guildwars2.com/wiki/Torment
- https://wiki.guildwars2.com/wiki/Confusion
- https://wiki.guildwars2.com/wiki/Condition_Damage
- https://wiki.guildwars2.com/wiki/Condition

Boons and modifiers:

- https://wiki.guildwars2.com/wiki/Fury
- https://wiki.guildwars2.com/wiki/Protection
- https://wiki.guildwars2.com/wiki/Resolution
- https://wiki.guildwars2.com/wiki/Vulnerability
- https://wiki.guildwars2.com/wiki/Might

Equipment and slot values:

- https://wiki.guildwars2.com/wiki/Attribute_combinations
- https://wiki.guildwars2.com/wiki/Weapon
- https://wiki.guildwars2.com/wiki/Trinket
- https://wiki.guildwars2.com/wiki/Back_item
- https://wiki.guildwars2.com/wiki/Ascended_back_item
- https://wiki.guildwars2.com/wiki/API:2/items
- https://wiki.guildwars2.com/wiki/API:2/itemstats

Balance split evidence:

- https://wiki.guildwars2.com/wiki/Game_updates/2026-01-13
