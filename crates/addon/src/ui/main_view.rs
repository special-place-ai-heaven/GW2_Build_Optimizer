use nexus::imgui::{ChildWindow, ComboBox, Selectable, Ui};

use crate::state::{AddonState, MainTab};
use gw2_optimizer::scoring::Archetype;

mod build_display;

pub fn render_main(ui: &Ui, state: &mut AddonState) {
    // Trigger character load on first render
    if state.main.characters.is_empty() && !state.main.characters_loading {
        load_characters(state);
    }

    // Load GameDb once on first entry (S11-T06)
    if state.main.game_db.is_none() && !state.main.game_db_loading {
        load_game_db(state);
    }

    // Left menu panel (fixed width)
    let menu_width = 180.0;

    ChildWindow::new("##left_menu")
        .size([menu_width, 0.0])
        .build(ui, || {
            render_left_menu(ui, state);
        });

    ui.same_line();

    // Main content area (fills remaining width)
    ChildWindow::new("##main_content")
        .size([0.0, 0.0])
        .build(ui, || {
            render_main_content(ui, state);
        });
}

fn render_left_menu(ui: &Ui, state: &mut AddonState) {
    // Character dropdown
    ui.text("Character:");
    ui.set_next_item_width(-1.0);

    let preview = state
        .main
        .selected_character
        .and_then(|i| state.main.characters.get(i))
        .cloned()
        .unwrap_or_else(|| {
            if state.main.characters_loading {
                "Loading...".into()
            } else if state.main.characters.is_empty() {
                "No characters".into()
            } else {
                "Select...".into()
            }
        });

    let chars_snapshot = state.main.characters.clone();
    let mut new_selection: Option<(usize, String)> = None;

    if let Some(_combo) = ComboBox::new("##char_select").preview_value(&preview).begin(ui) {
        for (i, name) in chars_snapshot.iter().enumerate() {
            let selected = state.main.selected_character == Some(i);
            if Selectable::new(name).selected(selected).build(ui) {
                if state.main.selected_character != Some(i) {
                    new_selection = Some((i, name.clone()));
                }
            }
        }
    }

    if let Some((idx, name)) = new_selection {
        state.main.selected_character = Some(idx);
        state.main.current_build = None;
        state.main.current_stats = None;
        load_character_build(state, name);
    }

    ui.spacing();
    ui.separator();
    ui.spacing();

    // Game mode selector
    ui.text("Game Mode:");
    for mode in &gw2_core::types::GameMode::ALL {
        let selected = state.main.game_mode == *mode;
        if ui.radio_button_bool(mode.label(), selected) {
            state.main.game_mode = mode.clone();
            if let Some(idx) = state.main.selected_character {
                if let Some(name) = state.main.characters.get(idx).cloned() {
                    load_character_build(state, name);
                }
            }
        }
    }

    ui.spacing();
    ui.separator();
    ui.spacing();

    // Menu tabs
    let tabs = [
        (MainTab::NewBuild, "New Build"),
        (MainTab::Improve, "Improve Character"),
        (MainTab::SaveLoad, "Save / Load"),
        (MainTab::Settings, "Settings"),
    ];

    for (tab, label) in &tabs {
        let selected = state.main.active_tab == *tab;
        if Selectable::new(label).selected(selected).build(ui) {
            state.main.active_tab = tab.clone();
        }
    }

    // Refresh button at bottom
    ui.spacing();
    ui.separator();
    ui.spacing();
    if ui.button_with_size("Refresh Data", [-1.0, 0.0]) {
        load_characters(state);
    }

    if let Some(ref err) = state.main.error {
        ui.spacing();
        ui.text_colored([1.0, 0.3, 0.0, 1.0], "Error:");
        ui.text_wrapped(err);
    }
}

fn render_main_content(ui: &Ui, state: &mut AddonState) {
    match state.main.active_tab {
        MainTab::NewBuild => {
            render_new_build_tab(ui, state);
        }
        MainTab::Improve => {
            render_improve_tab(ui, state);
        }
        MainTab::SaveLoad => {
            ui.text("Save / Load");
            ui.separator();
            ui.text_wrapped("Build saving and loading coming soon.");
            ui.text_wrapped("Saved builds will appear here organized by character.");
        }
        MainTab::Settings => {
            render_settings_tab(ui, state);
        }
    }
}

fn render_new_build_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.selected_character.is_none() {
        ui.text("Select a character from the left menu to create a new build.");
        return;
    }

    ui.text("New Build");
    ui.separator();

    // Show optimization progress (S11-T05)
    if state.main.optimizing {
        ui.text_colored([1.0, 1.0, 0.0, 1.0], &format!("Optimizing: {}", state.main.optimize_stage));
        ui.spacing();
    }

    // Archetype selector
    ui.text("Select build archetype:");
    ui.spacing();
    for archetype in &Archetype::ALL {
        if ui.button_with_size(archetype.label(), [200.0, 0.0]) {
            start_optimization(state, archetype.clone(), None);
        }
        ui.spacing();
    }

    // Show comparison if suggestions exist
    if !state.main.comparison.suggestions.is_empty() {
        ui.separator();
        if let Some(ref build) = state.main.current_build {
            let stats = state.main.current_stats.clone();
            crate::ui::comparison::render_comparison(
                ui,
                build,
                stats.as_ref(),
                &state.main.comparison,
            );
        }
    }

    // Chat bar at bottom
    ui.spacing();
    if let Some(msg) = crate::ui::chat_bar::render_chat_bar(ui, &mut state.main.chat) {
        state.main.chat.waiting = true;
        send_chat_message(state, msg);
    }
}

fn render_improve_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.build_loading {
        ui.text("Loading build...");
        return;
    }

    // Collect data for optimization before borrowing build
    let profession_name = state.main.current_build.as_ref().map(|b| b.profession.clone());
    let stats_snapshot = state.main.current_stats.clone();
    let archetype = state.main.current_build.as_ref()
        .map(|b| infer_archetype_from_build(b, stats_snapshot.as_ref()))
        .unwrap_or(Archetype::PowerDPS);

    // Check button press in separate scope to avoid borrow conflict
    let should_optimize = state.main.current_build.is_some() && ui.button_with_size("Improve This Build", [200.0, 30.0]);

    if let Some(ref build) = state.main.current_build {
        let stats = state.main.current_stats.clone();

        // Show current build
        build_display::render_build(ui, build, stats.as_ref());

        ui.spacing();
        ui.separator();

        // Show optimization progress (S11-T05)
        if state.main.optimizing {
            ui.text_colored([1.0, 1.0, 0.0, 1.0], &format!("Optimizing: {}", state.main.optimize_stage));
            ui.spacing();
        }
    }

    // Handle optimization after the build borrow ends
    if should_optimize {
        if let Some(ref prof_name) = profession_name {
            start_optimization_with_profession(state, archetype, prof_name);
        }
    }

    // Show comparison if suggestions exist (separate from build display)
    if !state.main.comparison.suggestions.is_empty() {
        if let Some(ref build) = state.main.current_build {
            let stats = state.main.current_stats.clone();
            ui.spacing();
            crate::ui::comparison::render_comparison(
                ui,
                build,
                stats.as_ref(),
                &state.main.comparison,
            );
        }
    }

    // Chat bar at bottom
    if state.main.current_build.is_some() {
        ui.spacing();
        if let Some(msg) = crate::ui::chat_bar::render_chat_bar(ui, &mut state.main.chat) {
            state.main.chat.waiting = true;
            send_chat_message(state, msg);
        }
    } else if state.main.selected_character.is_some() {
        ui.text("Loading character build...");
    } else {
        ui.text("Select a character from the left menu.");
    }
}

fn render_settings_tab(ui: &Ui, state: &mut AddonState) {
    ui.text("Settings");
    ui.separator();
    ui.spacing();

    // API Keys
    if let Some(ref key) = state.config.gw2_api_key {
        let display = if key.len() > 12 {
            format!("{}...{}", &key[..8], &key[key.len() - 4..])
        } else {
            "****".into()
        };
        ui.text(&format!("GW2 API Key: {}", display));
    }
    if state.config.gemini_api_key.is_some() {
        ui.text("Gemini API Key: configured");
    }
    if let Some(build) = state.config.cache_build_number {
        ui.text(&format!("Cached game build: {}", build));
    }

    ui.spacing();
    ui.separator();
    ui.spacing();

    // Cache management
    if ui.button_with_size("Refresh Game Data", [200.0, 0.0]) {
        nexus::log::log(
            nexus::log::LogLevel::Info,
            "GW2 Build Optimizer",
            "Refreshing game data cache...",
        );
    }

    ui.spacing();

    if ui.button_with_size("Reset Setup", [200.0, 0.0]) {
        state.screen = crate::state::Screen::Setup(crate::state::SetupStep::Gw2ApiKey);
    }

    ui.spacing();
    ui.separator();
    ui.spacing();

    // About
    ui.text("GW2 Build Optimizer v0.1.0");
    ui.text("Powered by Google Gemini AI");
    ui.text_wrapped("Optimizes builds using GW2 API data and LLM reasoning about trait/sigil/rune/relic synergies.");
}

fn load_characters(state: &mut AddonState) {
    let Some(ref key) = state.config.gw2_api_key else {
        state.main.error = Some("No GW2 API key configured".into());
        return;
    };

    state.main.characters_loading = true;
    state.main.error = None;
    let key = key.clone();

    std::thread::spawn(move || {
        let result = gw2_api::client::Gw2Client::with_key(&key)
            .and_then(|c| c.fetch_characters());

        crate::state::with_state(|s| {
            s.main.characters_loading = false;
            match result {
                Ok(chars) => s.main.characters = chars,
                Err(e) => s.main.error = Some(e.to_string()),
            }
        });
    });
}

fn load_character_build(state: &mut AddonState, character_name: String) {
    let Some(ref key) = state.config.gw2_api_key else {
        return;
    };

    state.main.build_loading = true;
    state.main.error = None;
    let key = key.clone();
    let cache_dir = state.addon_dir.join("cache");

    std::thread::spawn(move || {
        let result = (|| -> Result<gw2_core::types::ResolvedBuild, String> {
            let client = gw2_api::client::Gw2Client::with_key(&key)
                .map_err(|e| e.to_string())?;

            let build_tabs = client
                .fetch_build_tabs(&character_name)
                .map_err(|e| e.to_string())?;

            let equipment_tabs = client
                .fetch_equipment_tabs(&character_name)
                .map_err(|e| e.to_string())?;

            let active_build = build_tabs
                .iter()
                .find(|t| t.is_active)
                .or(build_tabs.first())
                .ok_or("No build tabs found")?;

            let active_equipment = equipment_tabs
                .iter()
                .find(|t| t.is_active)
                .or(equipment_tabs.first())
                .ok_or("No equipment tabs found")?;

            let cache = gw2_api::cache::DataCache::new(&cache_dir);
            resolve_build(
                &character_name,
                &active_build.build,
                active_equipment,
                &cache,
            )
        })();

        crate::state::with_state(|s| {
            s.main.build_loading = false;
            match result {
                Ok(build) => {
                    s.main.current_build = Some(build);
                    s.main.current_stats = None; // S06 will calculate
                }
                Err(e) => s.main.error = Some(e),
            }
        });
    });
}

fn resolve_build(
    character_name: &str,
    build: &gw2_api::models::Build,
    equipment: &gw2_api::models::EquipmentTab,
    cache: &gw2_api::cache::DataCache,
) -> Result<gw2_core::types::ResolvedBuild, String> {
    use gw2_core::types::*;

    let specs: Vec<gw2_api::models::Specialization> = cache
        .load("specializations").map_err(|e| e.to_string())?.unwrap_or_default();
    let traits: Vec<gw2_api::models::Trait> = cache
        .load("traits").map_err(|e| e.to_string())?.unwrap_or_default();
    let skills: Vec<gw2_api::models::Skill> = cache
        .load("skills").map_err(|e| e.to_string())?.unwrap_or_default();
    let items: Vec<gw2_api::models::Item> = cache
        .load("items").map_err(|e| e.to_string())?.unwrap_or_default();

    let find_item = |id: u32| -> Option<&gw2_api::models::Item> {
        items.iter().find(|i| i.id == id)
    };
    let find_skill = |id: u32| -> Option<SkillInfo> {
        skills.iter().find(|s| s.id == id).map(|s| SkillInfo {
            id: s.id,
            name: s.name.clone(),
        })
    };

    // Resolve specializations
    let resolved_specs: Vec<ResolvedSpec> = build
        .specializations
        .iter()
        .filter_map(|sel| {
            let spec_id = sel.id?;
            let spec = specs.iter().find(|s| s.id == spec_id)?;
            let traits_selected: Vec<ResolvedTrait> = sel.traits.iter().enumerate()
                .filter_map(|(col, trait_id)| {
                    let tid = (*trait_id)?;
                    let t = traits.iter().find(|t| t.id == tid)?;
                    Some(ResolvedTrait {
                        id: t.id, name: t.name.clone(),
                        description: t.description.clone().unwrap_or_default(),
                        column: col, selected: true,
                    })
                }).collect();
            Some(ResolvedSpec {
                id: spec.id, name: spec.name.clone(), elite: spec.elite,
                traits_selected, traits_available: Vec::new(),
            })
        }).collect();

    // Resolve skills
    let resolved_skills = if let Some(ref sk) = build.skills {
        ResolvedSkills {
            heal: sk.heal.and_then(&find_skill),
            utilities: sk.utilities.iter().map(|id| id.and_then(&find_skill)).collect(),
            elite: sk.elite.and_then(&find_skill),
        }
    } else {
        ResolvedSkills::default()
    };

    // Resolve equipment
    let mut weapons = Vec::new();
    let mut armor = Vec::new();
    let mut trinkets_vec = Vec::new();
    let mut rune = None;
    let mut relic_resolved = None;
    let mut ws1 = ResolvedWeaponSet { label: "Set 1".into(), ..Default::default() };
    let mut ws2 = ResolvedWeaponSet { label: "Set 2".into(), ..Default::default() };

    let itemstats: Vec<gw2_api::models::ItemStat> = cache
        .load("itemstats").ok().flatten().unwrap_or_default();

    for piece in &equipment.equipment {
        let item = find_item(piece.id);
        let item_name = item.map(|i| i.name.clone()).unwrap_or_else(|| format!("#{}", piece.id));
        let stat_prefix = piece.stats.as_ref()
            .and_then(|s| itemstats.iter().find(|is| is.id == s.id).map(|is| is.name.clone()))
            .unwrap_or_default();

        let extract_sigils = |piece: &gw2_api::models::EquipmentPiece, ws: &mut ResolvedWeaponSet| {
            for &uid in &piece.upgrades {
                if let Some(u) = find_item(uid) {
                    ws.sigils.push(UpgradeInfo { id: uid, name: u.name.clone() });
                }
            }
        };

        match piece.slot.as_str() {
            "WeaponA1" => {
                ws1.main_hand = Some(WeaponInfo { name: item_name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.clone()).unwrap_or_default() });
                extract_sigils(piece, &mut ws1);
            }
            "WeaponA2" => {
                ws1.off_hand = Some(WeaponInfo { name: item_name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.clone()).unwrap_or_default() });
                extract_sigils(piece, &mut ws1);
            }
            "WeaponB1" => {
                ws2.main_hand = Some(WeaponInfo { name: item_name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.clone()).unwrap_or_default() });
                extract_sigils(piece, &mut ws2);
            }
            "WeaponB2" => {
                ws2.off_hand = Some(WeaponInfo { name: item_name, weapon_type: item.and_then(|i| i.details.as_ref()?.detail_type.clone()).unwrap_or_default() });
                extract_sigils(piece, &mut ws2);
            }
            "Helm" | "Shoulders" | "Coat" | "Gloves" | "Leggings" | "Boots" => {
                if rune.is_none() {
                    if let Some(&uid) = piece.upgrades.first() {
                        if let Some(u) = find_item(uid) {
                            rune = Some(ResolvedUpgrade { id: uid, name: u.name.clone() });
                        }
                    }
                }
                armor.push(ResolvedGearPiece { slot: piece.slot.clone(), name: item_name, stat_prefix, infusions: Vec::new() });
            }
            "Backpack" | "Accessory1" | "Accessory2" | "Amulet" | "Ring1" | "Ring2" => {
                trinkets_vec.push(ResolvedGearPiece { slot: piece.slot.clone(), name: item_name, stat_prefix, infusions: Vec::new() });
            }
            "Relic" => {
                relic_resolved = Some(ResolvedRelic {
                    id: piece.id, name: item_name,
                    description: item.and_then(|i| i.description.clone()).unwrap_or_default(),
                });
            }
            _ => {}
        }
    }

    if ws1.main_hand.is_some() || ws1.off_hand.is_some() { weapons.push(ws1); }
    if ws2.main_hand.is_some() || ws2.off_hand.is_some() { weapons.push(ws2); }

    Ok(ResolvedBuild {
        character_name: character_name.to_string(),
        profession: build.profession.clone().unwrap_or_default(),
        game_mode: GameMode::default(),
        specializations: resolved_specs,
        skills: resolved_skills,
        weapons, armor, trinkets: trinkets_vec,
        relic: relic_resolved, rune,
        pvp_amulet: None,
    })
}

/// Load GameDb once on main screen entry (S11-T06)
fn load_game_db(state: &mut AddonState) {
    state.main.game_db_loading = true;
    let cache_dir = state.addon_dir.join("cache");

    std::thread::spawn(move || {
        let cache = gw2_api::cache::DataCache::new(&cache_dir);
        let result = gw2_optimizer::gamedb::GameDb::load(&cache);

        crate::state::with_state(|s| {
            s.main.game_db_loading = false;
            match result {
                Ok(db) => {
                    nexus::log::log(
                        nexus::log::LogLevel::Info,
                        "GW2 Build Optimizer",
                        &db.summary(),
                    );
                    s.main.game_db = Some(db);
                }
                Err(e) => {
                    s.main.error = Some(format!("Failed to load game data: {}", e));
                }
            }
        });
    });
}

/// Start optimization in background thread (S11-T01, S11-T02, S11-T03)
fn start_optimization(state: &mut AddonState, archetype: Archetype, _current_build: Option<&gw2_core::types::ResolvedBuild>) {
    // Get profession from current build
    let profession_name = state.main.current_build.as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();

    start_optimization_with_profession(state, archetype, &profession_name);
}

/// Start optimization with explicit profession name (avoids borrow conflicts)
fn start_optimization_with_profession(state: &mut AddonState, archetype: Archetype, profession_name: &str) {
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
    let gemini_key = state.config.gemini_api_key.clone();
    let game_mode_label = state.main.game_mode.label().to_string();
    let current_build_summary = state.main.current_build.as_ref()
        .map(|b| summarize_resolved_build(b));
    let addon_dir = state.addon_dir.clone();

    state.main.optimizing = true;
    state.main.optimize_stage = "Starting...".into();
    state.main.comparison.suggestions.clear();
    state.main.comparison.loading = true;
    state.main.comparison.error = None;

    std::thread::spawn(move || {
        let result = (|| -> Result<Vec<crate::ui::comparison::BuildSuggestion>, String> {
            let db = db.ok_or("GameDb not loaded")?;
            let profession = db.profession(&profession_name)
                .ok_or_else(|| format!("Profession {} not found", profession_name))?;

            let candidates = gw2_optimizer::engine::optimize(
                profession,
                &archetype,
                None,
                &db.items,
                &db.itemstats,
                &db.specializations,
                &db.traits,
                |progress| {
                    crate::state::with_state(|s| {
                        s.main.optimize_stage = progress.stage.clone();
                    });
                },
                5,
            );

            let mut suggestions: Vec<crate::ui::comparison::BuildSuggestion> =
                candidates.iter().map(|c| candidate_to_suggestion(c, &db)).collect();

            // Enrich top suggestion with Gemini LLM reasoning
            if let Some(ref key) = gemini_key {
                crate::state::with_state(|s| {
                    s.main.optimize_stage = "Consulting Gemini for synergy analysis...".into();
                });

                match enrich_with_gemini(
                    key,
                    &profession_name,
                    &archetype,
                    &game_mode_label,
                    &candidates,
                    &db,
                    current_build_summary.as_deref(),
                    &mut suggestions,
                    &addon_dir,
                ) {
                    Ok(()) => {}
                    Err(e) => {
                        nexus::log::log(
                            nexus::log::LogLevel::Warning,
                            "GW2 Build Optimizer",
                            &format!("Gemini enrichment skipped: {}", e),
                        );
                    }
                }
            }

            Ok(suggestions)
        })();

        crate::state::with_state(|s| {
            s.main.optimizing = false;
            s.main.comparison.loading = false;
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
    });
}

/// Convert BuildCandidate to BuildSuggestion for display (S11-T04)
fn candidate_to_suggestion(
    candidate: &gw2_optimizer::engine::BuildCandidate,
    db: &gw2_optimizer::gamedb::GameDb,
) -> crate::ui::comparison::BuildSuggestion {
    use crate::ui::comparison::BuildSuggestion;

    // Get spec names
    let mut specializations = Vec::new();
    if let Some(elite_id) = candidate.elite_spec {
        if let Some(spec) = db.spec(elite_id) {
            let traits: Vec<String> = spec.major_traits.iter()
                .take(3)
                .filter_map(|&tid| db.traits.get(&tid).map(|t| t.name.clone()))
                .collect();
            specializations.push((format!("{} [E]", spec.name), traits));
        }
    }
    for &core_id in &candidate.core_specs {
        if let Some(spec) = db.spec(core_id) {
            let traits: Vec<String> = spec.major_traits.iter()
                .take(3)
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

    BuildSuggestion {
        label: format!("Score: {:.2}", candidate.score),
        build_summary: format!("Gear: {}", candidate.gear.stat_prefix_name),
        stat_prefix: candidate.gear.stat_prefix_name.clone(),
        specializations,
        weapons: Vec::new(), // Not determined by optimizer yet
        skills: Vec::new(),  // Not determined by optimizer yet
        rune: String::new(), // Not determined by optimizer yet
        sigils: Vec::new(),  // Not determined by optimizer yet
        relic: String::new(), // Not determined by optimizer yet
        explanation: String::new(), // LLM will fill this in S08
        changes_made: Vec::new(),
        estimated_stats,
    }
}

/// Infer archetype from current build stats.
fn infer_archetype_from_build(_build: &gw2_core::types::ResolvedBuild, stats: Option<&gw2_core::types::StatBlock>) -> Archetype {
    let Some(stats) = stats else {
        return Archetype::PowerDPS;
    };

    let max_stat = [
        (stats.power, Archetype::PowerDPS),
        (stats.condition_damage, Archetype::ConditionDPS),
        (stats.healing_power, Archetype::HealSupport),
        (stats.concentration, Archetype::BoonSupport),
        (stats.toughness + stats.vitality, Archetype::Tank),
    ].iter().max_by_key(|(v, _)| v).map(|(_, a)| a.clone());

    max_stat.unwrap_or(Archetype::PowerDPS)
}

/// Summarize a ResolvedBuild as text for LLM prompts.
fn summarize_resolved_build(build: &gw2_core::types::ResolvedBuild) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Profession: {}", build.profession));

    let specs: Vec<String> = build.specializations.iter()
        .map(|s| {
            let elite = if s.elite { " [E]" } else { "" };
            let traits: Vec<&str> = s.traits_selected.iter().map(|t| t.name.as_str()).collect();
            format!("{}{}: {}", s.name, elite, traits.join(", "))
        }).collect();
    if !specs.is_empty() {
        parts.push(format!("Specs: {}", specs.join(" | ")));
    }

    if let Some(ref h) = build.skills.heal {
        parts.push(format!("Heal: {}", h.name));
    }
    let utils: Vec<String> = build.skills.utilities.iter()
        .filter_map(|u| u.as_ref().map(|s| s.name.clone())).collect();
    if !utils.is_empty() {
        parts.push(format!("Utils: {}", utils.join(", ")));
    }
    if let Some(ref e) = build.skills.elite {
        parts.push(format!("Elite: {}", e.name));
    }

    for set in &build.weapons {
        let mut w = Vec::new();
        if let Some(ref mh) = set.main_hand { w.push(mh.weapon_type.clone()); }
        if let Some(ref oh) = set.off_hand { w.push(oh.weapon_type.clone()); }
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

/// Call Gemini to enrich the top optimizer suggestion with LLM reasoning.
fn enrich_with_gemini(
    key: &str,
    profession_name: &str,
    archetype: &Archetype,
    game_mode: &str,
    candidates: &[gw2_optimizer::engine::BuildCandidate],
    db: &gw2_optimizer::gamedb::GameDb,
    current_build_summary: Option<&str>,
    suggestions: &mut [crate::ui::comparison::BuildSuggestion],
    addon_dir: &std::path::Path,
) -> Result<(), String> {
    let usage_path = addon_dir.join("gemini_usage.json");
    let client = gw2_optimizer::gemini::GeminiClient::with_persistence(key, usage_path)
        .map_err(|e| e.to_string())?;

    let context = gw2_optimizer::prompts::build_game_context(profession_name, archetype);
    let spec_names: Vec<(u32, String)> = db.specializations.iter()
        .map(|(&id, s)| (id, s.name.clone()))
        .collect();
    let candidate_summaries: String = candidates.iter().take(3)
        .map(|c| gw2_optimizer::prompts::summarize_build(c, &spec_names))
        .collect::<Vec<_>>()
        .join("\n");
    let full_context = format!("{}\n\nTop optimizer candidates:\n{}", context, candidate_summaries);

    let prompt = if let Some(summary) = current_build_summary {
        gw2_optimizer::prompts::improve_build_prompt(
            profession_name, archetype, game_mode, summary, &full_context,
        )
    } else {
        let available_specs: Vec<(String, bool)> = db.specializations.values()
            .filter(|s| s.profession == profession_name)
            .map(|s| (s.name.clone(), s.elite))
            .collect();
        gw2_optimizer::prompts::new_build_prompt(
            profession_name, archetype, game_mode, &available_specs, &full_context,
        )
    };

    let response = client.generate_cached(&prompt).map_err(|e| e.to_string())?;
    let gemini_build = gw2_optimizer::prompts::parse_gemini_build(&response)
        .map_err(|e| format!("Parse failed: {}", e))?;

    if let Some(first) = suggestions.first_mut() {
        apply_gemini_response(first, &gemini_build);
    }

    Ok(())
}

/// Send a chat message to Gemini for build refinement.
fn send_chat_message(state: &mut AddonState, message: String) {
    let gemini_key = match state.config.gemini_api_key.clone() {
        Some(key) => key,
        None => {
            crate::ui::chat_bar::add_ai_response(
                &mut state.main.chat,
                "No Gemini API key configured.".into(),
            );
            return;
        }
    };

    let profession = state.main.current_build.as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();
    let build_summary = state.main.current_build.as_ref()
        .map(|b| summarize_resolved_build(b))
        .unwrap_or_default();
    let context = gw2_optimizer::prompts::build_game_context(
        &profession, &Archetype::PowerDPS,
    );
    let addon_dir = state.addon_dir.clone();

    std::thread::spawn(move || {
        let result = (|| -> Result<gw2_optimizer::prompts::GeminiBuildResponse, String> {
            let prompt = gw2_optimizer::prompts::chat_refinement_prompt(
                &profession, &build_summary, &message, &context,
            );
            let usage_path = addon_dir.join("gemini_usage.json");
            let client = gw2_optimizer::gemini::GeminiClient::with_persistence(&gemini_key, usage_path)
                .map_err(|e| e.to_string())?;
            let response = client.generate_cached(&prompt)
                .map_err(|e| e.to_string())?;
            gw2_optimizer::prompts::parse_gemini_build(&response)
                .map_err(|e| format!("Parse failed: {}", e))
        })();

        crate::state::with_state(|s| {
            match result {
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
            }
        });
    });
}
