use super::stats::{compute_3tier_combat, perf_to_combat_metrics};
use crate::state::AddonState;
use gw2_optimizer::balance::BalanceContext;
use gw2_optimizer::scoring::OptimizationWeights;

/// Start optimization in background thread (S11-T01, S11-T02, S11-T03)
pub(super) fn start_optimization(state: &mut AddonState) {
    // Guard against concurrent optimization
    if state.main.optimizing {
        return;
    }

    // Get profession from current build
    let profession_name = state
        .main
        .current_build
        .as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();

    start_optimization_with_profession(state, &profession_name);
}

/// Start optimization with explicit profession name (avoids borrow conflicts).
/// Uses `state.main.build_locks` for spec/trait lock constraints.
pub(super) fn start_optimization_with_profession(state: &mut AddonState, profession_name: &str) {
    if state.main.game_db.is_none() {
        state.main.error = Some("Game data not loaded. Wait for cache to load.".into());
        return;
    }

    if profession_name.is_empty() {
        state.main.error = Some("No character selected".into());
        return;
    }

    let db = state.main.game_db.clone();
    let profession_name = profession_name.to_string();
    let config = state.config.clone();
    let game_mode = state.main.game_mode.clone();
    let game_mode_label = game_mode.label().to_string();
    let balance_ctx = BalanceContext::new(game_mode.clone());
    let current_build_summary = state
        .main
        .current_build
        .as_ref()
        .map(|b| summarize_resolved_build(b));
    let addon_dir = state.addon_dir.clone();
    let token = state.cancel_token.clone();
    let weights = state.main.weights.clone();
    let build_locks = state.main.build_locks.clone();
    // Capture selection snapshot so results can be discarded if the user switches
    // character or build tab while optimization is running (TOCTOU guard).
    let optimizing_for_char = state.main.selected_character;
    let optimizing_for_build_tab = state.main.selected_build_tab;
    let optimizing_for_equip_tab = state.main.selected_equipment_tab;

    state.main.optimizing = true;
    state.main.optimize_stage = "Starting...".into();

    // Log the weights and deterministic gear prefix for debugging
    let gear_match = gw2_optimizer::scoring::select_gear_prefix(&weights);
    nexus::log::log(
        nexus::log::LogLevel::Info,
        "GW2BuildOpt",
        &format!(
            "Optimizing {}/{}: weights P={:.2} C={:.2} B={:.2} H={:.2} S={:.2} Ctrl={:.2} ({}) -> gear: {} (sim={:.3})",
            profession_name, game_mode_label,
            weights.power, weights.condition, weights.boon_support, weights.healing, weights.sustain, weights.control,
            weights.summary_label(),
            gear_match.primary, gear_match.similarity,
        ),
    );
    state.main.comparison.suggestions.clear();
    state.main.comparison.loading = true;
    state.main.comparison.error = None;

    std::thread::spawn(move || {
        let panic_token = token.clone();
        let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let result = (|| -> Result<Vec<crate::ui::comparison::BuildSuggestion>, String> {
                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                let db = db.ok_or("GameDb not loaded")?;

                // ═══ Primary: Deterministic synergy engine (no LLM for build selection) ═══
                {
                    let llm_client_opt: Option<Box<dyn gw2_optimizer::llm::LlmClient>> =
                        gw2_optimizer::llm::create_client(&config, &addon_dir).ok();

                    let token_det = token.clone();
                    let llm_ref: Option<&dyn gw2_optimizer::llm::LlmClient> =
                        llm_client_opt.as_ref().map(|c| c.as_ref());
                    match gw2_optimizer::engine::optimize_deterministic(
                        &db,
                        &profession_name,
                        &weights,
                        &balance_ctx,
                        llm_ref,
                        current_build_summary.as_deref(),
                        &build_locks,
                        &mut |progress: gw2_optimizer::engine::OptimizeProgress| {
                            if token_det.is_cancelled() {
                                return;
                            }
                            crate::state::with_state(|s| {
                                s.main.optimize_stage = progress.stage.clone();
                            });
                        },
                    ) {
                        Ok(synergy_result) => {
                            if token.is_cancelled() {
                                return Err("Cancelled".into());
                            }
                            let suggestion =
                                synergy_result_to_suggestion(&synergy_result, &profession_name);
                            return Ok(vec![suggestion]);
                        }
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                &format!(
                                    "Deterministic engine failed, trying Gemini pipeline: {}",
                                    e
                                ),
                            );
                            // Fall through to Gemini pipeline
                        }
                    }
                }

                // ═══ Fallback 1: LLM synergy pipeline (LLM-driven build selection) ═══
                if config.has_active_llm_key() {
                    let llm_client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                        .map_err(|e| e.to_string())?;

                    let token_synergy = token.clone();
                    match gw2_optimizer::engine::optimize_with_gemini(
                        &db,
                        &profession_name,
                        &weights,
                        &balance_ctx,
                        llm_client.as_ref(),
                        current_build_summary.as_deref(),
                        &build_locks,
                        &mut |progress| {
                            if token_synergy.is_cancelled() {
                                return;
                            }
                            crate::state::with_state(|s| {
                                s.main.optimize_stage = progress.stage.clone();
                            });
                        },
                    ) {
                        Ok(synergy_result) => {
                            if token.is_cancelled() {
                                return Err("Cancelled".into());
                            }
                            let suggestion =
                                synergy_result_to_suggestion(&synergy_result, &profession_name);
                            return Ok(vec![suggestion]);
                        }
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                &format!("LLM pipeline failed, falling back to legacy: {}", e),
                            );
                            // Fall through to legacy pipeline
                        }
                    }
                }

                // ═══ Legacy pipeline (fallback or no Gemini key) ═══
                let profession = db.profession(&profession_name).ok_or_else(|| {
                    format!("Profession '{}' not found in GameDb", profession_name)
                })?;

                let token_progress = token.clone();
                let candidates = gw2_optimizer::engine::optimize(
                    profession,
                    &weights,
                    None,
                    &db.items,
                    &db.itemstats,
                    &db.specializations,
                    &db.traits,
                    |progress| {
                        if token_progress.is_cancelled() {
                            return;
                        }
                        crate::state::with_state(|s| {
                            s.main.optimize_stage = progress.stage.clone();
                        });
                    },
                    5,
                    &balance_ctx,
                    &build_locks,
                    &db.pvp_amulets,
                )?;

                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                let mut suggestions: Vec<crate::ui::comparison::BuildSuggestion> = candidates
                    .iter()
                    .map(|c| candidate_to_suggestion(c, &db, &balance_ctx))
                    .collect();

                // Enrich top suggestion with LLM reasoning (legacy path)
                if config.has_active_llm_key() {
                    if token.is_cancelled() {
                        return Err("Cancelled".into());
                    }

                    crate::state::with_state(|s| {
                        s.main.optimize_stage = "Consulting AI for synergy analysis...".into();
                    });

                    match enrich_with_llm(
                        &config,
                        &profession_name,
                        &weights,
                        &game_mode_label,
                        &candidates,
                        &db,
                        current_build_summary.as_deref(),
                        &mut suggestions,
                        &addon_dir,
                        &balance_ctx,
                    ) {
                        Ok(()) => {}
                        Err(e) => {
                            nexus::log::log(
                                nexus::log::LogLevel::Warning,
                                "GW2 Build Optimizer",
                                &format!("LLM enrichment skipped: {}", e),
                            );
                        }
                    }
                }

                Ok(suggestions)
            })();

            if !token.is_cancelled() {
                crate::state::with_state(|s| {
                    s.main.optimizing = false;
                    s.main.comparison.loading = false;
                    // Discard results if the user switched character or build tab while
                    // optimization was running — showing results for a different context
                    // would silently corrupt the comparison panel.
                    let context_changed = s.main.selected_character != optimizing_for_char
                        || s.main.selected_build_tab != optimizing_for_build_tab
                        || s.main.selected_equipment_tab != optimizing_for_equip_tab;
                    if context_changed {
                        return;
                    }
                    match result {
                        Ok(suggestions) => {
                            s.main.comparison.suggestions = suggestions;
                            s.main.comparison.selected_suggestion = 0;
                        }
                        Err(e) => {
                            s.main.comparison.error = Some(e);
                        }
                    }
                });
            }
        }));

        // If the thread panicked, recover and show error
        if let Err(panic_info) = thread_result {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("Internal error (panic): {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("Internal error (panic): {}", s)
            } else {
                "Internal error: optimization thread panicked".into()
            };
            if !panic_token.is_cancelled() {
                crate::state::with_state(|s| {
                    s.main.optimizing = false;
                    s.main.comparison.loading = false;
                    s.main.comparison.error = Some(msg);
                });
            }
        }
    });
}

/// Convert a SynergyResult from the new pipeline into a BuildSuggestion for display.
fn synergy_result_to_suggestion(
    result: &gw2_optimizer::engine::SynergyResult,
    profession_name: &str,
) -> crate::ui::comparison::BuildSuggestion {
    use crate::ui::comparison::BuildSuggestion;

    let v = &result.validated;

    // Specializations: (name, [trait_name1, trait_name2, trait_name3])
    let specializations: Vec<(String, Vec<String>)> = v
        .specializations
        .iter()
        .map(|s| {
            let label = if s.elite {
                format!("{} [E]", s.name)
            } else {
                s.name.clone()
            };
            (label, s.trait_names.clone())
        })
        .collect();

    // Weapons: flatten into display strings like "Set 1: Sword / Shield"
    let mut weapons = Vec::new();
    let fmt_set =
        |set: &gw2_optimizer::validation::ValidatedWeaponSet, label: &str| -> Option<String> {
            match (&set.main_hand, &set.off_hand) {
                (Some(main), Some(off)) => Some(format!("{}: {} / {}", label, main, off)),
                (Some(main), None) => Some(format!("{}: {}", label, main)),
                _ => None,
            }
        };
    if let Some(s) = fmt_set(&v.weapons.set1, "Set 1") {
        weapons.push(s);
    }
    if let Some(s) = fmt_set(&v.weapons.set2, "Set 2") {
        weapons.push(s);
    }

    // Skills: flatten into display strings
    let mut skills = Vec::new();
    if let Some((_, name)) = &v.skills.heal {
        skills.push(format!("Heal: {}", name));
    }
    for util in &v.skills.utilities {
        if let Some((_, name)) = util {
            skills.push(format!("Utility: {}", name));
        }
    }
    if let Some((_, name)) = &v.skills.elite {
        skills.push(format!("Elite: {}", name));
    }

    // Sigils: flatten to display strings
    let sigils: Vec<String> = v.sigils.iter().map(|s| s.name.clone()).collect();

    // Convert stats from optimizer StatBlock (f64) to core StatBlock (i32)
    let derived = gw2_optimizer::stats::compute_derived(&result.stats, profession_name);
    let estimated_stats = Some(gw2_core::types::StatBlock {
        power: result.stats.power.round() as i32,
        precision: result.stats.precision.round() as i32,
        toughness: result.stats.toughness.round() as i32,
        vitality: result.stats.vitality.round() as i32,
        condition_damage: result.stats.condition_damage.round() as i32,
        expertise: result.stats.expertise.round() as i32,
        concentration: result.stats.concentration.round() as i32,
        ferocity: result.stats.ferocity.round() as i32,
        healing_power: result.stats.healing_power.round() as i32,
        crit_chance: derived.crit_chance,
        crit_damage: derived.crit_damage,
        health: derived.health.round() as i32,
        armor: derived.armor.round() as i32,
    });

    // Convert combat performance to CombatMetrics
    let combat_solo = Some(perf_to_combat_metrics(&result.combat_solo));
    let combat_party = Some(perf_to_combat_metrics(&result.combat_party));
    let combat_squad = Some(perf_to_combat_metrics(&result.combat_squad));

    // Convert rotation simulation result
    let rotation = result
        .rotation
        .as_ref()
        .map(|sim| gw2_core::types::RotationBreakdown {
            simulated_dps: sim.total_dps.round() as i32,
            strike_dps: sim.strike_dps.round() as i32,
            condition_dps: sim.condition_dps.round() as i32,
            condition_uptime: sim
                .condition_uptime
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            buff_uptime: sim
                .buff_uptime
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            skill_usage: sim
                .skill_usage
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        s.cast_count,
                        s.dps_contribution.round() as i32,
                    )
                })
                .collect(),
            stunbreak_count: sim.stunbreak_count,
            has_stability: sim.has_stability,
            stability_uptime: sim.stability_uptime,
            cleanse_count: sim.cleanse_count,
            cleanse_rate_per_20s: sim.cleanse_rate_per_20s,
        });

    // Build changes_made from validated structured changes
    let changes_made: Vec<String> = v
        .changes
        .iter()
        .map(|c| {
            if c.from.is_empty() {
                format!("[{}] → {} ({})", c.slot, c.to, c.reason)
            } else {
                format!("[{}] {} → {} ({})", c.slot, c.from, c.to, c.reason)
            }
        })
        .collect();

    // Warnings as additional info
    let mut explanation = v.explanation.clone();
    if !v.warnings.is_empty() {
        if !explanation.is_empty() {
            explanation.push_str("\n\n");
        }
        explanation.push_str("Warnings: ");
        explanation.push_str(&v.warnings.join("; "));
    }

    BuildSuggestion {
        label: "Synergy Build".into(),
        build_summary: format!(
            "Gear: {}",
            v.gear_prefix
                .as_ref()
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown")
        ),
        stat_prefix: v
            .gear_prefix
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        specializations,
        weapons,
        skills,
        rune: v.rune.as_ref().map(|r| r.name.clone()).unwrap_or_default(),
        sigils,
        relic: v.relic.as_ref().map(|r| r.name.clone()).unwrap_or_default(),
        explanation,
        synergy_explanation: v.synergy_explanation.clone(),
        changes_made,
        estimated_stats,
        combat_solo,
        combat_party,
        combat_squad,
        rotation,
        viability: None,
    }
}

/// Convert BuildCandidate to BuildSuggestion for display (S11-T04)
fn candidate_to_suggestion(
    candidate: &gw2_optimizer::engine::BuildCandidate,
    db: &gw2_optimizer::gamedb::GameDb,
    balance_ctx: &BalanceContext,
) -> crate::ui::comparison::BuildSuggestion {
    use crate::ui::comparison::BuildSuggestion;

    // Get spec names with actually selected traits (not all 9)
    let mut specializations = Vec::new();
    if let Some(elite_id) = candidate.elite_spec {
        if let Some(spec) = db.spec(elite_id) {
            let traits: Vec<String> = candidate
                .equipped_traits
                .iter()
                .filter(|tid| spec.major_traits.contains(tid))
                .filter_map(|&tid| db.traits.get(&tid).map(|t| t.name.clone()))
                .collect();
            specializations.push((format!("{} [E]", spec.name), traits));
        }
    }
    for &core_id in &candidate.core_specs {
        if let Some(spec) = db.spec(core_id) {
            let traits: Vec<String> = candidate
                .equipped_traits
                .iter()
                .filter(|tid| spec.major_traits.contains(tid))
                .filter_map(|&tid| db.traits.get(&tid).map(|t| t.name.clone()))
                .collect();
            specializations.push((spec.name.clone(), traits));
        }
    }

    // Convert stats from optimizer::stats::StatBlock to core::types::StatBlock
    let estimated_stats = Some(gw2_core::types::StatBlock {
        power: candidate.stats.power.round() as i32,
        precision: candidate.stats.precision.round() as i32,
        toughness: candidate.stats.toughness.round() as i32,
        vitality: candidate.stats.vitality.round() as i32,
        condition_damage: candidate.stats.condition_damage.round() as i32,
        expertise: candidate.stats.expertise.round() as i32,
        concentration: candidate.stats.concentration.round() as i32,
        ferocity: candidate.stats.ferocity.round() as i32,
        healing_power: candidate.stats.healing_power.round() as i32,
        crit_chance: candidate.derived.crit_chance,
        crit_damage: candidate.derived.crit_damage,
        health: candidate.derived.health.round() as i32,
        armor: candidate.derived.armor.round() as i32,
    });

    // Compute combat metrics for all 3 buff profiles
    let profession_name = db
        .professions
        .values()
        .next()
        .map(|p| p.name.as_str())
        .unwrap_or("Warrior");
    // Try to determine profession from elite spec
    let prof_name = if let Some(elite_id) = candidate.elite_spec {
        db.spec(elite_id)
            .map(|s| s.profession.as_str())
            .unwrap_or(profession_name)
    } else if let Some(&core_id) = candidate.core_specs.first() {
        db.spec(core_id)
            .map(|s| s.profession.as_str())
            .unwrap_or(profession_name)
    } else {
        profession_name
    };

    let (combat_solo, combat_party, combat_squad) = compute_3tier_combat(
        &candidate.stats,
        &candidate.derived,
        &candidate.modifiers,
        prof_name,
        balance_ctx,
    );

    BuildSuggestion {
        label: format!("Score: {:.2}", candidate.score),
        build_summary: format!("Gear: {}", candidate.gear.stat_prefix_name),
        stat_prefix: candidate.gear.stat_prefix_name.clone(),
        specializations,
        weapons: Vec::new(),
        skills: Vec::new(),
        rune: String::new(),
        sigils: Vec::new(),
        relic: String::new(),
        explanation: String::new(),
        synergy_explanation: String::new(),
        changes_made: Vec::new(),
        estimated_stats,
        combat_solo,
        combat_party,
        combat_squad,
        rotation: None,
        viability: None,
    }
}

/// Run rotation simulation for a suggestion's skills and attach the results.
///
/// Resolves ALL build skills: weapon skills from both weapon sets (tagged for
/// weapon swap scheduling) + heal/utility/elite from the skills list.
/// The simulator uses DPCT-optimal scheduling with automatic weapon swapping.
pub(super) fn simulate_suggestion_rotation(
    suggestion: &mut crate::ui::comparison::BuildSuggestion,
    db: &gw2_optimizer::gamedb::GameDb,
) {
    if suggestion.skills.is_empty() && suggestion.weapons.is_empty() {
        return;
    }

    let mut all_rotation_skills: Vec<gw2_optimizer::rotation::RotationSkill> = Vec::new();

    // 1. Resolve weapon skills from suggestion.weapons (format: "Set 1: Axe / Axe")
    if !suggestion.weapons.is_empty() {
        let profession = infer_profession_from_specs(&suggestion.specializations, db);
        let weapon_sets = parse_weapon_sets(&suggestion.weapons);

        for (set_num, weapon_types) in &weapon_sets {
            let mut set_skill_ids: Vec<u32> = Vec::new();
            for wtype in weapon_types {
                for skill in db.skills.values() {
                    if skill.weapon_type.as_deref() == Some(wtype.as_str())
                        && skill
                            .professions
                            .iter()
                            .any(|p| p.eq_ignore_ascii_case(&profession))
                        && skill
                            .slot
                            .as_deref()
                            .map(|s| s.starts_with("Weapon_"))
                            .unwrap_or(false)
                        && !set_skill_ids.contains(&skill.id)
                    {
                        set_skill_ids.push(skill.id);
                    }
                }
            }
            if !set_skill_ids.is_empty() {
                let mut set_skills =
                    gw2_optimizer::rotation::builder::build_rotation_skills(&set_skill_ids, db);
                gw2_optimizer::rotation::builder::tag_weapon_set(&mut set_skills, *set_num);
                all_rotation_skills.extend(set_skills);
            }
        }
    }

    // 2. Resolve heal/utility/elite from suggestion.skills
    //    Format: "Heal: Name", "Utils: Name1, Name2, Name3", "Elite: Name"
    let skill_names = parse_skill_names(&suggestion.skills);
    for name in &skill_names {
        if let Some(skill) = db
            .skills
            .values()
            .find(|s| s.name.eq_ignore_ascii_case(name))
        {
            if !all_rotation_skills.iter().any(|rs| rs.skill_id == skill.id) {
                let mut rs_vec =
                    gw2_optimizer::rotation::builder::build_rotation_skills(&[skill.id], db);
                // Non-weapon skills stay at weapon_set=0 (always available)
                all_rotation_skills.append(&mut rs_vec);
            }
        }
    }

    if all_rotation_skills.is_empty() {
        return;
    }

    // Extract stats from estimated_stats for the simulation
    let stats = suggestion.estimated_stats.as_ref();
    let power = stats.map(|s| s.power as f64).unwrap_or(1000.0);
    let condition_damage = stats.map(|s| s.condition_damage as f64).unwrap_or(0.0);
    let weapon_strength = 1100.0; // reference weapon strength (same as combat.rs)

    let result = gw2_optimizer::rotation::simulator::simulate(
        &all_rotation_skills,
        0,
        power,
        condition_damage,
        weapon_strength,
    );

    suggestion.rotation = Some(gw2_core::types::RotationBreakdown {
        simulated_dps: result.total_dps.round() as i32,
        strike_dps: result.strike_dps.round() as i32,
        condition_dps: result.condition_dps.round() as i32,
        condition_uptime: result.condition_uptime.into_iter().collect(),
        buff_uptime: result.buff_uptime.into_iter().collect(),
        skill_usage: result
            .skill_usage
            .iter()
            .map(|su| {
                (
                    su.name.clone(),
                    su.cast_count,
                    su.dps_contribution.round() as i32,
                )
            })
            .collect(),
        stunbreak_count: result.stunbreak_count,
        has_stability: result.has_stability,
        stability_uptime: result.stability_uptime,
        cleanse_count: result.cleanse_count,
        cleanse_rate_per_20s: result.cleanse_rate_per_20s,
    });
}

/// Parse weapon sets from suggestion.weapons strings.
/// Input format: "Set 1: Axe / Axe", "Set 2: Greatsword"
/// Returns: [(1, ["Axe", "Axe"]), (2, ["Greatsword"])]
fn parse_weapon_sets(weapons: &[String]) -> Vec<(u8, Vec<String>)> {
    let mut sets = Vec::new();
    for w in weapons {
        let set_num = if w.starts_with("Set 1") {
            1u8
        } else if w.starts_with("Set 2") {
            2u8
        } else {
            1u8
        }; // fallback

        let rest = w.split(':').nth(1).unwrap_or(w).trim();
        let types: Vec<String> = rest
            .split('/')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "null")
            .collect();

        if !types.is_empty() {
            sets.push((set_num, types));
        }
    }
    sets
}

/// Parse individual skill names from formatted suggestion.skills strings.
/// "Heal: Mending" → "Mending"
/// "Utils: Blood Reckoning, Bull's Charge, Signet of Fury" → 3 names
/// "Elite: Head Butt" → "Head Butt"
fn parse_skill_names(skills: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for s in skills {
        if let Some(rest) = s.strip_prefix("Heal: ") {
            names.push(rest.trim().to_string());
        } else if let Some(rest) = s.strip_prefix("Utils: ") {
            for name in rest.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        } else if let Some(rest) = s.strip_prefix("Elite: ") {
            names.push(rest.trim().to_string());
        } else if let Some(rest) = s.strip_prefix("Utility: ") {
            let name = rest.trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
        } else {
            // Fallback: try the whole string as a skill name
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                names.push(trimmed.to_string());
            }
        }
    }
    names
}

/// Infer profession name from specialization names in the suggestion.
fn infer_profession_from_specs(
    specs: &[(String, Vec<String>)],
    db: &gw2_optimizer::gamedb::GameDb,
) -> String {
    for (spec_name, _) in specs {
        let clean = spec_name.replace(" [E]", "");
        for spec in db.specializations.values() {
            if spec.name.eq_ignore_ascii_case(&clean) {
                return spec.profession.clone();
            }
        }
    }
    // Fallback to first profession in db
    db.professions
        .values()
        .next()
        .map(|p| p.name.clone())
        .unwrap_or_default()
}

// infer_weights_from_stats is now in radar_chart.rs

/// Summarize a ResolvedBuild as text for LLM prompts.
fn summarize_resolved_build(build: &gw2_core::types::ResolvedBuild) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Profession: {}", build.profession));

    let specs: Vec<String> = build
        .specializations
        .iter()
        .map(|s| {
            let elite = if s.elite { " [E]" } else { "" };
            let traits: Vec<&str> = s.traits_selected.iter().map(|t| t.name.as_str()).collect();
            format!("{}{}: {}", s.name, elite, traits.join(", "))
        })
        .collect();
    if !specs.is_empty() {
        parts.push(format!("Specs: {}", specs.join(" | ")));
    }

    if let Some(ref h) = build.skills.heal {
        parts.push(format!("Heal: {}", h.name));
    }
    let utils: Vec<String> = build
        .skills
        .utilities
        .iter()
        .filter_map(|u| u.as_ref().map(|s| s.name.clone()))
        .collect();
    if !utils.is_empty() {
        parts.push(format!("Utils: {}", utils.join(", ")));
    }
    if let Some(ref e) = build.skills.elite {
        parts.push(format!("Elite: {}", e.name));
    }

    for set in &build.weapons {
        let mut w = Vec::new();
        if let Some(ref mh) = set.main_hand {
            w.push(mh.weapon_type.clone());
        }
        if let Some(ref oh) = set.off_hand {
            w.push(oh.weapon_type.clone());
        }
        if !w.is_empty() {
            parts.push(format!("{}: {}", set.label, w.join(" / ")));
        }
    }

    if !build.armor.is_empty() && !build.armor[0].stat_prefix.is_empty() {
        parts.push(format!("Gear: {}", build.armor[0].stat_prefix));
    }
    if let Some(ref r) = build.rune {
        parts.push(format!("Rune: {}", r.name));
    }
    if let Some(ref r) = build.relic {
        parts.push(format!("Relic: {}", r.name));
    }

    parts.join("\n")
}

/// Apply Gemini's parsed response onto a BuildSuggestion.
fn apply_gemini_response(
    suggestion: &mut crate::ui::comparison::BuildSuggestion,
    gemini: &gw2_optimizer::prompts::GeminiBuildResponse,
) {
    if !gemini.explanation.is_empty() {
        suggestion.explanation = gemini.explanation.clone();
    }
    if let Some(ref synergy) = gemini.synergy_explanation {
        if !synergy.is_empty() {
            suggestion.synergy_explanation = synergy.clone();
        }
    }
    if !gemini.specializations.is_empty() {
        suggestion.specializations = gemini.specializations.clone();
    }
    if !gemini.weapons.is_empty() {
        suggestion.weapons = gemini.weapons.clone();
    }
    if !gemini.skills.is_empty() {
        suggestion.skills = gemini.skills.clone();
    }
    if !gemini.rune.is_empty() {
        suggestion.rune = gemini.rune.clone();
    }
    if !gemini.sigils.is_empty() {
        suggestion.sigils = gemini.sigils.clone();
    }
    if !gemini.relic.is_empty() {
        suggestion.relic = gemini.relic.clone();
    }
    if !gemini.stat_prefix.is_empty() {
        suggestion.stat_prefix = gemini.stat_prefix.clone();
    }
    if !gemini.changes_made.is_empty() {
        suggestion.changes_made = gemini.changes_made.clone();
    }
}

/// Convert Gemini tool function names to human-readable descriptions.
fn humanize_tool_names(tool_names: &[String]) -> String {
    let labels: Vec<&str> = tool_names
        .iter()
        .map(|n| match n.as_str() {
            "get_profession_info" => "reading profession",
            "get_spec_traits" => "checking traits",
            "get_trait_details" => "analyzing trait",
            "get_skill_info" => "checking skill",
            "list_runes" => "browsing runes",
            "list_sigils" => "browsing sigils",
            "list_relics" => "browsing relics",
            "calculate_stats" => "calculating stats",
            "simulate_combat" => "simulating combat",
            "score_build" => "scoring build",
            "get_current_build" => "reading current build",
            "get_optimizer_results" => "reviewing candidates",
            "search_traits_by_effect" => "searching trait synergies",
            "find_condition_sources" => "finding condition sources",
            "search_skills_by_effect" => "searching skill synergies",
            "find_synergies" => "analyzing synergies",
            "get_build_synergy_report" => "building synergy report",
            "simulate_rotation" => "simulating rotation",
            _ => "working",
        })
        .collect();
    labels.join(", ")
}

/// Call the active LLM provider to enrich the top optimizer suggestion with AI reasoning.
/// Uses function calling (tool use) so the LLM can query game data and simulate builds.
fn enrich_with_llm(
    config: &gw2_core::config::AppConfig,
    profession_name: &str,
    weights: &OptimizationWeights,
    game_mode: &str,
    candidates: &[gw2_optimizer::engine::BuildCandidate],
    db: &gw2_optimizer::gamedb::GameDb,
    current_build_summary: Option<&str>,
    suggestions: &mut [crate::ui::comparison::BuildSuggestion],
    addon_dir: &std::path::Path,
    balance_ctx: &BalanceContext,
) -> Result<(), String> {
    let client = gw2_optimizer::llm::create_client(config, addon_dir).map_err(|e| e.to_string())?;

    // Build tool-aware prompt
    let prompt = if current_build_summary.is_some() {
        gw2_optimizer::prompts::improve_build_prompt_with_tools(profession_name, weights, game_mode)
    } else {
        gw2_optimizer::prompts::new_build_prompt_with_tools(profession_name, weights, game_mode)
    };

    let tools = gw2_optimizer::llm::tools::tool_definitions();
    let build_summary_owned = current_build_summary.map(|s| s.to_string());
    let ctx = gw2_optimizer::gemini_tools::ToolContext {
        db,
        profession_name,
        candidates,
        current_build_summary: build_summary_owned.as_deref(),
        weights: weights.clone(),
        balance_ctx,
    };

    let response = client
        .generate_with_tools_progress(
            &prompt,
            &tools,
            &mut |name: &str, args: &serde_json::Value| {
                gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx)
            },
            8,
            &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                let tools_str = humanize_tool_names(tool_names);
                crate::state::with_state(|s| {
                    s.main.optimize_stage =
                        format!("AI thinking ({}/{})... {}", turn, max_turns, tools_str);
                });
            },
        )
        .map_err(|e| e.to_string())?;

    let gemini_build = gw2_optimizer::prompts::parse_gemini_build(&response)
        .map_err(|e| format!("Parse failed: {}", e))?;

    // Validate LLM's output against GameDb before applying
    let validated =
        gw2_optimizer::validation::validate_gemini_build(&gemini_build, db, profession_name);
    if !validated.errors.is_empty() {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            &format!(
                "Legacy enrichment validation errors: {}",
                validated.errors.join("; ")
            ),
        );
    }

    crate::state::with_state(|s| {
        s.main.optimize_stage = "Applying AI build + simulating rotation...".into();
    });

    if let Some(first) = suggestions.first_mut() {
        apply_gemini_response(first, &gemini_build);
        // Run rotation simulation now that LLM has populated skills
        simulate_suggestion_rotation(first, db);
    }

    Ok(())
}

/// Send a chat message to the active LLM provider for build refinement.
/// Uses function calling so the LLM can query game data to answer questions.
pub(super) fn send_chat_message(state: &mut AddonState, message: String) {
    // Guard against concurrent chat messages
    if state.main.chat.waiting {
        return;
    }

    if !state.config.has_active_llm_key() {
        crate::ui::chat_bar::add_ai_response(
            &mut state.main.chat,
            "No AI API key configured. Set one in Settings.".into(),
        );
        return;
    }

    state.main.chat.waiting = true;

    let config = state.config.clone();
    let profession = state
        .main
        .current_build
        .as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();
    let build_summary = state
        .main
        .current_build
        .as_ref()
        .map(|b| summarize_resolved_build(b));
    let addon_dir = state.addon_dir.clone();
    let token = state.cancel_token.clone();
    let db_clone = state.main.game_db.clone();
    let weights = state.main.weights.clone();
    let chat_balance_ctx = BalanceContext::new(state.main.game_mode.clone());

    std::thread::spawn(move || {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if token.is_cancelled() {
                return;
            }

            let result = (|| -> Result<gw2_optimizer::prompts::GeminiBuildResponse, String> {
                let client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                    .map_err(|e| e.to_string())?;

                if token.is_cancelled() {
                    return Err("Cancelled".into());
                }

                // Use tool-enabled generation if GameDb is available
                if let Some(ref db) = db_clone {
                    let prompt = gw2_optimizer::prompts::chat_refinement_prompt_with_tools(
                        &profession,
                        &message,
                    );
                    let tools = gw2_optimizer::llm::tools::tool_definitions();
                    let empty_candidates = vec![];
                    let ctx = gw2_optimizer::gemini_tools::ToolContext {
                        db,
                        profession_name: &profession,
                        candidates: &empty_candidates,
                        current_build_summary: build_summary.as_deref(),
                        weights: weights.clone(),
                        balance_ctx: &chat_balance_ctx,
                    };

                    let response = client
                        .generate_with_tools_progress(
                            &prompt,
                            &tools,
                            &mut |name: &str, args: &serde_json::Value| {
                                gw2_optimizer::gemini_tools::execute_tool(name, args, &ctx)
                            },
                            8,
                            &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
                                let tools_str = humanize_tool_names(tool_names);
                                crate::state::with_state(|s| {
                                    s.main.optimize_stage = format!(
                                        "AI thinking ({}/{})... {}",
                                        turn, max_turns, tools_str
                                    );
                                });
                            },
                        )
                        .map_err(|e| e.to_string())?;

                    gw2_optimizer::prompts::parse_gemini_build(&response)
                        .map_err(|e| format!("Parse failed: {}", e))
                } else {
                    // Fallback: no GameDb, use simple prompt
                    let build_summary_str = build_summary.as_deref().unwrap_or("");
                    let context =
                        gw2_optimizer::prompts::build_game_context(&profession, &weights, "PvE");
                    let prompt = gw2_optimizer::prompts::chat_refinement_prompt(
                        &profession,
                        build_summary_str,
                        &message,
                        &context,
                    );
                    let response = client.generate_cached(&prompt).map_err(|e| e.to_string())?;
                    gw2_optimizer::prompts::parse_gemini_build(&response)
                        .map_err(|e| format!("Parse failed: {}", e))
                }
            })();

            // Validate Gemini's response against GameDb before applying (if available)
            let validated_result = result.as_ref().ok().and_then(|gemini_build| {
                db_clone.as_ref().map(|db| {
                    let validated = gw2_optimizer::validation::validate_gemini_build(
                        gemini_build,
                        db,
                        &profession,
                    );
                    if !validated.errors.is_empty() {
                        nexus::log::log(
                            nexus::log::LogLevel::Warning,
                            "GW2BuildOpt",
                            &format!(
                                "Chat refinement validation errors: {}",
                                validated.errors.join("; ")
                            ),
                        );
                    }
                    validated
                })
            });
            let _ = validated_result; // Validation logged; apply_gemini_response uses raw parsed fields

            if !token.is_cancelled() {
                crate::state::with_state(|s| match result {
                    Ok(gemini_build) => {
                        let display = if gemini_build.explanation.is_empty() {
                            "Build updated.".to_string()
                        } else {
                            gemini_build.explanation.clone()
                        };
                        crate::ui::chat_bar::add_ai_response(&mut s.main.chat, display);

                        let mut suggestion = crate::ui::comparison::BuildSuggestion {
                            label: "Chat Refinement".into(),
                            ..Default::default()
                        };
                        apply_gemini_response(&mut suggestion, &gemini_build);
                        if let Some(ref db) = s.main.game_db {
                            simulate_suggestion_rotation(&mut suggestion, db);
                        }
                        s.main.comparison.error = None;
                        s.main.comparison.suggestions.push(suggestion);
                        s.main.comparison.selected_suggestion =
                            s.main.comparison.suggestions.len() - 1;
                    }
                    Err(e) => {
                        crate::ui::chat_bar::add_ai_response(
                            &mut s.main.chat,
                            format!("Error: {}", e),
                        );
                    }
                });
            }
        }));
        if panic_result.is_err() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                "bg thread panicked: send_chat_message",
            );
            crate::state::with_state(|s| {
                s.main.chat.waiting = false;
            });
        }
    });
}
