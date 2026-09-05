//! Audit `data/cleanse_sources.json` against the live game cache. Run:
//!   cargo run --release -p gw2-optimizer --example cleanse_registry_check
//! Reads the cache directory from dev.cfg (copy dev.cfg.example).
//!
//! Fails (exit 1) when a registry entry names an id the cache does not have,
//! or whose name / profession / specialization / slot disagree with the cache.
//! Then lists, for review, every skill, trait, sigil, rune and relic whose text
//! the heuristic still flags as a cleanse but the registry does not carry.
use gw2_api::cache::DataCache;
use gw2_optimizer::data::cleanse_sources::{registry, text_suggests_cleanse, SourceKind};
use gw2_optimizer::gamedb::GameDb;
use std::collections::BTreeMap;

/// The verb window that made the heuristic fire, for the review list.
fn why(text: &str) -> String {
    let lower = text.to_lowercase();
    for verb in [
        "remov", "cleanse", "cure", "purg", "transfer", "consum", "send", "sent", "convert",
    ] {
        if let Some(i) = lower.find(verb) {
            let lo = i.saturating_sub(24);
            let hi = (i + 40).min(lower.len());
            let lo = (lo..=i).rev().find(|&b| lower.is_char_boundary(b)).unwrap_or(0);
            let hi = (hi..lower.len()).find(|&b| lower.is_char_boundary(b)).unwrap_or(lower.len());
            return format!("...{}...", lower[lo..hi].replace('\n', " "));
        }
    }
    String::from("(no verb found)")
}

fn main() {
    let cache = DataCache::new(gw2_api::dev_config::cache_dir_or_exit());
    let db = GameDb::load(&cache).expect("load real GameDb");
    let reg = registry();
    let mut problems = 0usize;
    let mut per_prof: BTreeMap<String, [usize; 2]> = BTreeMap::new();
    let mut gear = 0usize;
    let flag = |msg: String| {
        println!("PROBLEM {msg}");
    };
    for s in reg.all() {
        match s.kind {
            SourceKind::Skill => {
                let Some(k) = db.skills.get(&s.id) else {
                    flag(format!("skill {} {:?} not in cache", s.id, s.name));
                    problems += 1;
                    continue;
                };
                if !k.name.eq_ignore_ascii_case(&s.name) {
                    flag(format!("skill {} name {:?} != cache {:?}", s.id, s.name, k.name));
                    problems += 1;
                }
                // Trait-granted skills (Lesser Smite Condition, Invoke Torment)
                // carry no profession in the API; the table names the one that
                // grants them, which the cache cannot confirm or deny.
                if !k.professions.is_empty()
                    && (k.professions.len() != 1 || Some(&k.professions[0]) != s.profession.as_ref())
                {
                    flag(format!(
                        "skill {} {:?} profession {:?} != cache {:?}",
                        s.id, s.name, s.profession, k.professions
                    ));
                    problems += 1;
                }
                let spec = k
                    .specialization
                    .and_then(|id| db.specializations.get(&id))
                    .map(|sp| sp.name.clone());
                if spec.is_some() && spec != s.specialization {
                    flag(format!(
                        "skill {} {:?} specialization {:?} != cache {:?}",
                        s.id, s.name, s.specialization, spec
                    ));
                    problems += 1;
                }
                if k.slot != s.slot {
                    flag(format!(
                        "skill {} {:?} slot {:?} != cache {:?}",
                        s.id, s.name, s.slot, k.slot
                    ));
                    problems += 1;
                }
                per_prof
                    .entry(s.profession.clone().unwrap_or_default())
                    .or_default()[0] += 1;
            }
            SourceKind::Trait => {
                let Some(t) = db.traits.get(&s.id) else {
                    flag(format!("trait {} {:?} not in cache", s.id, s.name));
                    problems += 1;
                    continue;
                };
                if !t.name.eq_ignore_ascii_case(&s.name) {
                    flag(format!("trait {} name {:?} != cache {:?}", s.id, s.name, t.name));
                    problems += 1;
                }
                let sp = db.specializations.get(&t.specialization);
                if sp.map(|sp| &sp.profession) != s.profession.as_ref() {
                    flag(format!(
                        "trait {} {:?} profession {:?} != cache {:?}",
                        s.id,
                        s.name,
                        s.profession,
                        sp.map(|sp| &sp.profession)
                    ));
                    problems += 1;
                }
                if sp.map(|sp| &sp.name) != s.specialization.as_ref() {
                    flag(format!(
                        "trait {} {:?} specialization {:?} != cache {:?}",
                        s.id,
                        s.name,
                        s.specialization,
                        sp.map(|sp| &sp.name)
                    ));
                    problems += 1;
                }
                per_prof
                    .entry(s.profession.clone().unwrap_or_default())
                    .or_default()[1] += 1;
            }
            SourceKind::Sigil | SourceKind::Rune | SourceKind::Relic => {
                let Some(it) = db.items.get(&s.id) else {
                    flag(format!("item {} {:?} not in cache", s.id, s.name));
                    problems += 1;
                    continue;
                };
                if !it.name.eq_ignore_ascii_case(&s.name) {
                    flag(format!("item {} name {:?} != cache {:?}", s.id, s.name, it.name));
                    problems += 1;
                }
                gear += 1;
            }
        }
    }

    println!("\nREGISTRY {} sources (game build {})", reg.all().len(), reg.game_build);
    for (p, [skills, traits]) in &per_prof {
        println!("  {p:<13} skills={skills:<3} traits={traits}");
    }
    println!("  gear          {gear}");

    // Review list: what the text heuristic flags that the table does not know.
    println!("\nHEURISTIC-ONLY (flagged by text, absent from the registry) — review each:");
    let mut n = 0usize;
    let mut skills: Vec<_> = db.skills.values().collect();
    skills.sort_by_key(|k| k.id);
    for k in skills {
        if k.professions.len() != 1 || reg.knows_skill(k.id) {
            continue;
        }
        let mut text = k.name.clone();
        if let Some(d) = &k.description {
            text.push(' ');
            text.push_str(d);
        }
        for f in &k.facts {
            text.push(' ');
            text.push_str(&format!("{f:?}"));
        }
        if text_suggests_cleanse(&text) {
            n += 1;
            println!(
                "  skill {} {:?} [{} {:?}] {}",
                k.id,
                k.name,
                k.professions[0],
                k.slot,
                why(&text)
            );
        }
    }
    let mut traits: Vec<_> = db.traits.values().collect();
    traits.sort_by_key(|t| t.id);
    for t in traits {
        if reg.knows_trait(t.id) {
            continue;
        }
        let mut text = t.name.clone();
        if let Some(d) = &t.description {
            text.push(' ');
            text.push_str(d);
        }
        for f in &t.facts {
            text.push(' ');
            text.push_str(&format!("{f:?}"));
        }
        if text_suggests_cleanse(&text) {
            n += 1;
            let sp = db.specializations.get(&t.specialization);
            println!(
                "  trait {} {:?} [{} / {}] {}",
                t.id,
                t.name,
                sp.map(|s| s.profession.as_str()).unwrap_or("?"),
                sp.map(|s| s.name.as_str()).unwrap_or("?"),
                why(&text)
            );
        }
    }
    let mut items: Vec<_> = db
        .items
        .values()
        .filter(|i| {
            i.item_type == "Relic"
                || (i.item_type == "UpgradeComponent"
                    && (i.name.contains("Sigil of") || i.name.contains("Rune of")))
        })
        .collect();
    items.sort_by_key(|i| i.id);
    for it in items {
        if reg.knows_item(it.id) {
            continue;
        }
        let mut text = it.name.clone();
        if let Some(d) = &it.description {
            text.push(' ');
            text.push_str(d);
        }
        if let Some(det) = &it.details {
            for b in &det.bonuses {
                text.push(' ');
                text.push_str(b);
            }
        }
        if text_suggests_cleanse(&text) {
            n += 1;
            println!(
                "  item {} {:?} [{} {}] {}",
                it.id,
                it.name,
                it.item_type,
                it.rarity,
                why(&text)
            );
        }
    }
    println!("  ({n} heuristic-only ids)");

    println!("\n{problems} problems");
    if problems > 0 {
        std::process::exit(1);
    }
}
