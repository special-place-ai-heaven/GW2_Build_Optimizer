use base64::Engine as _;

use super::resolution::resolve_selected_build_inner;
use crate::state::AddonState;

fn report_cache_write_error(state: &mut AddonState, message: String) {
    nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &message);
    state.main.error = Some(message);
}

/// Phase 1: Load characters from cache (instant) then refresh from API in background.
pub(super) fn load_characters(state: &mut AddonState) {
    let Some(ref key) = state.config.gw2_api_key else {
        state.main.error = Some("No GW2 API key configured".into());
        return;
    };

    state.main.error = None;

    // Phase 1: try loading from cache instantly
    let cache_dir = state.addon_dir.join("cache");
    let cache = gw2_api::cache::DataCache::new(&cache_dir);
    if let Ok(Some(cached_chars)) = cache.load_characters() {
        state.main.characters = cached_chars;
        state.main.characters_loading = false;
    } else {
        // No cache — show loading indicator until API responds
        state.main.characters_loading = true;
    }

    // Phase 2: background refresh from API
    let key = key.clone();
    let token = state.cancel_token.clone();
    let had_cache = !state.main.characters.is_empty();

    std::thread::spawn(move || {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Always reset characters_loading on every exit path. Early-return cancels
            // previously left the spinner stuck if the user navigated away mid-fetch.
            let result = if token.is_cancelled() {
                None
            } else {
                let r =
                    gw2_api::client::Gw2Client::with_key(&key).and_then(|c| c.fetch_characters());
                if token.is_cancelled() {
                    None
                } else {
                    Some(r)
                }
            };

            crate::state::with_state(|s| {
                s.main.characters_loading = false;
                match result {
                    Some(Ok(fresh_chars)) => {
                        // Save to cache for next time
                        let cache_dir = s.addon_dir.join("cache");
                        let cache = gw2_api::cache::DataCache::new(&cache_dir);
                        if let Err(e) = cache.save_characters(&fresh_chars) {
                            report_cache_write_error(
                                s,
                                format!("Loaded characters, but failed to update cache: {}", e),
                            );
                        }

                        // Only update UI if data changed
                        if s.main.characters != fresh_chars {
                            s.main.characters = fresh_chars;
                        }
                    }
                    Some(Err(e)) => {
                        // If we had cached data, don't overwrite it with an error
                        if !had_cache {
                            s.main.error = Some(e.to_string());
                        }
                        // Update API health status on failure
                        s.main.api_status = crate::state::ApiStatus::Offline;
                    }
                    None => { /* cancelled — flag reset above */ }
                }
            });
        }));
        if panic_result.is_err() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                "bg thread panicked: load_characters",
            );
            crate::state::with_state(|s| {
                s.main.characters_loading = false;
            });
        }
    });
}

/// Apply fetched tabs to state: auto-select active tabs, generate chat code, resolve build.
fn apply_character_tabs(
    state: &mut AddonState,
    build_tabs: Vec<gw2_api::models::BuildTab>,
    equipment_tabs: Vec<gw2_api::models::EquipmentTab>,
) {
    let bt_idx = build_tabs.iter().position(|t| t.is_active).unwrap_or(0);
    let et_idx = equipment_tabs.iter().position(|t| t.is_active).unwrap_or(0);
    state.main.build_tabs = build_tabs;
    state.main.equipment_tabs = equipment_tabs;
    state.main.selected_build_tab = if state.main.build_tabs.is_empty() {
        None
    } else {
        Some(bt_idx)
    };
    state.main.selected_equipment_tab = if state.main.equipment_tabs.is_empty() {
        None
    } else {
        Some(et_idx)
    };
    update_build_chat_code_inner(state);
    resolve_selected_build_inner(state);
}

/// Phase 1: Load character tabs from cache (instant) then refresh from API in background.
pub(super) fn load_character_tabs(state: &mut AddonState, character_name: String) {
    let key = match state.config.gw2_api_key.clone() {
        Some(k) => k,
        None => return,
    };

    state.main.error = None;

    // Phase 1: try loading from cache instantly
    let cache_dir = state.addon_dir.join("cache");
    let cache = gw2_api::cache::DataCache::new(&cache_dir);
    let cached_bt: Option<Vec<gw2_api::models::BuildTab>> = cache
        .load_character(&character_name, "buildtabs")
        .ok()
        .flatten();
    let cached_et: Option<Vec<gw2_api::models::EquipmentTab>> = cache
        .load_character(&character_name, "equiptabs")
        .ok()
        .flatten();

    let had_cache = if let (Some(bt), Some(et)) = (cached_bt, cached_et) {
        // Cache hit — display immediately
        apply_character_tabs(state, bt, et);
        state.main.build_loading = false;
        true
    } else {
        // No cache — show loading indicator
        state.main.build_loading = true;
        false
    };

    // Phase 2: background refresh from API
    let expected_char = character_name.clone();
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Always reset build_loading on every exit path. Early-return cancels
            // previously left the spinner stuck if the user switched character mid-fetch.
            let result = if token.is_cancelled() {
                None
            } else {
                let r: Result<
                    (
                        Vec<gw2_api::models::BuildTab>,
                        Vec<gw2_api::models::EquipmentTab>,
                    ),
                    String,
                > = (|| {
                    let client =
                        gw2_api::client::Gw2Client::with_key(&key).map_err(|e| e.to_string())?;
                    let build_tabs = client
                        .fetch_build_tabs(&character_name)
                        .map_err(|e| e.to_string())?;
                    let equip_tabs = client
                        .fetch_equipment_tabs(&character_name)
                        .map_err(|e| e.to_string())?;
                    Ok((build_tabs, equip_tabs))
                })();
                if token.is_cancelled() {
                    None
                } else {
                    Some(r)
                }
            };

            crate::state::with_state(|s| {
                // Always clear the loading flag — covers cancellation and the case where
                // the user switched character (skipping the apply branch below) which
                // previously left the spinner stuck.
                s.main.build_loading = false;
                let Some(result) = result else {
                    return;
                };
                // Only apply if user hasn't switched to a different character
                if s.main
                    .selected_character
                    .and_then(|i| s.main.characters.get(i))
                    .map(|n| n == &expected_char)
                    .unwrap_or(false)
                {
                    match result {
                        Ok((fresh_bt, fresh_et)) => {
                            // Save to cache
                            let cache_dir = s.addon_dir.join("cache");
                            let cache = gw2_api::cache::DataCache::new(&cache_dir);
                            if let Err(e) =
                                cache.save_character(&expected_char, "buildtabs", &fresh_bt)
                            {
                                report_cache_write_error(
                                    s,
                                    format!(
                                        "Loaded build tabs, but failed to update cache for {}: {}",
                                        expected_char, e
                                    ),
                                );
                            }
                            if let Err(e) =
                                cache.save_character(&expected_char, "equiptabs", &fresh_et)
                            {
                                report_cache_write_error(
                                    s,
                                    format!(
                                        "Loaded equipment tabs, but failed to update cache for {}: {}",
                                        expected_char, e
                                    ),
                                );
                            }

                            // Compare: only update UI if data actually changed
                            let bt_changed = serde_json::to_string(&s.main.build_tabs).ok()
                                != serde_json::to_string(&fresh_bt).ok();
                            let et_changed = serde_json::to_string(&s.main.equipment_tabs).ok()
                                != serde_json::to_string(&fresh_et).ok();

                            if bt_changed || et_changed {
                                apply_character_tabs(s, fresh_bt, fresh_et);
                            }
                        }
                        Err(e) => {
                            // If we had cached data, don't overwrite with error
                            if !had_cache {
                                s.main.error = Some(e);
                            }
                            s.main.api_status = crate::state::ApiStatus::Offline;
                        }
                    }
                }
            });
        }));
        if panic_result.is_err() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                "bg thread panicked: load_character_tabs",
            );
            crate::state::with_state(|s| {
                s.main.build_loading = false;
            });
        }
    });
}

/// Update build chat code from currently selected build tab.
pub(super) fn update_build_chat_code(state: &mut AddonState) {
    update_build_chat_code_inner(state);
}

fn update_build_chat_code_inner(state: &mut AddonState) {
    let build_tab = state
        .main
        .selected_build_tab
        .and_then(|i| state.main.build_tabs.get(i));
    let game_db = state.main.game_db.as_ref();

    if let (Some(bt), Some(db)) = (build_tab, game_db) {
        state.main.build_chat_code = generate_build_chat_code(&bt.build, db);
    } else {
        state.main.build_chat_code = None;
    }
}

/// Generate GW2 build template chat code from a Build.
/// Format: 0x0D + profession_code(1) + 3x(spec_id(1) + trait_bits(1)) + 10x skill_palette(2 LE)
/// + 16 bytes profession-specific + base64 → [&...]
pub(in crate::ui::main_view) fn generate_build_chat_code(
    build: &gw2_api::models::Build,
    db: &gw2_optimizer::gamedb::GameDb,
) -> Option<String> {
    let profession_name = build.profession.as_deref()?;
    let profession = db.profession(profession_name)?;
    let code = profession.code?;
    if code > 255 {
        return None;
    }
    let profession_code = code as u8;

    let mut buf: Vec<u8> = Vec::with_capacity(44);
    buf.push(0x0D); // chat code type: build template
    buf.push(profession_code);

    // 3 specialization slots: spec_id(1 byte) + trait_choices(1 byte)
    for i in 0..3 {
        if let Some(sel) = build.specializations.get(i) {
            if let Some(spec_id) = sel.id {
                if spec_id > 255 {
                    return None;
                }
                buf.push(spec_id as u8);

                // Encode trait choices as 2-bit positions packed into 1 byte
                // Bits: 00CCBBAA where AA = col0, BB = col1, CC = col2
                let spec = db.spec(spec_id);
                let mut trait_byte: u8 = 0;
                for (col, trait_id) in sel.traits.iter().enumerate() {
                    if col >= 3 {
                        break;
                    }
                    if let Some(tid) = trait_id {
                        // Find position of this trait in the column (0=top, 1=mid, 2=bot)
                        if let Some(spec_data) = spec {
                            let col_start = col * 3;
                            let position = spec_data
                                .major_traits
                                .iter()
                                .skip(col_start)
                                .take(3)
                                .position(|&mt| mt == *tid);
                            if let Some(pos) = position {
                                trait_byte |= ((pos as u8 + 1) & 0x03) << (col * 2);
                            }
                        }
                    }
                }
                buf.push(trait_byte);
            } else {
                buf.push(0); // no spec
                buf.push(0);
            }
        } else {
            buf.push(0);
            buf.push(0);
        }
    }

    // 5 terrestrial skills as palette IDs (u16 LE): heal, util1, util2, util3, elite
    // Interleaved with 5 aquatic skills
    let terrestrial_skills = build
        .skills
        .as_ref()
        .map(|sk| {
            let mut ids = vec![sk.heal.unwrap_or(0)];
            for u in &sk.utilities {
                ids.push(u.unwrap_or(0));
            }
            while ids.len() < 4 {
                ids.push(0);
            }
            ids.push(sk.elite.unwrap_or(0));
            ids
        })
        .unwrap_or_else(|| vec![0; 5]);

    let aquatic_skills = build
        .aquatic_skills
        .as_ref()
        .map(|sk| {
            let mut ids = vec![sk.heal.unwrap_or(0)];
            for u in &sk.utilities {
                ids.push(u.unwrap_or(0));
            }
            while ids.len() < 4 {
                ids.push(0);
            }
            ids.push(sk.elite.unwrap_or(0));
            ids
        })
        .unwrap_or_else(|| vec![0; 5]);

    // Interleave: terr_heal, aqua_heal, terr_util1, aqua_util1, ..., terr_elite, aqua_elite
    for i in 0..5 {
        let t_skill = terrestrial_skills.get(i).copied().unwrap_or(0);
        let t_palette = db.skill_to_palette.get(&t_skill).copied().unwrap_or(0);
        buf.extend_from_slice(&(t_palette as u16).to_le_bytes());

        let a_skill = aquatic_skills.get(i).copied().unwrap_or(0);
        let a_palette = db.skill_to_palette.get(&a_skill).copied().unwrap_or(0);
        buf.extend_from_slice(&(a_palette as u16).to_le_bytes());
    }

    // 16 bytes profession-specific data
    match profession_name {
        "Ranger" => {
            // Ranger pets: 4 bytes (terrestrial1, terrestrial2, aquatic1, aquatic2) + 12 zeros
            if let Some(ref pets) = build.pets {
                for pet in pets.terrestrial.iter().take(2) {
                    buf.push(pet.unwrap_or(0) as u8);
                }
                for pet in pets.aquatic.iter().take(2) {
                    buf.push(pet.unwrap_or(0) as u8);
                }
            } else {
                buf.extend_from_slice(&[0u8; 4]);
            }
            buf.extend_from_slice(&[0u8; 12]);
        }
        "Revenant" => {
            // Revenant legends: 4 bytes (legend number parsed from "LegendN" ID) + 12 zeros
            let legend_to_byte = |legend: &Option<String>| -> u8 {
                legend
                    .as_deref()
                    .and_then(|l| l.strip_prefix("Legend").and_then(|n| n.parse::<u8>().ok()))
                    .unwrap_or(0)
            };
            let legends = &build.legends;
            buf.push(legends.first().map(&legend_to_byte).unwrap_or(0));
            buf.push(legends.get(1).map(&legend_to_byte).unwrap_or(0));
            let aquatic_legends = &build.aquatic_legends;
            buf.push(
                aquatic_legends
                    .first()
                    .map(&legend_to_byte)
                    .unwrap_or(0),
            );
            buf.push(
                aquatic_legends
                    .get(1)
                    .map(legend_to_byte)
                    .unwrap_or(0),
            );
            buf.extend_from_slice(&[0u8; 12]);
        }
        _ => {
            buf.extend_from_slice(&[0u8; 16]);
        }
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(&buf);
    Some(format!("[&{}]", encoded))
}
