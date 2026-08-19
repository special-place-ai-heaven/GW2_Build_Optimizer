pub(crate) mod fonts;

pub mod chat_bar;
pub mod comparison;
mod gear_diff;
mod gear_sheet;
pub(crate) mod icons;
pub mod main_view;
pub mod radar_chart;
mod setup;
pub(crate) mod theme;

use nexus::imgui::{
    Condition, MouseButton, MouseCursor, Ui, Window, WindowFlags, WindowHoveredFlags,
};

/// Convert RGBA `[f32;4]` (each channel 0.0–1.0) to ImGui's packed `u32` color
/// (ABGR byte order). Shared by `radar_chart` and `lock_panel` to avoid
/// duplicate definitions drifting out of sync.
pub(crate) fn color_u32(c: [f32; 4]) -> u32 {
    let r = (c[0] * 255.0).clamp(0.0, 255.0) as u32;
    let g = (c[1] * 255.0).clamp(0.0, 255.0) as u32;
    let b = (c[2] * 255.0).clamp(0.0, 255.0) as u32;
    let a = (c[3] * 255.0).clamp(0.0, 255.0) as u32;
    (a << 24) | (b << 16) | (g << 8) | r
}

use crate::state::{self, Screen};

/// True when the overlay has little usable area on the game framebuffer
/// (imgui.ini parked it past the right edge, or it was stretched wider than
/// the display).
pub(crate) fn window_needs_snap(pos: [f32; 2], size: [f32; 2], display: [f32; 2]) -> bool {
    let x0 = pos[0].max(0.0);
    let y0 = pos[1].max(0.0);
    let x1 = (pos[0] + size[0]).min(display[0]);
    let y1 = (pos[1] + size[1]).min(display[1]);
    let vis_w = (x1 - x0).max(0.0);
    let vis_h = (y1 - y0).max(0.0);
    if vis_w < 120.0 || vis_h < 80.0 {
        return true;
    }
    let vis_area = vis_w * vis_h;
    let total = size[0].max(1.0) * size[1].max(1.0);
    vis_area < total * 0.75
}

pub fn render(ui: &Ui) {
    if !state::is_window_visible() {
        return;
    }

    let (snap, pos, size, opacity, ui_font, ui_lang) = state::with_state(|s| {
        let snap = s.force_window_pos;
        s.force_window_pos = false;
        if snap {
            s.config.set_window_rect(
                gw2_core::config::DEFAULT_WINDOW_POS,
                gw2_core::config::DEFAULT_WINDOW_SIZE,
            );
            let _ = s.config.save(&s.config_path);
        }
        let (pos, size) = s.config.window_rect();
        (
            snap,
            pos,
            size,
            s.config.window_opacity,
            s.config.ui_font.clone(),
            s.config.ui_language.clone(),
        )
    })
    .unwrap_or((
        false,
        gw2_core::config::DEFAULT_WINDOW_POS,
        gw2_core::config::DEFAULT_WINDOW_SIZE,
        1.0,
        String::from("auto"),
        String::from("auto"),
    ));

    // Catch panics inside the ImGui frame so a bug in any render path
    // doesn't unwind through Nexus' C-unwind FFI boundary. A panic mid-frame
    // also leaves the ImGui stack unbalanced (open `begin` without `end`),
    // which the next frame would inherit and corrupt — `catch_unwind` here
    // protects only this addon's draw calls, but that's enough to keep the
    // rest of the game's ImGui state intact.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _theme = theme::push(ui, opacity);
        fonts::init();
        let _font = fonts::push(&ui_font, &ui_lang);
        let mut opened = true;
        let cond = if snap {
            Condition::Always
        } else {
            Condition::Appearing
        };
        Window::new("GW2 Build Optimizer")
            .opened(&mut opened)
            .flags(WindowFlags::NO_SAVED_SETTINGS)
            .size_constraints(gw2_core::config::MIN_WINDOW_SIZE, [99999.0, 99999.0])
            .collapsed(false, cond)
            .position(pos, cond)
            .size(size, cond)
            .build(ui, || {
                state::with_state(|s| {
                    if window_needs_snap(ui.window_pos(), ui.window_size(), ui.io().display_size) {
                        s.force_window_pos = true;
                    }
                    if !ui.is_window_collapsed() && !ui.is_mouse_down(MouseButton::Left) {
                        let p = ui.window_pos();
                        let sz = ui.window_size();
                        let (old_p, old_sz) = s.config.window_rect();
                        if (p[0] - old_p[0]).abs() > 0.5
                            || (p[1] - old_p[1]).abs() > 0.5
                            || (sz[0] - old_sz[0]).abs() > 0.5
                            || (sz[1] - old_sz[1]).abs() > 0.5
                        {
                            s.config.set_window_rect(p, sz);
                            let _ = s.config.save(&s.config_path);
                        }
                    }
                    gw2_core::i18n::set_language(&s.config.ui_language);
                    match &s.screen {
                        Screen::Setup(step) => {
                            setup::render_setup(ui, s, step.clone());
                        }
                        Screen::Main => {
                            main_view::render_main(ui, s);
                        }
                    }
                    // Last write wins: buttons/selectables/pills set Hand while hovered.
                    // Nexus maps that to GW2's gloved click cursor. Pin arrow after all widgets,
                    // but only while the mouse is over this overlay so the world cursor stays intact.
                    if ui.is_window_hovered_with_flags(WindowHoveredFlags::ROOT_AND_CHILD_WINDOWS)
                        || ui.is_any_item_hovered()
                    {
                        ui.set_mouse_cursor(Some(MouseCursor::Arrow));
                    }
                });
            });
        if !opened {
            state::with_state(|s| {
                s.window_visible = false;
                s.config.window_visible = false;
                let _ = s.config.save(&s.config_path);
            });
        }
    }));
    if outcome.is_err() {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            "Render panicked — skipping this frame. See debugger or logs for details.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::window_needs_snap;

    #[test]
    fn parked_at_right_edge_of_1080p_needs_snap() {
        assert!(window_needs_snap(
            [1880.0, 293.0],
            [800.0, 600.0],
            [1920.0, 1080.0]
        ));
    }

    #[test]
    fn stretched_wider_than_remaining_1080p_needs_snap() {
        assert!(window_needs_snap(
            [1159.0, 181.0],
            [1840.0, 867.0],
            [1920.0, 1080.0]
        ));
    }

    #[test]
    fn default_corner_on_1080p_stays() {
        assert!(!window_needs_snap(
            [80.0, 80.0],
            [800.0, 600.0],
            [1920.0, 1080.0]
        ));
    }

    #[test]
    fn ultrawide_keeps_rightish_window() {
        assert!(!window_needs_snap(
            [1880.0, 293.0],
            [800.0, 600.0],
            [3440.0, 1440.0]
        ));
    }
}
