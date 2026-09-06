//! Show which published builds a proposal would be matched with.
//!
//! Fixtures prove the shape; only the player's own synced corpus proves the
//! matching. Reads the real benchmarks folder from `dev.cfg`.
//!
//!   cargo run -p gw2-optimizer --example closest_builds -- Guardian WvW "Support"

use gw2_optimizer::benchmark::{closest_per_source, BuildShape};

fn main() {
    let mut args = std::env::args().skip(1);
    let profession = args.next().unwrap_or_else(|| "Guardian".into());
    let mode = args.next().unwrap_or_else(|| "WvW".into());
    let role = args.next().unwrap_or_else(|| "Support".into());
    let specs: Vec<String> = args.next().map(split_list).unwrap_or_default();
    let weapons: Vec<String> = args.next().map(split_list).unwrap_or_default();

    let addon_dir = match gw2_api::dev_config::addons_dir() {
        Ok(dir) => dir.join("gw2_build_optimizer"),
        Err(e) => {
            eprintln!("no addons_dir in dev.cfg ({e}) — copy dev.cfg.example and set it");
            std::process::exit(2);
        }
    };
    let builds = gw2_optimizer::scraper::load_benchmarks(&addon_dir);
    println!("{} benchmark rows from {}", builds.len(), addon_dir.display());

    let cache = gw2_api::cache::DataCache::new(&addon_dir.join("cache"));
    let db = match gw2_optimizer::gamedb::GameDb::load(&cache) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("game data not cached yet ({e}) — sync it in-game first");
            std::process::exit(2);
        }
    };

    let shape = BuildShape {
        profession: profession.clone(),
        mode: mode.clone(),
        specs,
        weapons,
        role: role.clone(),
        ..Default::default()
    };
    println!("looking for: {profession} · {mode} · {role}");
    println!("  specs   : {:?}", shape.specs);
    println!("  weapons : {:?}", shape.weapons);
    println!();

    for (build, score) in closest_per_source(&builds, &shape, &db) {
        println!(
            "{:<10} score {:>3}  {:<26} {:<22} {}",
            build.source, score, build.role, build.gear_prefix, build.spec_name
        );
        println!("           {}", build.source_url);
        let weapons: Vec<&str> = build
            .published
            .gear
            .iter()
            .map(|g| g.slot.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        println!("           gear rows: {weapons:?}");
    }
}

fn split_list(raw: String) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
