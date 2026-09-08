//! Does the additive-vs-multiplicative modifier question actually move rankings?
//!
//! The wiki's Damage calculation page says the real model is MIXED: some
//! sigils, traits and utility effects share one additive bucket, and the rest
//! are separate multiplicative factors —
//!
//!   (1 + 0.05 + 0.03 + 0.10) * 1.20 * 1.05 = 1.4868
//!
//! Our `total_strike_mult` is fully multiplicative; gw2combat is one additive
//! bucket. Neither is the wiki's model, and the wiki gives no rule for sorting
//! an arbitrary trait into a bucket, so the faithful version is not derivable
//! from API data.
//!
//! What is answerable is whether it matters. A correction applied equally to
//! every build cannot reorder them, so the number to look at is not the size of
//! the gap but its *spread*, and the rank churn it causes. The two models
//! bracket the truth, so their disagreement is the upper bound on the error.
//!
//!   cargo run -p gw2-optimizer --example damage_modifier_stacking -- WvW

use std::collections::BTreeMap;

use gw2_core::types::GameMode;
use gw2_optimizer::balance::BalanceContext;
use gw2_optimizer::gamedb::GameDb;

/// Drop `<c=@reminder>`-style markup so a percent sits next to its own words.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

struct Row {
    label: String,
    count: usize,
    multiplicative: f64,
    additive: f64,
    actual: f64,
    bucketed: usize,
    profession: String,
    has_upgrades: bool,
    unparsed: usize,
    unparsed_text: Vec<String>,
}

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

    let mut rows: Vec<Row> = Vec::new();
    for build in &builds {
        if let Some(want) = &only_mode {
            if !build.mode.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let p = &build.published;
        if p.specs.is_empty() {
            continue;
        }
        let trait_ids: Vec<u32> = p.specs.iter().flat_map(|s| s.trait_ids.clone()).collect();
        let mode = match build.mode.as_str() {
            "WvW" => GameMode::WvW,
            "PvP" => GameMode::PvP,
            _ => GameMode::PvE,
        };
        let ctx = BalanceContext::new(mode);
        let mods = gw2_optimizer::combat::extract_damage_modifiers(
            &trait_ids,
            p.rune_id,
            &p.sigil_ids,
            p.relic_id,
            &db.traits,
            &db.items,
            &ctx,
        );
        // The two extremes bracket the truth; `actual` is the bucketed model.
        let all: Vec<f64> = mods
            .strike_pct
            .iter()
            .chain(mods.strike_add_pct.iter())
            .copied()
            .collect();
        let multiplicative = all.iter().fold(1.0, |acc, m| acc * (1.0 + m));
        let additive = 1.0 + all.iter().sum::<f64>();
        let actual = mods.total_strike_mult();
        rows.push(Row {
            label: format!("{} {}", build.spec_name, build.role),
            profession: build.profession.clone(),
            count: all.len(),
            bucketed: mods.strike_add_pct.len(),
            actual,
            multiplicative,
            additive,
            has_upgrades: p.rune_id.is_some() || !p.sigil_ids.is_empty() || p.relic_id.is_some(),
            unparsed: mods.unparsed.len(),
            unparsed_text: mods.unparsed.clone(),
        });
    }

    if rows.is_empty() {
        eprintln!("no builds matched");
        std::process::exit(1);
    }
    println!("{} builds scored\n", rows.len());

    let mut by_count = std::collections::BTreeMap::<usize, usize>::new();
    for r in &rows {
        *by_count.entry(r.count).or_default() += 1;
    }
    let with_bucket = rows.iter().filter(|r| r.bucketed > 0).count();
    let bucketed_total: usize = rows.iter().map(|r| r.bucketed).sum();
    println!(
        "modifier_buckets.json reached {with_bucket} builds ({bucketed_total} additive entries)
"
    );
    println!("strike modifiers per build:");
    for (n, hits) in &by_count {
        println!("  {n:2} modifiers  {hits:3} builds  {}", "#".repeat(*hits));
    }

    // A build with no strike modifiers scores at a flat 1.0x. That is honest
    // more often than it looks: roaming and support builds take sustain traits
    // and Energy/Cleansing sigils, and conditional bonuses ("below 50%
    // health", "after using X") are skipped here by design — the activation
    // evaluator owns those. Measured 2026-09-07: 69/145 WvW builds, none of
    // them a parsing miss. A page that never published its upgrades is the
    // one case that is genuinely missing data.
    let zero: Vec<&Row> = rows.iter().filter(|r| r.count == 0).collect();
    let no_data = zero.iter().filter(|r| !r.has_upgrades).count();
    let parsed_nothing: Vec<&&Row> = zero.iter().filter(|r| r.has_upgrades).collect();
    println!(
        "\n{} builds have no unconditional strike modifier:",
        zero.len()
    );
    println!("  {no_data} published no rune/sigil/relic at all — missing data");
    println!(
        "  {} carry upgrades and traits with no flat damage % among them",
        parsed_nothing.len()
    );
    let mut by_prof = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for r in &rows {
        let e = by_prof.entry(r.profession.clone()).or_default();
        e.1 += 1;
        if r.count == 0 {
            e.0 += 1;
        }
    }
    println!("      zero-modifier rate by profession:");
    for (prof, (zero, total)) in &by_prof {
        println!(
            "        {:<16} {:3}/{:<3} {:>4.0}%",
            prof,
            zero,
            total,
            100.0 * *zero as f64 / *total as f64
        );
    }
    let mut worst: Vec<&&Row> = parsed_nothing.clone();
    worst.sort_by_key(|r| std::cmp::Reverse(r.unparsed));
    println!("      most unparsed percent strings:");
    for r in worst.iter().take(6) {
        println!("        {:<38} {} unparsed", r.label, r.unparsed);
        for u in r.unparsed_text.iter().take(4) {
            println!("            | {}", u.replace('\n', " "));
        }
    }

    // Cross-check that nothing hides in prose. The `unparsed` bucket only sees
    // facts that carried a percent; a bonus written only in the description
    // would be invisible to it. GW2 keeps the number in the fact, so this
    // stays at zero — if it ever moves, a trait's tooltip format changed.
    let mut builds_with_hidden = 0usize;
    let mut resolve_stats: Vec<(usize, usize, usize, usize)> = Vec::new();
    let mut dumped = false;
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    for build in &builds {
        if let Some(want) = &only_mode {
            if !build.mode.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let p = &build.published;
        if p.specs.is_empty() {
            continue;
        }
        let trait_ids: Vec<u32> = p.specs.iter().flat_map(|s| s.trait_ids.clone()).collect();
        let mode = match build.mode.as_str() {
            "WvW" => GameMode::WvW,
            "PvP" => GameMode::PvP,
            _ => GameMode::PvE,
        };
        let ctx = BalanceContext::new(mode);
        let mods = gw2_optimizer::combat::extract_damage_modifiers(
            &trait_ids,
            p.rune_id,
            &p.sigil_ids,
            p.relic_id,
            &db.traits,
            &db.items,
            &ctx,
        );
        if !mods.strike_pct.is_empty() {
            continue;
        }
        // Second arg: dump every fact of the first zero-modifier build whose
        // label contains it, so a hole can be read instead of guessed at.
        if let Some(needle) = std::env::args().nth(2) {
            let label = format!("{} {}", build.spec_name, build.role);
            if label.contains(&needle) && !dumped {
                dumped = true;
                println!("\n==== {label} ({}) ====", build.source_url);
                for id in &trait_ids {
                    let Some(t) = db.traits.get(id) else { continue };
                    println!("  [{}] {} ({})", t.id, t.name, t.slot);
                    for f in &t.facts {
                        println!("      fact {:?}", f);
                    }
                    for tf in &t.traited_facts {
                        println!(
                            "      traited(requires {}) {:?}",
                            tf.requires_trait, tf.fact
                        );
                    }
                }
            }
        }
        let resolved = trait_ids
            .iter()
            .filter(|id| db.traits.contains_key(id))
            .count();
        let with_facts = trait_ids
            .iter()
            .filter_map(|id| db.traits.get(id))
            .filter(|t| !t.facts.is_empty())
            .count();
        let with_desc = trait_ids
            .iter()
            .filter_map(|id| db.traits.get(id))
            .filter(|t| t.description.is_some())
            .count();
        resolve_stats.push((trait_ids.len(), resolved, with_facts, with_desc));
        let mut hit = false;
        for id in &trait_ids {
            let Some(t) = db.traits.get(id) else { continue };
            let Some(desc) = t.description.as_deref() else {
                continue;
            };
            let text = strip_tags(desc);
            let lower = text.to_lowercase();
            for (i, _) in text.match_indices('%') {
                let lo = i.saturating_sub(70);
                let hi = (i + 40).min(text.len());
                let window = &lower[lo..hi];
                if window.contains("damage")
                    && !window.contains("reduce")
                    && !window.contains("condition damage")
                {
                    hit = true;
                    examples
                        .entry(t.name.clone())
                        .or_insert_with(|| text[lo..hi].trim().to_string());
                    break;
                }
            }
        }
        if hit {
            builds_with_hidden += 1;
        }
    }
    let sum = |f: fn(&(usize, usize, usize, usize)) -> usize| -> usize {
        resolve_stats.iter().map(f).sum()
    };
    println!("\nzero-modifier builds: where the trait data actually goes");
    println!("  {:5} trait slots published", sum(|s| s.0));
    println!("  {:5} resolve in db.traits", sum(|s| s.1));
    println!("  {:5} of those carry any facts", sum(|s| s.2));
    println!("  {:5} of those carry a description", sum(|s| s.3));
    println!(
        "  {} of {} such builds have a trait whose DESCRIPTION states a damage %",
        builds_with_hidden,
        zero.len()
    );
    for (name, snippet) in examples.iter().take(10) {
        println!("    {name:<26} …{snippet}…");
    }

    // The gap between the two models, per build. Its SPREAD is the ranking
    // signal: a constant offset reorders nothing.
    let mut gaps: Vec<f64> = rows
        .iter()
        .map(|r| r.multiplicative / r.additive - 1.0)
        .collect();
    gaps.sort_by(|a, b| a.total_cmp(b));
    let pct = |v: f64| format!("{:.2}%", v * 100.0);
    let at = |q: f64| gaps[((gaps.len() - 1) as f64 * q) as usize];
    println!("\nmultiplicative over additive, per build:");
    println!(
        "  min {}   median {}   max {}",
        pct(gaps[0]),
        pct(at(0.5)),
        pct(*gaps.last().unwrap())
    );
    println!(
        "  spread (max - min): {}",
        pct(gaps.last().unwrap() - gaps[0])
    );

    // Where the bucketed model actually lands between the two extremes.
    let mut shift: Vec<f64> = rows
        .iter()
        .filter(|r| r.bucketed > 0)
        .map(|r| r.multiplicative / r.actual - 1.0)
        .collect();
    shift.sort_by(|a, b| a.total_cmp(b));
    if let (Some(lo), Some(hi)) = (shift.first(), shift.last()) {
        println!(
            "
all-multiplicative over the bucketed model, builds with an additive entry:
  min {}   max {}",
            pct(*lo),
            pct(*hi)
        );
    }

    // Rank churn: strike damage scales linearly with the multiplier, so ranking
    // builds by it is directly comparable between the two models.
    let rank_by = |key: fn(&Row) -> f64| {
        let mut idx: Vec<usize> = (0..rows.len()).collect();
        idx.sort_by(|&a, &b| key(&rows[b]).total_cmp(&key(&rows[a])));
        let mut rank = vec![0usize; rows.len()];
        for (place, &i) in idx.iter().enumerate() {
            rank[i] = place;
        }
        rank
    };
    let mult_rank = rank_by(|r| r.multiplicative);
    let add_rank = rank_by(|r| r.additive);
    let moved = (0..rows.len())
        .filter(|&i| mult_rank[i] != add_rank[i])
        .count();
    let worst = (0..rows.len())
        .map(|i| (mult_rank[i].abs_diff(add_rank[i]), i))
        .max()
        .unwrap();
    println!("\nrank churn between the two models:");
    println!("  {moved} of {} builds change rank", rows.len());
    println!(
        "  largest shift: {} places ({})",
        worst.0, rows[worst.1].label
    );
}
