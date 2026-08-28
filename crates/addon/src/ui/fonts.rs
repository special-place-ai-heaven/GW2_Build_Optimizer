//! Overlay fonts via Nexus FontApi (host owns the ImGui atlas).
//!
//! Do not call `io.Fonts->AddFontFromFileTTF` here — Nexus rebuilds the atlas
//! for every addon. Missing glyphs (`?`) are the default GW2/Nexus Latin-only
//! atlas; we load one Windows TTF per script and `igPushFont` for our window.

use nexus::font::add_font_from_file;
use nexus::imgui::sys::{
    self, ImFont, ImFontAtlas_GetGlyphRangesChineseSimplifiedCommon,
    ImFontAtlas_GetGlyphRangesJapanese, ImFontAtlas_GetGlyphRangesKorean, ImFontConfig,
    ImFontConfig_ImFontConfig, ImFontConfig_destroy, ImWchar,
};
use nexus::log::{log, LogLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

const SIZE_PX: f32 = 16.0;
const ID_LATIN: &str = "GW2BO_FONT_LATIN";
const ID_ZH: &str = "GW2BO_FONT_ZH";
const ID_JA: &str = "GW2BO_FONT_JA";
const ID_KO: &str = "GW2BO_FONT_KO";

/// Inclusive pairs, 0-terminated. Latin-1 + Ext-A (Polish) + Cyrillic + dashes/ellipsis.
const LATIN_RANGES: &[ImWchar] = &[
    0x0020, 0x00FF, 0x0100, 0x017F, 0x0400, 0x04FF, 0x2010, 0x2027, 0x2600, 0x27BF, 0,
];

static STARTED: AtomicBool = AtomicBool::new(false);
static LOADED_LATIN: AtomicBool = AtomicBool::new(false);
static LOADED_ZH: AtomicBool = AtomicBool::new(false);
static LOADED_JA: AtomicBool = AtomicBool::new(false);
static LOADED_KO: AtomicBool = AtomicBool::new(false);
static LATIN: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());
static ZH: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());
static JA: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());
static KO: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());

const RECEIVE: nexus::font::RawFontReceive = nexus::font_receive!(|id, font| {
    let ptr = font
        .map(|f| f as *mut ImFont)
        .unwrap_or(std::ptr::null_mut());
    store_ptr(id, ptr);
});

fn store_ptr(id: &str, ptr: *mut ImFont) {
    match id {
        ID_LATIN => LATIN.store(ptr, Ordering::Release),
        ID_ZH => ZH.store(ptr, Ordering::Release),
        ID_JA => JA.store(ptr, Ordering::Release),
        ID_KO => KO.store(ptr, Ordering::Release),
        _ => {}
    }
}

fn slot_ptr(id: &str) -> *mut ImFont {
    match id {
        ID_LATIN => LATIN.load(Ordering::Acquire),
        ID_ZH => ZH.load(Ordering::Acquire),
        ID_JA => JA.load(Ordering::Acquire),
        ID_KO => KO.load(Ordering::Acquire),
        _ => std::ptr::null_mut(),
    }
}

/// Pops the font Nexus/`igPushFont` pushed. Must drop even if the window panics.
pub struct FontGuard;

impl Drop for FontGuard {
    fn drop(&mut self) {
        // Safety: paired with a successful `igPushFont` in `push`.
        unsafe { sys::igPopFont() };
    }
}

/// Register Windows fonts once ImGui exists. Safe to call every frame.
pub fn init() {
    if STARTED.load(Ordering::Acquire) {
        return;
    }
    let atlas = unsafe {
        let io = sys::igGetIO();
        if io.is_null() {
            return;
        }
        (*io).Fonts
    };
    if atlas.is_null() {
        return;
    }
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    let dir = windows_fonts_dir();
    // ImGui copies ImFontConfig during AddFont; only GlyphRanges must outlive Build.
    let latin_cfg = make_cfg(LATIN_RANGES.as_ptr(), 2);
    try_add(
        ID_LATIN,
        first_existing(
            &dir,
            &["segoeui.ttf", "arial.ttf", "tahoma.ttf", "calibri.ttf"],
        ),
        Some(&latin_cfg),
        &LOADED_LATIN,
    );

    let zh_cfg = make_cfg(
        unsafe { ImFontAtlas_GetGlyphRangesChineseSimplifiedCommon(atlas) },
        1,
    );
    try_add(
        ID_ZH,
        first_existing(&dir, &["msyh.ttc", "msyh.ttf", "simsun.ttc"]),
        Some(&zh_cfg),
        &LOADED_ZH,
    );
    let ja_cfg = make_cfg(unsafe { ImFontAtlas_GetGlyphRangesJapanese(atlas) }, 1);
    try_add(
        ID_JA,
        first_existing(
            &dir,
            &["YuGothM.ttc", "YuGothR.ttc", "meiryo.ttc", "msgothic.ttc"],
        ),
        Some(&ja_cfg),
        &LOADED_JA,
    );
    let ko_cfg = make_cfg(unsafe { ImFontAtlas_GetGlyphRangesKorean(atlas) }, 1);
    try_add(
        ID_KO,
        first_existing(&dir, &["malgun.ttf", "malgunsl.ttf"]),
        Some(&ko_cfg),
        &LOADED_KO,
    );
}

fn try_add(id: &str, path: Option<PathBuf>, config: Option<&ImFontConfig>, loaded: &AtomicBool) {
    let Some(path) = path else {
        log(
            LogLevel::Info,
            "GW2 Build Optimizer",
            format!("overlay font {id}: no Windows TTF found, skipping"),
        );
        return;
    };
    add_font_from_file(id, &path, SIZE_PX, config, RECEIVE).revert_on_unload();
    loaded.store(true, Ordering::Release);
    log(
        LogLevel::Info,
        "GW2 Build Optimizer",
        format!("overlay font {id}: {}", path.display()),
    );
}

fn make_cfg(ranges: *const ImWchar, oversample_h: i32) -> ImFontConfig {
    unsafe {
        let p = ImFontConfig_ImFontConfig();
        let mut c = *p;
        ImFontConfig_destroy(p);
        c.GlyphRanges = ranges;
        c.FontNo = 0;
        c.OversampleH = oversample_h;
        c.OversampleV = 1;
        c.PixelSnapH = true;
        c
    }
}

/// Push the configured overlay font. `None` keeps the Nexus/GW2 typeface.
pub fn push(pref: &str, ui_language: &str) -> Option<FontGuard> {
    let wanted = resolve_font_id(pref, ui_language)?;
    let ptr = live_ptr(wanted);
    if ptr.is_null() {
        return None;
    }
    // Safety: `ptr` came from Nexus' font callback (null during atlas rebuild).
    // FontGuard pops on drop, including unwind inside `ui::render`'s catch_unwind.
    unsafe { sys::igPushFont(ptr) };
    Some(FontGuard)
}

fn live_ptr(id: &str) -> *mut ImFont {
    let ptr = slot_ptr(id);
    if !ptr.is_null() {
        return ptr;
    }
    if id != ID_LATIN {
        return LATIN.load(Ordering::Acquire);
    }
    std::ptr::null_mut()
}

/// `"auto"` follows language; `"game"` never pushes.
pub fn resolve_font_id(pref: &str, ui_language: &str) -> Option<&'static str> {
    match pref {
        "game" => None,
        "segoe" => Some(ID_LATIN),
        "zh" => Some(ID_ZH),
        "ja" => Some(ID_JA),
        "ko" => Some(ID_KO),
        _ => match gw2_core::i18n::resolve(ui_language) {
            "zh" => Some(ID_ZH),
            "ja" => Some(ID_JA),
            "ko" => Some(ID_KO),
            "ru" | "pl" => Some(ID_LATIN),
            _ => None,
        },
    }
}

pub fn combo_options() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        ("auto", "settings.font_auto"),
        ("game", "settings.font_game"),
    ];
    if LOADED_LATIN.load(Ordering::Acquire) {
        v.push(("segoe", "settings.font_segoe"));
    }
    if LOADED_ZH.load(Ordering::Acquire) {
        v.push(("zh", "settings.font_yahei"));
    }
    if LOADED_JA.load(Ordering::Acquire) {
        v.push(("ja", "settings.font_japanese"));
    }
    if LOADED_KO.load(Ordering::Acquire) {
        v.push(("ko", "settings.font_korean"));
    }
    v
}

pub fn label_key(pref: &str) -> &'static str {
    match pref {
        "game" => "settings.font_game",
        "segoe" => "settings.font_segoe",
        "zh" => "settings.font_yahei",
        "ja" => "settings.font_japanese",
        "ko" => "settings.font_korean",
        _ => "settings.font_auto",
    }
}

pub(crate) fn windows_fonts_dir() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

pub(crate) fn first_existing(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn latin_ranges_are_zero_terminated_pairs() {
        assert_eq!(LATIN_RANGES.last().copied(), Some(0));
        assert_eq!(LATIN_RANGES.len() % 2, 1);
        assert!(LATIN_RANGES.len() >= 3);
    }

    #[test]
    fn resolve_game_never_pushes() {
        assert_eq!(resolve_font_id("game", "zh"), None);
        assert_eq!(resolve_font_id("game", "en"), None);
    }

    #[test]
    fn resolve_auto_picks_script() {
        assert_eq!(resolve_font_id("auto", "zh"), Some(ID_ZH));
        assert_eq!(resolve_font_id("auto", "ja"), Some(ID_JA));
        assert_eq!(resolve_font_id("auto", "ko"), Some(ID_KO));
        assert_eq!(resolve_font_id("auto", "ru"), Some(ID_LATIN));
        assert_eq!(resolve_font_id("auto", "pl"), Some(ID_LATIN));
        assert_eq!(resolve_font_id("auto", "en"), None);
        assert_eq!(resolve_font_id("auto", "fr"), None);
    }

    #[test]
    fn resolve_explicit_segoe() {
        assert_eq!(resolve_font_id("segoe", "zh"), Some(ID_LATIN));
    }

    #[test]
    fn first_existing_picks_first_real_file() {
        let dir = std::env::temp_dir().join(format!("gw2bo_font_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let hit = dir.join("arial.ttf");
        fs::write(&hit, b"x").unwrap();
        let found = first_existing(&dir, &["missing.ttf", "arial.ttf", "later.ttf"]);
        assert_eq!(found.as_deref(), Some(hit.as_path()));
        assert!(first_existing(&dir, &["nope.ttf"]).is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn label_key_defaults_to_auto() {
        assert_eq!(label_key("auto"), "settings.font_auto");
        assert_eq!(label_key(""), "settings.font_auto");
        assert_eq!(label_key("game"), "settings.font_game");
    }
}
