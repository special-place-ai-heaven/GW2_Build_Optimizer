//! Run every synced community build through our own viability gates.
//!
//! These builds are the ground truth. People play them, sites publish them,
//! and they demonstrably work — so a gate that fails them is describing our
//! model, not the build. This prints the pass rate per gate so a
//! miscalibrated one is a number rather than an argument.
//!
//!   cargo run -p gw2-optimizer --example calibrate_viability
//!   cargo run -p gw2-optimizer --example calibrate_viability -- WvW

use std::collections::BTreeMap;

use gw2_optimizer::benchmark::BenchmarkBuild;
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::prompts::GeminiBuildResponse;
use gw2_optimizer::scenario::{CombatKind, CombatTier, ScenarioSpec};

fn main() {
    let only_mode = std::env::args().nth(1);

    let addon_dir = match gw2_api::dev_config::addons_dir() {
        Ok(dir) => dir.join("gw2_build_optimizer"),
        Err(e) => {
            eprintln!("no addons_dir in dev.cfg ({e})");
            std::process::exit(2);
        }
    };
    let builds = gw2_optimizer::scraper::load_benchmarks(&addon_dir);
    let cache = gw2_api::cache::DataCache::new(addon_dir.join("cache"));
    let db = match GameDb::load(&cache) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("game data not cached ({e}) — sync it in-game first");
            std::process::exit(2);
        }
    };
    println!(
        "{} synced builds, {} skills, {} palette entries in db",
        builds.len(),
        db.skills.len(),
        db.palette_to_skill.len()
    );
    {
        let mut have_code = 0;
        let mut decoded = 0;
        let mut resolved = 0;
        let mut marked = 0;
        for b in &builds {
            if !b.published.skill_ids.is_empty() {
                marked += 1;
            }
            let Some(code) = b.published.build_code.as_deref() else {
                continue;
            };
            have_code += 1;
            let Some(t) = gw2_optimizer::build_template::decode(code) else {
                continue;
            };
            decoded += 1;
            if t.skills.iter().any(|pid| {
                *pid != 0
                    && db
                        .palette_to_skill
                        .get(pid)
                        .is_some_and(|id| db.skills.contains_key(id))
            }) {
                resolved += 1;
            }
        }
        println!(
            "  skills marked up on {marked}; chat code on {have_code}, decodes {decoded},              palette resolves for {resolved}"
        );
    }

    // gate -> (passed, failed); plus the first few failing notes per gate so a
    // number can be chased back to a build.
    let mut tally: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut by_role: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut unusable = 0u32;
    let mut no_plate = 0u32;
    let mut bad_examples: Vec<String> = Vec::new();
    let mut scored = 0u32;

    for build in &builds {
        if let Some(ref want) = only_mode {
            if !build.mode.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let Some(plate) = plate_from(build, &db) else {
            no_plate += 1;
            continue;
        };
        let validated =
            gw2_optimizer::validation::validate_gemini_build(&plate, &db, &build.profession);
        if !validated.errors.is_empty() {
            unusable += 1;
            if bad_examples.len() < 5 {
                bad_examples.push(format!(
                    "{} {} [{}] {}",
                    build.source, build.profession, build.role, validated.errors[0].detail
                ));
            }
            continue;
        }
        let scenario = scenario_for(build);
        let weights = gw2_optimizer::scoring::OptimizationWeights::default();
        let ctx = gw2_optimizer::balance::BalanceContext::new(scenario.game_mode.clone());
        let report = gw2_optimizer::referee::evaluate_validated_build(
            &validated,
            &db,
            &build.profession,
            &weights,
            &ctx,
            &scenario,
        );
        scored += 1;

        let mut all_passed = true;
        for gate in &report.viability.gates {
            let key = format!("{:?}", gate.gate);
            let slot = tally.entry(key.clone()).or_default();
            if gate.passed {
                slot.0 += 1;
            } else {
                slot.1 += 1;
                all_passed = false;
                let notes = examples.entry(key).or_default();
                if notes.len() < 3 {
                    notes.push(format!(
                        "{} {} [{}] {}",
                        build.source, build.profession, build.role, gate.note
                    ));
                }
            }
        }
        let role = by_role.entry(build.role.clone()).or_default();
        if all_passed {
            role.0 += 1;
        } else {
            role.1 += 1;
        }
    }

    println!(
        "scored {scored}; skipped {no_plate} with no three-spec plate, \
         {unusable} that failed validation"
    );
    for note in &bad_examples {
        println!("   rejected: {note}");
    }
    println!();
    println!("{:<24} {:>7} {:>7} {:>7}", "gate", "pass", "fail", "pass%");
    for (gate, (pass, fail)) in &tally {
        let total = pass + fail;
        let pct = if total > 0 {
            *pass as f64 * 100.0 / total as f64
        } else {
            0.0
        };
        println!("{gate:<24} {pass:>7} {fail:>7} {pct:>6.0}%");
    }

    println!("\nwhole-build pass rate by published role (worst first):");
    let mut roles: Vec<_> = by_role.into_iter().collect();
    roles.sort_by_key(|(_, (p, f))| {
        let total = p + f;
        if total == 0 {
            100
        } else {
            (*p as i64 * 100) / total as i64
        }
    });
    for (role, (pass, fail)) in roles.iter().take(14) {
        let total = pass + fail;
        let pct = if total > 0 {
            *pass as f64 * 100.0 / total as f64
        } else {
            0.0
        };
        println!("  {role:<28} {pass:>4}/{total:<4} {pct:>5.0}%");
    }

    println!("\nfailing examples:");
    for (gate, notes) in &examples {
        println!("  == {gate}");
        for note in notes {
            println!("     {note}");
        }
    }
}

/// A published build as a plate, in the same shape the model answers in.
fn plate_from(build: &BenchmarkBuild, db: &GameDb) -> Option<GeminiBuildResponse> {
    let p = &build.published;
    if p.specs.is_empty() {
        return None;
    }
    let specializations: Vec<(String, Vec<String>)> = p
        .specs
        .iter()
        .filter_map(|line| {
            let spec = db.specializations.get(&line.id)?;
            let traits: Vec<String> = line
                .trait_ids
                .iter()
                .filter_map(|id| db.traits.get(id).map(|t| t.name.clone()))
                .collect();
            Some((spec.name.clone(), traits))
        })
        .collect();
    if specializations.len() != 3 {
        return None;
    }
    let name = |id: Option<u32>| {
        id.and_then(|id| db.items.get(&id))
            .map(|i| i.name.clone())
            .unwrap_or_default()
    };
    const WEAPONS: [&str; 17] = [
        "axe",
        "dagger",
        "mace",
        "pistol",
        "scepter",
        "sword",
        "focus",
        "shield",
        "torch",
        "warhorn",
        "greatsword",
        "hammer",
        "longbow",
        "rifle",
        "shortbow",
        "staff",
        "spear",
    ];
    let weapons: Vec<String> = p
        .gear
        .iter()
        .map(|g| g.slot.clone())
        .filter(|s| WEAPONS.contains(&s.to_lowercase().as_str()))
        .collect();
    // Heal, three utilities and elite. Sites vary on whether they mark these
    // up at all — GuildJen mostly does not — but nearly every page publishes
    // a chat code, and the code carries them as palette ids. Weapons decide
    // skills 1-5 and are resolved from the profession; 6-0 are chosen, and
    // this is where the choice is written down.
    let skills = published_skills(p, db);
    Some(GeminiBuildResponse {
        specializations,
        weapons,
        skills,
        rune: name(p.rune_id),
        sigils: p
            .sigil_ids
            .iter()
            .filter_map(|id| db.items.get(id).map(|i| i.name.clone()))
            .collect(),
        relic: name(p.relic_id),
        stat_prefix: p
            .dominant_stat()
            .unwrap_or_else(|| build.gear_prefix.clone()),
        ..Default::default()
    })
}

/// The slot bar, labelled the way a plate labels it.
///
/// `validation::parse_skill_names_from_response` reads `Heal: `, `Utils: `
/// and `Elite: ` prefixes rather than a bare list, so this has to speak the
/// same shape. Where the ids come from is `ProviderBuild::slot_skills`.
fn published_skills(p: &gw2_optimizer::providers::ProviderBuild, db: &GameDb) -> Vec<String> {
    let slots = p.slot_skills(db);
    let name = |slot: Option<&Option<u32>>| {
        slot.and_then(|s| *s)
            .and_then(|id| db.skills.get(&id))
            .map(|s| s.name.clone())
    };
    let mut lines = Vec::new();
    if let Some(heal) = name(slots.first()) {
        lines.push(format!("Heal: {heal}"));
    }
    let utils: Vec<String> = slots
        .iter()
        .skip(1)
        .take(3)
        .filter_map(|slot| name(Some(slot)))
        .collect();
    if !utils.is_empty() {
        lines.push(format!("Utils: {}", utils.join(", ")));
    }
    if let Some(elite) = name(slots.get(4)) {
        lines.push(format!("Elite: {elite}"));
    }
    lines
}

/// The scenario a published build was written for, as near as its own labels say.
fn scenario_for(build: &BenchmarkBuild) -> ScenarioSpec {
    let mode = match build.mode.as_str() {
        "WvW" => gw2_core::types::GameMode::WvW,
        "PvP" => gw2_core::types::GameMode::PvP,
        _ => gw2_core::types::GameMode::PvE,
    };
    let role = build.role.to_lowercase();
    let kind = if role.contains("heal") || role.contains("support") {
        CombatKind::Support
    } else if role.contains("commander") {
        CombatKind::Commander
    } else if role.contains("disable") || role.contains("boonstrip") {
        CombatKind::Disabler
    } else if role.contains("condi") {
        CombatKind::CondiRamp
    } else if role.contains("roam") || role.contains("duel") || role.contains("assassin") {
        CombatKind::Harasser
    } else {
        CombatKind::StrikeSpike
    };
    let tier = if role.contains("zerg") || role.contains("cloud") || role.contains("raid") {
        CombatTier::Squad
    } else if role.contains("havoc") || role.contains("party") || role.contains("fractal") {
        CombatTier::Party
    } else {
        CombatTier::Solo
    };
    // Built from the balance context so every field the optimizer sets is
    // set the same way here — then the mode, scale and job the site itself
    // published are laid over it.
    let ctx = gw2_optimizer::balance::BalanceContext::new(mode);
    ScenarioSpec {
        combat_tier: tier,
        combat_kind: kind,
        ..ScenarioSpec::from_balance_context(&ctx)
    }
}
