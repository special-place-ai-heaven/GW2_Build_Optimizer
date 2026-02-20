mod state;
mod ui;

use nexus::gui::{register_render, RenderType};
use nexus::keybind::{keybind_handler, register_keybind_with_string};
use nexus::log::{log, LogLevel};

nexus::export! {
    name: "GW2 Build Optimizer",
    signature: -0x47573242, // "GW2B" as hex, negated per Nexus convention
    load: on_load,
    unload: on_unload,
}

fn on_load() {
    state::init();

    let _keybind = register_keybind_with_string(
        "GW2_BUILD_OPT_TOGGLE",
        keybind_handler!(|_id, is_release| {
            if !is_release {
                state::toggle_window();
            }
        }),
        "CTRL+SHIFT+O",
    );

    let _render = register_render(
        RenderType::Render,
        nexus::gui::render!(ui::render),
    );

    log(LogLevel::Info, "GW2 Build Optimizer", "Addon loaded successfully.");
}

fn on_unload() {
    log(LogLevel::Info, "GW2 Build Optimizer", "Addon unloaded.");
}
