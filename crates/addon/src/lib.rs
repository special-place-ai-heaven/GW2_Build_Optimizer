mod cache_bounds;
mod chat_links;
mod clipboard;
mod feedback;
mod news;
mod news_art;
mod radio;
mod state;
pub mod ui;

use nexus::addon::UpdateProvider;
use nexus::gui::{register_render, RenderType};
use nexus::imgui::Ui;
use nexus::keybind::{keybind_handler, register_keybind_with_string};
use nexus::log::{log, LogLevel};
use nexus::paths::get_addon_dir;
use nexus::quick_access::{add_quick_access, remove_quick_access};
use nexus::texture::get_texture_or_create_from_memory;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Crate version from `Cargo.toml`. UI and logs must use this, never a literal.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Hand-written instead of `nexus::export!`: the macro's load wrapper calls
// nexus-rs `globals::init`, which `expect`s that its `OnceLock`s are empty.
// A detached worker keeps this image pinned past unload (`pin_addon_module`),
// so an Enable in that window reloads the SAME image and `init` panics
// ("addon api initialized multiple times", 1.14.52). The globals it would set
// are still set, so a reload into this image skips it. Nexus hands out one
// API table per version that outlives the addon (2026.2.17 caches it; main
// leaks a fresh one per load and never frees the old), so the kept pointer
// stays valid.
const ADDON_NAME: &str = "GW2 Build Optimizer";

/// Set by the first load of this image, never cleared: it lives exactly as
/// long as the nexus-rs globals it stands for.
static NEXUS_GLOBALS_SET: AtomicBool = AtomicBool::new(false);

/// True only for the first load of this mapped image.
fn first_load_of_image() -> bool {
    !NEXUS_GLOBALS_SET.swap(true, Ordering::AcqRel)
}

const fn version_part(s: &str) -> i16 {
    let b = s.as_bytes();
    let mut n: i16 = 0;
    let mut i = 0;
    while i < b.len() {
        assert!(b[i].is_ascii_digit(), "crate version not number");
        n = n * 10 + (b[i] - b'0') as i16;
        i += 1;
    }
    n
}

// The macro encodes pre-release tags into `revision`; this export does not.
const _: () = assert!(
    env!("CARGO_PKG_VERSION_PRE").is_empty(),
    "pre-release versions need the nexus-rs revision encoding"
);

static ADDON_DEF: nexus::addon::AddonDefinition = nexus::addon::AddonDefinition {
    signature: -0x47573242,
    api_version: nexus::AddonApi::VERSION,
    name: c"GW2 Build Optimizer".as_ptr(),
    version: nexus::addon::AddonVersion {
        major: version_part(env!("CARGO_PKG_VERSION_MAJOR")),
        minor: version_part(env!("CARGO_PKG_VERSION_MINOR")),
        build: version_part(env!("CARGO_PKG_VERSION_PATCH")),
        revision: -1, // stable: Nexus hides it
    },
    author: concat!(env!("CARGO_PKG_AUTHORS"), "\0").as_ptr().cast(),
    description: concat!(env!("CARGO_PKG_DESCRIPTION"), "\0").as_ptr().cast(),
    load: load_wrapper,
    unload: Some(unload_wrapper),
    flags: nexus::AddonFlags::None,
    provider: UpdateProvider::GitHub,
    update_link: c"https://github.com/special-place-ai-heaven/GW2_Build_Optimizer".as_ptr(),
};

#[no_mangle]
unsafe extern "system-unwind" fn GetAddonDef() -> *const nexus::addon::AddonDefinition {
    &ADDON_DEF
}

/// Log from a wrapper without ever unwinding: if nexus-rs `init` panicked
/// before storing the API table, `log` itself panics, and that is swallowed.
fn wrapper_log(message: &str) {
    let _ = std::panic::catch_unwind(|| log(LogLevel::Critical, ADDON_NAME, message));
}

unsafe extern "C-unwind" fn load_wrapper(api: *const nexus::AddonApi) {
    if first_load_of_image() {
        let api = std::panic::AssertUnwindSafe(api);
        // SAFETY: Nexus passes a valid API table that outlives the addon.
        let init = std::panic::catch_unwind(move || unsafe {
            nexus::__macro::init(*api, ADDON_NAME, None)
        });
        if init.is_err() {
            // Without the globals every Nexus call in `on_load` would panic.
            wrapper_log("nexus-rs init panicked; addon stays inert until the game restarts.");
            return;
        }
    }
    on_load();
}

unsafe extern "C-unwind" fn unload_wrapper() {
    on_unload();
    // SAFETY: called once per load, from Nexus unload, as the macro does.
    if std::panic::catch_unwind(|| unsafe { nexus::__macro::deinit() }).is_err() {
        wrapper_log("nexus-rs deinit panicked; some unload actions may not have run.");
    }
}

/// Run addon load with an unwind guard.
///
/// Nexus calls `on_load` across the addon ABI, so a panic escaping it takes the
/// game with it. Unlike [`unload_step`], remaining init is skipped: a half-registered
/// addon is worse than a logged abort.
fn load_guard(run: impl FnOnce() + std::panic::UnwindSafe) {
    if std::panic::catch_unwind(run).is_err() {
        log(
            LogLevel::Warning,
            "GW2 Build Optimizer",
            "Load panicked; aborting addon initialization.",
        );
    }
}

/// Heap crash (0xC0000374) hits ~1s after load, while ArcDPS is still hooking
/// D3D11. First PostRender can fire before that, so wait this out first.
pub(crate) const CHROME_SETTLE: Duration = Duration::from_millis(2000);

/// `attach_overlay_host` is one-shot. `BOOTSTRAP_FAILED` latches a panic inside
/// attach so the PostRender bootstrapper stops retrying for the session;
/// recovery requires an addon reload.
static HOST_ATTACHED: AtomicBool = AtomicBool::new(false);
static BOOTSTRAP_FAILED: AtomicBool = AtomicBool::new(false);
static CHROME_AT: OnceLock<Instant> = OnceLock::new();
const QUICK_ACCESS_ID: &str = "QA_GW2_BUILD_OPTIMIZER";

/// D3D-touching chrome that must wait for ArcDPS to finish hooking: texture
/// uploads and the Nexus quick-access entry. The ImGui `Render` hook for
/// [`ui::render`] is registered in `on_load` because Nexus's render registry
/// is locked for the whole frame and a `Register` call from inside a
/// PostRender callback would mutate the very vector being iterated.
fn attach_overlay_host() {
    if HOST_ATTACHED.load(Ordering::Acquire) {
        return;
    }
    if BOOTSTRAP_FAILED.load(Ordering::Acquire) {
        return;
    }
    // Claim the slot *before* doing D3D work. If the swap loses, another caller
    // already attached. If we panic after this point we still flag the failure
    // so the bootstrapper can retry next frame.
    HOST_ATTACHED.store(true, Ordering::Release);
    let result = std::panic::catch_unwind(|| {
        let _ = get_texture_or_create_from_memory(
            "GW2_BUILD_OPT_ICON_v1",
            include_bytes!("../assets/build_optimizer.png"),
        );
        let _ = get_texture_or_create_from_memory(
            "GW2_BUILD_OPT_ICON_HOVER_v1",
            include_bytes!("../assets/build_optimizer_hover.png"),
        );
        add_quick_access(
            QUICK_ACCESS_ID,
            "GW2_BUILD_OPT_ICON_v1",
            "GW2_BUILD_OPT_ICON_HOVER_v1",
            "GW2_BUILD_OPT_TOGGLE",
            "GW2 Build Optimizer",
        )
        // Removed by the action `on_load` registers, never from here: this
        // runs on the render thread, and nexus-rs `deinit` holds its
        // unload-action lock while it waits for this very frame to end.
        .leak();
    });
    if result.is_err() {
        HOST_ATTACHED.store(false, Ordering::Release);
        BOOTSTRAP_FAILED.store(true, Ordering::Release);
        log(
            LogLevel::Warning,
            "GW2 Build Optimizer",
            "Overlay chrome attach panicked; will not retry this session.",
        );
    } else {
        log(
            LogLevel::Info,
            "GW2 Build Optimizer",
            "Overlay chrome attached.",
        );
    }
}

/// PostRender callback Nexus invokes once per frame after the ImGui frame ends.
/// Nexus only fires this once it has a stable frame, which is the earliest
/// point we trust ArcDPS has finished hooking D3D11. We still wait
/// [`CHROME_SETTLE`] past `state::init` so a slow machine where Nexus starts
/// framing before ArcDPS settles still has the buffer observed in production.
fn bootstrap_chrome(_ui: &Ui) {
    let Some(at) = CHROME_AT.get() else {
        return;
    };
    if Instant::now() < *at {
        return;
    }
    if HOST_ATTACHED.load(Ordering::Acquire) {
        return;
    }
    attach_overlay_host();
}

fn on_load() {
    load_guard(|| {
        let Some(addon_dir) = get_addon_dir(gw2_core::ADDON_DIR_NAME) else {
            log(
                LogLevel::Warning,
                "GW2 Build Optimizer",
                "Failed to locate the Nexus addon directory. Aborting addon initialization.",
            );
            return;
        };

        let models_dev_dir = addon_dir.clone();
        // A pinned image keeps statics across unload/reload; undo the
        // previous unload's playback latch.
        radio::player::arm();
        // Same for the chrome slot the previous unload closed.
        HOST_ATTACHED.store(false, Ordering::SeqCst);
        BOOTSTRAP_FAILED.store(false, Ordering::SeqCst);
        state::init(addon_dir);
        let _ = CHROME_AT.set(Instant::now() + CHROME_SETTLE);

        // models.dev is updated most days; refresh our copy of it at launch
        // in the background so the model picker and the handshake read a
        // current catalog. A failed fetch keeps yesterday's file.
        state::with_state(|s| {
            s.spawn_worker("models-dev-refresh", move |token| {
                if !token.is_cancelled() {
                    gw2_optimizer::llm::models_dev::load(&models_dev_dir);
                }
            });
        });

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

        register_keybind_with_string(
            "GW2_BUILD_OPT_RADIO_TOGGLE",
            keybind_handler!(|_id, is_release| {
                if !is_release {
                    // Same ABI-boundary guard as the window toggle above: the
                    // radio toggle touches the playback runtime and config.
                    if std::panic::catch_unwind(radio::player::toggle).is_err() {
                        log(
                            LogLevel::Warning,
                            "GW2 Build Optimizer",
                            "Radio keybind panicked; playback state may be stale.",
                        );
                    }
                }
            }),
            "", // unbound by default; the user assigns one in Nexus
        )
        .revert_on_unload();

        register_keybind_with_string(
            "GW2_BUILD_OPT_MINI_RADIO_TOGGLE",
            keybind_handler!(|_id, is_release| {
                if !is_release {
                    // Same ABI-boundary guard as the toggles above: it writes
                    // config to disk.
                    if std::panic::catch_unwind(state::toggle_mini_radio).is_err() {
                        log(
                            LogLevel::Warning,
                            "GW2 Build Optimizer",
                            "Mini radio keybind panicked; its toggle may be stale.",
                        );
                    }
                }
            }),
            "", // unbound by default; the user assigns one in Nexus
        )
        .revert_on_unload();

        register_keybind_with_string(
            "GW2_BUILD_OPT_MINI_RADIO_ANCHOR",
            keybind_handler!(|_id, is_release| {
                if !is_release {
                    // Same ABI-boundary guard as the toggles above.
                    if std::panic::catch_unwind(state::toggle_mini_radio_anchor).is_err() {
                        log(
                            LogLevel::Warning,
                            "GW2 Build Optimizer",
                            "Mini radio anchor keybind panicked; its anchor may be stale.",
                        );
                    }
                }
            }),
            "", // unbound by default; the user assigns one in Nexus
        )
        .revert_on_unload();

        // The `Render` hook for `ui::render` is registered on load. Nexus locks
        // its render registry for the entire frame and iterates it on every
        // PreRender/Render/PostRender pass; a `Register` call from inside a
        // PostRender callback (deferring chrome attach) would push into the
        // very vector being iterated. Texture uploads and the quick-access
        // entry still defer to PostRender via `bootstrap_chrome`.
        register_render(RenderType::Render, nexus::gui::render!(ui::render)).revert_on_unload();

        // Function pointer only — no D3D. Texture uploads and the quick-access
        // entry attach from the first PostRender after [`CHROME_SETTLE`], when
        // ArcDPS has had time to hook D3D11.
        register_render(
            RenderType::PostRender,
            nexus::gui::render!(bootstrap_chrome),
        )
        .revert_on_unload();

        // After the render hooks: `deinit` runs actions in push order, and the
        // two deregisters above wait out the frame in flight, so by the time
        // these run no callback can attach chrome or add a font. Nexus removes
        // a shortcut by id, so one that never attached has nothing to remove.
        nexus::on_unload(|| remove_quick_access(QUICK_ACCESS_ID));
        nexus::on_unload(ui::fonts::release_all);
        log(
            LogLevel::Info,
            "GW2 Build Optimizer",
            format!("v{} loaded. Press Ctrl+Shift+O to toggle.", crate::VERSION),
        );
    });
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
    // Close the chrome attach slot for the rest of this load: the PostRender
    // bootstrap stays registered until nexus-rs `deinit`, which holds its
    // unload-action lock while it deregisters the render hooks. An attach in
    // that window calls `revert_on_unload` from inside the frame the
    // deregister waits on, and the game freezes. `on_load` reopens it.
    HOST_ATTACHED.store(true, Ordering::SeqCst);
    // Cancel first: the workers get the window-rect disk write worth of head start
    // on their `is_cancelled()` checks before anything waits on them.
    // Audio first: the playback stack owns OS threads (cpal, tokio) that must
    // be joined before FreeLibrary; radio teardown is bounded and independent
    // of the worker registry.
    unload_step("stop radio", radio::player::shutdown);
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
        // Detached workers keep the addon image pinned (`pin_addon_module`) so
        // Nexus `FreeLibrary` cannot unmap `.text` under them. The leftover is a
        // long-lived mapping, not the old return-into-unmapped-code crash.
        log(
            LogLevel::Warning,
            "GW2 Build Optimizer",
            format!(
                "{} background worker(s) did not stop within {} ms and were detached; the addon image stays pinned until they return.",
                report.abandoned.len(),
                state::UNLOAD_JOIN_BUDGET.as_millis()
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    /// Pin: `state::init` (and the rest of load) must sit behind `catch_unwind`
    /// so a panic cannot cross Nexus `C-unwind`. Freeze SHA had `catch_unwind`
    /// only on the keybind nested inside `on_load`, after `state::init`.
    ///
    /// This is a source pin, not an in-process call of `on_load`: that entry
    /// talks to the Nexus API table (`get_addon_dir`, `log`, register_*), which
    /// unit tests do not have. A panic inside `load_guard`'s log path would also
    /// need that table. Leave a live load-panic on the in-game list.
    #[test]
    fn on_load_wraps_init_in_catch_unwind() {
        let src = include_str!("lib.rs");
        let start = src.find("\nfn on_load()").expect("on_load must exist");
        let rest = &src[start..];
        let end = rest[1..].find("\nfn ").map(|i| i + 1).unwrap_or(rest.len());
        let body = &rest[..end];
        let init = body
            .find("state::init")
            .expect("on_load must call state::init");
        let catch = body.find("catch_unwind");
        let guard = body.find("load_guard");
        let wrapped =
            catch.map(|c| c < init).unwrap_or(false) || guard.map(|g| g < init).unwrap_or(false);
        assert!(
            wrapped,
            "state::init must run inside catch_unwind so a load panic cannot unwind into the game"
        );
        assert!(
            !body.contains("fonts::init"),
            "fonts must not register on load"
        );
        assert!(
            !body.contains("attach_overlay_host"),
            "on_load must not attach D3D chrome, even when the overlay starts visible"
        );
        assert!(
            !body.contains("get_texture") && !body.contains("add_quick_access"),
            "texture upload and quick access are D3D; they belong in attach_overlay_host"
        );
        assert!(
            body.contains("PostRender"),
            "on_load registers a PostRender bootstrap so chrome can attach after ArcDPS"
        );
        assert!(
            body.contains("RenderType::Render"),
            "on_load must register the Render hook for ui::render; registering it from PostRender mutates the very vector Nexus is iterating (heap crash)"
        );
    }

    /// Pin: unload closes the chrome attach slot and never reopens it.
    ///
    /// Nexus runs unload off the render thread, and the PostRender bootstrap
    /// stays registered until nexus-rs `deinit` removes it. `deinit` holds its
    /// unload-action lock while it deregisters the render hooks; an attach in
    /// that window calls `revert_on_unload` for the same lock from inside the
    /// frame the deregister waits on, and the game freezes (1.14.51, second
    /// unload). The first unload that day logged "Overlay chrome attached."
    /// after "Addon unloaded.": the reset flag had re-armed the attach.
    /// Source pin: `on_unload` needs the Nexus API table, which tests lack.
    #[test]
    fn on_unload_closes_the_chrome_slot_and_on_load_reopens_it() {
        let src = include_str!("lib.rs");
        let body_of = |name: &str| {
            let start = src.find(name).expect("function must exist");
            let rest = &src[start..];
            &rest[..rest.find("\n}\n").expect("top-level fn ends")]
        };
        let unload = body_of("\nfn on_unload()");
        let close = unload
            .find("HOST_ATTACHED.store(true")
            .expect("on_unload must close the attach slot");
        let first_step = unload.find("unload_step(").expect("unload steps");
        assert!(close < first_step, "close the slot before any unload step");
        assert!(
            !unload.contains("HOST_ATTACHED.store(false"),
            "on_unload must not re-arm the attach while PostRender is still registered"
        );
        let load = body_of("\nfn on_load()");
        let reopen = load
            .find("HOST_ATTACHED.store(false")
            .expect("on_load must reopen the slot for a pinned image");
        let post_render = load.find("PostRender").unwrap();
        assert!(
            reopen < post_render,
            "reopen before the bootstrap is registered"
        );

        // Render-thread registrations must not push unload actions: `deinit`
        // holds that lock while it waits for the frame. Their removals are
        // pushed at load, after the render hooks, so they run once no frame
        // can register another.
        let attach = body_of("\nfn attach_overlay_host()");
        assert!(
            !attach.contains("revert_on_unload"),
            "attach runs on the render thread; leak and remove from on_load"
        );
        let fonts = include_str!("ui/fonts.rs");
        assert!(
            !fonts.contains(".revert_on_unload()"),
            "font adds run on the render thread; leak and release from on_load"
        );
        for removal in ["remove_quick_access(", "fonts::release_all"] {
            let at = load.find(removal).expect("on_load registers the removal");
            assert!(
                at > post_render,
                "{removal} must run after the render hooks are deregistered"
            );
        }
    }

    /// The hand-written export must describe the addon exactly as
    /// `nexus::export!` did, or Nexus sees a different addon (signature) or
    /// a wrong version to update against.
    #[test]
    fn hand_written_export_matches_the_macro_contract() {
        use std::ffi::CStr;
        let def = &super::ADDON_DEF;
        let text = |p: *const std::ffi::c_char| unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(def.signature, -0x47573242);
        assert_eq!(def.api_version, nexus::AddonApi::VERSION);
        assert_eq!(text(def.name), super::ADDON_NAME);
        assert_eq!(text(def.author), env!("CARGO_PKG_AUTHORS"));
        assert_eq!(text(def.description), env!("CARGO_PKG_DESCRIPTION"));
        assert_eq!(
            text(def.update_link),
            "https://github.com/special-place-ai-heaven/GW2_Build_Optimizer"
        );
        assert_eq!(def.provider, nexus::addon::UpdateProvider::GitHub);
        assert_eq!(def.flags, nexus::AddonFlags::None);
        assert!(def.unload.is_some());
        let v = &def.version;
        assert_eq!(
            format!("{}.{}.{}", v.major, v.minor, v.build),
            crate::VERSION
        );
        assert_eq!(v.revision, -1);
        assert!(std::ptr::eq(unsafe { super::GetAddonDef() }, def));
    }

    /// Enable while the previous load's image is still pinned reloads the
    /// same image: nexus-rs `init` must run for the first load only, and
    /// our `on_load` for every load. `init` needs a live Nexus API table, so
    /// the wrapper is pinned by source; the flag is exercised directly.
    #[test]
    fn reload_into_a_pinned_image_skips_nexus_init() {
        let _ = super::first_load_of_image();
        assert!(!super::first_load_of_image(), "second load of an image");
        assert!(!super::first_load_of_image(), "third load of an image");

        let src = include_str!("lib.rs");
        assert!(
            !src.contains(&["nexus::export", "! {"].concat()),
            "the macro's load wrapper panics on a reload into a pinned image"
        );
        let start = src
            .find("\nunsafe extern \"C-unwind\" fn load_wrapper")
            .unwrap();
        let body = &src[start..start + src[start..].find("\n}\n").unwrap()];
        let gate = body
            .find("if first_load_of_image()")
            .expect("init is gated");
        let init = body.find("__macro::init").expect("first load inits");
        let load = body.find("on_load()").expect("every load runs on_load");
        assert!(gate < init && init < load, "gate, then init, then on_load");
        assert_eq!(body.matches("on_load()").count(), 1);
        let catch = body.find("catch_unwind").expect("init is caught");
        assert!(
            gate < catch && catch < init,
            "init runs inside catch_unwind"
        );
        let start = src
            .find("\nunsafe extern \"C-unwind\" fn unload_wrapper")
            .unwrap();
        let unload = &src[start..start + src[start..].find("\n}\n").unwrap()];
        assert!(
            unload.contains("catch_unwind(|| unsafe { nexus::__macro::deinit() })"),
            "deinit runs inside catch_unwind"
        );

        // The resumed image still holds the previous unload's latches.
        let start = src.find("\nfn on_load()").unwrap();
        let on_load = &src[start..start + src[start..].find("\n}\n").unwrap()];
        let init = on_load.find("state::init").unwrap();
        for reset in [
            "radio::player::arm()",
            "HOST_ATTACHED.store(false",
            "BOOTSTRAP_FAILED.store(false",
        ] {
            let at = on_load.find(reset).unwrap_or(usize::MAX);
            assert!(at < init, "on_load must run {reset} before state::init");
        }
    }

    #[test]
    fn chrome_settle_outlasts_the_observed_arcdps_race() {
        assert!(
            super::CHROME_SETTLE >= Duration::from_millis(2000),
            "crash is ~1s after load; settle must wait out that window"
        );
    }

    /// Pin: chrome attach must always run inside `catch_unwind`. A panic from
    /// `register_render` / `add_quick_access` / `get_texture_or_create_from_memory`
    /// would otherwise unwind into Nexus's `extern "C-unwind"` callback and the
    /// game. `bootstrap_chrome` owns that guard for the deferred path; the
    /// function body itself must not skip it.
    #[test]
    fn bootstrap_chrome_guards_attach_in_catch_unwind() {
        let src = include_str!("lib.rs");
        let start = src
            .find("\nfn bootstrap_chrome(")
            .expect("bootstrap_chrome must exist");
        let rest = &src[start..];
        let end = rest[1..].find("\nfn ").map(|i| i + 1).unwrap_or(rest.len());
        let body = &rest[..end];
        assert!(
            body.contains("attach_overlay_host"),
            "bootstrap_chrome must call attach_overlay_host, which itself wraps D3D work in catch_unwind"
        );
        assert!(
            body.contains("attach_overlay_host"),
            "bootstrap_chrome must call attach_overlay_host"
        );
    }

    /// Pin: `attach_overlay_host` must short-circuit when already attached and
    /// must release the slot on panic, otherwise a panic inside D3D work would
    /// silently kill the overlay for the rest of the session.
    #[test]
    fn attach_overlay_host_is_idempotent_and_recovers_from_panic() {
        let src = include_str!("lib.rs");
        let start = src
            .find("\nfn attach_overlay_host(")
            .expect("attach_overlay_host must exist");
        let rest = &src[start..];
        let end = rest[1..].find("\nfn ").map(|i| i + 1).unwrap_or(rest.len());
        let body = &rest[..end];
        assert!(
            body.contains("HOST_ATTACHED"),
            "attach_overlay_host must consult HOST_ATTACHED"
        );
        assert!(
            body.contains("catch_unwind"),
            "attach_overlay_host must catch_unwind so a panic cannot escape into Nexus"
        );
        assert!(
            body.contains("BOOTSTRAP_FAILED"),
            "attach_overlay_host must latch BOOTSTRAP_FAILED so the bootstrapper can observe a dead attach"
        );
    }
}
