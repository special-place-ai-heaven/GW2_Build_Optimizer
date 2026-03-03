use crate::state::AddonState;
use super::resolution::resolve_selected_build_inner;

/// Fetch available models from the active provider's API in a background thread.
pub(super) fn start_fetch_models(state: &mut AddonState) {
    state.main.models_loading = true;
    state.main.models_error = None;
    let addon_dir = state.addon_dir.clone();
    let config_snapshot = state.config.clone();
    let token = state.cancel_token.clone();
    std::thread::spawn(move || {
        if token.is_cancelled() { return; }
        let result = gw2_optimizer::llm::create_client(&config_snapshot, &addon_dir)
            .map_err(|e| e.to_string())
            .and_then(|c| c.list_models().map_err(|e| e.to_string()));
        if token.is_cancelled() { return; }
        crate::state::with_state(|s| {
            s.main.models_loading = false;
            match result {
                Ok(models) => {
                    s.main.available_models = models
                        .into_iter()
                        .map(|m| (m.id, m.display_name))
                        .collect();
                    s.main.models_error = None;
                }
                Err(e) => {
                    s.main.models_error = Some(e);
                }
            }
        });
    });
}

/// Re-download game data from the GW2 API, then reload GameDb.
pub(super) fn start_game_data_refresh(state: &mut AddonState) {
    state.main.game_db_loading = true;
    let cache_dir = state.addon_dir.join("cache");
    let config_path = state.config_path.clone();
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let client = match gw2_api::client::Gw2Client::without_key() {
            Ok(c) => c,
            Err(e) => {
                crate::state::with_state(|s| {
                    s.main.game_db_loading = false;
                    s.main.error = Some(format!("Refresh failed: {}", e));
                });
                return;
            }
        };
        let cache = gw2_api::cache::DataCache::new(&cache_dir);

        let result = gw2_api::download::download_all(&client, &cache, |progress| {
            if token.is_cancelled() { return; }
            crate::state::with_state(|s| {
                let detail = if let Some(ref d) = progress.detail {
                    format!("Refreshing: {} ({})", progress.step_name, d)
                } else {
                    format!("Refreshing: {}", progress.step_name)
                };
                s.main.game_refresh_stage = detail;
            });
        });

        if token.is_cancelled() { return; }

        match result {
            Ok(build_number) => {
                // Save new build number
                crate::state::with_state(|s| {
                    s.config.cache_build_number = Some(build_number);
                    if let Err(e) = s.config.save(&config_path) {
                        nexus::log::log(nexus::log::LogLevel::Warning, "GW2BuildOpt", &format!("Config save failed: {}", e));
                    }
                });

                // Reload GameDb from fresh cache
                if token.is_cancelled() { return; }
                let cache2 = gw2_api::cache::DataCache::new(&cache_dir);
                let db_result = gw2_optimizer::gamedb::GameDb::load(&cache2);

                if token.is_cancelled() { return; }

                crate::state::with_state(|s| {
                    s.main.game_db_loading = false;
                    s.main.game_refresh_stage = String::new();
                    match db_result {
                        Ok(db) => {
                            nexus::log::log(nexus::log::LogLevel::Info, "GW2 Build Optimizer", "Game data refreshed successfully");
                            s.main.game_db = Some(db);
                            // Re-resolve build with fresh data
                            if s.main.selected_build_tab.is_some() && s.main.selected_equipment_tab.is_some() {
                                resolve_selected_build_inner(s);
                            }
                        }
                        Err(e) => {
                            s.main.error = Some(format!("Failed to reload game data: {}", e));
                        }
                    }
                });
            }
            Err(e) => {
                crate::state::with_state(|s| {
                    s.main.game_db_loading = false;
                    s.main.game_refresh_stage = String::new();
                    s.main.error = Some(format!("Refresh failed: {}", e));
                });
            }
        }
    });
}

/// Lightweight API health check: pings GET /v2/build (unauthenticated, returns a single integer).
pub(super) fn check_api_health(state: &mut AddonState) {
    state.main.api_health_checking = true;
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let start = std::time::Instant::now();
        let result = gw2_api::client::Gw2Client::without_key()
            .and_then(|c| c.get_build_number());

        if token.is_cancelled() { return; }

        let status = match result {
            Ok(_) => {
                if start.elapsed().as_secs() >= 5 {
                    crate::state::ApiStatus::Degraded
                } else {
                    crate::state::ApiStatus::Online
                }
            }
            Err(_) => crate::state::ApiStatus::Offline,
        };

        crate::state::with_state(|s| {
            s.main.api_status = status;
            s.main.api_health_checking = false;
        });
    });
}

/// Load GameDb once on main screen entry (S11-T06)
pub(super) fn load_game_db(state: &mut AddonState) {
    state.main.game_db_loading = true;
    let cache_dir = state.addon_dir.join("cache");
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
        if token.is_cancelled() { return; }

        let cache = gw2_api::cache::DataCache::new(&cache_dir);
        let result = gw2_optimizer::gamedb::GameDb::load(&cache);

        if token.is_cancelled() { return; }

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
                    // If build tabs were loaded before GameDb, trigger resolve now
                    if s.main.selected_build_tab.is_some() && s.main.selected_equipment_tab.is_some() {
                        resolve_selected_build_inner(s);
                    }
                }
                Err(e) => {
                    s.main.error = Some(format!("Failed to load game data: {}", e));
                }
            }
        });
    });
}

/// Convert CombatPerformance to the display-friendly CombatMetrics bridge type.
pub(super) fn perf_to_combat_metrics(perf: &gw2_optimizer::combat::CombatPerformance) -> gw2_core::types::CombatMetrics {
    gw2_core::types::CombatMetrics {
        effective_power: perf.effective_power.round() as i32,
        strike_dps_index: perf.strike_dps_index.round() as i32,
        condition_dps_index: perf.condition_dps_index.round() as i32,
        total_dps_index: perf.total_dps_index.round() as i32,
        healing_index: perf.healing_power_index.round() as i32,
        crit_chance: perf.crit_chance,
        boon_duration_pct: perf.boon_duration_pct,
        condi_duration_pct: perf.condi_duration_pct,
        effective_health: perf.effective_health.round() as i32,
        damage_reduction_pct: perf.damage_reduction_pct,
        bleeding_tick: perf.condition_ticks.bleeding.round() as i32,
        burning_tick: perf.condition_ticks.burning.round() as i32,
        poison_tick: perf.condition_ticks.poison.round() as i32,
        torment_tick: perf.condition_ticks.torment.round() as i32,
        confusion_tick: perf.condition_ticks.confusion.round() as i32,
    }
}

/// Compute 3-tier combat metrics (Solo, Party, Full Squad) from stats + modifiers.
pub(super) fn compute_3tier_combat(
    stats: &gw2_optimizer::stats::StatBlock,
    derived: &gw2_optimizer::stats::DerivedStats,
    modifiers: &gw2_optimizer::combat::DamageModifiers,
    profession: &str,
) -> (Option<gw2_core::types::CombatMetrics>, Option<gw2_core::types::CombatMetrics>, Option<gw2_core::types::CombatMetrics>) {
    let profiles = gw2_optimizer::combat::default_buff_profiles();
    let cw = gw2_optimizer::combat::condition_weights_for_profession(profession);
    let compute = |profile: &gw2_optimizer::combat::BuffProfile| -> gw2_core::types::CombatMetrics {
        let perf = gw2_optimizer::combat::calculate_combat_performance(
            stats, derived, modifiers, profile, &cw, profession,
        );
        perf_to_combat_metrics(&perf)
    };
    (profiles.get(0).map(&compute), profiles.get(1).map(&compute), profiles.get(2).map(&compute))
}
