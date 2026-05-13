//! Save/Load tab — saved-build list, save UI, and SavedBuild ↔ BuildSuggestion conversion.

use nexus::imgui::Ui;

use crate::state::{AddonState, MainTab};

use super::super::{build_display, optimization, stats};

/// Render the save build UI (name input + Save button) below the comparison view.
pub(in crate::ui::main_view) fn render_save_build_ui(ui: &Ui, state: &mut AddonState) {
    if state.main.comparison.suggestions.is_empty() {
        return;
    }
    ui.spacing();
    ui.separator();
    ui.text("Save Build:");
    ui.same_line();
    ui.set_next_item_width(200.0);
    ui.input_text("##save_name", &mut state.main.save_name_input)
        .build();
    ui.same_line();

    let can_save = !state.main.save_name_input.trim().is_empty();
    let save_clicked = if can_save {
        ui.button_with_size("Save", [60.0, 0.0])
    } else {
        let style = ui.push_style_var(nexus::imgui::StyleVar::Alpha(0.4));
        ui.button_with_size("Save", [60.0, 0.0]);
        style.pop();
        if ui.is_item_hovered() {
            ui.tooltip_text("Enter a build name first");
        }
        false
    };
    if save_clicked {
        let idx = state
            .main
            .comparison
            .selected_suggestion
            .min(state.main.comparison.suggestions.len().saturating_sub(1));
        let suggestion = &state.main.comparison.suggestions[idx];
        let character_name = state
            .main
            .current_build
            .as_ref()
            .map(|b| b.character_name.clone())
            .unwrap_or_default();
        let profession = state
            .main
            .current_build
            .as_ref()
            .map(|b| b.profession.clone())
            .unwrap_or_default();
        let game_mode = state.main.game_mode.clone();
        // Capture the active balance patch so saved builds remember which
        // patch they were optimized against. Lets the load-side warn the user
        // when a build is loaded under a different patch.
        let balance_ctx = gw2_optimizer::balance::BalanceContext::new(game_mode.clone());
        let saved = suggestion_to_saved(
            &state.main.save_name_input,
            &character_name,
            &profession,
            &game_mode,
            Some(&balance_ctx.patch_id),
            suggestion,
        );

        let storage = gw2_core::storage::BuildStorage::new(&state.addon_dir);
        match storage.save_new(&saved) {
            Ok(()) => {
                state.main.save_status = Some(format!("Saved '{}'", saved.name));
                state.main.save_status_frames = 0;
                state.main.save_name_input.clear();
                state.main.saved_builds_loaded = false; // force refresh
            }
            Err(e) => {
                state.main.save_status = Some(format!("Save failed: {}", e));
                state.main.save_status_frames = 0;
            }
        }
    }

    if let Some(ref status) = state.main.save_status {
        ui.same_line();
        if status.starts_with("Save failed") {
            ui.text_colored([1.0, 0.3, 0.0, 1.0], status);
        } else {
            ui.text_colored([0.0, 1.0, 0.0, 1.0], status);
        }
    }
}

/// Render the Save/Load tab.
pub(in crate::ui::main_view) fn render_saveload_tab(ui: &Ui, state: &mut AddonState) {
    // Lazy-load saved builds on first view
    if !state.main.saved_builds_loaded {
        let storage = gw2_core::storage::BuildStorage::new(&state.addon_dir);
        state.main.saved_builds = storage.list();
        state.main.saved_builds_loaded = true;
    }

    build_display::render_card_header(
        ui,
        &format!("SAVED BUILDS ({})", state.main.saved_builds.len()),
        [1.0, 0.88, 0.35, 1.0],
    );

    if state.main.saved_builds.is_empty() {
        ui.spacing();
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "No saved builds yet.");
        ui.text_colored(
            [0.5, 0.5, 0.5, 1.0],
            "Optimize a build, then use Save to store it here.",
        );
        return;
    }

    // Snapshot for iteration (avoids borrow conflict with mut state)
    let builds_snapshot: Vec<(String, String, String, String, String)> = state
        .main
        .saved_builds
        .iter()
        .map(|b| {
            let time = format_timestamp(b.timestamp);
            let mode = b.game_mode.label().to_string();
            (
                b.name.clone(),
                b.character_name.clone(),
                b.stat_prefix.clone(),
                time,
                mode,
            )
        })
        .collect();

    let mut load_idx: Option<usize> = None;
    let mut request_delete: Option<usize> = None;

    for (i, (name, character, prefix, time, mode)) in builds_snapshot.iter().enumerate() {
        // Name + action buttons on one line
        ui.text_colored([0.6, 0.8, 1.0, 1.0], name);
        ui.same_line();
        if ui.button_with_size(&format!("Load##load_{}", i), [50.0, 0.0]) {
            load_idx = Some(i);
        }
        ui.same_line();

        // Delete with confirmation
        if state.main.confirm_delete == Some(i) {
            ui.text_colored([1.0, 0.3, 0.0, 1.0], "Delete?");
            ui.same_line();
            if ui.small_button(&format!("Yes##confirm_del_{}", i)) {
                request_delete = Some(i);
                state.main.confirm_delete = None;
            }
            ui.same_line();
            if ui.small_button(&format!("No##cancel_del_{}", i)) {
                state.main.confirm_delete = None;
            }
        } else if ui.button_with_size(&format!("Delete##del_{}", i), [50.0, 0.0]) {
            state.main.confirm_delete = Some(i);
        }

        // Details on second line
        ui.text_colored(
            [0.55, 0.55, 0.55, 1.0],
            &format!("  {} | {} | {} | {}", character, mode, prefix, time),
        );

        ui.spacing();
    }

    // Handle load
    if let Some(idx) = load_idx {
        let saved = state.main.saved_builds[idx].clone();
        let db_ref = state.main.game_db.as_ref();
        let mut suggestion = saved_to_suggestion(&saved, db_ref);
        // Run rotation simulation if GameDb is available
        if let Some(ref db) = state.main.game_db {
            optimization::simulate_suggestion_rotation(&mut suggestion, db);
        }
        state.main.comparison.suggestions = vec![suggestion];
        state.main.comparison.selected_suggestion = 0;
        state.main.comparison.error = None;
        state.main.active_tab = MainTab::NewBuild;
    }

    // Handle delete (confirmed)
    if let Some(idx) = request_delete {
        let name = state.main.saved_builds[idx].name.clone();
        let storage = gw2_core::storage::BuildStorage::new(&state.addon_dir);
        match storage.delete(&name) {
            Ok(()) => {
                state.main.saved_builds.remove(idx);
                // Reset confirmation index if it was beyond the removed item
                if let Some(ref mut ci) = state.main.confirm_delete {
                    if *ci > idx {
                        *ci -= 1;
                    } else if *ci == idx {
                        state.main.confirm_delete = None;
                    }
                }
            }
            Err(e) => {
                state.main.error = Some(format!("Delete failed: {}", e));
            }
        }
    }
}

/// Convert a BuildSuggestion to a SavedBuild.
fn suggestion_to_saved(
    name: &str,
    character_name: &str,
    profession: &str,
    game_mode: &gw2_core::types::GameMode,
    balance_manifest_version: Option<&str>,
    suggestion: &crate::ui::comparison::BuildSuggestion,
) -> gw2_core::types::SavedBuild {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    gw2_core::types::SavedBuild {
        name: name.trim().to_string(),
        timestamp,
        character_name: character_name.to_string(),
        game_mode: game_mode.clone(),
        profession: profession.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        balance_manifest_version: balance_manifest_version.map(|s| s.to_string()),
        label: suggestion.label.clone(),
        stat_prefix: suggestion.stat_prefix.clone(),
        specializations: suggestion.specializations.clone(),
        weapons: suggestion.weapons.clone(),
        skills: suggestion.skills.clone(),
        rune: suggestion.rune.clone(),
        sigils: suggestion.sigils.clone(),
        relic: suggestion.relic.clone(),
        explanation: suggestion.explanation.clone(),
        synergy_explanation: suggestion.synergy_explanation.clone(),
        changes_made: suggestion.changes_made.clone(),
        estimated_stats: suggestion.estimated_stats.clone(),
    }
}

/// Convert a SavedBuild back to a BuildSuggestion for display.
/// Recomputes combat metrics from estimated stats if available.
/// When `game_db` is provided, reconstructs DamageModifiers from saved
/// spec/trait/rune/sigil/relic names for accurate combat metric recomputation.
fn saved_to_suggestion(
    saved: &gw2_core::types::SavedBuild,
    game_db: Option<&gw2_optimizer::gamedb::GameDb>,
) -> crate::ui::comparison::BuildSuggestion {
    // Determine profession — fallback to "Warrior" for pre-P3-16 saves
    let profession = if saved.profession.is_empty() {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            "Loaded build with empty profession — falling back to Warrior",
        );
        "Warrior"
    } else {
        &saved.profession
    };

    // Reconstruct DamageModifiers from saved build config if GameDb is available.
    let ctx = gw2_optimizer::balance::BalanceContext::new(saved.game_mode.clone());
    let mods = game_db
        .map(|db| reconstruct_damage_modifiers(saved, db, &ctx))
        .unwrap_or_default();

    // Recompute combat metrics from saved stats (lossy i32→f64 but good enough for display)
    let (combat_solo, combat_party, combat_squad) = saved
        .estimated_stats
        .as_ref()
        .map(|est| {
            let stats = gw2_optimizer::stats::StatBlock {
                power: est.power as f64,
                precision: est.precision as f64,
                toughness: est.toughness as f64,
                vitality: est.vitality as f64,
                condition_damage: est.condition_damage as f64,
                expertise: est.expertise as f64,
                concentration: est.concentration as f64,
                ferocity: est.ferocity as f64,
                healing_power: est.healing_power as f64,
            };
            let derived = gw2_optimizer::stats::compute_derived(&stats, profession);
            stats::compute_3tier_combat(&stats, &derived, &mods, profession, &ctx)
        })
        .unwrap_or((None, None, None));

    crate::ui::comparison::BuildSuggestion {
        label: if saved.label.is_empty() {
            saved.name.clone()
        } else {
            saved.label.clone()
        },
        build_summary: String::new(),
        stat_prefix: saved.stat_prefix.clone(),
        specializations: saved.specializations.clone(),
        weapons: saved.weapons.clone(),
        skills: saved.skills.clone(),
        rune: saved.rune.clone(),
        sigils: saved.sigils.clone(),
        relic: saved.relic.clone(),
        chat_code: None,
        explanation: saved.explanation.clone(),
        synergy_explanation: saved.synergy_explanation.clone(),
        changes_made: saved.changes_made.clone(),
        estimated_stats: saved.estimated_stats.clone(),
        combat_solo,
        combat_party,
        combat_squad,
        rotation: None,
        viability: None,
        benchmark_delta: None,
        data_quality: gw2_optimizer::data::DataQuality::Verified,
        quality_reasons: vec![],
    }
}

/// Reconstruct DamageModifiers from a saved build by resolving spec/trait/rune/sigil/relic
/// names against GameDb. Unresolvable entities are skipped with a warning.
fn reconstruct_damage_modifiers(
    saved: &gw2_core::types::SavedBuild,
    db: &gw2_optimizer::gamedb::GameDb,
    ctx: &gw2_optimizer::balance::BalanceContext,
) -> gw2_optimizer::combat::DamageModifiers {
    let mut equipped_trait_ids: Vec<u32> = Vec::new();

    // Resolve specialization + trait names to IDs.
    // Match case-insensitively so old/edited save files with drifted casing
    // still resolve. find() returns one hit at most for exact-name lookup, so
    // HashMap iteration order doesn't matter here.
    for (spec_name, trait_names) in &saved.specializations {
        let spec = db
            .specializations
            .values()
            .find(|s| s.name.eq_ignore_ascii_case(spec_name));
        let Some(spec) = spec else {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                &format!(
                    "Could not resolve spec '{}' for modifier reconstruction — skipping",
                    spec_name
                ),
            );
            continue;
        };

        for trait_name in trait_names {
            let trait_id = db.traits_by_spec.get(&spec.id).and_then(|ids| {
                ids.iter()
                    .filter_map(|id| db.traits.get(id))
                    .find(|t| t.name.eq_ignore_ascii_case(trait_name))
                    .map(|t| t.id)
            });
            match trait_id {
                Some(id) => equipped_trait_ids.push(id),
                None => {
                    nexus::log::log(
                        nexus::log::LogLevel::Warning,
                        "GW2BuildOpt",
                        &format!(
                            "Could not resolve trait '{}' in spec '{}' — skipping",
                            trait_name, spec_name
                        ),
                    );
                }
            }
        }
    }

    // Resolve rune name to ID (case-insensitive)
    let rune_id = if !saved.rune.is_empty() {
        let found = db
            .runes
            .iter()
            .filter_map(|id| db.items.get(id))
            .find(|item| item.name.eq_ignore_ascii_case(&saved.rune))
            .map(|item| item.id);
        if found.is_none() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                &format!("Could not resolve rune '{}' — skipping", saved.rune),
            );
        }
        found
    } else {
        None
    };

    // Resolve sigil names to IDs
    let sigil_ids: Vec<u32> = saved
        .sigils
        .iter()
        .filter_map(|name| {
            if name.is_empty() {
                return None;
            }
            let found = db
                .sigils
                .iter()
                .filter_map(|id| db.items.get(id))
                .find(|item| item.name.eq_ignore_ascii_case(name))
                .map(|item| item.id);
            if found.is_none() {
                nexus::log::log(
                    nexus::log::LogLevel::Warning,
                    "GW2BuildOpt",
                    &format!("Could not resolve sigil '{}' — skipping", name),
                );
            }
            found
        })
        .collect();

    // Resolve relic name to ID (case-insensitive)
    let relic_id = if !saved.relic.is_empty() {
        let found = db
            .relics
            .iter()
            .filter_map(|id| db.items.get(id))
            .find(|item| item.name.eq_ignore_ascii_case(&saved.relic))
            .map(|item| item.id);
        if found.is_none() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                &format!("Could not resolve relic '{}' — skipping", saved.relic),
            );
        }
        found
    } else {
        None
    };

    gw2_optimizer::combat::extract_damage_modifiers(
        &equipped_trait_ids,
        rune_id,
        &sigil_ids,
        relic_id,
        &db.traits,
        &db.items,
        ctx,
    )
}

/// Format a Unix timestamp as a readable date+time string (YYYY-MM-DD HH:MM).
fn format_timestamp(timestamp: u64) -> String {
    let secs_per_day: u64 = 86400;
    let day_secs = timestamp % secs_per_day;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let days = timestamp / secs_per_day;
    // Days since epoch to Y/M/D
    let mut y = 1970u64;
    let mut remaining_days = days;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days_in_months: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 11;
    for (i, &dim) in days_in_months.iter().enumerate() {
        if remaining_days < dim {
            m = i;
            break;
        }
        remaining_days -= dim;
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        y,
        m + 1,
        remaining_days + 1,
        hours,
        minutes
    )
}

#[cfg(test)]
mod tests {
    fn build_saved_for_modifier_reconstruction() -> gw2_core::types::SavedBuild {
        gw2_core::types::SavedBuild {
            name: "test-save".into(),
            timestamp: 0,
            character_name: "Test Character".into(),
            game_mode: gw2_core::types::GameMode::PvE,
            profession: "Warrior".into(),
            engine_version: "test".into(),
            balance_manifest_version: None,
            label: "Test Build".into(),
            stat_prefix: "Viper's".into(),
            specializations: vec![("Test Spec".into(), vec!["Test Condition Trait".into()])],
            weapons: vec![],
            skills: vec![],
            rune: "Superior Rune of Test".into(),
            sigils: vec!["Superior Sigil of Bursting".into()],
            relic: "Relic of the Nightmare".into(),
            explanation: String::new(),
            synergy_explanation: String::new(),
            changes_made: vec![],
            estimated_stats: Some(gw2_core::types::StatBlock {
                power: 1800,
                precision: 1800,
                toughness: 1200,
                vitality: 1200,
                condition_damage: 1800,
                expertise: 0,
                concentration: 0,
                ferocity: 0,
                healing_power: 0,
                crit_chance: 0.0,
                crit_damage: 0.0,
                health: 0,
                armor: 0,
            }),
        }
    }

    fn build_test_gamedb_for_modifier_reconstruction() -> gw2_optimizer::gamedb::GameDb {
        let trait_id = 1001u32;
        let spec_id = 5001u32;
        let rune_id = 2001u32;
        let sigil_id = 2002u32;
        let relic_id = 2003u32;

        let mut traits = std::collections::HashMap::new();
        traits.insert(
            trait_id,
            gw2_api::models::Trait {
                id: trait_id,
                name: "Test Condition Trait".into(),
                icon: None,
                description: None,
                specialization: spec_id,
                tier: 1,
                order: 0,
                slot: "Major".into(),
                facts: vec![gw2_api::models::Fact::Percent {
                    text: Some("Increase condition damage by 20%".into()),
                    icon: None,
                    percent: Some(20.0),
                }],
                traited_facts: vec![],
                skills: vec![],
            },
        );

        let mut specializations = std::collections::HashMap::new();
        specializations.insert(
            spec_id,
            gw2_api::models::Specialization {
                id: spec_id,
                name: "Test Spec".into(),
                profession: "Warrior".into(),
                elite: false,
                minor_traits: vec![],
                major_traits: vec![trait_id],
                weapon_trait: None,
                icon: None,
                background: None,
                profession_icon: None,
                profession_icon_big: None,
            },
        );

        let mut items = std::collections::HashMap::new();
        items.insert(
            rune_id,
            gw2_api::models::Item {
                id: rune_id,
                name: "Superior Rune of Test".into(),
                description: None,
                icon: None,
                item_type: "UpgradeComponent".into(),
                rarity: "Exotic".into(),
                level: 80,
                vendor_value: None,
                chat_link: None,
                default_skin: None,
                flags: vec![],
                game_types: vec![],
                restrictions: vec![],
                details: Some(gw2_api::models::ItemDetails {
                    detail_type: Some("UpgradeComponent".into()),
                    weight_class: None,
                    defense: None,
                    damage_type: None,
                    min_power: None,
                    max_power: None,
                    suffix: None,
                    bonuses: vec!["+20% Condition Duration".into()],
                    infusion_upgrade_flags: vec![],
                    infusion_slots: vec![],
                    attribute_adjustment: None,
                    infix_upgrade: None,
                    suffix_item_id: None,
                    secondary_suffix_item_id: None,
                    stat_choices: vec![],
                }),
            },
        );
        items.insert(
            sigil_id,
            gw2_api::models::Item {
                id: sigil_id,
                name: "Superior Sigil of Bursting".into(),
                description: None,
                icon: None,
                item_type: "UpgradeComponent".into(),
                rarity: "Exotic".into(),
                level: 80,
                vendor_value: None,
                chat_link: None,
                default_skin: None,
                flags: vec![],
                game_types: vec![],
                restrictions: vec![],
                details: None,
            },
        );
        items.insert(
            relic_id,
            gw2_api::models::Item {
                id: relic_id,
                name: "Relic of the Nightmare".into(),
                description: None,
                icon: None,
                item_type: "Relic".into(),
                rarity: "Exotic".into(),
                level: 80,
                vendor_value: None,
                chat_link: None,
                default_skin: None,
                flags: vec![],
                game_types: vec![],
                restrictions: vec![],
                details: None,
            },
        );

        let mut traits_by_spec = std::collections::HashMap::new();
        traits_by_spec.insert(spec_id, vec![trait_id]);

        gw2_optimizer::gamedb::GameDb {
            items,
            itemstats: std::collections::HashMap::new(),
            skills: std::collections::HashMap::new(),
            traits,
            specializations,
            professions: std::collections::HashMap::new(),
            legends: std::collections::HashMap::new(),
            pvp_amulets: std::collections::HashMap::new(),
            skills_by_profession: std::collections::HashMap::new(),
            traits_by_spec,
            items_by_type: std::collections::HashMap::new(),
            runes: vec![rune_id],
            sigils: vec![sigil_id],
            relics: vec![relic_id],
            skill_to_palette: std::collections::HashMap::new(),
            palette_to_skill: std::collections::HashMap::new(),
            traits_by_condition: std::collections::HashMap::new(),
            skills_by_condition: std::collections::HashMap::new(),
            traits_by_buff: std::collections::HashMap::new(),
            skills_by_buff: std::collections::HashMap::new(),
        }
    }

    fn contains_approx(values: &[f64], expected: f64) -> bool {
        values.iter().any(|v| (v - expected).abs() < 1e-9)
    }

    #[test]
    fn test_reconstruct_damage_modifiers_resolves_saved_entities() {
        let saved = build_saved_for_modifier_reconstruction();
        let db = build_test_gamedb_for_modifier_reconstruction();
        let ctx = gw2_optimizer::balance::BalanceContext::new(saved.game_mode.clone());

        let mods = super::reconstruct_damage_modifiers(&saved, &db, &ctx);

        assert!(
            contains_approx(&mods.condition_pct, 0.20),
            "expected trait-based +20% condition damage to be reconstructed"
        );
        assert!(
            contains_approx(&mods.condition_pct, 0.06),
            "expected sigil-based +6% condition damage to be reconstructed"
        );
        assert!(
            contains_approx(&mods.condi_duration_pct, 0.20),
            "expected rune-based +20% condition duration to be reconstructed"
        );
        assert!(
            contains_approx(&mods.condi_duration_pct, 0.10),
            "expected relic-based +10% condition duration to be reconstructed"
        );
    }

    #[test]
    fn test_saved_to_suggestion_load_path_uses_reconstructed_modifiers() {
        let saved = build_saved_for_modifier_reconstruction();
        let db = build_test_gamedb_for_modifier_reconstruction();

        let without_db = super::saved_to_suggestion(&saved, None);
        let with_db = super::saved_to_suggestion(&saved, Some(&db));

        let without_solo = without_db
            .combat_solo
            .expect("saved test fixture should produce combat metrics without GameDb");
        let with_solo = with_db
            .combat_solo
            .expect("saved test fixture should produce combat metrics with GameDb");

        assert!(
            with_solo.condition_dps_index > without_solo.condition_dps_index,
            "load path with GameDb should reconstruct condition modifiers instead of defaulting"
        );
        assert!(
            with_solo.total_dps_index > without_solo.total_dps_index,
            "total DPS should reflect reconstructed modifiers on load"
        );
    }
}
