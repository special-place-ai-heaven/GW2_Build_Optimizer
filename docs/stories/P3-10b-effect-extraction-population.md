# Story 3.10b: Effect Extraction and Population

Status: ready-for-dev

## Story

As a GW2 player,
I want the optimizer to have populated effect data for every trait, skill, rune, sigil, and relic that produces a numeric effect,
so that the optimizer's scoring and synergy evaluation is based on classified, structured effect data instead of ad-hoc extraction logic scattered across the codebase.

## Scope: Phase 1 Only

Per epic guidance ("the story may be split at this boundary — Phase 1 is independently valuable as a data deliverable"), this story delivers:

**Phase 1** (this story):
- Mapping infrastructure from old 8-variant `synergy::NormalizedEffect` to new 23-category `data::NormalizedEffect`
- Effect population logic that converts extracted effects to the new type system
- Baseline populated data files with representative effect entries
- Coverage report infrastructure
- Scorer for new 23-category effects (`score_effect()`)
- Tests proving the new type system can represent all existing effect patterns

**Phase 2** (future story):
- Full runtime cutover: synergy_pipeline.rs consumes data files instead of runtime extraction
- Complete population of ALL effects (programmatic extraction against GameDb)
- Full coverage percentages

## Non-Goals

- **No heuristic uptime values** -- P3-14 scope.
- **No rotation profiles** -- P3-14 scope.
- **No full engine cutover** -- that is Phase 2 scope.
- **No exhaustive data population** -- initial baseline covers representative examples per source type; full programmatic extraction is Phase 2.

## Dependencies

- **P3-10a (done)** -- NormalizedEffect type system with FactualValue<T>.
- **P3-04 (done)** -- Boon/condition metadata (stacking_mode, max_stacks, effect_class).
- **P3-09 (done)** -- FactualValue<T>, DataQuality.
- **Downstream**: P3-13 (evidence classification), P3-14 (uptime population).

## Acceptance Criteria

1. A mapping function `map_legacy_effect()` converts old 8-variant `synergy::NormalizedEffect` to new `data::NormalizedEffect` entries.
2. Baseline data files `data/normalized_effects/2026-01-13/{pve,pvp,wvw}.json` populated with representative effect entries covering all 8 legacy categories + new interaction categories.
3. Each populated effect has: effect_id, source_type, source_id, source_name, category (from 23), value (FactualValue), stacking_rule, trigger_rule, uptime_model, evidence_level.
4. Timer/cap metadata populated where factually known: effect_duration, internal_cooldown, max_stacks.
5. StatusOperation payloads populated for AppliesBoon/AppliesCondition entries with: operation_type, target_side, status_kind, amount_mode, amount_value, target_scope.
6. `score_effect()` function scores new 23-category NormalizedEffect entries, producing values comparable to existing `score_normalized_effect()`.
7. No `Estimated` uptime values in data files (only AlwaysOn, Derived, or Unknown).
8. Coverage report committed at `docs/reports/p3-10b-effect-coverage.md`.
9. All data files pass P3-10a validation (no duplicate effect_ids, evidence constraints, etc.).
10. At least one mode-split effect test (same source_id, different values in PvE vs PvP).
11. ProcEffect vs TriggeredEffect boundary tested with at least one borderline example each.
12. Evidence levels assigned: Factual entries include source URL, Derived entries document method.

## Technical Context

### Old 8-Variant System (synergy.rs)
```rust
pub enum NormalizedEffect {
    StatBonus { stat: StatType, value: f64 },
    DamageModifier { category: DamageCategory, percent: f64 },
    AppliesStatus { status: String, is_condition: bool, duration_s: u32, stacks: u32 },
    BenefitsFromStatus { status: String, effect: Box<NormalizedEffect> },
    StatConversion { source: StatType, target: StatType, percent: f64 },
    DurationBonus { kind: DurationKind, percent: f64 },
    Conditional { requires_trait_id: u32, overrides_index: Option<u32>, effect: Box<NormalizedEffect> },
    ProcEffect { trigger: ProcTrigger, effect: Box<NormalizedEffect>, estimated_uptime: f64 },
}
```

### Category Mapping (old → new)
| Old Variant | New Category |
|---|---|
| StatBonus | FlatStat |
| DamageModifier(Strike) | StrikeDamagePct |
| DamageModifier(Condition) | ConditionDamagePct |
| DamageModifier(CritDamage) | CritDamagePct |
| DamageModifier(Healing) | OutgoingHealingPct |
| AppliesStatus(boon) | AppliesBoon + StatusOperation |
| AppliesStatus(condition) | AppliesCondition + StatusOperation |
| BenefitsFromStatus | TriggeredEffect (inner = the wrapped effect) |
| StatConversion | StatConversion |
| DurationBonus(Boon) | BoonDurationPct |
| DurationBonus(Condition) | ConditionDurationPct |
| DurationBonus(SpecificCondition) | SpecificConditionDurationPct |
| Conditional | TriggeredEffect (conditional trigger) |
| ProcEffect | ProcEffect or TriggeredEffect depending on inner |

New categories NOT in old system (populated from domain knowledge):
- SpecificConditionDamagePct, IncomingStrikeMultiplier, IncomingConditionMultiplier
- RemovesBoon, StealsBoon, CorruptsBoon (boon denial)
- RemovesCondition, ConvertsConditionToBoon, TransfersCondition (condition cleanse)
- DefianceDamage

### Existing Extraction Functions
- `extract_trait_effects(trait, equipped_traits)` — parses GW2Trait Facts
- `extract_rune_effects(item)` — parses rune bonus text
- `extract_sigil_effects(item)` — parses sigil facts
- `extract_relic_effects(item)` — parses relic facts
- `extract_skill_effects(skill)` — parses Skill facts

### Scoring Function
`score_normalized_effect(effect, weights) -> f64` — the existing scorer. The new `score_effect()` should produce comparable scores for equivalent effects.

### Baseline Data Population Approach

For the initial baseline, hand-author representative entries covering:
1. **Traits**: 3-5 common traits per category (e.g., Scholar rune effect as FlatStat, a damage modifier trait as StrikeDamagePct)
2. **Runes**: 2-3 popular runes (Scholar, Firebrand, etc.)
3. **Sigils**: 2-3 popular sigils (Force, Accuracy, Smoldering)
4. **Relics**: 1-2 relics
5. **Boon/condition applications**: At least 2 AppliesBoon + 2 AppliesCondition with StatusOperation
6. **Interaction categories**: At least 1 entry per new interaction category (can be synthetic/example)
7. **ProcEffect + TriggeredEffect**: At least 1 borderline example each

Total: ~20-30 representative entries per mode file. NOT exhaustive — this proves the system works and enables Phase 2's programmatic extraction.

### Where to Put Code
- `crates/optimizer/src/data/normalized_effects.rs` — add `score_effect()`, `map_legacy_effect()`
- `data/normalized_effects/2026-01-13/{pve,pvp,wvw}.json` — populate with real entries
- `docs/reports/p3-10b-effect-coverage.md` — coverage report

## Tasks

- [ ] 1. Implement `map_legacy_effect()` that converts old NormalizedEffect → new data::NormalizedEffect (AC: 1)
- [ ] 2. Implement `score_effect()` for new 23-category types (AC: 6)
- [ ] 3. Populate pve.json with ~20-30 representative effect entries (AC: 2, 3, 4, 5, 7, 9, 12)
- [ ] 4. Populate pvp.json with mode-specific variants where different (AC: 2, 10)
- [ ] 5. Populate wvw.json (can mirror PvE for baseline where no split known) (AC: 2)
- [ ] 6. Add StatusOperation payloads for AppliesBoon/AppliesCondition entries (AC: 5)
- [ ] 7. Write ProcEffect vs TriggeredEffect boundary tests (AC: 11)
- [ ] 8. Write mode-split test (AC: 10)
- [ ] 9. Create coverage report (AC: 8)
- [ ] 10. Ensure all data files pass P3-10a validation (AC: 9)
- [ ] 11. Test score_effect() produces comparable values to old scorer (AC: 6)

## Verification

```bash
cargo test --package gw2-optimizer -v
cargo check
```

## Dev Notes

- The baseline data files prove the system works end-to-end. Phase 2 will programmatically extract from GameDb and reach high coverage.
- For evidence_level on hand-authored entries: use Factual with wiki URLs for well-known effects, Derived for calculated values.
- For uptime_model: passive effects → AlwaysOn, proc effects → Unknown (not Estimated — P3-14 handles that), conditional effects → Derived.
- The `map_legacy_effect()` function is useful for Phase 2 — it bridges the old extraction output to new types.
- `score_effect()` should be in normalized_effects.rs alongside the types. It needs access to OptimizationWeights.
- The DamageCategory enum in the old system maps to: Strike→StrikeDamagePct, Condition→ConditionDamagePct, CritDamage→CritDamagePct, Healing→OutgoingHealingPct.
- For representative entries, use real GW2 trait/skill IDs from the wiki. Example: Trait 214 (Empowered, Warrior) = +1% strike damage per bar of adrenaline. Sigil of Force (ID 24615) = +5% strike damage.

### References

- [Source: _bmad-output/planning-artifacts/epics.md, Story 3.10b]
- [Source: crates/optimizer/src/synergy.rs, NormalizedEffect and extractors]
- [Source: crates/optimizer/src/data/normalized_effects.rs, P3-10a types]
- [Source: docs/optimizer-data-schemas.md, Schema 9]
