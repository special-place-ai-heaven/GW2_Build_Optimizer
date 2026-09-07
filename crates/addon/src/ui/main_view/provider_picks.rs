//! "Additional suggestions you might like" — the closest published build
//! from each community site, shown beside whatever this addon proposed.
//!
//! Lives here rather than in either tab because both of them show
//! suggestions: New Build renders them through `comparison::render_comparison`
//! and Improve draws its own panes, and a feature that only appeared on one
//! of them was invisible to anyone whose results land on the other.

use nexus::imgui::Ui;

use gw2_core::i18n::{t, tf};

use crate::state::AddonState;
use crate::ui::theme;

/// Match the current proposal against the synced community builds.
///
/// Cheap on every frame but the first for a given proposal: matching reads
/// every benchmark row on disk, so it runs when the proposal changes and not
/// otherwise.
pub(in crate::ui::main_view) fn refresh_provider_picks(state: &mut AddonState) {
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
        .map(|r| crate::ui::main_view::role_i18n_key(&state.main.game_mode, r))
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
    // The elite specialization the plate wears is what the player asked for.
    // A published build without it is not "something like it", whatever
    // else it shares (2026-09-07: a Ritualist plate, a Reaper card). A site
    // with no build in that specialization shows nothing rather than the
    // nearest wrong thing.
    let elite: Option<String> = shape.specs.iter().find_map(|name| {
        db.specializations
            .values()
            .find(|spec| spec.elite && spec.name.eq_ignore_ascii_case(name))
            .map(|spec| spec.name.clone())
    });
    state.main.provider_picks = gw2_optimizer::benchmark::closest_per_source(&builds, &shape, &db)
        .into_iter()
        .map(|(build, _)| build.clone())
        .filter(|build| {
            elite.as_ref().is_none_or(|elite| {
                build
                    .published
                    .specs
                    .iter()
                    .filter_map(|line| db.specializations.get(&line.id))
                    .any(|spec| spec.name.eq_ignore_ascii_case(elite))
            })
        })
        .collect();
}

/// "Additional suggestions you might like" — the closest published build from
/// each site.
///
/// Deliberately below the proposal and visibly separate: these are not what
/// Choya cooked, they are what other people published for the same job. They
/// appear only once the player has synced, because until then there is
/// nothing to compare against.
pub(in crate::ui::main_view) fn render_provider_picks(
    ui: &Ui,
    state: &AddonState,
) -> Option<usize> {
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
            let fill = if hovered {
                p.gold_hover
            } else {
                p.chip_idle_fill
            };
            dl.add_rect(origin, [origin[0] + width, origin[1] + height], fill)
                .filled(true)
                .rounding(4.0)
                .build();
            dl.add_rect(
                origin,
                [origin[0] + width, origin[1] + height],
                p.chip_idle_rim,
            )
            .rounding(4.0)
            .build();
            dl.add_text(
                [origin[0] + 6.0, origin[1] + 4.0],
                crate::ui::color_u32(p.gold),
                &title,
            );
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
        "axe",
        "dagger",
        "mace",
        "pistol",
        "scepter",
        "sword",
        "focus",
        "shield",
        "torch",
        "warhorn",
        "greatsword",
        "hammer",
        "longbow",
        "rifle",
        "shortbow",
        "staff",
        "spear",
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
pub(in crate::ui::main_view) fn take_sync_invite(ui: &Ui, state: &AddonState) -> bool {
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
pub(in crate::ui::main_view) fn adopt_provider_pick(state: &mut AddonState, index: usize) {
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

    // The slot bar, positionally: heal, three utilities, elite. Read from the
    // chat code where the site marks up no skills, which is most of them —
    // reading `skill_ids` alone left the heal and elite slots empty on every
    // GuildJen build.
    let slots = published.slot_skills(&db);
    let slot_name = |at: usize| {
        slots
            .get(at)
            .and_then(|id| *id)
            .and_then(|id| db.skills.get(&id))
            .map(|s| s.name.clone())
    };
    let mut skills: Vec<String> = Vec::new();
    if let Some(heal) = slot_name(0) {
        skills.push(format!("Heal: {heal}"));
    }
    let utils: Vec<String> = (1..4).filter_map(slot_name).collect();
    if !utils.is_empty() {
        skills.push(format!("Utils: {}", utils.join(", ")));
    }
    if let Some(elite) = slot_name(4) {
        skills.push(format!("Elite: {elite}"));
    }

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

    // The same build in the shape validation reads, so the stats below come
    // from the published gear rather than from the summary strings.
    let plate = gw2_optimizer::prompts::GeminiBuildResponse {
        specializations: specializations.clone(),
        weapons: weapons.clone(),
        skills: skills.clone(),
        rune: item_name(published.rune_id),
        sigils: sigils.clone(),
        relic: item_name(published.relic_id),
        stat_prefix: published
            .dominant_stat()
            .unwrap_or_else(|| build.gear_prefix.clone()),
        ..Default::default()
    };
    let mut suggestion = crate::ui::comparison::BuildSuggestion {
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
    // Stats, computed the same way a plated build's are. Without this the
    // panel showed a column of zeroes beside the player's real numbers, which
    // reads as a build with no stats rather than a build we did not measure.
    let validated =
        gw2_optimizer::validation::validate_gemini_build(&plate, &db, &build.profession);
    let game_mode = state.main.game_mode.clone();
    crate::ui::main_view::optimization::attach_chat_stats(
        &mut suggestion,
        &db,
        &build.profession,
        &game_mode,
        Some(&validated),
    );

    // Opening the same card twice is one build, not two. Select the tab that
    // already holds it instead of stacking another beside it.
    if let Some(at) = state
        .main
        .comparison
        .suggestions
        .iter()
        .position(|s| !s.source_url.is_empty() && s.source_url == suggestion.source_url)
    {
        state.main.comparison.suggestions[at] = suggestion;
        state.main.comparison.selected_suggestion = at;
    } else {
        state.main.comparison.suggestions.push(suggestion);
        state.main.comparison.selected_suggestion = state.main.comparison.suggestions.len() - 1;
    }
    // Same landing as Choya's own plate: the tab where a build is actually
    // shown. Opening a build and leaving the player on the page they opened
    // it from is a click that appears to do nothing.
    state.main.active_tab =
        crate::ui::main_view::optimization::result_alert_tab(state.main.current_build.is_some());
}
