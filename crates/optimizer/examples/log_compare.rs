//! Compare the simulator against Elite Insights (EI) JSON logs.
//!
//! For each log: the per-player diff table (kit provenance, observables, skill
//! shares), the measured fight profile, and at the end the error bands per
//! profession, mode and observable over every log. Error size never fails
//! the run; it is what the run is for.
//!
//!   cargo run -p gw2-optimizer --example log_compare -- <log.json|dir> [--code "Char=[&..]"]...
//!   cargo run -p gw2-optimizer --example log_compare -- <log.json> --trim out.json [--players N]
//!   cargo run -p gw2-optimizer --example log_compare -- --gamedb crates/optimizer/tests/fixtures/gamedb
//!
//! A directory argument reads every `*.json` in it except `codes.json`, which
//! holds `{"<log file>": {"<character>": "<chat code>"}}` (or, per character,
//! `{"code": "<chat code>", "gear": {...}}` with stated gear); `--code` adds to it
//! for every log and wins on a clash. `--trim` parses and re-serialises one
//! log keeping only squad players (WvW: the largest group, lowest group
//! number on ties), at most N (default 10), records the untrimmed squad size
//! for the tier, and compares nothing. `--gamedb` writes the synced cache's
//! subset the fidelity budget gate needs (see [`write_gamedb`]) and compares
//! nothing.
//!
//! Exit: 0 ran and printed; 2 setup missing (dev.cfg, game cache, unreadable
//! log or bad arguments); 1 zero players compared.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::exit;

use gw2_core::types::GameMode;
use gw2_optimizer::fidelity::kit::CodeEntry;
use gw2_optimizer::fidelity::{compare, ei_log, fight_profile};
use gw2_optimizer::gamedb::GameDb;
use serde_json::Value;

type Codes = BTreeMap<String, BTreeMap<String, CodeEntry>>;

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    exit(2);
}

fn main() {
    let mut input: Option<PathBuf> = None;
    let mut cli_codes: Vec<(String, String)> = Vec::new();
    let mut trim: Option<PathBuf> = None;
    let mut players = 10usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--code" => {
                let v = args
                    .next()
                    .unwrap_or_else(|| fail("--code needs Char=[&..]"));
                let (name, code) = v
                    .split_once('=')
                    .unwrap_or_else(|| fail("--code needs Char=[&..]"));
                cli_codes.push((name.to_string(), code.to_string()));
            }
            "--trim" => {
                trim = Some(
                    args.next()
                        .unwrap_or_else(|| fail("--trim needs a path"))
                        .into(),
                )
            }
            "--gamedb" => {
                let out: PathBuf = args
                    .next()
                    .unwrap_or_else(|| fail("--gamedb needs a directory"))
                    .into();
                return write_gamedb(&out);
            }
            "--players" => {
                players = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| fail("--players needs a number"));
            }
            _ if input.is_none() => input = Some(a.into()),
            _ => fail(&format!("unexpected argument {a}")),
        }
    }
    let input = input.unwrap_or_else(|| fail("usage: log_compare <log.json|dir> [--code \"Char=[&..]\"]... [--trim out.json [--players N]]"));

    if let Some(out) = trim {
        let log = ei_log::load(&input).unwrap_or_else(|e| fail(&e));
        let trimmed = trim_log(log, players);
        let kept = trimmed.players.len();
        let json =
            serde_json::to_string(&trimmed).unwrap_or_else(|e| fail(&format!("serialize: {e}")));
        std::fs::write(&out, json)
            .unwrap_or_else(|e| fail(&format!("write {}: {e}", out.display())));
        println!("{} -> {} ({kept} players)", input.display(), out.display());
        return;
    }

    let (dir, logs) = if input.is_dir() {
        (input.clone(), json_logs(&input))
    } else {
        let dir = input.parent().map(Path::to_path_buf).unwrap_or_default();
        (dir, vec![input.clone()])
    };
    let file_codes: Codes = match std::fs::read_to_string(dir.join("codes.json")) {
        Ok(text) => {
            serde_json::from_str(&text).unwrap_or_else(|e| fail(&format!("codes.json: {e}")))
        }
        Err(_) => Codes::new(),
    };

    let addon_dir = match gw2_api::dev_config::addons_dir() {
        Ok(dir) => dir.join("gw2_build_optimizer"),
        Err(e) => fail(&format!("no addons_dir in dev.cfg ({e})")),
    };
    let corpus = gw2_optimizer::scraper::load_benchmarks(&addon_dir);
    let cache_dir = addon_dir.join("cache");
    let cache = gw2_api::cache::DataCache::new(&cache_dir);
    let db = GameDb::load(&cache).unwrap_or_else(|e| {
        fail(&format!(
            "game data not cached ({e}) — sync it in-game first"
        ))
    });
    println!(
        "{} published builds, {} skills in db, {} logs",
        corpus.len(),
        db.skills.len(),
        logs.len()
    );

    let mut all = Vec::new();
    for path in &logs {
        let log = ei_log::load(path).unwrap_or_else(|e| fail(&e));
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let mut codes: HashMap<String, CodeEntry> = file_codes
            .get(&name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        codes.extend(
            cli_codes
                .iter()
                .map(|(n, c)| (n.clone(), CodeEntry::Code(c.clone()))),
        );
        let rows = compare::compare_log(&name, &log, &codes, Some(&cache_dir), &corpus, &db);
        println!(
            "\n## {name} ({:?}, {:?}, {} squad players)\n",
            log.mode(),
            log.tier(),
            rows.len()
        );
        println!("{}", compare::render_table(&rows));
        println!(
            "{}",
            fight_profile::render(&fight_profile::extract(&log, &db))
        );
        all.extend(rows);
    }

    let compared = all.iter().filter(|r| r.refused.is_none()).count();
    println!("\n## Bands over {} logs\n", logs.len());
    println!("{}", compare::render_bands(&compare::bands(&all)));
    println!("compared {compared}/{} squad players", all.len());
    for r in all.iter().filter(|r| r.refused.is_some()) {
        println!(
            "  refused {} / {} ({}): {}",
            r.log,
            r.player,
            r.spec,
            r.refused.as_deref().unwrap_or("")
        );
    }
    if compared == 0 {
        exit(1);
    }
}

/// Every `*.json` in `dir` except `codes.json`, sorted.
fn json_logs(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| fail(&format!("read {}: {e}", dir.display())))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter(|p| p.file_name().is_some_and(|n| n != "codes.json"))
        .collect();
    paths.sort();
    paths
}

/// Squad players only; in WvW the largest group (lowest number on ties);
/// at most `n` of them. The original squad size is kept so the tier does
/// not change, and boon source maps keep only the wearer's own share, so no
/// other player's name survives. Everything the model does not read was
/// already dropped by the parse.
fn trim_log(mut log: ei_log::EiLog, n: usize) -> ei_log::EiLog {
    let mut squad: Vec<ei_log::EiPlayer> = log.squad().cloned().collect();
    log.trimmed_squad_size = Some(
        log.trimmed_squad_size
            .unwrap_or(u32::try_from(squad.len()).unwrap_or(u32::MAX)),
    );
    if log.mode() == GameMode::WvW {
        let mut sizes: BTreeMap<u32, usize> = BTreeMap::new();
        for p in &squad {
            *sizes.entry(p.group).or_default() += 1;
        }
        // BTreeMap iterates groups ascending; max_by_key keeps the last max,
        // so reverse to keep the lowest group number on a tie.
        if let Some((&g, _)) = sizes.iter().rev().max_by_key(|(_, &c)| c) {
            squad.retain(|p| p.group == g);
        }
    }
    squad.truncate(n);
    for p in &mut squad {
        let own = p.name.clone();
        for d in p.buff_uptimes.iter_mut().flat_map(|b| &mut b.buff_data) {
            for m in [&mut d.generated, &mut d.generated_presence]
                .into_iter()
                .flatten()
            {
                m.retain(|k, _| *k == own);
            }
        }
    }
    log.players = squad;
    log
}

/// The `EXPECTED_FIDELITY` rows of `tests/fidelity_logs.rs` as (profession,
/// spec, mode). Its guard test fails when a budget row's profession or a
/// kit item is missing from the written subset, so a drift here is loud.
const GAMEDB_ROWS: [(&str, &str, &str); 2] = [
    ("Necromancer", "Reaper", "PvE"),
    ("Engineer", "Mechanist", "WvW"),
];

const CATALOGS: [&str; 9] = [
    "items",
    "itemstats",
    "skills",
    "traits",
    "specializations",
    "professions",
    "legends",
    "pvp_amulets",
    "pets",
];

/// A cache file in `DataCache::save`'s field order. Rows stay raw JSON, so
/// they are copied, not re-modelled; `format` only where the cache has it.
#[derive(serde::Deserialize, serde::Serialize)]
struct Envelope {
    build: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    format: Option<u32>,
    fetched_at: String,
    data: Vec<Value>,
}

fn ids(v: &Value) -> impl Iterator<Item = u64> + '_ {
    v.as_array().into_iter().flatten().filter_map(Value::as_u64)
}

/// Writes the part of the synced cache the fidelity budget gate reads, as
/// cache files `GameDb::load` reads unchanged: the budget rows' professions
/// whole (their specializations' traits; their skills, the skills their
/// traits and the rows' logs name, closed over flips, chains, toolbelts,
/// transforms and bundles), only the items the rows' kits name, and the small
/// catalogs whole. Kits are built as the test builds them: fixture logs,
/// `codes.json`, fixture corpus, no account cache.
fn write_gamedb(out: &Path) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let ei = fixtures.join("ei_logs");
    let read = |p: &Path| {
        std::fs::read_to_string(p).unwrap_or_else(|e| fail(&format!("read {}: {e}", p.display())))
    };
    let cache_dir = gw2_api::dev_config::cache_dir()
        .unwrap_or_else(|e| fail(&format!("no addons_dir in dev.cfg ({e})")));
    let db = GameDb::load(&gw2_api::cache::DataCache::new(&cache_dir)).unwrap_or_else(|e| {
        fail(&format!(
            "game data not cached ({e}) — sync it in-game first"
        ))
    });
    let corpus = gw2_optimizer::scraper::load_benchmarks_from(&fixtures.join("benchmarks"));
    let codes: Codes = serde_json::from_str(&read(&ei.join("codes.json")))
        .unwrap_or_else(|e| fail(&format!("codes.json: {e}")));

    let mut kit_items: BTreeSet<u64> = BTreeSet::new();
    let mut seeds: BTreeSet<u64> = BTreeSet::new();
    let mut found: BTreeSet<(&str, &str, &str)> = BTreeSet::new();
    for path in json_logs(&ei) {
        let log = ei_log::load(&path).unwrap_or_else(|e| fail(&e));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let for_log: HashMap<String, CodeEntry> = codes
            .get(&name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        for r in compare::compare_log(&name, &log, &for_log, None, &corpus, &db) {
            let key = (r.profession.as_str(), r.spec.as_str(), r.mode.as_str());
            let Some(&row) = GAMEDB_ROWS.iter().find(|k| **k == key) else {
                continue;
            };
            let kit = r.kit.as_ref().unwrap_or_else(|| {
                fail(&format!("{name} / {}: no kit ({:?})", r.player, r.refused))
            });
            kit_items.extend(kit.item_ids().into_iter().map(u64::from));
            found.insert(row);
            let log_skills = log.skill_map.keys();
            seeds.extend(log_skills.filter_map(|k| k.strip_prefix('s')?.parse::<u64>().ok()));
        }
    }
    if let Some(missing) = GAMEDB_ROWS.iter().find(|k| !found.contains(*k)) {
        fail(&format!("no fixture squad player is {missing:?}"));
    }

    let mut raw: BTreeMap<&str, Envelope> = CATALOGS
        .iter()
        .map(|&k| {
            let text = read(&cache_dir.join(format!("{k}.json")));
            let env = serde_json::from_str(&text).unwrap_or_else(|e| fail(&format!("{k}: {e}")));
            (k, env)
        })
        .collect();
    let profs: BTreeSet<&str> = GAMEDB_ROWS.iter().map(|r| r.0).collect();
    let prof_of = |v: &Value, field: &str| profs.contains(v[field].as_str().unwrap_or(""));
    let specs: BTreeSet<u64> = raw["specializations"]
        .data
        .iter()
        .filter(|s| prof_of(s, "profession"))
        .filter_map(|s| s["id"].as_u64())
        .collect();
    let in_specs = |t: &Value| specs.contains(&t["specialization"].as_u64().unwrap_or(0));
    for p in raw["professions"].data.iter().filter(|p| prof_of(p, "id")) {
        for pair in p["skills_by_palette"].as_array().into_iter().flatten() {
            seeds.extend(ids(pair).skip(1));
        }
        for w in p["weapons"]
            .as_object()
            .into_iter()
            .flat_map(|o| o.values())
        {
            let skills = w["skills"].as_array().into_iter().flatten();
            seeds.extend(skills.filter_map(|s| s["id"].as_u64()));
        }
        for t in p["training"].as_array().into_iter().flatten() {
            let track = t["track"].as_array().into_iter().flatten();
            seeds.extend(track.filter_map(|s| s["skill_id"].as_u64()));
        }
    }
    for s in &raw["skills"].data {
        if s["professions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|p| profs.contains(p.as_str().unwrap_or("")))
        {
            seeds.extend(s["id"].as_u64());
        }
    }
    for t in raw["traits"].data.iter().filter(|t| in_specs(t)) {
        let skills = t["skills"].as_array().into_iter().flatten();
        seeds.extend(skills.filter_map(|s| s["id"].as_u64()));
    }
    let by_id: HashMap<u64, &Value> = raw["skills"]
        .data
        .iter()
        .filter_map(|s| Some((s["id"].as_u64()?, s)))
        .collect();
    let mut skills: BTreeSet<u64> = BTreeSet::new();
    let mut stack: Vec<u64> = seeds.into_iter().collect();
    while let Some(id) = stack.pop() {
        if !skills.insert(id) {
            continue;
        }
        let Some(s) = by_id.get(&id) else { continue };
        for k in ["flip_skill", "next_chain", "prev_chain", "toolbelt_skill"] {
            stack.extend(s[k].as_u64());
        }
        for k in ["transform_skills", "bundle_skills"] {
            stack.extend(ids(&s[k]));
        }
    }
    let traits: BTreeSet<u64> = raw["traits"]
        .data
        .iter()
        .filter(|t| in_specs(t))
        .filter_map(|t| t["id"].as_u64())
        .collect();
    let id = |v: &Value| v["id"].as_u64().unwrap_or(0);
    for (key, env) in raw.iter_mut() {
        match *key {
            "professions" => env.data.retain(|p| prof_of(p, "id")),
            "skills" => env.data.retain(|s| skills.contains(&id(s))),
            "traits" => env.data.retain(|t| traits.contains(&id(t))),
            "items" => env.data.retain(|i| kit_items.contains(&id(i))),
            _ => {}
        }
    }

    // Kit items the addon cache keep rule filters out (by type and rarity:
    // Fine infusions, Jade Bot cores). They are real items; the guard allows exactly these.
    let kept: BTreeSet<u64> = raw["items"].data.iter().map(id).collect();
    let uncached: Vec<u64> = kit_items.difference(&kept).copied().collect();

    std::fs::create_dir_all(out)
        .unwrap_or_else(|e| fail(&format!("create {}: {e}", out.display())));
    let files = raw
        .iter()
        .map(|(key, env)| (format!("{key}.json"), serde_json::to_string(env)))
        .chain([(
            "uncached_kit_items.json".into(),
            serde_json::to_string(&uncached),
        )]);
    for (file, json) in files {
        let json = json.unwrap_or_else(|e| fail(&format!("{file}: {e}")));
        let path = out.join(&file);
        std::fs::write(&path, &json)
            .unwrap_or_else(|e| fail(&format!("write {}: {e}", path.display())));
        println!("{file}: {} bytes", json.len());
    }
    let rows: Vec<String> = raw
        .iter()
        .map(|(k, env)| format!("{k} {}", env.data.len()))
        .collect();
    println!("build {}; rows: {}", raw["skills"].build, rows.join(", "));
    println!(
        "kit-named items {}, filtered out by the cache keep rule {uncached:?}",
        kit_items.len()
    );
}
