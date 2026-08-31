//! radio-browser.info directory client — blocking reqwest, DNS-discovered
//! mirror walk, descriptive User-Agent, `hidebroken=true` always.
//!
//! Contract (filled by the directory workstream):
//! - Discover mirrors via DNS lookup of all.api.radio-browser.info (shuffle,
//!   walk on failure); never hardcode one mirror.
//! - Filter out `hls == 1` and `lastcheckok != 1` stations.
//! - Parsing is separated from fetching so tests run on JSON fixtures with
//!   zero sockets.

use super::RbStation;

/// Search stations by free-text name, most-voted first.
pub fn search_by_name(query: &str, limit: usize) -> Result<Vec<RbStation>, String> {
    let _ = (query, limit);
    Err("directory client not built yet".into()) // stub: directory workstream
}

/// Search stations by radio-browser tag (genre chip), most-voted first.
pub fn search_by_tag(tag: &str, limit: usize) -> Result<Vec<RbStation>, String> {
    let _ = (tag, limit);
    Err("directory client not built yet".into()) // stub: directory workstream
}

/// Fire-and-forget click ping (`/json/url/{uuid}`) on play start, as the
/// radio-browser usage policy asks. Must never delay or fail playback.
pub fn click(stationuuid: &str) {
    let _ = stationuuid; // stub: directory workstream
}
