use nexus::imgui::{ChildWindow, ComboBox, Selectable, Ui};

use crate::state::{AddonState, MainTab};

mod build_display;

pub fn render_main(ui: &Ui, state: &mut AddonState) {
    // Trigger character load on first render
    if state.main.characters.is_empty() && !state.main.characters_loading {
        load_characters(state);
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
        MainTab::NewBuild | MainTab::Improve => {
            if state.main.build_loading {
                ui.text("Loading build...");
            } else if let Some(ref build) = state.main.current_build {
                let stats = state.main.current_stats.clone();
                build_display::render_build(ui, build, stats.as_ref());
            } else if state.main.selected_character.is_some() {
                ui.text("Loading character build...");
            } else {
                ui.text("Select a character from the left menu.");
            }
        }
        MainTab::SaveLoad => {
            ui.text("Save / Load");
            ui.separator();
            ui.text_wrapped("Build saving and loading coming in a future sprint.");
        }
        MainTab::Settings => {
            ui.text("Settings");
            ui.separator();
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
        }
    }
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
