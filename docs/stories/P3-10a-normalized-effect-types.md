# Story 3.10a: NormalizedEffect Types, Schema, and Contracts

Status: ready-for-dev

## Story

As a GW2 player,
I want the optimizer to have a structured type system for every effect that traits, skills, runes, sigils, and relics produce,
so that the optimizer can reason about stacking rules, trigger conditions, and evidence levels per effect instead of treating all modifiers as a single multiplied blob.

## Non-Goals

- **No effect data population** -- that is P3-10b scope. Stub data files have empty effects arrays.
- **No heuristic uptime values** -- that is P3-14 scope. `Estimated` uptime values are NOT populated.
- **No effect extraction from API data** -- P3-10b scope.
- **No scoring changes** -- the scorer doesn't consume NormalizedEffect yet (that's P3-15).
- **No UI changes** -- types and data infrastructure only.

## Dependencies

- **P3-04 (done)** -- StatusDefinition metadata (stacking_mode, max_stacks, effect_class) in boon/condition formulas.
- **P3-07 (done)** -- DataLoadError, typed loader pattern.
- **P3-08 (done)** -- Patch manifest infrastructure for patch_id/mode directory structure.
- **P3-09 (done)** -- FactualValue<T> for numeric uncertainty, DataQuality for Unknown integration.
- **Downstream**: P3-10b (populates effect data), P3-14 (assigns heuristic uptime).

## Acceptance Criteria

1. `NormalizedEffect` struct with fields: `effect_id`, `source_type`, `source_id`, `source_name`, `category`, `value` (FactualValue<f64>), `stacking_rule`, `trigger_rule`, `uptime_model`, `evidence_level`, `source` (optional URL), plus optional timer/cap metadata: `effect_duration`, `internal_cooldown`, `max_stacks`.
2. `EffectCategory` enum with 23 variants (12 modifier + 2 application + 6 interaction + 3 control/proc/meta).
3. `StackingRule` enum: Multiplicative, Additive, Highest, NonStacking.
4. `TriggerRule` enum: Passive, OnCrit, OnHit, OnSkillUse, OnHealthThreshold, Conditional.
5. `UptimeModel` struct with `kind` field: AlwaysOn, Estimated, Derived, Unknown. Estimated carries uptime value as FactualValue<f64>.
6. `StatusOperation` struct for boon/condition interaction payloads (categories 13-20).
7. JSON schema at `data/normalized_effects/<patch_id>/<mode>.json`. Initial baseline files are empty stubs.
8. Typed loader following P3-07 pattern with `Result<T, Vec<DataLoadError>>`.
9. Validation: duplicate effect_id detection, Estimated+non-Heuristic evidence rejection, timer/cap consistency.
10. Numeric fields use `FactualValue<T>`, not raw f64 + separate evidence marker.
11. No catch-all "Other" or "Misc" category variants.
12. `docs/optimizer-data-schemas.md` Schema 9 updated to reflect all 23 categories.

## Technical Context

### EffectCategory Enum (23 variants)

Modifier categories (1-12):
1. FlatStat
2. StatConversion
3. StrikeDamagePct
4. ConditionDamagePct
5. SpecificConditionDamagePct
6. CritDamagePct
7. BoonDurationPct
8. ConditionDurationPct
9. SpecificConditionDurationPct
10. OutgoingHealingPct
11. IncomingStrikeMultiplier
12. IncomingConditionMultiplier

Application categories (13-14):
13. AppliesBoon
14. AppliesCondition

Interaction categories (15-20):
15. RemovesBoon
16. StealsBoon
17. CorruptsBoon
18. RemovesCondition
19. ConvertsConditionToBoon
20. TransfersCondition

Control/proc/meta (21-23):
21. DefianceDamage
22. ProcEffect -- the proc IS the final output (damage, heal, utility)
23. TriggeredEffect -- the trigger GATES another effect (carries inner_category: EffectCategory)

ProcEffect vs TriggeredEffect boundary: if the event directly produces damage/healing/utility, it's ProcEffect. If it activates a modifier/buff, it's TriggeredEffect.

### StatusOperation Payload (for categories 13-20)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusOperation {
    pub operation_type: OperationType,
    pub target_side: TargetSide,
    pub status_kind: String,
    pub amount_mode: AmountMode,
    pub amount_value: FactualValue<f64>,
    pub base_duration_ms: Option<FactualValue<u32>>,
    pub target_scope: TargetScope,
    pub target_count: Option<FactualValue<u32>>,
    pub internal_cooldown_ms: Option<FactualValue<u32>>,
    pub source_duration_multiplier: Option<FactualValue<f64>>,
}
```

### NormalizedEffect Struct

```rust
pub struct NormalizedEffect {
    pub effect_id: String,
    pub source_type: SourceType,
    pub source_id: u32,
    pub source_name: String,
    pub category: EffectCategory,
    pub value: FactualValue<f64>,
    pub stacking_rule: StackingRule,
    pub trigger_rule: TriggerRule,
    pub uptime_model: UptimeModel,
    pub evidence_level: EvidenceLevel,
    pub source: Option<String>,
    // Timer/cap metadata
    pub effect_duration: Option<FactualValue<f64>>,
    pub internal_cooldown: Option<FactualValue<f64>>,
    pub max_stacks: Option<FactualValue<u32>>,
    // Interaction payload (for categories 13-20)
    pub status_operation: Option<StatusOperation>,
    // TriggeredEffect inner category reference
    pub inner_category: Option<EffectCategory>,
}
```

### Existing P3-04 metadata (in boon_condition_formulas.rs)
Boons have: `stacking_mode` (Intensity/Duration), `max_stacks`, `max_duration_ms`, `effect_class` (OffensiveStat/DefensiveStat/Throughput/etc).
Conditions have: `stacking_mode`, `max_stacks` (optional), `effect_class` (Damage/Debuff/Suppression/Control).

### Data File Location
- `data/normalized_effects/2026-01-13/pve.json` -- empty stub
- `data/normalized_effects/2026-01-13/pvp.json` -- empty stub
- `data/normalized_effects/2026-01-13/wvw.json` -- empty stub

### Where to Put Code
- `crates/optimizer/src/data/normalized_effects.rs` -- types, enums, loader, validation
- Register in `data/mod.rs`

## Tasks

- [ ] 1. Create `SourceType` enum (Trait, Skill, Rune, Sigil, Relic) (AC: 1)
- [ ] 2. Create `EffectCategory` enum with 23 variants (AC: 2, 11)
- [ ] 3. Create `StackingRule` enum (AC: 3)
- [ ] 4. Create `TriggerRule` enum (AC: 4)
- [ ] 5. Create `UptimeModel` struct with kind enum and optional uptime value (AC: 5)
- [ ] 6. Create supporting enums: `OperationType`, `TargetSide`, `AmountMode`, `TargetScope` (AC: 6)
- [ ] 7. Create `StatusOperation` struct (AC: 6)
- [ ] 8. Create `NormalizedEffect` struct with all fields including timer/cap metadata (AC: 1, 10)
- [ ] 9. Create `NormalizedEffectsFile` wrapper struct with patch_id, mode, effects vec (AC: 7)
- [ ] 10. Implement loader: `include_str!` + `OnceLock` + `try_load_normalized_effects()` (AC: 8)
- [ ] 11. Implement validation: duplicate effect_id, Estimated+non-Heuristic, timer/cap consistency (AC: 9)
- [ ] 12. Create empty baseline stub files (AC: 7)
- [ ] 13. Register module in `data/mod.rs` with re-exports (AC: 8)
- [ ] 14. Update `docs/optimizer-data-schemas.md` Schema 9 with all 23 categories (AC: 12)
- [ ] 15. Add tests: enum serde round-trips, validation rules, error paths, FactualValue integration (AC: 1-11)

## Verification

```bash
cargo test --package gw2-optimizer -v
cargo check
```

## Dev Notes

- This is a HIGH RISK story due to the number of types. Focus on getting the type system right. Data population is P3-10b.
- The `EffectCategory` enum replaces the schema doc's `ProcDamage` with `ProcEffect` (more general).
- `TriggeredEffect` carries `inner_category` to reference which effect type it gates.
- Categories 13-20 carry `StatusOperation` payload. Categories 1-12, 21-23 may have it as None.
- `FactualValue<u32>` arithmetic: P3-09 only impl'd f64 arithmetic. If needed, add u32 impls or convert to f64 for math. For this story, u32 fields are metadata (max_stacks, duration_ms) — arithmetic isn't needed, just Resolved/Unknown wrapping.
- The Estimated uptime constraint (must be Heuristic evidence) is validation-only. No Estimated values are populated in this story.
- Use `#[serde(rename_all = "PascalCase")]` or explicit rename for JSON field names matching existing conventions.
- Keep enum variant names matching the schema doc where possible (IncomingStrikeMultiplier, not IncomingStrikeModifier).

### References

- [Source: _bmad-output/planning-artifacts/epics.md, Story 3.10a]
- [Source: docs/optimizer-data-schemas.md, Schema 9]
- [Source: docs/optimizer-source-of-truth.md, Section 11]
- [Source: crates/optimizer/src/data/boon_condition_formulas.rs, StatusDefinition metadata]
