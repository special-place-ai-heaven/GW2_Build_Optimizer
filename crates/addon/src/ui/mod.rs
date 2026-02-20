mod setup;

use nexus::imgui::{Condition, Ui, Window};

use crate::state::{self, Screen};

pub fn render(ui: &Ui) {
    if !state::is_window_visible() {
        return;
    }

    Window::new("GW2 Build Optimizer")
        .size([700.0, 500.0], Condition::FirstUseEver)
        .build(ui, || {
            state::with_state(|s| {
                match &s.screen {
                    Screen::Setup(step) => {
                        setup::render_setup(ui, s, step.clone());
                    }
                    Screen::Main => {
                        render_main_placeholder(ui);
                    }
                }
            });
        });
}

fn render_main_placeholder(ui: &Ui) {
    ui.text("GW2 Build Optimizer");
    ui.separator();
    ui.text_wrapped("Setup complete! Main UI coming in S05+.");
    ui.text_wrapped("Side menu, character selection, build views, and chat bar are planned.");
}
