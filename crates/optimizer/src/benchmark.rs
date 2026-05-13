//! Benchmark build type — normalized reference build data from community sources.
//!
//! Scraped from Snowcrows (PvE), Hardstuck (general), and GuildJen (WvW/PvP).
//! Stored as JSON files in `{addon_dir}/benchmarks/`.

use serde::{Deserialize, Serialize};

use crate::scoring::{select_gear_prefix, OptimizationWeights};

/// A single normalized reference build from a community build site.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Gear stat prefix (e.g. "Berserker's", "Viper's").
    pub gear_prefix: String,
    /// Rune name.
    pub rune: String,
    /// Sigil names.
    pub sigils: Vec<String>,
    /// Relic name.
    pub relic: String,
    /// Trait line names (short form, from page heading or text).
    pub traits: Vec<String>,
    /// Skill names found on the page (heal, utilities, elite).
    pub skills: Vec<String>,
    /// Page URL this was scraped from.
    pub source_url: String,
    /// ISO date when this was scraped (e.g. "2026-03-30").
    pub scraped_at: String,
    /// Any additional notes parsed from the page.
    pub notes: String,
}

impl BenchmarkBuild {
    /// Canonical filename for this benchmark entry (for storage).
    pub fn filename(&self) -> String {
        format!(
            "{}_{}_{}.json",
            self.source,
            self.profession.to_lowercase().replace(' ', "_"),
            self.mode.to_lowercase()
        )
    }
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
    let best = candidates
        .into_iter()
        .max_by_key(|b| role_similarity(&b.role.to_lowercase(), &role_lower));

    best
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
            rune: String::new(),
            sigils: vec![],
            relic: String::new(),
            traits: vec![],
            skills: vec![],
            source_url: "https://example.com".into(),
            scraped_at: "2026-01-01".into(),
            notes: String::new(),
        }
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
