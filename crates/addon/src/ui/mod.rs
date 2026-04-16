pub mod chat_bar;
pub mod comparison;
mod gear_diff;
pub mod main_view;
pub mod radar_chart;
mod setup;

use nexus::imgui::{Condition, Ui, Window};

use crate::state::{self, Screen};

pub fn render(ui: &Ui) {
    if !state::is_window_visible() {
        return;
    }

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
}
