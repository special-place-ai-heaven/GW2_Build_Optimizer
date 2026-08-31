//! radio-browser.info directory client — blocking reqwest, pool-name-first
//! mirror walk, descriptive User-Agent, `hidebroken=true` always.
//!
//! Server discovery (verified against the API docs at
//! <https://de1.api.radio-browser.info/> on 2026-08-31): the project
//! recommends a client-side DNS lookup of `all.api.radio-browser.info`. That
//! pool name serves the `*.api.radio-browser.info` wildcard certificate, so
//! `https://all.api.radio-browser.info` is directly usable over TLS — the OS
//! resolver picks a live mirror and no reverse-DNS dance is needed. The walk
//! therefore tries the pool name first and only then the static fallback
//! mirrors, in rotating order. The pool membership shifts over time (on the
//! verification date `/json/servers` listed only `de1`), which is exactly why
//! the pool name leads and no single mirror is load-bearing.
//!
//! Stations are filtered client-side: `lastcheckok != 1` (failed the daily
//! connectivity check) and HLS streams (`hls == 1`, plus a defensive `.m3u8`
//! sniff on the stream URL — the player speaks icecast, not HLS). Parsing is
//! separated from fetching so tests run on JSON fixtures with zero sockets.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use super::RbStation;

/// Round-robin pool name for the live mirror set; DNS resolution of this name
/// IS the recommended server discovery, delegated to the OS resolver.
const POOL: &str = "https://all.api.radio-browser.info";

/// Long-lived first-party mirrors, walked in rotating order only when the
/// pool name itself fails (DNS outage, stale resolver, dead pool member).
const FALLBACK_MIRRORS: [&str; 3] = [
    "https://de1.api.radio-browser.info",
    "https://de2.api.radio-browser.info",
    "https://fi1.api.radio-browser.info",
];

/// Descriptive User-Agent, required by the radio-browser.info usage policy.
const USER_AGENT: &str = concat!("GW2BuildOptimizer/", env!("CARGO_PKG_VERSION"));

/// Blanket per-request timeout: bounds connect AND body per mirror attempt,
/// so one dead host costs at most this before the walk advances.
const TIMEOUT: Duration = Duration::from_secs(6);

/// Body cap for directory responses. A search page is well under 1 MiB; the
/// cap only exists so a hostile mirror cannot stream unbounded bytes into
/// the game process.
const MAX_BODY_BYTES: u64 = 4 * 1024 * 1024;

/// Search stations by free-text name, most-voted first.
///
/// May return fewer than `limit` rows: the API has no HLS filter parameter,
/// so HLS entries are dropped here after the fetch.
pub fn search_by_name(query: &str, limit: usize) -> Result<Vec<RbStation>, String> {
    search(&format!("name={}", url_encode(query)), limit)
}

/// Search stations by radio-browser tag (genre chip), most-voted first.
/// An empty tag means "top stations": the `tag=` param is omitted entirely so
/// the API returns the unfiltered vote-ordered list (a literal empty `tag=`
/// matches nothing).
pub fn search_by_tag(tag: &str, limit: usize) -> Result<Vec<RbStation>, String> {
    if tag.is_empty() {
        return search("", limit);
    }
    search(&format!("tag={}", url_encode(tag)), limit)
}

/// Fire-and-forget click ping (`/json/url/{uuid}`) on play start, as the
/// radio-browser usage policy asks (counted once per IP+station per day).
/// Best-effort by contract: the response body and every error are dropped on
/// the floor — the caller already runs this on its own thread and playback
/// has moved on.
pub fn click(stationuuid: &str) {
    if stationuuid.is_empty() {
        return; // nothing to count — e.g. a hand-entered station without a uuid
    }
    let _ = get_from_any_mirror(&format!("/json/url/{}", url_encode(stationuuid)));
}

/// Parse a `/json/stations/search` body and drop unplayable rows:
/// `lastcheckok != 1` (failed the daily connectivity check), `hls == 1`, and
/// — defensively — stations whose stream URL still points at an `.m3u8`
/// playlist despite `hls == 0` (the directory flag lags reality).
pub fn parse_stations(body: &str) -> Result<Vec<RbStation>, String> {
    let stations: Vec<RbStation> = serde_json::from_str(body)
        .map_err(|e| format!("radio-browser response did not parse: {e}"))?;
    Ok(stations
        .into_iter()
        .filter(|s| s.lastcheckok == 1 && s.hls == 0 && !is_hls_url(s.stream_url()))
        .collect())
}

/// Shared `/json/stations/search` GET: sort order comes from the API
/// (`order=votes&reverse=true`), never re-sorted here.
fn search(param: &str, limit: usize) -> Result<Vec<RbStation>, String> {
    if limit == 0 {
        // The API treats limit=0 as "no limit" (default 100000) — asking for
        // nothing must not download everything.
        return Ok(Vec::new());
    }
    let sep = if param.is_empty() { "" } else { "&" };
    let path = format!(
        "/json/stations/search?{param}{sep}limit={limit}&hidebroken=true&order=votes&reverse=true"
    );
    let body = get_from_any_mirror(&path)?;
    parse_stations(&body)
}

/// `.m3u8` sniff on the path portion of a URL, case-insensitive. Query and
/// fragment are trimmed first so `playlist.m3u8?token=x` is still caught —
/// and `stream.mp3?ext=.m3u8` is not.
fn is_hls_url(url: &str) -> bool {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    url[..end].to_ascii_lowercase().ends_with(".m3u8")
}

/// GET `path` from the pool name first, then every static fallback once, in
/// rotating order. The last error wins when every host fails.
fn get_from_any_mirror(path: &str) -> Result<String, String> {
    static MIRROR_CURSOR: AtomicUsize = AtomicUsize::new(0);
    let client = shared_client()?;
    let start = MIRROR_CURSOR.fetch_add(1, Ordering::Relaxed);
    let mut last_err = String::from("no mirrors configured");
    for attempt in 0..=FALLBACK_MIRRORS.len() {
        let base = mirror_for_attempt(start, attempt);
        match fetch(&client, &format!("{base}{path}")) {
            Ok(body) => return Ok(body),
            Err(e) => last_err = e,
        }
    }
    Err(format!(
        "all radio-browser mirrors failed; last: {last_err}"
    ))
}

/// Attempt 0 is always the DNS pool name; attempts `1..=N` cover each static
/// fallback exactly once, starting at a per-call rotated offset so a dead
/// mirror is not retried first on every walk.
fn mirror_for_attempt(start: usize, attempt: usize) -> &'static str {
    if attempt == 0 {
        POOL
    } else {
        FALLBACK_MIRRORS[(start + attempt - 1) % FALLBACK_MIRRORS.len()]
    }
}

fn fetch(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("HTTP error fetching {url}: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {} fetching {url}", status.as_u16()));
    }
    let bytes = gw2_api::transport::read_body_capped(resp, MAX_BODY_BYTES)
        .map_err(|e| format!("error reading {url}: {e}"))?;
    String::from_utf8(bytes).map_err(|_| format!("UTF-8 error reading {url}"))
}

/// Process-wide directory HTTP client (TLS state + connection pool built
/// once, cheaply cloned per call). A builder failure is cached and reported
/// as an error instead of panicking inside the game process.
fn shared_client() -> Result<reqwest::blocking::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(TIMEOUT)
                .build()
                .map_err(|e| format!("radio directory HTTP client init failed: {e}"))
        })
        .clone()
}

/// Minimal percent-encoding for query values and path segments (the codebase
/// builds query strings by hand — reqwest's `.query()` re-encodes). Unreserved
/// characters pass through; everything else, including space, is `%XX`-encoded
/// byte-wise, so multibyte UTF-8 is handled without any char slicing.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 38 stations trimmed from a live 5000-station directory pull
    /// (2026-08-31). Three rows are curated because the pull (made with
    /// `hidebroken`) had no organic specimens: one flipped to
    /// `lastcheckok: 0` ("(dead)"), and two `.m3u8` rows flipped to `hls: 0`
    /// ("(sneaky HLS)"; "(SHOUTY HLS)" is additionally upper-cased to
    /// `.M3U8`). The last row is a hand-written sparse record (name + url
    /// only) exercising the serde defaults.
    const FIXTURE: &str = include_str!("directory_fixture.json");

    #[test]
    fn fixture_parses_every_row_including_sparse() {
        let raw: Vec<RbStation> = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(raw.len(), 38);
        let sparse = raw.iter().find(|s| s.name == "Bare Sparse FM").unwrap();
        assert_eq!(sparse.url, "http://sparse.example/stream");
        assert_eq!(sparse.lastcheckok, 0, "missing lastcheckok defaults to 0");
        assert_eq!(sparse.hls, 0);
        assert!(sparse.stationuuid.is_empty());
        assert!(sparse.url_resolved.is_empty());
    }

    #[test]
    fn filters_drop_broken_hls_and_disguised_hls_rows() {
        let kept = parse_stations(FIXTURE).unwrap();
        assert_eq!(kept.len(), 33, "38 rows minus the 5 unplayable ones");
        for s in &kept {
            assert_eq!(s.lastcheckok, 1);
            assert_eq!(s.hls, 0);
            assert!(!is_hls_url(s.stream_url()));
        }
        let kept_ids: Vec<&str> = kept.iter().map(|s| s.stationuuid.as_str()).collect();
        // hls == 1
        assert!(!kept_ids.contains(&"19e53e6b-0b27-414b-b05c-bcb55e474041"));
        // hls == 0 but url_resolved still ends in .m3u8
        assert!(!kept_ids.contains(&"4d97e7c1-dbea-4e16-ae51-8ebc3a643b17"));
        // same, upper-cased .M3U8
        assert!(!kept_ids.contains(&"b1628f4d-89e6-44d0-b270-638e4cbdba4f"));
        // lastcheckok == 0
        assert!(!kept_ids.contains(&"9f30d02d-ad16-4b8a-8d10-745d147f83cc"));
        // sparse row: lastcheckok defaults to 0, so it is (rightly) dropped
        assert!(!kept.iter().any(|s| s.name == "Bare Sparse FM"));
    }

    #[test]
    fn empty_url_resolved_falls_back_to_raw_url() {
        let kept = parse_stations(FIXTURE).unwrap();
        let s = kept
            .iter()
            .find(|s| s.stationuuid == "281d2206-7635-4403-b367-ac435dbc3af0")
            .expect("empty-url_resolved row must survive the filters");
        assert!(s.url_resolved.is_empty());
        assert!(!s.url.is_empty());
        assert_eq!(s.stream_url(), s.url);
    }

    #[test]
    fn m3u8_defense_is_case_insensitive_and_trims_query() {
        assert!(is_hls_url("https://x.example/live/playlist.m3u8"));
        assert!(is_hls_url("https://x.example/live/PLAYLIST.M3U8"));
        assert!(is_hls_url("https://x.example/live/playlist.m3u8?token=abc"));
        assert!(is_hls_url("https://x.example/live/playlist.m3u8#frag"));
        assert!(!is_hls_url("https://x.example/stream.mp3"));
        // the extension has to be on the path, not smuggled in the query
        assert!(!is_hls_url("https://x.example/stream.mp3?ext=.m3u8"));
        assert!(!is_hls_url("https://x.example/stream?.mp3"));
    }

    #[test]
    fn parse_reports_malformed_json_instead_of_panicking() {
        assert!(parse_stations("{not json").is_err());
        assert!(
            parse_stations("{}").is_err(),
            "object where an array is due"
        );
        assert_eq!(parse_stations("[]").unwrap().len(), 0);
    }

    #[test]
    fn url_encode_is_query_and_path_safe() {
        assert_eq!(url_encode("soma fm"), "soma%20fm");
        assert_eq!(url_encode("drum&bass=yes?"), "drum%26bass%3Dyes%3F");
        assert_eq!(url_encode("a+b/c"), "a%2Bb%2Fc");
        assert_eq!(url_encode("Radio-Ω.fm~"), "Radio-%CE%A9.fm~");
        assert_eq!(url_encode("纽约"), "%E7%BA%BD%E7%BA%A6");
    }

    #[test]
    fn mirror_walk_tries_pool_first_then_every_fallback_once() {
        for start in 0..7 {
            assert_eq!(mirror_for_attempt(start, 0), POOL);
            let mut walked: Vec<&str> = (1..=FALLBACK_MIRRORS.len())
                .map(|a| mirror_for_attempt(start, a))
                .collect();
            walked.sort_unstable();
            let mut all = FALLBACK_MIRRORS.to_vec();
            all.sort_unstable();
            assert_eq!(
                walked, all,
                "rotation must cover each fallback exactly once"
            );
        }
        // consecutive walks start the fallback leg on different mirrors
        assert_ne!(mirror_for_attempt(0, 1), mirror_for_attempt(1, 1));
    }

    /// Live directory search against the real mirror pool. Ignored by
    /// default (network); run explicitly with
    /// `cargo test -p gw2-build-optimizer -- --ignored live_directory`.
    /// Directory servers only — never a stream host.
    #[test]
    #[ignore = "hits the live radio-browser.info directory"]
    fn live_directory_search_returns_playable_stations() {
        let stations = search_by_name("soma", 5).expect("directory search should succeed");
        assert!(!stations.is_empty(), "'soma' should match SomaFM stations");
        for s in &stations {
            let url = s.stream_url();
            assert!(url.starts_with("http://") || url.starts_with("https://"));
            assert!(!is_hls_url(url));
        }
    }
}
