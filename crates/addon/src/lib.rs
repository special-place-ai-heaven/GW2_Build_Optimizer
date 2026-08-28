mod chat_links;
mod clipboard;
mod feedback;
mod news;
mod news_art;
mod state;
pub mod ui;

use nexus::addon::UpdateProvider;
use nexus::gui::{register_render, RenderType};
use nexus::keybind::{keybind_handler, register_keybind_with_string};
use nexus::log::{log, LogLevel};
use nexus::paths::get_addon_dir;
use nexus::quick_access::add_quick_access;
use nexus::texture::get_texture_or_create_from_memory;

/// Crate version from `Cargo.toml`. UI and logs must use this, never a literal.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

nexus::export! {
    name: "GW2 Build Optimizer",
    signature: -0x47573242,
    load: on_load,
    unload: on_unload,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/special-place-ai-heaven/GW2_Build_Optimizer",
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
                // Nexus dispatches keybinds from its own input hook across the
                // addon ABI, exactly like the render callback that `ui::render`
                // already guards. `toggle_window` writes config to disk, so it
                // has real failure modes; none of them may unwind into the game.
                if std::panic::catch_unwind(state::toggle_window).is_err() {
                    log(
                        LogLevel::Warning,
                        "GW2 Build Optimizer",
                        "Toggle keybind panicked; overlay state may be stale.",
                    );
                }
            }
        }),
        "CTRL+SHIFT+O",
    )
    .revert_on_unload();

    let _ = get_texture_or_create_from_memory(
        "GW2_BUILD_OPT_ICON_v1",
        include_bytes!("../assets/build_optimizer.png"),
    );
    let _ = get_texture_or_create_from_memory(
        "GW2_BUILD_OPT_ICON_HOVER_v1",
        include_bytes!("../assets/build_optimizer_hover.png"),
    );
    add_quick_access(
        "QA_GW2_BUILD_OPTIMIZER",
        "GW2_BUILD_OPT_ICON_v1",
        "GW2_BUILD_OPT_ICON_HOVER_v1",
        "GW2_BUILD_OPT_TOGGLE",
        "GW2 Build Optimizer",
    )
    .revert_on_unload();

    register_render(RenderType::Render, nexus::gui::render!(ui::render)).revert_on_unload();

    log(
        LogLevel::Info,
        "GW2 Build Optimizer",
        format!("v{} loaded. Press Ctrl+Shift+O to toggle.", crate::VERSION),
    );
}

/// Run one shutdown step with its own unwind guard.
///
/// Nexus calls `on_unload` across the addon ABI, so a panic escaping it takes the
/// game with it — and a step that panics must not skip the steps after it. Leaving
/// workers running with no cancel flag, or the state alive after unload, is a worse
/// outcome than the panic that caused it.
fn unload_step(step: &str, run: impl FnOnce() + std::panic::UnwindSafe) {
    if std::panic::catch_unwind(run).is_err() {
        log(
            LogLevel::Warning,
            "GW2 Build Optimizer",
            format!("Unload step '{}' panicked; continuing shutdown.", step),
        );
    }
}

fn on_unload() {
    // Cancel first: the workers get the window-rect disk write worth of head start
    // on their `is_cancelled()` checks before anything waits on them.
    unload_step("cancel workers", state::request_shutdown);
    unload_step("persist window", state::persist_window);

    // Wait for the workers with STATE released, then drop the state — it is that
    // drop which makes `with_state` start returning None to anything still alive,
    // so doing it before the wait would strand results mid-flight.
    let report = std::panic::catch_unwind(|| state::join_workers(state::UNLOAD_JOIN_BUDGET))
        .unwrap_or_default();
    unload_step("release state", state::clear);

    log(
        LogLevel::Info,
        "GW2 Build Optimizer",
        format!("Addon unloaded. {}", report),
    );
    if !report.abandoned.is_empty() {
        // Detached workers outlive the DLL. Say so: this is the line that explains
        // a crash right after the player disables the addon.
        log(
            LogLevel::Warning,
            "GW2 Build Optimizer",
            format!(
                "{} background worker(s) did not stop within {} ms and were left running.",
                report.abandoned.len(),
                state::UNLOAD_JOIN_BUDGET.as_millis()
            ),
        );
    }
}
