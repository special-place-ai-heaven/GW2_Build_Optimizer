//! Benchmark build type — normalized reference build data from community sources.
//!
//! Scraped from Snowcrows (PvE), Hardstuck (general), and GuildJen (WvW/PvP).
//! Stored as JSON files in `{addon_dir}/benchmarks/`.

use serde::{Deserialize, Serialize};

use crate::providers::ProviderBuild;
use crate::scoring::{select_gear_prefix, OptimizationWeights};

/// A single normalized reference build from a community build site.
///
/// `Default` is derived so adding a field is a one-line diff at every
/// construction site instead of a compile error at each one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BenchmarkBuild {
    /// Source site: "snowcrows", "hardstuck", or "guildjen".
    pub source: String,
    /// Profession name (e.g. "Guardian", "Necromancer").
    pub profession: String,
    /// Elite spec name (e.g. "Firebrand", "Scourge").
    pub spec_name: String,
    /// Game mode: "PvE", "WvW", "PvP".
    pub mode: String,
    /// Role description (e.g. "Power DPS", "Condi DPS", "Heal Support").
    pub role: String,
    /// GW2 build template code if found.
    pub build_code: Option<String>,
    /// The stat prefix most of the gear uses (e.g. "Berserker's", "Viper's").
    ///
    /// A reduction, not the whole truth — most builds mix prefixes, and the
    /// per-slot detail is on [`BenchmarkBuild::published`]. This one string
    /// is what the scorer compares against.
    pub gear_prefix: String,
    /// Page URL this was scraped from.
    pub source_url: String,
    /// ISO date when this was scraped (e.g. "2026-03-30").
    pub scraped_at: String,
    /// What the page published, in GW2 API ids: gear, upgrades, traits and
    /// the skill bar.
    ///
    /// This replaced six name fields — `rune`, `sigils`, `relic`, `traits`,
    /// `skills` and `notes` — which no code ever read and which could not be
    /// filled honestly: none of the three sites writes gear names into its
    /// markup, so the text extractors returned empty strings 570 to 739
    /// times out of 739, and where they did return something it was page
    /// furniture. 431 of 739 rows recorded the same three elite specs
    /// whatever the profession, taken from a navigation menu.
    ///
    /// Ids do not have that failure mode: they need no fuzzy matching, they
    /// do not move with the site's wording, and they are the numbers
    /// `GameDb` is already keyed on — so a name is a read-time lookup, not a
    /// scrape-time guess.
    #[serde(skip_serializing_if = "ProviderBuild::is_empty")]
    pub published: ProviderBuild,
}

/// Score delta between the optimizer's result and a community reference build.
#[derive(Debug, Clone)]
pub struct BenchmarkDelta {
    /// Source site the reference came from.
    pub source: String,
    /// Profession name of the reference.
    pub profession: String,
    /// Role of the reference (e.g. "Power DPS").
    pub role: String,
    /// Gear prefix of the reference build.
    pub ref_gear_prefix: String,
    /// Estimated score of the community reference build (0.0-1.0).
    pub ref_score: f64,
    /// Score of the optimizer's result (0.0-1.0).
    pub our_score: f64,
    /// `our_score / ref_score` as a percentage (100 = on-par, >100 = better).
    pub pct_of_ref: f64,
    /// URL of the reference page.
    pub ref_url: String,
}

/// Result from scraping one source site.
#[derive(Debug, Clone)]
pub struct ScrapeResult {
    /// Source identifier.
    pub source: String,
    /// Successfully parsed builds.
    pub builds: Vec<BenchmarkBuild>,
    /// Error message if the scrape failed or partially failed.
    pub error: Option<String>,
    /// Pages that were listed but produced no build.
    ///
    /// Separate from `error`, which is about the source as a whole. A run
    /// can list 157 builds, return 148 and be entirely healthy apart from
    /// nine pages whose layout the extractor did not recognise - and saying
    /// "done 148" alone hides that nine went missing.
    pub failed: usize,
}

// ─── Matching ────────────────────────────────────────────────────────────────

/// Find the best-matching benchmark build for a given profession, mode, and role hint.
///
/// Matching priority:
/// 1. Profession (case-insensitive contains match)
/// 2. Mode (exact, case-insensitive)
/// 3. Role hint similarity (word overlap score)
///
/// Returns `None` if no builds match the profession+mode criteria.
pub fn find_best_benchmark<'a>(
    builds: &'a [BenchmarkBuild],
    profession: &str,
    mode: &str,
    role_hint: &str,
) -> Option<&'a BenchmarkBuild> {
    let prof_lower = profession.to_lowercase();
    let mode_lower = mode.to_lowercase();
    let role_lower = role_hint.to_lowercase();

    // Filter: must match profession AND mode
    let candidates: Vec<&BenchmarkBuild> = builds
        .iter()
        .filter(|b| {
            b.profession.to_lowercase().contains(&prof_lower) && b.mode.to_lowercase() == mode_lower
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Score by role similarity (word overlap)

    candidates
        .into_iter()
        .max_by_key(|b| role_similarity(&b.role.to_lowercase(), &role_lower))
}

/// Compute a simple word-overlap similarity score between two role strings.
fn role_similarity(a: &str, b: &str) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    words_a.intersection(&words_b).count()
}

// ─── Scoring proxy ────────────────────────────────────────────────────────────

/// Estimate a score for a benchmark build by proxying its gear prefix through the
/// objective scorer. This is a rough estimate — the benchmark build has no full
/// stat block, so we use the gear prefix cosine similarity as a proxy for how
/// well it matches the current scoring weights.
///
/// Returns a score in [0.0, 1.0]. Higher = better match for the current weights.
pub fn score_benchmark_build(build: &BenchmarkBuild, weights: &OptimizationWeights) -> f64 {
    if build.gear_prefix.is_empty() {
        // No gear data scraped — use a neutral estimate
        return 0.5;
    }

    // Use cosine similarity between benchmark's gear prefix purpose profile
    // and the current weights as the proxy score.
    let gear_match = select_gear_prefix(weights);

    if gear_match.primary.to_lowercase() == build.gear_prefix.to_lowercase()
        || build
            .gear_prefix
            .to_lowercase()
            .contains(&gear_match.primary.to_lowercase())
    {
        // Benchmark uses the same gear as the optimizer would pick — full similarity score
        gear_match.similarity
    } else {
        // Different gear — penalise by how far it is from the target profile
        // Use secondary match if the benchmark gear matches it
        if let Some(sec) = gear_match.secondary {
            if sec.to_lowercase() == build.gear_prefix.to_lowercase() {
                return gear_match.similarity * 0.85;
            }
        }
        // Generic fallback: use 0.6 as baseline for a valid but non-ideal gear choice
        0.60_f64.min(gear_match.similarity * 0.75)
    }
}

/// Compute a `BenchmarkDelta` comparing the optimizer's scored result to the
/// best matching community reference build.
///
/// `our_score` should be the `user_intent_score` from the RefereeReport (or
/// a normalised combat metric if unavailable).
pub fn compute_benchmark_delta(
    builds: &[BenchmarkBuild],
    profession: &str,
    mode: &str,
    role_hint: &str,
    weights: &OptimizationWeights,
    our_score: f64,
) -> Option<BenchmarkDelta> {
    let reference = find_best_benchmark(builds, profession, mode, role_hint)?;
    let ref_score = score_benchmark_build(reference, weights).max(0.01);
    let pct_of_ref = if ref_score > 0.0 {
        (our_score / ref_score * 100.0).min(200.0) // cap at 200% to avoid absurd display
    } else {
        100.0
    };

    Some(BenchmarkDelta {
        source: reference.source.clone(),
        profession: reference.profession.clone(),
        role: reference.role.clone(),
        ref_gear_prefix: reference.gear_prefix.clone(),
        ref_score,
        our_score,
        pct_of_ref,
        ref_url: reference.source_url.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_build(profession: &str, mode: &str, role: &str, gear: &str) -> BenchmarkBuild {
        BenchmarkBuild {
            source: "test".into(),
            profession: profession.into(),
            spec_name: String::new(),
            mode: mode.into(),
            role: role.into(),
            build_code: None,
            gear_prefix: gear.into(),
            source_url: "https://example.com".into(),
            scraped_at: "2026-01-01".into(),
            ..Default::default()
        }
    }

    /// A row exactly as the store held it before ids existed, verbatim from
    /// `guildjen_elementalist_pve.json` on 2026-09-05 — six fields that no
    /// longer exist among them.
    ///
    /// This test is the guard on the one mistake that would be invisible.
    /// Both load paths swallow a parse error (`if let Ok(v)` in
    /// `load_benchmarks`, `let Ok(..) else { continue }` in
    /// `load_todays_builds`), so a field added without `#[serde(default)]`
    /// would drop all 739 rows at once with no log line: the Settings tab
    /// would read "never synced", the Improve tab would show "no benchmark
    /// data", and the next sync would refetch every page.
    #[test]
    fn a_row_written_before_ids_existed_still_loads() {
        const LEGACY: &str = r#"{
            "source": "guildjen",
            "profession": "Elementalist",
            "spec_name": "Evoker",
            "mode": "PvE",
            "role": "WvW Roaming",
            "build_code": "[&BPcAAAA=]",
            "gear_prefix": "Plaguedoctor's",
            "rune": "",
            "sigils": [],
            "relic": "",
            "traits": ["Firebrand", "Willbender", "Dragonhunter"],
            "skills": [],
            "source_url": "https://guildjen.com/heal-dps-evoker-build/",
            "scraped_at": "2026-09-05",
            "notes": ""
        }"#;

        let build: BenchmarkBuild = serde_json::from_str(LEGACY).expect("a legacy row must load");
        // The fields every reader actually touches survive untouched.
        assert_eq!(build.profession, "Elementalist");
        assert_eq!(build.mode, "PvE");
        assert_eq!(build.gear_prefix, "Plaguedoctor's");
        assert_eq!(build.source_url, "https://guildjen.com/heal-dps-evoker-build/");
        // The six deleted fields are ignored rather than fatal, and the row
        // reports honestly that it carries no ids.
        assert!(
            build.published.is_empty(),
            "a pre-ids row must not pretend to have published any"
        );

        // A whole file of them, which is what load_benchmarks actually reads.
        let file = format!("[{LEGACY},{LEGACY}]");
        let rows: Vec<BenchmarkBuild> = serde_json::from_str(&file).expect("a legacy file");
        assert_eq!(rows.len(), 2);
    }

    /// Rolling back to an older build must not brick the store either: there
    /// is no `deny_unknown_fields`, so a row carrying ids loads anywhere.
    #[test]
    fn a_row_carrying_ids_round_trips() {
        let mut build = make_build("Necromancer", "PvE", "Condi DPS", "Viper's");
        build.published.rune_id = Some(24762);
        build.published.sigil_ids = vec![44944, 24560];
        build.published.specs = vec![crate::providers::SpecLine {
            id: 39,
            trait_ids: vec![815, 816, 801],
        }];

        let json = serde_json::to_string(&build).expect("serialise");
        let back: BenchmarkBuild = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.published, build.published);
        assert!(
            !json.contains("\"gear\""),
            "empty id fields are not written, keeping the store small: {json}"
        );
    }

    #[test]
    fn find_best_benchmark_matches_profession_and_mode() {
        let builds = vec![
            make_build("Guardian", "PvE", "Power DPS", "Berserker's"),
            make_build("Necromancer", "PvE", "Condi DPS", "Viper's"),
            make_build("Guardian", "WvW", "WvW Roaming", "Marauder"),
        ];

        let result = find_best_benchmark(&builds, "Guardian", "PvE", "Power DPS");
        assert!(result.is_some());
        let b = result.unwrap();
        assert_eq!(b.profession, "Guardian");
        assert_eq!(b.mode, "PvE");
        assert_eq!(b.gear_prefix, "Berserker's");
    }

    #[test]
    fn find_best_benchmark_returns_none_unknown_profession() {
        let builds = vec![make_build("Guardian", "PvE", "Power DPS", "Berserker's")];
        assert!(find_best_benchmark(&builds, "Thief", "PvE", "Power DPS").is_none());
    }

    #[test]
    fn find_best_benchmark_prefers_role_match() {
        let builds = vec![
            make_build("Guardian", "WvW", "WvW Zerg DPS", "Berserker's"),
            make_build("Guardian", "WvW", "WvW Roaming", "Marauder"),
        ];

        // Asking for Roaming should return Marauder build
        let result = find_best_benchmark(&builds, "Guardian", "WvW", "WvW Roaming");
        assert!(result.is_some());
        assert_eq!(result.unwrap().gear_prefix, "Marauder");
    }

    #[test]
    fn find_best_benchmark_case_insensitive_profession() {
        let builds = vec![make_build("Guardian", "PvE", "Power DPS", "Berserker's")];
        assert!(find_best_benchmark(&builds, "guardian", "PvE", "Power DPS").is_some());
    }

    #[test]
    fn role_similarity_counts_overlap() {
        assert_eq!(role_similarity("power dps", "power dps"), 2);
        assert_eq!(role_similarity("power dps", "condi dps"), 1);
        assert_eq!(role_similarity("power dps", "healer"), 0);
    }

    #[test]
    fn score_benchmark_build_no_gear_returns_neutral() {
        let mut b = make_build("Guardian", "PvE", "Power DPS", "");
        b.gear_prefix = String::new();
        let w = OptimizationWeights::preset_power_dps();
        let score = score_benchmark_build(&b, &w);
        assert!((score - 0.5).abs() < 0.001);
    }

    #[test]
    fn score_benchmark_build_matching_gear_scores_higher() {
        let berserker = make_build("Guardian", "PvE", "Power DPS", "Berserker's");
        let nomad = make_build("Guardian", "PvE", "Tank", "Nomad's");
        let w = OptimizationWeights::preset_power_dps();
        let score_b = score_benchmark_build(&berserker, &w);
        let score_n = score_benchmark_build(&nomad, &w);
        assert!(
            score_b > score_n,
            "Berserker (score={:.3}) should score higher than Nomad (score={:.3}) with Power DPS weights",
            score_b, score_n
        );
    }

    #[test]
    fn compute_benchmark_delta_produces_pct() {
        let builds = vec![make_build("Guardian", "PvE", "Power DPS", "Berserker's")];
        let w = OptimizationWeights::preset_power_dps();
        let delta = compute_benchmark_delta(&builds, "Guardian", "PvE", "Power DPS", &w, 0.7);
        assert!(delta.is_some());
        let d = delta.unwrap();
        assert!(d.pct_of_ref > 0.0);
        assert!(d.pct_of_ref <= 200.0);
    }
}
