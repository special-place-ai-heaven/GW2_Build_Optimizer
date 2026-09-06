//! New Build tab — scenario summary + comparison view + chat refinement.
//! Role lives in the left menu (shared across modes).

use nexus::imgui::Ui;

use crate::state::AddonState;
use crate::ui::theme;
use gw2_core::i18n::{t, tf};
use gw2_optimizer::scenario::CombatTier;

use super::render_optimization_progress;

fn render_scenario_ready(ui: &Ui, state: &AddonState) {
    let mode = state.main.game_mode.label();
    let role = state
        .main
        .selected_role
        .map(|role| super::super::role_i18n_key(&state.main.game_mode, role))
        .map(t)
        .unwrap_or_else(|| t("label.pick_role"));
    let line = if state.main.game_mode == gw2_core::types::GameMode::WvW {
        let scale = match state.main.combat_tier {
            CombatTier::Solo => t("scale.roam"),
            CombatTier::Party => t("scale.havoc"),
            CombatTier::Squad => t("scale.cloud"),
        };
        format!("{} · {} · {}", mode, scale, role)
    } else {
        format!("{} · {}", mode, role)
    };
    theme::wrapped(ui, theme::pal().gold, &line);
    ui.spacing();
    if state.main.selected_role.is_none() {
        theme::wrapped(ui, theme::pal().muted, &t("new_build.pick_role"));
    } else {
        theme::wrapped(ui, theme::pal().muted, &t("new_build.family_hint"));
        ui.spacing();
        theme::wrapped(ui, theme::pal().muted, &t("new_build.click_optimize"));
    }
}

pub(in crate::ui::main_view) fn render_new_build_tab(ui: &Ui, state: &mut AddonState) {
    if state.main.selected_character.is_none() {
        theme::wrapped(ui, theme::pal().muted, &t("new_build.select_character"));
        return;
    }

    if let Some(err) = state.main.comparison.error.clone() {
        ui.text_colored(theme::ERR, format!("[!] {}", err));
        ui.same_line();
        if ui.small_button(format!("{}##opt_err", t("btn.dismiss"))) {
            state.main.comparison.error = None;
        }
        ui.spacing();
    }

    if state.main.optimizing {
        render_optimization_progress(ui, &state.main.optimize_stage, ui.frame_count());
    }

    if state.main.comparison.suggestions.is_empty() && !state.main.optimizing {
        render_scenario_ready(ui, state);
    }

    if !state.main.comparison.suggestions.is_empty() {
        refresh_provider_picks(state);
        // Set inside the scroll child, acted on after it: switching tabs or
        // pushing a suggestion mid-render would disturb the window being
        // drawn.
        let mut go_to_settings = false;
        let mut picked: Option<usize> = None;
        let footer = ui.current_font_size() + 22.0;
        let scroll_h = (ui.content_region_avail()[1] - footer).max(64.0);
        nexus::imgui::ChildWindow::new("##new_build_scroll")
            .size([0.0, scroll_h])
            .build(ui, || {
                if let Some(build) = state.main.current_build.clone() {
                    let stats = state.main.current_stats.clone();
                    crate::ui::comparison::render_comparison(
                        ui,
                        &build,
                        stats.as_ref(),
                        &mut state.main.comparison,
                        state.main.game_db.as_deref(),
                    );
                } else {
                    ui.text_colored(theme::WARN, t("cmp.wait_build"));
                }
                picked = render_provider_picks(ui, state);
                if take_sync_invite(ui, state) {
                    go_to_settings = true;
                }
            });

        if let Some(i) = picked {
            adopt_provider_pick(state, i);
        }
        if go_to_settings {
            state.main.active_tab = crate::state::MainTab::Settings;
        }

        super::saveload::render_save_build_ui(ui, state);
        ui.same_line();
        if ui.small_button(t("btn.clear_results")) {
            state.main.comparison.suggestions.clear();
            state.main.comparison.error = None;
        }
    }
}

/// Match the current proposal against the synced community builds.
///
/// Cheap on every frame but the first for a given proposal: matching reads
/// every benchmark row on disk, so it runs when the proposal changes and not
/// otherwise.
fn refresh_provider_picks(state: &mut AddonState) {
    let Some(suggestion) = state
        .main
        .comparison
        .suggestions
        .get(state.main.comparison.selected_suggestion)
    else {
        return;
    };
    let profession = state
        .main
        .current_build
        .as_ref()
        .map(|b| b.profession.clone())
        .unwrap_or_default();
    let role = state
        .main
        .selected_role
        .map(|r| super::super::role_i18n_key(&state.main.game_mode, r))
        .map(t)
        .unwrap_or_default();
    let shape = gw2_optimizer::benchmark::BuildShape {
        profession,
        mode: state.main.game_mode.label().to_string(),
        specs: suggestion
            .specializations
            .iter()
            .map(|(name, _)| name.clone())
            .collect(),
        weapons: suggestion.weapons.clone(),
        stat_prefix: suggestion.stat_prefix.clone(),
        rune: suggestion.rune.clone(),
        relic: suggestion.relic.clone(),
        role,
    };
    let key = format!(
        "{}|{}|{}|{}",
        shape.profession,
        shape.mode,
        shape.role,
        shape.specs.join(",")
    );
    if key == state.main.provider_picks_key {
        return;
    }
    state.main.provider_picks_key = key;
    state.main.provider_picks.clear();
    // No game data means no specialization or item names to compare, so
    // there is nothing to match on and nothing worth showing.
    let Some(db) = state.main.game_db.clone() else {
        return;
    };
    let builds = gw2_optimizer::scraper::load_benchmarks(&state.addon_dir);
    state.main.provider_picks = gw2_optimizer::benchmark::closest_per_source(&builds, &shape, &db)
        .into_iter()
        .map(|(build, _)| build.clone())
        .collect();
}

/// "Additional suggestions you might like" — the closest published build from
/// each site.
///
/// Deliberately below the proposal and visibly separate: these are not what
/// Choya cooked, they are what other people published for the same job. They
/// appear only once the player has synced, because until then there is
/// nothing to compare against.
fn render_provider_picks(ui: &Ui, state: &AddonState) -> Option<usize> {
    if state.main.provider_picks.is_empty() {
        return None;
    }
    ui.spacing();
    ui.separator();
    ui.spacing();
    theme::wrapped(ui, theme::pal().gold, &t("cmp.also_like"));
    ui.spacing();
    let mut chosen = None;
    for (i, build) in state.main.provider_picks.iter().enumerate() {
        let title = if build.spec_name.is_empty() {
            build.profession.clone()
        } else {
            build.spec_name.clone()
        };
        let mut line = build.role.clone();
        if !build.gear_prefix.is_empty() {
            line.push_str(" \u{00b7} ");
            line.push_str(&build.gear_prefix);
        }
        let weapons = published_weapons(build);
        if !weapons.is_empty() {
            line.push_str(" \u{00b7} ");
            line.push_str(&weapons.join("/"));
        }

        // The whole card is the button: opening the build here is the point
        // of showing it, and a card that only links out sends the player to
        // a website to read what this panel could have shown them.
        let origin = ui.cursor_screen_pos();
        let width = ui.content_region_avail()[0].max(80.0);
        let height = ui.text_line_height() * 2.0 + 10.0;
        let clicked = ui.invisible_button(format!("##pick_{i}"), [width, height]);
        let hovered = ui.is_item_hovered();
        {
            let dl = ui.get_window_draw_list();
            let p = theme::pal();
            let fill = if hovered { p.gold_hover } else { p.chip_idle_fill };
            dl.add_rect(origin, [origin[0] + width, origin[1] + height], fill)
                .filled(true)
                .rounding(4.0)
                .build();
            dl.add_rect(origin, [origin[0] + width, origin[1] + height], p.chip_idle_rim)
                .rounding(4.0)
                .build();
            dl.add_text([origin[0] + 6.0, origin[1] + 4.0], crate::ui::color_u32(p.gold), &title);
            let after = ui.calc_text_size(&title)[0] + 12.0;
            dl.add_text(
                [origin[0] + after, origin[1] + 4.0],
                crate::ui::color_u32(p.muted),
                format!("\u{00b7} {}", build.source),
            );
            dl.add_text(
                [origin[0] + 6.0, origin[1] + 5.0 + ui.text_line_height()],
                crate::ui::color_u32(p.muted),
                &line,
            );
        }
        if clicked {
            chosen = Some(i);
        }

        // No link on the card. Clicking it opens the build in the panel that
        // shows builds, and the link to the site lives there, beside the
        // build it belongs to.
        ui.spacing();
    }
    chosen
}

/// Weapon types named by a published build's gear rows.
///
/// Every site writes the weapon type into the slot label, so a row whose slot
/// is a weapon name is a weapon — see `providers::GearRow::slot`.
fn published_weapons(build: &gw2_optimizer::benchmark::BenchmarkBuild) -> Vec<String> {
    const WEAPONS: [&str; 17] = [
        "axe", "dagger", "mace", "pistol", "scepter", "sword", "focus", "shield", "torch",
        "warhorn", "greatsword", "hammer", "longbow", "rifle", "shortbow", "staff", "spear",
    ];
    let mut seen: Vec<String> = Vec::new();
    for row in &build.published.gear {
        let slot = row.slot.trim();
        if WEAPONS.contains(&slot.to_lowercase().as_str()) && !seen.iter().any(|s| s == slot) {
            seen.push(slot.to_string());
        }
    }
    seen
}

/// Offer the sync, but only to someone who has never done one.
///
/// Two silences look the same from here and must not be treated the same. No
/// benchmark data at all means the feature has never been available and is
/// worth pointing at. Data present but nothing close enough to show means the
/// player has already synced and the honest answer is that their build has no
/// published cousin — nagging them to sync again would be a lie.
///
/// Returns true when the player asked to go and do it.
fn take_sync_invite(ui: &Ui, state: &AddonState) -> bool {
    if !state.main.provider_picks.is_empty() || !state.main.benchmark_counts.is_empty() {
        return false;
    }
    ui.spacing();
    ui.separator();
    ui.spacing();
    theme::wrapped(ui, theme::pal().muted, &t("cmp.sync_for_more"));
    theme::gold_button(ui, t("btn.sync_now"))
}

/// Open a published build in this panel, as its own suggestion.
///
/// The row already holds the whole build in API ids — specializations with
/// their chosen traits, gear with per-slot stat prefixes, rune, sigils,
/// relic, skills — so nothing is fetched and nothing is guessed. Names come
/// from the same `GameDb` the rest of the panel reads, and an id the cache
/// does not know is dropped rather than shown as a number.
///
/// It becomes a suggestion beside Choya's own, which is the honest framing:
/// somebody published this for this job, here it is next to what Choya
/// cooked, compare them.
fn adopt_provider_pick(state: &mut AddonState, index: usize) {
    let Some(build) = state.main.provider_picks.get(index).cloned() else {
        return;
    };
    let Some(db) = state.main.game_db.clone() else {
        return;
    };
    let published = &build.published;

    let specializations: Vec<(String, Vec<String>)> = published
        .specs
        .iter()
        .filter_map(|line| {
            let spec = db.specializations.get(&line.id)?;
            let traits = line
                .trait_ids
                .iter()
                .filter_map(|id| db.traits.get(id).map(|t| t.name.clone()))
                .collect();
            Some((spec.name.clone(), traits))
        })
        .collect();

    let skills: Vec<String> = published
        .skill_ids
        .iter()
        .filter_map(|id| db.skills.get(id).map(|s| s.name.clone()))
        .collect();

    let item_name = |id: Option<u32>| {
        id.and_then(|id| db.items.get(&id))
            .map(|item| item.name.clone())
            .unwrap_or_default()
    };
    let sigils: Vec<String> = published
        .sigil_ids
        .iter()
        .filter_map(|id| db.items.get(id).map(|item| item.name.clone()))
        .collect();

    let label = if build.spec_name.is_empty() {
        format!("{} \u{00b7} {}", build.profession, build.source)
    } else {
        format!("{} \u{00b7} {}", build.spec_name, build.source)
    };
    let weapons = published_weapons(&build);
    let mut summary = build.role.clone();
    if !weapons.is_empty() {
        summary.push_str(" \u{00b7} ");
        summary.push_str(&weapons.join(" / "));
    }

    let suggestion = crate::ui::comparison::BuildSuggestion {
        label,
        build_summary: summary,
        stat_prefix: published
            .dominant_stat()
            .unwrap_or_else(|| build.gear_prefix.clone()),
        specializations,
        weapons,
        skills,
        rune: item_name(published.rune_id),
        sigils,
        relic: item_name(published.relic_id),
        // The site's own code, not one we re-encoded: it is what they
        // published and what their page hands out.
        chat_code: build.build_code.clone(),
        explanation: tf(
            "fmt.published_by",
            &[("source", &build.source), ("role", &build.role)],
        ),
        source_url: build.source_url.clone(),
        ..Default::default()
    };
    state.main.comparison.suggestions.push(suggestion);
    state.main.comparison.selected_suggestion = state.main.comparison.suggestions.len() - 1;
    // Same landing as Choya's own plate: the tab where a build is actually
    // shown. Opening a build and leaving the player on the page they opened
    // it from is a click that appears to do nothing.
    state.main.active_tab =
        crate::ui::main_view::optimization::result_alert_tab(state.main.current_build.is_some());
}
