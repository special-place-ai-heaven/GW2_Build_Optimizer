//! Integration tests for P3-15: Objective Profiles and Typed State-Aware Scorer Isolation.
//!
//! Validates that:
//! - Different objective profiles produce different build rankings
//! - Changing boon priorities changes scoring behavior
//! - Changing condition priorities changes scoring behavior
//! - ObjectiveScorer wires through correctly from profiles to scoring
//! - Backward compatibility (old "disable" field deserializes correctly)

use gw2_optimizer::combat::CombatPerformance;
use gw2_optimizer::data::objective_profiles;
use gw2_optimizer::scoring::{
    score_with_weights, ObjectiveScorer, OptimizationWeights, WEIGHT_BUDGET,
};

// ─── Helpers ───

/// Create a CombatPerformance biased toward strike/power damage.
fn power_dps_perf() -> CombatPerformance {
    CombatPerformance {
        effective_power: 4500.0,
        strike_dps_index: 2800.0,
        condition_ticks: Default::default(),
        condition_dps_index: 200.0,
        total_dps_index: 3000.0,
        healing_power_index: 50.0,
        boon_duration_pct: 10.0,
        condi_duration_pct: 5.0,
        crit_chance: 85.0,
        effective_health: 15000.0,
        damage_reduction_pct: 0.0,
    }
}

/// Create a CombatPerformance biased toward condition damage.
fn condi_dps_perf() -> CombatPerformance {
    CombatPerformance {
        effective_power: 1200.0,
        strike_dps_index: 500.0,
        condition_ticks: Default::default(),
        condition_dps_index: 3200.0,
        total_dps_index: 3700.0,
        healing_power_index: 80.0,
        boon_duration_pct: 15.0,
        condi_duration_pct: 80.0,
        crit_chance: 30.0,
        effective_health: 18000.0,
        damage_reduction_pct: 0.0,
    }
}

/// Create a CombatPerformance biased toward boon support / healing.
fn support_perf() -> CombatPerformance {
    CombatPerformance {
        effective_power: 800.0,
        strike_dps_index: 300.0,
        condition_ticks: Default::default(),
        condition_dps_index: 100.0,
        total_dps_index: 400.0,
        healing_power_index: 1400.0,
        boon_duration_pct: 90.0,
        condi_duration_pct: 20.0,
        crit_chance: 15.0,
        effective_health: 35000.0,
        damage_reduction_pct: 33.0,
    }
}

/// Create a CombatPerformance biased toward tanking / sustain.
fn sustain_perf() -> CombatPerformance {
    CombatPerformance {
        effective_power: 1000.0,
        strike_dps_index: 600.0,
        condition_ticks: Default::default(),
        condition_dps_index: 150.0,
        total_dps_index: 750.0,
        healing_power_index: 200.0,
        boon_duration_pct: 30.0,
        condi_duration_pct: 10.0,
        crit_chance: 25.0,
        effective_health: 48000.0,
        damage_reduction_pct: 33.0,
    }
}

// ─── Profile Loading Tests ───

#[test]
fn test_all_embedded_profiles_load_successfully() {
    let data = objective_profiles::objective_profiles();
    assert!(
        data.files.contains_key("PvE"),
        "PvE profiles must be loaded"
    );
    assert!(
        data.files.contains_key("PvP"),
        "PvP profiles must be loaded"
    );
    assert!(
        data.files.contains_key("WvW"),
        "WvW profiles must be loaded"
    );
}

#[test]
fn test_each_mode_has_a_default_profile() {
    let data = objective_profiles::objective_profiles();
    for mode in &["PvE", "PvP", "WvW"] {
        let default = data.default_for_mode(mode);
        assert!(default.is_some(), "{} must have a default profile", mode);
    }
}

#[test]
fn test_pve_has_expected_profile_count() {
    let data = objective_profiles::objective_profiles();
    let pve = data.profiles_for_mode("PvE");
    assert_eq!(pve.len(), 7, "PvE should have 7 profiles");
}

#[test]
fn test_pvp_has_expected_profile_count() {
    let data = objective_profiles::objective_profiles();
    let pvp = data.profiles_for_mode("PvP");
    assert_eq!(pvp.len(), 5, "PvP should have 5 profiles");
}

#[test]
fn test_wvw_has_expected_profile_count() {
    let data = objective_profiles::objective_profiles();
    let wvw = data.profiles_for_mode("WvW");
    assert_eq!(wvw.len(), 6, "WvW should have 6 profiles");
}

#[test]
fn test_all_profile_ids_unique() {
    let data = objective_profiles::objective_profiles();
    let all = data.all_profiles();
    let mut ids: Vec<&str> = all
        .iter()
        .map(|p| p.objective_profile_id.as_str())
        .collect();
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        total,
        "All profile IDs must be unique across modes"
    );
}

#[test]
fn test_profile_by_id_round_trip() {
    let data = objective_profiles::objective_profiles();
    let all = data.all_profiles();
    for profile in &all {
        let found = data.profile_by_id(&profile.objective_profile_id);
        assert!(
            found.is_some(),
            "profile_by_id should find '{}'",
            profile.objective_profile_id
        );
        assert_eq!(
            found.unwrap().objective_profile_id,
            profile.objective_profile_id
        );
    }
}

// ─── Different Profiles Produce Different Rankings ───

#[test]
fn test_power_profile_prefers_power_build() {
    let data = objective_profiles::objective_profiles();
    let power_profile = data.profile_by_id("PvE_Power_DPS").unwrap();
    let weights = OptimizationWeights {
        power: power_profile.axis_weights.power,
        condition: power_profile.axis_weights.condition,
        boon_support: power_profile.axis_weights.boon_support,
        healing: power_profile.axis_weights.healing,
        sustain: power_profile.axis_weights.sustain,
        control: power_profile.axis_weights.control,
    };

    let power_score = score_with_weights(&power_dps_perf(), &weights);
    let condi_score = score_with_weights(&condi_dps_perf(), &weights);
    let support_score = score_with_weights(&support_perf(), &weights);

    assert!(
        power_score > condi_score,
        "Power DPS profile should rank power build ({:.4}) above condi build ({:.4})",
        power_score,
        condi_score
    );
    assert!(
        power_score > support_score,
        "Power DPS profile should rank power build ({:.4}) above support build ({:.4})",
        power_score,
        support_score
    );
}

#[test]
fn test_condi_profile_prefers_condi_build() {
    let data = objective_profiles::objective_profiles();
    let condi_profile = data.profile_by_id("PvE_Condi_DPS").unwrap();
    let weights = OptimizationWeights {
        power: condi_profile.axis_weights.power,
        condition: condi_profile.axis_weights.condition,
        boon_support: condi_profile.axis_weights.boon_support,
        healing: condi_profile.axis_weights.healing,
        sustain: condi_profile.axis_weights.sustain,
        control: condi_profile.axis_weights.control,
    };

    let power_score = score_with_weights(&power_dps_perf(), &weights);
    let condi_score = score_with_weights(&condi_dps_perf(), &weights);

    assert!(
        condi_score > power_score,
        "Condi DPS profile should rank condi build ({:.4}) above power build ({:.4})",
        condi_score,
        power_score
    );
}

#[test]
fn test_healer_profile_prefers_support_build() {
    let data = objective_profiles::objective_profiles();
    let healer_profile = data.profile_by_id("PvE_Healer").unwrap();
    let weights = OptimizationWeights {
        power: healer_profile.axis_weights.power,
        condition: healer_profile.axis_weights.condition,
        boon_support: healer_profile.axis_weights.boon_support,
        healing: healer_profile.axis_weights.healing,
        sustain: healer_profile.axis_weights.sustain,
        control: healer_profile.axis_weights.control,
    };

    let power_score = score_with_weights(&power_dps_perf(), &weights);
    let support_score = score_with_weights(&support_perf(), &weights);

    assert!(
        support_score > power_score,
        "Healer profile should rank support build ({:.4}) above power build ({:.4})",
        support_score,
        power_score
    );
}

#[test]
fn test_boon_support_profile_prefers_support_build() {
    let data = objective_profiles::objective_profiles();
    let boon_profile = data.profile_by_id("PvE_Boon_Support").unwrap();
    let weights = OptimizationWeights {
        power: boon_profile.axis_weights.power,
        condition: boon_profile.axis_weights.condition,
        boon_support: boon_profile.axis_weights.boon_support,
        healing: boon_profile.axis_weights.healing,
        sustain: boon_profile.axis_weights.sustain,
        control: boon_profile.axis_weights.control,
    };

    let power_score = score_with_weights(&power_dps_perf(), &weights);
    let support_score = score_with_weights(&support_perf(), &weights);

    assert!(
        support_score > power_score,
        "Boon support profile should rank support build ({:.4}) above power build ({:.4})",
        support_score,
        power_score
    );
}

// ─── PvP profiles produce different rankings than PvE ───

#[test]
fn test_pvp_sustain_profile_prefers_sustain_build() {
    let data = objective_profiles::objective_profiles();
    let pvp_sustain = data.profile_by_id("PvP_Sustain").unwrap();
    let weights = OptimizationWeights {
        power: pvp_sustain.axis_weights.power,
        condition: pvp_sustain.axis_weights.condition,
        boon_support: pvp_sustain.axis_weights.boon_support,
        healing: pvp_sustain.axis_weights.healing,
        sustain: pvp_sustain.axis_weights.sustain,
        control: pvp_sustain.axis_weights.control,
    };

    let sustain_score = score_with_weights(&sustain_perf(), &weights);
    let power_score = score_with_weights(&power_dps_perf(), &weights);

    assert!(
        sustain_score > power_score,
        "PvP Sustain profile should rank sustain build ({:.4}) above power build ({:.4})",
        sustain_score,
        power_score
    );
}

// ─── ObjectiveScorer Wiring Tests ───

#[test]
fn test_objective_scorer_from_mode_produces_valid_scorer() {
    let weights = OptimizationWeights::preset_balanced();
    let scorer = ObjectiveScorer::from_mode(weights.clone(), "PvE");

    assert_eq!(scorer.weight_budget, 2.0);
    assert!(scorer.strike_dps_norm > 0.0);
    assert!(scorer.condi_dps_norm > 0.0);
    assert!(
        !scorer.boon_priorities.is_empty(),
        "PvE should have boon priorities"
    );
    assert!(
        !scorer.condition_priorities.is_empty(),
        "PvE should have condition priorities"
    );
}

#[test]
fn test_objective_scorer_from_profile_uses_profile_norms() {
    let data = objective_profiles::objective_profiles();
    let profile = data.profile_by_id("PvE_Power_DPS").unwrap();
    let weights = OptimizationWeights::preset_power_dps();
    let scorer = ObjectiveScorer::from_profile(weights, profile);

    assert_eq!(
        scorer.strike_dps_norm, profile.normalization_constants.strike_dps_norm,
        "Scorer should use profile's strike_dps_norm"
    );
    assert_eq!(
        scorer.condi_dps_norm, profile.normalization_constants.condi_dps_norm,
        "Scorer should use profile's condi_dps_norm"
    );
}

#[test]
fn test_objective_scorer_fallback_uses_defaults() {
    let weights = OptimizationWeights::preset_balanced();
    let scorer = ObjectiveScorer::fallback(weights);

    assert_eq!(scorer.weight_budget, WEIGHT_BUDGET);
    assert!(
        scorer.boon_priorities.is_empty(),
        "Fallback should have no boon priorities"
    );
    assert!(
        scorer.condition_priorities.is_empty(),
        "Fallback should have no condition priorities"
    );
}

#[test]
fn test_objective_scorer_score_matches_score_with_weights() {
    let weights = OptimizationWeights::preset_power_dps();
    let scorer = ObjectiveScorer::fallback(weights.clone());
    let perf = power_dps_perf();

    let scorer_result = scorer.score(&perf);
    let direct_result = score_with_weights(&perf, &weights);

    assert!(
        (scorer_result - direct_result).abs() < f64::EPSILON,
        "ObjectiveScorer.score() ({:.6}) should match score_with_weights() ({:.6})",
        scorer_result,
        direct_result
    );
}

// ─── Boon Priority Tests ───

#[test]
fn test_boon_priorities_differ_between_profiles() {
    let data = objective_profiles::objective_profiles();
    let power_profile = data.profile_by_id("PvE_Power_DPS").unwrap();
    let healer_profile = data.profile_by_id("PvE_Healer").unwrap();

    let power_scorer =
        ObjectiveScorer::from_profile(OptimizationWeights::preset_power_dps(), power_profile);
    let healer_scorer =
        ObjectiveScorer::from_profile(OptimizationWeights::preset_healer(), healer_profile);

    // Healer should prioritize Regeneration more than power DPS
    let power_regen = power_scorer.boon_priority("Regeneration");
    let healer_regen = healer_scorer.boon_priority("Regeneration");
    assert!(
        healer_regen > power_regen,
        "Healer should prioritize Regeneration ({:.2}) more than Power DPS ({:.2})",
        healer_regen,
        power_regen
    );
}

#[test]
fn test_boon_priority_defaults_when_missing() {
    let weights = OptimizationWeights::preset_balanced();
    let scorer = ObjectiveScorer::fallback(weights);

    // Fallback scorer has no boon priorities, should default to 0.5
    let priority = scorer.boon_priority("Might");
    assert!(
        (priority - 0.5).abs() < f64::EPSILON,
        "Missing boon should default to 0.5, got {}",
        priority
    );
}

// ─── Condition Priority Tests ───

#[test]
fn test_condition_priorities_differ_between_profiles() {
    let data = objective_profiles::objective_profiles();
    let power_profile = data.profile_by_id("PvE_Power_DPS").unwrap();
    let condi_profile = data.profile_by_id("PvE_Condi_DPS").unwrap();

    let power_scorer =
        ObjectiveScorer::from_profile(OptimizationWeights::preset_power_dps(), power_profile);
    let condi_scorer =
        ObjectiveScorer::from_profile(OptimizationWeights::preset_condi_dps(), condi_profile);

    // Condi profile should prioritize Burning higher than power profile
    let power_burning = power_scorer.condition_priority("Burning");
    let condi_burning = condi_scorer.condition_priority("Burning");
    assert!(
        condi_burning > power_burning,
        "Condi profile should prioritize Burning ({:.2}) more than Power profile ({:.2})",
        condi_burning,
        power_burning
    );
}

#[test]
fn test_condition_priority_defaults_when_missing() {
    let weights = OptimizationWeights::preset_balanced();
    let scorer = ObjectiveScorer::fallback(weights);

    let priority = scorer.condition_priority("Bleeding");
    assert!(
        (priority - 0.5).abs() < f64::EPSILON,
        "Missing condition should default to 0.5, got {}",
        priority
    );
}

// ─── Interaction Priority Tests ───

#[test]
fn test_interaction_priorities_loaded() {
    let data = objective_profiles::objective_profiles();
    // WvW Disruptor should have high interaction priorities for boon denial
    let disruptor = data.profile_by_id("WvW_Disruptor").unwrap();
    assert!(
        !disruptor.interaction_priorities.is_empty(),
        "WvW_Disruptor should have interaction priorities"
    );
}

#[test]
fn test_interaction_priority_defaults_when_missing() {
    let weights = OptimizationWeights::preset_balanced();
    let scorer = ObjectiveScorer::fallback(weights);

    let priority = scorer.interaction_priority("removes_boon");
    assert!(
        (priority - 0.5).abs() < f64::EPSILON,
        "Missing interaction should default to 0.5, got {}",
        priority
    );
}

// ─── Backward Compatibility Tests ───

#[test]
fn test_deserialize_old_disable_field_as_control() {
    let json = r#"{
        "power": 0.8,
        "condition": 0.2,
        "healing": 0.1,
        "sustain": 0.3,
        "disable": 0.4
    }"#;
    let weights: OptimizationWeights = serde_json::from_str(json).unwrap();
    assert!(
        (weights.control - 0.4).abs() < f64::EPSILON,
        "Old 'disable' field should deserialize into 'control', got {}",
        weights.control
    );
    assert!(
        (weights.boon_support - 0.0).abs() < f64::EPSILON,
        "Missing 'boon_support' should default to 0.0, got {}",
        weights.boon_support
    );
}

#[test]
fn test_deserialize_new_format_with_all_6_axes() {
    let json = r#"{
        "power": 0.8,
        "condition": 0.2,
        "boon_support": 0.3,
        "healing": 0.1,
        "sustain": 0.3,
        "control": 0.4
    }"#;
    let weights: OptimizationWeights = serde_json::from_str(json).unwrap();
    assert!((weights.power - 0.8).abs() < f64::EPSILON);
    assert!((weights.condition - 0.2).abs() < f64::EPSILON);
    assert!((weights.boon_support - 0.3).abs() < f64::EPSILON);
    assert!((weights.healing - 0.1).abs() < f64::EPSILON);
    assert!((weights.sustain - 0.3).abs() < f64::EPSILON);
    assert!((weights.control - 0.4).abs() < f64::EPSILON);
}

// ─── Cross-Mode Scoring Differentiation ───

#[test]
fn test_pve_and_pvp_default_profiles_produce_different_scores() {
    let data = objective_profiles::objective_profiles();
    let pve_default = data.default_for_mode("PvE").unwrap();
    let pvp_default = data.default_for_mode("PvP").unwrap();

    let pve_weights = OptimizationWeights {
        power: pve_default.axis_weights.power,
        condition: pve_default.axis_weights.condition,
        boon_support: pve_default.axis_weights.boon_support,
        healing: pve_default.axis_weights.healing,
        sustain: pve_default.axis_weights.sustain,
        control: pve_default.axis_weights.control,
    };
    let pvp_weights = OptimizationWeights {
        power: pvp_default.axis_weights.power,
        condition: pvp_default.axis_weights.condition,
        boon_support: pvp_default.axis_weights.boon_support,
        healing: pvp_default.axis_weights.healing,
        sustain: pvp_default.axis_weights.sustain,
        control: pvp_default.axis_weights.control,
    };

    let perf = power_dps_perf();
    let pve_score = score_with_weights(&perf, &pve_weights);
    let pvp_score = score_with_weights(&perf, &pvp_weights);

    // They should differ because the profiles have different weight distributions
    assert!(
        (pve_score - pvp_score).abs() > 0.001,
        "PvE ({:.4}) and PvP ({:.4}) default profiles should score differently for same build",
        pve_score,
        pvp_score
    );
}

// ─── Full Pipeline: Profile -> Scorer -> Score -> Ranking ───

#[test]
fn test_full_pipeline_ranking_with_objective_scorer() {
    let builds = [
        ("Power DPS", power_dps_perf()),
        ("Condi DPS", condi_dps_perf()),
        ("Support", support_perf()),
        ("Sustain", sustain_perf()),
    ];

    // Power DPS profile should rank Power DPS highest
    let power_scorer =
        ObjectiveScorer::from_mode(OptimizationWeights::default_for_mode("PvE"), "PvE");
    let mut power_rankings: Vec<(&str, f64)> = builds
        .iter()
        .map(|(name, perf)| (*name, power_scorer.score(perf)))
        .collect();
    power_rankings.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // With default PvE profile (Power DPS), power build should be ranked highly
    assert_eq!(
        power_rankings[0].0, "Power DPS",
        "Default PvE profile should rank Power DPS first, got: {:?}",
        power_rankings
    );

    // Healer profile should rank Support highest
    let data = objective_profiles::objective_profiles();
    let healer_profile = data.profile_by_id("PvE_Healer").unwrap();
    let healer_weights = OptimizationWeights {
        power: healer_profile.axis_weights.power,
        condition: healer_profile.axis_weights.condition,
        boon_support: healer_profile.axis_weights.boon_support,
        healing: healer_profile.axis_weights.healing,
        sustain: healer_profile.axis_weights.sustain,
        control: healer_profile.axis_weights.control,
    };
    let healer_scorer = ObjectiveScorer::from_mode(healer_weights, "PvE");
    let mut healer_rankings: Vec<(&str, f64)> = builds
        .iter()
        .map(|(name, perf)| (*name, healer_scorer.score(perf)))
        .collect();
    healer_rankings.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    assert_eq!(
        healer_rankings[0].0, "Support",
        "Healer profile should rank Support first, got: {:?}",
        healer_rankings
    );
}

// ─── OptimizationWeights 6-axis API Tests ───

#[test]
fn test_as_array_returns_6_elements() {
    let w = OptimizationWeights::preset_balanced();
    let arr = w.as_array();
    assert_eq!(arr.len(), 6, "as_array() should return 6 elements");
}

#[test]
fn test_set_constrained_respects_budget() {
    let mut w = OptimizationWeights::preset_balanced();
    // Set one axis to maximum
    w.set_constrained(0, 1.0);
    assert!(
        w.total() <= WEIGHT_BUDGET + 0.001,
        "Total ({:.4}) should not exceed budget ({:.1})",
        w.total(),
        WEIGHT_BUDGET
    );
}

#[test]
fn test_default_for_mode_loads_from_profiles() {
    let pve = OptimizationWeights::default_for_mode("PvE");
    let pvp = OptimizationWeights::default_for_mode("PvP");
    let wvw = OptimizationWeights::default_for_mode("WvW");

    // Each mode should produce different default weights
    assert_ne!(
        pve.as_array(),
        pvp.as_array(),
        "PvE and PvP defaults should differ"
    );
    // PvE and WvW may also differ
    let pve_arr = pve.as_array();
    let wvw_arr = wvw.as_array();
    let differ = pve_arr
        .iter()
        .zip(wvw_arr.iter())
        .any(|(a, b)| (a - b).abs() > 0.001);
    assert!(
        differ,
        "PvE and WvW defaults should differ in at least one axis"
    );
}
