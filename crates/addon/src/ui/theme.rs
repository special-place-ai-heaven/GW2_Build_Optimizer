//! Tyrian night overlay tokens. Dark stone + warm gold — GW2, not a dashboard.
//! Hold the return value of [`push`] for the whole window frame or styles pop.

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

/// Push overlay colors/rounding. Keep the value alive for the window frame.
pub fn push<'ui>(ui: &'ui Ui<'_>) -> impl Sized + 'ui {
    (
        ui.push_style_var(StyleVar::WindowRounding(8.0)),
        ui.push_style_var(StyleVar::ChildRounding(6.0)),
        ui.push_style_var(StyleVar::FrameRounding(5.0)),
        ui.push_style_var(StyleVar::GrabRounding(4.0)),
        ui.push_style_var(StyleVar::PopupRounding(6.0)),
        ui.push_style_var(StyleVar::WindowBorderSize(1.0)),
        ui.push_style_var(StyleVar::WindowPadding([24.0, 16.0])),
        ui.push_style_color(StyleColor::WindowBg, INK),
        ui.push_style_color(StyleColor::ChildBg, CHILD),
        ui.push_style_color(StyleColor::PopupBg, [0.08, 0.07, 0.05, 0.98]),
        ui.push_style_color(StyleColor::Border, GOLD_DIM),
        ui.push_style_color(StyleColor::FrameBg, [0.12, 0.10, 0.07, 0.95]),
        ui.push_style_color(StyleColor::FrameBgHovered, [0.18, 0.15, 0.09, 1.0]),
        ui.push_style_color(StyleColor::FrameBgActive, [0.22, 0.18, 0.08, 1.0]),
        ui.push_style_color(StyleColor::Button, [0.32, 0.25, 0.08, 0.95]),
        ui.push_style_color(StyleColor::ButtonHovered, [0.46, 0.36, 0.10, 1.0]),
        ui.push_style_color(StyleColor::ButtonActive, [0.58, 0.45, 0.12, 1.0]),
        ui.push_style_color(StyleColor::Header, [0.22, 0.18, 0.08, 0.9]),
        ui.push_style_color(StyleColor::HeaderHovered, [0.30, 0.24, 0.10, 1.0]),
        ui.push_style_color(StyleColor::TitleBg, [0.10, 0.08, 0.05, 1.0]),
        ui.push_style_color(StyleColor::TitleBgActive, [0.16, 0.12, 0.06, 1.0]),
        ui.push_style_color(StyleColor::Separator, GOLD_DIM),
        ui.push_style_color(StyleColor::CheckMark, GOLD),
        ui.push_style_color(StyleColor::SliderGrab, GOLD_FILL),
        ui.push_style_color(StyleColor::Text, CREAM),
        ui.push_style_color(StyleColor::TextDisabled, MUTED),
        ui.push_style_color(StyleColor::ScrollbarGrab, GOLD_DIM),
    )
}

/// Gold plate button (Copy, Send). Dark text on gold so it reads as the action.
pub fn gold_button(ui: &Ui, label: impl AsRef<str>) -> bool {
    let _bg = ui.push_style_color(StyleColor::Button, GOLD_FILL);
    let _h = ui.push_style_color(StyleColor::ButtonHovered, [0.90, 0.74, 0.28, 1.0]);
    let _a = ui.push_style_color(StyleColor::ButtonActive, [0.70, 0.55, 0.16, 1.0]);
    let _t = ui.push_style_color(StyleColor::Text, [0.10, 0.08, 0.04, 1.0]);
    ui.button(label.as_ref())
}

pub fn gold_button_sized(ui: &Ui, label: impl AsRef<str>, size: [f32; 2]) -> bool {
    let label = label.as_ref();
    let visible = label.split("##").next().unwrap_or(label);
    let need_w = ui.calc_text_size(visible)[0] + 18.0;
    let w = if size[0] < 0.0 {
        size[0]
    } else {
        size[0].max(need_w)
    };
    let _bg = ui.push_style_color(StyleColor::Button, GOLD_FILL);
    let _h = ui.push_style_color(StyleColor::ButtonHovered, [0.90, 0.74, 0.28, 1.0]);
    let _a = ui.push_style_color(StyleColor::ButtonActive, [0.70, 0.55, 0.16, 1.0]);
    let _t = ui.push_style_color(StyleColor::Text, [0.10, 0.08, 0.04, 1.0]);
    ui.button_with_size(label, [w, size[1]])
}

/// Rounded pill tab. `id` must be unique (used as an invisible button id).
pub fn pill(ui: &Ui, label: &str, selected: bool, id: &str) -> bool {
    let pad_x = 10.0;
    let pad_y = 3.0;
    let sz = ui.calc_text_size(label);
    let w = (sz[0] + pad_x * 2.0).max(36.0);
    let h = sz[1] + pad_y * 2.0;
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
        dl.add_rect(
            [start[0] - 1.0, start[1]],
            [start[0] + 3.0, start[1] + 22.0],
            GOLD,
        )
        .filled(true)
        .rounding(2.0)
        .build();
        dl.add_text([start[0] + 10.0, start[1] + 3.0], color_u32(GOLD), title);
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
    ui.set_window_font_scale(CAP_SCALE);
    let cap_sz = ui.calc_text_size(caption);
    ui.set_window_font_scale(1.0);
    let total_h = title_h + headroom + bar_h + line_gap + 6.0 + coin_h + CAP_GAP + cap_sz[1] + 8.0;
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
            t,
            track_x1,
        );

        ui.set_window_font_scale(CAP_SCALE);
        let cap_sz = ui.calc_text_size(caption);
        let cap_x = origin[0] + ((full - cap_sz[0]) * 0.5).max(0.0);
        let cap_y = label_y + CAP_GAP;
        dl.add_text([cap_x, cap_y], color_u32(GOLD), caption);
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

/// Piñata choya standing on `feet`, slow bob + rock, chatting.
fn draw_choya(
    ui: &Ui,
    dl: &DrawListMut,
    feet: [f32; 2],
    height: f32,
    sway: f32,
    t: f32,
    bar_right: f32,
) {
    let Some(tid) = embedded_tex("GW2BO_CHOYA", include_bytes!("../../assets/choya.png")) else {
        return;
    };
    // cropped asset is 172x192
    let h = height;
    let w = h * (172.0 / 192.0);
    let tilt = sway * 0.18;
    let (c, s) = (tilt.cos(), tilt.sin());
    let rot =
        |dx: f32, dy: f32| -> [f32; 2] { [feet[0] + dx * c - dy * s, feet[1] + dx * s + dy * c] };
    dl.add_image_quad(
        tid,
        rot(-w * 0.5, -h),
        rot(w * 0.5, -h),
        rot(w * 0.5, 0.0),
        rot(-w * 0.5, 0.0),
    )
    .build();

    const LINES: &[&str] = &[
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
    ];
    let slot = 160;
    let i = ((t as i32 / slot).rem_euclid(LINES.len() as i32)) as usize;
    if (t as i32).rem_euclid(slot) >= 120 {
        return;
    }
    let text = LINES[i];
    let pad = 6.0;
    let sz = ui.calc_text_size(text);
    let bw = sz[0] + pad * 2.0;
    let bh = sz[1] + pad;
    let gap = 12.0;
    let mut bx = feet[0] + w * 0.5 + gap;
    if bx + bw > bar_right {
        bx = feet[0] - w * 0.5 - gap - bw;
    }
    let by = feet[1] - h * 0.78;
    dl.add_rect([bx, by], [bx + bw, by + bh], PLATE)
        .filled(true)
        .rounding(6.0)
        .build();
    dl.add_rect([bx, by], [bx + bw, by + bh], GOLD_DIM)
        .rounding(6.0)
        .build();
    dl.add_text([bx + pad, by + pad * 0.4], color_u32(CREAM), text);
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
