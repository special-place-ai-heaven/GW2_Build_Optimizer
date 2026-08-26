//! Card-style build display using DrawList for visual card sections.

use nexus::imgui::Ui;

use gw2_core::i18n::{t, tf};
use gw2_core::types::{ResolvedBuild, StatBlock};

// ─── Color palette ───

const SECTION_HEADER_BG: [f32; 4] = [0.28, 0.24, 0.12, 0.95]; // warm dark gold header
                                                              // Body bg is not drawn (would cover text due to DrawList ordering).
                                                              // Card look comes from header + border + accent line.
const SPEC_COLOR: [f32; 4] = [0.85, 0.65, 1.0, 1.0]; // purple spec names
const TRAIT_COLOR: [f32; 4] = [0.65, 0.8, 1.0, 1.0]; // light blue traits
const GEAR_COLOR: [f32; 4] = [1.0, 0.85, 0.3, 1.0]; // bright gold for gear
const DIM_COLOR: [f32; 4] = [0.55, 0.55, 0.55, 1.0]; // dim gray
const LABEL_COLOR: [f32; 4] = [0.7, 0.7, 0.75, 1.0]; // cool gray labels
const VALUE_COLOR: [f32; 4] = [0.95, 0.95, 0.95, 1.0]; // near-white values
const SECTION_TITLE_COLOR: [f32; 4] = [1.0, 0.88, 0.35, 1.0]; // bright gold titles
const ACCENT_LINE_COLOR: [f32; 4] = [0.7, 0.55, 0.15, 0.5]; // subtle gold accent
const CARD_BORDER_COLOR: [f32; 4] = [0.35, 0.3, 0.15, 0.4]; // dim gold border

const STAT_BETTER: [f32; 4] = [0.3, 1.0, 0.4, 1.0]; // green — this stat is higher
const STAT_WORSE: [f32; 4] = [1.0, 0.35, 0.3, 1.0]; // red — this stat is lower

/// Pick color for a stat value based on comparison: green if better, red if worse, default if equal.
fn stat_color(val: i32, compare: i32) -> [f32; 4] {
    if val > compare {
        STAT_BETTER
    } else if val < compare {
        STAT_WORSE
    } else {
        VALUE_COLOR
    }
}

const CARD_ROUNDING: f32 = 5.0;
const CARD_PAD: f32 = 4.0;
const TITLE_HEIGHT: f32 = 20.0;
const CARD_GAP: f32 = 8.0;

/// Render a prominent panel header (e.g. "CURRENT BUILD") with colored text.
pub fn render_card_header(ui: &Ui, title: &str, color: [f32; 4]) {
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    let th = ui.calc_text_size(title)[1];
    let bar_h = 22.0;
    let ty = start[1] + ((bar_h - th) * 0.5).round();
    {
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [start[0] - 1.0, start[1]],
                [start[0] + width + 1.0, start[1] + bar_h],
                [0.15, 0.13, 0.08, 0.9],
            )
            .filled(true)
            .rounding(CARD_ROUNDING)
            .round_bot_left(false)
            .round_bot_right(false)
            .build();
        crate::ui::theme::paint_header_accent(&draw_list, start[0], start[1], bar_h);
        draw_list.add_text(
            [crate::ui::theme::header_title_x(start[0]), ty],
            color,
            title,
        );
    }
    ui.dummy([0.0, 24.0]);
}

/// Render a section card: header bar with title + body with content.
/// DrawListMut borrows are scoped to avoid the "already loaded" panic.
fn render_card_section(ui: &Ui, title: &str, content: impl FnOnce(&Ui)) {
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    let hdr_top = start[1];
    let hdr_bottom = hdr_top + CARD_PAD + TITLE_HEIGHT;

    // ── Phase 1: Draw header background + title ──
    {
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [start[0] - 1.0, hdr_top],
                [start[0] + width + 1.0, hdr_bottom],
                SECTION_HEADER_BG,
            )
            .filled(true)
            .rounding(CARD_ROUNDING)
            .round_bot_left(false)
            .round_bot_right(false)
            .build();
        draw_list.add_text(
            [
                crate::ui::theme::header_title_x(start[0]),
                hdr_top + CARD_PAD,
            ],
            SECTION_TITLE_COLOR,
            title,
        );
        crate::ui::theme::paint_header_accent(&draw_list, start[0], hdr_top, hdr_bottom - hdr_top);
    } // DrawListMut dropped here

    // Reserve space for the header
    ui.dummy([0.0, CARD_PAD + TITLE_HEIGHT + 2.0]);

    // ── Phase 2: Render content (ImGui widgets) ──
    let body_top = ui.cursor_screen_pos()[1];
    ui.dummy([0.0, 2.0]);
    content(ui);
    ui.dummy([0.0, CARD_PAD]);
    let body_bottom = ui.cursor_screen_pos()[1];

    // ── Phase 3: Draw accent line + border ──
    {
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_line(
                [start[0] - 1.0, body_top],
                [start[0] + width + 1.0, body_top],
                ACCENT_LINE_COLOR,
            )
            .thickness(1.0)
            .build();
        draw_list
            .add_rect(
                [start[0] - 1.0, hdr_top],
                [start[0] + width + 1.0, body_bottom],
                CARD_BORDER_COLOR,
            )
            .rounding(CARD_ROUNDING)
            .build();
    } // DrawListMut dropped here

    ui.dummy([0.0, CARD_GAP]);
}

/// Render a compact suggestion card (right panel) from a BuildSuggestion.
#[allow(dead_code)]
pub fn render_suggestion_card(ui: &Ui, suggestion: &super::super::comparison::BuildSuggestion) {
    // ── Specializations Card ──
    render_card_section(ui, &t("section.specializations"), |ui| {
        for (name, traits) in &suggestion.specializations {
            ui.text_colored(SPEC_COLOR, format!("  {}", name));
            if !traits.is_empty() {
                ui.text_colored(TRAIT_COLOR, format!("    {}", traits.join(" | ")));
            }
        }
    });

    // ── Skills Card ──
    render_card_section(ui, &t("section.skills"), |ui| {
        let parsed = crate::ui::gear_diff::parse_suggestion_skills(&suggestion.skills);
        render_skill_bar(
            ui,
            None,
            &parsed.stances,
            &parsed.pets,
            &parsed.heal,
            &parsed.utilities,
            &parsed.elite,
            "sugcard",
        );
    });

    // ── Weapons Card ──
    render_card_section(ui, &t("section.weapons"), |ui| {
        for weapon in &suggestion.weapons {
            ui.text_colored(VALUE_COLOR, format!("  {}", weapon));
        }
        if !suggestion.sigils.is_empty() {
            ui.text_colored(
                DIM_COLOR,
                format!(
                    "  {}",
                    tf("fmt.sigils", &[("list", &suggestion.sigils.join(", "))])
                ),
            );
        }
    });

    // ── Gear Card ──
    render_card_section(ui, &t("section.gear"), |ui| {
        let groups = &suggestion.gear_prefixes;
        if !groups.armor.is_empty() || !groups.trinkets.is_empty() || !groups.weapons.is_empty() {
            if !groups.armor.is_empty() {
                render_label_value(ui, "Armor", &groups.armor);
            }
            if !groups.trinkets.is_empty() {
                render_label_value(ui, "Trinkets", &groups.trinkets);
            }
            if !groups.weapons.is_empty() {
                render_label_value(ui, "Weapons", &groups.weapons);
            }
        } else if !suggestion.stat_prefix.is_empty() {
            render_label_value(ui, &t("slot.prefix"), &suggestion.stat_prefix);
        }
        if !suggestion.rune.is_empty() {
            render_label_value_colored(ui, &t("slot.rune"), &suggestion.rune, GEAR_COLOR);
        }
        if !suggestion.relic.is_empty() {
            render_label_value_colored(ui, &t("slot.relic"), &suggestion.relic, GEAR_COLOR);
        }
    });

    // ── Stats Card ──
    if let Some(ref s) = suggestion.estimated_stats {
        render_card_section(ui, &t("section.stats"), |ui| {
            ui.columns(2, "##sug_stats", false);
            let rows = [
                ("stat.power", s.power),
                ("stat.precision", s.precision),
                ("stat.toughness", s.toughness),
                ("stat.vitality", s.vitality),
                ("stat.condi", s.condition_damage),
                ("stat.ferocity", s.ferocity),
                ("stat.healing", s.healing_power),
                ("stat.expertise", s.expertise),
            ];
            for (key, val) in &rows {
                ui.text_colored(LABEL_COLOR, format!("  {}: ", t(key)));
                ui.same_line();
                ui.text_colored(VALUE_COLOR, format!("{}", val));
                ui.next_column();
            }
            ui.columns(1, "##sug_stats_end", false);
            ui.text_colored(
                DIM_COLOR,
                format!(
                    "  {}",
                    tf(
                        "fmt.crit_hp_armor",
                        &[
                            ("crit", &format!("{:.1}", s.crit_chance)),
                            ("hp", &s.health.to_string()),
                            ("armor", &s.armor.to_string()),
                        ],
                    )
                ),
            );
        });
    }
}

/// Render a suggestion card without the Specializations section (used when optimized specs panel replaces it).
/// `compare_stats`: if provided, stat values are colored green/red relative to these (the current build's stats).
#[allow(dead_code)]
pub fn render_suggestion_card_no_specs(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    compare_stats: Option<&StatBlock>,
) {
    render_suggestion_skills(ui, suggestion, None);
    render_suggestion_weapons(ui, suggestion, None);
    render_suggestion_gear(ui, suggestion, None);
    render_suggestion_stats(ui, suggestion, compare_stats);
}

// ─── Helpers ───

fn render_label_value(ui: &Ui, label: &str, value: &str) {
    ui.text_colored(LABEL_COLOR, format!("  {}: ", label));
    ui.same_line();
    ui.text_colored(VALUE_COLOR, value);
}

fn render_label_value_inspect(
    ui: &Ui,
    label: &str,
    value: &str,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
) {
    ui.text_colored(LABEL_COLOR, format!("  {}: ", label));
    ui.same_line();
    ui.text_colored(VALUE_COLOR, crate::ui::comparison::loc_name(db, value));
    crate::ui::comparison::inspect_if_hovered(ui, value, db);
}

fn render_label_value_colored(ui: &Ui, label: &str, value: &str, color: [f32; 4]) {
    ui.text_colored(LABEL_COLOR, format!("  {}: ", label));
    ui.same_line();
    ui.text_colored(color, value);
}

fn truncate_to_width(ui: &Ui, text: &str, max_w: f32) -> String {
    if ui.calc_text_size(text)[0] <= max_w {
        return text.to_string();
    }
    let mut s = String::new();
    for c in text.chars() {
        let mut next = s.clone();
        next.push(c);
        next.push('\u{2026}');
        if ui.calc_text_size(&next)[0] > max_w {
            break;
        }
        s.push(c);
    }
    s.push('\u{2026}');
    s
}

fn render_slash_list(
    ui: &Ui,
    label: &str,
    joined: &str,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
) {
    if joined.is_empty() {
        return;
    }
    ui.text_colored(LABEL_COLOR, format!("  {label}"));
    ui.same_line();
    for (i, part) in joined.split(" / ").enumerate() {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if i > 0 {
            ui.same_line_with_spacing(0.0, 6.0);
        }
        crate::ui::theme::chip(
            ui,
            crate::ui::comparison::loc_name(db, part),
            &format!("##{label}_chip_{i}"),
        );
        crate::ui::comparison::inspect_if_hovered(ui, part, db);
    }
}

fn slash_parts(joined: &str) -> Vec<&str> {
    joined
        .split(" / ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Heal / utilities / elite for one revenant legend, keyed by compact stance label
/// ("Dwarf", "Entity", …). Character API skills are the active legend only —
/// and older legends often share palettes with the newest one — so the bar
/// must read `/v2/legends`, not `build.skills`.
fn stance_kit(
    db: &gw2_optimizer::gamedb::GameDb,
    compact: &str,
) -> Option<(String, Vec<String>, String)> {
    let legend = db.legends.values().find(|l| {
        db.skills.get(&l.swap).is_some_and(|s| {
            crate::ui::comparison::compact_stance_name(&s.name).eq_ignore_ascii_case(compact)
        })
    })?;
    let name = |id: u32| {
        db.skills
            .get(&id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("#{id}"))
    };
    Some((
        name(legend.heal),
        legend.utilities.iter().copied().map(name).collect(),
        name(legend.elite),
    ))
}

// ponytail: one preview index for the visible skill bar (Improve never shows two).
thread_local! {
    static STANCE_PREVIEW: std::cell::Cell<usize> = std::cell::Cell::new(0);
}

/// Clickable stance pills. Returns that legend's kit when the db has it.
fn render_stance_tabs(
    ui: &Ui,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
    joined: &str,
    id_suffix: &str,
) -> Option<(String, Vec<String>, String)> {
    let names = slash_parts(joined);
    if names.is_empty() {
        return None;
    }
    ui.text_colored(LABEL_COLOR, t("slot.stances"));
    let n = names.len();
    let mut selected = STANCE_PREVIEW.with(|c| {
        let v = c.get();
        if v >= n {
            c.set(0);
            0
        } else {
            v
        }
    });
    let avail = ui.content_region_avail()[0];
    let mut row_x = 0.0_f32;
    for (i, name) in names.iter().enumerate() {
        let [cw, _] = crate::ui::theme::select_chip_size(ui, name, false);
        crate::ui::theme::wrap_chip(ui, avail, &mut row_x, cw, 4.0);
        let id = format!("##stance_tab_{id_suffix}_{i}");
        if crate::ui::theme::select_chip(
            ui,
            crate::ui::comparison::loc_name(db, name),
            i == selected,
            &id,
            None,
        ) {
            selected = i;
            STANCE_PREVIEW.with(|c| c.set(i));
        }
        crate::ui::comparison::inspect_if_hovered(ui, name, db);
    }
    db.and_then(|d| stance_kit(d, names[selected]))
}

fn wrap_slot_lines(ui: &Ui, text: &str, max_w: f32) -> Vec<String> {
    if ui.calc_text_size(text)[0] <= max_w {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return vec![truncate_to_width(ui, text, max_w)];
    }
    let mut line1 = String::new();
    let mut i = 0;
    while i < words.len() {
        let trial = if line1.is_empty() {
            words[i].to_string()
        } else {
            format!("{} {}", line1, words[i])
        };
        if ui.calc_text_size(&trial)[0] > max_w && !line1.is_empty() {
            break;
        }
        line1 = trial;
        i += 1;
    }
    let rest = words[i..].join(" ");
    if rest.is_empty() {
        vec![line1]
    } else {
        vec![line1, truncate_to_width(ui, &rest, max_w)]
    }
}

fn render_skill_bar(
    ui: &Ui,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
    stances: &str,
    pets: &str,
    heal: &str,
    utilities: &[String],
    elite: &str,
    id_suffix: &str,
) {
    let kit = render_stance_tabs(ui, db, stances, id_suffix);
    render_slash_list(ui, &t("slot.pets"), pets, db);

    let (heal, utilities, elite) = match kit {
        Some((h, u, e)) => (h, u, e),
        None => (heal.to_string(), utilities.to_vec(), elite.to_string()),
    };
    let u1 = utilities.first().map(|s| s.as_str()).unwrap_or("");
    let u2 = utilities.get(1).map(|s| s.as_str()).unwrap_or("");
    let u3 = utilities.get(2).map(|s| s.as_str()).unwrap_or("");
    let l_heal = t("slot.heal");
    let l_u1 = t("slot.util1");
    let l_u2 = t("slot.util2");
    let l_u3 = t("slot.util3");
    let l_elite = t("slot.elite");
    let slots = [
        (l_heal.as_str(), heal.as_str(), crate::ui::theme::HEAL_RIM),
        (l_u1.as_str(), u1, crate::ui::theme::GOLD_DIM),
        (l_u2.as_str(), u2, crate::ui::theme::GOLD_DIM),
        (l_u3.as_str(), u3, crate::ui::theme::GOLD_DIM),
        (
            l_elite.as_str(),
            elite.as_str(),
            crate::ui::theme::ELITE_RIM,
        ),
    ];
    let avail = ui.content_region_avail()[0].max(1.0);
    let gap = 5.0;
    let line = ui.text_line_height();
    let pad = 4.0;
    let icon = line * 2.0;
    let icon_text_gap = 8.0;
    let slot_h = icon + pad * 2.0;
    let min_slot = pad + icon + icon_text_gap + 72.0 + pad;
    let cols = if avail >= min_slot * 5.0 + gap * 4.0 {
        5
    } else if avail >= min_slot * 3.0 + gap * 2.0 {
        3
    } else {
        1
    };
    let slot_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
    let start = ui.cursor_screen_pos();
    for (i, (label, value, rim)) in slots.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = start[0] + col as f32 * (slot_w + gap);
        let y = start[1] + row as f32 * (slot_h + gap);
        let p = [x, y];
        let empty = value.is_empty();
        let fill = if empty {
            crate::ui::theme::PLATE_EMPTY
        } else {
            crate::ui::theme::PLATE
        };
        let border = if empty {
            [0.28, 0.24, 0.14, 0.45]
        } else {
            *rim
        };
        ui.set_cursor_screen_pos(p);
        let _ = ui.invisible_button(&format!("##skill_slot_{id_suffix}_{i}"), [slot_w, slot_h]);
        if !empty {
            crate::ui::comparison::inspect_if_hovered(ui, value, db);
        }
        {
            let dl = ui.get_window_draw_list();
            dl.add_rect(p, [p[0] + slot_w, p[1] + slot_h], fill)
                .filled(true)
                .rounding(6.0)
                .build();
            dl.add_rect(p, [p[0] + slot_w, p[1] + slot_h], border)
                .rounding(6.0)
                .build();
            let icon_p = [p[0] + pad, p[1] + pad];
            let icon_url = if empty {
                None
            } else {
                db.and_then(|d| crate::ui::icons::skill_url_by_name(d, value))
            };
            crate::ui::icons::paint_on(
                &dl,
                icon_url,
                icon_p,
                [icon_p[0] + icon, icon_p[1] + icon],
                [1.0, 1.0, 1.0, 1.0],
            );
            let text_x = p[0] + pad + icon + icon_text_gap;
            let text_w = (p[0] + slot_w - pad - text_x).max(16.0);
            dl.add_text(
                [text_x, p[1] + pad],
                crate::ui::color_u32(crate::ui::theme::GOLD),
                *label,
            );
            let shown = if empty {
                "\u{2014}"
            } else {
                crate::ui::comparison::loc_name(db, value)
            };
            let color = if empty {
                crate::ui::theme::MUTED
            } else {
                crate::ui::theme::CREAM
            };
            let lines = wrap_slot_lines(ui, shown, text_w);
            if let Some(ln) = lines.first() {
                dl.add_text([text_x, p[1] + pad + line], crate::ui::color_u32(color), ln);
            }
        }
    }
    let rows = slots.len().div_ceil(cols);
    ui.set_cursor_screen_pos([start[0], start[1] + rows as f32 * (slot_h + gap)]);
    ui.dummy([avail, 0.0]);
}

// ─── Individual section renderers (for column-aligned layouts) ───

/// Render the SKILLS section for the current build.
pub fn render_build_skills(
    ui: &Ui,
    build: &ResolvedBuild,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
) {
    render_card_section(ui, &t("section.skills"), |ui| {
        let heal = build
            .skills
            .heal
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("");
        let elite = build
            .skills
            .elite
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("");
        let utils: Vec<String> = (0..3)
            .map(|i| {
                build
                    .skills
                    .utilities
                    .get(i)
                    .and_then(|u| u.as_ref().map(|s| s.name.clone()))
                    .unwrap_or_default()
            })
            .collect();
        render_skill_bar(
            ui,
            db,
            &build.legends.join(" / "),
            &build.pets.join(" / "),
            heal,
            &utils,
            elite,
            "cur",
        );
    });
}

/// Render the SKILLS section for the suggestion.
pub fn render_suggestion_skills(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
) {
    render_card_section(ui, &t("section.skills"), |ui| {
        let parsed = crate::ui::gear_diff::parse_suggestion_skills(&suggestion.skills);
        render_skill_bar(
            ui,
            db,
            &parsed.stances,
            &parsed.pets,
            &parsed.heal,
            &parsed.utilities,
            &parsed.elite,
            "sug",
        );
    });
}

/// Render the WEAPONS section for the suggestion.
pub fn render_suggestion_weapons(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
) {
    render_card_section(ui, &t("section.weapons"), |ui| {
        for weapon in &suggestion.weapons {
            ui.text_colored(VALUE_COLOR, format!("  {}", weapon));
        }
        for sigil in &suggestion.sigils {
            render_label_value_inspect(ui, &t("slot.sigil"), sigil, db);
        }
    });
}

/// Render the GEAR section for the suggestion.
pub fn render_suggestion_gear(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
) {
    render_card_section(ui, &t("section.gear"), |ui| {
        if !suggestion.stat_prefix.is_empty() {
            render_label_value(ui, &t("slot.prefix"), &suggestion.stat_prefix);
        }
        if !suggestion.rune.is_empty() {
            ui.text_colored(LABEL_COLOR, format!("  {}: ", t("slot.rune")));
            ui.same_line();
            ui.text_colored(GEAR_COLOR, &suggestion.rune);
            crate::ui::comparison::inspect_if_hovered(ui, &suggestion.rune, db);
        }
        if !suggestion.relic.is_empty() {
            ui.text_colored(LABEL_COLOR, format!("  {}: ", t("slot.relic")));
            ui.same_line();
            ui.text_colored(GEAR_COLOR, &suggestion.relic);
            crate::ui::comparison::inspect_if_hovered(ui, &suggestion.relic, db);
        }
    });
}

/// Render the STATS section for the suggestion (single-column, no inner columns).
/// Color-coded against compare_stats if provided.
pub fn render_suggestion_stats(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    compare_stats: Option<&StatBlock>,
) {
    if let Some(ref s) = suggestion.estimated_stats {
        render_card_section(ui, &t("section.stats"), |ui| {
            let cmp = compare_stats.cloned().unwrap_or_default();
            let rows = [
                ("stat.power", s.power, cmp.power),
                ("stat.precision", s.precision, cmp.precision),
                ("stat.toughness", s.toughness, cmp.toughness),
                ("stat.vitality", s.vitality, cmp.vitality),
                ("stat.condi", s.condition_damage, cmp.condition_damage),
                ("stat.ferocity", s.ferocity, cmp.ferocity),
                ("stat.healing", s.healing_power, cmp.healing_power),
                ("stat.expertise", s.expertise, cmp.expertise),
            ];
            for (key, val, cmp_val) in &rows {
                ui.text_colored(LABEL_COLOR, format!("  {}: ", t(key)));
                ui.same_line();
                let color = if compare_stats.is_some() {
                    stat_color(*val, *cmp_val)
                } else {
                    VALUE_COLOR
                };
                ui.text_colored(color, format!("{}", val));
            }
            ui.text_colored(
                DIM_COLOR,
                format!(
                    "  {}",
                    tf(
                        "fmt.crit_hp_armor",
                        &[
                            ("crit", &format!("{:.1}", s.crit_chance)),
                            ("hp", &s.health.to_string()),
                            ("armor", &s.armor.to_string()),
                        ],
                    )
                ),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::stance_kit;
    use gw2_api::models::Legend;

    fn skill(id: u32, name: &str) -> gw2_api::models::Skill {
        serde_json::from_value(serde_json::json!({ "id": id, "name": name }))
            .expect("skill fixture")
    }

    fn legend(id: &str, swap: u32, heal: u32, elite: u32, utilities: [u32; 3]) -> Legend {
        Legend {
            id: id.into(),
            code: None,
            swap,
            heal,
            elite,
            utilities: utilities.to_vec(),
        }
    }

    #[test]
    fn stance_kit_uses_that_legend_not_the_api_active_one() {
        let mut db = gw2_optimizer::gamedb::GameDb::empty_for_tests();
        db.skills.insert(1, skill(1, "Legendary Dwarf Stance"));
        db.skills.insert(10, skill(10, "Soothing Stone"));
        db.skills.insert(11, skill(11, "Inspiring Reinforcement"));
        db.skills.insert(12, skill(12, "Forced Engagement"));
        db.skills.insert(13, skill(13, "Vengeful Hammers"));
        db.skills.insert(14, skill(14, "Rite of the Great Dwarf"));
        db.skills.insert(2, skill(2, "Legendary Alliance Stance"));
        db.skills.insert(20, skill(20, "Selfish Spirit"));
        db.skills.insert(21, skill(21, "Battle Scorned"));
        db.legends
            .insert("Legend3".into(), legend("Legend3", 1, 10, 14, [11, 12, 13]));
        db.legends
            .insert("Legend8".into(), legend("Legend8", 2, 20, 20, [21, 21, 21]));

        let (heal, utils, elite) = stance_kit(&db, "Dwarf").expect("dwarf kit");
        assert_eq!(heal, "Soothing Stone");
        assert_eq!(utils[0], "Inspiring Reinforcement");
        assert_eq!(elite, "Rite of the Great Dwarf");
        let (heal, _, _) = stance_kit(&db, "Alliance").expect("alliance kit");
        assert_eq!(heal, "Selfish Spirit");
        assert!(stance_kit(&db, "Entity").is_none());
    }
}
