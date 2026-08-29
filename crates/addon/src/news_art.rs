//! Still images for news (JPEG/PNG). No video, no web pages.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use nexus::imgui::TextureId;
use reqwest::header::{HeaderMap, HeaderValue};

use crate::state::CancellationToken;

const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_BYTES: usize = 1_500_000;

#[derive(Clone)]
enum Slot {
    Pending,
    Ready { path: PathBuf, aspect: f32 },
    Failed,
}

static SLOTS: OnceLock<Mutex<HashMap<String, Slot>>> = OnceLock::new();

fn slots() -> std::sync::MutexGuard<'static, HashMap<String, Slot>> {
    SLOTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn url_ok(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else {
        return false;
    };
    if u.scheme() != "https" {
        return false;
    }
    match u.host_str() {
        Some("localhost") | Some("127.0.0.1") | Some("::1") => false,
        Some(_) => true,
        None => false,
    }
}

fn cache_id(url: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    format!("GW2BOnews{:016x}", h.finish())
}

fn file_stem(url: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        Some("jpg")
    } else if bytes.len() >= 4
        && bytes[0] == 0x89
        && bytes[1] == 0x50
        && bytes[2] == 0x4E
        && bytes[3] == 0x47
    {
        Some("png")
    } else {
        None
    }
}

/// Mark the next `n` unseen https URLs as in-flight so a worker can fetch them.
pub fn take_batch(urls: &[String], n: usize) -> Vec<String> {
    let mut map = slots();
    let mut out = Vec::new();
    for url in urls {
        if out.len() >= n {
            break;
        }
        if !url_ok(url) {
            continue;
        }
        match map.get(url) {
            Some(Slot::Pending | Slot::Failed) => continue,
            Some(Slot::Ready { path, .. }) if path.exists() => continue,
            _ => {}
        }
        map.insert(url.clone(), Slot::Pending);
        out.push(url.clone());
    }
    out
}

pub fn mark_ready(url: &str, path: PathBuf, aspect: f32) {
    slots().insert(
        url.to_string(),
        Slot::Ready {
            path,
            aspect: if aspect > 0.05 { aspect } else { 16.0 / 9.0 },
        },
    );
}

pub fn mark_failed(url: &str) {
    slots().insert(url.to_string(), Slot::Failed);
}

/// Drop Failed slots so Refresh can queue the URL again. Pending stays skipped.
pub fn clear_failed() {
    slots().retain(|_, slot| !matches!(slot, Slot::Failed));
}


pub fn release_pending(urls: &[String]) {
    let mut map = slots();
    for url in urls {
        if matches!(map.get(url), Some(Slot::Pending)) {
            map.remove(url);
        }
    }
}

pub fn download(
    url: &str,
    dir: &Path,
    token: &CancellationToken,
    version: &str,
) -> Option<(PathBuf, f32)> {
    if token.is_cancelled() || !url_ok(url) {
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(2))
        .build()
        .ok()?;
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&format!("GW2BuildOptimizer/{version} news")) {
        headers.insert(reqwest::header::USER_AGENT, v);
    }
    let resp = client.get(url).headers(headers).send().ok()?;
    if !resp.status().is_success() || resp.url().scheme() != "https" {
        return None;
    }
    let bytes = resp.bytes().ok()?;
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let ext = sniff(&bytes)?;
    let (pw, ph) = pixel_size(&bytes).unwrap_or((16, 9));
    let aspect = pw as f32 / ph as f32;
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("{}.{ext}", file_stem(url)));
    let tmp = path.with_extension(format!("{ext}.tmp"));
    std::fs::write(&tmp, &bytes).ok()?;
    std::fs::rename(&tmp, &path).ok()?;
    Some((path, aspect))
}

pub fn pixel_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() >= 24
        && bytes[0] == 0x89
        && bytes[1] == 0x50
        && bytes[2] == 0x4E
        && bytes[3] == 0x47
    {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return (w > 0 && h > 0).then_some((w, h));
    }
    jpeg_size(bytes)
}

fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 8 < bytes.len() {
        if bytes[i] != 0xFF {
            return None;
        }
        while i < bytes.len() && bytes[i] == 0xFF {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let marker = bytes[i];
        i += 1;
        if marker == 0xD9 || marker == 0xDA {
            return None;
        }
        if i + 2 > bytes.len() {
            return None;
        }
        let len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        if len < 2 || i + len > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF
        ) {
            if len < 7 {
                return None;
            }
            let h = u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]) as u32;
            let w = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            return (w > 0 && h > 0).then_some((w, h));
        }
        i += len;
    }
    None
}

/// Width / height. 16:9 while the file is still in flight.
pub fn aspect(url: &str) -> f32 {
    match slots().get(url) {
        Some(Slot::Ready { aspect, .. }) => *aspect,
        _ => 16.0 / 9.0,
    }
}

/// Load a cached still on the render thread. None while the file is in flight.
pub fn texture(url: &str) -> Option<TextureId> {
    if !url_ok(url) {
        return None;
    }
    let id = cache_id(url);
    let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nexus::texture::get_texture(&id)
    }))
    .ok()
    .flatten();
    if let Some(tex) = loaded {
        return Some(tex.id());
    }
    let path = {
        let map = slots();
        match map.get(url) {
            Some(Slot::Ready { path, .. }) if path.exists() => Some(path.clone()),
            _ => None,
        }
    }?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        nexus::texture::get_texture_or_create_from_file(&id, &path)
    }))
    .ok()
    .flatten()
    .map(|tex| tex.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_public_hosts() {
        assert!(url_ok("https://i.ytimg.com/vi/x/hqdefault.jpg"));
        assert!(!url_ok("http://i.ytimg.com/vi/x/hqdefault.jpg"));
        assert!(!url_ok("https://localhost/x.jpg"));
        assert!(!url_ok("https://127.0.0.1/x.jpg"));
        assert!(!url_ok("javascript:alert(1)"));
    }

    #[test]
    fn refresh_clears_failed_so_take_batch_retries() {
        let url = "https://i.ytimg.com/vi/retry-failed-still/hqdefault.jpg";
        mark_failed(url);
        assert!(
            take_batch(&[url.to_string()], 1).is_empty(),
            "Failed must skip within one wave"
        );
        clear_failed();
        assert_eq!(take_batch(&[url.to_string()], 1), vec![url.to_string()]);
        release_pending(&[url.to_string()]);
    }


    #[test]
    fn sniff_jpeg_and_png_magic() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(sniff(&[0x89, 0x50, 0x4E, 0x47, 0x0D]), Some("png"));
        assert_eq!(sniff(b"<html>"), None);
        assert_eq!(sniff(&[]), None);
    }

    #[test]
    fn png_ihdr_size() {
        let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&[0, 0, 0, 13]);
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&320u32.to_be_bytes());
        png.extend_from_slice(&180u32.to_be_bytes());
        assert_eq!(pixel_size(&png), Some((320, 180)));
    }
}
