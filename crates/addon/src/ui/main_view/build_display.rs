//! Card-style build display using DrawList for visual card sections.

use nexus::imgui::Ui;

use gw2_core::i18n::t;
use gw2_core::types::ResolvedBuild;

// ─── Color palette ───

const SECTION_HEADER_BG: [f32; 4] = [0.28, 0.24, 0.12, 0.95]; // warm dark gold header
                                                              // Body bg is not drawn (would cover text due to DrawList ordering).
                                                              // Card look comes from header + border + accent line.
const LABEL_COLOR: [f32; 4] = [0.7, 0.7, 0.75, 1.0]; // cool gray labels
const SECTION_TITLE_COLOR: [f32; 4] = [1.0, 0.88, 0.35, 1.0]; // bright gold titles
const ACCENT_LINE_COLOR: [f32; 4] = [0.7, 0.55, 0.15, 0.5]; // subtle gold accent
const CARD_BORDER_COLOR: [f32; 4] = [0.35, 0.3, 0.15, 0.4]; // dim gold border

const CARD_ROUNDING: f32 = 5.0;
const CARD_PAD: f32 = 4.0;
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

// ─── Helpers ───

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
    static STANCE_PREVIEW: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn peek_stance_kit(
    db: Option<&gw2_optimizer::gamedb::GameDb>,
    joined: &str,
) -> Option<(String, Vec<String>, String)> {
    let names = slash_parts(joined);
    if names.is_empty() {
        return None;
    }
    let n = names.len();
    let selected = STANCE_PREVIEW.with(|c| {
        let v = c.get();
        if v >= n {
            c.set(0);
            0
        } else {
            v
        }
    });
    db.and_then(|d| stance_kit(d, names[selected]))
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

fn two_line_split(name: &str) -> Vec<String> {
    let words: Vec<&str> = name.split_whitespace().filter(|w| !w.is_empty()).collect();
    match words.len() {
        0 => vec!["-".into()],
        1 => vec![words[0].into()],
        2 => vec![words[0].into(), words[1].into()],
        n => vec![words[..n - 1].join(" "), words[n - 1].to_string()],
    }
}

fn min_name_col_w(ui: &Ui, name: &str) -> f32 {
    two_line_split(name)
        .iter()
        .map(|l| ui.calc_text_size(l)[0])
        .fold(16.0_f32, f32::max)
}

fn paint_group_header(ui: &Ui, x: f32, y: f32, w: f32, h: f32, title: &str) {
    let dl = ui.get_window_draw_list();
    crate::ui::theme::paint_header_accent(&dl, x, y, h);
    let [tw, th] = ui.calc_text_size(title);
    let inner_left = x + crate::ui::theme::HEADER_ACCENT_W + 4.0;
    let inner_w = (w - crate::ui::theme::HEADER_ACCENT_W - 8.0).max(1.0);
    let tx = inner_left + ((inner_w - tw) * 0.5).max(0.0);
    let ty = y + ((h - th) * 0.5).round();
    dl.add_text([tx, ty], SECTION_TITLE_COLOR, title);
}

fn paint_vdiv(ui: &Ui, x: f32, y: f32, h: f32) {
    ui.get_window_draw_list()
        .add_line([x, y + 1.0], [x, y + h - 1.0], crate::ui::theme::GOLD_DIM)
        .thickness(1.0)
        .build();
}

fn slot_row_w(inner_w: f32, n: usize, gap: f32) -> f32 {
    (inner_w - gap * n.saturating_sub(1) as f32) / n.max(1) as f32
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
#[allow(clippy::too_many_arguments)]
fn paint_kit_slot(
    ui: &Ui,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
    p: [f32; 2],
    slot_w: f32,
    slot_h: f32,
    pad: f32,
    icon: f32,
    icon_text_gap: f32,
    line: f32,
    id: &str,
    value: &str,
    rim: [f32; 4],
    icon_url: Option<&str>,
    inspect: &str,
    icon_zoom: f32,
) {
    let empty = value.is_empty();
    let fill = if empty {
        crate::ui::theme::PLATE_EMPTY
    } else {
        crate::ui::theme::PLATE
    };
    let border = if empty { [0.28, 0.24, 0.14, 0.45] } else { rim };
    ui.set_cursor_screen_pos(p);
    let _ = ui.invisible_button(id, [slot_w, slot_h]);
    if !empty && !inspect.is_empty() {
        crate::ui::comparison::inspect_if_hovered(ui, inspect, db);
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
        let icon_p = [p[0] + pad, p[1] + ((slot_h - icon) * 0.5).round()];
        crate::ui::icons::paint_on_zoomed(
            &dl,
            if empty { None } else { icon_url },
            icon_p,
            [icon_p[0] + icon, icon_p[1] + icon],
            [1.0, 1.0, 1.0, 1.0],
            icon_zoom,
        );
        let text_x = p[0] + pad + icon + icon_text_gap;
        let text_w = (p[0] + slot_w - pad - text_x).max(16.0);
        let shown = if empty {
            "-"
        } else {
            crate::ui::comparison::loc_name(db, value)
        };
        let color = if empty {
            crate::ui::theme::MUTED
        } else {
            crate::ui::theme::CREAM
        };
        let lines = wrap_slot_lines(ui, shown, text_w);
        let block_h = lines.len() as f32 * line;
        let name_y = p[1] + ((slot_h - block_h) * 0.5).round();
        for (i, ln) in lines.iter().enumerate() {
            let lw = ui.calc_text_size(ln)[0];
            let tx = text_x + ((text_w - lw) * 0.5).max(0.0);
            dl.add_text(
                [tx, name_y + i as f32 * line],
                crate::ui::color_u32(color),
                ln,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
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
    let (heal, utilities, elite) = match peek_stance_kit(db, stances) {
        Some((h, u, e)) => (h, u, e),
        None => (heal.to_string(), utilities.to_vec(), elite.to_string()),
    };
    let pet_raw = slash_parts(pets);
    let pet_shown: Vec<String> = pet_raw
        .iter()
        .map(|name| match db.and_then(|d| d.pet_by_name(name)) {
            Some(p) => {
                let loc = db
                    .map(|d| d.loc_pet(p.id, &p.name))
                    .unwrap_or(p.name.as_str());
                crate::ui::comparison::compact_pet_name(loc)
            }
            None => {
                crate::ui::comparison::compact_pet_name(crate::ui::comparison::loc_name(db, name))
            }
        })
        .collect();
    let loc = |s: &str| {
        if s.is_empty() {
            String::new()
        } else {
            crate::ui::comparison::loc_name(db, s).to_string()
        }
    };
    let heal_s = loc(&heal);
    let u1_s = loc(utilities.first().map(|s| s.as_str()).unwrap_or(""));
    let u2_s = loc(utilities.get(1).map(|s| s.as_str()).unwrap_or(""));
    let u3_s = loc(utilities.get(2).map(|s| s.as_str()).unwrap_or(""));
    let elite_s = loc(&elite);
    let g_pets = t("group.pet_skills");
    let g_util = t("group.utility_skills");
    let g_elite = t("group.elite_skill");

    let avail = ui.content_region_avail()[0].max(1.0);
    let gap = 4.0;
    let group_pad = 4.0;
    let div_w = 8.0;
    let line = ui.text_line_height();
    let pad = 4.0;
    let icon = line * 2.0;
    let icon_text_gap = 6.0;
    let bar_h = 22.0;
    let slot_h = pad + icon.max(line * 2.0) + pad;
    let min_slot = |name: &str| {
        let col = if name.is_empty() {
            ui.calc_text_size("-")[0]
        } else {
            min_name_col_w(ui, name)
        };
        pad + icon + icon_text_gap + col + pad
    };
    let group_need = |title: &str, mins: &[f32]| {
        if mins.is_empty() {
            return 0.0;
        }
        let slots = mins.iter().sum::<f32>() + gap * mins.len().saturating_sub(1) as f32;
        (slots + group_pad * 2.0)
            .max(ui.calc_text_size(title)[0] + group_pad * 2.0 + crate::ui::theme::HEADER_ACCENT_W)
    };

    let pet_mins: Vec<f32> = pet_shown.iter().map(|n| min_slot(n)).collect();
    let util_mins = [
        min_slot(&heal_s),
        min_slot(&u1_s),
        min_slot(&u2_s),
        min_slot(&u3_s),
    ];
    let elite_mins = [min_slot(&elite_s)];
    let has_pets = !pet_mins.is_empty();
    let n_div = if has_pets { 2.0 } else { 1.0 };
    let pet_need = group_need(&g_pets, &pet_mins);
    let util_need = group_need(&g_util, &util_mins);
    let elite_need = group_need(&g_elite, &elite_mins);
    let need = pet_need + util_need + elite_need + n_div * div_w;
    let (pet_w, util_w, elite_w) = if need <= avail {
        let extra = avail - need;
        if has_pets {
            (
                pet_need + extra * 0.20,
                util_need + extra * 0.70,
                elite_need + extra * 0.10,
            )
        } else {
            (0.0, util_need + extra * 0.75, elite_need + extra * 0.25)
        }
    } else {
        let leftover = (avail - n_div * div_w - pet_need - elite_need).max(0.0);
        if leftover >= util_need * 0.55 {
            (pet_need, leftover, elite_need)
        } else {
            let scale = ((avail - n_div * div_w) / (pet_need + util_need + elite_need).max(1.0))
                .clamp(0.4, 1.0);
            (pet_need * scale, util_need * scale, elite_need * scale)
        }
    };

    let start = ui.cursor_screen_pos();
    let hdr_top = start[1];
    let hdr_bottom = hdr_top + bar_h;
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect(
            [start[0] - 1.0, hdr_top],
            [start[0] + avail + 1.0, hdr_bottom],
            SECTION_HEADER_BG,
        )
        .filled(true)
        .rounding(CARD_ROUNDING)
        .round_bot_left(false)
        .round_bot_right(false)
        .build();
    }
    let mut hx = start[0];
    if has_pets {
        paint_group_header(ui, hx, hdr_top, pet_w, bar_h, &g_pets);
        hx += pet_w + div_w;
    }
    paint_group_header(ui, hx, hdr_top, util_w, bar_h, &g_util);
    hx += util_w + div_w;
    paint_group_header(ui, hx, hdr_top, elite_w, bar_h, &g_elite);

    ui.dummy([0.0, bar_h + 2.0]);
    let body_top = ui.cursor_screen_pos()[1];
    ui.dummy([0.0, 2.0]);

    let (heal, utilities, elite) = match render_stance_tabs(ui, db, stances, id_suffix) {
        Some((h, u, e)) => (h, u, e),
        None => (heal, utilities, elite),
    };
    let u1 = utilities.first().map(|s| s.as_str()).unwrap_or("");
    let u2 = utilities.get(1).map(|s| s.as_str()).unwrap_or("");
    let u3 = utilities.get(2).map(|s| s.as_str()).unwrap_or("");

    let slot_y = ui.cursor_screen_pos()[1];
    let mut x = start[0];
    if has_pets {
        let inner_x = x + group_pad;
        let inner_w = (pet_w - group_pad * 2.0).max(1.0);
        let n = pet_shown.len();
        let sw = slot_row_w(inner_w, n, gap);
        for (i, (name, shown)) in pet_raw.iter().zip(pet_shown.iter()).enumerate() {
            paint_kit_slot(
                ui,
                db,
                [inner_x + i as f32 * (sw + gap), slot_y],
                sw,
                slot_h,
                pad,
                icon,
                icon_text_gap,
                line,
                &format!("##pet_slot_{id_suffix}_{i}"),
                shown,
                crate::ui::theme::GOLD_DIM,
                db.and_then(|d| crate::ui::icons::pet_url(d, name)),
                name,
                crate::ui::icons::PET_ICON_ZOOM,
            );
        }
        x += pet_w + div_w;
    }

    {
        let inner_x = x + group_pad;
        let inner_w = (util_w - group_pad * 2.0).max(1.0);
        let sw = slot_row_w(inner_w, 4, gap);
        let utils = [
            (0usize, heal.as_str(), crate::ui::theme::HEAL_RIM),
            (1, u1, crate::ui::theme::GOLD_DIM),
            (2, u2, crate::ui::theme::GOLD_DIM),
            (3, u3, crate::ui::theme::GOLD_DIM),
        ];
        for (i, value, rim) in utils {
            paint_kit_slot(
                ui,
                db,
                [inner_x + i as f32 * (sw + gap), slot_y],
                sw,
                slot_h,
                pad,
                icon,
                icon_text_gap,
                line,
                &format!("##skill_slot_{id_suffix}_{i}"),
                value,
                rim,
                db.and_then(|d| crate::ui::icons::skill_url_by_name(d, value)),
                value,
                1.0,
            );
        }
        x += util_w + div_w;
    }

    {
        let inner_x = x + group_pad;
        let inner_w = (elite_w - group_pad * 2.0).max(1.0);
        paint_kit_slot(
            ui,
            db,
            [inner_x, slot_y],
            inner_w,
            slot_h,
            pad,
            icon,
            icon_text_gap,
            line,
            &format!("##skill_slot_{id_suffix}_elite"),
            elite.as_str(),
            crate::ui::theme::ELITE_RIM,
            db.and_then(|d| crate::ui::icons::skill_url_by_name(d, elite.as_str())),
            elite.as_str(),
            1.0,
        );
    }

    ui.set_cursor_screen_pos([start[0], slot_y + slot_h]);
    ui.dummy([avail, 0.0]);
    ui.dummy([0.0, CARD_PAD]);
    let body_bottom = ui.cursor_screen_pos()[1];
    {
        let dl = ui.get_window_draw_list();
        dl.add_line(
            [start[0] - 1.0, body_top],
            [start[0] + avail + 1.0, body_top],
            ACCENT_LINE_COLOR,
        )
        .thickness(1.0)
        .build();
        dl.add_rect(
            [start[0] - 1.0, hdr_top],
            [start[0] + avail + 1.0, body_bottom],
            CARD_BORDER_COLOR,
        )
        .rounding(CARD_ROUNDING)
        .build();
    }
    let card_h = body_bottom - hdr_top;
    let mut vx = start[0];
    if has_pets {
        vx += pet_w;
        paint_vdiv(ui, vx + div_w * 0.5, hdr_top, card_h);
        vx += div_w;
    }
    vx += util_w;
    paint_vdiv(ui, vx + div_w * 0.5, hdr_top, card_h);
    ui.dummy([0.0, CARD_GAP]);
}

// ─── Individual section renderers (for column-aligned layouts) ───

pub fn render_build_skills(
    ui: &Ui,
    build: &ResolvedBuild,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
) {
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
}

pub fn render_suggestion_skills(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    db: Option<&gw2_optimizer::gamedb::GameDb>,
) {
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
}

#[cfg(test)]
mod tests {
    use super::{stance_kit, two_line_split};
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

    #[test]
    fn two_line_split_puts_last_word_on_line_two() {
        assert_eq!(
            two_line_split("Siege Turtle"),
            vec!["Siege".to_string(), "Turtle".to_string()]
        );
        assert_eq!(
            two_line_split("Glyph of Equality"),
            vec!["Glyph of".to_string(), "Equality".to_string()]
        );
        assert_eq!(two_line_split("Entangle"), vec!["Entangle".to_string()]);
    }
}
