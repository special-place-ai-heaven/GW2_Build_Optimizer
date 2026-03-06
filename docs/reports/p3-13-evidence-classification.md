# P3-13 Evidence Classification Report

Generated: 2026-03-06

## Purpose

This report classifies every data entry across the Phase A and Phase B data tables
by evidence level (Factual, Derived, Heuristic, Unknown) and verifies cross-file
referential integrity. It also documents deferred Phase C tables and integrates
findings from the P3-12 WvW Non-Fallback Audit.

## Evidence Level Definitions

| Level | Definition | Source Requirement |
|-------|-----------|-------------------|
| **Factual** | Directly from wiki, GW2 API, or game data with exact values | Must cite wiki URL or API endpoint |
| **Derived** | Calculated from factual data using known formulas | Must cite derivation method or source factual data |
| **Heuristic** | Empirically tuned or estimated values | Should cite reasoning or gameplay basis |
| **Unknown** | Unverified or placeholder values | No citation required |

---

## Per-Table Evidence Breakdown

### 1. profession_profiles.json

| Metric | Value |
|--------|-------|
| Total entries | 9 |
| Factual | 9 (100%) |
| Derived | 0 |
| Heuristic | 0 |
| Unknown | 0 |
| Source citations | All 9 entries cite wiki Health and Armor pages |

**Assessment**: Complete. All 9 GW2 professions present with wiki-verified base health
and base defense values. Cross-validated against in-game HP-class != armor-class rule
(e.g., Guardian is Heavy armor but Low health).

### 2. universal_formulas.json (data/formulas/universal.json)

| Metric | Value |
|--------|-------|
| Total entries | 1 (single file with 11 formula constants) |
| Factual | 1 (100%) |
| Derived | 0 |
| Heuristic | 0 |
| Unknown | 0 |
| Source citations | 7 wiki URLs (Attribute, Critical_Chance, Ferocity, Damage, Health, Boon_duration, Condition_duration) |

**Assessment**: Complete. All constants (base_primary_attribute=1000, precision_offset=895,
precision_per_crit_pct=21, ferocity_per_crit_damage_pct=15, base_crit_damage_pct=150,
vitality_to_health=10, tooltip_reference_armor=2597, duration caps=1.0) are mode-invariant
and wiki-verified. The loader enforces `evidence_level == Factual` at validation time.

### 3. boon_condition_formulas (data/formulas/boons.json)

| Metric | Value |
|--------|-------|
| Total entries | 13 boon definitions |
| Factual | 13 (100%) |
| Derived | 0 |
| Heuristic | 0 |
| Unknown | 0 |
| Source citations | All 13 cite individual wiki boon pages |

**Boons covered**: Fury, Might, Protection, Resolution, Vulnerability, Quickness,
Alacrity, Aegis, Stability, Resistance, Regeneration, Vigor, Swiftness.

**Mode splits handled**: Fury crit_chance_bonus (PvE: 0.25, PvP/WvW: 0.20). All other
boon effects are mode-invariant via `all_modes` entries.

### 4. boon_condition_formulas (data/formulas/conditions.json)

| Metric | Value |
|--------|-------|
| Total entries | 15 condition definitions |
| Factual | 15 (100%) |
| Derived | 0 |
| Heuristic | 0 |
| Unknown | 0 |
| Source citations | All 15 cite individual wiki condition pages |

**Conditions covered**: Bleeding, Burning, Poisoned, Torment, Confusion, Vulnerability,
Weakness, Blinded, Slow, Chilled, Immobile, Crippled, Fear, Taunt, Daze.

**Mode splits handled**: Torment (stationary/moving coefficients differ PvE vs PvP/WvW),
Confusion (over_time/on_skill_use coefficients differ PvE vs PvP/WvW).

### 5. slot_budgets (data/slot_budgets/level80_ascended.json)

| Metric | Value |
|--------|-------|
| Total entries | 36 (12 slots x 3 stat shapes) |
| Factual | 36 (100%) |
| Derived | 0 |
| Heuristic | 0 |
| Unknown | 0 |
| Source citations | File-level: 3 sources (GW2 API items, itemstats, wiki Attribute_combinations) |

**Assessment**: Complete. All 36 slot/shape combinations present. ThreeStat values
cross-verified against specific API items (Zojja's series for Berserker). FourStat
and CelestialLike values derived from attribute_adjustment formulas and cross-verified
against API items (Commander's ShortBow, Celestial Ring). Weight-class invariance
verified across Heavy/Medium/Light armor.

### 6. patch_manifests (data/manifests/2026-01-13.json)

| Metric | Value |
|--------|-------|
| Total entries | 1 manifest |
| Source citations | 1 URL (wiki Game_updates/2025) |

**Assessment**: Baseline manifest anchors to game_build_id 175218. Status is "active".
Supports all 3 modes (PvE, PvP, WvW). The manifest system includes inheritance chain
validation (no cycles, no dangling references, no two active in same lineage).

### 7. balance_overrides (data/balance_overrides/2026-01-13/{pve,pvp,wvw}.json)

| Metric | Value |
|--------|-------|
| Total files | 3 (PvE, PvP, WvW) |
| Total entities | 0 (baseline has no overrides) |
| Factual | 0 |
| Derived | 0 |
| Heuristic | 0 |
| Unknown | 0 |

**Assessment**: Empty baseline by design. Override entries will be added when balance
patches modify skill/trait coefficients. The infrastructure supports per-entity,
per-field overrides with evidence levels and source citations. Mode isolation verified
(WvW lookup never falls back to PvE — see P3-12 audit).

### 8. patch_ledger (data/patch_ledgers/2026-01-13.yaml)

| Metric | Value |
|--------|-------|
| Total entries | 1 ledger |
| Total changes | 0 (baseline has no changes) |

**Assessment**: Empty baseline by design. Ledger entries will be added when balance
patches are recorded. Each change entry requires a non-empty source URL. The ledger's
`patch_id` is validated against the manifest set.

### 9. normalized_effects (data/normalized_effects/2026-01-13/{pve,pvp,wvw}.json)

#### PvE (data/normalized_effects/2026-01-13/pve.json)

| Evidence Level | Count | Percentage |
|---------------|-------|------------|
| Factual | 20 | 74% |
| Derived | 1 (Sigil of Fire proc) | 4% |
| Heuristic | 6 (uptime estimates) | 22% |
| Unknown | 0 | 0% |
| **Total** | **27** | **100%** |

All 27 entries have non-empty source citations (wiki URLs).

#### PvP (data/normalized_effects/2026-01-13/pvp.json)

| Evidence Level | Count | Percentage |
|---------------|-------|------------|
| Factual | 4 | 31% |
| Derived | 4 (PvP split values) | 31% |
| Heuristic | 5 (uptime estimates) | 38% |
| Unknown | 0 | 0% |
| **Total** | **13** | **100%** |

All 13 entries have non-empty source citations (wiki URLs).

#### WvW (data/normalized_effects/2026-01-13/wvw.json)

| Evidence Level | Count | Percentage |
|---------------|-------|------------|
| Factual | 6 | 46% |
| Derived | 1 (Sigil of Fire proc) | 8% |
| Heuristic | 6 (uptime estimates) | 46% |
| Unknown | 0 | 0% |
| **Total** | **13** | **100%** |

All 13 entries have non-empty source citations (wiki URLs).

---

## Aggregate Evidence Summary

| Table | Total Entries | Factual | Derived | Heuristic | Unknown |
|-------|--------------|---------|---------|-----------|---------|
| profession_profiles | 9 | 9 | 0 | 0 | 0 |
| universal_formulas | 1 | 1 | 0 | 0 | 0 |
| boon_formulas | 13 | 13 | 0 | 0 | 0 |
| condition_formulas | 15 | 15 | 0 | 0 | 0 |
| slot_budgets | 36 | 36 | 0 | 0 | 0 |
| patch_manifests | 1 | -- | -- | -- | -- |
| balance_overrides | 0 | 0 | 0 | 0 | 0 |
| patch_ledger | 0 | 0 | 0 | 0 | 0 |
| normalized_effects (PvE) | 27 | 20 | 1 | 6 | 0 |
| normalized_effects (PvP) | 13 | 4 | 4 | 5 | 0 |
| normalized_effects (WvW) | 13 | 6 | 1 | 6 | 0 |
| **Total** | **128** | **104** | **6** | **17** | **0** |

**Overall**: 81% Factual, 5% Derived, 13% Heuristic, 0% Unknown.

All Factual and Derived entries have source citations. All Heuristic entries also
have source citations documenting the wiki page for the entity being estimated.

---

## Deferred Phase C Tables

The following tables are planned for Phase C stories and do not yet exist in the
data layer. They are documented here for completeness.

### rotation_profiles (P3-14)

- **Purpose**: Typed rotation profiles per elite spec, defining skill sequences,
  cooldown rotations, and expected DPS output
- **Evidence level expectation**: Primarily Heuristic (rotation optimization is
  empirical) with Factual skill cooldowns/coefficients
- **Status**: Deferred to P3-14

### objective_profiles (P3-15)

- **Purpose**: Typed objective profiles with 6-axis scorer (power, condition,
  boon_support, healing, sustain, control) and 3 typed priority maps
  (boon_priorities, condition_priorities, interaction_priorities)
- **Evidence level expectation**: Primarily Heuristic (game mode and encounter
  objective weighting is subjective)
- **Status**: Deferred to P3-15

### scoring_rules (P3-15)

- **Purpose**: Configurable scoring rule sets that map optimization weights to
  the 6-axis objective model
- **Evidence level expectation**: Primarily Heuristic (scoring tuning constants
  like STRIKE_DPS_NORM=3000 are empirically derived)
- **Status**: Deferred to P3-15

---

## P3-12 WvW Audit Integration

The P3-12 WvW Non-Fallback Audit (see `docs/reports/p3-12-wvw-non-fallback-audit.md`)
audited 29 mode-sensitive computation paths and found:

| Classification | Count | Description |
|---------------|-------|-------------|
| **(a) Uses WvW-specific data** | 11 | Fury, Torment, Confusion formulas; override lookup; engine dispatch; context propagation |
| **(b) Mode-invariant base data** | 16 | Universal formulas, stat calculations, gear formulas, duration formulas, Might/Protection/Resolution/Vulnerability |
| **(c) Known split, WvW unresolved** | 0 | None -- all known splits handled in Phase A |
| **(d) Uncertain split status** | 2 | `condition_weights_for_profession()`, `default_buff_profiles()` |

**Key findings integrated**:

1. **No silent PvE fallback paths**: The balance_overrides lookup is strictly keyed by
   `(patch_id, mode)`. WvW lookup returns `None` (use base value), never PvE data.

2. **All known mode splits resolved**: Fury (25% vs 20%), Torment (PvE vs PvP/WvW
   coefficients), and Confusion (PvE vs PvP/WvW coefficients) are all handled in
   Phase A boon/condition formula data files with per-mode entries.

3. **Two uncertain paths flagged for Phase C**:
   - `condition_weights_for_profession()` in combat.rs: Currently uses PvE-oriented
     presets for all modes. WvW may warrant different condition weight distributions.
     Flagged for P3-14 (rotation_profiles).
   - `default_buff_profiles()` in combat.rs: Returns same Solo/Party/Full Squad buff
     assumptions regardless of mode. WvW encounters may have different expected buff
     uptime. Flagged for P3-14.

4. **Quality degradation infrastructure ready**: `check_wvw_quality()` returns
   `(DataQuality, Vec<DataQualityReason>)`. Currently returns `Verified` because
   all known splits are handled. Will activate when Phase C adds trait/skill splits
   with `handled_in_phase_a: false`.

---

## Non-Blocking Findings

### Evidence Upgrade Opportunities

1. **Normalized effects uptime estimates**: 17 entries across all modes use Heuristic
   evidence for uptime modeling (e.g., Phalanx Strength 60% uptime, Relic of the Thief
   85% uptime). These could be upgraded to Derived once P3-14 rotation profiles provide
   computed uptime values from skill timing analysis.

2. **PvP normalized effects**: 4 PvP entries are classified Derived (PvP split values
   for Sigil of Force, Sigil of Bursting, Sigil of Fire, Rune of Scholar bonus). These
   are reasonable Derived classifications since the PvP-specific values come from
   ArenaNet's split-balance system, but the exact mechanism of derivation differs from
   formula-based derivation. Consider documenting the derivation basis more explicitly.

### Coverage Gaps

1. **IncomingConditionMultiplier**: No entries in any mode. Need Resolution-like trait
   examples (e.g., traits that reduce incoming condition damage). Non-blocking because
   Resolution itself is handled in the boon_formulas system.

2. **StealsBoon**: No entries in any mode. Need Mesmer/Thief boon-theft trait examples.
   Non-blocking because boon theft is a niche interaction.

3. **PvP/WvW category coverage**: Several categories have entries only in PvE (StatConversion,
   SpecificConditionDamagePct, CritDamagePct, BoonDurationPct, ConditionDurationPct,
   SpecificConditionDurationPct, OutgoingHealingPct, IncomingStrikeMultiplier,
   ConvertsConditionToBoon, TransfersCondition, DefianceDamage). These PvE-only entries
   are non-blocking for the type system validation but will need PvP/WvW variants for
   complete mode coverage. Tracked in `docs/reports/p3-10b-effect-coverage.md`.

4. **Balance overrides and patch ledger**: Both are empty baselines. This is by design
   (no balance patch has occurred since the baseline snapshot). Will populate when the
   first post-baseline patch arrives.

### Data File Integrity

- **No Factual entries lack source citations**: All Factual entries across all data files
  have non-empty source fields with wiki URLs or API references.
- **No entries have missing evidence_level**: Serde deserialization enforces the field
  is present. The consistency tests additionally verify this at runtime.
- **No downgrade actions needed**: No Factual entries were found without sources, so no
  evidence level downgrades are required.

---

## Consistency Test Summary

22 cross-file consistency tests were added in `crates/optimizer/src/data/consistency_tests.rs`:

| Test | Validates |
|------|----------|
| `test_all_canonical_professions_exist` | All 9 professions in profession_profiles |
| `test_all_profiles_have_positive_health_and_defense` | Non-zero health/defense for all professions |
| `test_balance_override_patch_ids_exist_in_manifests` | Override patch_ids match manifests |
| `test_normalized_effects_patch_ids_exist_in_manifests` | Effect patch_ids match manifests |
| `test_patch_ledger_ids_exist_in_manifests` | Ledger patch_ids match manifests |
| `test_normalized_effects_have_valid_categories` | All effect categories deserialize correctly |
| `test_normalized_effects_modes_are_valid` | Effects exist for all 3 valid modes |
| `test_status_operation_categories_have_payloads` | Status operation categories have payloads |
| `test_triggered_effects_have_inner_category` | TriggeredEffect entries have inner_category |
| `test_profession_profiles_factual_have_sources` | Factual profiles cite sources |
| `test_universal_formulas_factual_have_sources` | Factual formulas cite sources |
| `test_slot_budgets_evidence_levels_present` | All 36 slot budget entries are Factual |
| `test_normalized_effects_factual_have_sources` | Factual effects cite sources |
| `test_normalized_effects_derived_have_sources` | Derived effects cite sources |
| `test_normalized_effects_heuristic_have_sources` | Heuristic effects cite sources |
| `test_manifests_have_source_citations` | Manifests have non-empty source URLs |
| `test_patch_ledger_factual_changes_have_sources` | Factual ledger changes cite sources |
| `test_effect_ids_unique_within_mode` | No duplicate effect_ids within a mode file |
| `test_manifest_supported_modes_cover_data_files` | Active manifest lists all data file modes |
| `test_evidence_level_distribution_normalized_effects` | All evidence levels are valid enum variants |
| `test_uptime_model_evidence_level_consistency` | Estimated uptime implies Heuristic; Passive has no ICD |
| `test_full_data_initialization_returns_ready` | Full data pipeline returns DataState::Ready |

All 22 tests pass. Total test suite: 422 tests, 0 failures.
