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
    // Cancel before any work
    if should_cancel() {
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
        return vec![
            cancelled_result("snowcrows"),
            cancelled_result("hardstuck"),
            cancelled_result("guildjen"),
        ];
    }
    let sc_result = match scrape_snowcrows(&client, &today, should_cancel) {
        Ok((builds, cancelled)) => {
            save_builds(&builds, &benchmarks_dir);
            let error = if cancelled {
                Some(CANCELLED_ERROR.into())
            } else {
                None
            };
            ScrapeResult {
                source: "snowcrows".into(),
                builds,
                error,
            }
        }
        Err(e) => ScrapeResult {
            source: "snowcrows".into(),
            builds: vec![],
            error: Some(e),
        },
    };

    // Scrape #2: Hardstuck.
    if should_cancel() {
        return vec![
            sc_result,
            cancelled_result("hardstuck"),
            cancelled_result("guildjen"),
        ];
    }
    let hs_result = match scrape_hardstuck(&client, &today, should_cancel) {
        Ok((builds, cancelled)) => {
            save_builds(&builds, &benchmarks_dir);
            let error = if cancelled {
                Some(CANCELLED_ERROR.into())
            } else {
                None
            };
            ScrapeResult {
                source: "hardstuck".into(),
                builds,
                error,
            }
        }
        Err(e) => ScrapeResult {
            source: "hardstuck".into(),
            builds: vec![],
            error: Some(e),
        },
    };

    // Scrape #3: GuildJen.
    if should_cancel() {
        return vec![sc_result, hs_result, cancelled_result("guildjen")];
    }
    let gj_result = match scrape_guildjen(&client, &today, should_cancel) {
        Ok((builds, cancelled)) => {
            save_builds(&builds, &benchmarks_dir);
            let error = if cancelled {
                Some(CANCELLED_ERROR.into())
            } else {
                None
            };
            ScrapeResult {
                source: "guildjen".into(),
                builds,
                error,
            }
        }
        Err(e) => ScrapeResult {
            source: "guildjen".into(),
            builds: vec![],
            error: Some(e),
        },
    };

    vec![sc_result, hs_result, gj_result]
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
) -> Result<(Vec<BenchmarkBuild>, bool), String> {
    let mut all_links: Vec<String> = Vec::new();

    // Collect build links from each profession's page
    for profession in SC_PROFESSIONS {
        if should_cancel() {
            return Ok((Vec::new(), true));
        }
        let prof_url = format!("https://snowcrows.com/builds/raids/{}", profession);
        let Ok(html) = fetch_html(client, &prof_url) else {
            continue;
        };
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
        return Err("Snowcrows: no build links found on any profession page".into());
    }

    let mut builds = Vec::new();
    // Cap at 45 builds total (5 per profession on average across 9 professions)
    for url in all_links.into_iter().take(45) {
        if should_cancel() {
            return Ok((builds, true));
        }
        if let Ok(b) = scrape_snowcrows_build(client, &url, today) { builds.push(b) }
    }
    Ok((builds, false))
}

/// Build a `BenchmarkBuild` from page HTML once the per-site fields
/// (source/profession/spec/mode/role) have been derived. The gear/trait/skill
/// extraction is identical across every source, so it lives here.
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

/// Scrape Hardstuck (multi-mode builds).
///
/// `should_cancel` is consulted before every outer (per-profession) and inner
/// (per-build) HTTP fetch. On cancellation, returns whatever builds were
/// collected so far paired with `cancelled = true`.
fn scrape_hardstuck(
    client: &reqwest::blocking::Client,
    today: &str,
    should_cancel: &dyn Fn() -> bool,
) -> Result<(Vec<BenchmarkBuild>, bool), String> {
    let mut all_links: Vec<String> = Vec::new();

    // Each profession page lists builds for that profession
    for profession in HS_PROFESSIONS {
        if should_cancel() {
            return Ok((Vec::new(), true));
        }
        let prof_url = format!("https://hardstuck.gg/gw2/builds/{}/", profession);
        let Ok(html) = fetch_html(client, &prof_url) else {
            continue;
        };
        // Build links: href="/gw2/builds/{profession}/{slug}/" with a non-empty slug
        // slug can be numeric (24929) or text (blood-harbinger)
        let links = extract_build_links(&html, &format!("/gw2/builds/{}/", profession), 40);
        for link in links {
            let parts: Vec<&str> = link.trim_matches('/').split('/').collect();
            // /gw2/builds/{profession}/{slug} = exactly 4 segments
            if parts.len() == 4 && !parts[3].is_empty() && !parts[3].contains('?') {
                let full = if link.starts_with("http") {
                    link
                } else {
                    format!("https://hardstuck.gg{}", link)
                };
                if !all_links.contains(&full) {
                    all_links.push(full);
                }
            }
        }
    }

    if all_links.is_empty() {
        return Err("Hardstuck: no build links found on any profession page".into());
    }

    let mut builds = Vec::new();
    for url in all_links.into_iter().take(45) {
        if should_cancel() {
            return Ok((builds, true));
        }
        if let Ok(b) = scrape_hardstuck_build(client, &url, today) { builds.push(b) }
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
        let Ok(html) = fetch_html(client, index_url) else {
            continue;
        };

        let links = extract_build_links(&html, "guildjen.com/", 40);
        any_success = true;

        for link in links.into_iter().take(15) {
            if should_cancel() {
                return Ok((builds, true));
            }
            let url = if link.starts_with("http") {
                link
            } else {
                format!("https://guildjen.com{}", link)
            };
            if let Ok(mut b) = scrape_guildjen_build(client, &url, today) {
                b.mode = mode.to_string();
                builds.push(b);
            }
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
        &html,
        url,
        today,
        "guildjen",
        profession,
        spec_name,
        mode,
        role,
    ))
}

// ─── HTML extraction helpers ──────────────────────────────────────────────────

fn fetch_html(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    client
        .get(url)
        .send()
        .map_err(|e| format!("HTTP error fetching {}: {}", url, e))?
        .text()
        .map_err(|e| format!("UTF-8 error reading {}: {}", url, e))
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
    let html_lower = html.to_lowercase();
    for p in &prefixes {
        if html_lower.contains(&p.to_lowercase()) {
            return p.to_string();
        }
    }
    String::new()
}

/// Extract rune name from HTML.
fn extract_rune(html: &str) -> String {
    // Rune names follow "Rune of" or "Superior Rune"
    for marker in &["Rune of the ", "Rune of ", "Superior Rune"] {
        if let Some(pos) = html.find(marker) {
            let after = &html[pos..];
            // Take up to 40 chars and trim at next HTML tag or quote
            let raw = &after[..after.len().min(60)];
            let end = raw
                .find(['<', '"', '\n'])
                .unwrap_or(raw.len());
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
            let after = &html[abs..abs.min(html.len() - 1) + 60.min(html.len() - abs)];
            let end = after
                .find(['<', '"', '\n'])
                .unwrap_or(after.len());
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
            let raw = &after[..after.len().min(60)];
            let end = raw
                .find(['<', '"', '\n'])
                .unwrap_or(raw.len());
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
    let markers = [
        "Heal:",
        "Utility:",
        "Elite:",
        "utility-skill",
        "heal-skill",
        "elite-skill",
    ];
    let mut skills = Vec::new();
    for marker in &markers {
        if let Some(pos) = html.find(marker) {
            let after = &html[pos + marker.len()..];
            let raw = &after[..after.len().min(80)];
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

// ─── Storage ──────────────────────────────────────────────────────────────────

fn save_builds(builds: &[BenchmarkBuild], dir: &Path) {
    if builds.is_empty() {
        return;
    }
    // Group by (source, profession, mode)
    let mut groups: std::collections::HashMap<String, Vec<&BenchmarkBuild>> =
        std::collections::HashMap::new();
    for b in builds {
        let key = format!(
            "{}_{}_{}.json",
            b.source,
            b.profession.to_lowercase().replace(' ', "_"),
            b.mode.to_lowercase()
        );
        groups.entry(key).or_default().push(b);
    }
    for (filename, group) in &groups {
        let path = dir.join(filename);
        if let Ok(json) = serde_json::to_string_pretty(group) {
            let _ = std::fs::write(path, json);
        }
    }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

fn build_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        // Accept invalid/untrusted certs — required on Windows where certain CA roots
        // (e.g. Let's Encrypt R3 chain used by hardstuck.gg) may not be trusted in the
        // Schannel TLS stack used inside the game process.
        .danger_accept_invalid_certs(true)
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
    fn test_extract_build_links_finds_hrefs() {
        let html =
            r#"<a href="/builds/guardian/firebrand">Firebrand</a><a href="/other">Other</a>"#;
        let links = extract_build_links(html, "/builds/", 10);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "/builds/guardian/firebrand");
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
        let result = scrape_snowcrows(&client, "2026-04-16", &predicate);
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
        let result = scrape_hardstuck(&client, "2026-04-16", &predicate);
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
        let result = scrape_guildjen(&client, "2026-04-16", &predicate);
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
}
