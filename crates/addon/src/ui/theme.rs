//! Tyrian night overlay tokens. Dark stone + warm gold — GW2, not a dashboard.
//! Hold the return value of [`push`] for the whole window frame or styles pop.

use nexus::imgui::{StyleColor, StyleVar, Ui};

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
