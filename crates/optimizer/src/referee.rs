use crate::balance::BalanceContext;
use crate::combat::{self, CombatPerformance, DamageModifiers};
use crate::data::{DataQuality, DataQualityReason};
use crate::engine;
use crate::genome::BuildGenome;
use crate::gamedb::GameDb;
use crate::rotation;
use crate::rotation::SimulationResult;
use crate::scenario::{CombatTier, ScenarioSpec};
use crate::scoring::{score_with_weights, OptimizationWeights};
use crate::stats;
use crate::validation::ValidatedBuild;
use gw2_core::types::GameMode;

// ─── Viability Gate Thresholds ───────────────────────────────────────────────

/// Minimum stunbreak skills required for PvP/WvW viability. // HEURISTIC
const MIN_STUNBREAKS: u32 = 1;

/// Minimum cleanse count (distinct skills with cleanse) for PvP/WvW viability. // HEURISTIC
const MIN_CLEANSE_COUNT: u32 = 1;

/// Minimum effective health for PvE viability (arbitrary baseline). // HEURISTIC
const EHP_FLOOR_PVE: f64 = 5_000.0;

/// Minimum effective health for WvW/PvP viability (higher floor for open-world combat). // HEURISTIC
const EHP_FLOOR_WVW: f64 = 10_000.0;

// ─── Viability Gate Types ────────────────────────────────────────────────────

/// Which gate a `GateResult` describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViabilityGate {
    /// Build must have ≥ MIN_STUNBREAKS stunbreak skills (WvW/PvP only).
    StunbreakCount,
    /// Build must have access to Stability (WvW/PvP only).
    StabilityAccess,
    /// Build must have ≥ MIN_CLEANSE_COUNT cleanse skills (WvW/PvP only).
    CleanseRate,
    /// Build effective health must meet the mode-specific EHP floor.
    EffectiveHealth,
}

/// Result of a single viability gate check.
#[derive(Debug, Clone)]
pub struct GateResult {
    /// Which gate was checked.
    pub gate: ViabilityGate,
    /// Whether the gate passed.
    pub passed: bool,
    /// Human-readable explanation (threshold, actual value, or reason for skip/fail).
    pub note: String,
}

/// Aggregate viability report produced by `evaluate_viability_gates`.
///
/// A build is viable if all gates pass. A single failure marks the build
/// non-viable and the referee assigns it a sentinel score of -1.0.
#[derive(Debug, Clone)]
pub struct ViabilityReport {
    /// Results for each gate that was evaluated.
    pub gates: Vec<GateResult>,
    /// Whether all evaluated gates passed.
    pub is_viable: bool,
}

impl ViabilityReport {
    /// Returns the first failing gate, if any.
    pub fn first_failure(&self) -> Option<&GateResult> {
        self.gates.iter().find(|g| !g.passed)
    }
}

/// Evaluate all mode-appropriate viability gates for a build.
///
/// - WvW/PvP: StunbreakCount, StabilityAccess, CleanseRate, EffectiveHealth
/// - PvE: EffectiveHealth only
///
/// When `rotation` is `None` (simulation unavailable), rotation-dependent
/// gates (StunbreakCount, StabilityAccess, CleanseRate) fail with
/// `note = "rotation unavailable"` in WvW/PvP. They are skipped entirely for PvE.
pub fn evaluate_viability_gates(
    rotation: Option<&SimulationResult>,
    combat_perf: &CombatPerformance,
    scenario: &ScenarioSpec,
) -> ViabilityReport {
    let mut gates: Vec<GateResult> = Vec::new();

    let requires_pvp_gates = matches!(scenario.game_mode, GameMode::WvW | GameMode::PvP);

    if requires_pvp_gates {
        // ── Stunbreak gate ──────────────────────────────────────────────────
        gates.push(match rotation {
            Some(rot) => {
                let passed = rot.stunbreak_count >= MIN_STUNBREAKS;
                GateResult {
                    gate: ViabilityGate::StunbreakCount,
                    passed,
                    note: format!(
                        "stunbreak_count={} (required ≥{})",
                        rot.stunbreak_count, MIN_STUNBREAKS
                    ),
                }
            }
            None => GateResult {
                gate: ViabilityGate::StunbreakCount,
                passed: false,
                note: "rotation unavailable".into(),
            },
        });

        // ── Stability gate ──────────────────────────────────────────────────
        gates.push(match rotation {
            Some(rot) => {
                let passed = rot.has_stability;
                GateResult {
                    gate: ViabilityGate::StabilityAccess,
                    passed,
                    note: if passed {
                        "stability available".into()
                    } else {
                        "no stability access".into()
                    },
                }
            }
            None => GateResult {
                gate: ViabilityGate::StabilityAccess,
                passed: false,
                note: "rotation unavailable".into(),
            },
        });

        // ── Cleanse gate ────────────────────────────────────────────────────
        gates.push(match rotation {
            Some(rot) => {
                let passed = rot.cleanse_count >= MIN_CLEANSE_COUNT;
                GateResult {
                    gate: ViabilityGate::CleanseRate,
                    passed,
                    note: format!(
                        "cleanse_count={} (required ≥{})",
                        rot.cleanse_count, MIN_CLEANSE_COUNT
                    ),
                }
            }
            None => GateResult {
                gate: ViabilityGate::CleanseRate,
                passed: false,
                note: "rotation unavailable".into(),
            },
        });
    }

    // ── Effective health gate (always runs) ─────────────────────────────────
    let ehp_floor = if requires_pvp_gates {
        EHP_FLOOR_WVW
    } else {
        EHP_FLOOR_PVE
    };
    let passed = combat_perf.effective_health >= ehp_floor;
    gates.push(GateResult {
        gate: ViabilityGate::EffectiveHealth,
        passed,
        note: format!(
            "effective_health={:.0} (required ≥{:.0})",
            combat_perf.effective_health, ehp_floor
        ),
    });

    let is_viable = gates.iter().all(|g| g.passed);
    ViabilityReport { gates, is_viable }
}

/// Deterministic build evaluation output.
///
/// The referee is the authority. Search strategies and AI advisors may generate
/// candidates, but they do not decide winners; this report does.
#[derive(Debug, Clone)]
pub struct RefereeReport {
    pub genome: BuildGenome,
    pub scenario: ScenarioSpec,
    pub stats: stats::StatBlock,
    pub modifiers: DamageModifiers,
    pub combat_solo: CombatPerformance,
    pub combat_party: CombatPerformance,
    pub combat_squad: CombatPerformance,
    pub primary_combat: CombatPerformance,
    pub rotation: Option<rotation::SimulationResult>,
    pub user_intent_score: f64,
    pub quality: DataQuality,
    pub quality_reasons: Vec<DataQualityReason>,
}

pub fn evaluate_validated_build(
    validated: &ValidatedBuild,
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &ScenarioSpec,
) -> RefereeReport {
    let genome = BuildGenome::from_validated(profession_name, validated);
    let (stats, modifiers) = engine::calculate_validated_stats(validated, db, profession_name, ctx);
    let derived = stats::compute_derived(&stats, profession_name);
    let buff_profiles = combat::buff_profiles_for_profession(profession_name, ctx);
    let condition_weights = combat::condition_weights_for_profession(profession_name, ctx);

    let combat_solo = combat::calculate_combat_performance(
        &stats,
        &derived,
        &modifiers,
        &buff_profiles[0],
        &condition_weights,
        profession_name,
        ctx,
    );
    let combat_party = combat::calculate_combat_performance(
        &stats,
        &derived,
        &modifiers,
        &buff_profiles[1],
        &condition_weights,
        profession_name,
        ctx,
    );
    let combat_squad = combat::calculate_combat_performance(
        &stats,
        &derived,
        &modifiers,
        &buff_profiles[2],
        &condition_weights,
        profession_name,
        ctx,
    );

    let primary_combat = match scenario.combat_tier {
        CombatTier::Solo => combat_solo.clone(),
        CombatTier::Party => combat_party.clone(),
        CombatTier::Squad => combat_squad.clone(),
    };
    let rotation = engine::simulate_validated_rotation(validated, db, &stats);

    let mut quality = DataQuality::Verified;
    let mut quality_reasons = Vec::new();

    if !validated.warnings.is_empty() {
        quality = quality.merge(&DataQuality::Provisional);
        quality_reasons.extend(validated.warnings.iter().map(|warning| DataQualityReason {
            field: "validated_build.warning".into(),
            entity: profession_name.into(),
            modes: vec![ctx.game_mode.label().to_string()],
            explanation: warning.clone(),
        }));
    }

    if !validated.errors.is_empty() {
        quality = quality.merge(&DataQuality::Blocked);
        quality_reasons.extend(validated.errors.iter().map(|error| DataQualityReason {
            field: "validated_build.error".into(),
            entity: profession_name.into(),
            modes: vec![ctx.game_mode.label().to_string()],
            explanation: error.clone(),
        }));
    }

    RefereeReport {
        genome,
        scenario: scenario.clone(),
        stats,
        modifiers,
        combat_solo,
        combat_party,
        combat_squad,
        primary_combat: primary_combat.clone(),
        rotation,
        user_intent_score: score_with_weights(&primary_combat, weights),
        quality,
        quality_reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_validated_build, evaluate_viability_gates, GateResult, ViabilityGate,
        EHP_FLOOR_PVE, EHP_FLOOR_WVW,
    };
    use crate::balance::BalanceContext;
    use crate::combat::CombatPerformance;
    use crate::data::DataQuality;
    use crate::gamedb::GameDb;
    use crate::rotation::SimulationResult;
    use crate::scenario::{CombatTier, ScenarioSpec, TargetProfile, OptimizationTarget};
    use crate::scoring::OptimizationWeights;
    use crate::validation::{
        ValidatedBuild, ValidatedGearPrefix, ValidatedSkills, ValidatedSpec, ValidatedWeaponSet,
        ValidatedWeapons,
    };
    use gw2_core::types::GameMode;
    use std::collections::HashMap;

    // ─── Gate test helpers ────────────────────────────────────────────────

    /// A `SimulationResult` that satisfies all WvW/PvP gates.
    fn make_viable_rotation() -> SimulationResult {
        SimulationResult {
            duration_ms: 20_000,
            strike_dps: 5_000.0,
            condition_dps: 1_000.0,
            total_dps: 6_000.0,
            condition_uptime: HashMap::new(),
            buff_uptime: HashMap::new(),
            skill_usage: vec![],
            stunbreak_count: 2,
            has_stability: true,
            stability_uptime: 0.6,
            cleanse_count: 2,
            cleanse_rate_per_20s: 4.0,
        }
    }

    /// A `CombatPerformance` with sufficient effective health for WvW.
    fn make_viable_combat() -> CombatPerformance {
        CombatPerformance {
            effective_health: EHP_FLOOR_WVW + 1_000.0,
            ..CombatPerformance::default()
        }
    }

    fn make_wvw_scenario() -> ScenarioSpec {
        ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: CombatTier::Squad,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget { label: "WvW".into() },
            patch_id: None,
        }
    }

    fn make_pve_scenario() -> ScenarioSpec {
        ScenarioSpec {
            game_mode: GameMode::PvE,
            combat_tier: CombatTier::Party,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget { label: "PvE".into() },
            patch_id: None,
        }
    }

    fn gate_by_kind<'a>(gates: &'a [GateResult], kind: &ViabilityGate) -> Option<&'a GateResult> {
        gates.iter().find(|g| &g.gate == kind)
    }

    // ─── Gate scenario tests ──────────────────────────────────────────────

    /// WvW build with all gates satisfied → viable.
    #[test]
    fn gate_wvw_all_pass_is_viable() {
        let rot = make_viable_rotation();
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);

        assert!(report.is_viable, "expected viable; gates: {:?}", report.gates);
        assert_eq!(report.gates.len(), 4); // stunbreak + stability + cleanse + ehp
        for g in &report.gates {
            assert!(g.passed, "gate {:?} should pass but failed: {}", g.gate, g.note);
        }
    }

    /// WvW build missing stunbreak → non-viable, stunbreak gate fails.
    #[test]
    fn gate_wvw_no_stunbreak_fails() {
        let mut rot = make_viable_rotation();
        rot.stunbreak_count = 0;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);

        assert!(!report.is_viable);
        let g = gate_by_kind(&report.gates, &ViabilityGate::StunbreakCount).unwrap();
        assert!(!g.passed);
    }

    /// WvW build missing stability → non-viable, stability gate fails.
    #[test]
    fn gate_wvw_no_stability_fails() {
        let mut rot = make_viable_rotation();
        rot.has_stability = false;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);

        assert!(!report.is_viable);
        let g = gate_by_kind(&report.gates, &ViabilityGate::StabilityAccess).unwrap();
        assert!(!g.passed);
        assert_eq!(g.note, "no stability access");
    }

    /// WvW build with no cleanse skills → non-viable, cleanse gate fails.
    #[test]
    fn gate_wvw_no_cleanse_fails() {
        let mut rot = make_viable_rotation();
        rot.cleanse_count = 0;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);

        assert!(!report.is_viable);
        let g = gate_by_kind(&report.gates, &ViabilityGate::CleanseRate).unwrap();
        assert!(!g.passed);
    }

    /// PvE build has no stunbreak/stability/cleanse gates; only EHP gate.
    #[test]
    fn gate_pve_only_ehp_gate_runs() {
        let rot = make_viable_rotation(); // has all PvP flags set
        let mut combat = make_viable_combat();
        combat.effective_health = EHP_FLOOR_PVE + 1_000.0;
        let scenario = make_pve_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);

        // Only EHP gate should be present
        assert_eq!(report.gates.len(), 1);
        assert_eq!(report.gates[0].gate, ViabilityGate::EffectiveHealth);
        assert!(report.is_viable);

        // Rotation-based gates are not present
        assert!(gate_by_kind(&report.gates, &ViabilityGate::StunbreakCount).is_none());
        assert!(gate_by_kind(&report.gates, &ViabilityGate::StabilityAccess).is_none());
        assert!(gate_by_kind(&report.gates, &ViabilityGate::CleanseRate).is_none());
    }

    /// WvW build with `rotation = None` → rotation-dependent gates fail with "rotation unavailable".
    #[test]
    fn gate_wvw_rotation_none_rotation_gates_fail_gracefully() {
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(None, &combat, &scenario);

        // Should have 4 gates; the first 3 must fail with "rotation unavailable"
        assert_eq!(report.gates.len(), 4);
        assert!(!report.is_viable);

        let sb = gate_by_kind(&report.gates, &ViabilityGate::StunbreakCount).unwrap();
        assert!(!sb.passed);
        assert_eq!(sb.note, "rotation unavailable");

        let stab = gate_by_kind(&report.gates, &ViabilityGate::StabilityAccess).unwrap();
        assert!(!stab.passed);
        assert_eq!(stab.note, "rotation unavailable");

        let cl = gate_by_kind(&report.gates, &ViabilityGate::CleanseRate).unwrap();
        assert!(!cl.passed);
        assert_eq!(cl.note, "rotation unavailable");

        // EHP gate still runs and passes (viable combat)
        let ehp = gate_by_kind(&report.gates, &ViabilityGate::EffectiveHealth).unwrap();
        assert!(ehp.passed);
    }

    /// PvE build with `rotation = None` → only EHP gate, still viable if EHP passes.
    #[test]
    fn gate_pve_rotation_none_only_ehp_gate() {
        let mut combat = make_viable_combat();
        combat.effective_health = EHP_FLOOR_PVE + 500.0;
        let scenario = make_pve_scenario();
        let report = evaluate_viability_gates(None, &combat, &scenario);

        assert_eq!(report.gates.len(), 1);
        assert!(report.is_viable);
        assert_eq!(report.gates[0].gate, ViabilityGate::EffectiveHealth);
    }

    /// EHP gate uses the WvW floor for WvW scenarios.
    #[test]
    fn gate_wvw_ehp_below_wvw_floor_fails() {
        let rot = make_viable_rotation();
        let mut combat = make_viable_combat();
        combat.effective_health = EHP_FLOOR_WVW - 1.0; // just below WvW floor
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);

        assert!(!report.is_viable);
        let ehp = gate_by_kind(&report.gates, &ViabilityGate::EffectiveHealth).unwrap();
        assert!(!ehp.passed);
    }

    /// EHP gate uses the PvE floor (lower) for PvE scenarios.
    #[test]
    fn gate_pve_ehp_above_pve_floor_passes() {
        let combat_passes = CombatPerformance {
            effective_health: EHP_FLOOR_PVE + 1.0,
            ..CombatPerformance::default()
        };
        let scenario = make_pve_scenario();
        let report = evaluate_viability_gates(None, &combat_passes, &scenario);
        assert!(report.is_viable);
    }

    fn make_test_db() -> GameDb {
        GameDb {
            items: Default::default(),
            itemstats: Default::default(),
            skills: Default::default(),
            traits: Default::default(),
            specializations: Default::default(),
            professions: Default::default(),
            legends: Default::default(),
            pvp_amulets: Default::default(),
            skills_by_profession: Default::default(),
            traits_by_spec: Default::default(),
            items_by_type: Default::default(),
            runes: Default::default(),
            sigils: Default::default(),
            relics: Default::default(),
            skill_to_palette: Default::default(),
            palette_to_skill: Default::default(),
            traits_by_condition: Default::default(),
            skills_by_condition: Default::default(),
            traits_by_buff: Default::default(),
            skills_by_buff: Default::default(),
        }
    }

    fn make_minimal_validated() -> ValidatedBuild {
        ValidatedBuild {
            specializations: vec![
                ValidatedSpec {
                    spec_id: 1,
                    name: "Spec A".into(),
                    elite: false,
                    trait_ids: vec![],
                    trait_names: vec![],
                    all_trait_ids: vec![],
                },
                ValidatedSpec {
                    spec_id: 2,
                    name: "Spec B".into(),
                    elite: false,
                    trait_ids: vec![],
                    trait_names: vec![],
                    all_trait_ids: vec![],
                },
                ValidatedSpec {
                    spec_id: 3,
                    name: "Spec C".into(),
                    elite: true,
                    trait_ids: vec![],
                    trait_names: vec![],
                    all_trait_ids: vec![],
                },
            ],
            weapons: ValidatedWeapons {
                set1: ValidatedWeaponSet {
                    main_hand: None,
                    off_hand: None,
                },
                set2: ValidatedWeaponSet {
                    main_hand: None,
                    off_hand: None,
                },
            },
            skills: ValidatedSkills {
                heal: None,
                utilities: vec![],
                elite: None,
            },
            rune: None,
            sigils: vec![],
            relic: None,
            gear_prefix: Some(ValidatedGearPrefix {
                itemstat_id: 9999,
                name: "Test".into(),
            }),
            explanation: String::new(),
            synergy_explanation: String::new(),
            changes: vec![],
            warnings: vec![],
            errors: vec![],
        }
    }

    #[test]
    fn referee_evaluation_is_deterministic_for_same_inputs() {
        let db = make_test_db();
        let validated = make_minimal_validated();
        let ctx = BalanceContext::new(GameMode::PvE);
        let mut scenario = ScenarioSpec::from_balance_context(&ctx);
        scenario.combat_tier = CombatTier::Party;
        let weights = OptimizationWeights::default_for_mode(GameMode::PvE.label());

        let report_a =
            evaluate_validated_build(&validated, &db, "Guardian", &weights, &ctx, &scenario);
        let report_b =
            evaluate_validated_build(&validated, &db, "Guardian", &weights, &ctx, &scenario);

        assert_eq!(report_a.genome, report_b.genome);
        assert_eq!(report_a.quality, DataQuality::Verified);
        assert_eq!(report_a.user_intent_score, report_b.user_intent_score);
        assert_eq!(
            report_a.primary_combat.total_dps_index,
            report_b.primary_combat.total_dps_index
        );
    }

    #[test]
    fn referee_marks_build_blocked_when_validation_has_errors() {
        let db = make_test_db();
        let mut validated = make_minimal_validated();
        validated.errors.push("illegal weapon".into());
        let ctx = BalanceContext::new(GameMode::PvE);
        let scenario = ScenarioSpec::from_balance_context(&ctx);
        let weights = OptimizationWeights::default_for_mode(GameMode::PvE.label());

        let report = evaluate_validated_build(&validated, &db, "Guardian", &weights, &ctx, &scenario);

        assert_eq!(report.quality, DataQuality::Blocked);
        assert_eq!(report.quality_reasons.len(), 1);
    }
}
