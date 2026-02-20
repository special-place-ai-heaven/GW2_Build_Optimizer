use nexus::imgui::{Condition, Ui, Window};

use crate::state;

pub fn render(ui: &Ui) {
    if !state::is_window_visible() {
        return;
    }

    Window::new("GW2 Build Optimizer")
        .size([600.0, 400.0], Condition::FirstUseEver)
        .build(ui, || {
            ui.text("Setup required");
            ui.separator();
            ui.text_wrapped("Press Ctrl+Shift+O to toggle this window.");
            ui.text_wrapped(
                "This addon will help you optimize your Guild Wars 2 builds.",
            );
        });
}
