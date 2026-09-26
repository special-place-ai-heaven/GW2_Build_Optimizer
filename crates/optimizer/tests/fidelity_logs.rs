//! The Elite Insights (EI) log fixtures, and the simulator's fidelity against
//! them.
//!
//! Plain CI: every fixture parses and is a fixed point of parse then
//! serialize, `codes.json` names real fixture players, the fight profile
//! stays print-only, and the fidelity budget gate runs on the checked-in
//! GameDb subset (`tests/fixtures/gamedb`) and fixture corpus. The
//! `#[ignore]` tests need the synced game data: compare coverage over every
//! squad player, and the parity check that the subset still reproduces the
//! synced cache (run it before a release).
//!
//!   cargo test -p gw2-optimizer --test fidelity_logs -- --ignored

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use gw2_api::cache::DataCache;
use gw2_optimizer::benchmark::BenchmarkBuild;
use gw2_optimizer::fidelity::compare::{self, PlayerComparison};
use gw2_optimizer::fidelity::ei_log::{self, EiLog};
use gw2_optimizer::fidelity::kit::CodeEntry;
use gw2_optimizer::gamedb::GameDb;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ei_logs");

type Codes = BTreeMap<String, BTreeMap<String, CodeEntry>>;

fn fixtures() -> Vec<(String, EiLog, String)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(FIXTURES)
        .expect("fixture dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter(|p| p.file_name().is_some_and(|n| n != "codes.json"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 3, "expected >= 3 EI fixtures in {FIXTURES}");
    paths
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).expect("read fixture");
            let log =
                ei_log::parse(&text).unwrap_or_else(|e| panic!("{} parses: {e}", p.display()));
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, log, text)
        })
        .collect()
}

fn codes() -> Codes {
    let text = std::fs::read_to_string(Path::new(FIXTURES).join("codes.json")).expect("codes.json");
    serde_json::from_str(&text).expect("codes.json parses")
}

#[test]
fn every_fixture_parses_and_is_a_fixed_point() {
    for (name, log, text) in fixtures() {
        assert!(log.squad().count() > 0, "{name} has no squad players");
        let out = serde_json::to_string(&log).expect("serializes");
        assert_eq!(out, text.trim_end(), "{name} is not a fixed point");
    }
}

#[test]
fn codes_name_players_that_exist_in_their_fixture() {
    let logs: BTreeMap<String, EiLog> = fixtures().into_iter().map(|(n, l, _)| (n, l)).collect();
    for (file, by_name) in codes() {
        let log = logs
            .get(&file)
            .unwrap_or_else(|| panic!("codes.json names {file}, which is not a fixture"));
        for (name, entry) in by_name {
            let code = entry.code();
            assert!(
                log.squad().any(|p| p.name == name),
                "codes.json: {file} has no squad player {name}"
            );
            assert!(
                code.starts_with("[&") && code.ends_with(']'),
                "codes.json: {file} / {name} is not a chat code"
            );
        }
    }
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                rs_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The fight profile is measured and printed in this increment, never
/// consumed: nothing outside `src/fidelity/` may name the type.
#[test]
fn nothing_outside_fidelity_reads_a_fight_profile() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = manifest.parent().expect("crates dir");
    let fidelity = manifest.join("src").join("fidelity");
    let this_file = manifest.join("tests").join("fidelity_logs.rs");
    let mut files = Vec::new();
    rs_files(crates, &mut files);
    assert!(files.len() > 50, "walked only {} .rs files", files.len());
    let offenders: Vec<String> = files
        .iter()
        .filter(|p| !p.starts_with(&fidelity) && **p != this_file)
        .filter(|p| std::fs::read_to_string(p).is_ok_and(|t| t.contains("FightProfile")))
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "FightProfile read outside fidelity: {offenders:?}"
    );
}

/// (profession, spec, mode, observable, max p90 |error|, reason), keyed like
/// `compare::bands`: spec is the log's elite spec or core profession name.
///
/// Each budget is a cache p90 measured by
/// `no_observable_exceeds_its_fidelity_budget` on 2026-09-25 (1.14.50 plus
/// FCR-025), both rows n=1, rounded up by a margin inside [`stale_slack`].
/// They are ratchet ceilings on today's engine, not accuracy claims. The
/// gate runs in plain CI on the checked-in cache subset, frozen at a game
/// build; `fixture_gamedb_reproduces_the_synced_cache` (`--ignored`) proves
/// the subset still gives the synced cache's numbers. `skill_share` / boon uptimes /
/// WvW DPS stay out: the documented ranges are too wide to ratchet.
/// `kent_fidelity_fixture_budgets_ratchet` fails if this table is emptied.
const EXPECTED_FIDELITY: &[(&str, &str, &str, &str, f64, &str)] = &[
    (
        "Necromancer",
        "Reaper",
        "PvE",
        "condi_fraction",
        0.0015,
        "measured p90 0.00121 (n=1, golem fixture), 2026-09-25; was run-3 median 0.003",
    ),
    (
        "Engineer",
        "Mechanist",
        "WvW",
        "condi_fraction",
        0.055,
        "measured ratchet ceiling, not accuracy: p90 0.0489 (n=1, aBtd Joe Wvw), 2026-09-25; run-3 median 0.005 no longer holds",
    ),
];

/// Ratchet slack, relative: a budget more than this share of itself above
/// the measured p90 is stale. An absolute slack exceeded these small
/// budgets, so the check could never fire.
const STALE_BY: f64 = 0.5;
/// Floor under the slack so a tiny budget does not flap on rounding.
const STALE_FLOOR: f64 = 0.0005;

fn stale_slack(budget: f64) -> f64 {
    (STALE_BY * budget).max(STALE_FLOOR)
}

const PROFESSIONS: [&str; 9] = [
    "Elementalist",
    "Engineer",
    "Guardian",
    "Mesmer",
    "Necromancer",
    "Ranger",
    "Revenant",
    "Thief",
    "Warrior",
];

#[test]
fn the_fidelity_budget_table_is_well_formed() {
    assert!(
        !EXPECTED_FIDELITY.is_empty(),
        "EXPECTED_FIDELITY must not ship empty — budget and well-formed tests are otherwise vacuously green"
    );
    let modes: Vec<String> = fixtures()
        .iter()
        .map(|(_, l, _)| format!("{:?}", l.mode()))
        .collect();
    let mut seen: Vec<(&str, &str, &str, &str)> = Vec::new();
    for &(profession, spec, mode, observable, budget, reason) in EXPECTED_FIDELITY {
        let key = (profession, spec, mode, observable);
        assert!(
            PROFESSIONS.contains(&profession),
            "EXPECTED_FIDELITY names {profession}, not a profession"
        );
        assert!(
            !spec.trim().is_empty(),
            "EXPECTED_FIDELITY: {key:?} has no spec"
        );
        assert!(
            modes.iter().any(|m| m == mode),
            "EXPECTED_FIDELITY names {mode}, which no fixture has"
        );
        assert!(
            !observable.is_empty(),
            "EXPECTED_FIDELITY: {key:?} has no observable"
        );
        assert!(
            budget.is_finite() && budget > 0.0,
            "EXPECTED_FIDELITY: {key:?} budgets {budget}"
        );
        assert!(
            budget - stale_slack(budget) > 0.0,
            "EXPECTED_FIDELITY: {key:?} staleness threshold is not positive, so the ratchet cannot fire"
        );
        assert!(
            !reason.trim().is_empty(),
            "EXPECTED_FIDELITY: {key:?} has no reason"
        );
        assert!(
            !seen.contains(&key),
            "EXPECTED_FIDELITY: {key:?} appears twice"
        );
        seen.push(key);
    }
}

/// Log-side burst observables of the golem fixture, facts of the file.
/// Python over `damage1S[0]`: per-second = [s1, s2 - s1, ...] (95 seconds,
/// 4 030 026 total); best 5 s sum 285 462, best 10 s sum 515 818. The
/// fixture was re-downloaded and re-trimmed 2026-09-23 to keep `states`,
/// `conditionDamage1S` and the `dpsAll` actor split, so overlap, condition
/// share and ramp are now measured; these are the values `log_compare`
/// reports for this log, pinned as facts of the file.
#[test]
fn golem_fixture_burst_observables_are_facts_of_the_file() {
    let (_, log, _) = fixtures()
        .into_iter()
        .find(|(n, _, _)| n == "1f33-20260720-163045_golem.json")
        .expect("golem fixture");
    let p = log.squad().next().expect("one player");
    let o = compare::observe(&log, p, &GameDb::empty_for_tests());
    let close = |a: Option<f64>, b: f64| {
        let a = a.expect("measured");
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    };
    close(o.burst_peak_5s, 285_462.0 / 5.0);
    close(o.burst_peak_10s, 515_818.0 / 10.0);
    assert!(o.burst_peak_5s > Some(o.dps_engaged));
    close(o.burst_overlap_share, 1.0);
    close(o.condition_share, 0.0029288346379509602);
    close(o.condition_ramp_s, 11.0);
    // Greatsword autos are absent. Shroud autos are Life Rend + Life Slash
    // + Life Reap (241416 + 274771 + 314473) / 3_966_424.
    let gs_auto = ["Dusk Strike", "Fading Twilight", "Chilling Scythe"]
        .into_iter()
        .map(|n| o.skill_share.get(n).copied().unwrap_or(0.0))
        .sum::<f64>();
    assert_eq!(gs_auto, 0.0, "golem Reaper casts no greatsword auto");
    let shroud_auto = ["Life Rend", "Life Slash", "Life Reap"]
        .into_iter()
        .map(|n| o.skill_share.get(n).copied().unwrap_or(0.0))
        .sum::<f64>();
    close(Some(shroud_auto), 830_660.0 / 3_966_424.0);
    // dpsAll[0] condiDamage / damage; distinct from actor condition_share.
    let condi = o.condi_fraction;
    let condi_file = 11_617.0 / 4_030_026.0;
    assert!((condi - condi_file).abs() < 1e-9, "{condi} != {condi_file}");
}

/// Fail-closed: emptying `EXPECTED_FIDELITY` is a CI lie. Every budgeted
/// spec+mode is a committed fixture, and log-side `condi_fraction` for those
/// rows is a fact of the file (not a p90 we did not measure).
#[test]
fn kent_fidelity_fixture_budgets_ratchet() {
    assert!(
        !EXPECTED_FIDELITY.is_empty(),
        "EXPECTED_FIDELITY must stay seeded; empty table makes compare/budget tests vacuously green"
    );
    let logs = fixtures();
    for &(_profession, spec, mode, _observable, _budget, _reason) in EXPECTED_FIDELITY {
        let present = logs.iter().any(|(_, log, _)| {
            format!("{:?}", log.mode()) == mode && log.squad().any(|p| p.profession == spec)
        });
        assert!(
            present,
            "EXPECTED_FIDELITY names {spec} · {mode}, which no fixture squad has"
        );
    }

    let db = GameDb::empty_for_tests();
    let close = |a: f64, b: f64| {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    };
    let observe = |file: &str, name: &str| {
        let (_, log, _) = logs
            .iter()
            .find(|(n, _, _)| n == file)
            .unwrap_or_else(|| panic!("{file}"));
        let p = log
            .squad()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{file} has no {name}"));
        compare::observe(log, p, &db)
    };
    close(
        observe("1f33-20260720-163045_golem.json", "Aisxka").condi_fraction,
        11_617.0 / 4_030_026.0,
    );
    close(
        observe("aBtd-20260604-211449_wvw.json", "Joe Wvw").condi_fraction,
        2_495.0 / 39_478.0,
    );
}

fn cached_db() -> GameDb {
    let cache = gw2_api::cache::DataCache::new(
        gw2_api::dev_config::cache_dir().expect("dev.cfg with addons_dir"),
    );
    GameDb::load(&cache).expect("game data cached \u{2014} sync it in-game first")
}

/// The synced cache's subset the budget gate reads, frozen at a game build.
const GAMEDB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/gamedb");
const BENCHMARKS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/benchmarks");
const REGENERATE: &str = "cargo run -p gw2-optimizer --example log_compare -- --gamedb crates/optimizer/tests/fixtures/gamedb";

fn fixture_db() -> GameDb {
    GameDb::load(&DataCache::new(GAMEDB))
        .unwrap_or_else(|e| panic!("{GAMEDB}: {e}; regenerate: {REGENERATE}"))
}

fn fixture_corpus() -> Vec<BenchmarkBuild> {
    let corpus = gw2_optimizer::scraper::load_benchmarks_from(Path::new(BENCHMARKS));
    assert!(!corpus.is_empty(), "fixture corpus {BENCHMARKS}");
    corpus
}

/// The synced corpus lives in the addon's directory, beside its cache.
fn synced_corpus() -> Vec<BenchmarkBuild> {
    let cache = gw2_api::dev_config::cache_dir().expect("dev.cfg with addons_dir");
    let corpus = gw2_optimizer::scraper::load_benchmarks(cache.parent().expect("addon dir"));
    assert!(!corpus.is_empty(), "synced benchmark corpus");
    corpus
}

/// Does an `EXPECTED_FIDELITY` row budget this player?
fn budgeted(r: &PlayerComparison) -> bool {
    EXPECTED_FIDELITY.iter().any(|&(p, s, m, ..)| {
        (p, s, m) == (r.profession.as_str(), r.spec.as_str(), r.mode.as_str())
    })
}

/// Every fixture compared, codes from `codes.json`.
fn compare_all(db: &GameDb, corpus: &[BenchmarkBuild]) -> Vec<PlayerComparison> {
    let codes = codes();
    fixtures()
        .into_iter()
        .flat_map(|(name, log, _)| {
            let for_log: HashMap<String, CodeEntry> = codes
                .get(&name)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            compare::compare_log(&name, &log, &for_log, None, corpus, db)
        })
        .collect()
}

#[test]
#[ignore = "needs a synced game-data cache; see dev.cfg"]
fn every_fixture_squad_player_is_compared_or_named() {
    let rows = compare_all(&cached_db(), &synced_corpus());
    // Compared = reconstructed, validator-clean and simulated. A failed
    // blocking gate is a finding about the kit, not a refusal to compare.
    let compared = rows.iter().filter(|r| r.refused.is_none()).count();
    for r in rows
        .iter()
        .filter(|r| r.refused.is_none() && !r.gates.is_empty())
    {
        println!(
            "compared, gates failed {} / {} ({}): {}",
            r.log,
            r.player,
            r.spec,
            r.gates.join(", ")
        );
    }
    for r in rows.iter().filter(|r| r.refused.is_some()) {
        println!(
            "refused {} / {} ({}): {}",
            r.log,
            r.player,
            r.spec,
            r.refused.as_deref().unwrap_or("")
        );
    }
    println!("compared {compared}/{}", rows.len());
    assert!(
        compared * 5 >= rows.len() * 4,
        "compared {compared}/{} < 80%",
        rows.len()
    );
}

/// A stale subset fails here by name (profession, player, missing ids), not
/// in the gate as "nothing measured" or a moved number.
#[test]
fn fixture_gamedb_holds_every_id_the_budget_kits_name() {
    let db = fixture_db();
    let rows = compare_all(&db, &fixture_corpus());
    // Real kit items the addon cache keep rule filters out, as the generator
    // found them; the synced cache lacks them too.
    let uncached: Vec<u32> = serde_json::from_str(
        &std::fs::read_to_string(Path::new(GAMEDB).join("uncached_kit_items.json"))
            .unwrap_or_else(|e| panic!("{GAMEDB}: {e}; regenerate: {REGENERATE}")),
    )
    .expect("uncached_kit_items.json is a list of ids");
    for &(profession, spec, mode, ..) in EXPECTED_FIDELITY {
        assert!(
            db.professions.contains_key(profession),
            "{GAMEDB} has no {profession}; regenerate: {REGENERATE}"
        );
        assert!(
            rows.iter().any(
                |r| (r.profession.as_str(), r.spec.as_str(), r.mode.as_str())
                    == (profession, spec, mode)
            ),
            "no fixture squad player is {profession} · {spec} · {mode}"
        );
    }
    for r in rows.iter().filter(|r| budgeted(r)) {
        let kit = r.kit.as_ref().unwrap_or_else(|| {
            panic!(
                "{} / {}: no kit on {GAMEDB} ({:?}); regenerate: {REGENERATE}",
                r.log, r.player, r.refused
            )
        });
        let specs = kit.build.published.specs.iter().map(|l| l.id);
        let missing = [
            (
                "specializations",
                specs
                    .filter(|id| !db.specializations.contains_key(id))
                    .collect(),
            ),
            (
                "traits",
                kit.trait_ids(&db)
                    .into_iter()
                    .filter(|id| !db.traits.contains_key(id))
                    .collect(),
            ),
            (
                "skills",
                kit.skill_ids()
                    .into_iter()
                    .filter(|id| !db.skills.contains_key(id))
                    .collect(),
            ),
            (
                "items",
                kit.item_ids()
                    .into_iter()
                    .filter(|id| !db.items.contains_key(id) && !uncached.contains(id))
                    .collect::<Vec<u32>>(),
            ),
        ];
        for (what, ids) in missing {
            assert!(
                ids.is_empty(),
                "{} / {}: kit {what} {ids:?} not in {GAMEDB}; regenerate: {REGENERATE}",
                r.log,
                r.player
            );
        }
    }
}

/// Runs in plain CI on the checked-in subset; the parity test below runs
/// the same gate on the synced cache.
#[test]
fn no_observable_exceeds_its_fidelity_budget() {
    assert_within_budgets(&compare_all(&fixture_db(), &fixture_corpus()));
}

/// The subset is frozen at the build it was cut from. After a re-sync or an
/// engine change that reads records outside it, the synced cache and the
/// subset part ways; this catches that. Run it with the release checklist.
#[test]
#[ignore = "needs a synced game-data cache; see dev.cfg"]
fn fixture_gamedb_reproduces_the_synced_cache() {
    let corpus = fixture_corpus();
    let synced = compare_all(&cached_db(), &corpus);
    assert_within_budgets(&synced);
    let budget_rows = |rows: &[PlayerComparison]| -> Vec<_> {
        rows.iter()
            .filter(|r| budgeted(r))
            .map(|r| {
                let rest = (r.unmapped_share, r.gates.clone(), r.refused.clone());
                (
                    r.log.clone(),
                    r.player.clone(),
                    r.diffs.clone(),
                    r.share.clone(),
                    rest,
                )
            })
            .collect()
    };
    let cache = DataCache::new(gw2_api::dev_config::cache_dir().expect("dev.cfg"));
    assert_eq!(
        budget_rows(&compare_all(&fixture_db(), &corpus)),
        budget_rows(&synced),
        "subset (build {:?}) and synced cache (build {:?}) disagree: regenerate ({REGENERATE}), then re-measure the budgets",
        DataCache::new(GAMEDB).cached_build("skills"),
        cache.cached_build("skills"),
    );
}

fn assert_within_budgets(rows: &[PlayerComparison]) {
    for r in rows {
        let key = (r.profession.as_str(), r.spec.as_str(), r.mode.as_str());
        for d in &r.diffs {
            if EXPECTED_FIDELITY
                .iter()
                .any(|&(p, s, m, o, ..)| ((p, s, m), o) == (key, d.observable))
            {
                println!(
                    "{} / {} {}: error {:?}",
                    r.log, r.player, d.observable, d.error
                );
            }
        }
    }
    let bands = compare::bands(rows);
    let mut failures = Vec::new();
    for &(profession, spec, mode, observable, budget, _) in EXPECTED_FIDELITY {
        let key = (
            profession.to_string(),
            spec.to_string(),
            mode.to_string(),
            observable,
        );
        match bands.get(&key).filter(|b| b.n > 0) {
            None => failures.push(format!(
                "{profession} · {spec} · {mode} · {observable}: budgeted {budget} but nothing measured"
            )),
            Some(b) if b.p90_abs > budget => failures.push(format!(
                "{profession} · {spec} · {mode} · {observable}: p90 {:.3} over budget {budget}",
                b.p90_abs
            )),
            Some(b) if b.p90_abs < budget - stale_slack(budget) => failures.push(format!(
                "{profession} · {spec} · {mode} · {observable}: p90 {:.4} under budget {budget} by more than {:.4} — ratchet it down",
                b.p90_abs,
                stale_slack(budget)
            )),
            Some(_) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{}\n\nmeasured now:\n{}",
        failures.join("\n"),
        compare::render_bands(&bands)
    );
}
