mod chat_links;
mod clipboard;
mod state;
pub mod ui;

use nexus::addon::UpdateProvider;
use nexus::gui::{register_render, RenderType};
use nexus::keybind::{keybind_handler, register_keybind_with_string};
use nexus::log::{log, LogLevel};
use nexus::paths::get_addon_dir;

/// Crate version from `Cargo.toml`. UI and logs must use this, never a literal.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    ui::fonts::init();

    register_keybind_with_string(
        "GW2_BUILD_OPT_TOGGLE",
        keybind_handler!(|_id, is_release| {
            if !is_release {
                state::toggle_window();
            }
        }),
        "CTRL+SHIFT+O",
    )
    .revert_on_unload();

    register_render(RenderType::Render, nexus::gui::render!(ui::render)).revert_on_unload();

    log(
        LogLevel::Info,
        "GW2 Build Optimizer",
        format!("v{} loaded. Press Ctrl+Shift+O to toggle.", crate::VERSION),
    );
}

fn on_unload() {
    state::persist_window();
    state::clear();
    log(LogLevel::Info, "GW2 Build Optimizer", "Addon unloaded.");
}
