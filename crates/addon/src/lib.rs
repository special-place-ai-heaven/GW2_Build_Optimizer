mod state;
pub mod ui;

use nexus::addon::UpdateProvider;
use nexus::gui::{register_render, RenderType};
use nexus::keybind::{keybind_handler, register_keybind_with_string};
use nexus::log::{log, LogLevel};
use nexus::paths::get_addon_dir;

nexus::export! {
    name: "GW2 Build Optimizer",
    signature: -0x47573242,
    load: on_load,
    unload: on_unload,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/special-place-administrator/GW2_Build_Optimizer",
}

fn on_load() {
    let Some(addon_dir) = get_addon_dir("gw2_build_optimizer") else {
        log(
            LogLevel::Warning,
            "GW2 Build Optimizer",
            "Failed to locate the Nexus addon directory. Aborting addon initialization.",
        );
        return;
    };

    state::init(addon_dir);

    let _keybind = register_keybind_with_string(
        "GW2_BUILD_OPT_TOGGLE",
        keybind_handler!(|_id, is_release| {
            if !is_release {
                state::toggle_window();
            }
        }),
        "CTRL+SHIFT+O",
    );

    let _render = register_render(RenderType::Render, nexus::gui::render!(ui::render));

    log(
        LogLevel::Info,
        "GW2 Build Optimizer",
        "v1.0.0 loaded. Press Ctrl+Shift+O to open.",
    );
}

fn on_unload() {
    state::clear();
    log(LogLevel::Info, "GW2 Build Optimizer", "Addon unloaded.");
}
