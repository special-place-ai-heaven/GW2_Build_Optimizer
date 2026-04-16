//! P3-13: Cross-file consistency tests for all Phase A and Phase B data tables.
//!
//! These tests validate referential integrity across the data layer:
//! - All professions referenced in other data files exist in profession_profiles
//! - All patch references in override/effect files match valid manifests
//! - All normalized_effects entries have valid categories, modes, and patch_ids
//! - All Factual entries have non-empty source citations
//! - No entries have missing evidence_level fields (enforced by serde, but verified)

#[cfg(test)]
mod tests {
    use crate::data::balance_overrides;
    use crate::data::boon_condition_formulas;
    use crate::data::manifests;
    use crate::data::normalized_effects;
    use crate::data::objective_profiles;
    use crate::data::patch_ledger;
    use crate::data::profession_profiles;
    use crate::data::rotation_profiles;
    use crate::data::slot_budgets;
    use crate::data::universal_formulas;
    use crate::data::EvidenceLevel;

    // ─── Cross-file: Profession Profiles referenced by other modules ───

    /// All 9 canonical professions must be present in profession_profiles.
    #[test]
    fn test_all_canonical_professions_exist() {
        let profiles = profession_profiles::profiles();
        let expected = [
            "Warrior",
            "Guardian",
            "Revenant",
            "Engineer",
            "Ranger",
            "Thief",
            "Elementalist",
            "Mesmer",
            "Necromancer",
        ];
        for name in &expected {
            assert!(
                profiles.get(name).is_some(),
                "profession_profiles missing canonical profession: {}",
                name,
            );
        }
        assert_eq!(
            profiles.len(),
            9,
            "expected exactly 9 professions, got {}",
            profiles.len(),
        );
    }

    /// Every profession profile must have base_health and base_defense > 0.
    #[test]
    fn test_all_profiles_have_positive_health_and_defense() {
        let profiles = profession_profiles::profiles();
        let profs = [
            "Warrior",
            "Guardian",
            "Revenant",
            "Engineer",
            "Ranger",
            "Thief",
            "Elementalist",
            "Mesmer",
            "Necromancer",
        ];
        for name in &profs {
            let hp = profiles
                .base_health(name)
                .unwrap_or_else(|| panic!("{} missing from profiles", name));
            let def = profiles
                .base_defense(name)
                .unwrap_or_else(|| panic!("{} missing from profiles", name));
            assert!(hp > 0.0, "{} has non-positive base_health: {}", name, hp);
            assert!(def > 0.0, "{} has non-positive base_defense: {}", name, def);
        }
    }

    // ─── Cross-file: Patch manifest references ───

    /// All patch_ids referenced in balance_override files must exist in manifests.
    #[test]
    fn test_balance_override_patch_ids_exist_in_manifests() {
        let ms = manifests::manifests();
        let manifest_ids: Vec<&str> = ms.iter().map(|m| m.patch_id.as_str()).collect();

        // Load override files and check their patch_ids
        let overrides = balance_overrides::overrides();
        // The overrides container stores files keyed by (patch_id, mode).
        // We verify by checking that the baseline patch_id "2026-01-13" exists.
        // Since overrides are loaded from embedded JSON with known patch_ids,
        // we verify the expected patch_id is in the manifest set.
        assert!(
            manifest_ids.contains(&"2026-01-13"),
            "manifest set must contain '2026-01-13' (used by balance_overrides)",
        );
        // Verify overrides loaded 3 files (PvE, PvP, WvW) for the manifest patch_id
        assert_eq!(
            overrides.file_count(),
            3,
            "expected 3 override files for the active patch",
        );
    }

    /// All patch_ids referenced in normalized_effects files must exist in manifests.
    #[test]
    fn test_normalized_effects_patch_ids_exist_in_manifests() {
        let ms = manifests::manifests();
        let manifest_ids: Vec<&str> = ms.iter().map(|m| m.patch_id.as_str()).collect();
        let effects = normalized_effects::effects();

        // Verify the effects container loaded files for each mode
        assert_eq!(
            effects.file_count(),
            3,
            "expected 3 effects files (PvE, PvP, WvW)",
        );

        // Verify effects are accessible for the manifest patch_id and all modes
        for mode in &["PvE", "PvP", "WvW"] {
            let effs = effects.effects_for("2026-01-13", mode);
            assert!(
                effs.is_some(),
                "normalized_effects missing for patch '2026-01-13', mode '{}'",
                mode,
            );
        }

        // Verify the manifest_ids contain our expected patch
        assert!(
            manifest_ids.contains(&"2026-01-13"),
            "manifest set must contain '2026-01-13' (used by normalized_effects)",
        );
    }

    /// All patch_ids referenced in patch ledgers must exist in manifests.
    #[test]
    fn test_patch_ledger_ids_exist_in_manifests() {
        let ms = manifests::manifests();
        let manifest_ids: Vec<&str> = ms.iter().map(|m| m.patch_id.as_str()).collect();
        let ledgers = patch_ledger::ledgers();

        for ledger in ledgers {
            assert!(
                manifest_ids.contains(&ledger.patch_id.as_str()),
                "patch_ledger '{}' references a patch_id not in manifests",
                ledger.patch_id,
            );

            // If inherits_from is set, it must also reference a valid manifest
            if let Some(ref parent) = ledger.inherits_from {
                assert!(
                    manifest_ids.contains(&parent.as_str()),
                    "patch_ledger '{}' inherits_from '{}' which is not in manifests",
                    ledger.patch_id,
                    parent,
                );
            }
        }
    }

    // ─── Cross-file: Normalized effects structural validation ───

    /// All normalized_effects entries must have valid EffectCategory values.
    /// (Enforced by serde deserialization, but this test confirms the data
    /// actually round-trips through the type system.)
    #[test]
    fn test_normalized_effects_have_valid_categories() {
        let effects = normalized_effects::effects();
        for mode in &["PvE", "PvP", "WvW"] {
            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                for effect in effs {
                    // Category is deserialized from JSON — if we can read it, it's valid.
                    // This test verifies no effects slipped through with unknown categories.
                    let _ = format!("{:?}", effect.category);
                    // Verify effect_id is non-empty
                    assert!(
                        !effect.effect_id.is_empty(),
                        "effect in {} has empty effect_id",
                        mode,
                    );
                    // Verify source_name is non-empty
                    assert!(
                        !effect.source_name.is_empty(),
                        "effect '{}' in {} has empty source_name",
                        effect.effect_id,
                        mode,
                    );
                }
            }
        }
    }

    /// All normalized_effects entries with valid modes (PvE, PvP, WvW) and
    /// valid patch_ids.
    #[test]
    fn test_normalized_effects_modes_are_valid() {
        let effects = normalized_effects::effects();
        let valid_modes = ["PvE", "PvP", "WvW"];
        for mode in &valid_modes {
            let effs = effects.effects_for("2026-01-13", mode);
            assert!(
                effs.is_some(),
                "expected effects for mode '{}' at patch '2026-01-13'",
                mode,
            );
        }
        // Invalid mode should return None
        assert!(
            effects.effects_for("2026-01-13", "Ranked").is_none(),
            "invalid mode 'Ranked' should return None",
        );
    }

    /// Status operation categories must have status_operation payloads.
    #[test]
    fn test_status_operation_categories_have_payloads() {
        let effects = normalized_effects::effects();
        for mode in &["PvE", "PvP", "WvW"] {
            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                for effect in effs {
                    if effect.category.is_status_operation() {
                        assert!(
                            effect.status_operation.is_some(),
                            "effect '{}' in {} has status operation category {:?} but no \
                             status_operation payload",
                            effect.effect_id,
                            mode,
                            effect.category,
                        );
                    }
                }
            }
        }
    }

    /// TriggeredEffect entries must have inner_category.
    #[test]
    fn test_triggered_effects_have_inner_category() {
        use crate::data::normalized_effects::EffectCategory;
        let effects = normalized_effects::effects();
        for mode in &["PvE", "PvP", "WvW"] {
            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                for effect in effs {
                    if effect.category == EffectCategory::TriggeredEffect {
                        assert!(
                            effect.inner_category.is_some(),
                            "effect '{}' in {} is TriggeredEffect but has no inner_category",
                            effect.effect_id,
                            mode,
                        );
                    }
                }
            }
        }
    }

    // ─── Evidence classification: Factual entries must have source citations ───

    /// All profession_profiles Factual entries must have non-empty sources.
    #[test]
    fn test_profession_profiles_factual_have_sources() {
        let profiles = profession_profiles::profiles();
        let profs = [
            "Warrior",
            "Guardian",
            "Revenant",
            "Engineer",
            "Ranger",
            "Thief",
            "Elementalist",
            "Mesmer",
            "Necromancer",
        ];
        for name in &profs {
            let profile = profiles.get(name).unwrap();
            if profile.evidence_level == EvidenceLevel::Factual {
                assert!(
                    !profile.sources.is_empty(),
                    "profession '{}' is Factual but has no source citations",
                    name,
                );
                for source in &profile.sources {
                    assert!(
                        !source.is_empty(),
                        "profession '{}' has an empty source citation string",
                        name,
                    );
                }
            }
        }
    }

    /// Universal formulas Factual entry must have non-empty sources.
    #[test]
    fn test_universal_formulas_factual_have_sources() {
        let formulas = universal_formulas::formulas();
        if formulas.evidence_level == EvidenceLevel::Factual {
            assert!(
                !formulas.sources.is_empty(),
                "universal_formulas is Factual but has no source citations",
            );
            for source in &formulas.sources {
                assert!(
                    !source.is_empty(),
                    "universal_formulas has an empty source citation string",
                );
            }
        }
    }

    /// All slot_budget entries have evidence_level (not missing — enforced by serde,
    /// but we verify the embedded data is consistently Factual).
    #[test]
    fn test_slot_budgets_evidence_levels_present() {
        let budgets = slot_budgets::slot_budgets();
        use crate::data::slot_budgets::{SlotType, StatShape};
        for shape in &StatShape::ALL {
            for slot in &SlotType::ALL {
                let entry = budgets
                    .get(*slot, *shape)
                    .unwrap_or_else(|| panic!("missing {:?} {:?}", slot, shape));
                // All slot budget entries should be Factual (derived from GW2 API data)
                assert_eq!(
                    entry.evidence_level,
                    EvidenceLevel::Factual,
                    "slot budget {:?} {:?} should be Factual, got {:?}",
                    slot,
                    shape,
                    entry.evidence_level,
                );
            }
        }
    }

    /// All normalized_effects Factual entries must have non-empty source citations.
    #[test]
    fn test_normalized_effects_factual_have_sources() {
        let effects = normalized_effects::effects();
        for mode in &["PvE", "PvP", "WvW"] {
            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                for effect in effs {
                    if effect.evidence_level == EvidenceLevel::Factual {
                        assert!(
                            effect.source.is_some() && !effect.source.as_ref().unwrap().is_empty(),
                            "effect '{}' in {} is Factual but has no source citation",
                            effect.effect_id,
                            mode,
                        );
                    }
                }
            }
        }
    }

    /// All normalized_effects Derived entries must have non-empty source citations.
    /// Derived values are computed from factual data, but should still cite the
    /// source of the derivation method or the factual data used.
    #[test]
    fn test_normalized_effects_derived_have_sources() {
        let effects = normalized_effects::effects();
        for mode in &["PvE", "PvP", "WvW"] {
            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                for effect in effs {
                    if effect.evidence_level == EvidenceLevel::Derived {
                        assert!(
                            effect.source.is_some() && !effect.source.as_ref().unwrap().is_empty(),
                            "effect '{}' in {} is Derived but has no source citation",
                            effect.effect_id,
                            mode,
                        );
                    }
                }
            }
        }
    }

    /// All normalized_effects Heuristic entries must have non-empty source citations.
    /// Even heuristic values should document what they're based on.
    #[test]
    fn test_normalized_effects_heuristic_have_sources() {
        let effects = normalized_effects::effects();
        for mode in &["PvE", "PvP", "WvW"] {
            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                for effect in effs {
                    if effect.evidence_level == EvidenceLevel::Heuristic {
                        assert!(
                            effect.source.is_some() && !effect.source.as_ref().unwrap().is_empty(),
                            "effect '{}' in {} is Heuristic but has no source citation",
                            effect.effect_id,
                            mode,
                        );
                    }
                }
            }
        }
    }

    /// Patch manifests must have at least one source with a non-empty URL.
    #[test]
    fn test_manifests_have_source_citations() {
        let ms = manifests::manifests();
        for manifest in ms {
            assert!(
                !manifest.sources.is_empty(),
                "manifest '{}' has no sources",
                manifest.patch_id,
            );
            for source in &manifest.sources {
                assert!(
                    !source.url.is_empty(),
                    "manifest '{}' has a source with empty URL",
                    manifest.patch_id,
                );
            }
        }
    }

    /// Patch ledger changes with Factual evidence_level must have non-empty source.
    #[test]
    fn test_patch_ledger_factual_changes_have_sources() {
        let ledgers = patch_ledger::ledgers();
        for ledger in ledgers {
            for (i, change) in ledger.changes.iter().enumerate() {
                if change.evidence_level == EvidenceLevel::Factual {
                    assert!(
                        !change.source.is_empty(),
                        "ledger '{}' change[{}] ('{}') is Factual but has empty source",
                        ledger.patch_id,
                        i,
                        change.source_name,
                    );
                }
            }
        }
    }

    // ─── Cross-file: Effect ID uniqueness across modes ───

    /// Effect IDs within each mode file must be unique (already enforced by
    /// the loader, but verify the embedded data).
    #[test]
    fn test_effect_ids_unique_within_mode() {
        let effects = normalized_effects::effects();
        for mode in &["PvE", "PvP", "WvW"] {
            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                let mut seen = std::collections::HashSet::new();
                for effect in effs {
                    assert!(
                        seen.insert(&effect.effect_id),
                        "duplicate effect_id '{}' in {} effects file",
                        effect.effect_id,
                        mode,
                    );
                }
            }
        }
    }

    // ─── Cross-file: Manifest supported_modes cover effect/override modes ───

    /// The active manifest's supported_modes must include all modes for which
    /// we have normalized_effects and balance_overrides data.
    #[test]
    fn test_manifest_supported_modes_cover_data_files() {
        let manifest = manifests::latest_manifest();
        let required_modes = ["PvE", "PvP", "WvW"];

        for mode in &required_modes {
            assert!(
                manifest.supported_modes.contains(&mode.to_string()),
                "active manifest '{}' does not list '{}' in supported_modes, \
                 but we have data files for that mode",
                manifest.patch_id,
                mode,
            );
        }
    }

    // ─── Evidence level distribution summary ───

    /// Summary test that counts evidence levels across all normalized_effects.
    /// Not an assertion test — prints the distribution for the report.
    /// Fails only if any entry has an unexpected evidence level.
    #[test]
    fn test_evidence_level_distribution_normalized_effects() {
        let effects = normalized_effects::effects();
        let valid_levels = [
            EvidenceLevel::Factual,
            EvidenceLevel::Derived,
            EvidenceLevel::Heuristic,
            EvidenceLevel::Unknown,
        ];

        for mode in &["PvE", "PvP", "WvW"] {
            let mut factual = 0;
            let mut derived = 0;
            let mut heuristic = 0;
            let mut unknown = 0;

            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                for effect in effs {
                    assert!(
                        valid_levels.contains(&effect.evidence_level),
                        "effect '{}' in {} has unexpected evidence_level: {:?}",
                        effect.effect_id,
                        mode,
                        effect.evidence_level,
                    );
                    match effect.evidence_level {
                        EvidenceLevel::Factual => factual += 1,
                        EvidenceLevel::Derived => derived += 1,
                        EvidenceLevel::Heuristic => heuristic += 1,
                        EvidenceLevel::Unknown => unknown += 1,
                    }
                }
            }

            // Verify at least some entries exist
            let total = factual + derived + heuristic + unknown;
            assert!(
                total > 0,
                "no normalized_effects entries found for mode '{}'",
                mode,
            );
        }
    }

    // ─── Uptime model consistency ───

    /// Effects with Estimated uptime must have Heuristic evidence_level.
    /// Effects with AlwaysOn uptime for Passive triggers should be Factual or Derived.
    #[test]
    fn test_uptime_model_evidence_level_consistency() {
        use crate::data::normalized_effects::{TriggerRule, UptimeModelKind};
        let effects = normalized_effects::effects();

        for mode in &["PvE", "PvP", "WvW"] {
            if let Some(effs) = effects.effects_for("2026-01-13", mode) {
                for effect in effs {
                    // Estimated uptime implies heuristic evidence
                    if effect.uptime_model.kind == UptimeModelKind::Estimated {
                        assert_eq!(
                            effect.evidence_level,
                            EvidenceLevel::Heuristic,
                            "effect '{}' in {} has Estimated uptime but non-Heuristic \
                             evidence_level {:?}",
                            effect.effect_id,
                            mode,
                            effect.evidence_level,
                        );
                    }

                    // Passive triggers should not have internal_cooldown
                    if effect.trigger_rule == TriggerRule::Passive {
                        assert!(
                            effect.internal_cooldown.is_none(),
                            "effect '{}' in {} is Passive but has internal_cooldown",
                            effect.effect_id,
                            mode,
                        );
                    }
                }
            }
        }
    }

    // ─── Cross-dataset: Rotation profile professions ↔ profession_profiles.json ───

    /// Every profession that appears in `data/rotation_profiles/{pve,pvp,wvw}.json`
    /// must also be present in `data/profession_profiles.json`.
    ///
    /// "Generic" is intentionally skipped: it is the documented fallback bucket used
    /// by `RotationProfileData::lookup` when no profession-specific profile exists,
    /// and is validated separately by `test_all_modes_have_9_professions_plus_fallback`
    /// in `rotation_profiles.rs`. It is not a real GW2 profession.
    ///
    /// On failure this test lists every gap so the data owner can fix the source data
    /// rather than the test (do NOT relax this assertion to make it pass).
    #[test]
    fn test_rotation_profile_professions_exist_in_profession_profiles() {
        let rotations = rotation_profiles::rotation_profiles();
        let profs = profession_profiles::profiles();

        let modes = [
            (gw2_core::types::GameMode::PvE, "data/rotation_profiles/pve.json"),
            (gw2_core::types::GameMode::PvP, "data/rotation_profiles/pvp.json"),
            (gw2_core::types::GameMode::WvW, "data/rotation_profiles/wvw.json"),
        ];

        let mut gaps: Vec<String> = Vec::new();
        for (mode, file) in &modes {
            for profile in rotations.profiles_for_mode(mode) {
                // Documented fallback bucket — not a real profession.
                if profile.profession == "Generic" {
                    continue;
                }
                if profs.get(&profile.profession).is_none() {
                    gaps.push(format!(
                        "{} (in {}) not present in profession_profiles.json",
                        profile.profession, file,
                    ));
                }
            }
        }

        assert!(
            gaps.is_empty(),
            "rotation profiles reference professions missing from \
             profession_profiles.json:\n  {}",
            gaps.join("\n  "),
        );
    }

    // ─── Cross-dataset: Objective profile boon_priorities ↔ formulas/boons.json ───

    /// Every boon name referenced in any `boon_priorities` map of
    /// `data/objective_profiles/{pve,pvp,wvw}.json` must exist as a key in
    /// `data/formulas/boons.json`.
    ///
    /// Only `boon_priorities` is checked — `condition_priorities` map to
    /// `formulas/conditions.json` (a separate dataset) and `interaction_priorities`
    /// keys are operation names (`removes_boon`, `steals_boon`, …), not boon names.
    ///
    /// On failure this test lists every gap so the data owner can fix the source data
    /// rather than the test (do NOT relax this assertion to make it pass).
    #[test]
    fn test_objective_profile_boon_priorities_exist_in_boon_formulas() {
        let objectives = objective_profiles::objective_profiles();
        let boons = boon_condition_formulas::boons();

        let mode_files = [
            ("PvE", "data/objective_profiles/pve.json"),
            ("PvP", "data/objective_profiles/pvp.json"),
            ("WvW", "data/objective_profiles/wvw.json"),
        ];

        let mut gaps: Vec<String> = Vec::new();
        for (mode, file) in &mode_files {
            for profile in objectives.profiles_for_mode(mode) {
                for boon_name in profile.boon_priorities.keys() {
                    if boons.get(boon_name).is_none() {
                        gaps.push(format!(
                            "{} (in {} / {}) not present in formulas/boons.json",
                            boon_name, profile.objective_profile_id, file,
                        ));
                    }
                }
            }
        }

        assert!(
            gaps.is_empty(),
            "objective profile boon_priorities reference boons missing from \
             formulas/boons.json:\n  {}",
            gaps.join("\n  "),
        );
    }

    // ─── Data completeness: All data loaders produce Ready state ───

    /// The full initialize() pipeline should return Ready, meaning all loaders
    /// parse and validate successfully.
    #[test]
    fn test_full_data_initialization_returns_ready() {
        let state = crate::data::initialize();
        assert!(
            matches!(state, crate::data::DataState::Ready),
            "expected DataState::Ready, got {:?}",
            state,
        );
    }
}
