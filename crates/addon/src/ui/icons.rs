//! ArenaNet skill/trait/item icons.
//!
//! Prefer `cache/graphics/*.png` (downloaded with game data). Fall back to the
//! render CDN through Nexus. Corners use the same 5px rounding as buttons.

use std::path::PathBuf;
use std::sync::Mutex;

use gw2_optimizer::gamedb::GameDb;
use nexus::imgui::{DrawListMut, TextureId, Ui};

use super::theme;

static GRAPHICS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static REQUESTED: Mutex<Option<std::collections::HashSet<String>>> = Mutex::new(None);

pub fn set_graphics_dir(dir: PathBuf) {
    if let Ok(mut g) = GRAPHICS_DIR.lock() {
        *g = Some(dir);
    }
}

fn graphics_dir() -> Option<PathBuf> {
    GRAPHICS_DIR.lock().ok()?.clone()
}

fn tex_id(endpoint: &str) -> String {
    format!("GW2BO{}", endpoint.replace('/', "_"))
}

fn ensure_texture(url: &str) -> Option<TextureId> {
    let (remote, endpoint) = gw2_api::graphics::parse_render_url(url)?;
    let id = tex_id(endpoint);
    let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nexus::texture::get_texture(&id)
    }))
    .ok()
    .flatten();
    if let Some(tex) = loaded {
        return Some(tex.id());
    }

    if let Some(path) = graphics_dir().and_then(|d| gw2_api::graphics::local_path(&d, endpoint)) {
        if path.exists() {
            let from_file = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                nexus::texture::get_texture_or_create_from_file(&id, &path)
            }))
            .ok()
            .flatten();
            if let Some(tex) = from_file {
                return Some(tex.id());
            }
        }
    }

    let mut req = REQUESTED.lock().ok()?;
    let set = req.get_or_insert_with(std::collections::HashSet::new);
    if set.insert(id.clone()) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            nexus::texture::load_texture_from_url(&id, remote, endpoint, None);
        }));
    }
    None
}

/// Draw a rounded icon (or a gold plate while it loads). Advances the cursor.
pub fn draw(ui: &Ui, url: Option<&str>, size: f32, tint: [f32; 4]) -> bool {
    let p = ui.cursor_screen_pos();
    let id = format!("##ico_{}_{}", p[0] as i32, p[1] as i32);
    let clicked = ui.invisible_button(&id, [size, size]);
    paint_at(ui, url, p, size, tint);
    clicked
}

pub fn paint_at(ui: &Ui, url: Option<&str>, p: [f32; 2], size: f32, tint: [f32; 4]) {
    let dl = ui.get_window_draw_list();
    paint_on(&dl, url, p, [p[0] + size, p[1] + size], tint);
}

pub fn paint_on(
    draw_list: &DrawListMut<'_>,
    url: Option<&str>,
    p_min: [f32; 2],
    p_max: [f32; 2],
    tint: [f32; 4],
) {
    let rounding = theme::ICON_ROUNDING;
    if let Some(tid) = url.and_then(ensure_texture) {
        draw_list
            .add_image_rounded(tid, p_min, p_max, rounding)
            .col(tint)
            .build();
    } else {
        draw_list
            .add_rect(p_min, p_max, theme::PLATE)
            .filled(true)
            .rounding(rounding)
            .build();
        draw_list
            .add_rect(p_min, p_max, theme::GOLD_DIM)
            .rounding(rounding)
            .build();
    }
}

pub fn item_url(db: &GameDb, id: u32) -> Option<&str> {
    db.items.get(&id).and_then(|i| i.icon.as_deref())
}

pub fn skill_url(db: &GameDb, id: u32) -> Option<&str> {
    db.skills.get(&id).and_then(|s| s.icon.as_deref())
}

pub fn trait_url(db: &GameDb, id: u32) -> Option<&str> {
    db.traits.get(&id).and_then(|t| t.icon.as_deref())
}

pub fn spec_url(db: &GameDb, id: u32) -> Option<&str> {
    db.specializations.get(&id).and_then(|s| s.icon.as_deref())
}

pub fn skill_url_by_name<'a>(db: &'a GameDb, name: &str) -> Option<&'a str> {
    lookup_name(db.skills.values().map(|s| (s.name.as_str(), s.icon.as_deref())), name)
}

pub fn spec_url_by_name<'a>(db: &'a GameDb, name: &str) -> Option<&'a str> {
    lookup_name(
        db.specializations
            .values()
            .map(|s| (s.name.as_str(), s.icon.as_deref())),
        name,
    )
}

pub fn trait_url_by_name<'a>(db: &'a GameDb, name: &str) -> Option<&'a str> {
    lookup_name(db.traits.values().map(|t| (t.name.as_str(), t.icon.as_deref())), name)
}

pub fn upgrade_url<'a>(db: &'a GameDb, name: &str) -> Option<&'a str> {
    if name.is_empty() {
        return None;
    }
    for ids in [&db.runes, &db.sigils, &db.relics] {
        if let Some(url) = lookup_item_ids(db, ids, name) {
            return Some(url);
        }
    }
    None
}

pub fn weapon_type_url<'a>(db: &'a GameDb, profession: &str, weapon_type: &str) -> Option<&'a str> {
    let prof = db.professions.get(profession)?;
    let w = prof.weapons.get(weapon_type)?;
    let sid = w.skills.first()?.id;
    skill_url(db, sid)
}

fn lookup_item_ids<'a>(db: &'a GameDb, ids: &[u32], name: &str) -> Option<&'a str> {
    lookup_name(
        ids.iter()
            .filter_map(|id| db.items.get(id).map(|i| (i.name.as_str(), i.icon.as_deref()))),
        name,
    )
}

fn lookup_name<'a>(
    rows: impl Iterator<Item = (&'a str, Option<&'a str>)>,
    needle: &str,
) -> Option<&'a str> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let lower = needle.to_lowercase();
    let mut best: Option<(usize, &'a str)> = None;
    for (name, icon) in rows {
        let Some(icon) = icon else { continue };
        if name.eq_ignore_ascii_case(needle) {
            return Some(icon);
        }
        let nlow = name.to_lowercase();
        if nlow.contains(&lower) {
            let key = name.len();
            if best.is_none_or(|(blen, _)| key < blen) {
                best = Some((key, icon));
            }
        }
    }
    best.map(|(_, u)| u)
}
