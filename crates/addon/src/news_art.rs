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

/// Feed hosts from `news::feed_url` plus the YouTube still CDNs that
/// `prefer_youtube_still` already rewrites to. Any other https host is a
/// still the overlay must not fetch.
const STILL_HOSTS: &[&str] = &[
    "www.guildwars2.com",
    "en-forum.guildwars2.com",
    "www.youtube.com",
    "www.guildjen.com",
    "i.ytimg.com",
    "img.youtube.com",
];

pub fn url_ok(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else {
        return false;
    };
    if u.scheme() != "https" {
        return false;
    }
    let Some(host) = u.host_str() else {
        return false;
    };
    if reserved_still_host(host) {
        return false;
    }
    STILL_HOSTS.iter().any(|ok| host.eq_ignore_ascii_case(ok))
}

/// Host-level reject shared with `radio::logos`: "localhost" or a literal IP
/// in a reserved range. Hostname DNS screening is the caller's business.
pub(crate) fn reserved_still_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip_is_reserved(ip),
        Err(_) => false,
    }
}

/// Loopback/private/link-local/unspecified/ULA - addresses no community-
/// submitted URL (news image or radio stream) has any business dialing.
pub(crate) fn ip_is_reserved(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v) => {
            v.is_loopback() || v.is_private() || v.is_link_local() || v.is_unspecified()
        }
        std::net::IpAddr::V6(v) => {
            v.is_loopback() || v.is_unspecified() || ipv6_unique_local_or_link_local(v)
        }
    }
}

/// `Ipv6Addr::is_unique_local` / `is_unicast_link_local` need a newer rustc
/// than this workspace pins; the prefix checks are the same RFCs.
fn ipv6_unique_local_or_link_local(v: std::net::Ipv6Addr) -> bool {
    let o = v.octets();
    (o[0] & 0xfe) == 0xfc || (o[0] == 0xfe && (o[1] & 0xc0) == 0x80)
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
    if !resp.status().is_success() || !url_ok(resp.url().as_str()) {
        return None;
    }
    let bytes = gw2_api::transport::read_body_capped(resp, MAX_BYTES as u64).ok()?;
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
static SLOTS_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn slots_test_guard() -> std::sync::MutexGuard<'static, ()> {
    SLOTS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_feed_hosts() {
        assert!(url_ok("https://i.ytimg.com/vi/x/hqdefault.jpg"));
        assert!(url_ok("https://www.guildwars2.com/wp-content/x.jpg"));
        assert!(url_ok("https://img.youtube.com/vi/x/mqdefault.jpg"));
        assert!(!url_ok("http://i.ytimg.com/vi/x/hqdefault.jpg"));
        assert!(!url_ok("https://localhost/x.jpg"));
        assert!(!url_ok("https://127.0.0.1/x.jpg"));
        assert!(!url_ok("https://[::1]/x.jpg"));
        assert!(!url_ok("https://10.0.0.1/x.jpg"));
        assert!(!url_ok("https://192.168.1.8/x.jpg"));
        assert!(!url_ok("https://172.16.0.1/x.jpg"));
        assert!(!url_ok("https://169.254.169.254/latest/meta-data"));
        assert!(!url_ok("https://[fe80::1]/x.jpg"));
        assert!(!url_ok("https://[fd00::1]/x.jpg"));
        assert!(!url_ok("https://evil.example/x.jpg"));
        assert!(!url_ok("javascript:alert(1)"));
    }

    #[test]
    fn refresh_clears_failed_so_take_batch_retries() {
        let _lock = slots_test_guard();
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
    fn release_pending_retries_stuck_pending_keeps_failed() {
        let _lock = slots_test_guard();
        let stuck = "https://i.ytimg.com/vi/release-stuck-pending/hqdefault.jpg";
        let failed = "https://i.ytimg.com/vi/release-keeps-failed/hqdefault.jpg";
        assert_eq!(take_batch(&[stuck.to_string()], 1), vec![stuck.to_string()]);
        mark_failed(failed);
        release_pending(&[stuck.to_string(), failed.to_string()]);
        assert_eq!(
            take_batch(&[stuck.to_string()], 1),
            vec![stuck.to_string()],
            "stuck Pending must be retryable after release"
        );
        assert!(
            take_batch(&[failed.to_string()], 1).is_empty(),
            "Failed must survive release_pending"
        );
        release_pending(&[stuck.to_string()]);
        clear_failed();
    }

    #[test]
    fn still_over_cap_is_rejected() {
        let err = gw2_api::transport::read_body_capped(
            std::io::Cursor::new(vec![0u8; MAX_BYTES + 1]),
            MAX_BYTES as u64,
        )
        .expect_err("over-cap must fail closed");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
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
