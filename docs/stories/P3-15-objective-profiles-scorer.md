# Story 3.15: Objective Profiles and Typed State-Aware Scorer Isolation

Status: ready-for-dev

## Story

As a GW2 player,
I want the optimizer's scoring logic to be separated from the factual combat engine and driven by explicit, mode-specific objective profiles with typed boon and condition priorities,
so that build rankings reflect intentional heuristic choices about damage, sustain, boon support, and control pressure — not hardcoded assumptions pretending to be math.

## Non-Goals

- **No factual combat formula changes** -- Phase A formulas remain untouched.
- **No factual effect definition changes** -- Phase B data untouched.
- **No factual CC simulator** -- control is a heuristic valuation, not a simulation.
- **No rotation profile changes** -- P3-14 delivers those.

## Dependencies

- **P3-14 (done)** -- Rotation profiles (consumed by scorer).
- **P3-10b (done)** -- Typed effect data.
- **P3-13 (done)** -- Evidence/cross-file validation.
- This is the **final story** in Epic 3.

## Acceptance Criteria

1. `data/objective_profiles/{pve,pvp,wvw}.json` exist with typed loader (P3-07 pattern).
2. Each profile has: objective_profile_id, axis_weights (6 axes: power, condition, boon_support, healing, sustain, control), weight_budget, normalization_constants, boon_priorities, condition_priorities, interaction_priorities (optional), is_mode_default, notes, evidence_level (Heuristic).
3. Exactly one profile per mode has `is_mode_default: true`.
4. `OptimizationWeights` revised to 6 axes: power, condition, boon_support, healing, sustain, control. Old `disable` axis removed, replaced by `control`.
5. `ObjectiveScorer` type constructed from OptimizationWeights + objective profile data. Single entry point for scoring.
6. Old `score_with_weights` free function removed or made private behind ObjectiveScorer.
7. Hardcoded normalization constants (`STRIKE_DPS_NORM`, etc.) and `WEIGHT_BUDGET` removed from scoring.rs — loaded from objective profile data.
8. `OptimizationWeights::default_for_mode()` resolves through objective profile data, not hardcoded match arms.
9. Existing presets (power DPS, condi DPS, healer, support) replaced by named objective profiles in data files.
10. `boon_priorities` is typed by specific boon kind — scorer values boon generation/sustain/uptime per typed boon.
11. `condition_priorities` is typed by specific condition kind — scorer values offensive condition pressure per typed condition.
12. `interaction_priorities` (optional) weights interaction operations for control/boon_support axes.
13. `control` axis documented as heuristic valuation of control/denial pressure, replacing old `disable` proxy.
14. Addon UI radar chart renders 6 axes, normalization from active objective profile.
15. `set_constrained` budget enforcement uses weight_budget from active objective profile.
16. Rotation profile's null `objective_profile_id` resolves to mode default. Non-existent reference → Provisional quality.
17. Integration test: different objective profiles produce different build rankings for same build set.
18. Integration test: changing boon_priorities changes ranking. Changing condition_priorities changes ranking.
19. Full pipeline integration test: factual data → effects → rotation profile → objective profile → ObjectiveScorer → ranked output with DataQuality::Provisional.
20. Initial profiles per mode: PvE (Power DPS, Condi DPS, Boon Support, Healer, Hybrid Support), PvP (Burst, Sustain, Boon Pressure, Control/Disruptor), WvW (Zerg DPS, Zerg Support, Roamer, Disruptor).

## Technical Context

### 6-Axis Scoring Model

Old 5-axis: power, disable, condition, healing, sustain
New 6-axis: power, condition, boon_support, healing, sustain, control

The `disable` → `control` rename reflects that this axis is a heuristic valuation of control/denial pressure, not a factual CC calculation.

### ObjectiveScorer Type (sketch)

```rust
pub struct ObjectiveScorer {
    pub weights: OptimizationWeights,
    pub normalization: NormalizationConstants,
    pub boon_priorities: HashMap<String, f64>,
    pub condition_priorities: HashMap<String, f64>,
    pub interaction_priorities: Option<HashMap<String, f64>>,
    pub weight_budget: f64,
}

impl ObjectiveScorer {
    pub fn from_profile(weights: &OptimizationWeights, profile: &ObjectiveProfile) -> Self { ... }
    pub fn score(&self, combat: &CombatPerformance, ...) -> f64 { ... }
}
```

### NormalizationConstants

```rust
pub struct NormalizationConstants {
    pub strike_dps_norm: f64,
    pub condi_dps_norm: f64,
    pub boon_support_norm: f64,
    pub healing_power_norm: f64,
    pub effective_health_norm: f64,
    pub control_norm: f64,
}
```

### Current Hardcoded Values to Replace

In `scoring.rs`:
```rust
const STRIKE_DPS_NORM: f64 = 3000.0;
const CONDI_DPS_NORM: f64 = 2000.0;
const HEALING_POWER_NORM: f64 = 1500.0;
const EFFECTIVE_HEALTH_NORM: f64 = 30000.0;
const BOON_DURATION_NORM: f64 = 0.5;
const WEIGHT_BUDGET: f64 = 2.0;
```

These move into objective profile data files.

### OptimizationWeights Changes

- Add `boon_support` field (new)
- Add `control` field (new)
- Remove `disable` field
- Update `as_array()` to return 6 elements
- Update `set()`, `get()`, `set_constrained()` for 6 axes
- Update `AXIS_LABELS` to 6 labels
- Update `default_for_mode()` to load from objective profile
- Update `PRESETS` to reference objective profiles

### Addon UI Impact

- Radar chart in `crates/addon/src/ui/` renders axes — update from 5 to 6
- Weight sliders — update from 5 to 6
- Budget display — source from active profile

### Where Code Lives

- New: `crates/optimizer/src/data/objective_profiles.rs` — types, loader, validation
- New: `data/objective_profiles/{pve,pvp,wvw}.json` — profile data
- Modified: `crates/optimizer/src/scoring.rs` — ObjectiveScorer, remove hardcoded constants
- Modified: `crates/optimizer/src/data/mod.rs` — register module
- Modified: `crates/optimizer/src/engine.rs` — use ObjectiveScorer
- Modified: `crates/addon/src/ui/` — radar chart 6 axes, weight sliders

## Tasks

- [ ] 1. Create ObjectiveProfile, NormalizationConstants types and typed loader (AC: 1, 2, 3)
- [ ] 2. Revise OptimizationWeights to 6 axes: add boon_support, control; remove disable (AC: 4)
- [ ] 3. Create ObjectiveScorer type with score() method (AC: 5, 6)
- [ ] 4. Move hardcoded normalization constants and WEIGHT_BUDGET into objective profile data (AC: 7)
- [ ] 5. Author PvE objective profiles (Power DPS, Condi DPS, Boon Support, Healer, Hybrid Support) (AC: 20)
- [ ] 6. Author PvP objective profiles (Burst, Sustain, Boon Pressure, Control/Disruptor) (AC: 20)
- [ ] 7. Author WvW objective profiles (Zerg DPS, Zerg Support, Roamer, Disruptor) (AC: 20)
- [ ] 8. Implement typed boon_priorities and condition_priorities scoring (AC: 10, 11)
- [ ] 9. Implement interaction_priorities scoring (AC: 12)
- [ ] 10. Update default_for_mode() to resolve through objective profile data (AC: 8, 9)
- [ ] 11. Update addon UI: radar chart 6 axes, weight sliders, budget from profile (AC: 14, 15)
- [ ] 12. Implement objective profile resolution (null → mode default, missing → Provisional) (AC: 16)
- [ ] 13. Write integration tests: different profiles → different rankings (AC: 17, 18, 19)
- [ ] 14. Delete old score_with_weights, hardcoded PRESETS, hardcoded norms (AC: 6, 7, 9)

## Verification

```bash
cargo test --package gw2-optimizer -v
cargo test -p gw2_addon -- --test-threads=1
cargo check
```

## Dev Notes

- **HIGH RISK** — this is the largest and most impactful story in Epic 3. It touches scoring, engine, addon UI, and creates a new data-driven scoring architecture.
- The `disable` → `control` rename affects OptimizationWeights which is used throughout the codebase. Find ALL references before changing.
- The radar chart in the addon UI renders 5 axes currently. Updating to 6 axes may require layout adjustments.
- `WEIGHT_BUDGET` is currently 2.0. The objective profile can specify a different budget per profile.
- Keep `set_constrained()` proportional-scaling logic — just source the budget from the active profile instead of a constant.
- For initial profiles, migrate existing preset values to the new 6-axis model. The old `disable` weight maps to `control`. The new `boon_support` axis needs new values.
- `score_with_weights()` currently uses `CombatPerformance` fields directly. The new `ObjectiveScorer::score()` should consume the same fields but add typed boon/condition priority weighting.
- The addon crate (`gw2_addon`) tests MUST run with `--test-threads=1` due to global state.
- All objective profiles are Heuristic — never Factual.

### References

- [Source: _bmad-output/planning-artifacts/epics.md, Story 3.15]
- [Source: crates/optimizer/src/scoring.rs, score_with_weights(), OptimizationWeights, PRESETS]
- [Source: crates/addon/src/ui/, radar chart rendering]
- [Source: crates/optimizer/src/engine.rs, scoring call sites]
