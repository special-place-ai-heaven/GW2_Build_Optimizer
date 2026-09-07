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

/// What a proposed build looks like, for finding published ones like it.
///
/// Names rather than ids, because that is what the proposal has: the model
/// answers in names and they are resolved against the API on the way in.
/// Everything is compared lowercased.
#[derive(Debug, Clone, Default)]
pub struct BuildShape {
    pub profession: String,
    pub mode: String,
    /// Specialization names, elite included.
    pub specs: Vec<String>,
    /// Weapon type names — `Greatsword`, `Dagger`.
    pub weapons: Vec<String>,
    pub stat_prefix: String,
    pub rune: String,
    pub relic: String,
    /// The job as this addon names it — healer, DPS, support. Carries nearly
    /// the weight of a specialization: it is what someone means when they say
    /// they want a healer for their spec.
    pub role: String,
}

/// The coarse job a role name describes, when it describes one.
///
/// Not a taxonomy of the eleven words the three sites use between them —
/// only the distinction that decides whether a build is the WRONG ANSWER
/// rather than a worse one. A healer and a DPS are not near neighbours; they
/// are opposite jobs, and offering one in place of the other is not a
/// suggestion, it is a wrong answer with a card around it.
///
/// Order matters and is the whole trick. "Zerg Boon DPS" is a DPS that
/// happens to give boons, so damage is tested before support; "Offensive
/// Support" is a support that happens to do damage, so `heal` is tested
/// before both. `None` means the words say nothing either way and nothing is
/// ruled out on their account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobFamily {
    /// Keeps other people alive: healing, cleansing, boons, stability.
    Support,
    /// Sustained damage — the thing that dies to it dies over a fight.
    Damage,
    /// Burst. Not a Damage build with better numbers: it opens with a
    /// disable and spends everything inside a window of a few seconds, and
    /// it is a glass cannon that must disengage if the window closes with
    /// the target alive. Asking for one and being handed a zerg DPS is the
    /// wrong build, not a lesser one.
    Assassin,
    /// Wins one fight at a time — medium damage, medium sustain, cleanse
    /// and control.
    Duelist,
    /// Stands in it. Front line, outnumbered, still alive.
    Bruiser,
}

/// See [`JobFamily`].
pub fn job_family(role: &str) -> Option<JobFamily> {
    let role = role.to_lowercase();
    let has = |word: &str| role.contains(word);
    // Order is the whole trick. The specific words go first, because the
    // generic ones appear inside them: "Roaming Assassin" is an assassin
    // before it is anything else, and "Offensive Support" is a support even
    // though it does damage.
    if has("heal") || has("medic") {
        return Some(JobFamily::Support);
    }
    if has("assassin") {
        return Some(JobFamily::Assassin);
    }
    if has("duelist") {
        return Some(JobFamily::Duelist);
    }
    // A PvP sidenoder holds a point alone against whoever walks onto it —
    // that is the duelist's job under another name.
    if has("sidenoder") {
        return Some(JobFamily::Duelist);
    }
    if has("dps") || has("damage") || has("hybrid") {
        return Some(JobFamily::Damage);
    }
    if has("support") {
        return Some(JobFamily::Support);
    }
    if has("bruiser") || has("tank") || has("troll") {
        return Some(JobFamily::Bruiser);
    }
    None
}

/// Power or condition, when the role name says.
///
/// Cross-referenced as a disqualifier, not a preference. A condition build
/// is not a power build with different numbers — different stats, different
/// runes, different sigils, different traits, and a different way of
/// killing something. Offered in place of one another they are simply the
/// wrong build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    Power,
    Condi,
    /// Says it wants both, so neither rules it out.
    Hybrid,
}

/// What a stat prefix is FOR, read off the attributes it grants.
///
/// The role name is a label somebody typed; the prefix is what the build
/// wears. GuildJen publishes a Reaper as "Roaming DPS" and says nothing
/// about power or condition — but it is Marauder, and Marauder grants Power,
/// Precision, Ferocity and Vitality, so the build is a power build whatever
/// the label omits. Matching on the label alone offered that build to
/// someone asking for condition damage.
///
/// This answers "power or condition", not "what job is this". Celestial
/// grants both and therefore reads as Hybrid here — but nobody wears
/// Celestial to be a damage build; it is a bruiser's prefix, moderate at
/// everything and excellent at nothing. That judgement belongs to
/// [`prefix_job`], which is about the job, not the damage type.
pub fn prefix_flavour(prefix: &str, db: &crate::gamedb::GameDb) -> Option<Flavour> {
    if prefix.trim().is_empty() {
        return None;
    }
    let wanted = prefix.trim().trim_end_matches("'s").to_lowercase();
    let stat = db.itemstats.values().find(|stat| {
        let name = stat.name.trim().trim_end_matches("'s").to_lowercase();
        name == wanted
    })?;

    let grants = |attribute: &str| {
        stat.attributes
            .iter()
            .any(|a| a.attribute.eq_ignore_ascii_case(attribute) && a.value + 1 > 0)
    };
    let condi = grants("ConditionDamage") || grants("ConditionDuration");
    let power = grants("Power") || grants("CritDamage");
    match (power, condi) {
        // Everything at once is the definition of Celestial, and of Hybrid.
        (true, true) => Some(Flavour::Hybrid),
        (true, false) => Some(Flavour::Power),
        (false, true) => Some(Flavour::Condi),
        // Minstrel's and Harrier's grant neither: they are not damage
        // prefixes at all, and have no flavour to disagree about.
        (false, false) => None,
    }
}

/// The job a stat prefix is dressed for, when the role name did not say.
///
/// Gear is a statement of intent. Nobody wears Minstrel's to deal damage or
/// Berserker's to hold a door, so where a site publishes a build as nothing
/// more specific than "Roamer" or "DPS", the prefix still says what it is
/// for.
///
/// Deliberately only three answers and only from the extremes, because this
/// is a fallback for silence and a confident wrong guess is worse than none:
///
/// - grants no offence at all but grants healing → Support (Minstrel's,
///   Harrier's)
/// - grants everything → Bruiser. Celestial is moderate at all nine
///   attributes, which is a bruiser's shape: enough damage to threaten,
///   enough sustain to stay. It is not a damage prefix.
/// - grants offence and no defence → Damage (Berserker's, Viper's)
///
/// Anything in between — Marauder, Trailblazer's, Demolisher — says both
/// things and is left to the label.
pub fn prefix_job(prefix: &str, db: &crate::gamedb::GameDb) -> Option<JobFamily> {
    if prefix.trim().is_empty() {
        return None;
    }
    let wanted = prefix.trim().trim_end_matches("'s").to_lowercase();
    let stat = db.itemstats.values().find(|stat| {
        let name = stat.name.trim().trim_end_matches("'s").to_lowercase();
        name == wanted
    })?;
    let grants = |attribute: &str| {
        stat.attributes
            .iter()
            .any(|a| a.attribute.eq_ignore_ascii_case(attribute) && a.value + 1 > 0)
    };
    let offence = grants("Power") || grants("ConditionDamage") || grants("CritDamage");
    let defence = grants("Toughness") || grants("Vitality");
    let healing = grants("Healing") || grants("BoonDuration");

    match (offence, defence, healing) {
        (false, _, true) => Some(JobFamily::Support),
        // Everything at once: Celestial.
        (true, true, true) => Some(JobFamily::Bruiser),
        (true, false, false) => Some(JobFamily::Damage),
        _ => None,
    }
}

/// See [`Flavour`]. `None` where the words do not say.
pub fn damage_flavour(role: &str) -> Option<Flavour> {
    let role = role.to_lowercase();
    if role.contains("hybrid") {
        return Some(Flavour::Hybrid);
    }
    if role.contains("condi") {
        return Some(Flavour::Condi);
    }
    if role.contains("power") {
        return Some(Flavour::Power);
    }
    None
}

/// How many people are around, as the sites name it.
///
/// The player put it plainly: the smaller the group, the more self-sufficient
/// a build has to be, because a large group covers for it with overlapping
/// boons and a lone one has nobody. A zerg healer dropped into a roaming
/// fight dies to the first assassin that looks at it. So scale is not
/// decoration on a role name; it changes what the job IS.
///
/// It disqualifies, but ASYMMETRICALLY, because self-reliance only runs one
/// way. A roaming build can walk into a zerg — it carries its own sustain
/// and never knew who it would meet, and roamers adapt by swapping utility
/// skills rather than rebuilding. A zerg build cannot go roaming: it leans
/// on twenty people's overlapping boons, and alone it dies to the first
/// assassin that looks at it.
///
/// So a candidate is allowed when it is at least as self-reliant as the
/// scale asked for — see [`Scale::self_reliance`]. Where that leaves
/// nothing, nothing is offered: the sites have simply not published that
/// build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Solo,
    Small,
    Large,
}

impl Scale {
    /// How much a build made for this scale has to carry alone, smallest
    /// group first. A build may always be offered to a LARGER group than it
    /// was written for, never to a smaller one.
    pub fn self_reliance(self) -> u8 {
        match self {
            Scale::Solo => 0,
            Scale::Small => 1,
            Scale::Large => 2,
        }
    }
}

/// See [`Scale`]. `None` where the words do not say.
pub fn role_scale(role: &str) -> Option<Scale> {
    let role = role.to_lowercase();
    let has = |word: &str| role.contains(word);
    if has("roam") || has("solo") || has("duel") || has("open world") {
        return Some(Scale::Solo);
    }
    if has("havoc") || has("party") || has("group") || has("fractal") {
        return Some(Scale::Small);
    }
    if has("zerg") || has("cloud") || has("squad") || has("raid") || has("strike") {
        return Some(Scale::Large);
    }
    None
}

/// How close a published build is to a proposed one. Higher is closer.
///
/// Weighted by how much each agreement actually says. Two builds sharing all
/// three specializations are the same build with different gear; two sharing
/// a stat prefix might have nothing else in common. So specializations
/// dominate, then the job, then the weapons that decide five skills each,
/// then the single choices — rune, relic, prefix.
///
/// The job is weighted near a specialization on purpose. A healing Firebrand
/// and a power Firebrand share all three specializations and are opposite
/// jobs; if the role only broke ties it could never tell them apart, and the
/// card offered to someone asking for a healer would be a DPS build wearing
/// the same specs. Matching on specs alone is how a Firebrand zerg healer
/// got matched to a Scrapper before any of this existed.
///
/// Zero means nothing in common, which is not the same as no data. A row
/// published without a rune cannot agree about runes and is not punished for
/// it beyond not scoring.
pub fn closeness(shape: &BuildShape, build: &BenchmarkBuild, db: &crate::gamedb::GameDb) -> u32 {
    let published = &build.published;
    let lower = |s: &str| s.to_lowercase();

    let their_specs: Vec<String> = published
        .specs
        .iter()
        .filter_map(|line| db.specializations.get(&line.id))
        .map(|spec| lower(&spec.name))
        .collect();
    // The elite specialization is the build's identity: a player who asked
    // for a Ritualist and got a Reaper card was not shown "something like
    // it" (in-game 2026-09-07, where a Harbinger and a Reaper outscored the
    // three published Ritualist roaming builds on shared core lines and
    // weapons). One elite match outweighs two core matches and the weapons.
    let is_elite = |name: &str| {
        db.specializations
            .values()
            .any(|spec| spec.elite && lower(&spec.name) == lower(name))
    };
    let spec_points: u32 = shape
        .specs
        .iter()
        .filter(|name| their_specs.iter().any(|theirs| theirs == &lower(name)))
        .map(|name| if is_elite(name) { 30 } else { 10 })
        .sum();

    // Weapons live in the gear rows' slot, which every site names with the
    // weapon type: `Greatsword`, `Dagger`, `Warhorn`.
    let their_weapons: Vec<String> = published.gear.iter().map(|g| lower(&g.slot)).collect();
    let weapon_hits = shape
        .weapons
        .iter()
        .filter(|name| their_weapons.iter().any(|theirs| theirs == &lower(name)))
        .count() as u32;

    let named = |id: Option<u32>| {
        id.and_then(|id| db.items.get(&id))
            .map(|item| lower(&item.name))
            .unwrap_or_default()
    };
    let rune_hit = !shape.rune.is_empty() && named(published.rune_id) == lower(&shape.rune);
    let relic_hit = !shape.relic.is_empty() && named(published.relic_id) == lower(&shape.relic);

    let scale_hit = match (role_scale(&shape.role), role_scale(&build.role)) {
        (Some(want), Some(theirs)) => want == theirs,
        _ => false,
    };

    let prefix_hit = !shape.stat_prefix.is_empty()
        && (lower(&build.gear_prefix) == lower(&shape.stat_prefix)
            || published
                .dominant_stat()
                .is_some_and(|stat| lower(&stat) == lower(&shape.stat_prefix)));

    spec_points
        + weapon_hits * 4
        + u32::from(rune_hit) * 3
        + u32::from(relic_hit) * 3
        + u32::from(prefix_hit) * 2
        + role_similarity(&lower(&build.role), &lower(&shape.role)) as u32 * 8
        + u32::from(scale_hit) * 9
}

/// The closest published build from each site, for "you might also like".
///
/// One per source rather than one overall, because the sites disagree and
/// that disagreement is the useful part: three takes on the same job tell a
/// player more than three rows from whichever site writes the most builds.
///
/// Only rows carrying real published data qualify. A row scraped before the
/// per-site parsers existed has a name and a URL and nothing to compare, so
/// this stays empty until the player has actually synced — which is the
/// intended gate, not a side effect. Rows with nothing at all in common are
/// dropped too: an unrelated build offered as a suggestion is worse than no
/// suggestion.
pub fn closest_per_source<'a>(
    builds: &'a [BenchmarkBuild],
    shape: &BuildShape,
    db: &crate::gamedb::GameDb,
) -> Vec<(&'a BenchmarkBuild, u32)> {
    let prof = shape.profession.to_lowercase();
    let mode = shape.mode.to_lowercase();

    let mut best: std::collections::BTreeMap<&str, (u32, &BenchmarkBuild)> = Default::default();
    for build in builds {
        if build.published.is_empty()
            || !build.profession.to_lowercase().contains(&prof)
            || build.mode.to_lowercase() != mode
        {
            continue;
        }
        // Wrong job, no card. A site with nothing for this job should offer
        // nothing: GuildJen publishes no Necromancer WvW support build at
        // all, and without this it answered a request for a healer with its
        // closest DPS — same profession, same mode, opposite job.
        // The label says the job where it can; the gear says it where the
        // label was vague. "Roamer" and a bare "DPS" name no job at all, and
        // a Celestial roamer is a bruiser whatever the page called it.
        let want_job = job_family(&shape.role).or_else(|| prefix_job(&shape.stat_prefix, db));
        let their_job = job_family(&build.role).or_else(|| {
            prefix_job(
                &build
                    .published
                    .dominant_stat()
                    .unwrap_or_else(|| build.gear_prefix.clone()),
                db,
            )
        });
        if let (Some(want), Some(theirs)) = (want_job, their_job) {
            if want != theirs {
                continue;
            }
        }
        // Power and condition are not variants of one build. Read from the
        // gear first and the label only when the gear is silent: a site that
        // writes "Roaming DPS" and equips Marauder has told us it is a power
        // build without using the word.
        let want_flavour =
            damage_flavour(&shape.role).or_else(|| prefix_flavour(&shape.stat_prefix, db));
        // Label first on both sides, gear only where the label is silent. A
        // build that says "Condi DPS" is a condition build even on Celestial
        // — Celestial grants power too, so reading the gear first made it
        // answer a request for a power build.
        let their_flavour = damage_flavour(&build.role).or_else(|| {
            prefix_flavour(
                &build
                    .published
                    .dominant_stat()
                    .unwrap_or_else(|| build.gear_prefix.clone()),
                db,
            )
        });
        if let (Some(want), Some(theirs)) = (want_flavour, their_flavour) {
            if want != theirs && want != Flavour::Hybrid && theirs != Flavour::Hybrid {
                continue;
            }
        }
        // Scale is not a shade of the same job — see `Scale`. A hybrid is the
        // exception: it carries its own sustain instead of leaning on twenty
        // other people's boons, so it plays at any scale and its published
        // one says nothing about where it can go.
        let hybrid = matches!(their_flavour, Some(Flavour::Hybrid))
            || matches!(want_flavour, Some(Flavour::Hybrid));
        if !hybrid {
            if let (Some(want), Some(theirs)) = (role_scale(&shape.role), role_scale(&build.role)) {
                if theirs.self_reliance() > want.self_reliance() {
                    continue;
                }
            }
        }
        let score = closeness(shape, build, db);
        if score == 0 {
            continue;
        }
        best.entry(build.source.as_str())
            .and_modify(|held| {
                if score > held.0 {
                    *held = (score, build);
                }
            })
            .or_insert((score, build));
    }
    let mut picks: Vec<(&BenchmarkBuild, u32)> =
        best.into_values().map(|(score, b)| (b, score)).collect();
    // Closest first, whichever site it came from.
    picks.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.source.cmp(&b.0.source)));
    picks
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
        assert_eq!(
            build.source_url,
            "https://guildjen.com/heal-dps-evoker-build/"
        );
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
    /// Every role string here is one the three sites actually publish.
    #[test]
    fn a_healer_and_a_dps_are_never_the_same_job() {
        use super::{job_family, JobFamily::*};

        // The case that produced a wrong card: asked for WvW Support on a
        // Necromancer, offered GuildJen's "Roaming DPS". GuildJen publishes
        // no Necromancer WvW support build at all, so the honest answer from
        // that site is nothing.
        assert_eq!(job_family("Support"), Some(Support));
        assert_eq!(job_family("Roaming DPS"), Some(Damage));
        assert_ne!(job_family("Support"), job_family("Roaming DPS"));

        // A DPS that hands out boons is a DPS. Damage is tested first for
        // exactly this reason.
        assert_eq!(job_family("Zerg Boon DPS"), Some(Damage));
        assert_eq!(job_family("Group Boon DPS"), Some(Damage));
        // A support that does damage is a support, and `heal` and `medic`
        // are tested before damage so this stays true.
        assert_eq!(job_family("Offensive Support"), Some(Support));
        assert_eq!(job_family("Havoc Medic"), Some(Support));
        assert_eq!(job_family("Fractal Healer"), Some(Support));
        assert_eq!(job_family("Zerg Support"), Some(Support));

        assert_eq!(job_family("Roaming Bruiser"), Some(Bruiser));
        assert_eq!(job_family("Cloud Tank"), Some(Bruiser));
        // An assassin is its own job, not a DPS with better numbers: ask
        // for one and a Zerg Power DPS is the wrong answer.
        assert_eq!(job_family("Roaming Assassin"), Some(Assassin));
        assert_eq!(job_family("Havoc Assassin"), Some(Assassin));
        assert_ne!(job_family("Roaming Assassin"), job_family("Zerg Power DPS"));
        assert_eq!(job_family("Duelist"), Some(Duelist));
        assert_ne!(job_family("Duelist"), job_family("Roaming DPS"));

        // Words that say nothing about the job rule nothing out: a Roamer
        // may be any of these, and refusing to guess is not the same as
        // guessing wrong.
        assert_eq!(
            job_family("Open World Hybrid"),
            Some(Damage),
            "hybrid deals damage"
        );
        assert_eq!(
            job_family("Sidenoder"),
            Some(Duelist),
            "holds a point alone"
        );
        // These say nothing about the job and are left saying nothing: a PvP
        // Roamer may be any of them, and refusing to guess is not the same
        // as guessing wrong.
        assert_eq!(job_family("Roamer"), None);
        assert_eq!(job_family("Raid"), None);
        assert_eq!(job_family("Group Niche"), None);
        assert_eq!(job_family(""), None);
    }
}
