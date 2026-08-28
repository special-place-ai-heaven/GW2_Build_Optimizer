//! Local PNG cache for ArenaNet render-service icons.
//!
//! Game JSON (`cache/*.json`) changes with almost every build. Icons almost
//! never do — only large patches. Keep them in `cache/graphics/` and skip any
//! file that already exists. Refresh Game Data must not delete this folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::cache::DataCache;
use crate::client::{ApiError, Gw2Client};
use crate::models::{Item, Pet, Profession, PvpAmulet, Skill, Specialization, Trait};

const RENDER_HOSTS: &[&str] = &[
    "https://render.guildwars2.com",
    "http://render.guildwars2.com",
];

/// Split a render-service URL into `(https host, /file/HASH/ID.png)`.
pub fn parse_render_url(url: &str) -> Option<(&'static str, &str)> {
    let url = url.trim();
    for host in RENDER_HOSTS {
        if let Some(path) = url.strip_prefix(host) {
            if path.starts_with("/file/") && path.len() > "/file/".len() {
                return Some(("https://render.guildwars2.com", path));
            }
        }
    }
    None
}

/// `12345.png` from `/file/<hash>/12345.png`.
///
/// The result is joined onto the graphics directory by `local_path`, so it must
/// be a single, ordinary filename. Anything that could redirect the join —
/// a `..` parent segment, an embedded separator, or a Windows drive/UNC prefix
/// (`Path::join` *replaces* the base when the joined path has a root or
/// prefix) — is rejected. Icon URLs come from the GW2 API today, but the cache
/// path must not depend on that staying true.
pub fn png_filename(endpoint: &str) -> Option<String> {
    let name = endpoint.rsplit('/').next()?;
    if !name.ends_with(".png") || name.len() <= 4 {
        return None;
    }
    // `:` is not a separator on Unix but is one on Windows (`C:x.png` is
    // drive-relative, `a:b` is an alternate data stream); reject it on every
    // platform so the check does not depend on the build target.
    if name.contains(['/', '\\', ':']) || name.starts_with('.') {
        return None;
    }
    // Must parse to exactly one ordinary component — this is what rejects
    // `..`, roots, and prefixes structurally rather than by blocklist.
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(std::path::Component::Normal(only)), None) if only == std::ffi::OsStr::new(name) => {
            Some(name.to_string())
        }
        _ => None,
    }
}

pub fn local_path(graphics_dir: &Path, endpoint: &str) -> Option<PathBuf> {
    Some(graphics_dir.join(png_filename(endpoint)?))
}

/// URLs we still need to fetch (existing files are skipped).
pub fn pending_downloads(graphics_dir: &Path, urls: &[String]) -> Vec<(String, PathBuf)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for url in urls {
        let Some((_, endpoint)) = parse_render_url(url) else {
            continue;
        };
        if !seen.insert(endpoint.to_string()) {
            continue;
        }
        let Some(path) = local_path(graphics_dir, endpoint) else {
            continue;
        };
        if path.exists() {
            continue;
        }
        out.push((format!("https://render.guildwars2.com{endpoint}"), path));
    }
    out
}

/// Collect render URLs from cached JSON. Skills, traits, specs, professions,
/// amulets, and equipment items (armor/weapons/runes/relics). Not the full
/// 50k item dump — only what we already cache.
pub fn collect_from_cache(cache: &DataCache) -> Vec<String> {
    let mut urls = Vec::new();
    push_icons(
        &mut urls,
        cache.load::<Vec<Skill>>("skills").ok().flatten(),
        |s| s.icon.as_deref(),
    );
    push_icons(
        &mut urls,
        cache.load::<Vec<Trait>>("traits").ok().flatten(),
        |t| t.icon.as_deref(),
    );
    if let Ok(Some(specs)) = cache.load::<Vec<Specialization>>("specializations") {
        for s in specs {
            if let Some(u) = s.icon {
                urls.push(u);
            }
            if let Some(u) = s.profession_icon {
                urls.push(u);
            }
        }
    }
    push_icons(
        &mut urls,
        cache.load::<Vec<Profession>>("professions").ok().flatten(),
        |p| p.icon.as_deref(),
    );
    push_icons(
        &mut urls,
        cache.load::<Vec<PvpAmulet>>("pvp_amulets").ok().flatten(),
        |a| a.icon.as_deref(),
    );
    push_icons(
        &mut urls,
        cache.load::<Vec<Pet>>("pets").ok().flatten(),
        |p| p.icon.as_deref(),
    );
    push_icons(
        &mut urls,
        cache.load::<Vec<Item>>("items").ok().flatten(),
        |i| i.icon.as_deref(),
    );
    urls
}

fn push_icons<T>(urls: &mut Vec<String>, rows: Option<Vec<T>>, icon: impl Fn(&T) -> Option<&str>) {
    let Some(rows) = rows else {
        return;
    };
    for row in &rows {
        if let Some(u) = icon(row) {
            urls.push(u.to_string());
        }
    }
}

/// Download missing PNGs. Existing files are left alone. Per-icon failures are
/// skipped so a bad icon never aborts a game-data refresh — but a cancelled
/// `client` stops the workers and returns `ApiError::Cancelled`, which is the
/// one condition the caller must not treat as "some icons were missing".
pub fn download_missing(
    client: &Gw2Client,
    graphics_dir: &Path,
    urls: &[String],
    mut on_progress: impl FnMut(usize, usize),
) -> Result<(u32, u32), ApiError> {
    std::fs::create_dir_all(graphics_dir).map_err(|e| ApiError::Cache(e.to_string()))?;
    let pending = pending_downloads(graphics_dir, urls);
    let skipped = urls.len().saturating_sub(pending.len()) as u32;
    let total = pending.len();
    if total == 0 {
        on_progress(0, 0);
        return Ok((0, skipped));
    }

    // ponytail: 6 workers is enough for the CDN; the API rate limiter is not used.
    let chunks: Vec<Vec<(String, PathBuf)>> = pending
        .chunks((total / 6).max(1))
        .map(|c| c.to_vec())
        .collect();
    let done = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for chunk in &chunks {
            let done = &done;
            scope.spawn(move || {
                for (url, path) in chunk {
                    if client.is_cancelled() {
                        // Leave `done` short of `total`; the progress loop
                        // below watches the same flag and stops with us.
                        return;
                    }
                    if let Ok(bytes) = client.fetch_bytes(url) {
                        if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
                            let tmp = path.with_extension("tmp");
                            if std::fs::write(&tmp, &bytes).is_ok() {
                                let _ = std::fs::rename(&tmp, path);
                            }
                        }
                    }
                    done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            });
        }
        // Progress from this thread while workers run. Cancelled workers stop
        // short of `total`, so this loop must watch the flag too or it would
        // spin until the scope join that never comes.
        while done.load(std::sync::atomic::Ordering::Relaxed) < total && !client.is_cancelled() {
            on_progress(done.load(std::sync::atomic::Ordering::Relaxed), total);
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });
    if client.is_cancelled() {
        return Err(ApiError::Cancelled);
    }
    on_progress(total, total);
    Ok((total as u32, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_https_render_url() {
        let (host, path) = parse_render_url(
            "https://render.guildwars2.com/file/943538394A94A491C8632FBEF6203C2013443555/102478.png",
        )
        .expect("parse");
        assert_eq!(host, "https://render.guildwars2.com");
        assert_eq!(
            path,
            "/file/943538394A94A491C8632FBEF6203C2013443555/102478.png"
        );
        assert_eq!(png_filename(path).as_deref(), Some("102478.png"));
    }

    #[test]
    fn png_filename_rejects_parent_and_rooted_segments() {
        // The one shape the render service actually produces.
        assert_eq!(
            png_filename("/file/AAA/102478.png").as_deref(),
            Some("102478.png")
        );
        // A `..` in an earlier segment is harmless — only the last segment is
        // ever joined onto the graphics dir.
        assert_eq!(
            png_filename("/file/../102478.png").as_deref(),
            Some("102478.png")
        );
        for hostile in [
            "/file/AAA/..\\..\\evil.png",       // Windows parent traversal
            "/file/AAA/../evil.png/..",         // last segment is `..`
            "/file/AAA/C:evil.png",             // drive-relative: join() replaces the base
            "/file/AAA/\\\\host\\c$\\evil.png", // UNC root
            "/file/AAA/.png",                   // no stem
            "/file/AAA/..png",                  // dot-leading
            "/file/AAA/evil.exe",               // not a png
        ] {
            assert!(
                png_filename(hostile).is_none(),
                "{hostile} must not become a cache filename"
            );
        }
    }

    #[test]
    fn local_path_stays_inside_the_graphics_dir() {
        let dir = std::env::temp_dir().join("gw2_gfx_escape_probe");
        let ok = local_path(&dir, "/file/AAA/1.png").expect("plain icon resolves");
        assert!(ok.starts_with(&dir));
        assert!(local_path(&dir, "/file/AAA/..\\..\\evil.png").is_none());
        assert!(local_path(&dir, "/file/AAA/C:evil.png").is_none());
    }

    #[test]
    fn parse_rejects_wiki() {
        assert!(parse_render_url("https://wiki.guildwars2.com/images/x.png").is_none());
    }

    #[test]
    fn pending_skips_existing_file() {
        let dir = std::env::temp_dir().join(format!(
            "gw2_gfx_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1.png"), b"x").unwrap();
        let urls = vec![
            "https://render.guildwars2.com/file/AAA/1.png".into(),
            "https://render.guildwars2.com/file/BBB/2.png".into(),
        ];
        let pending = pending_downloads(&dir, &urls);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(pending.len(), 1);
        assert!(pending[0].1.ends_with("2.png"));
    }
}
