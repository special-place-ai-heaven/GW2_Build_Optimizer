# Story 3.14: Rotation Profiles and Heuristic Uptime Population

Status: ready-for-dev

## Story

As a GW2 player,
I want the optimizer to use explicit, per-profession/spec rotation profiles instead of hardcoded preset condition weights,
so that condition application rates, buff uptimes, and target behavior assumptions are transparent, tunable, and replaceable.

## Non-Goals

- **No factual combat formula changes** -- Phase A formulas remain untouched.
- **No objective profiles or scorer isolation** -- that is P3-15.
- **No simulation-backed data** -- all profiles are heuristic estimates.
- **No UI changes** -- data and engine plumbing only.

## Dependencies

- **P3-04 (done)** -- StatusDefinition metadata (stacking_mode, max_stacks, max_duration_ms) for clamping.
- **P3-07 (done)** -- typed loader pattern.
- **P3-10b (done)** -- populated effect data with timer/ICD/cap metadata.
- **Downstream**: P3-15 (objective profiles consume rotation profiles).

## Acceptance Criteria

1. `data/rotation_profiles/{pve,pvp,wvw}.json` exist with typed loader following P3-07 pattern.
2. `RotationProfile` type with fields: profile_id, profession, elite_spec (nullable), objective_profile_id (nullable), boon_generation, boon_uptime, condition_application, incoming_suppression, target_behavior, scenarios, evidence_level (must be Heuristic), notes.
3. `condition_application` is a map of condition kind → typed application metrics matching P3-04 stacking_mode:
   - `avg_stacks_per_second` for intensity-stacking damaging conditions (Bleeding, Burning, Torment, Confusion, Poisoned)
   - `avg_duration_ms_per_second` for duration-stacking conditions (Fear, Taunt, Daze, etc.)
   - `avg_stacks` for steady-state debuffs (Vulnerability)
4. `boon_generation` is a map of boon kind → generation metrics (avg_stacks_per_second for intensity, avg_duration_ms_per_second for duration).
5. `boon_uptime` is a map of boon kind → f64 uptime fraction (0.0–1.0). Separate from boon_generation.
6. `incoming_suppression` is a map of condition/control kind → f64 uptime fraction (0.0–1.0).
7. `target_behavior` contains: movement_fraction (f64), skill_use_frequency_per_second (f64).
8. `scenarios` array with ScenarioProfile entries: scenario_id, label, might_stacks, vulnerability_stacks, optional boon overrides. At minimum 3 scenarios per profile: solo, party, full_squad.
9. All values are f64 averages (not booleans/integers). All profiles evidence_level = Heuristic.
10. `condition_weights_for_profession()` and `default_buff_profiles()` are deleted and replaced by rotation profile lookups.
11. All call sites in combat.rs, engine.rs, synergy_pipeline.rs consume RotationProfile/ScenarioProfile.
12. Fallback: missing profile → generic fallback profile → DataQuality::Provisional with reason.
13. Heuristic uptime population: NormalizedEffect entries with Unknown uptime updated to Estimated where applicable, with evidence_level set to Heuristic.
14. At minimum one profile per core profession per mode (9 × 3 = 27 minimum) plus generic fallback per mode.
15. Integration test: load factual data → load rotation profile → compute combat metrics → verify DataQuality is Provisional, verify changing rotation profile changes output, verify solo < party < full_squad for offensive metrics.

## Technical Context

### Current Systems to Replace

```rust
// In combat.rs — to be replaced by rotation profile data
pub fn condition_weights_for_profession(profession: &str) -> ConditionWeights { ... }
pub fn default_buff_profiles() -> Vec<BuffProfile> { ... }
```

These hardcoded functions are replaced by data-driven rotation profile lookups.

### RotationProfile Type (sketch)

```rust
pub struct RotationProfile {
    pub profile_id: String,
    pub profession: String,
    pub elite_spec: Option<String>,
    pub objective_profile_id: Option<String>,
    pub boon_generation: HashMap<String, GenerationMetrics>,
    pub boon_uptime: HashMap<String, f64>,
    pub condition_application: HashMap<String, ApplicationMetrics>,
    pub incoming_suppression: HashMap<String, f64>,
    pub target_behavior: TargetBehavior,
    pub scenarios: Vec<ScenarioProfile>,
    pub evidence_level: EvidenceLevel,
    pub notes: String,
}

pub struct ScenarioProfile {
    pub scenario_id: String,
    pub label: String,
    pub might_stacks: f64,
    pub vulnerability_stacks: f64,
    pub boon_overrides: Option<HashMap<String, f64>>,
}

pub struct TargetBehavior {
    pub movement_fraction: f64,
    pub skill_use_frequency_per_second: f64,
}
```

### ApplicationMetrics and GenerationMetrics

These use a tagged enum pattern matching P3-04 stacking modes:
```rust
#[serde(tag = "mode")]
pub enum ApplicationMetrics {
    IntensityRate { avg_stacks_per_second: f64 },
    DurationRate { avg_duration_ms_per_second: f64 },
    SteadyState { avg_stacks: f64 },
    ProcRate { expected_procs_per_second: f64 },
}
```

### Factual Cap Enforcement

Rotation profile heuristic values are clamped by factual constraints:
- Might stacks capped at 25 (from P3-04 max_stacks)
- Vulnerability stacks capped at 25
- Boon uptimes capped at 1.0
- Proc rates capped at 1/ICD (from P3-10b effect timer metadata)

### Where Code Lives

- New: `crates/optimizer/src/data/rotation_profiles.rs` — types, loader, validation
- New: `data/rotation_profiles/{pve,pvp,wvw}.json` — profile data
- Modified: `crates/optimizer/src/data/mod.rs` — register module
- Modified: `crates/optimizer/src/combat.rs` — consume RotationProfile instead of hardcoded functions
- Modified: `crates/optimizer/src/engine.rs` — load and pass rotation profiles
- Modified: `crates/optimizer/src/synergy_pipeline.rs` — consume ScenarioProfile for buff environment
- Deleted: `condition_weights_for_profession()`, `default_buff_profiles()`, `ConditionWeights`, `BuffProfile`

## Tasks

- [ ] 1. Create RotationProfile, ScenarioProfile, TargetBehavior, ApplicationMetrics, GenerationMetrics types (AC: 2, 3, 4, 5, 6, 7, 8)
- [ ] 2. Create typed loader with include_str! + OnceLock + validation (AC: 1)
- [ ] 3. Author PvE rotation profiles: 9 core professions + key elite specs + generic fallback (AC: 14)
- [ ] 4. Author PvP and WvW rotation profiles: 9 core + generic fallbacks per mode (AC: 14)
- [ ] 5. Implement profile lookup with fallback resolver (AC: 12)
- [ ] 6. Replace condition_weights_for_profession() with rotation profile lookups (AC: 10, 11)
- [ ] 7. Replace default_buff_profiles() with ScenarioProfile-driven computation (AC: 10, 11)
- [ ] 8. Update combat.rs to consume RotationProfile (AC: 11)
- [ ] 9. Update engine.rs and synergy_pipeline.rs to pass rotation profiles (AC: 11)
- [ ] 10. Apply factual cap enforcement (max_stacks, uptime clamps, ICD caps) (AC: 9)
- [ ] 11. Populate heuristic uptimes for NormalizedEffect entries (AC: 13)
- [ ] 12. Write integration test: full pipeline with rotation profiles (AC: 15)
- [ ] 13. Delete ConditionWeights, condition_weights_for_profession(), default_buff_profiles(), BuffProfile (AC: 10)

## Verification

```bash
cargo test --package gw2-optimizer -v
cargo check
```

## Dev Notes

- **HIGH RISK** story due to scope: new type system, new data files (27+ profiles), runtime replacement of existing systems, engine integration.
- All rotation profiles are explicitly Heuristic. No profile claims Factual.
- condition_application metric mode MUST match P3-04 stacking_mode for each condition. Don't use avg_stacks_per_second for a duration-stacking condition.
- The multi-scenario pattern (solo/party/full_squad) is preserved but data-driven. Engine currently computes 3 scenarios via default_buff_profiles() — replace with ScenarioProfile array from rotation profile.
- For initial profiles, use reasonable estimates from GW2 community benchmarks (e.g., Snow Crows, Hardstuck). Document assumptions in notes field.
- boon_generation and boon_uptime are SEPARATE concepts: generation = this build's output, uptime = what this build benefits from. A DPS player may have 0 boon_generation but 0.9 Might uptime (from party support).
- The generic fallback profile should be conservative: moderate Might (15 stacks party), moderate condition application, basic sustain. It's a "we don't know this build" fallback.
- When deleting condition_weights_for_profession() and default_buff_profiles(), find all call sites first (combat.rs, engine.rs, synergy_pipeline.rs) and update them to use the new types.

### References

- [Source: _bmad-output/planning-artifacts/epics.md, Story 3.14]
- [Source: crates/optimizer/src/combat.rs, condition_weights_for_profession(), default_buff_profiles()]
- [Source: crates/optimizer/src/data/boon_condition_formulas.rs, StatusDefinition metadata]
- [Source: crates/optimizer/src/data/normalized_effects.rs, UptimeModel]
