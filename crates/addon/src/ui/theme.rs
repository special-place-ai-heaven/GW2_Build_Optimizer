//! Tyrian night overlay tokens. Dark stone + warm gold — GW2, not a dashboard.
//! Hold the return value of [`push`] for the whole window frame or styles pop.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use nexus::imgui::{DrawListMut, StyleColor, StyleVar, TextureId, Ui};

use super::color_u32;

pub const INK: [f32; 4] = [0.07, 0.06, 0.045, 0.96];
pub const CHILD: [f32; 4] = [0.09, 0.08, 0.055, 0.35];
pub const CREAM: [f32; 4] = [0.93, 0.90, 0.82, 1.0];
pub const MUTED: [f32; 4] = [0.58, 0.54, 0.46, 1.0];
pub const GOLD: [f32; 4] = [1.0, 0.84, 0.38, 1.0];
pub const GOLD_DIM: [f32; 4] = [0.55, 0.44, 0.18, 0.85];
pub const GOLD_FILL: [f32; 4] = [0.78, 0.62, 0.22, 0.95];
pub const GOLD_HOVER: [f32; 4] = [0.22, 0.18, 0.08, 0.95];
pub const PLATE: [f32; 4] = [0.12, 0.10, 0.07, 0.94];
pub const PLATE_EMPTY: [f32; 4] = [0.08, 0.07, 0.055, 0.75];
pub const CURRENT: [f32; 4] = [0.62, 0.82, 1.0, 1.0];
pub const OPTIMIZED: [f32; 4] = [0.55, 0.92, 0.62, 1.0];
pub const HEAL_RIM: [f32; 4] = [0.42, 0.78, 0.48, 0.95];
pub const ELITE_RIM: [f32; 4] = [0.95, 0.62, 0.22, 0.95];
pub const ERR: [f32; 4] = [1.0, 0.38, 0.28, 1.0];
pub const WARN: [f32; 4] = [1.0, 0.72, 0.28, 1.0];
/// Same as `FrameRounding` in [`push`] — icons match buttons/windows.
pub const ICON_ROUNDING: f32 = 5.0;
/// Gold tick on section headers. Title starts this far after the tick.
pub const HEADER_ACCENT_W: f32 = 3.0;
pub const HEADER_TITLE_GAP: f32 = 10.0;

pub fn header_title_x(left: f32) -> f32 {
    left + HEADER_ACCENT_W + HEADER_TITLE_GAP
}

pub fn paint_header_accent(draw: &DrawListMut, left: f32, top: f32, height: f32) {
    draw.add_rect([left, top], [left + HEADER_ACCENT_W, top + height], GOLD)
        .filled(true)
        .rounding(2.0)
        .build();
}

fn fade(c: [f32; 4], opacity: f32) -> [f32; 4] {
    [c[0], c[1], c[2], c[3] * opacity.clamp(0.3, 1.0)]
}

/// Push overlay colors/rounding. Keep the value alive for the window frame.
pub fn push<'ui>(ui: &'ui Ui<'_>, opacity: f32) -> impl Sized + 'ui {
    let a = opacity.clamp(0.3, 1.0);
    (
        ui.push_style_var(StyleVar::WindowRounding(8.0)),
        ui.push_style_var(StyleVar::ChildRounding(6.0)),
        ui.push_style_var(StyleVar::FrameRounding(5.0)),
        ui.push_style_var(StyleVar::GrabRounding(4.0)),
        ui.push_style_var(StyleVar::PopupRounding(6.0)),
        ui.push_style_var(StyleVar::WindowBorderSize(1.0)),
        ui.push_style_var(StyleVar::WindowPadding([24.0, 16.0])),
        ui.push_style_color(StyleColor::WindowBg, fade(INK, a)),
        ui.push_style_color(StyleColor::ChildBg, fade(CHILD, a)),
        ui.push_style_color(StyleColor::PopupBg, fade([0.08, 0.07, 0.05, 0.98], a)),
        ui.push_style_color(StyleColor::Border, GOLD_DIM),
        ui.push_style_color(StyleColor::FrameBg, [0.12, 0.10, 0.07, 0.95]),
        ui.push_style_color(StyleColor::FrameBgHovered, [0.18, 0.15, 0.09, 1.0]),
        ui.push_style_color(StyleColor::FrameBgActive, [0.22, 0.18, 0.08, 1.0]),
        ui.push_style_color(StyleColor::Button, [0.32, 0.25, 0.08, 0.95]),
        ui.push_style_color(StyleColor::ButtonHovered, [0.46, 0.36, 0.10, 1.0]),
        ui.push_style_color(StyleColor::ButtonActive, [0.58, 0.45, 0.12, 1.0]),
        ui.push_style_color(StyleColor::Header, [0.22, 0.18, 0.08, 0.9]),
        ui.push_style_color(StyleColor::HeaderHovered, [0.30, 0.24, 0.10, 1.0]),
        ui.push_style_color(StyleColor::TitleBg, fade([0.10, 0.08, 0.05, 1.0], a)),
        ui.push_style_color(StyleColor::TitleBgActive, fade([0.16, 0.12, 0.06, 1.0], a)),
        ui.push_style_color(StyleColor::Separator, GOLD_DIM),
        ui.push_style_color(StyleColor::CheckMark, GOLD),
        ui.push_style_color(StyleColor::SliderGrab, GOLD_FILL),
        ui.push_style_color(StyleColor::Text, CREAM),
        ui.push_style_color(StyleColor::TextDisabled, MUTED),
        ui.push_style_color(StyleColor::ScrollbarGrab, GOLD_DIM),
    )
}

/// Shared frame pad so InputText and gold buttons are the same height.
pub fn control_pad(ui: &Ui) -> [f32; 2] {
    let s = (ui.current_font_size() / 13.0).max(0.75);
    [6.0 * s, 4.0 * s]
}

/// Font size + pad.y*2 — use this for sized buttons next to InputText.
pub fn control_height(ui: &Ui) -> f32 {
    let p = control_pad(ui);
    (ui.current_font_size() + p[1] * 2.0).round()
}

/// Gold plate button (Copy, Send, Test). Dark text on gold so it reads as the action.
pub fn gold_button(ui: &Ui, label: impl AsRef<str>) -> bool {
    let _pad = push_gold_button_pad(ui);
    let _bg = ui.push_style_color(StyleColor::Button, GOLD_FILL);
    let _h = ui.push_style_color(StyleColor::ButtonHovered, [0.90, 0.74, 0.28, 1.0]);
    let _a = ui.push_style_color(StyleColor::ButtonActive, [0.70, 0.55, 0.16, 1.0]);
    let _t = ui.push_style_color(StyleColor::Text, [0.10, 0.08, 0.04, 1.0]);
    ui.button(label.as_ref())
}

pub fn gold_button_sized(ui: &Ui, label: impl AsRef<str>, size: [f32; 2]) -> bool {
    let label = label.as_ref();
    let visible = label.split("##").next().unwrap_or(label);
    let (pad_x, _) = gold_button_pad(ui);
    let need_w = ui.calc_text_size(visible)[0] + pad_x * 2.0;
    let w = if size[0] < 0.0 {
        size[0]
    } else if size[0] <= 0.0 {
        need_w
    } else {
        size[0].max(need_w)
    };
    let h = if size[1] <= 0.0 {
        control_height(ui)
    } else {
        size[1]
    };
    let _pad = push_gold_button_pad(ui);
    let _bg = ui.push_style_color(StyleColor::Button, GOLD_FILL);
    let _h = ui.push_style_color(StyleColor::ButtonHovered, [0.90, 0.74, 0.28, 1.0]);
    let _a = ui.push_style_color(StyleColor::ButtonActive, [0.70, 0.55, 0.16, 1.0]);
    let _t = ui.push_style_color(StyleColor::Text, [0.10, 0.08, 0.04, 1.0]);
    ui.button_with_size(label, [w, h])
}

fn gold_button_pad(ui: &Ui) -> (f32, f32) {
    let p = control_pad(ui);
    let extra_x = 4.0 * (ui.current_font_size() / 13.0).max(0.75);
    (p[0] + extra_x, p[1])
}

fn push_gold_button_pad<'ui>(ui: &'ui Ui<'_>) -> impl Sized + 'ui {
    let (pad_x, pad_y) = gold_button_pad(ui);
    ui.push_style_var(StyleVar::FramePadding([pad_x, pad_y]))
}

/// Rounded pill tab. `id` must be unique (used as an invisible button id).
pub fn pill(ui: &Ui, label: &str, selected: bool, id: &str) -> bool {
    pill_pulse(ui, label, selected, id, 0.0)
}

/// `pulse` 0..=1 gold blink for a waiting result tab.
pub fn pill_pulse(ui: &Ui, label: &str, selected: bool, id: &str, pulse: f32) -> bool {
    let pad_x = 10.0;
    let pad_y = 3.0;
    let sz = ui.calc_text_size(label);
    let w = (sz[0] + pad_x * 2.0).max(36.0);
    let h = sz[1] + pad_y * 2.0;
    let p = ui.cursor_screen_pos();
    let clicked = ui.invisible_button(id, [w, h]);
    let hovered = ui.is_item_hovered();
    let pulse = pulse.clamp(0.0, 1.0);
    let mut fill = if selected {
        GOLD_FILL
    } else if hovered {
        GOLD_HOVER
    } else {
        [0.10, 0.09, 0.06, 0.55]
    };
    let mut rim = if selected {
        GOLD
    } else if hovered {
        GOLD_DIM
    } else {
        [0.32, 0.26, 0.12, 0.55]
    };
    if pulse > 0.0 && !selected {
        fill = [
            fill[0] + (GOLD_FILL[0] - fill[0]) * pulse,
            fill[1] + (GOLD_FILL[1] - fill[1]) * pulse,
            fill[2] + (GOLD_FILL[2] - fill[2]) * pulse,
            0.55 + 0.40 * pulse,
        ];
        rim = [GOLD[0], GOLD[1], GOLD[2], 0.45 + 0.55 * pulse];
    }
    let text = if selected {
        [0.10, 0.08, 0.04, 1.0]
    } else if pulse > 0.0 {
        let d = [0.10, 0.08, 0.04, 1.0];
        [
            CREAM[0] + (d[0] - CREAM[0]) * pulse,
            CREAM[1] + (d[1] - CREAM[1]) * pulse,
            CREAM[2] + (d[2] - CREAM[2]) * pulse,
            1.0,
        ]
    } else {
        CREAM
    };
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], fill)
            .filled(true)
            .rounding(h * 0.45)
            .build();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], rim)
            .rounding(h * 0.45)
            .build();
        dl.add_text([p[0] + pad_x, p[1] + pad_y], color_u32(text), label);
    }
    clicked
}

pub const PIP_DAMAGE: [f32; 4] = [0.90, 0.32, 0.28, 1.0];
pub const PIP_FRONT: [f32; 4] = [0.80, 0.82, 0.86, 1.0];
pub const PIP_HEAL: [f32; 4] = [0.32, 0.78, 0.48, 1.0];
pub const PIP_CTRL: [f32; 4] = [0.95, 0.70, 0.22, 1.0];

pub fn select_chip_size(ui: &Ui, label: &str, pip: bool) -> [f32; 2] {
    let sz = ui.calc_text_size(label);
    let extra = if pip { 12.0 } else { 0.0 };
    [(sz[0] + 16.0 + extra).max(28.0), sz[1] + 6.0]
}

/// GW2Mists-style filter chip. `pip` is the role-family color on the left.
pub fn select_chip(ui: &Ui, label: &str, selected: bool, id: &str, pip: Option<[f32; 4]>) -> bool {
    let [w, h] = select_chip_size(ui, label, pip.is_some());
    let p = ui.cursor_screen_pos();
    let clicked = ui.invisible_button(id, [w, h]);
    let hovered = ui.is_item_hovered();
    let fill = if selected {
        GOLD_FILL
    } else if hovered {
        GOLD_HOVER
    } else {
        [0.10, 0.09, 0.06, 0.55]
    };
    let rim = if selected {
        GOLD
    } else if hovered {
        GOLD_DIM
    } else {
        [0.32, 0.26, 0.12, 0.55]
    };
    let text = if selected {
        [0.10, 0.08, 0.04, 1.0]
    } else {
        CREAM
    };
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], fill)
            .filled(true)
            .rounding(h * 0.45)
            .build();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], rim)
            .rounding(h * 0.45)
            .build();
        let mut tx = p[0] + 8.0;
        if let Some(pip) = pip {
            let cy = p[1] + h * 0.5;
            dl.add_circle([tx + 3.5, cy], 3.5, pip).filled(true).build();
            tx += 12.0;
        }
        dl.add_text([tx, p[1] + 3.0], color_u32(text), label);
    }
    clicked
}

/// Place the next chip on this line, or wrap. `avail` is the row width captured
/// before any chips (content_region_avail shrinks after `same_line`).
pub fn wrap_chip(ui: &Ui, avail: f32, row_x: &mut f32, chip_w: f32, gap: f32) {
    if *row_x > 0.5 && *row_x + chip_w + gap > avail {
        *row_x = 0.0;
    } else if *row_x > 0.5 {
        ui.same_line_with_spacing(0.0, gap);
    }
    *row_x += chip_w + gap;
}

/// Width needed so every label in a segment row fits without clipping.
pub fn segment_row_min_width(ui: &Ui, labels: &[&str]) -> f32 {
    if labels.is_empty() {
        return 0.0;
    }
    let widest = labels
        .iter()
        .map(|l| ui.calc_text_size(l)[0])
        .fold(0.0_f32, f32::max);
    let n = labels.len() as f32;
    n * (widest + 16.0) + 3.0 * (n - 1.0)
}

/// Exclusive pills. Each pill is at least as wide as the longest label.
pub fn segment_row(ui: &Ui, labels: &[&str], selected: usize, id_prefix: &str) -> Option<usize> {
    let n = labels.len();
    if n == 0 {
        return None;
    }
    let avail = ui.content_region_avail()[0];
    let gap = 3.0;
    let th = ui.calc_text_size(labels[0])[1];
    let h = th + 8.0;
    let widest = labels
        .iter()
        .map(|l| ui.calc_text_size(l)[0])
        .fold(0.0_f32, f32::max);
    let fit = widest + 16.0;
    let share = ((avail - gap * (n as f32 - 1.0)) / n as f32).max(28.0);
    let w = share.max(fit);
    let mut clicked = None;
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            ui.same_line_with_spacing(0.0, gap);
        }
        let id = format!("{id_prefix}{i}");
        let p = ui.cursor_screen_pos();
        if ui.invisible_button(&id, [w, h]) {
            clicked = Some(i);
        }
        let hovered = ui.is_item_hovered();
        let on = i == selected;
        let fill = if on {
            GOLD_FILL
        } else if hovered {
            GOLD_HOVER
        } else {
            [0.10, 0.09, 0.06, 0.55]
        };
        let rim = if on {
            GOLD
        } else if hovered {
            GOLD_DIM
        } else {
            [0.32, 0.26, 0.12, 0.55]
        };
        let text = if on { [0.10, 0.08, 0.04, 1.0] } else { CREAM };
        let dl = ui.get_window_draw_list();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], fill)
            .filled(true)
            .rounding(h * 0.45)
            .build();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], rim)
            .rounding(h * 0.45)
            .build();
        let tw = ui.calc_text_size(label)[0];
        dl.add_text(
            [p[0] + (w - tw) * 0.5, p[1] + (h - th) * 0.5],
            color_u32(text),
            label,
        );
    }
    clicked.filter(|&i| i != selected)
}

/// Wrap to the current content edge. `text_colored` never wraps, so it clips.
pub fn wrapped(ui: &Ui, color: [f32; 4], text: &str) {
    let wrap_x = ui.cursor_screen_pos()[0] + ui.content_region_avail()[0].max(8.0);
    let wrap = ui.push_text_wrap_pos_with_pos(wrap_x);
    {
        let _c = ui.push_style_color(StyleColor::Text, color);
        ui.text_wrapped(text);
    }
    wrap.pop(ui);
}

/// Small name chip (stances / pets). `id` must be unique per row.
pub fn chip(ui: &Ui, text: &str, id: &str) {
    let pad = 6.0;
    let sz = ui.calc_text_size(text);
    let w = sz[0] + pad * 2.0;
    let h = sz[1] + 4.0;
    let p = ui.cursor_screen_pos();
    let _ = ui.invisible_button(id, [w, h]);
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], PLATE)
            .filled(true)
            .rounding(h * 0.4)
            .build();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], GOLD_DIM)
            .rounding(h * 0.4)
            .build();
        dl.add_text([p[0] + pad, p[1] + 2.0], color_u32(CREAM), text);
    }
}

/// Gold-tick section header — same plate as build cards, used on Setup/Settings too.
pub fn header(ui: &Ui, title: &str) {
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect(
            [start[0] - 1.0, start[1]],
            [start[0] + width + 1.0, start[1] + 22.0],
            [0.15, 0.13, 0.08, 0.9],
        )
        .filled(true)
        .rounding(5.0)
        .round_bot_left(false)
        .round_bot_right(false)
        .build();
        paint_header_accent(&dl, start[0], start[1], 22.0);
        let th = ui.calc_text_size(title)[1];
        let ty = start[1] + ((22.0 - th) * 0.5).round();
        dl.add_text([header_title_x(start[0]), ty], color_u32(GOLD), title);
    }
    ui.dummy([0.0, 24.0]);
}

/// Tooltips auto-size from content; wrap-width is ~0 until then, so
/// `text_wrapped` becomes a one-glyph column. Pin wrap at 360px.
pub fn wide_tooltip(ui: &Ui, body: impl FnOnce(&Ui)) {
    ui.tooltip(|| {
        let wrap = ui.push_text_wrap_pos_with_pos(360.0);
        body(ui);
        wrap.pop(ui);
    });
}

fn scribble_hash(x: i32, y: i32, salt: u32) -> u32 {
    let mut n = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ salt;
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n ^ (n >> 16)
}

fn checkpoint_fill(fraction: f32, t: f32) -> f32 {
    ((fraction - t + 0.06) / 0.14).clamp(0.0, 1.0)
}

fn hatch_span(cx: f32, cy: f32, r: f32, y: f32) -> Option<(f32, f32)> {
    let dy = y - cy;
    let t = r * r - dy * dy;
    if t <= 0.0 {
        return None;
    }
    let dx = t.sqrt();
    Some((cx - dx, cx + dx))
}

/// Doodle download bar: gold scribble fill, mystic-coin checkpoints on the
/// track, choya rocking the leading edge.
pub fn download_scribble(ui: &Ui, fraction: f32, caption: &str) {
    let fraction = fraction.clamp(0.0, 1.0);
    const SIDE: f32 = 28.0;
    let full = ui.content_region_avail()[0].max(120.0);
    let avail = (full - SIDE * 2.0).max(120.0);
    let bar_h = 22.0;
    let choya_h = bar_h * 3.0;
    let coin_r: f32 = 15.0;
    let chest_h: f32 = 80.0;
    let chest_w = chest_h * (308.0 / 256.0);
    let line_gap = 8.0;
    let title_h = ui.text_line_height() + 4.0;
    let headroom = choya_h - bar_h + 4.0;
    let coin_h = coin_r * 2.0;
    const CAP_SCALE: f32 = 1.45;
    const CAP_GAP: f32 = 28.0;
    const NOTE: &str = "Speed depends on GW2 servers, not this addon.";
    const NOTE_SCALE: f32 = 0.68;
    ui.set_window_font_scale(CAP_SCALE);
    let cap_sz = ui.calc_text_size(caption);
    ui.set_window_font_scale(NOTE_SCALE);
    let note_sz = ui.calc_text_size(NOTE);
    ui.set_window_font_scale(1.0);
    let total_h = title_h
        + headroom
        + bar_h
        + line_gap
        + 6.0
        + coin_h
        + CAP_GAP
        + cap_sz[1]
        + 3.0
        + note_sz[1]
        + 8.0;
    let origin = ui.cursor_screen_pos();
    let p = [origin[0] + SIDE, origin[1]];
    let _ = ui.invisible_button("##dl_scribble", [full, total_h]);
    let t = ui.frame_count() as f32;
    let labels = ["Bronze", "Silver", "Gold", "GEM"];
    let end_half = ui.calc_text_size(labels[0])[0].max(ui.calc_text_size(labels[3])[0]) * 0.5 + 6.0;
    {
        let dl = ui.get_window_draw_list();

        dl.add_text([p[0], p[1]], color_u32(GOLD), "FETCHING TYRIA...");

        let bar_x = p[0];
        let bar_y = p[1] + title_h + headroom;
        let left_pad = (coin_r + 4.0).max(end_half);
        let track_gap = 14.0;
        let right_reserve = chest_w + track_gap + 10.0;
        let track_x0 = bar_x + left_pad;
        let span = (avail - left_pad - right_reserve).max(40.0);
        let track_x1 = track_x0 + span;
        let fill_x = track_x0 + span * fraction;
        let empty = [0.32, 0.28, 0.22, 0.55];
        let gold_scribble = [0.95, 0.78, 0.28, 0.95];
        let gold_scribble_dim = [0.72, 0.55, 0.16, 0.85];

        let y0 = bar_y as i32;
        let y1 = (bar_y + bar_h) as i32;
        let x0 = track_x0 as i32;
        let x1 = track_x1 as i32;
        for y in (y0..y1).step_by(2) {
            for x in (x0..x1).step_by(3) {
                let h = scribble_hash(x, y, 0xA11CE);
                let jagged = fill_x + ((h % 11) as f32) - 5.0;
                let filled = (x as f32) < jagged;
                let color = if filled {
                    if h & 1 == 0 {
                        gold_scribble
                    } else {
                        gold_scribble_dim
                    }
                } else {
                    empty
                };
                let len = 4.0 + (h % 4) as f32;
                let slant = if h & 2 == 0 { 2.2 } else { -1.6 };
                dl.add_line(
                    [x as f32, y as f32 + ((h >> 3) % 3) as f32],
                    [x as f32 + len, y as f32 + slant],
                    color,
                )
                .thickness(1.15)
                .build();
            }
        }

        let line_y = bar_y + bar_h + line_gap;
        let mut prev = [track_x0, line_y];
        let mut x = track_x0 + 4.0;
        while x <= track_x1 {
            let wobble = ((x * 0.18 + t * 0.04).sin()) * 1.1;
            let cur = [x, line_y + wobble];
            dl.add_line(prev, cur, GOLD_DIM).thickness(1.2).build();
            prev = cur;
            x += 5.0;
        }

        let coins = [
            (0.0_f32, [0.78, 0.45, 0.18, 1.0]),
            (1.0 / 3.0, [0.78, 0.80, 0.84, 1.0]),
            (2.0 / 3.0, GOLD),
        ];
        let mark_top = line_y + 6.0;
        let coin_bottom = mark_top + coin_h;
        let label_y = coin_bottom + 2.0;
        for (i, (mark, metal)) in coins.iter().enumerate() {
            let cx = track_x0 + span * mark;
            let fill = checkpoint_fill(fraction, *mark);
            draw_mystic_coin(&dl, [cx, mark_top + coin_r], coin_r, i, fill, *metal);
            let label = labels[i];
            let tw = ui.calc_text_size(label)[0];
            dl.add_text([cx - tw * 0.5, label_y], color_u32(MUTED), label);
        }

        let chest_cx = track_x1 + track_gap + chest_w * 0.5;
        let chest_top = coin_bottom - chest_h;
        draw_gem_chest(&dl, [chest_cx, chest_top], chest_h);
        let gem_tw = ui.calc_text_size(labels[3])[0];
        dl.add_text(
            [chest_cx - gem_tw * 0.5, label_y],
            color_u32(MUTED),
            labels[3],
        );

        let t_slow = t * 0.032;
        let sway = t_slow.sin();
        let hop = (t_slow * 0.7).sin().abs() * 6.0;
        let feet_x = (fill_x + sway * 6.0).clamp(track_x0, track_x1);
        draw_choya(
            ui,
            &dl,
            [feet_x, bar_y + bar_h - hop],
            choya_h,
            sway,
            track_x1,
        );

        ui.set_window_font_scale(CAP_SCALE);
        let cap_sz = ui.calc_text_size(caption);
        let cap_x = origin[0] + ((full - cap_sz[0]) * 0.5).max(0.0);
        let cap_y = label_y + CAP_GAP;
        dl.add_text([cap_x, cap_y], color_u32(GOLD), caption);
        ui.set_window_font_scale(NOTE_SCALE);
        let note_sz = ui.calc_text_size(NOTE);
        let note_x = origin[0] + ((full - note_sz[0]) * 0.5).max(0.0);
        let note_y = cap_y + cap_sz[1] + 3.0;
        let faint = [MUTED[0], MUTED[1], MUTED[2], 0.42];
        dl.add_text([note_x, note_y], color_u32(faint), NOTE);
        ui.set_window_font_scale(1.0);
    }
}

fn draw_coin(dl: &DrawListMut, c: [f32; 2], r: f32, metal: [f32; 4], fill: f32) {
    dl.add_circle(c, r, [0.10, 0.08, 0.05, 0.9]).build();
    let y_top = c[1] + r - fill * 2.0 * r;
    let y0 = y_top.max(c[1] - r) as i32;
    let y1 = (c[1] + r) as i32;
    for y in y0..=y1 {
        if let Some((x0, x1)) = hatch_span(c[0], c[1], r - 0.8, y as f32) {
            dl.add_line([x0, y as f32], [x1, y as f32], metal)
                .thickness(1.0)
                .build();
        }
    }
    dl.add_circle(c, r, metal).build();
    dl.add_circle(c, r * 0.55, metal).build();
}

fn embedded_tex(key: &'static str, bytes: &'static [u8]) -> Option<TextureId> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nexus::texture::get_texture_or_create_from_memory(key, bytes)
    }))
    .ok()
    .flatten()
    .map(|t| t.id())
}

fn mystic_coin_tex(metal: usize) -> Option<TextureId> {
    let (key, bytes): (&str, &[u8]) = match metal {
        0 => (
            "GW2BO_MYSTIC_BRONZE2",
            include_bytes!("../../assets/mystic_coin_bronze.png"),
        ),
        1 => (
            "GW2BO_MYSTIC_SILVER2",
            include_bytes!("../../assets/mystic_coin_silver.png"),
        ),
        2 => (
            "GW2BO_MYSTIC_GOLD2",
            include_bytes!("../../assets/mystic_coin_gold.png"),
        ),
        _ => return None,
    };
    embedded_tex(key, bytes)
}

const CHOYA_SHEET_W: f32 = 1536.0;
const CHOYA_SHEET_H: f32 = 1024.0;
/// Pixel rect on `choya_animated.png`: x, y, w, h
const CHOYA_HERO: [f32; 4] = [42.0, 18.0, 456.0, 460.0];
const CHOYA_DANCE: [f32; 4] = [523.0, 98.0, 248.0, 277.0];
const CHOYA_THINK: [f32; 4] = [1101.0, 634.0, 117.0, 89.0];
const CHOYA_IDLE: [[f32; 4]; 9] = [
    [446.0, 417.0, 111.0, 119.0],
    [559.0, 415.0, 118.0, 122.0],
    [694.0, 418.0, 99.0, 118.0],
    [798.0, 421.0, 100.0, 115.0],
    [912.0, 421.0, 108.0, 115.0],
    [1030.0, 421.0, 104.0, 116.0],
    [1146.0, 424.0, 89.0, 115.0],
    [1263.0, 425.0, 106.0, 116.0],
    [1387.0, 417.0, 106.0, 123.0],
];

/// `choya_animated_02.png` — same atlas size, isolated props + portraits.
const CHOYA2_HERO: [f32; 4] = [8.0, 7.0, 286.0, 269.0];
const CHOYA2_PEEK: [f32; 4] = [870.0, 628.0, 208.0, 133.0];
const CHOYA2_PARTY: [f32; 4] = [1198.0, 297.0, 261.0, 259.0];
const CHOYA2_WALK: [[f32; 4]; 6] = [
    [5.0, 569.0, 122.0, 170.0],
    [138.0, 572.0, 125.0, 164.0],
    [270.0, 570.0, 139.0, 170.0],
    [414.0, 570.0, 142.0, 164.0],
    [567.0, 571.0, 147.0, 164.0],
    [707.0, 573.0, 154.0, 166.0],
];
const CHOYA2_FACE: [[f32; 4]; 5] = [
    [4.0, 784.0, 150.0, 190.0],
    [162.0, 792.0, 145.0, 178.0],
    [312.0, 784.0, 148.0, 184.0],
    [469.0, 797.0, 156.0, 176.0],
    [635.0, 790.0, 155.0, 183.0],
];
const CHOYA2_SOMBRERO: [f32; 4] = [1198.0, 799.0, 180.0, 108.0];
const CHOYA2_SHADES: [f32; 4] = [1378.0, 804.0, 138.0, 51.0];
const CHOYA2_MARACA: [f32; 4] = [842.0, 814.0, 68.0, 138.0];
const CHOYA2_MARACA2: [f32; 4] = [801.0, 897.0, 93.0, 110.0];
const CHOYA2_NOTE: [f32; 4] = [1145.0, 874.0, 32.0, 40.0];
const CHOYA2_HEART: [f32; 4] = [964.0, 944.0, 43.0, 44.0];
const CHOYA2_LEI: [f32; 4] = [1396.0, 863.0, 121.0, 65.0];
/// Belly-sleep + Zzz on sheet 1 (idle composer / header).
const CHOYA_SLEEP: [f32; 4] = [1180.0, 700.0, 311.0, 265.0];

fn choya_sheet() -> Option<TextureId> {
    embedded_tex(
        "GW2BO_CHOYA_SHEET",
        include_bytes!("../../assets/choya_animated.png"),
    )
}

fn choya_sheet2() -> Option<TextureId> {
    embedded_tex(
        "GW2BO_CHOYA_SHEET2",
        include_bytes!("../../assets/choya_animated_02.png"),
    )
}

fn sheet_uv(frame: [f32; 4]) -> ([f32; 2], [f32; 2]) {
    let [x, y, w, h] = frame;
    (
        [x / CHOYA_SHEET_W, y / CHOYA_SHEET_H],
        [(x + w) / CHOYA_SHEET_W, (y + h) / CHOYA_SHEET_H],
    )
}

fn blit_choya_frame(dl: &DrawListMut, center: [f32; 2], size: f32, frame: [f32; 4]) {
    let Some(tid) = choya_sheet() else {
        return;
    };
    blit_frame(dl, tid, center, size, frame);
}

fn blit_frame(dl: &DrawListMut, tid: TextureId, center: [f32; 2], size: f32, frame: [f32; 4]) {
    let [_, _, w, h] = frame;
    let aspect = (w / h).max(0.01);
    let (dw, dh) = if aspect > 1.0 {
        (size, size / aspect)
    } else {
        (size * aspect, size)
    };
    let pmin = [center[0] - dw * 0.5, center[1] - dh * 0.5];
    let pmax = [center[0] + dw * 0.5, center[1] + dh * 0.5];
    let (uv0, uv1) = sheet_uv(frame);
    dl.add_image(tid, pmin, pmax)
        .uv_min(uv0)
        .uv_max(uv1)
        .build();
}

fn draw_choya_sprite(dl: &DrawListMut, feet: [f32; 2], height: f32, sway: f32) {
    let (tid, frame) = if let Some(tid) = choya_sheet2() {
        (tid, CHOYA2_PARTY)
    } else if let Some(tid) = choya_sheet() {
        (tid, CHOYA_DANCE)
    } else {
        return;
    };
    let [_, _, fw, fh] = frame;
    let h = height;
    let w = h * (fw / fh);
    let tilt = sway * 0.18;
    let (c, s) = (tilt.cos(), tilt.sin());
    let rot =
        |dx: f32, dy: f32| -> [f32; 2] { [feet[0] + dx * c - dy * s, feet[1] + dx * s + dy * c] };
    let (uv0, uv1) = sheet_uv(frame);
    dl.add_image_quad(
        tid,
        rot(-w * 0.5, -h),
        rot(w * 0.5, -h),
        rot(w * 0.5, 0.0),
        rot(-w * 0.5, 0.0),
    )
    .uv(
        [uv0[0], uv0[1]],
        [uv1[0], uv0[1]],
        [uv1[0], uv1[1]],
        [uv0[0], uv1[1]],
    )
    .build();
}

/// Piñata choya standing on `feet`, slow bob + rock, chatting.
fn draw_choya(ui: &Ui, dl: &DrawListMut, feet: [f32; 2], height: f32, sway: f32, bar_right: f32) {
    draw_choya_sprite(dl, feet, height, sway);
    let [_, _, fw, fh] = CHOYA2_PARTY;
    let w = height * (fw / fh);

    let Some(text) = choya_quip() else {
        return;
    };
    let pad = 6.0;
    let sz = ui.calc_text_size(text);
    let bw = sz[0] + pad * 2.0;
    let bh = sz[1] + pad;
    let gap = 12.0;
    let mut bx = feet[0] + w * 0.5 + gap;
    if bx + bw > bar_right {
        bx = feet[0] - w * 0.5 - gap - bw;
    }
    let by = feet[1] - height * 0.78;
    dl.add_rect([bx, by], [bx + bw, by + bh], PLATE)
        .filled(true)
        .rounding(6.0)
        .build();
    dl.add_rect([bx, by], [bx + bw, by + bh], GOLD_DIM)
        .rounding(6.0)
        .build();
    dl.add_text([bx + pad, by + pad * 0.4], color_u32(CREAM), text);
}

/// Face portraits from sheet 2. `center` is the avatar slot center.
pub fn draw_choya_avatar(ui: &Ui, center: [f32; 2], size: f32) {
    let Some(tid) = choya_sheet2() else {
        let i = (ui.frame_count() as usize / 6) % CHOYA_IDLE.len();
        blit_choya_frame(&ui.get_window_draw_list(), center, size, CHOYA_IDLE[i]);
        return;
    };
    let i = (ui.frame_count() as usize / 48) % CHOYA2_FACE.len();
    blit_frame(
        &ui.get_window_draw_list(),
        tid,
        center,
        size,
        CHOYA2_FACE[i],
    );
}

/// Standing pose plus isolated props (sombrero, shades, maracas, notes).
pub fn draw_choya_hero(ui: &Ui, center: [f32; 2], size: f32) {
    let dl = ui.get_window_draw_list();
    let Some(tid) = choya_sheet2() else {
        blit_choya_frame(&dl, center, size, CHOYA_HERO);
        return;
    };
    blit_frame(&dl, tid, center, size, CHOYA2_HERO);
    blit_frame(
        &dl,
        tid,
        [center[0], center[1] + size * 0.22],
        size * 0.55,
        CHOYA2_LEI,
    );
    blit_frame(
        &dl,
        tid,
        [center[0], center[1] - size * 0.46],
        size * 0.78,
        CHOYA2_SOMBRERO,
    );
    blit_frame(
        &dl,
        tid,
        [center[0], center[1] - size * 0.04],
        size * 0.44,
        CHOYA2_SHADES,
    );
    let t = ui.frame_count() as f32 * 0.035;
    let bounce = t.sin() * size * 0.05;
    blit_frame(
        &dl,
        tid,
        [center[0] - size * 0.52, center[1] + size * 0.16 + bounce],
        size * 0.32,
        CHOYA2_MARACA,
    );
    blit_frame(
        &dl,
        tid,
        [center[0] + size * 0.54, center[1] + size * 0.20 - bounce],
        size * 0.28,
        CHOYA2_MARACA2,
    );
    let a = t * 0.45;
    blit_frame(
        &dl,
        tid,
        [
            center[0] + a.cos() * size * 0.62,
            center[1] - size * 0.18 + a.sin() * size * 0.22,
        ],
        size * 0.16,
        CHOYA2_NOTE,
    );
    blit_frame(
        &dl,
        tid,
        [
            center[0] - (a + 2.2).cos() * size * 0.55,
            center[1] + size * 0.38 + (a + 2.2).sin() * size * 0.10,
        ],
        size * 0.14,
        CHOYA2_HEART,
    );
}

/// Peeking-from-the-rock pose while the LLM is working.
pub fn draw_choya_thinking(ui: &Ui, center: [f32; 2], size: f32) {
    let Some(tid) = choya_sheet2() else {
        blit_choya_frame(&ui.get_window_draw_list(), center, size, CHOYA_THINK);
        return;
    };
    blit_frame(&ui.get_window_draw_list(), tid, center, size, CHOYA2_PEEK);
}

/// Six-frame bounce from sheet 2 (composer / small slots).
pub fn draw_choya_walk(ui: &Ui, center: [f32; 2], size: f32) {
    draw_choya_walk_paced(ui, center, size, 14);
}

pub fn draw_choya_walk_paced(ui: &Ui, center: [f32; 2], size: f32, frames_per_cell: usize) {
    let Some(tid) = choya_sheet2() else {
        draw_choya_avatar(ui, center, size);
        return;
    };
    let step = frames_per_cell.max(1);
    let i = (ui.frame_count() as usize / step) % CHOYA2_WALK.len();
    blit_frame(
        &ui.get_window_draw_list(),
        tid,
        center,
        size,
        CHOYA2_WALK[i],
    );
}

pub fn draw_choya_sleep(ui: &Ui, center: [f32; 2], size: f32) {
    blit_choya_frame(&ui.get_window_draw_list(), center, size, CHOYA_SLEEP);
}

pub fn draw_gem_icon(dl: &DrawListMut, top: [f32; 2], height: f32) {
    draw_gem_chest(dl, top, height);
}

const CHOYA_LINES: &[&str] = &[
    "poke!",
    "not salad.",
    "fiesta!",
    "olé!",
    "don't hug",
    "candy?",
    "spiky.",
    "ow ow ow",
    "shake it!",
    "I'm a plant",
    "needles.",
    "boom?",
    "hit me",
    "loot inside",
    "no touchy",
    "yeet?",
    "pop!",
    "not a pear",
    "hug = ouch",
    "I bite",
    "piñata!",
    "amigo!",
    "ay ay ay",
    "boop?",
    "no candy",
    "violence!",
    "revenge poke",
    "don't sit",
    "I'm loot",
    "wiggle",
    "too cute",
    "coins?",
    "spicy hug",
    "keep off",
    "Elona!",
];

struct ChoyaTalk {
    rng: u64,
    line: usize,
    showing: bool,
    until: Instant,
}

static CHOYA_TALK: Mutex<Option<ChoyaTalk>> = Mutex::new(None);

fn choya_gap_ms(rng: u64) -> u64 {
    1000 + rng % 4001
}

fn choya_rng_step(rng: u64) -> u64 {
    rng.wrapping_mul(6364136223846793005).wrapping_add(1)
}

fn choya_quip() -> Option<&'static str> {
    let now = Instant::now();
    let mut guard = CHOYA_TALK.lock().unwrap_or_else(|e| e.into_inner());
    let talk = guard.get_or_insert_with(|| {
        let rng = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xC0C0_A11CE);
        ChoyaTalk {
            rng,
            line: 0,
            showing: false,
            until: now + Duration::from_millis(choya_gap_ms(rng)),
        }
    });
    if now >= talk.until {
        talk.rng = choya_rng_step(talk.rng);
        if talk.showing {
            talk.showing = false;
            talk.until = now + Duration::from_millis(choya_gap_ms(talk.rng));
        } else {
            talk.showing = true;
            let n = CHOYA_LINES.len();
            let skip = 1 + (talk.rng as usize % (n - 1).max(1));
            talk.line = (talk.line + skip) % n;
            talk.until = now + Duration::from_millis(1600);
        }
    }
    if talk.showing {
        Some(CHOYA_LINES[talk.line])
    } else {
        None
    }
}

fn draw_gem_chest(dl: &DrawListMut, top: [f32; 2], height: f32) {
    let w = height * (308.0 / 256.0);
    let pmin = [top[0] - w * 0.5, top[1]];
    let pmax = [top[0] + w * 0.5, top[1] + height];
    if let Some(tid) = embedded_tex(
        "GW2BO_GEM_CHEST3",
        include_bytes!("../../assets/gem_chest.png"),
    ) {
        dl.add_image(tid, pmin, pmax).build();
    }
}

fn draw_mystic_coin(
    dl: &DrawListMut,
    c: [f32; 2],
    r: f32,
    metal: usize,
    fill: f32,
    fallback: [f32; 4],
) {
    if let Some(tid) = mystic_coin_tex(metal) {
        let a = 0.40 + 0.60 * fill;
        dl.add_image(tid, [c[0] - r, c[1] - r], [c[0] + r, c[1] + r])
            .col([1.0, 1.0, 1.0, a])
            .build();
    } else {
        draw_coin(dl, c, r, fallback, fill);
    }
}

#[cfg(test)]
mod tests {
    use super::choya_gap_ms;

    #[test]
    fn choya_gap_is_one_to_five_seconds() {
        for seed in [0u64, 1, 4000, 4001, u64::MAX] {
            let ms = choya_gap_ms(seed);
            assert!((1000..=5000).contains(&ms), "{ms} from {seed}");
        }
    }

    #[test]
    fn choya_sheet_uvs_stay_on_atlas() {
        let extra = [
            super::CHOYA2_HERO,
            super::CHOYA2_PEEK,
            super::CHOYA2_PARTY,
            super::CHOYA2_SOMBRERO,
            super::CHOYA2_SHADES,
            super::CHOYA2_MARACA,
            super::CHOYA2_MARACA2,
            super::CHOYA2_NOTE,
            super::CHOYA2_HEART,
            super::CHOYA2_LEI,
            super::CHOYA_SLEEP,
        ];
        for frame in [super::CHOYA_HERO, super::CHOYA_DANCE, super::CHOYA_THINK]
            .into_iter()
            .chain(super::CHOYA_IDLE)
            .chain(extra)
            .chain(super::CHOYA2_WALK)
            .chain(super::CHOYA2_FACE)
        {
            let (a, b) = super::sheet_uv(frame);
            assert!(a[0] >= 0.0 && a[1] >= 0.0, "{frame:?} {a:?}");
            assert!(b[0] <= 1.0 && b[1] <= 1.0, "{frame:?} {b:?}");
            assert!(b[0] > a[0] && b[1] > a[1], "{frame:?}");
        }
    }
}
