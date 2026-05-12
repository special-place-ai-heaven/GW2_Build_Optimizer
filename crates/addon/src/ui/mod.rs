pub mod chat_bar;
pub mod comparison;
mod gear_diff;
pub mod main_view;
pub mod radar_chart;
mod setup;

use nexus::imgui::{Condition, Ui, Window};

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

pub fn render(ui: &Ui) {
    if !state::is_window_visible() {
        return;
    }

    // Catch panics inside the ImGui frame so a bug in any render path
    // doesn't unwind through Nexus' C-unwind FFI boundary. A panic mid-frame
    // also leaves the ImGui stack unbalanced (open `begin` without `end`),
    // which the next frame would inherit and corrupt — `catch_unwind` here
    // protects only this addon's draw calls, but that's enough to keep the
    // rest of the game's ImGui state intact.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Window::new("GW2 Build Optimizer")
            .size([800.0, 600.0], Condition::FirstUseEver)
            .build(ui, || {
                state::with_state(|s| match &s.screen {
                    Screen::Setup(step) => {
                        setup::render_setup(ui, s, step.clone());
                    }
                    Screen::Main => {
                        main_view::render_main(ui, s);
                    }
                });
            });
    }));
    if outcome.is_err() {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            "Render panicked — skipping this frame. See debugger or logs for details.",
        );
    }
}
