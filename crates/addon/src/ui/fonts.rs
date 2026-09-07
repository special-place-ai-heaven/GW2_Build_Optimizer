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
use std::sync::atomic::{AtomicPtr, Ordering};

const SIZE_PX: f32 = 16.0;
/// Native size for the now-playing ticker face — rasterized crisp at this
/// size instead of bitmap-upscaling the 16 px atlas (which reads as a badly
/// scaled image in-game).
const TICKER_SIZE_PX: f32 = 42.0;
const LATIN_FILES: &[&str] = &["segoeui.ttf", "arial.ttf", "tahoma.ttf", "calibri.ttf"];
const ZH_FILES: &[&str] = &["msyh.ttc", "msyh.ttf", "simsun.ttc"];
const JA_FILES: &[&str] = &["YuGothM.ttc", "YuGothR.ttc", "meiryo.ttc", "msgothic.ttc"];
const KO_FILES: &[&str] = &["malgun.ttf", "malgunsl.ttf"];
const ID_LATIN: &str = "GW2BO_FONT_LATIN";
const ID_ZH: &str = "GW2BO_FONT_ZH";
const ID_JA: &str = "GW2BO_FONT_JA";
const ID_KO: &str = "GW2BO_FONT_KO";
const ID_TICKER: &str = "GW2BO_FONT_TICKER";

/// Inclusive pairs, 0-terminated. Latin-1 + Ext-A (Polish) + Cyrillic +
/// General Punctuation + arrows + math + symbols.
///
/// Most of this text is written by a language model, not by us, and a glyph
/// the atlas lacks reaches the player as '?'. Our own strings can be held to
/// ASCII by a test; the model's cannot, and it writes em dashes, curly quotes,
/// bullets, `->` as an arrow and `>=` as a relation without being asked.
/// General Punctuation was cut at 0x2027, which covered dashes and the
/// ellipsis but stopped short of the primes and the wider quotes; arrows and
/// math were absent entirely. Rasterizing the rest costs a few kilobytes of a
/// 16 px atlas, which is cheaper than one more round of hunting a question
/// mark through twelve catalogs.
const LATIN_RANGES: &[ImWchar] = &[
    0x0020, 0x00FF, // Latin-1
    0x0100, 0x017F, // Latin Extended-A (Polish)
    0x0400, 0x04FF, // Cyrillic (Russian)
    0x2010, 0x205E, // General Punctuation: dashes, quotes, ellipsis, bullets
    0x2190, 0x21FF, // Arrows
    0x2200, 0x22FF, // Mathematical Operators
    0x2600, 0x27BF, // Misc symbols + dingbats
    0,
];

static LATIN: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());
static ZH: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());
static JA: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());
static KO: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());
static TICKER: AtomicPtr<ImFont> = AtomicPtr::new(std::ptr::null_mut());

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
        ID_TICKER => TICKER.store(ptr, Ordering::Release),
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

/// Register only the face this frame will push. Safe to call every frame.
///
/// Only `"game"` returns before touching the atlas; every language, English
/// included, loads the face `resolve_font_id` picks.
pub fn init(pref: &str, ui_language: &str) {
    let Some(id) = resolve_font_id(pref, ui_language) else {
        return;
    };
    if !slot_ptr(id).is_null() {
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

    let dir = windows_fonts_dir();
    // ImGui copies ImFontConfig during AddFont; only GlyphRanges must outlive Build.
    match id {
        ID_LATIN => {
            let latin_cfg = make_cfg(LATIN_RANGES.as_ptr(), 2);
            try_add(
                ID_LATIN,
                first_existing(&dir, LATIN_FILES),
                Some(&latin_cfg),
                SIZE_PX,
            );
        }
        ID_ZH => {
            let zh_cfg = make_cfg(
                with_latin(unsafe { ImFontAtlas_GetGlyphRangesChineseSimplifiedCommon(atlas) }),
                1,
            );
            try_add(
                ID_ZH,
                first_existing(&dir, ZH_FILES),
                Some(&zh_cfg),
                SIZE_PX,
            );
        }
        ID_JA => {
            let ja_cfg = make_cfg(
                with_latin(unsafe { ImFontAtlas_GetGlyphRangesJapanese(atlas) }),
                1,
            );
            try_add(
                ID_JA,
                first_existing(&dir, JA_FILES),
                Some(&ja_cfg),
                SIZE_PX,
            );
        }
        ID_KO => {
            let ko_cfg = make_cfg(
                with_latin(unsafe { ImFontAtlas_GetGlyphRangesKorean(atlas) }),
                1,
            );
            try_add(
                ID_KO,
                first_existing(&dir, KO_FILES),
                Some(&ko_cfg),
                SIZE_PX,
            );
        }
        _ => {}
    }
}

/// Register the big now-playing ticker face (Latin, 42 px native) once —
/// English UI included: the marquee wants a crisp large font, not a 3x
/// bitmap upscale of the 16 px atlas. Safe to call every frame.
pub fn init_ticker() {
    if !TICKER.load(Ordering::Acquire).is_null() {
        return;
    }
    let dir = windows_fonts_dir();
    let cfg = make_cfg(LATIN_RANGES.as_ptr(), 2);
    try_add(
        ID_TICKER,
        first_existing(&dir, LATIN_FILES),
        Some(&cfg),
        TICKER_SIZE_PX,
    );
}

/// Push the 42 px ticker face; `None` (atlas rebuild, no TTF found) lets the
/// caller fall back to scaling the current font.
pub fn push_ticker() -> Option<FontGuard> {
    let ptr = TICKER.load(Ordering::Acquire);
    if ptr.is_null() {
        return None;
    }
    // Safety: from the Nexus font callback; FontGuard pops on drop.
    unsafe { sys::igPushFont(ptr) };
    Some(FontGuard)
}

/// Whether `u` is one of the codepoints the Latin face is built with.
///
/// Read straight off `LATIN_RANGES` rather than restated as a chain of range
/// checks, which is what the constant is for. The restatement drifted once
/// already: a gate narrower than the font meant one decoration symbol (stars,
/// notes) anywhere in a title kicked the whole string to the blurry 3x bitmap
/// path, where the ASCII-only base atlas also turned every accented char into
/// '?'. A copy that must be kept in step by hand eventually is not.
fn in_latin_ranges(u: u32) -> bool {
    // Pairs are inclusive; the terminating 0 is left over and ignored.
    LATIN_RANGES
        .as_chunks::<2>()
        .0
        .iter()
        .any(|[lo, hi]| (u32::from(*lo)..=u32::from(*hi)).contains(&u))
}

/// Whether every char of `text` is inside the ticker face's glyph ranges.
/// CJK titles fall back to the scaled UI font rather than 42 px tofu.
pub fn ticker_can_render(text: &str) -> bool {
    text.chars().all(|c| in_latin_ranges(c as u32))
}

fn try_add(id: &str, path: Option<PathBuf>, config: Option<&ImFontConfig>, size_px: f32) {
    let Some(path) = path else {
        log(
            LogLevel::Info,
            "GW2 Build Optimizer",
            format!("overlay font {id}: no Windows TTF found, skipping"),
        );
        return;
    };
    add_font_from_file(id, &path, size_px, config, RECEIVE).revert_on_unload();
    log(
        LogLevel::Info,
        "GW2 Build Optimizer",
        format!("overlay font {id}: {}", path.display()),
    );
}

/// ImGui's built-in CJK range lists cover their script plus Latin-1 and stop
/// there: no General Punctuation, no arrows, no math. A player who picks the
/// Japanese face for an English UI (`ui_font: "ja"`, seen 2026-09-07) then
/// reads the model's em dash as '?', exactly the bug the Latin face was
/// fixed for the day before. Append [`LATIN_RANGES`] to the built-in list so
/// no face choice can lose the punctuation a language model writes.
///
/// The merged list must outlive the atlas build, so it is leaked once per
/// face — three small allocations for the life of the process.
fn with_latin(builtin: *const ImWchar) -> *const ImWchar {
    let mut merged: Vec<ImWchar> = Vec::new();
    if !builtin.is_null() {
        let mut i = 0;
        // Safety: ImGui range lists are 0-terminated pairs.
        loop {
            let v = unsafe { *builtin.add(i) };
            if v == 0 {
                break;
            }
            merged.push(v);
            i += 1;
        }
    }
    merged.extend(
        LATIN_RANGES
            .iter()
            .copied()
            .take(LATIN_RANGES.len().saturating_sub(1)),
    );
    merged.push(0);
    Box::leak(merged.into_boxed_slice()).as_ptr()
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
            // Every Latin language, English included. This used to be
            // `"ru" | "pl" => Some(ID_LATIN), _ => None`, so an English
            // player drew in the Nexus/GW2 typeface - an atlas we neither
            // build nor declare ranges on, whose missing glyphs ImGui
            // replaces with '?'. Declaring `LATIN_RANGES` could not help,
            // because it configures a font English never pushed. In-game
            // 2026-09-06 the model wrote "Minstrel's has zero toughness -
            // focus fire was always going to eat you first" and the player
            // read a question mark mid-sentence.
            //
            // The face is loaded either way (the ticker uses it), so this
            // spends no extra memory, and anyone who prefers the game
            // typeface still picks "game" in Settings. If no Windows TTF
            // is found, `live_ptr` returns null and `push` falls back to
            // exactly the old behaviour.
            _ => Some(ID_LATIN),
        },
    }
}

pub fn combo_options() -> Vec<(&'static str, &'static str)> {
    let dir = windows_fonts_dir();
    let mut v = vec![
        ("auto", "settings.font_auto"),
        ("game", "settings.font_game"),
    ];
    if first_existing(&dir, LATIN_FILES).is_some() {
        v.push(("segoe", "settings.font_segoe"));
    }
    if first_existing(&dir, ZH_FILES).is_some() {
        v.push(("zh", "settings.font_yahei"));
    }
    if first_existing(&dir, JA_FILES).is_some() {
        v.push(("ja", "settings.font_japanese"));
    }
    if first_existing(&dir, KO_FILES).is_some() {
        v.push(("ko", "settings.font_korean"));
    }
    v
}

/// zh/ja/ko native names need a CJK face. Otherwise use the English catalog name.
pub fn language_label(
    lang: &gw2_core::i18n::Language,
    pref: &str,
    ui_language: &str,
) -> &'static str {
    if !matches!(lang.code, "zh" | "ja" | "ko") {
        return lang.native_name;
    }
    let ok = matches!(
        (lang.code, resolve_font_id(pref, ui_language)),
        ("zh", Some(ID_ZH)) | ("ja", Some(ID_JA)) | ("ko", Some(ID_KO))
    );
    if ok {
        lang.native_name
    } else {
        lang.choya_name
    }
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
    }

    /// English used to resolve to `None`, which drew the whole overlay in the
    /// Nexus/GW2 typeface - an atlas we do not build, so `LATIN_RANGES` bought
    /// it nothing and a missing glyph reached the player as '?'.
    #[test]
    fn auto_gives_every_latin_language_the_face_we_declare_ranges_on() {
        for lang in ["en", "fr", "de", "es", "it", "pt", "cs"] {
            assert_eq!(
                resolve_font_id("auto", lang),
                Some(ID_LATIN),
                "{lang} must draw in the face whose glyph ranges we control"
            );
        }
    }

    /// The model writes this text, and no test can hold it to ASCII. In-game
    /// 2026-09-06 it wrote an em dash and the player read a question mark.
    #[test]
    fn the_typography_a_model_writes_is_inside_the_atlas() {
        for c in "—–…“”‘’•·→←≥≤×≈".chars() {
            assert!(
                in_latin_ranges(c as u32),
                "U+{:04X} {c:?} is not in LATIN_RANGES; it would draw as '?'",
                c as u32
            );
        }
    }

    #[test]
    fn the_ticker_gate_tracks_the_font_it_guards() {
        assert!(ticker_can_render("Cafe - Bloc Party"));
        assert!(ticker_can_render("Пикник ★"));
        assert!(!ticker_can_render("東京"));
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

    #[test]
    fn combo_lists_faces_present_on_disk() {
        let ids: Vec<_> = combo_options().into_iter().map(|(id, _)| id).collect();
        assert!(ids.contains(&"auto"));
        assert!(ids.contains(&"game"));
        let dir = windows_fonts_dir();
        if first_existing(&dir, LATIN_FILES).is_some() {
            assert!(ids.contains(&"segoe"), "{ids:?}");
        }
        if first_existing(&dir, ZH_FILES).is_some() {
            assert!(ids.contains(&"zh"), "{ids:?}");
        }
        if first_existing(&dir, JA_FILES).is_some() {
            assert!(ids.contains(&"ja"), "{ids:?}");
        }
        if first_existing(&dir, KO_FILES).is_some() {
            assert!(ids.contains(&"ko"), "{ids:?}");
        }
    }

    #[test]
    fn cjk_language_rows_use_english_until_that_face_is_active() {
        let zh = gw2_core::i18n::language_by_code("zh").unwrap();
        let ja = gw2_core::i18n::language_by_code("ja").unwrap();
        let ko = gw2_core::i18n::language_by_code("ko").unwrap();
        assert_eq!(language_label(zh, "auto", "en"), "Simplified Chinese");
        assert_eq!(language_label(ja, "game", "en"), "Japanese");
        assert_eq!(language_label(ko, "segoe", "en"), "Korean");
        assert_eq!(language_label(zh, "zh", "en"), zh.native_name);
        assert_eq!(language_label(ja, "ja", "en"), ja.native_name);
        assert_eq!(language_label(ko, "ko", "en"), ko.native_name);
        let ru = gw2_core::i18n::language_by_code("ru").unwrap();
        assert_eq!(language_label(ru, "auto", "en"), ru.native_name);
    }
}
