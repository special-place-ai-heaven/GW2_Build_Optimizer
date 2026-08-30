//! Community build scraper for Snowcrows, Hardstuck, and GuildJen.
//!
//! Each scraper fetches HTML pages, extracts build data via text pattern matching,
//! and returns normalized `BenchmarkBuild` records.
//!
//! All scrapers are resilient — they return partial results on failure rather than
//! panicking. HTTP errors or parse failures produce a `ScrapeResult.error` string.
//!
//! Storage: writes one JSON file per (source, profession, mode) to
//! `{addon_dir}/benchmarks/`.

use std::path::Path;

use crate::benchmark::{BenchmarkBuild, ScrapeResult};

// ─── User agent ──────────────────────────────────────────────────────────────

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) GW2BuildOptimizer/1.0 (research scraper)";

// ─── Public entry point ───────────────────────────────────────────────────────

/// Sentinel error string set on `ScrapeResult.error` when a scrape was skipped
/// because cancellation was requested before it started.
pub const CANCELLED_ERROR: &str = "cancelled";

/// One-line status for Settings: live heartbeat, down, count, or never-synced dash.
pub fn format_source_status(
    display_name: &str,
    count: usize,
    live: Option<&str>,
    error: Option<&str>,
) -> String {
    if let Some(live) = live.filter(|s| !s.is_empty()) {
        return format!("{}  {}", display_name, live);
    }
    if let Some(err) = error {
        let short: String = err.chars().take(60).collect();
        if count == 0 {
            return format!("{}  down — {}", display_name, short);
        }
        return format!("{}  {} builds (warning: {})", display_name, count, short);
    }
    if count == 0 {
        return format!("{}  —", display_name);
    }
    format!("{}  {} builds", display_name, count)
}

/// Scrape all three community sites and save results to `{addon_dir}/benchmarks/`.
///
/// `should_cancel` is consulted before each of the three sequential HTTP scrapes.
/// When it returns `true`, the remaining sources are skipped and replaced with a
/// `ScrapeResult` carrying `error = Some(CANCELLED_ERROR)` and empty builds.
/// Pass `&|| false` from contexts where cancellation is not meaningful.
///
/// Returns one `ScrapeResult` per site regardless of whether it succeeded or was
/// skipped. Never panics — errors are captured in `ScrapeResult.error`.
pub fn scrape_all(addon_dir: &Path, should_cancel: &dyn Fn() -> bool) -> Vec<ScrapeResult> {
    scrape_all_with_progress(addon_dir, should_cancel, &|_, _| {})
}

/// Same as [`scrape_all`], plus a `(source, message)` heartbeat for the UI.
pub fn scrape_all_with_progress(
    addon_dir: &Path,
    should_cancel: &dyn Fn() -> bool,
    on_progress: &dyn Fn(&str, &str),
) -> Vec<ScrapeResult> {
    // Cancel before any work
    if should_cancel() {
        on_progress("snowcrows", "cancelled");
        on_progress("hardstuck", "cancelled");
        on_progress("guildjen", "cancelled");
        return vec![
            cancelled_result("snowcrows"),
            cancelled_result("hardstuck"),
            cancelled_result("guildjen"),
        ];
    }

    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("Failed to build HTTP client: {}", e);
            on_progress("snowcrows", &format!("down: {}", msg));
            on_progress("hardstuck", &format!("down: {}", msg));
            on_progress("guildjen", &format!("down: {}", msg));
            return vec![
                ScrapeResult {
                    source: "snowcrows".into(),
                    builds: vec![],
                    error: Some(msg.clone()),
                },
                ScrapeResult {
                    source: "hardstuck".into(),
                    builds: vec![],
                    error: Some(msg.clone()),
                },
                ScrapeResult {
                    source: "guildjen".into(),
                    builds: vec![],
                    error: Some(msg),
                },
            ];
        }
    };

    let benchmarks_dir = addon_dir.join("benchmarks");
    if let Err(e) = std::fs::create_dir_all(&benchmarks_dir) {
        let msg = format!("Cannot create benchmarks dir: {}", e);
        on_progress("snowcrows", &format!("down: {}", msg));
        on_progress("hardstuck", &format!("down: {}", msg));
        on_progress("guildjen", &format!("down: {}", msg));
        return vec![
            ScrapeResult {
                source: "snowcrows".into(),
                builds: vec![],
                error: Some(msg.clone()),
            },
            ScrapeResult {
                source: "hardstuck".into(),
                builds: vec![],
                error: Some(msg.clone()),
            },
            ScrapeResult {
                source: "guildjen".into(),
                builds: vec![],
                error: Some(msg),
            },
        ];
    }

    let today = today_string();

    // Scrape #1: Snowcrows. Cancellation re-checked here so a pulse during
    // setup above still aborts before any network I/O.
    if should_cancel() {
        on_progress("snowcrows", "cancelled");
        on_progress("hardstuck", "cancelled");
        on_progress("guildjen", "cancelled");
        return vec![
            cancelled_result("snowcrows"),
            cancelled_result("hardstuck"),
            cancelled_result("guildjen"),
        ];
    }
    on_progress("snowcrows", "starting");
    let sc_result = finish_source(
        "snowcrows",
        scrape_snowcrows(&client, &today, should_cancel, on_progress),
        &benchmarks_dir,
        on_progress,
    );

    // Scrape #2: Hardstuck.
    if should_cancel() {
        on_progress("hardstuck", "cancelled");
        on_progress("guildjen", "cancelled");
        return vec![
            sc_result,
            cancelled_result("hardstuck"),
            cancelled_result("guildjen"),
        ];
    }
    on_progress("hardstuck", "starting");
    let hs_result = finish_source(
        "hardstuck",
        scrape_hardstuck(&client, &today, should_cancel, on_progress),
        &benchmarks_dir,
        on_progress,
    );

    // Scrape #3: GuildJen.
    if should_cancel() {
        on_progress("guildjen", "cancelled");
        return vec![sc_result, hs_result, cancelled_result("guildjen")];
    }
    on_progress("guildjen", "starting");
    let gj_result = finish_source(
        "guildjen",
        scrape_guildjen(&client, &today, should_cancel, on_progress),
        &benchmarks_dir,
        on_progress,
    );

    vec![sc_result, hs_result, gj_result]
}

fn finish_source(
    source: &str,
    result: Result<(Vec<BenchmarkBuild>, bool), String>,
    dir: &Path,
    on_progress: &dyn Fn(&str, &str),
) -> ScrapeResult {
    match result {
        Ok((builds, cancelled)) => {
            // Cancel mid-source must not overwrite last-good on-disk groups
            // with a partial scrape. Keep the in-memory vec for this session.
            if cancelled {
                on_progress(source, "cancelled");
                return ScrapeResult {
                    source: source.into(),
                    builds,
                    error: Some(CANCELLED_ERROR.into()),
                };
            }
            if let Err(e) = save_builds(&builds, dir) {
                on_progress(source, &format!("save failed: {e}"));
                return ScrapeResult {
                    source: source.into(),
                    builds,
                    error: Some(e),
                };
            }
            on_progress(source, &format!("done {}", builds.len()));
            ScrapeResult {
                source: source.into(),
                builds,
                error: None,
            }
        }
        Err(e) => {
            on_progress(source, &format!("down: {}", e));
            ScrapeResult {
                source: source.into(),
                builds: vec![],
                error: Some(e),
            }
        }
    }
}

/// Construct a placeholder `ScrapeResult` that records cancellation for a source.
fn cancelled_result(source: &str) -> ScrapeResult {
    ScrapeResult {
        source: source.into(),
        builds: vec![],
        error: Some(CANCELLED_ERROR.into()),
    }
}

/// Load all previously saved benchmark builds from `{addon_dir}/benchmarks/`.
///
/// Entries are sorted by path before processing so two calls with the same
/// disk contents return Vecs in identical order. `fs::read_dir` order is
/// OS-defined (Windows is alphabetical, Linux is roughly inode order), and
/// downstream `find_best_benchmark` ties broke on raw iteration order —
/// meaning the "best" benchmark could differ between machines.
pub fn load_benchmarks(addon_dir: &Path) -> Vec<BenchmarkBuild> {
    let dir = addon_dir.join("benchmarks");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut builds = Vec::new();
    for path in paths {
        if let Ok(file) = std::fs::File::open(&path) {
            let reader = std::io::BufReader::new(file);
            if let Ok(v) = serde_json::from_reader::<_, Vec<BenchmarkBuild>>(reader) {
                builds.extend(v);
            }
        }
    }
    builds
}

// ─── Snowcrows ────────────────────────────────────────────────────────────────

/// Professions enumerated directly — Snowcrows uses server-side rendering per-profession page.
/// The top-level `/builds` page is a SPA shell with no static links.
const SC_PROFESSIONS: &[&str] = &[
    "guardian",
    "warrior",
    "engineer",
    "ranger",
    "thief",
    "elementalist",
    "mesmer",
    "necromancer",
    "revenant",
];

/// Scrape Snowcrows (PvE raid/strike meta builds).
/// Enumerates per-profession pages: https://snowcrows.com/builds/raids/{profession}
///
/// `should_cancel` is consulted before every outer (per-profession) and inner
/// (per-build) HTTP fetch. On cancellation, returns whatever builds were
/// collected so far paired with `cancelled = true`.
fn scrape_snowcrows(
    client: &reqwest::blocking::Client,
    today: &str,
    should_cancel: &dyn Fn() -> bool,
    on_progress: &dyn Fn(&str, &str),
) -> Result<(Vec<BenchmarkBuild>, bool), String> {
    let mut last_html: Option<String> = None;
    let mut all_links: Vec<String> = Vec::new();

    on_progress("snowcrows", "listing builds…");
    // Collect build links from each profession's page
    for profession in SC_PROFESSIONS {
        if should_cancel() {
            return Ok((Vec::new(), true));
        }
        let prof_url = format!("https://snowcrows.com/builds/raids/{}", profession);
        on_progress("snowcrows", &format!("listing {}…", profession));
        let Ok(html) = fetch_html(client, &prof_url) else {
            continue;
        };
        last_html = Some(html.clone());
        // Build links look like: href="/builds/raids/guardian/power-dragonhunter-..."
        let links = extract_build_links(&html, "/builds/raids/", 50);
        for link in links {
            // Skip profession-index links (no build slug after the profession name)
            let parts: Vec<&str> = link.trim_matches('/').split('/').collect();
            // /builds/raids/{profession}/{slug} has 4 parts
            if parts.len() >= 4 && !parts[3].is_empty() && !parts[3].contains('?') {
                let full = format!("https://snowcrows.com{}", link);
                if !all_links.contains(&full) {
                    all_links.push(full);
                }
            }
        }
    }

    if all_links.is_empty() {
        // Distinguish "we were served a block page / wrong content" from
        // "the site genuinely listed no builds" — the former is almost always a
        // network filter (e.g. a UniFi/UDMPro content block on snowcrows.com) and
        // needs an allowlist change, not a code fix.
        if last_html
            .as_deref()
            .map(looks_like_blocked_page)
            .unwrap_or(false)
        {
            return Err(
                "Snowcrows: request was intercepted by a network block/filter page \
                 (the domain appears restricted by your gateway/firewall — allowlist \
                 snowcrows.com, e.g. on your UniFi/UDMPro content filter)"
                    .into(),
            );
        }
        return Err("Snowcrows: no build links found on any profession page".into());
    }

    let mut builds = Vec::new();
    let total = all_links.len().min(45);
    on_progress("snowcrows", &format!("0/{}", total));
    // Cap at 45 builds total (5 per profession on average across 9 professions)
    for (i, url) in all_links.into_iter().take(45).enumerate() {
        if should_cancel() {
            return Ok((builds, true));
        }
        if let Ok(b) = scrape_snowcrows_build(client, &url, today) {
            builds.push(b)
        }
        on_progress("snowcrows", &format!("{}/{}", i + 1, total));
    }
    Ok((builds, false))
}

/// Build a `BenchmarkBuild` from page HTML once the per-site fields
/// (source/profession/spec/mode/role) have been derived. The gear/trait/skill
/// extraction is identical across every source, so it lives here.
// Builder over per-site fields already derived by the caller; bundling the HTML
// and metadata into a struct would just mirror the argument list.
#[allow(clippy::too_many_arguments)]
fn benchmark_from_html(
    html: &str,
    url: &str,
    today: &str,
    source: &str,
    profession: String,
    spec_name: String,
    mode: &str,
    role: &str,
) -> BenchmarkBuild {
    BenchmarkBuild {
        source: source.into(),
        profession,
        spec_name,
        mode: mode.into(),
        role: role.to_string(),
        build_code: extract_build_code(html),
        gear_prefix: extract_gear_prefix(html),
        rune: extract_rune(html),
        sigils: extract_sigils(html),
        relic: extract_relic(html),
        traits: extract_traits(html),
        skills: extract_skills(html),
        source_url: url.to_string(),
        scraped_at: today.to_string(),
        notes: String::new(),
    }
}

fn scrape_snowcrows_build(
    client: &reqwest::blocking::Client,
    url: &str,
    today: &str,
) -> Result<BenchmarkBuild, String> {
    let html = fetch_html(client, url)?;

    // URL structure: /builds/raids/{profession}/{slug}
    // slug like "power-dragonhunter-virtues-longbow-greatsword"
    let url_parts: Vec<&str> = url.trim_end_matches('/').split('/').collect();
    let profession = url_parts
        .get(url_parts.len().saturating_sub(2))
        .map(|s| title_case(&s.replace('-', " ")))
        .unwrap_or_default();
    let slug = url_parts.last().copied().unwrap_or("");

    // Derive spec and role from slug
    // "power-dragonhunter-virtues-longbow-greatsword" → spec=Dragonhunter, role=Power DPS
    let spec_name = extract_spec_from_slug(slug);
    let role = if slug.starts_with("condition") || slug.starts_with("condi") {
        "Condi DPS"
    } else if slug.starts_with("heal") {
        "Heal Support"
    } else if slug.starts_with("power") {
        "Power DPS"
    } else if slug.starts_with("celestial") {
        "Hybrid / Celestial"
    } else {
        "Power DPS"
    };

    Ok(benchmark_from_html(
        &html,
        url,
        today,
        "snowcrows",
        profession,
        spec_name,
        "PvE",
        role,
    ))
}

// ─── Hardstuck ────────────────────────────────────────────────────────────────

/// Hardstuck URL format: /gw2/builds/{profession}/{id-or-name-slug}/
/// The index page is Next.js — links appear as hrefs in the static HTML.
/// We enumerate per-profession pages to bypass the JS-rendered filter UI.
const HS_PROFESSIONS: &[&str] = &[
    "guardian",
    "warrior",
    "engineer",
    "ranger",
    "thief",
    "elementalist",
    "mesmer",
    "necromancer",
    "revenant",
];

/// Pin a Hardstuck listing href to `https://hardstuck.gg`.
///
/// Rejects `http://` and off-origin absolute URLs (same pin as GuildJen).
fn pin_hardstuck_href(link: &str) -> Option<String> {
    if link.starts_with("https://hardstuck.gg/") {
        Some(link.to_string())
    } else if link.starts_with("http") {
        None
    } else {
        Some(format!("https://hardstuck.gg{}", link))
    }
}

/// Scrape Hardstuck (multi-mode builds).
///
/// `should_cancel` is consulted before every outer (per-profession) and inner
/// (per-build) HTTP fetch. On cancellation, returns whatever builds were
/// collected so far paired with `cancelled = true`.
fn scrape_hardstuck(
    client: &reqwest::blocking::Client,
    today: &str,
    should_cancel: &dyn Fn() -> bool,
    on_progress: &dyn Fn(&str, &str),
) -> Result<(Vec<BenchmarkBuild>, bool), String> {
    let mut last_html: Option<String> = None;
    let mut all_links: Vec<String> = Vec::new();

    on_progress("hardstuck", "listing builds…");
    // Each profession page lists builds for that profession
    for profession in HS_PROFESSIONS {
        if should_cancel() {
            return Ok((Vec::new(), true));
        }
        let prof_url = format!("https://hardstuck.gg/gw2/builds/{}/", profession);
        on_progress("hardstuck", &format!("listing {}…", profession));
        let Ok(html) = fetch_html(client, &prof_url) else {
            continue;
        };
        last_html = Some(html.clone());
        // Build links: href="/gw2/builds/{profession}/{slug}/" with a non-empty slug
        // slug can be numeric (24929) or text (blood-harbinger)
        let links = extract_build_links(&html, &format!("/gw2/builds/{}/", profession), 40);
        for link in links {
            let parts: Vec<&str> = link.trim_matches('/').split('/').collect();
            // /gw2/builds/{profession}/{slug} = exactly 4 segments
            if parts.len() == 4 && !parts[3].is_empty() && !parts[3].contains('?') {
                // Pin to the real host: an index page (compromised, or MITM'd if
                // TLS were ever bypassed) must not steer us to arbitrary absolute
                // URLs — only https://hardstuck.gg and relative paths may be followed.
                let Some(full) = pin_hardstuck_href(&link) else {
                    continue;
                };
                if !all_links.contains(&full) {
                    all_links.push(full);
                }
            }
        }
    }

    if all_links.is_empty() {
        // Distinguish "we were served a block page / wrong content" from
        // "the site genuinely listed no builds" — the former is almost always a
        // network filter (e.g. a UniFi/UDMPro content block on hardstuck.gg) and
        // needs an allowlist change, not a code fix.
        if last_html
            .as_deref()
            .map(looks_like_blocked_page)
            .unwrap_or(false)
        {
            return Err(
                "Hardstuck: request was intercepted by a network block/filter page \
                 (the domain appears restricted by your gateway/firewall — allowlist \
                 hardstuck.gg, e.g. on your UniFi/UDMPro content filter)"
                    .into(),
            );
        }
        return Err("Hardstuck: no build links found on any profession page".into());
    }

    let mut builds = Vec::new();
    let total = all_links.len().min(45);
    on_progress("hardstuck", &format!("0/{}", total));
    for (i, url) in all_links.into_iter().take(45).enumerate() {
        if should_cancel() {
            return Ok((builds, true));
        }
        if let Ok(b) = scrape_hardstuck_build(client, &url, today) {
            builds.push(b)
        }
        on_progress("hardstuck", &format!("{}/{}", i + 1, total));
    }
    Ok((builds, false))
}

fn scrape_hardstuck_build(
    client: &reqwest::blocking::Client,
    url: &str,
    today: &str,
) -> Result<BenchmarkBuild, String> {
    let html = fetch_html(client, url)?;

    // URL: /gw2/builds/{profession}/{slug}/
    // slug like "blood-harbinger" or "24929"
    let url_parts: Vec<&str> = url.trim_end_matches('/').split('/').collect();
    let profession_raw = url_parts
        .get(url_parts.len().saturating_sub(2))
        .copied()
        .unwrap_or("");
    let slug = url_parts.last().copied().unwrap_or("");
    let profession = title_case(&profession_raw.replace('-', " "));

    // Spec name from slug (same approach as Snowcrows)
    let spec_name = extract_spec_from_slug(slug);

    // Mode: try to extract from page content — Hardstuck tags with PVP/WVW/PVE labels
    let html_lower = html.to_lowercase();
    let mode = if html_lower.contains("pvp") || html_lower.contains("player vs player") {
        "PvP"
    } else if html_lower.contains("wvw") || html_lower.contains("world vs world") {
        "WvW"
    } else {
        "PvE"
    };

    // Role from page content
    let role = if html_lower.contains("condi") || html_lower.contains("condition damage") {
        "Condi DPS"
    } else if html_lower.contains("support")
        || html_lower.contains("healer")
        || html_lower.contains("heal")
    {
        "Heal Support"
    } else if html_lower.contains("bruiser") || html_lower.contains("sustain") {
        "Sustain / Bruiser"
    } else if html_lower.contains("roamer") || html_lower.contains("roaming") {
        "WvW Roaming"
    } else {
        "Power DPS"
    };

    Ok(benchmark_from_html(
        &html,
        url,
        today,
        "hardstuck",
        profession,
        spec_name,
        mode,
        role,
    ))
}

// ─── GuildJen ─────────────────────────────────────────────────────────────────

/// Scrape GuildJen (WvW/PvP builds).
/// Index: https://guildjen.com/
///
/// `should_cancel` is consulted before every outer (per-index-page) and inner
/// (per-build) HTTP fetch. On cancellation, returns whatever builds were
/// collected so far paired with `cancelled = true`.
fn scrape_guildjen(
    client: &reqwest::blocking::Client,
    today: &str,
    should_cancel: &dyn Fn() -> bool,
    on_progress: &dyn Fn(&str, &str),
) -> Result<(Vec<BenchmarkBuild>, bool), String> {
    // GuildJen's main build index pages
    let index_urls = [
        "https://guildjen.com/wvw-builds/",
        "https://guildjen.com/pvp-builds/",
    ];

    let mut builds = Vec::new();
    let mut any_success = false;

    for index_url in &index_urls {
        if should_cancel() {
            return Ok((builds, true));
        }
        let mode = if index_url.contains("wvw") {
            "WvW"
        } else {
            "PvP"
        };
        on_progress("guildjen", &format!("listing {}…", mode));
        let Ok(html) = fetch_html(client, index_url) else {
            continue;
        };

        let links = extract_build_links(&html, "guildjen.com/", 40);
        any_success = true;
        let cap = links.len().min(15);

        for (i, link) in links.into_iter().take(15).enumerate() {
            if should_cancel() {
                return Ok((builds, true));
            }
            // Pin to the real host: an index page (compromised, or MITM'd if
            // TLS were ever bypassed) must not steer us to arbitrary absolute
            // URLs — only relative paths on guildjen.com may be followed.
            let url = if link.starts_with("https://guildjen.com/") {
                link
            } else if link.starts_with("http") {
                continue;
            } else {
                format!("https://guildjen.com{}", link)
            };
            if let Ok(mut b) = scrape_guildjen_build(client, &url, today) {
                b.mode = mode.to_string();
                builds.push(b);
            }
            on_progress("guildjen", &format!("{} {}/{}", mode, i + 1, cap));
        }
    }

    if !any_success {
        return Err("GuildJen: failed to fetch any index pages".to_string());
    }
    Ok((builds, false))
}

fn scrape_guildjen_build(
    client: &reqwest::blocking::Client,
    url: &str,
    today: &str,
) -> Result<BenchmarkBuild, String> {
    let html = fetch_html(client, url)?;

    let (profession, spec_name) = parse_profession_spec_from_url(url);
    let mode = if url.contains("wvw") {
        "WvW"
    } else if url.contains("pvp") {
        "PvP"
    } else {
        "WvW"
    };

    let role = if html.to_lowercase().contains("roam") {
        "WvW Roaming"
    } else if html.to_lowercase().contains("zerg") || html.to_lowercase().contains("squad") {
        "WvW Zerg DPS"
    } else if html.to_lowercase().contains("support") || html.to_lowercase().contains("heal") {
        "WvW Zerg Support"
    } else {
        "WvW Roaming"
    };

    Ok(benchmark_from_html(
        &html, url, today, "guildjen", profession, spec_name, mode, role,
    ))
}

// ─── HTML extraction helpers ──────────────────────────────────────────────────

fn fetch_html(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("HTTP error fetching {}: {}", url, e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!(
            "HTTP {} fetching {} (expected 2xx)",
            status.as_u16(),
            url
        ));
    }

    // Cap the body: a hostile endpoint must not stream unbounded bytes into
    // the game process. Build pages are well under 1 MiB.
    let bytes = gw2_api::transport::read_body_capped(resp, 2 * 1024 * 1024)
        .map_err(|e| format!("error reading {}: {}", url, e))?;
    String::from_utf8(bytes).map_err(|_| format!("UTF-8 error reading {}", url))
}

/// Heuristic: does this look like a network/security interstitial (block page,
/// captive portal, WAF challenge) rather than the real site content?
///
/// These pages return HTTP 200 with a body that mentions being blocked, so a
/// status check alone won't catch them. We only flag high-confidence markers to
/// avoid false positives on legitimate build pages.
fn looks_like_blocked_page(html: &str) -> bool {
    let lower = html.to_lowercase();
    // "blocked" + a gateway/filter vendor or "restricted"/"administrator" cue.
    let mentions_blocked = lower.contains("blocked because")
        || lower.contains("access denied")
        || lower.contains("this site is blocked")
        || lower.contains("domain is restricted");
    let mentions_gateway = lower.contains("ubiquiti")
        || lower.contains("unifi")
        || lower.contains("contact your administrator")
        || lower.contains("content filter");
    (mentions_blocked && mentions_gateway) || lower.contains("domain is restricted")
}

/// Extract hrefs containing `needle` from anchor tags in HTML.
fn extract_build_links(html: &str, needle: &str, max: usize) -> Vec<String> {
    let mut links = Vec::new();
    let mut pos = 0;
    while let Some(href_start) = html[pos..].find("href=\"") {
        let abs = pos + href_start + 6;
        if let Some(end) = html[abs..].find('"') {
            let href = &html[abs..abs + end];
            if href.contains(needle) && !links.contains(&href.to_string()) {
                links.push(href.to_string());
                if links.len() >= max {
                    break;
                }
            }
            pos = abs + end + 1;
        } else {
            break;
        }
    }
    links
}

/// Parse profession and spec name from a URL path.
/// E.g. /builds/guardian/firebrand → ("Guardian", "Firebrand")
fn parse_profession_spec_from_url(url: &str) -> (String, String) {
    // Remove query strings and fragments
    let url = url.split('?').next().unwrap_or(url);
    let url = url.split('#').next().unwrap_or(url);
    let parts: Vec<&str> = url
        .trim_end_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();

    // Known profession names for matching
    let professions = [
        "guardian",
        "warrior",
        "engineer",
        "ranger",
        "thief",
        "elementalist",
        "mesmer",
        "necromancer",
        "revenant",
    ];

    let mut profession = String::new();
    let mut spec = String::new();

    for (i, part) in parts.iter().enumerate() {
        let lower = part.to_lowercase();
        if professions.iter().any(|&p| lower.contains(p)) {
            profession = title_case(&lower.replace('-', " "));
            if let Some(next) = parts.get(i + 1) {
                spec = title_case(&next.replace('-', " "));
            }
            break;
        }
    }

    // If not found via profession keyword, use last two meaningful path segments
    if profession.is_empty() && parts.len() >= 2 {
        let last_idx = parts.len() - 1;
        spec = title_case(&parts[last_idx].replace('-', " "));
        if last_idx > 0 {
            profession = title_case(&parts[last_idx - 1].replace('-', " "));
        }
    }

    (profession, spec)
}

/// Known GW2 elite spec names for slug extraction.
const KNOWN_SPECS: &[&str] = &[
    "firebrand",
    "willbender",
    "dragonhunter",
    "berserker",
    "spellbreaker",
    "bladesworn",
    "scrapper",
    "mechanist",
    "holosmith",
    "soulbeast",
    "untamed",
    "druid",
    "daredevil",
    "specter",
    "deadeye",
    "weaver",
    "tempest",
    "catalyst",
    "chronomancer",
    "virtuoso",
    "mirage",
    "scourge",
    "harbinger",
    "reaper",
    "renegade",
    "vindicator",
    "herald",
    "luminary",
];

/// Extract elite spec name from a Snowcrows URL slug.
/// e.g. "power-dragonhunter-virtues-longbow-greatsword" → "Dragonhunter"
fn extract_spec_from_slug(slug: &str) -> String {
    let lower = slug.to_lowercase();
    for spec in KNOWN_SPECS {
        if lower.contains(spec) {
            return title_case(spec);
        }
    }
    // Fallback: take the second hyphen-segment (after the role prefix)
    let parts: Vec<&str> = slug.split('-').collect();
    if parts.len() >= 2 {
        title_case(parts[1])
    } else {
        title_case(slug)
    }
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract GW2 build template code (e.g. "[&...]").
fn extract_build_code(html: &str) -> Option<String> {
    // Build codes start with [& and end with ]
    let start = html.find("[&")?;
    let end = html[start..].find(']')?;
    let code = &html[start..start + end + 1];
    // Basic sanity: build codes are base64 and typically 44-60 chars
    if code.len() >= 10 {
        Some(code.to_string())
    } else {
        None
    }
}

/// Space-padded alnum words — same boundary idea as `prefix_named_in_text`.
fn padded_alnum_words(text: &str) -> String {
    format!(
        " {} ",
        text.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    ' '
                }
            })
            .collect::<String>()
    )
}

/// Extract gear stat prefix from HTML text.
///
/// Case-insensitive — scraped sites vary in casing (e.g. "viper's", "VIPER'S"),
/// and a case-sensitive `contains` previously silently dropped those.
fn extract_gear_prefix(html: &str) -> String {
    // Common gear prefixes — match in order of specificity
    let prefixes = [
        "Trailblazer's",
        "Plaguedoctor's",
        "Ritualist's",
        "Apothecary's",
        "Diviner's",
        "Celestial",
        "Minstrel's",
        "Harrier's",
        "Berserker's",
        "Viper's",
        "Sinister",
        "Grieving",
        "Marauder",
        "Dragon's",
        "Valkyrie",
        "Trailblazer",
        "Assassin's",
        "Knight's",
        "Nomad's",
        "Soldier's",
        "Cavalier's",
        "Dire",
        "Magi's",
        "Cleric's",
    ];
    let hay = padded_alnum_words(html);
    for p in &prefixes {
        let stem = p.trim_end_matches("'s").to_ascii_lowercase();
        if hay.contains(&format!(" {stem} ")) || hay.contains(&format!(" {stem}s ")) {
            return p.to_string();
        }
    }
    String::new()
}

/// Longest prefix of `s` of at most `max` bytes that ends on a char
/// boundary. Slicing mid-UTF-8 panics, and build-site HTML is
/// attacker-influenced content.
fn take_chars_window(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut bound = max;
    while !s.is_char_boundary(bound) {
        bound -= 1;
    }
    &s[..bound]
}

/// Extract rune name from HTML.
fn extract_rune(html: &str) -> String {
    // Rune names follow "Rune of" or "Superior Rune"
    for marker in &["Rune of the ", "Rune of ", "Superior Rune"] {
        if let Some(pos) = html.find(marker) {
            let after = &html[pos..];
            // Take up to 60 bytes and trim at next HTML tag or quote
            let raw = take_chars_window(after, 60);
            let end = raw.find(['<', '"', '\n']).unwrap_or(raw.len());
            let name = raw[..end].trim().to_string();
            if name.len() > 5 {
                return name;
            }
        }
    }
    String::new()
}

/// Extract sigil names from HTML.
fn extract_sigils(html: &str) -> Vec<String> {
    let mut sigils = Vec::new();
    for marker in &["Superior Sigil of ", "Sigil of "] {
        let mut pos = 0;
        while let Some(idx) = html[pos..].find(marker) {
            let abs = pos + idx;
            let after = take_chars_window(&html[abs..], 60);
            let end = after.find(['<', '"', '\n']).unwrap_or(after.len());
            let name = after[..end].trim().to_string();
            if name.len() > 5 && !sigils.contains(&name) {
                sigils.push(name);
            }
            pos = abs + marker.len();
            if sigils.len() >= 4 {
                break;
            }
        }
    }
    sigils
}

/// Extract relic name from HTML.
fn extract_relic(html: &str) -> String {
    for marker in &["Relic of ", "Superior Relic"] {
        if let Some(pos) = html.find(marker) {
            let after = &html[pos..];
            let raw = take_chars_window(after, 60);
            let end = raw.find(['<', '"', '\n']).unwrap_or(raw.len());
            let name = raw[..end].trim().to_string();
            if name.len() > 5 {
                return name;
            }
        }
    }
    String::new()
}

/// Extract trait names from HTML (look for known specialization names as section headers).
fn extract_traits(html: &str) -> Vec<String> {
    let known_specs = [
        "Firebrand",
        "Willbender",
        "Dragonhunter",
        "Berserker",
        "Spellbreaker",
        "Bladesworn",
        "Scrapper",
        "Mechanist",
        "Holosmith",
        "Soulbeast",
        "Untamed",
        "Druid",
        "Daredevil",
        "Specter",
        "Deadeye",
        "Weaver",
        "Tempest",
        "Catalyst",
        "Chronomancer",
        "Virtuoso",
        "Mirage",
        "Scourge",
        "Harbinger",
        "Reaper",
        "Renegade",
        "Vindicator",
        "Herald",
        // Core specs
        "Guardian",
        "Warrior",
        "Engineer",
        "Ranger",
        "Thief",
        "Elementalist",
        "Mesmer",
        "Necromancer",
        "Revenant",
    ];
    let mut traits = Vec::new();
    for spec in &known_specs {
        if html.contains(spec) && !traits.contains(&spec.to_string()) {
            traits.push(spec.to_string());
            if traits.len() >= 3 {
                break;
            }
        }
    }
    traits
}

/// Extract skill names from HTML.
fn extract_skills(html: &str) -> Vec<String> {
    // Common skill markers on build sites
    let markers = ["Heal:", "Utility:", "Elite:", "utility-skill", "heal-skill"];
    let mut skills = Vec::new();
    for marker in &markers {
        if let Some(pos) = html.find(marker) {
            let after = &html[pos + marker.len()..];
            // Truncate on a char boundary — slicing mid-UTF-8 panics, and
            // build-site HTML is attacker-influenced content.
            let bound = after
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= 80)
                .last()
                .unwrap_or(0);
            let raw = &after[..bound];
            // Strip HTML tags
            let text = strip_tags(raw);
            let name = text.trim().trim_matches(':').trim().to_string();
            if name.len() > 3 && name.len() < 50 {
                skills.push(name);
            }
        }
        if skills.len() >= 5 {
            break;
        }
    }
    skills
}

fn strip_tags(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
}
fn save_builds(builds: &[BenchmarkBuild], dir: &Path) -> Result<(), String> {
    if builds.is_empty() {
        return Ok(());
    }
    // Group by (source, profession, mode)
    let mut groups: std::collections::HashMap<String, Vec<&BenchmarkBuild>> =
        std::collections::HashMap::new();
    for b in builds {
        let key = format!(
            "{}_{}_{}.json",
            sanitize_filename_component(&b.source),
            sanitize_filename_component(&b.profession.to_lowercase().replace(' ', "_")),
            sanitize_filename_component(&b.mode.to_lowercase())
        );
        groups.entry(key).or_default().push(b);
    }
    for (filename, group) in &groups {
        // Filename components can carry scraped, attacker-influenced text
        // (profession parsed from a URL path). Windows treats '\\' as a
        // separator, so whitelist instead of trusting the URL splitter.
        let path = dir.join(sanitize_filename_component(filename));
        let json = serde_json::to_string_pretty(group).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Reduce a string to a safe single path component: keep alphanumerics,
/// '-', '_', '.', ' ' — everything else (including '\\'/''/') becomes '_'.
/// Mirrors the whitelist in crates/core/src/storage.rs.
fn sanitize_filename_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    // A component of only dots (e.g. "..") must not survive.
    if cleaned.chars().all(|c| c == '.') {
        "_".repeat(cleaned.len())
    } else {
        cleaned
    }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

fn build_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        // Certificates must validate (Schannel trust store). The old
        // danger_accept_invalid_certs(true) here let any MITM poison the
        // benchmark cache that feeds optimizer comparisons and LLM prompts.
        .build()
}

fn today_string() -> String {
    // Use chrono so the date is correct. The previous manual approximation
    // (1970 + days/365, day_of_year/30 + 1) ignored leap years and assumed
    // 30-day months — the rendered date drifted up to ~14 days off the real
    // calendar date, which propagated into the scraped benchmark filenames.
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_source_status_shows_live_progress() {
        let line = format_source_status("Snowcrows (PvE)", 0, Some("12/45"), None);
        assert_eq!(line, "Snowcrows (PvE)  12/45");
    }

    #[test]
    fn format_source_status_shows_down_when_empty_and_errored() {
        let line = format_source_status(
            "GuildJen (WvW/PvP)",
            0,
            None,
            Some("HTTP 404 fetching https://guildjen.com/wvw-builds/"),
        );
        assert!(line.starts_with("GuildJen (WvW/PvP)  down — "));
        assert!(line.contains("HTTP 404"));
    }

    #[test]
    fn format_source_status_shows_count_when_done() {
        let line = format_source_status("Hardstuck", 42, None, None);
        assert_eq!(line, "Hardstuck  42 builds");
    }

    #[test]
    fn format_source_status_dash_when_never_touched() {
        let line = format_source_status("Snowcrows (PvE)", 0, None, None);
        assert_eq!(line, "Snowcrows (PvE)  —");
    }

    #[test]
    fn scrape_all_reports_cancelled_progress_when_cancelled_at_entry() {
        use std::sync::Mutex;

        let events = Mutex::new(Vec::<(String, String)>::new());
        let tmp = std::env::temp_dir().join("gw2_scraper_progress_cancel");
        let _ = scrape_all_with_progress(&tmp, &|| true, &|src, msg| {
            events
                .lock()
                .unwrap()
                .push((src.to_string(), msg.to_string()));
        });
        let ev = events.lock().unwrap();
        assert!(
            ev.iter()
                .any(|(s, m)| s == "snowcrows" && m.contains("cancelled")),
            "expected snowcrows cancelled event, got {:?}",
            *ev
        );
        assert!(
            ev.iter()
                .any(|(s, m)| s == "guildjen" && m.contains("cancelled")),
            "expected guildjen cancelled event, got {:?}",
            *ev
        );
    }

    #[test]
    fn test_extract_build_links_finds_hrefs() {
        let html =
            r#"<a href="/builds/guardian/firebrand">Firebrand</a><a href="/other">Other</a>"#;
        let links = extract_build_links(html, "/builds/", 10);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "/builds/guardian/firebrand");
    }

    #[test]
    fn pin_hardstuck_href_rejects_off_origin_http() {
        assert_eq!(pin_hardstuck_href("http://evil.example/build"), None);
        assert_eq!(
            pin_hardstuck_href("https://evil.example/gw2/builds/necromancer/x/"),
            None
        );
        assert_eq!(
            pin_hardstuck_href("http://hardstuck.gg/gw2/builds/necromancer/x/"),
            None
        );
        assert_eq!(
            pin_hardstuck_href("/gw2/builds/necromancer/blood-harbinger/").as_deref(),
            Some("https://hardstuck.gg/gw2/builds/necromancer/blood-harbinger/")
        );
        assert_eq!(
            pin_hardstuck_href("https://hardstuck.gg/gw2/builds/necromancer/blood-harbinger/")
                .as_deref(),
            Some("https://hardstuck.gg/gw2/builds/necromancer/blood-harbinger/")
        );
    }

    #[test]
    fn test_looks_like_blocked_page_detects_unifi_block() {
        // Markers taken from the observed UniFi/Ubiquiti block page.
        let html = r#"<html><head><title>Blocked</title></head><body>
            <div id="info-box"><svg id="ubiquiti-logo"></svg>
            <p>This domain is restricted. Contact your administrator for more information.</p>
            <p>blocked because the domain is restricted</p></body></html>"#;
        assert!(looks_like_blocked_page(html));
    }

    #[test]
    fn test_looks_like_blocked_page_ignores_real_build_page() {
        // A normal build page that happens to use the word "blocks" must NOT trip it.
        let html = r#"<html><body>
            <a href="/gw2/builds/guardian/firebrand">Firebrand</a>
            <p>Shield of Courage blocks the next attack. Great sustain build.</p>
            </body></html>"#;
        assert!(!looks_like_blocked_page(html));
    }

    #[test]
    fn test_looks_like_blocked_page_ignores_empty() {
        assert!(!looks_like_blocked_page(""));
        assert!(!looks_like_blocked_page(
            "<html><body>No builds here yet.</body></html>"
        ));
    }

    #[test]
    fn test_parse_profession_spec_guardian_firebrand() {
        let (prof, spec) = parse_profession_spec_from_url(
            "https://snowcrows.com/builds/guardian/firebrand/power-dps",
        );
        assert_eq!(prof, "Guardian");
        assert_eq!(spec, "Firebrand");
    }

    #[test]
    fn test_parse_profession_spec_necromancer_scourge() {
        let (prof, spec) = parse_profession_spec_from_url(
            "https://snowcrows.com/builds/necromancer/scourge/condi",
        );
        assert_eq!(prof, "Necromancer");
        assert_eq!(spec, "Scourge");
    }

    #[test]
    fn test_extract_gear_prefix_berserker() {
        let html = "Use Berserker's gear for maximum DPS.";
        assert_eq!(extract_gear_prefix(html), "Berserker's");
    }

    #[test]
    fn test_extract_gear_prefix_viper() {
        let html = "Viper's stat combo is optimal for condition builds.";
        assert_eq!(extract_gear_prefix(html), "Viper's");
    }

    #[test]
    fn test_extract_build_code() {
        let html =
            r#"Build code: [&DQYAAAAqASsATgA2ADYARgBGAEYARgAAAAAAAAAAAAAAAAAAAAAAAAA=] use it"#;
        let code = extract_build_code(html);
        assert!(code.is_some());
        assert!(code.unwrap().starts_with("[&"));
    }

    #[test]
    fn extract_gear_prefix_directly_is_not_dire() {
        assert_eq!(extract_gear_prefix("Boons apply directly to allies."), "");
        assert_eq!(
            extract_gear_prefix("Use Dire gear for condition sustain."),
            "Dire"
        );
    }

    #[test]
    fn test_extract_rune() {
        let html = "equip Superior Rune of the Scholar for best results";
        let rune = extract_rune(html);
        assert!(
            rune.contains("Scholar"),
            "rune='{}' should contain Scholar",
            rune
        );
    }

    #[test]
    fn test_strip_tags() {
        assert_eq!(strip_tags("<b>hello</b> world"), "hello world");
        assert_eq!(strip_tags("no tags here"), "no tags here");
    }

    #[test]
    fn test_title_case() {
        assert_eq!(title_case("power-dps"), "Power-dps");
        assert_eq!(title_case("guardian firebrand"), "Guardian Firebrand");
    }

    #[test]
    fn test_today_string_format() {
        let s = today_string();
        assert_eq!(s.len(), 10);
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
    }

    /// Cancellation requested before any work must short-circuit `scrape_all`
    /// without performing any HTTP I/O. The function returns one
    /// `ScrapeResult` per source, each tagged with `CANCELLED_ERROR`. The
    /// bounded time budget asserts no network call was attempted (each scrape
    /// has a 15s reqwest timeout, so three sequential failures would take many
    /// seconds; cancellation must return in milliseconds).
    #[test]
    fn scrape_all_returns_immediately_when_cancelled_at_entry() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::time::Instant;

        let cancelled = Arc::new(AtomicBool::new(true));
        let cancelled_clone = cancelled.clone();
        let predicate = move || cancelled_clone.load(Ordering::Relaxed);

        let tmp = std::env::temp_dir().join("gw2_scraper_cancel_test_entry");
        let start = Instant::now();
        let results = scrape_all(&tmp, &predicate);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 500,
            "scrape_all should return within 500ms when cancelled at entry, took {:?}",
            elapsed
        );
        assert_eq!(results.len(), 3, "must still emit one result per source");
        for r in &results {
            assert_eq!(
                r.error.as_deref(),
                Some(CANCELLED_ERROR),
                "{} not flagged cancelled",
                r.source
            );
            assert!(
                r.builds.is_empty(),
                "{} should have no builds when cancelled",
                r.source
            );
        }
        // Sources stay in canonical order so the UI can rely on positional access.
        assert_eq!(results[0].source, "snowcrows");
        assert_eq!(results[1].source, "hardstuck");
        assert_eq!(results[2].source, "guildjen");

        // Defensive: keep `cancelled` alive across the call so the closure's
        // Arc clone observes a live AtomicBool (clippy::redundant_clone hint
        // would otherwise tempt removal).
        assert!(cancelled.load(Ordering::Relaxed));
    }

    /// A predicate that flips from false to true after the first invocation
    /// simulates the user clicking cancel after `scrape_all` has begun but
    /// before the first network call. Because the entry-point check fires
    /// first (sees false), we then proceed past `build_client` / mkdir, hit
    /// the pre-snowcrows check (still false on second call? no — it flips on
    /// every call, so call 2 returns true), and bail out before scraping.
    /// The test asserts bounded latency and that all three results are
    /// flagged cancelled.
    #[test]
    fn scrape_all_aborts_between_phases_when_predicate_flips() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Instant;

        // Predicate returns false on the first call (entry check), true on
        // every subsequent call (pre-snowcrows and beyond).
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let predicate = move || {
            let n = calls_clone.fetch_add(1, Ordering::Relaxed);
            n > 0
        };

        let tmp = std::env::temp_dir().join("gw2_scraper_cancel_test_phases");
        let start = Instant::now();
        let results = scrape_all(&tmp, &predicate);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 1000,
            "scrape_all should bail out within 1s when cancel pulses after entry, took {:?}",
            elapsed
        );
        assert_eq!(results.len(), 3);
        for r in &results {
            assert_eq!(
                r.error.as_deref(),
                Some(CANCELLED_ERROR),
                "{} expected cancelled, got {:?}",
                r.source,
                r.error
            );
        }
        // Predicate fired at least twice (entry + pre-snowcrows).
        assert!(calls.load(Ordering::Relaxed) >= 2);
    }

    /// Sanity: passing a never-cancel predicate must not change the
    /// public observable result shape — three entries in canonical order.
    /// This test does not perform network I/O reliably across CI, so we
    /// only check the no-cancel branch returns the right vector length
    /// and source ordering when scraping is allowed to attempt and fail
    /// (errors are captured, not panicked). The test is bounded by the
    /// reqwest 15s × 3 timeout cap; in practice DNS for snowcrows.com
    /// resolves and the test completes quickly even offline.
    #[test]
    #[ignore = "performs real network I/O; run with `cargo test -- --ignored`"]
    fn scrape_all_no_cancel_returns_three_results() {
        let tmp = std::env::temp_dir().join("gw2_scraper_no_cancel_test");
        let results = scrape_all(&tmp, &|| false);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].source, "snowcrows");
        assert_eq!(results[1].source, "hardstuck");
        assert_eq!(results[2].source, "guildjen");
        for r in &results {
            assert_ne!(
                r.error.as_deref(),
                Some(CANCELLED_ERROR),
                "{} unexpectedly flagged cancelled with never-cancel predicate",
                r.source
            );
        }
    }

    /// Inner-loop cancellation: each per-source scraper must consult
    /// `should_cancel` BEFORE every outer (per-profession / per-index-page)
    /// HTTP fetch. With an always-true predicate, the function must return
    /// without performing any network I/O — proven by bounded latency well
    /// below the reqwest 15s timeout. The returned tuple's `cancelled` flag
    /// must be true and `builds` must be empty (nothing collected before
    /// the very first fetch). The predicate must have been observed at
    /// least once (the top-of-loop check).
    ///
    /// Mid-loop cancellation (cancel after N successful fetches) is not
    /// asserted here because it would require mocking the HTTP layer; see
    /// NOTES in the round-3 commit. The wiring proven here is sufficient:
    /// the predicate is in scope at the inner loop and short-circuits the
    /// loop body before each iteration's HTTP work.
    #[test]
    fn scrape_snowcrows_aborts_at_inner_loop_when_cancelled() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Instant;

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let predicate = move || {
            calls_clone.fetch_add(1, Ordering::Relaxed);
            true
        };
        let client = build_client().expect("client build must succeed");

        let start = Instant::now();
        let result = scrape_snowcrows(&client, "2026-04-16", &predicate, &|_, _| {});
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 500,
            "scrape_snowcrows must abort within 500ms when cancel pulses, took {:?}",
            elapsed
        );
        match result {
            Ok((builds, cancelled)) => {
                assert!(cancelled, "cancelled flag must be true");
                assert!(
                    builds.is_empty(),
                    "no builds should be collected before first fetch"
                );
            }
            Err(e) => panic!("expected Ok((empty, true)) on cancel, got Err({})", e),
        }
        assert!(
            calls.load(Ordering::Relaxed) >= 1,
            "predicate must be invoked at least once at inner loop entry"
        );
    }

    /// Inner-loop cancellation for hardstuck. Same shape as the snowcrows
    /// test — see that test's docstring for rationale.
    #[test]
    fn scrape_hardstuck_aborts_at_inner_loop_when_cancelled() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Instant;

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let predicate = move || {
            calls_clone.fetch_add(1, Ordering::Relaxed);
            true
        };
        let client = build_client().expect("client build must succeed");

        let start = Instant::now();
        let result = scrape_hardstuck(&client, "2026-04-16", &predicate, &|_, _| {});
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 500,
            "scrape_hardstuck must abort within 500ms when cancel pulses, took {:?}",
            elapsed
        );
        match result {
            Ok((builds, cancelled)) => {
                assert!(cancelled, "cancelled flag must be true");
                assert!(
                    builds.is_empty(),
                    "no builds should be collected before first fetch"
                );
            }
            Err(e) => panic!("expected Ok((empty, true)) on cancel, got Err({})", e),
        }
        assert!(
            calls.load(Ordering::Relaxed) >= 1,
            "predicate must be invoked at least once at inner loop entry"
        );
    }

    /// Inner-loop cancellation for guildjen. Same shape as the snowcrows
    /// test — see that test's docstring for rationale.
    #[test]
    fn scrape_guildjen_aborts_at_inner_loop_when_cancelled() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Instant;

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = calls.clone();
        let predicate = move || {
            calls_clone.fetch_add(1, Ordering::Relaxed);
            true
        };
        let client = build_client().expect("client build must succeed");

        let start = Instant::now();
        let result = scrape_guildjen(&client, "2026-04-16", &predicate, &|_, _| {});
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 500,
            "scrape_guildjen must abort within 500ms when cancel pulses, took {:?}",
            elapsed
        );
        match result {
            Ok((builds, cancelled)) => {
                assert!(cancelled, "cancelled flag must be true");
                assert!(
                    builds.is_empty(),
                    "no builds should be collected before first fetch"
                );
            }
            Err(e) => panic!("expected Ok((empty, true)) on cancel, got Err({})", e),
        }
        assert!(
            calls.load(Ordering::Relaxed) >= 1,
            "predicate must be invoked at least once at inner loop entry"
        );
    }

    fn sample_guardian_pve(notes: &str) -> BenchmarkBuild {
        BenchmarkBuild {
            source: "snowcrows".into(),
            profession: "Guardian".into(),
            spec_name: "Firebrand".into(),
            mode: "PvE".into(),
            role: "Power DPS".into(),
            build_code: None,
            gear_prefix: "Berserker's".into(),
            rune: "Scholar".into(),
            sigils: vec!["Force".into(), "Accuracy".into()],
            relic: "Fireworks".into(),
            traits: vec!["Radiance".into(), "Honor".into(), "Firebrand".into()],
            skills: vec![],
            source_url: "https://snowcrows.com/builds/raids/guardian/firebrand".into(),
            scraped_at: "2026-08-01".into(),
            notes: notes.into(),
        }
    }

    /// Mid-source cancel must not overwrite a previously good
    /// `{source}_{profession}_{mode}.json`. Inject a non-empty partial
    /// into `finish_source` with `cancelled=true` (the same Ok branch
    /// snowcrows/hardstuck/guildjen take after collecting at least one
    /// inner build). Seeded file bytes must stay unchanged when error
    /// is `CANCELLED_ERROR`.
    #[test]
    fn finish_source_cancel_does_not_overwrite_seeded_benchmark_file() {
        let tmp = std::env::temp_dir().join(format!(
            "gw2_scraper_cancel_partial_save_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("temp benchmarks dir");

        let seeded_path = tmp.join("snowcrows_guardian_pve.json");
        let seeded = serde_json::to_string_pretty(&vec![sample_guardian_pve("LAST_GOOD_SEED")])
            .expect("seed json");
        std::fs::write(&seeded_path, &seeded).expect("seed last-good file");
        let before = std::fs::read(&seeded_path).expect("read seed");

        let partial = sample_guardian_pve("SHOULD_NOT_LAND_ON_DISK");
        let result = finish_source(
            "snowcrows",
            Ok((vec![partial], true)),
            &tmp,
            &|_, _| {},
        );

        assert_eq!(
            result.error.as_deref(),
            Some(CANCELLED_ERROR),
            "cancelled finish_source must tag CANCELLED_ERROR"
        );
        assert_eq!(
            result.builds.len(),
            1,
            "in-memory partial stays on ScrapeResult"
        );

        let after = std::fs::read(&seeded_path).expect("read after cancel");
        assert_eq!(
            after, before,
            "cancelled finish_source must not overwrite last-good snowcrows_guardian_pve.json"
        );
        let on_disk = std::fs::read_to_string(&seeded_path).unwrap();
        assert!(
            on_disk.contains("LAST_GOOD_SEED"),
            "seeded last-good content must remain"
        );
        assert!(
            !on_disk.contains("SHOULD_NOT_LAND_ON_DISK"),
            "partial notes must not replace the last-good file"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_builds_reports_write_failure() {
        let missing = std::env::temp_dir().join(format!(
            "gw2bo-no-dir-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let err = save_builds(&[sample_guardian_pve("x")], &missing).expect_err("hollow dir");
        assert!(
            err.contains("write"),
            "write failure must surface, got {err}"
        );
        let result = finish_source(
            "snowcrows",
            Ok((vec![sample_guardian_pve("x")], false)),
            &missing,
            &|_, _| {},
        );
        assert!(
            result.error.as_ref().is_some_and(|e| e.contains("write")),
            "finish_source must not report done on silent write, got {:?}",
            result.error
        );
    }
}
