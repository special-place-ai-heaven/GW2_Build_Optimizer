use crate::balance::BalanceContext;
use crate::combat::{self, CombatPerformance, DamageModifiers};
use crate::data::{DataQuality, DataQualityReason};
use crate::engine;
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

/// Minimum effective health for PvE viability.
/// Evidence: a glass Berserker Guardian (vit~1000, no toughness investment) computes
/// ~18,030 blended EHP (65% strike / 35% condition). A minimal test build with empty
/// gear (~1099) is far below any real build. This floor screens out obviously
/// under-geared or broken builds while passing all real ascended/exotic builds.
const EHP_FLOOR_PVE: f64 = 11_000.0;

/// EHP floor for WvW Roaming / Solo play.
/// Evidence: a Berserker Guardian (marauder variant) with Vitality gear reaches ~20,473
/// blended EHP; a Trailblazer Scourge reaches ~32,542. Glass Berserker Guardian solo
/// is ~18,030. Floor set to 15,000 — below any viable roaming gear set but high enough
/// to reject near-naked builds. Without a healer, you need this floor to survive burst.
pub const EHP_FLOOR_WVW_ROAM: f64 = 15_000.0;

/// EHP floor for WvW Havoc / small group play (5-15 players).
/// Evidence: small groups have a healer or support but you're still frequently 1-targeted.
/// Celestial Ele at ~18,680; glass Warrior at ~24,587. Floor set to 13,000 — accepts any
/// reasonable stat investment while rejecting pure paper builds.
pub const EHP_FLOOR_WVW_HAVOC: f64 = 13_000.0;

/// EHP floor for WvW Zerg / Squad play.
/// Evidence: zerg play has dedicated healers (Minstrel Firebrand ~24,866 EHP pre-healing).
/// Even a glass Berserker Warrior gets ~24,587 and is viable in a zerg. Floor set to 10,000
/// — loose enough that any remotely geared build passes; screens out completely naked builds.
pub const EHP_FLOOR_WVW_ZERG: f64 = 10_000.0;

/// Legacy alias kept for test backward-compatibility. Equals the havoc (party) floor. // HEURISTIC
pub const EHP_FLOOR_WVW: f64 = EHP_FLOOR_WVW_HAVOC;

/// EHP floor for sPvP / structured PvP. PvP uses amulet-based stat allocation
/// with a smaller total stat budget than ascended WvW gear, so EHP at level 80
/// is materially lower. Setting the floor to WvW levels would systematically
/// fail viable PvP builds and score them with the non-viable -1.0 sentinel.
///
/// Evidence: a Marauder amulet on a medium-armor profession with no toughness
/// investment lands ~12-14k blended EHP; tankier amulets (Cleric / Paladin)
/// hover at 18-22k. Floor at 8,000 rejects clearly broken builds while leaving
/// every real amulet/rune combo viable. // HEURISTIC
pub const EHP_FLOOR_PVP: f64 = 8_000.0;

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
    // WvW floor varies by combat tier: Roamers need more personal sustain than Zerg players.
    // PvP uses its own (lower) floor — amulet-based gear has a smaller stat budget than
    // ascended WvW, so reusing WvW floors here would non-viably score most real PvP builds.
    let ehp_floor = match scenario.game_mode {
        GameMode::WvW => match scenario.combat_tier {
            crate::scenario::CombatTier::Solo => EHP_FLOOR_WVW_ROAM,
            crate::scenario::CombatTier::Party => EHP_FLOOR_WVW_HAVOC,
            crate::scenario::CombatTier::Squad => EHP_FLOOR_WVW_ZERG,
        },
        GameMode::PvP => EHP_FLOOR_PVP,
        GameMode::PvE => EHP_FLOOR_PVE,
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
///
/// `viability` captures per-gate pass/fail results. When `viability.is_viable` is
/// `false`, `user_intent_score` is set to the sentinel value `-1.0` and the build
/// should be excluded from rankings.
#[derive(Debug, Clone)]
pub struct RefereeReport {
    pub scenario: ScenarioSpec,
    pub stats: stats::StatBlock,
    pub modifiers: DamageModifiers,
    pub combat_solo: CombatPerformance,
    pub combat_party: CombatPerformance,
    pub combat_squad: CombatPerformance,
    pub primary_combat: CombatPerformance,
    pub rotation: Option<rotation::SimulationResult>,
    /// Structured viability report: per-gate pass/fail with values and notes.
    pub viability: ViabilityReport,
    /// Final score for ranking. Set to `-1.0` (sentinel) when `viability.is_viable` is false.
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

    // ── Viability gating ──────────────────────────────────────────────────────
    // Run before score computation. Non-viable builds receive sentinel score -1.0.
    let viability = evaluate_viability_gates(rotation.as_ref(), &primary_combat, scenario);
    let user_intent_score = if viability.is_viable {
        score_with_weights(&primary_combat, weights)
    } else {
        -1.0
    };

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
            explanation: error.detail.clone(),
        }));
    }

    RefereeReport {
        scenario: scenario.clone(),
        stats,
        modifiers,
        combat_solo,
        combat_party,
        combat_squad,
        primary_combat: primary_combat.clone(),
        rotation,
        viability,
        user_intent_score,
        quality,
        quality_reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_validated_build, evaluate_viability_gates, GateResult, ViabilityGate,
        EHP_FLOOR_PVE, EHP_FLOOR_PVP, EHP_FLOOR_WVW_HAVOC, EHP_FLOOR_WVW_ROAM, EHP_FLOOR_WVW_ZERG,
    };
    use crate::balance::BalanceContext;
    use crate::combat::CombatPerformance;
    use crate::data::DataQuality;
    use crate::gamedb::GameDb;
    use crate::rotation::SimulationResult;
    use crate::scenario::{CombatTier, OptimizationTarget, ScenarioSpec, TargetProfile};
    use crate::scoring::OptimizationWeights;
    use crate::validation::{
        RejectCode, ValidatedBuild, ValidatedGearPrefix, ValidatedSkills, ValidatedSpec,
        ValidatedWeaponSet, ValidatedWeapons, ValidationReject,
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
    /// A `CombatPerformance` with sufficient effective health for all WvW tiers including Roaming.
    fn make_viable_combat() -> CombatPerformance {
        CombatPerformance {
            // Use ROAM floor + buffer so this helper works across all WvW combat tiers.
            effective_health: EHP_FLOOR_WVW_ROAM + 1_000.0,
            ..CombatPerformance::default()
        }
    }

    fn make_wvw_scenario() -> ScenarioSpec {
        ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: CombatTier::Squad,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "WvW".into(),
            },
            patch_id: None,
        }
    }

    fn make_pve_scenario() -> ScenarioSpec {
        ScenarioSpec {
            game_mode: GameMode::PvE,
            combat_tier: CombatTier::Party,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "PvE".into(),
            },
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

        assert!(
            report.is_viable,
            "expected viable; gates: {:?}",
            report.gates
        );
        assert_eq!(report.gates.len(), 4); // stunbreak + stability + cleanse + ehp
        for g in &report.gates {
            assert!(
                g.passed,
                "gate {:?} should pass but failed: {}",
                g.gate, g.note
            );
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

    /// EHP gate uses the WvW floor for WvW scenarios; tier-aware (Squad uses ZERG floor).
    #[test]
    fn gate_wvw_ehp_below_wvw_floor_fails() {
        let rot = make_viable_rotation();
        let mut combat = make_viable_combat();
        // Use a value below the Zerg (Squad) floor to ensure failure at Squad tier
        combat.effective_health = EHP_FLOOR_WVW_ZERG - 1.0;
        // make_wvw_scenario() uses CombatTier::Squad
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

        assert_eq!(report_a.quality, DataQuality::Verified);
        assert_eq!(report_a.user_intent_score, report_b.user_intent_score);
        assert_eq!(
            report_a.primary_combat.total_dps_index,
            report_b.primary_combat.total_dps_index
        );
        // Viability must be deterministic: same is_viable flag across both calls.
        assert_eq!(
            report_a.viability.is_viable, report_b.viability.is_viable,
            "viability gate outcome must be deterministic"
        );
    }

    #[test]
    fn referee_marks_build_blocked_when_validation_has_errors() {
        let db = make_test_db();
        let mut validated = make_minimal_validated();
        validated.errors.push(ValidationReject {
            code: RejectCode::WeaponNotAvailable {
                slot: "Set 1".into(),
                weapon: "illegal".into(),
                profession: "Guardian".into(),
            },
            detail: "illegal weapon".into(),
        });
        let ctx = BalanceContext::new(GameMode::PvE);
        let scenario = ScenarioSpec::from_balance_context(&ctx);
        let weights = OptimizationWeights::default_for_mode(GameMode::PvE.label());

        let report =
            evaluate_validated_build(&validated, &db, "Guardian", &weights, &ctx, &scenario);

        assert_eq!(report.quality, DataQuality::Blocked);
        assert_eq!(report.quality_reasons.len(), 1);
    }

    /// WvW build with a minimal (no-gear) Guardian → rotation=None from empty DB, EHP far below
    /// WvW floor → all rotation gates fail + EHP gate fails → non-viable → sentinel score -1.0.
    /// This tests that RefereeReport.viability is populated and the sentinel is applied end-to-end.
    #[test]
    fn referee_wvw_minimal_build_is_non_viable_sentinel_score() {
        let db = make_test_db();
        let validated = make_minimal_validated();
        let ctx = BalanceContext::new(GameMode::WvW);
        let scenario = ScenarioSpec::from_balance_context(&ctx);
        let weights = OptimizationWeights::default_for_mode(GameMode::WvW.label());

        let report =
            evaluate_validated_build(&validated, &db, "Guardian", &weights, &ctx, &scenario);

        // Minimal Guardian has EHP well below WvW floor and no rotation skills → non-viable.
        assert!(
            !report.viability.is_viable,
            "minimal WvW build should be non-viable; gates: {:?}",
            report.viability.gates
        );
        assert_eq!(
            report.user_intent_score, -1.0,
            "non-viable build must receive sentinel score -1.0"
        );
        // ViabilityReport must be populated (not empty).
        assert!(
            !report.viability.gates.is_empty(),
            "viability.gates must be populated even for non-viable builds"
        );
    }

    /// PvE build with stunbreak_count=0 in the rotation → only EHP gate runs in PvE,
    /// so stunbreak absence does NOT cause non-viability. Tests the gate directly since
    /// evaluate_validated_build cannot inject a custom rotation.
    #[test]
    fn gate_pve_zero_stunbreaks_still_viable_when_ehp_passes() {
        let mut rot = make_viable_rotation();
        rot.stunbreak_count = 0; // zero stunbreaks
        let mut combat = make_viable_combat();
        combat.effective_health = EHP_FLOOR_PVE + 1_000.0; // PvE EHP floor satisfied
        let scenario = make_pve_scenario();

        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);

        // PvE only runs EHP gate — 0 stunbreaks must not cause failure.
        assert!(
            report.is_viable,
            "PvE build with 0 stunbreaks should be viable when EHP passes; gates: {:?}",
            report.gates
        );
        // Confirm rotation gates are absent in PvE.
        assert!(gate_by_kind(&report.gates, &ViabilityGate::StunbreakCount).is_none());
    }

    // ─── CombatTier-differentiated EHP gate tests ──────────────────────────

    /// EHP between Zerg floor and Roam floor: passes Zerg (Squad), fails Roaming (Solo).
    #[test]
    fn gate_wvw_ehp_passes_zerg_but_fails_roam() {
        use crate::scenario::{OptimizationTarget, TargetProfile};
        let rot = make_viable_rotation();
        // EHP is above ZERG floor but below ROAM floor
        let mid_ehp = EHP_FLOOR_WVW_ZERG + 500.0;
        assert!(
            mid_ehp < EHP_FLOOR_WVW_ROAM,
            "test setup: mid_ehp={} must be below ROAM floor={}",
            mid_ehp,
            EHP_FLOOR_WVW_ROAM
        );

        let mut combat = make_viable_combat();
        combat.effective_health = mid_ehp;

        // ── Squad scenario → should pass EHP gate ──
        let squad_scenario = ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: crate::scenario::CombatTier::Squad,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "WvW".into(),
            },
            patch_id: None,
        };
        let report_squad = evaluate_viability_gates(Some(&rot), &combat, &squad_scenario);
        let ehp_squad = gate_by_kind(&report_squad.gates, &ViabilityGate::EffectiveHealth).unwrap();
        assert!(
            ehp_squad.passed,
            "EHP={} should pass Squad floor={}; note='{}'",
            mid_ehp, EHP_FLOOR_WVW_ZERG, ehp_squad.note
        );

        // ── Solo scenario (Roaming) → should fail EHP gate ──
        let solo_scenario = ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: crate::scenario::CombatTier::Solo,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "WvW".into(),
            },
            patch_id: None,
        };
        let report_solo = evaluate_viability_gates(Some(&rot), &combat, &solo_scenario);
        let ehp_solo = gate_by_kind(&report_solo.gates, &ViabilityGate::EffectiveHealth).unwrap();
        assert!(
            !ehp_solo.passed,
            "EHP={} should fail Solo/Roam floor={}; note='{}'",
            mid_ehp, EHP_FLOOR_WVW_ROAM, ehp_solo.note
        );
    }

    /// A fully equipped roaming build (EHP above ROAM floor) passes all WvW gates at Solo tier.
    #[test]
    fn gate_wvw_solo_viable_when_ehp_above_roam_floor() {
        use crate::scenario::{OptimizationTarget, TargetProfile};
        let rot = make_viable_rotation();
        let mut combat = make_viable_combat();
        combat.effective_health = EHP_FLOOR_WVW_ROAM + 1_000.0;

        let solo_scenario = ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: crate::scenario::CombatTier::Solo,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "WvW".into(),
            },
            patch_id: None,
        };
        let report = evaluate_viability_gates(Some(&rot), &combat, &solo_scenario);
        assert!(
            report.is_viable,
            "Build with EHP above ROAM floor and all rotation gates met should be viable; gates: {:?}",
            report.gates
        );
    }

    /// EHP threshold ordering: WvW tiers are correctly graduated; PvE floor is between HAVOC and ZERG.
    /// Rationale: PvE expects real ascended gear with no healer but no CC pressure either;
    /// WvW Zerg accepts lower personal EHP because squad healers cover survival.
    #[test]
    fn ehp_floor_ordering_is_correct() {
        assert!(
            EHP_FLOOR_WVW_ROAM > EHP_FLOOR_WVW_HAVOC,
            "Roam floor should be stricter than Havoc floor"
        );
        assert!(
            EHP_FLOOR_WVW_HAVOC > EHP_FLOOR_WVW_ZERG,
            "Havoc floor should be stricter than Zerg floor"
        );
        // PvE floor sits between Havoc and Zerg — WvW Zerg has squad healers so lower personal EHP
        // is acceptable; PvE lacks those but also lacks the CC pressure that makes EHP critical.
        assert!(
            EHP_FLOOR_WVW_HAVOC > EHP_FLOOR_PVE,
            "Havoc floor should be stricter than PvE floor"
        );
        assert!(
            EHP_FLOOR_PVE > EHP_FLOOR_WVW_ZERG,
            "PvE floor should be stricter than Zerg floor (squad healers compensate)"
        );
        assert!(
            EHP_FLOOR_PVP < EHP_FLOOR_WVW_ZERG,
            "PvP floor should be the loosest — amulet stat budget is smaller than ascended"
        );
    }

    #[test]
    fn gate_pvp_uses_pvp_floor_not_wvw_floor() {
        // PvP build with 9_000 EHP: below WvW Roam (15k) but above EHP_FLOOR_PVP (8k).
        // Must pass on PvP. Before the fix, requires_pvp_gates routed PvP through
        // WvW EHP tiers and the build would non-viably score at -1.0.
        let rot = make_viable_rotation();
        let combat = CombatPerformance {
            effective_health: 9_000.0,
            ..make_viable_combat()
        };
        let pvp_scenario = ScenarioSpec {
            game_mode: GameMode::PvP,
            combat_tier: CombatTier::Solo,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "PvP".to_string(),
            },
            patch_id: None,
        };
        let report = evaluate_viability_gates(Some(&rot), &combat, &pvp_scenario);
        let ehp = gate_by_kind(&report.gates, &ViabilityGate::EffectiveHealth).expect("gate");
        assert!(
            ehp.passed,
            "PvP 9k EHP should pass PvP floor (8k), not be measured against WvW Roam (15k); note: {}",
            ehp.note
        );
    }

    #[test]
    fn gate_pvp_low_ehp_still_fails_pvp_floor() {
        // Sanity check: an unreasonably low PvP EHP (e.g. 5k) still fails the PvP floor.
        let rot = make_viable_rotation();
        let combat = CombatPerformance {
            effective_health: 5_000.0,
            ..make_viable_combat()
        };
        let pvp_scenario = ScenarioSpec {
            game_mode: GameMode::PvP,
            combat_tier: CombatTier::Solo,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "PvP".to_string(),
            },
            patch_id: None,
        };
        let report = evaluate_viability_gates(Some(&rot), &combat, &pvp_scenario);
        let ehp = gate_by_kind(&report.gates, &ViabilityGate::EffectiveHealth).expect("gate");
        assert!(
            !ehp.passed,
            "PvP 5k EHP should fail the PvP floor (8k); note: {}",
            ehp.note
        );
    }
}
