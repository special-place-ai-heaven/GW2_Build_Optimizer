//! Draw-list glyphs for the About tab — category icons on message rows and
//! wizard tiles. The overlay fonts have no colour emoji, so each icon is a few
//! draw-list primitives tinted with the category colour; the Ko-fi cup is the
//! one embedded texture. An unknown icon name renders as a filled dot so a
//! taxonomy newer than the DLL still draws something sensible.

use nexus::imgui::{DrawListMut, Ui};

use crate::ui::{color_u32, theme};

/// Dark seam ink for detail lines drawn over a filled body (same ink as the
/// coin outline in `theme::draw_coin`).
const INK: [f32; 4] = [0.10, 0.08, 0.05, 0.90];

/// Which glyph a taxonomy `icon` name resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphKind {
    Bug,
    Broken,
    Bulb,
    Question,
    Fist,
    Kofi,
    Dot,
}

/// Map a taxonomy `icon` name to a glyph; anything unknown is a dot.
pub fn glyph_kind(name: &str) -> GlyphKind {
    match name {
        "bug" => GlyphKind::Bug,
        "broken" => GlyphKind::Broken,
        "bulb" => GlyphKind::Bulb,
        "question" => GlyphKind::Question,
        "fist" => GlyphKind::Fist,
        "kofi" => GlyphKind::Kofi,
        _ => GlyphKind::Dot,
    }
}

/// Map a taxonomy `color` name to RGBA; anything unknown is `theme::MUTED`.
pub fn category_color(name: &str) -> [f32; 4] {
    match name {
        "red" => [0.90, 0.32, 0.28, 1.0],
        "orange" => [0.95, 0.62, 0.22, 1.0],
        "green" => [0.55, 0.92, 0.62, 1.0],
        "blue" => [0.62, 0.82, 1.0, 1.0],
        "gold" => theme::GOLD,
        _ => theme::MUTED,
    }
}

/// Draw the glyph for `name` inside the `size`-sided box around `center`.
/// Every stroke stays within `center ± size / 2`; the Ko-fi texture keeps its
/// own aspect (wider than tall) and is the one deliberate exception.
pub fn draw_glyph(
    ui: &Ui,
    dl: &DrawListMut,
    name: &str,
    center: [f32; 2],
    size: f32,
    color: [f32; 4],
) {
    let thick = (size / 12.0).max(1.0);
    match glyph_kind(name) {
        GlyphKind::Bug => draw_bug(dl, center, size, color, thick),
        GlyphKind::Broken => draw_broken(dl, center, size, color, thick),
        GlyphKind::Bulb => draw_bulb(dl, center, size, color, thick),
        GlyphKind::Question => draw_question(ui, dl, center, color),
        GlyphKind::Fist => draw_fist(dl, center, size, color, thick),
        GlyphKind::Kofi => {
            if !draw_kofi(dl, center, size) {
                draw_dot(dl, center, size, color);
            }
        }
        GlyphKind::Dot => draw_dot(dl, center, size, color),
    }
}

/// Beetle: filled body + head, three legs a side, a wing seam down the middle.
fn draw_bug(dl: &DrawListMut, c: [f32; 2], size: f32, color: [f32; 4], thick: f32) {
    let body_r = size * 0.28;
    let head_r = size * 0.14;
    let reach = size * 0.48;
    for side in [-1.0f32, 1.0] {
        for angle in [-0.35f32, 0.0, 0.35] {
            let dir = [side * angle.cos(), angle.sin()];
            let a = [c[0] + dir[0] * body_r * 0.9, c[1] + dir[1] * body_r * 0.9];
            let b = [c[0] + dir[0] * reach, c[1] + dir[1] * reach];
            dl.add_line(a, b, color).thickness(thick).build();
        }
    }
    dl.add_circle(c, body_r, color).filled(true).build();
    dl.add_circle([c[0], c[1] - size * 0.36], head_r, color)
        .filled(true)
        .build();
    dl.add_line([c[0], c[1] - body_r], [c[0], c[1] + body_r], INK)
        .thickness(thick)
        .build();
}

/// Cracked shield: rounded outline with a three-segment zig-zag down the middle.
fn draw_broken(dl: &DrawListMut, c: [f32; 2], size: f32, color: [f32; 4], thick: f32) {
    let top = c[1] - size * 0.38;
    let bottom = c[1] + size * 0.30;
    dl.add_rect(
        [c[0] - size * 0.32, top],
        [c[0] + size * 0.32, bottom],
        color,
    )
    .rounding(size * 0.12)
    .thickness(thick)
    .build();
    let step = (bottom - top) / 3.0;
    let jag = size * 0.12;
    let pts = [
        [c[0], top],
        [c[0] - jag, top + step],
        [c[0] + jag, top + step * 2.0],
        [c[0], bottom],
    ];
    for w in pts.windows(2) {
        dl.add_line(w[0], w[1], color).thickness(thick).build();
    }
}

/// Light bulb: outlined globe with a filled filament, a base, two rays.
fn draw_bulb(dl: &DrawListMut, c: [f32; 2], size: f32, color: [f32; 4], thick: f32) {
    let globe = [c[0], c[1] - size * 0.08];
    dl.add_circle(globe, size * 0.28, color)
        .thickness(thick)
        .build();
    dl.add_circle(globe, size * 0.10, color)
        .filled(true)
        .build();
    dl.add_rect(
        [c[0] - size * 0.12, c[1] + size * 0.22],
        [c[0] + size * 0.12, c[1] + size * 0.36],
        color,
    )
    .filled(true)
    .build();
    let d = std::f32::consts::FRAC_1_SQRT_2;
    for sx in [-1.0f32, 1.0] {
        let a = [globe[0] + sx * d * size * 0.32, globe[1] - d * size * 0.32];
        let b = [globe[0] + sx * d * size * 0.46, globe[1] - d * size * 0.46];
        dl.add_line(a, b, color).thickness(thick).build();
    }
}

/// Question mark: text at the current font size, centred on `c`.
fn draw_question(ui: &Ui, dl: &DrawListMut, c: [f32; 2], color: [f32; 4]) {
    let ts = ui.calc_text_size("?");
    dl.add_text(
        [c[0] - ts[0] * 0.5, c[1] - ts[1] * 0.5],
        color_u32(color),
        "?",
    );
}

/// Fist bump: filled rounded block, knuckle seams across the top third, thumb
/// folded across the left.
fn draw_fist(dl: &DrawListMut, c: [f32; 2], size: f32, color: [f32; 4], thick: f32) {
    let left = c[0] - size * 0.34;
    let right = c[0] + size * 0.30;
    let top = c[1] - size * 0.22;
    let bottom = c[1] + size * 0.30;
    dl.add_rect([left, top], [right, bottom], color)
        .filled(true)
        .rounding(size * 0.12)
        .build();
    let knuckle_bottom = top + (bottom - top) / 3.0;
    for k in 1..4 {
        let x = left + (right - left) * k as f32 / 4.0;
        dl.add_line([x, top + thick], [x, knuckle_bottom], INK)
            .thickness(thick)
            .build();
    }
    let thumb_y = c[1] + size * 0.06;
    let thumb_x = c[0] - size * 0.08;
    dl.add_line([left, thumb_y], [thumb_x, thumb_y], INK)
        .thickness(thick)
        .build();
    dl.add_line([thumb_x, thumb_y], [thumb_x, bottom - thick], INK)
        .thickness(thick)
        .build();
}

/// Ko-fi cup texture, `size` tall at its native 161:130 aspect. Returns false
/// when the texture is unavailable so the caller can fall back to a dot.
fn draw_kofi(dl: &DrawListMut, c: [f32; 2], size: f32) -> bool {
    let Some(tid) = theme::kofi_tex() else {
        return false;
    };
    let h = size;
    let w = size * (161.0 / 130.0);
    dl.add_image(
        tid,
        [c[0] - w * 0.5, c[1] - h * 0.5],
        [c[0] + w * 0.5, c[1] + h * 0.5],
    )
    .build();
    true
}

/// Fallback: a filled dot in the category colour.
fn draw_dot(dl: &DrawListMut, c: [f32; 2], size: f32, color: [f32; 4]) {
    dl.add_circle(c, size * 0.35, color).filled(true).build();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_icon_falls_back_to_dot() {
        assert_eq!(glyph_kind("sparkle"), GlyphKind::Dot);
        assert_eq!(glyph_kind(""), GlyphKind::Dot);
    }

    #[test]
    fn known_icons_map() {
        assert_eq!(glyph_kind("bug"), GlyphKind::Bug);
        assert_eq!(glyph_kind("broken"), GlyphKind::Broken);
        assert_eq!(glyph_kind("bulb"), GlyphKind::Bulb);
        assert_eq!(glyph_kind("question"), GlyphKind::Question);
        assert_eq!(glyph_kind("fist"), GlyphKind::Fist);
        assert_eq!(glyph_kind("kofi"), GlyphKind::Kofi);
    }

    #[test]
    fn category_color_unknown_is_muted() {
        assert_eq!(category_color("pink"), theme::MUTED);
        assert_eq!(category_color(""), theme::MUTED);
    }

    #[test]
    fn category_colors_are_distinct() {
        let names = ["red", "orange", "green", "blue", "gold"];
        for (i, a) in names.iter().enumerate() {
            for b in &names[i + 1..] {
                assert_ne!(category_color(a), category_color(b), "{a} vs {b}");
            }
            assert_ne!(category_color(a), theme::MUTED, "{a} is muted");
        }
    }
}
