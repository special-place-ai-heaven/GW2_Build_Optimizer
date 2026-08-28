//! Live-catalog proof: graph ranking vs the on-disk GW2 items cache.
//! Snapshot fixtures in `upgrade_graph` tests always run; this catches API drift.
//!
//! This test reads a live, in-game cache directory — it is `#[ignore]`d by
//! default so CI (which has no such cache) never runs it. Run explicitly with
//! `cargo test -p gw2-optimizer --test upgrade_graph_live -- --ignored`.

use std::path::Path;

use gw2_api::cache::DataCache;
use gw2_api::models::Item;
use gw2_optimizer::balance::BalanceContext;
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::scoring::AXIS_KEYS;
use gw2_optimizer::upgrade_graph::{UpgradeGraph, UpgradeKind};

const LIVE_CACHE: &str = r"C:\GAMES\Guild Wars 2\addons\gw2_build_optimizer\cache";

fn try_live_graph() -> Option<UpgradeGraph> {
    let dir = Path::new(LIVE_CACHE);
    if !dir.join("items.json").exists() {
        return None;
    }
    let cache = DataCache::new(dir);
    let items: Vec<Item> = cache.load("items").ok()??;
    let mut db = GameDb::empty_for_tests();
    for item in items {
        let id = item.id;
        let dt = item.details.as_ref().and_then(|d| d.detail_type.as_deref());
        if item.item_type == "Relic" {
            db.relics.push(id);
        } else if dt == Some("Rune") {
            db.runes.push(id);
        } else if dt == Some("Sigil") {
            db.sigils.push(id);
        } else {
            continue;
        }
        db.items.insert(id, item);
    }
    Some(UpgradeGraph::from_db(&db, &BalanceContext::pve()))
}

#[test]
#[ignore] // Requires the in-game addon's live items cache (see LIVE_CACHE)
fn live_catalog_search_is_not_alphabetical() {
    let Some(g) = try_live_graph() else {
        eprintln!("skip: live items cache not present");
        return;
    };

    let sigils = g.search(Some("power"), Some(UpgradeKind::Sigil), None, None, 12);
    let sigil_names: Vec<&str> = sigils.iter().map(|n| n.name.as_str()).collect();
    assert!(
        sigil_names.iter().any(|n| n.contains("Force")),
        "Force missing from power sigils: {sigil_names:?}"
    );
    assert!(
        sigil_names.iter().any(|n| n.contains("Accuracy")),
        "Accuracy missing from power sigils: {sigil_names:?}"
    );
    assert!(sigil_names.iter().all(|n| !n.contains("Bloodlust")));

    let relics = g.search(Some("power"), Some(UpgradeKind::Relic), None, None, 12);
    let relic_names: Vec<&str> = relics.iter().map(|n| n.name.as_str()).collect();
    assert!(
        relic_names.iter().any(|n| n.contains("Thief")),
        "Thief missing from power relics: {relic_names:?}"
    );

    let fireworks = g.get("Relic of Fireworks").expect("Fireworks in catalog");
    assert_eq!(fireworks.rely, "long_recharge");
    assert!(fireworks.axes.power > 0.0);
    let power_relics = g.search(Some("power"), Some(UpgradeKind::Relic), None, None, 80);
    assert!(
        power_relics.iter().any(|n| n.name.contains("Fireworks")),
        "Fireworks classified but missing from power relic search"
    );

    let runes = g.search(Some("power"), Some(UpgradeKind::Rune), None, None, 12);
    let rune_names: Vec<&str> = runes.iter().map(|n| n.name.as_str()).collect();
    assert!(
        rune_names.iter().any(|n| n.contains("Scholar")),
        "Scholar missing from power runes: {rune_names:?}"
    );

    for key in AXIS_KEYS {
        let hits = g.search(Some(key), None, None, None, 8);
        assert!(
            !hits.is_empty(),
            "live catalog produced no hits for axis {key}"
        );
    }

    let force = g.get("Superior Sigil of Force").expect("Force in catalog");
    assert_eq!(force.rely, "passive");
    let scholar = g.get("Superior Rune of the Scholar").expect("Scholar");
    assert!(
        scholar.blurb.contains("125 Ferocity") || scholar.tags.iter().any(|t| t == "attr:ferocity")
    );
}
