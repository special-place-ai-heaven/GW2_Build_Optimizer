use crate::balance::BalanceContext;
use crate::combat::{self, CombatPerformance, DamageModifiers};
use crate::data::{DataQuality, DataQualityReason};
use crate::engine;
use crate::gamedb::GameDb;
use crate::rotation;
use crate::rotation::SimulationResult;
use crate::scenario::{CombatKind, CombatTier, ScenarioSpec};
use crate::scoring::{self, raw_direction_score, OptimizationWeights};
use crate::stats;
use crate::validation::ValidatedBuild;
use gw2_core::types::GameMode;

// ─── Viability Gate Thresholds ───────────────────────────────────────────────

/// Minimum stunbreak skills required for PvP/WvW viability. // HEURISTIC
const MIN_STUNBREAKS: u32 = 1;

/// Minimum cleanse count (distinct skills with cleanse) for PvP/WvW viability. // HEURISTIC
const MIN_CLEANSE_COUNT: u32 = 1;
const MIN_CLEANSE_RATE_PER_20S: f64 = 2.0;

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
    /// WvW/PvP: personal cover vs CC — Stability *or* evade/block/invuln/stealth.
    /// Roam also accepts interrupt/disable-first (cut their cast before yours).
    StabilityAccess,
    /// Build must have ≥ MIN_CLEANSE_COUNT cleanse skills (WvW/PvP only).
    CleanseRate,
    /// Build effective health must meet the mode-specific EHP floor.
    EffectiveHealth,
    /// WvW roam (Solo): stealth, evade, block, or mobility to disengage a group.
    MobilityOut,
    /// Harasser: strip/steal/corrupt before dump (Stability strip-all + Protection).
    HarasserStrip,
    /// Harasser / PvP duel: the modeled target threshold was reached inside the clock.
    EncounterOutcome,
    /// Roam / harasser: the kit can interrupt the target's recovery action.
    SecureCompletion,
    /// WvW: an ordered offensive/control chain actually completed under
    /// continuous anti-interrupt cover for at least two seconds.
    ProtectedExecution,
    /// WvW: the player survived the modeled retaliation and can recover or
    /// repeat the exchange instead of winning only on a paper opener.
    SustainRecovery,
    /// WvW: priority actions obey the modeled profession-resource paywall.
    ResourceLegality,
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
    /// Sum over failed gates of how far below threshold each sits, normalized
    /// to 0..=1 per gate (1.0 for gates with no graded metric). Zero when
    /// viable. `search_rank` cannot see a cleanse rate climbing 0.5 -> 3.9
    /// while it still fails; this can, so seed repair and the beam tie-break
    /// can climb a gate gradually instead of only on the flip.
    pub shortfall: f64,
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

/// WvW roam and stallers: rank fight DPS and escape kit, not paper zerg indices.
pub(crate) fn is_roam_objective(scenario: &ScenarioSpec) -> bool {
    needs_outcome_clock(scenario) || needs_mobility_out(scenario)
}

/// Higher is better. WvW first requires a viable, completed exchange, then
/// honors the player's radar weights before ranking surplus role execution.
/// PvE/PvP: realized capped score, realized uncapped direction, then the
/// closed-form stat direction as the last word when output ties exactly.
pub fn search_rank(report: &RefereeReport) -> [i64; 9] {
    let viable = i64::from(report.viability.is_viable);
    let gates = report.viability.gates.iter().filter(|g| g.passed).count() as i64;
    if is_roam_objective(&report.scenario) {
        let rot = report.rotation.as_ref();
        let wvw = rot.and_then(|rotation| rotation.wvw.as_ref());
        let sequence = wvw
            .map(|fight| i64::from(fight.chain_completed))
            .unwrap_or(0);
        let outcome = wvw
            .map(|fight| match report.scenario.combat_kind {
                CombatKind::StrikeSpike | CombatKind::CondiRamp | CombatKind::Harasser => {
                    i64::from(fight.target_reached)
                }
                CombatKind::Disabler
                | CombatKind::Support
                | CombatKind::Commander
                | CombatKind::Staller => i64::from(fight.repeatable),
            })
            .unwrap_or(0);
        let execution = wvw
            .map(|fight| match report.scenario.combat_kind {
                CombatKind::StrikeSpike | CombatKind::Harasser => fight.peak_protected_damage_2s,
                CombatKind::CondiRamp => fight.protected_damage,
                CombatKind::Disabler => fight.control_landed_ms as f64 * 10.0,
                CombatKind::Support | CombatKind::Commander | CombatKind::Staller => {
                    fight.sustain_margin.max(0.0)
                }
            })
            .unwrap_or(0.0)
            .round() as i64;
        let repeatable = wvw.map(|fight| i64::from(fight.repeatable)).unwrap_or(0);
        let sustain = wvw
            .map(|fight| (fight.remaining_health_ratio * 100_000.0) as i64)
            .unwrap_or(0);
        let tempo = wvw
            .and_then(|fight| {
                fight
                    .target_reached_at_ms
                    .map(|at| fight.duration_ms.saturating_sub(at))
            })
            .unwrap_or(0) as i64;
        let intent = (report.user_intent_score * 1_000_000.0).round() as i64;
        let raw = (report.raw_direction_score * 1_000_000.0).round() as i64;
        [
            viable,
            gates,
            sequence,
            outcome,
            intent,
            execution,
            tempo,
            repeatable * 1_000_000 + sustain,
            raw,
        ]
    } else {
        let score = (report.user_intent_score * 1_000_000.0) as i64;
        let raw = (report.raw_direction_score * 1_000_000.0) as i64;
        let stats = (report.stat_direction_score * 1_000_000.0) as i64;
        [viable, gates, score, raw, stats, 0, 0, 0, 0]
    }
}

/// Failed-gate notes for the optimize error path.
pub fn viability_failure_summary(report: &ViabilityReport) -> String {
    let fails: Vec<&str> = report
        .gates
        .iter()
        .filter(|g| !g.passed)
        .map(|g| g.note.as_str())
        .collect();
    if fails.is_empty() {
        "unknown gate failure".into()
    } else {
        fails.join("; ")
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
    evaluate_viability_gates_for(rotation, combat_perf, scenario, None)
}

/// Same as [`evaluate_viability_gates`], applying `profile.viability_gates`
/// floors when present. Unset fields keep the hardcoded mode/tier defaults.
pub fn evaluate_viability_gates_for(
    rotation: Option<&SimulationResult>,
    combat_perf: &CombatPerformance,
    scenario: &ScenarioSpec,
    profile: Option<&crate::data::ObjectiveProfile>,
) -> ViabilityReport {
    let mut gates: Vec<GateResult> = Vec::new();
    // Graded distance below threshold, per gate that has a metric. Gates that
    // fail without a graded entry count 1.0 in the final pass below.
    let mut shortfall = 0.0f64;
    let mut graded: Vec<ViabilityGate> = Vec::new();

    let requires_pvp_gates = matches!(scenario.game_mode, GameMode::WvW | GameMode::PvP);

    if requires_pvp_gates {
        // ── Stunbreak gate ──────────────────────────────────────────────────
        gates.push(match rotation {
            Some(rot) => {
                let passed = rot.stunbreak_count >= MIN_STUNBREAKS;
                if !passed {
                    shortfall += (MIN_STUNBREAKS.saturating_sub(rot.stunbreak_count)) as f64
                        / MIN_STUNBREAKS.max(1) as f64;
                    graded.push(ViabilityGate::StunbreakCount);
                }
                GateResult {
                    gate: ViabilityGate::StunbreakCount,
                    passed,
                    note: format!(
                        "stunbreak_count={} (required >={})",
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

        // Cover, not Stability-only. Meta roam (Daredevil stealth/evade, Mesmer
        // Distortion, Fresh Air invuln, Spellbreaker Full Counter) survives without
        // a dedicated stab utility. Roam also counts interrupt/disable-first.
        gates.push(match rotation {
            Some(rot) => {
                let roam = scenario.combat_tier == CombatTier::Solo;
                let passed =
                    rot.has_stability || rot.has_cover_answer || (roam && rot.has_interrupt);
                let note = if rot.has_stability {
                    "stability available".into()
                } else if rot.has_cover_answer {
                    "cover: evade/block/invuln/stealth".into()
                } else if roam && rot.has_interrupt {
                    "interrupt/disable before incoming CC".into()
                } else {
                    "no cover (stability, evade, block, invuln, stealth) and no interrupt".into()
                };
                GateResult {
                    gate: ViabilityGate::StabilityAccess,
                    passed,
                    note,
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
                let required_rate = effective_cleanse_requirement(scenario, rot);
                let passed = rot.cleanse_count >= MIN_CLEANSE_COUNT
                    && rot.cleanse_rate_per_20s >= required_rate;
                if !passed {
                    let count_short = (MIN_CLEANSE_COUNT.saturating_sub(rot.cleanse_count)) as f64
                        / MIN_CLEANSE_COUNT.max(1) as f64;
                    let rate_short =
                        ((required_rate - rot.cleanse_rate_per_20s) / required_rate).clamp(0.0, 1.0);
                    shortfall += count_short.max(rate_short);
                    graded.push(ViabilityGate::CleanseRate);
                }
                GateResult {
                    gate: ViabilityGate::CleanseRate,
                    passed,
                    note: format!(
                        "cleanse_count={}, rate={:.1}/20s (required count >={}, rate >={required_rate:.1}/20s)",
                        rot.cleanse_count, rot.cleanse_rate_per_20s, MIN_CLEANSE_COUNT
                    ),
                }
            }
            None => GateResult {
                gate: ViabilityGate::CleanseRate,
                passed: false,
                note: "rotation unavailable".into(),
            },
        });

        if scenario.game_mode == GameMode::WvW {
            gates.push(match rotation.and_then(|rotation| rotation.wvw.as_ref()) {
                Some(fight) => {
                    let damage_route = match scenario.combat_kind {
                        CombatKind::StrikeSpike => {
                            fight.target_reached
                                || fight.target_health.is_some_and(|hp| {
                                    fight.peak_protected_damage_2s >= hp * 0.30
                                })
                        }
                        CombatKind::CondiRamp => fight
                            .target_health
                            .is_some_and(|hp| fight.protected_damage >= hp * 0.20),
                        CombatKind::Harasser => {
                            fight.target_health.is_some_and(|hp| {
                                fight.peak_protected_damage_2s >= hp * 0.15
                            }) || fight.secured_sequence_control_ms >= 750
                        }
                        CombatKind::Disabler => fight.secured_sequence_control_ms >= 750,
                        CombatKind::Support | CombatKind::Commander | CombatKind::Staller => true,
                    };
                    let passed = fight.chain_completed && damage_route;
                    GateResult {
                        gate: ViabilityGate::ProtectedExecution,
                        passed,
                        note: format!(
                            "protected={}ms, actions={}, 2s spike={:.0}, sequence control={}ms, interrupted={} (minimum {}ms secured inside the sequence)",
                            fight.longest_protected_window_ms,
                            fight.protected_action_count,
                            fight.peak_protected_damage_2s,
                            fight.secured_sequence_control_ms,
                            fight.interrupted_casts,
                            crate::rotation::wvw_timeline::MIN_PROTECTED_WINDOW_MS,
                        ),
                    }
                }
                None => GateResult {
                    gate: ViabilityGate::ProtectedExecution,
                    passed: false,
                    note: "WvW counterplay timeline unavailable".into(),
                },
            });

            gates.push(match rotation.and_then(|rotation| rotation.wvw.as_ref()) {
                Some(fight) => {
                    let requires_repeat = matches!(
                        scenario.combat_kind,
                        CombatKind::CondiRamp
                            | CombatKind::Support
                            | CombatKind::Commander
                            | CombatKind::Staller
                    );
                    let passed = fight.player_survived && (!requires_repeat || fight.repeatable);
                    if !passed {
                        // Not repeatable but alive: closer to repeatable the
                        // more health is left, so that is the gradient.
                        shortfall += if fight.player_survived {
                            (1.0 - fight.remaining_health_ratio).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        graded.push(ViabilityGate::SustainRecovery);
                    }
                    GateResult {
                        gate: ViabilityGate::SustainRecovery,
                        passed,
                        note: format!(
                            "survived={}, health={:.0}%, margin={:+.0}/s, repeatable={}",
                            fight.player_survived,
                            fight.remaining_health_ratio * 100.0,
                            fight.sustain_margin,
                            fight.repeatable,
                        ),
                    }
                }
                None => GateResult {
                    gate: ViabilityGate::SustainRecovery,
                    passed: false,
                    note: "WvW counterplay timeline unavailable".into(),
                },
            });

            gates.push(match rotation.and_then(|rotation| rotation.wvw.as_ref()) {
                Some(fight) => GateResult {
                    gate: ViabilityGate::ResourceLegality,
                    passed: fight.resource_legal,
                    note: format!(
                        "resource-blocked priority actions={}",
                        fight.resource_blocked_actions
                    ),
                },
                None => GateResult {
                    gate: ViabilityGate::ResourceLegality,
                    passed: false,
                    note: "WvW timeline unavailable".into(),
                },
            });
        }

        if needs_mobility_out(scenario) {
            let staller = scenario.combat_kind == CombatKind::Staller;
            gates.push(match rotation {
                Some(rot) => GateResult {
                    gate: ViabilityGate::MobilityOut,
                    passed: rot.has_mobility_out,
                    note: if rot.has_mobility_out {
                        "escape kit present".into()
                    } else if staller {
                        "no stealth/evade/block/mobility — cannot evade a group".into()
                    } else {
                        "no stealth/evade/block/mobility — cannot disengage a group".into()
                    },
                },
                None => GateResult {
                    gate: ViabilityGate::MobilityOut,
                    passed: false,
                    note: "rotation unavailable".into(),
                },
            });
        }

        if needs_harasser_strip(scenario) {
            gates.push(match rotation {
                Some(rot) => GateResult {
                    gate: ViabilityGate::HarasserStrip,
                    passed: rot.has_strip,
                    note: if rot.has_strip {
                        "strip/steal/corrupt present".into()
                    } else {
                        "harasser/roam without cover-crack (strip/steal/corrupt)".into()
                    },
                },
                None => GateResult {
                    gate: ViabilityGate::HarasserStrip,
                    passed: false,
                    note: "rotation unavailable".into(),
                },
            });
        }

        if needs_outcome_clock(scenario) {
            gates.push(match rotation {
                Some(rot) => {
                    let target_reached = if scenario.game_mode == GameMode::WvW {
                        rot.wvw.as_ref().is_some_and(|fight| fight.target_reached)
                    } else {
                        rot.downed
                    };
                    GateResult {
                        gate: ViabilityGate::EncounterOutcome,
                        passed: target_reached,
                        note: if target_reached {
                            "target threshold reached in window".into()
                        } else {
                            "target threshold not reached by end of clock".into()
                        },
                    }
                }
                None => GateResult {
                    gate: ViabilityGate::EncounterOutcome,
                    passed: false,
                    note: "rotation unavailable".into(),
                },
            });
            gates.push(match rotation {
                Some(rot) => GateResult {
                    gate: ViabilityGate::SecureCompletion,
                    passed: rot.has_interrupt,
                    note: if rot.has_interrupt {
                        "interrupt available for the target's recovery action".into()
                    } else {
                        "no interrupt available for the target's recovery action".into()
                    },
                },
                None => GateResult {
                    gate: ViabilityGate::SecureCompletion,
                    passed: false,
                    note: "rotation unavailable".into(),
                },
            });
        }
    }

    // ── Effective health gate (always runs) ─────────────────────────────────
    // WvW floor varies by combat tier: Roamers need more personal sustain than Zerg players.
    // PvP uses its own (lower) floor — amulet-based gear has a smaller stat budget than
    // ascended WvW, so reusing WvW floors here would non-viably score most real PvP builds.
    let default_ehp_floor = if scenario.combat_kind == CombatKind::Staller {
        match scenario.game_mode {
            GameMode::WvW => EHP_FLOOR_WVW_ROAM,
            GameMode::PvP => EHP_FLOOR_PVP,
            GameMode::PvE => EHP_FLOOR_PVE,
        }
    } else {
        match scenario.game_mode {
            GameMode::WvW => match scenario.combat_tier {
                crate::scenario::CombatTier::Solo => EHP_FLOOR_WVW_ROAM,
                crate::scenario::CombatTier::Party => EHP_FLOOR_WVW_HAVOC,
                crate::scenario::CombatTier::Squad => EHP_FLOOR_WVW_ZERG,
            },
            GameMode::PvP => EHP_FLOOR_PVP,
            GameMode::PvE => EHP_FLOOR_PVE,
        }
    };
    let ehp_floor = profile
        .and_then(|p| p.viability_gates.ehp_floor)
        .unwrap_or(default_ehp_floor);
    let passed = combat_perf.effective_health >= ehp_floor;
    if !passed && ehp_floor > 0.0 {
        shortfall += ((ehp_floor - combat_perf.effective_health) / ehp_floor).clamp(0.0, 1.0);
        graded.push(ViabilityGate::EffectiveHealth);
    }
    gates.push(GateResult {
        gate: ViabilityGate::EffectiveHealth,
        passed,
        note: format!(
            "effective_health={:.0} (required >={:.0})",
            combat_perf.effective_health, ehp_floor
        ),
    });

    let is_viable = gates.iter().all(|g| g.passed);
    for g in &gates {
        if !g.passed && !graded.contains(&g.gate) {
            shortfall += 1.0;
        }
    }
    ViabilityReport {
        gates,
        is_viable,
        shortfall,
    }
}

/// Cleanse rate the scenario demands per 20s. Sustained/support kinds need
/// double the floor. Shared by the gate and [`apply_offbar_cleanse`] so the
/// two cannot drift.
pub fn required_cleanse_rate(scenario: &ScenarioSpec) -> f64 {
    if matches!(
        scenario.combat_kind,
        CombatKind::CondiRamp | CombatKind::Support | CombatKind::Commander | CombatKind::Staller
    ) {
        MIN_CLEANSE_RATE_PER_20S * 2.0
    } else {
        MIN_CLEANSE_RATE_PER_20S
    }
}

/// The rate the gate actually demands of THIS kit: the scenario floor, lowered
/// by self-Resistance uptime. Resistance ignores non-damaging conditions, so a
/// build that holds it needs less cleanse — but damaging conditions still
/// tick through it, so the reduction is capped at 75%. Uses the same
/// `buff_uptime` map the Stability gate reads; a kit with no Resistance sees
/// exactly the scenario floor. Shared by the gate and the off-bar pass.
pub fn effective_cleanse_requirement(scenario: &ScenarioSpec, rot: &SimulationResult) -> f64 {
    let resistance = rot
        .buff_uptime
        .get("Resistance")
        .copied()
        .unwrap_or(0.0)
        .clamp(0.0, 0.75);
    required_cleanse_rate(scenario) * (1.0 - resistance)
}

/// Seconds after "cooldown" in gear/trait tooltip text ("(Cooldown: 9
/// Seconds)"). Skill facts use "recharge"; upgrades use "cooldown".
fn cooldown_seconds_in_text(text: &str) -> Option<f64> {
    let lower = text.to_lowercase();
    let idx = lower.find("cooldown")?;
    let rest = &lower[idx..];
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let n: f64 = digits.parse().ok()?;
    (n > 0.0).then_some(n)
}

/// Leading count before the remove/cleanse verb ("Remove 2 conditions").
/// Defaults to 1 when the text only says "a condition".
fn cleanse_count_in_text(text: &str) -> u32 {
    let lower = text.to_lowercase();
    let verb = ["remov", "cleanse", "cure", "purg", "transfer", "consum"]
        .iter()
        .filter_map(|v| lower.find(v))
        .min();
    let Some(v) = verb else { return 1 };
    let after = &lower[v..];
    let digits: String = after
        .chars()
        .skip_while(|c| !c.is_ascii_digit() && *c != '.')
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().map(|n: u32| n.max(1)).unwrap_or(1)
}

/// Per-20s cleanse rate one piece of tooltip text contributes: count times
/// 20 / cooldown. Without a stated cooldown the trigger is on some event
/// (weapon swap, heal use) whose cadence is unknown here, so it is credited
/// at ONE cleanse per 20s — deliberately conservative.
fn cleanse_rate_from_text(text: &str) -> f64 {
    if !crate::text_util::text_describes_condition_cleanse(text) {
        return 0.0;
    }
    let count = cleanse_count_in_text(text) as f64;
    match cooldown_seconds_in_text(text) {
        Some(cd) => count * 20.0 / cd,
        None => count,
    }
}

fn item_cleanse_rate(db: &GameDb, id: u32) -> f64 {
    let Some(item) = db.items.get(&id) else {
        return 0.0;
    };
    let mut rate = cleanse_rate_from_text(&item.name);
    if let Some(d) = item.description.as_deref() {
        rate = rate.max(cleanse_rate_from_text(d));
    }
    if let Some(details) = &item.details {
        for b in &details.bonuses {
            rate = rate.max(cleanse_rate_from_text(b));
        }
        if let Some(d) = details
            .infix_upgrade
            .as_ref()
            .and_then(|u| u.buff.as_ref())
            .and_then(|b| b.description.as_deref())
        {
            rate = rate.max(cleanse_rate_from_text(d));
        }
    }
    rate
}

/// Cleanses per 20s the kit gets from OFF the skill bar: active sigils, the
/// rune, the relic, and traits. The rotation only sees skills, so Sigil of
/// Cleansing (1 condition per weapon swap, 9s ICD, ~2.2/20s on its own) and
/// every cleanse trait counted for nothing at the gate — a WvW-roam seed was
/// measured stuck at 1.0/20s against a required 2.0 with no way up. Every
/// profession runs through this; nothing here is class-specific.
pub fn kit_cleanse_rate_from_gear(validated: &ValidatedBuild, db: &GameDb) -> f64 {
    let reg = crate::data::cleanse_sources::registry();
    // Registry first (data/cleanse_sources.json); tooltip text only for ids
    // the registry does not know.
    let item_rate = |id: u32| match reg.item(id) {
        Some(s) => s.rate_per_20s(),
        None if reg.knows_item(id) => 0.0, // judged not to cleanse
        None => item_cleanse_rate(db, id),
    };
    let mut rate = 0.0;
    for id in validated.active_sigil_ids() {
        rate += item_rate(id);
    }
    if let Some(r) = &validated.rune {
        rate += item_rate(r.id);
    }
    if let Some(r) = &validated.relic {
        rate += item_rate(r.id);
    }
    for spec in &validated.specializations {
        for &id in spec.all_trait_ids.iter().chain(spec.trait_ids.iter()) {
            if let Some(src) = reg.trait_(id) {
                rate += src.rate_per_20s();
            } else if reg.knows_trait(id) {
                // read by a cataloguer and judged not to cleanse
            } else if let Some(tr) = db.traits.get(&id) {
                let mut t = cleanse_rate_from_text(&tr.name);
                if let Some(d) = tr.description.as_deref() {
                    t = t.max(cleanse_rate_from_text(d));
                }
                rate += t;
            }
        }
    }
    rate
}

/// Re-judge the CleanseRate gate with off-bar cleanse added, keeping the
/// report's `shortfall` exact for the new total.
pub fn apply_offbar_cleanse(
    report: &mut ViabilityReport,
    rotation: Option<&SimulationResult>,
    validated: &ValidatedBuild,
    db: &GameDb,
    scenario: &ScenarioSpec,
) {
    let Some(rot) = rotation else { return };
    let gear = kit_cleanse_rate_from_gear(validated, db);
    if gear <= 0.0 {
        return;
    }
    let required = effective_cleanse_requirement(scenario, rot);
    let count_short = (MIN_CLEANSE_COUNT.saturating_sub(rot.cleanse_count)) as f64
        / MIN_CLEANSE_COUNT.max(1) as f64;
    let short = |rate: f64| {
        let rate_short = ((required - rate) / required).clamp(0.0, 1.0);
        count_short.max(rate_short)
    };
    let before = rot.cleanse_rate_per_20s;
    let after = before + gear;
    let was_failing = rot.cleanse_count < MIN_CLEANSE_COUNT || before < required;
    let now_passes = rot.cleanse_count >= MIN_CLEANSE_COUNT && after >= required;
    let mut changed = false;
    for g in &mut report.gates {
        if g.gate != ViabilityGate::CleanseRate {
            continue;
        }
        g.note = format!(
            "cleanse_count={}, rate={:.1}/20s incl. {:.1} from sigils/rune/relic/traits (required count >={}, rate >={required:.1}/20s)",
            rot.cleanse_count, after, gear, MIN_CLEANSE_COUNT
        );
        if was_failing {
            let prev = short(before);
            let next = if now_passes { 0.0 } else { short(after) };
            report.shortfall = (report.shortfall - prev + next).max(0.0);
        }
        if !g.passed && now_passes {
            g.passed = true;
            changed = true;
        }
    }
    if changed {
        report.is_viable = report.gates.iter().all(|g| g.passed);
    }
}

/// Relic, rune, or trait grants Stability even when the skill bar has none.
/// Thief/Daredevil kits often use Relic of the Cavalier for this.
pub fn kit_grants_stability(validated: &ValidatedBuild, db: &GameDb) -> bool {
    if let Some(r) = &validated.relic {
        let item = db.items.get(&r.id);
        let bonuses: &[String] = item
            .and_then(|i| i.details.as_ref())
            .map(|d| d.bonuses.as_slice())
            .unwrap_or(&[]);
        let desc = item.and_then(|i| {
            i.description.as_deref().or_else(|| {
                i.details
                    .as_ref()
                    .and_then(|d| d.infix_upgrade.as_ref())
                    .and_then(|u| u.buff.as_ref())
                    .and_then(|b| b.description.as_deref())
            })
        });
        if crate::text_util::gear_text_grants_stability(&r.name, desc, bonuses) {
            return true;
        }
    }
    if let Some(r) = &validated.rune {
        if let Some(item) = db.items.get(&r.id) {
            let bonuses = item
                .details
                .as_ref()
                .map(|d| d.bonuses.as_slice())
                .unwrap_or(&[]);
            let desc = item.description.as_deref().or_else(|| {
                item.details
                    .as_ref()
                    .and_then(|d| d.infix_upgrade.as_ref())
                    .and_then(|u| u.buff.as_ref())
                    .and_then(|b| b.description.as_deref())
            });
            if crate::text_util::gear_text_grants_stability(&item.name, desc, bonuses) {
                return true;
            }
        }
    }
    for spec in &validated.specializations {
        for &id in spec.all_trait_ids.iter().chain(spec.trait_ids.iter()) {
            if let Some(tr) = db.traits.get(&id) {
                if crate::text_util::text_describes_stability(&tr.name)
                    || tr
                        .description
                        .as_deref()
                        .is_some_and(crate::text_util::text_describes_stability)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Count relic/rune/trait Stability as passing the skill-bar gate.
pub fn apply_offbar_stability(
    report: &mut ViabilityReport,
    validated: &ValidatedBuild,
    db: &GameDb,
) {
    if !kit_grants_stability(validated, db) {
        return;
    }
    let mut changed = false;
    for g in &mut report.gates {
        if g.gate == ViabilityGate::StabilityAccess && !g.passed {
            g.passed = true;
            g.note = "stability from relic, rune, or trait".into();
            changed = true;
        }
    }
    if changed {
        report.is_viable = report.gates.iter().all(|g| g.passed);
        // StabilityAccess has no graded metric, so it counted 1.0 when failed.
        report.shortfall = (report.shortfall - 1.0).max(0.0);
    }
}

fn needs_mobility_out(scenario: &ScenarioSpec) -> bool {
    scenario.combat_kind == CombatKind::Staller
        || (scenario.game_mode == GameMode::WvW && scenario.combat_tier == CombatTier::Solo)
}

fn needs_harasser_strip(scenario: &ScenarioSpec) -> bool {
    scenario.combat_kind == CombatKind::Harasser
}

fn needs_outcome_clock(scenario: &ScenarioSpec) -> bool {
    if matches!(
        scenario.combat_kind,
        CombatKind::Support | CombatKind::Commander | CombatKind::Staller
    ) {
        return false;
    }
    scenario.combat_kind == CombatKind::Harasser
        || (scenario.game_mode == GameMode::PvP && scenario.combat_tier == CombatTier::Solo)
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
    /// Uncapped radar-direction score — the final rank tie-break so
    /// post-saturation piece swaps toward the user's wished stats win ties
    /// that the capped `user_intent_score` cannot see.
    pub raw_direction_score: f64,
    /// Per-axis output of the 60s flow simulation, as fractions of the
    /// realized norms. This is what `user_intent_score` is computed from.
    pub realized: scoring::RealizedAxes,
    /// Uncapped closed-form radar direction of the stats alone. The last
    /// PvE/PvP rank key: it only speaks when realized output ties exactly,
    /// which a kit that produces nothing at all will do.
    pub stat_direction_score: f64,
    pub quality: DataQuality,
    pub quality_reasons: Vec<DataQualityReason>,
}

/// Look up the objective profile named on `scenario`, if any.
/// Unset or unknown ids resolve to `None` and keep hardcoded gate floors.
fn objective_profile_for<'a>(
    scenario: &ScenarioSpec,
    catalog: &'a crate::data::ObjectiveProfileData,
) -> Option<&'a crate::data::ObjectiveProfile> {
    scenario
        .objective_profile_id
        .as_deref()
        .and_then(|id| catalog.profile_by_id(id))
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
    let prepared = engine::prepare_validated_rotation(validated, db, &stats, Some(scenario));
    let rotation = prepared
        .as_ref()
        .map(|p| engine::simulate_prepared(p, validated, db, Some(scenario)));
    // ── Viability gating ──────────────────────────────────────────────────────
    // Run before score computation. Non-viable builds receive sentinel score -1.0.
    let mut viability = evaluate_viability_gates_for(
        rotation.as_ref(),
        &primary_combat,
        scenario,
        objective_profile_for(
            scenario,
            crate::data::objective_profiles::objective_profiles(),
        ),
    );
    apply_offbar_stability(&mut viability, validated, db);
    apply_offbar_cleanse(&mut viability, rotation.as_ref(), validated, db, scenario);

    // The flow simulation is most of an evaluation's cost and nothing reads
    // its axes for a build the gates already sent to -1.0.
    let realized = match prepared.as_ref() {
        Some(p) if viability.is_viable => scoring::realized_axes(
            &engine::simulate_flow(p, weights, Some(scenario)),
            &primary_combat,
        ),
        _ => scoring::realized_axes_no_rotation(&primary_combat),
    };
    // What the rotation produced, scheduled toward the radar. The closed-form
    // stat score could not see a skill at all (measured 2026-09-04: 0 of 36
    // utilities moved a PvE rank).
    let (user_intent_score, raw_direction_score, stat_direction_score) = if viability.is_viable {
        (
            scoring::score_realized(&realized, weights),
            scoring::raw_realized(&realized, weights),
            raw_direction_score(&primary_combat, weights),
        )
    } else {
        (-1.0, -1.0, -1.0)
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

    if let Some(fight) = rotation.as_ref().and_then(|result| result.wvw.as_ref()) {
        if fight.unmodeled_effect_sources > 0 {
            quality = quality.merge(&DataQuality::Provisional);
            quality_reasons.push(DataQualityReason {
                field: "wvw_timeline.effects".into(),
                entity: profession_name.into(),
                modes: vec![ctx.game_mode.label().to_string()],
                explanation: format!(
                    "{} equipped or triggered effect sources are not yet represented by timed rules",
                    fight.unmodeled_effect_sources
                ),
            });
        }
        if !fight.resource_model_complete {
            quality = quality.merge(&DataQuality::Provisional);
            quality_reasons.push(DataQualityReason {
                field: "wvw_timeline.resources".into(),
                entity: profession_name.into(),
                modes: vec![ctx.game_mode.label().to_string()],
                explanation:
                    "The active profession mechanic is outside the bounded resource ledger".into(),
            });
        }
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
        raw_direction_score,
        realized,
        stat_direction_score,
        quality,
        quality_reasons,
    }
}

#[cfg(test)]
mod tests {
    // Intentional invariant tripwires.
    #![allow(clippy::assertions_on_constants)]
    use super::{
        evaluate_validated_build, evaluate_viability_gates, evaluate_viability_gates_for,
        search_rank, GateResult, RefereeReport, ViabilityGate, ViabilityReport, EHP_FLOOR_PVE,
        EHP_FLOOR_PVP, EHP_FLOOR_WVW_HAVOC, EHP_FLOOR_WVW_ROAM, EHP_FLOOR_WVW_ZERG,
    };
    use crate::balance::BalanceContext;
    use crate::combat::CombatPerformance;
    use crate::data::DataQuality;
    use crate::gamedb::GameDb;
    use crate::rotation::{wvw_timeline::WvwCombatReport, SimulationResult};
    use crate::scenario::{CombatTier, OptimizationTarget, ScenarioSpec, TargetProfile};
    use crate::scoring::OptimizationWeights;
    use crate::validation::{
        RejectCode, ValidatedBuild, ValidatedSkills, ValidatedSpec, ValidatedWeaponSet,
        ValidatedWeapons, ValidationReject,
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
            healing_per_second: 0.0,
            control_uptime: 0.0,
            might_stacks_avg: 0.0,
            boon_equivalents: 0.0,
            has_mobility_out: true,
            escape_kinds: 1,
            has_strip: true,
            has_corrupt: false,
            downed: true,
            finished: true,
            has_interrupt: true,
            has_cover_answer: true,
            wvw: Some(WvwCombatReport {
                duration_ms: 5_000,
                target_health: Some(18_000.0),
                target_reached_at_ms: None,
                longest_protected_window_ms: 2_500,
                protected_action_count: 3,
                successful_action_count: 4,
                interrupted_casts: 0,
                protected_damage: 10_000.0,
                peak_protected_damage_2s: 8_000.0,
                peak_protected_damage_5s: 10_000.0,
                total_damage: 10_000.0,
                control_landed_ms: 1_000,
                incoming_damage: 2_000.0,
                avoided_damage: 2_000.0,
                healing: 2_000.0,
                barrier_absorbed: 0.0,
                conditions_cleansed: 2,
                combo_activations: 0,
                remaining_health_ratio: 0.9,
                sustain_margin: 400.0,
                player_survived: true,
                target_reached: true,
                chain_completed: true,
                secured_sequence_damage: 10_000.0,
                secured_sequence_control_ms: 1_000,
                repeatable: true,
                resource_blocked_actions: 0,
                resource_legal: true,
                resource_model_complete: true,
                unmodeled_effect_sources: 0,
            }),
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

    fn sigil_item(id: u32, name: &str, bonus: &str) -> gw2_api::models::Item {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": name, "type": "UpgradeComponent", "rarity": "Exotic",
            "level": 60, "details": { "type": "Sigil", "bonuses": [bonus] }
        }))
        .expect("test item")
    }

    /// Sigil of Cleansing: 1 condition per weapon swap, 9s ICD = 20/9 per 20s.
    /// A damage sigil contributes nothing. Rates add across seats.
    #[test]
    fn gear_cleanse_rate_reads_sigil_tooltips() {
        use super::{
            cleanse_count_in_text, cleanse_rate_from_text, cooldown_seconds_in_text,
            kit_cleanse_rate_from_gear,
        };
        use crate::validation::ValidatedItem;
        let mut db = GameDb::empty_for_tests();
        db.items.insert(1, sigil_item(1, "Superior Sigil of Cleansing",
            "Remove 1 condition when you swap to this weapon while in combat. (Cooldown: 9 Seconds)"));
        db.items.insert(2, sigil_item(2, "Superior Sigil of Force", "+5% Damage"));
        let mut b = ValidatedBuild {
            sigils: vec![
                ValidatedItem { id: 1, name: "Cleansing".into() },
                ValidatedItem { id: 2, name: "Force".into() },
            ],
            ..Default::default()
        };
        let rate = kit_cleanse_rate_from_gear(&b, &db);
        assert!((rate - 20.0 / 9.0).abs() < 1e-9, "got {rate}");
        b.sigils.push(ValidatedItem { id: 1, name: "Cleansing".into() });
        // Only the two active seats count.
        assert!((kit_cleanse_rate_from_gear(&b, &db) - 20.0 / 9.0).abs() < 1e-9);
        assert_eq!(cleanse_count_in_text("Remove 2 conditions from allies"), 2);
        assert_eq!(cooldown_seconds_in_text("(Cooldown: 9 Seconds)"), Some(9.0));
        assert_eq!(cleanse_rate_from_text("Grants Might on hit"), 0.0);
    }

    /// The registry knows Superior Sigil of Cleansing (67340) even when the
    /// item is not in the database: 3 conditions per swap, 9 s cooldown.
    #[test]
    fn gear_cleanse_rate_reads_the_registry_first() {
        use super::kit_cleanse_rate_from_gear;
        use crate::validation::ValidatedItem;
        let db = GameDb::empty_for_tests();
        let b = ValidatedBuild {
            sigils: vec![ValidatedItem { id: 67340, name: "Cleansing".into() }],
            ..Default::default()
        };
        let rate = kit_cleanse_rate_from_gear(&b, &db);
        assert!((rate - 3.0 * 20.0 / 9.0).abs() < 1e-9, "got {rate}");
    }

    /// A gate failing on rate alone must flip to passed once gear covers the
    /// gap, and the report's shortfall must return to exactly zero.
    #[test]
    fn offbar_cleanse_flips_the_gate_and_zeroes_shortfall() {
        use super::{apply_offbar_cleanse, effective_cleanse_requirement, MIN_CLEANSE_COUNT};
        use crate::validation::ValidatedItem;
        let mut db = GameDb::empty_for_tests();
        db.items.insert(1, sigil_item(1, "Superior Sigil of Cleansing",
            "Remove 1 condition when you swap to this weapon while in combat. (Cooldown: 9 Seconds)"));
        let b = ValidatedBuild {
            sigils: vec![ValidatedItem { id: 1, name: "Cleansing".into() }],
            ..Default::default()
        };
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        let mut rot = make_viable_rotation();
        rot.cleanse_count = MIN_CLEANSE_COUNT;
        // Same requirement the gate and the off-bar pass use (Resistance-aware).
        let required = effective_cleanse_requirement(&scenario, &rot);
        rot.cleanse_rate_per_20s = required * 0.5; // fails on rate only
        let combat = make_viable_combat();
        let mut report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let cleanse = |r: &ViabilityReport| {
            r.gates.iter().find(|g| g.gate == ViabilityGate::CleanseRate).cloned().unwrap()
        };
        assert!(!cleanse(&report).passed);
        assert!((report.shortfall - 0.5).abs() < 1e-9, "rate shortfall is 0.5: {}", report.shortfall);
        apply_offbar_cleanse(&mut report, Some(&rot), &b, &db, &scenario);
        assert!(cleanse(&report).passed, "{}", cleanse(&report).note);
        assert!(report.is_viable);
        assert!(report.shortfall.abs() < 1e-9, "shortfall must be zero: {}", report.shortfall);
    }

    fn make_rank_report(rotation: SimulationResult) -> RefereeReport {
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        RefereeReport {
            scenario,
            stats: crate::stats::StatBlock::default(),
            modifiers: crate::combat::DamageModifiers::default(),
            combat_solo: CombatPerformance::default(),
            combat_party: CombatPerformance::default(),
            combat_squad: CombatPerformance::default(),
            primary_combat: CombatPerformance::default(),
            rotation: Some(rotation),
            viability: ViabilityReport {
                gates: Vec::new(),
                is_viable: true,
                shortfall: 0.0,
            },
            user_intent_score: 0.0,
            raw_direction_score: -1.0,
            realized: Default::default(),
            stat_direction_score: -1.0,
            quality: DataQuality::Verified,
            quality_reasons: Vec::new(),
        }
    }

    #[test]
    fn roam_rank_prefers_completed_sequence_over_larger_uncovered_total() {
        let mut completed = make_viable_rotation();
        let completed_wvw = completed.wvw.as_mut().expect("WvW report");
        completed_wvw.target_reached = true;
        completed_wvw.target_reached_at_ms = Some(2_000);
        let protected_damage = completed_wvw.protected_damage;
        let protected_peak = completed_wvw.peak_protected_damage_2s;

        let mut uncovered = completed.clone();
        let uncovered_wvw = uncovered.wvw.as_mut().expect("WvW report");
        uncovered_wvw.chain_completed = false;
        uncovered_wvw.target_reached = false;
        uncovered_wvw.protected_damage = protected_damage * 2.0;
        uncovered_wvw.peak_protected_damage_2s = protected_peak * 2.0;

        assert!(
            search_rank(&make_rank_report(completed)) > search_rank(&make_rank_report(uncovered))
        );
    }

    #[test]
    fn roam_rank_prefers_earlier_target_threshold_when_other_terms_match() {
        let mut earlier = make_viable_rotation();
        let earlier_wvw = earlier.wvw.as_mut().expect("WvW report");
        earlier_wvw.target_reached = true;
        earlier_wvw.target_reached_at_ms = Some(1_000);

        let mut later = earlier.clone();
        later.wvw.as_mut().expect("WvW report").target_reached_at_ms = Some(4_000);

        assert!(search_rank(&make_rank_report(earlier)) > search_rank(&make_rank_report(later)));
    }
    #[test]
    fn roam_disabler_rank_honors_user_weights_after_required_exchange() {
        let mut aligned_rotation = make_viable_rotation();
        aligned_rotation
            .wvw
            .as_mut()
            .expect("WvW report")
            .control_landed_ms = 2_000;

        let mut misaligned_rotation = aligned_rotation.clone();
        misaligned_rotation
            .wvw
            .as_mut()
            .expect("WvW report")
            .control_landed_ms = 3_000;

        let mut aligned = make_rank_report(aligned_rotation);
        aligned.scenario.combat_kind = crate::scenario::CombatKind::Disabler;
        aligned.user_intent_score = 0.85;

        let mut misaligned = make_rank_report(misaligned_rotation);
        misaligned.scenario.combat_kind = crate::scenario::CombatKind::Disabler;
        misaligned.user_intent_score = 0.25;

        assert!(search_rank(&aligned) > search_rank(&misaligned));
    }

    fn make_wvw_scenario() -> ScenarioSpec {
        ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: CombatTier::Squad,
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "WvW".into(),
            },
            patch_id: None,
            objective_profile_id: None,
        }
    }

    fn make_pve_scenario() -> ScenarioSpec {
        ScenarioSpec {
            game_mode: GameMode::PvE,
            combat_tier: CombatTier::Party,
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "PvE".into(),
            },
            patch_id: None,
            objective_profile_id: None,
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
        assert_eq!(report.gates.len(), 7); // three bar checks + three timeline checks + effective health
        for g in &report.gates {
            assert!(
                g.passed,
                "gate {:?} should pass but failed: {}",
                g.gate, g.note
            );
        }
    }

    /// A missing dummy HP must not pass the percent-of-target damage routes.
    /// `unwrap_or(0.0)` used to make `peak >= 0` true for every StrikeSpike fight.
    #[test]
    fn gate_wvw_no_target_does_not_pass_percent_damage() {
        let mut rot = make_viable_rotation();
        let fight = rot.wvw.as_mut().expect("fixture has a timeline");
        fight.target_health = None;
        fight.target_reached = false;
        fight.peak_protected_damage_2s = 8_000.0;
        fight.protected_damage = 10_000.0;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let gate = gate_by_kind(&report.gates, &ViabilityGate::ProtectedExecution)
            .expect("WvW evaluates ProtectedExecution");
        assert!(
            !gate.passed,
            "unknown target HP must not satisfy the 30% spike route: {}",
            gate.note
        );
    }

    #[test]
    fn wvw_outcome_uses_timeline_not_legacy_dummy() {
        let mut rot = make_viable_rotation();
        rot.downed = true;
        rot.wvw.as_mut().expect("WvW report").target_reached = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        scenario.combat_kind = crate::scenario::CombatKind::Harasser;

        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let gate =
            gate_by_kind(&report.gates, &ViabilityGate::EncounterOutcome).expect("outcome gate");

        assert!(!gate.passed);
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
        rot.has_cover_answer = false;
        rot.has_interrupt = false;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);

        assert!(!report.is_viable);
        let g = gate_by_kind(&report.gates, &ViabilityGate::StabilityAccess).unwrap();
        assert!(!g.passed);
        assert!(g.note.contains("no cover"));
    }

    #[test]
    fn gate_wvw_evade_without_stability_passes() {
        let mut rot = make_viable_rotation();
        rot.has_stability = false;
        rot.has_cover_answer = true;
        rot.has_interrupt = false;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let g = gate_by_kind(&report.gates, &ViabilityGate::StabilityAccess).unwrap();
        assert!(g.passed, "note={}", g.note);
        assert!(report.is_viable);
    }

    #[test]
    fn gate_roam_interrupt_without_stability_passes() {
        let mut rot = make_viable_rotation();
        rot.has_stability = false;
        rot.has_cover_answer = false;
        rot.has_interrupt = true;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let g = gate_by_kind(&report.gates, &ViabilityGate::StabilityAccess).unwrap();
        assert!(g.passed, "note={}", g.note);
    }

    #[test]
    fn gate_zerg_interrupt_without_cover_fails() {
        let mut rot = make_viable_rotation();
        rot.has_stability = false;
        rot.has_cover_answer = false;
        rot.has_interrupt = true;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let g = gate_by_kind(&report.gates, &ViabilityGate::StabilityAccess).unwrap();
        assert!(!g.passed);
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

    #[test]
    fn profile_ehp_floors_change_gate_outcome() {
        let combat = CombatPerformance {
            effective_health: 20_000.0,
            ..Default::default()
        };
        let scenario = make_pve_scenario();
        let mut low = crate::data::objective_profiles::objective_profiles()
            .default_for_mode("PvE")
            .expect("embedded PvE default")
            .clone();
        low.viability_gates.ehp_floor = Some(15_000.0);
        let mut high = low.clone();
        high.viability_gates.ehp_floor = Some(25_000.0);

        let pass = evaluate_viability_gates_for(None, &combat, &scenario, Some(&low));
        let fail = evaluate_viability_gates_for(None, &combat, &scenario, Some(&high));
        assert!(
            pass.is_viable,
            "20k EHP should pass a 15k floor: {:?}",
            pass.gates
        );
        assert!(
            !fail.is_viable,
            "20k EHP should fail a 25k floor: {:?}",
            fail.gates
        );
        assert_eq!(pass.gates[0].gate, ViabilityGate::EffectiveHealth);
        assert_eq!(fail.gates[0].gate, ViabilityGate::EffectiveHealth);
        // The custom floor, not the hardcoded default, must surface in the note.
        assert!(
            pass.gates[0].note.contains("15000"),
            "note should reflect the 15k override: {}",
            pass.gates[0].note
        );
        assert!(
            fail.gates[0].note.contains("25000"),
            "note should reflect the 25k override: {}",
            fail.gates[0].note
        );
    }

    #[test]
    fn evaluate_validated_build_uses_scenario_profile_ehp_floor() {
        let combat = CombatPerformance {
            effective_health: 20_000.0,
            ..Default::default()
        };

        let mut high = crate::data::objective_profiles::objective_profiles()
            .default_for_mode("PvE")
            .expect("embedded PvE default")
            .clone();
        high.objective_profile_id = "test_high_ehp".into();
        high.viability_gates.ehp_floor = Some(25_000.0);
        let catalog = crate::data::ObjectiveProfileData {
            files: HashMap::from([(
                "PvE".into(),
                crate::data::ObjectiveProfileFile {
                    mode: "PvE".into(),
                    profiles: vec![high],
                },
            )]),
        };

        let mut named = make_pve_scenario();
        named.objective_profile_id = Some("test_high_ehp".into());
        let fail = evaluate_viability_gates_for(
            None,
            &combat,
            &named,
            super::objective_profile_for(&named, &catalog),
        );
        assert!(
            !fail.is_viable,
            "20k EHP should fail the scenario profile's 25k floor: {:?}",
            fail.gates
        );

        let mut none = make_pve_scenario();
        none.objective_profile_id = None;
        let pass = evaluate_viability_gates_for(
            None,
            &combat,
            &none,
            super::objective_profile_for(&none, &catalog),
        );
        assert!(
            pass.is_viable,
            "20k EHP should pass hardcoded PvE floor when profile id is unset: {:?}",
            pass.gates
        );
    }

    /// PIN: embedded JSONs carry no `viability_gates` key, so deserialized profiles
    /// have `viability_gates.ehp_floor == None` and the hardcoded mode/tier constants
    /// from `evaluate_viability_gates_for` apply. Pins the exact current mapping —
    /// if this trips, the gate defaults or the embedded JSONs changed.
    #[test]
    fn unset_viability_gates_pin_hardcoded_ehp_floors() {
        let data = crate::data::objective_profiles::objective_profiles();
        let pve_profile = data.default_for_mode("PvE").expect("embedded PvE default");
        let pvp_profile = data.default_for_mode("PvP").expect("embedded PvP default");
        let wvw_profile = data.default_for_mode("WvW").expect("embedded WvW default");
        assert!(
            pve_profile.viability_gates.ehp_floor.is_none()
                && pvp_profile.viability_gates.ehp_floor.is_none()
                && wvw_profile.viability_gates.ehp_floor.is_none(),
            "PIN setup: embedded default profiles must have no ehp_floor override"
        );

        let mut pvp_scenario = make_pve_scenario();
        pvp_scenario.game_mode = GameMode::PvP;
        pvp_scenario.optimization_target = OptimizationTarget {
            label: "PvP".into(),
        };
        let wvw_tier = |tier: CombatTier| {
            let mut s = make_wvw_scenario();
            s.combat_tier = tier;
            s
        };
        let mut wvw_staller = make_wvw_scenario();
        wvw_staller.combat_kind = crate::scenario::CombatKind::Staller;

        // (label, scenario, embedded profile, pinned hardcoded floor).
        // Mapping pinned from production: PvE→EHP_FLOOR_PVE, PvP→EHP_FLOOR_PVP,
        // WvW Solo→ROAM, WvW Party→HAVOC, WvW Squad→ZERG, WvW Staller→ROAM.
        let cases: Vec<(&str, ScenarioSpec, &crate::data::ObjectiveProfile, f64)> = vec![
            ("PvE", make_pve_scenario(), pve_profile, EHP_FLOOR_PVE),
            ("PvP", pvp_scenario, pvp_profile, EHP_FLOOR_PVP),
            (
                "WvW/Solo",
                wvw_tier(CombatTier::Solo),
                wvw_profile,
                EHP_FLOOR_WVW_ROAM,
            ),
            (
                "WvW/Party",
                wvw_tier(CombatTier::Party),
                wvw_profile,
                EHP_FLOOR_WVW_HAVOC,
            ),
            (
                "WvW/Squad",
                wvw_tier(CombatTier::Squad),
                wvw_profile,
                EHP_FLOOR_WVW_ZERG,
            ),
            ("WvW/Staller", wvw_staller, wvw_profile, EHP_FLOOR_WVW_ROAM),
        ];

        for (label, scenario, profile, floor) in &cases {
            // A no-`viability_gates` profile must behave exactly like no profile at all.
            assert_ehp_boundary(label, scenario, Some(profile), *floor);
            assert_ehp_boundary(label, scenario, None, *floor);
        }
    }

    /// Asserts that EHP exactly at `floor` passes and just below fails.
    fn assert_ehp_boundary(
        label: &str,
        scenario: &ScenarioSpec,
        profile: Option<&crate::data::ObjectiveProfile>,
        floor: f64,
    ) {
        let at = CombatPerformance {
            effective_health: floor,
            ..CombatPerformance::default()
        };
        let below = CombatPerformance {
            effective_health: floor - 0.5,
            ..CombatPerformance::default()
        };
        let at_report = evaluate_viability_gates_for(None, &at, scenario, profile);
        let below_report = evaluate_viability_gates_for(None, &below, scenario, profile);
        let at_gate = gate_by_kind(&at_report.gates, &ViabilityGate::EffectiveHealth)
            .expect("EffectiveHealth gate");
        let below_gate = gate_by_kind(&below_report.gates, &ViabilityGate::EffectiveHealth)
            .expect("EffectiveHealth gate");
        assert!(
            at_gate.passed,
            "{label}: EHP={floor} should pass pinned floor; note='{}'",
            at_gate.note
        );
        assert!(
            !below_gate.passed,
            "{label}: EHP={} should fail pinned floor {floor}",
            floor - 0.5
        );
    }

    /// WvW build with `rotation = None` → rotation-dependent gates fail with "rotation unavailable".
    #[test]
    fn gate_wvw_rotation_none_rotation_gates_fail_gracefully() {
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(None, &combat, &scenario);

        // Three rotation gates, two WvW timeline gates, and effective health.
        assert_eq!(report.gates.len(), 7);
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
            pets: Default::default(),
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
            localized: None,
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
                profession: vec![],
            },
            legends: vec![],
            aquatic_legends: vec![],
            pets: None,
            rune: None,
            sigils: vec![],
            sigil_seats: Default::default(),
            relic: None,
            gear_slots: gw2_core::types::GearSlots::default(), // itemstat 9999 intentionally absent
            explanation: String::new(),
            synergy_explanation: String::new(),
            changes: vec![],
            warnings: vec![],
            errors: vec![],
        }
    }

    /// The rank must see skills. The same build with a striking utility in a
    /// slot outranks the build with every slot empty, in PvE, where the
    /// closed-form stat score could not tell them apart (0 of 36 fillers
    /// moved the rank, measured 2026-09-04).
    #[test]
    fn pve_rank_prefers_the_bar_that_produces_more() {
        let mut db = make_test_db();
        db.skills.insert(
            777,
            gw2_api::models::Skill {
                id: 777,
                name: "Cleave".into(),
                description: None,
                icon: None,
                chat_link: None,
                skill_type: None,
                weapon_type: None,
                professions: vec!["Warrior".into()],
                slot: Some("Utility".into()),
                facts: vec![
                    gw2_api::models::Fact::Damage {
                        text: None,
                        icon: None,
                        hit_count: Some(3),
                        dmg_multiplier: Some(1.5),
                    },
                    gw2_api::models::Fact::Recharge {
                        text: None,
                        icon: None,
                        value: Some(8.0),
                    },
                ],
                traited_facts: vec![],
                categories: vec![],
                attunement: None,
                cost: None,
                dual_wield: None,
                flip_skill: None,
                initiative: None,
                next_chain: None,
                prev_chain: None,
                transform_skills: vec![],
                bundle_skills: vec![],
                toolbelt_skill: None,
                flags: vec![],
                specialization: None,
            },
        );
        let ctx = BalanceContext::new(GameMode::PvE);
        let mut scenario = ScenarioSpec::from_balance_context(&ctx);
        scenario.combat_tier = CombatTier::Party;
        let weights = OptimizationWeights::preset_power_dps();

        db.skills.insert(
            778,
            gw2_api::models::Skill {
                id: 778,
                name: "Second Cleave".into(),
                facts: vec![
                    gw2_api::models::Fact::Damage {
                        text: None,
                        icon: None,
                        hit_count: Some(2),
                        dmg_multiplier: Some(1.2),
                    },
                    gw2_api::models::Fact::Recharge {
                        text: None,
                        icon: None,
                        value: Some(12.0),
                    },
                ],
                ..db.skills[&777].clone()
            },
        );
        let mut full = make_minimal_validated();
        full.skills.utilities = vec![Some((777, "Cleave".into())), None, None];
        let mut fuller = make_minimal_validated();
        fuller.skills.utilities = vec![
            Some((777, "Cleave".into())),
            Some((778, "Second Cleave".into())),
            None,
        ];
        let mut hole = make_minimal_validated();
        hole.skills.utilities = vec![None, None, None];

        let full_report =
            evaluate_validated_build(&full, &db, "Warrior", &weights, &ctx, &scenario);
        let hole_report =
            evaluate_validated_build(&hole, &db, "Warrior", &weights, &ctx, &scenario);
        assert!(
            full_report.viability.is_viable && hole_report.viability.is_viable,
            "{:?} / {:?}",
            full_report.viability.gates,
            hole_report.viability.gates
        );
        assert!(
            full_report.realized.power > hole_report.realized.power,
            "cleave produced strike: {:?} vs {:?}",
            full_report.realized,
            hole_report.realized
        );
        assert!(
            search_rank(&full_report) > search_rank(&hole_report),
            "full {:?} vs hole {:?}",
            search_rank(&full_report),
            search_rank(&hole_report)
        );
        // Two real rotations: the fuller bar must outrank the one-skill bar.
        let fuller_report =
            evaluate_validated_build(&fuller, &db, "Warrior", &weights, &ctx, &scenario);
        assert!(
            search_rank(&fuller_report) > search_rank(&full_report),
            "fuller {:?} vs full {:?}",
            search_rank(&fuller_report),
            search_rank(&full_report)
        );
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
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "WvW".into(),
            },
            patch_id: None,
            objective_profile_id: None,
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
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "WvW".into(),
            },
            patch_id: None,
            objective_profile_id: None,
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
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "WvW".into(),
            },
            patch_id: None,
            objective_profile_id: None,
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
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "PvP".to_string(),
            },
            patch_id: None,
            objective_profile_id: None,
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
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "PvP".to_string(),
            },
            patch_id: None,
            objective_profile_id: None,
        };
        let report = evaluate_viability_gates(Some(&rot), &combat, &pvp_scenario);
        let ehp = gate_by_kind(&report.gates, &ViabilityGate::EffectiveHealth).expect("gate");
        assert!(
            !ehp.passed,
            "PvP 5k EHP should fail the PvP floor (8k); note: {}",
            ehp.note
        );
    }

    #[test]
    fn gate_wvw_roam_fails_without_mobility_out() {
        let mut rot = make_viable_rotation();
        rot.has_mobility_out = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let g = gate_by_kind(&report.gates, &ViabilityGate::MobilityOut).unwrap();
        assert!(!g.passed);
        assert!(!report.is_viable);
    }

    #[test]
    fn gate_roam_power_does_not_require_strip() {
        let mut rot = make_viable_rotation();
        rot.has_strip = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        scenario.combat_kind = crate::scenario::CombatKind::StrikeSpike;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        assert!(gate_by_kind(&report.gates, &ViabilityGate::HarasserStrip).is_none());
    }

    #[test]
    fn gate_harasser_fails_without_strip() {
        let mut rot = make_viable_rotation();
        rot.has_strip = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_kind = crate::scenario::CombatKind::Harasser;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let g = gate_by_kind(&report.gates, &ViabilityGate::HarasserStrip).unwrap();
        assert!(!g.passed);
        assert!(!report.is_viable);
    }

    #[test]
    fn gate_zerg_dps_does_not_require_roam_out() {
        let mut rot = make_viable_rotation();
        rot.has_mobility_out = false;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario(); // Squad strike
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        assert!(gate_by_kind(&report.gates, &ViabilityGate::MobilityOut).is_none());
        assert!(report.is_viable);
    }

    #[test]
    fn gate_wvw_solo_does_not_require_outcome_window() {
        let mut rot = make_viable_rotation();
        rot.downed = false;
        rot.has_interrupt = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        assert!(gate_by_kind(&report.gates, &ViabilityGate::EncounterOutcome).is_none());
        assert!(gate_by_kind(&report.gates, &ViabilityGate::SecureCompletion).is_none());
        assert!(report.is_viable);
    }

    #[test]
    fn gate_harasser_requires_the_target_threshold() {
        let mut rot = make_viable_rotation();
        rot.downed = false;
        rot.wvw.as_mut().unwrap().target_reached = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_kind = crate::scenario::CombatKind::Harasser;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let g = gate_by_kind(&report.gates, &ViabilityGate::EncounterOutcome).unwrap();
        assert!(!g.passed);
        assert!(!report.is_viable);
    }

    #[test]
    fn gate_harasser_fails_without_interrupt() {
        let mut rot = make_viable_rotation();
        rot.has_interrupt = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_kind = crate::scenario::CombatKind::Harasser;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let g = gate_by_kind(&report.gates, &ViabilityGate::SecureCompletion).unwrap();
        assert!(!g.passed);
        assert!(!report.is_viable);
    }

    #[test]
    fn gate_zerg_pressure_does_not_require_outcome_window() {
        let mut rot = make_viable_rotation();
        rot.downed = false;
        rot.has_interrupt = false;
        let combat = make_viable_combat();
        let scenario = make_wvw_scenario();
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        assert!(gate_by_kind(&report.gates, &ViabilityGate::EncounterOutcome).is_none());
        assert!(gate_by_kind(&report.gates, &ViabilityGate::SecureCompletion).is_none());
        assert!(report.is_viable);
    }

    #[test]
    fn gate_support_does_not_require_outcome_window() {
        let mut rot = make_viable_rotation();
        rot.downed = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        scenario.combat_kind = crate::scenario::CombatKind::Support;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        assert!(gate_by_kind(&report.gates, &ViabilityGate::EncounterOutcome).is_none());
    }

    #[test]
    fn gate_staller_skips_outcome_and_strip_even_on_roam() {
        let mut rot = make_viable_rotation();
        rot.downed = false;
        rot.has_interrupt = false;
        rot.has_strip = false;
        rot.has_mobility_out = true;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Solo;
        scenario.combat_kind = crate::scenario::CombatKind::Staller;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        assert!(gate_by_kind(&report.gates, &ViabilityGate::EncounterOutcome).is_none());
        assert!(gate_by_kind(&report.gates, &ViabilityGate::HarasserStrip).is_none());
        let out = gate_by_kind(&report.gates, &ViabilityGate::MobilityOut).unwrap();
        assert!(out.passed);
        assert!(report.is_viable);
    }

    #[test]
    fn gate_staller_on_zerg_still_needs_escape_kit() {
        let mut rot = make_viable_rotation();
        rot.has_mobility_out = false;
        let combat = make_viable_combat();
        let mut scenario = make_wvw_scenario();
        scenario.combat_tier = CombatTier::Squad;
        scenario.combat_kind = crate::scenario::CombatKind::Staller;
        let report = evaluate_viability_gates(Some(&rot), &combat, &scenario);
        let g = gate_by_kind(&report.gates, &ViabilityGate::MobilityOut).unwrap();
        assert!(!g.passed);
        assert!(!report.is_viable);
    }

    #[test]
    fn raw_direction_breaks_capped_score_ties() {
        // Two builds with identical capped intent but different uncapped raw
        // direction: the radar direction must win the tie at the rank tail.
        let mk = |raw: f64| {
            let mut scenario = make_wvw_scenario();
            scenario.combat_tier = CombatTier::Solo;
            RefereeReport {
                scenario,
                stats: crate::stats::StatBlock::default(),
                modifiers: crate::combat::DamageModifiers::default(),
                combat_solo: CombatPerformance::default(),
                combat_party: CombatPerformance::default(),
                combat_squad: CombatPerformance::default(),
                primary_combat: CombatPerformance::default(),
                rotation: None,
                viability: ViabilityReport {
                    gates: Vec::new(),
                    is_viable: true,
                    shortfall: 0.0,
                },
                user_intent_score: 0.5,
                raw_direction_score: raw,
                realized: Default::default(),
                stat_direction_score: -1.0,
                quality: DataQuality::Verified,
                quality_reasons: Vec::new(),
            }
        };
        assert!(search_rank(&mk(0.7)) > search_rank(&mk(0.5)));
    }
}
