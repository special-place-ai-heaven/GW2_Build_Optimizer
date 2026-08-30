//! The canonical gear-prefix pool.
//!
//! `/v2/itemstats` is a table of stat *templates*, not of prefixes. The live
//! cache holds 191 rows that resolve to 66 named prefixes: 43 display names
//! carry between two and nine ids (armour / weapon / trinket variants, plus a
//! legacy band at 1041-1052 whose multipliers are all `0.0`), and 13 rows
//! carry no name at all.
//!
//! Enumerating that raw map is how a "top four prefixes" list becomes four
//! Berserker's rows, and how a search settles on an id that the name-keyed
//! appliers can never resolve back to. [`canonical_itemstats`] is the one pool
//! every prefix enumerator is meant to draw from: exactly one id per display
//! name. For most names that id is the lowest, matching
//! [`GameDb::itemstat_by_name`]. Giver's is the live exception: the wiki
//! three-stat template wins the group so a search does not kit every slot as
//! Toughness-only 627.
//!
//! The pool answers one question — *which rows are prefixes a search may
//! choose* — and it answers it with two rules: a row must be **priceable** by
//! the slot-budget model (a positive multiplier somewhere), and a display name
//! gets exactly **one id** (the lowest, except Giver's — see
//! [`canonical_itemstats`]). The legacy 1041-1052 band fails the
//! first rule: `multiplier: 0.0` everywhere with the real numbers in the flat
//! `value` field. On live data each of those ten rows also loses its name group
//! to a lower healthy id, so the identity rule alone hides the problem — until
//! a degenerate row turns up with a name nothing else shares, which is exactly
//! the hole the priceable rule closes.
//!
//! Pricing itself stays in the budget code: `max_positive_multiplier` is
//! defined here once and `engine::add_budget_stats_for_itemstat` classifies
//! majors with the same function, so "unpriceable" means the same thing on both
//! sides and neither can drift into a private copy of the rule.

use std::collections::HashMap;

use gw2_api::models::ItemStat;

use crate::gamedb::GameDb;

/// Grouping key for "the same displayed prefix", or `None` for a row that has
/// no display name.
///
/// Deliberately mirrors [`GameDb::itemstat_by_name`]'s notion of an exact hit:
/// that function treats two rows as the same name when their ASCII-alphanumeric
/// keys match ("Knight's" == "Knights"), and falls back to a plain lowercase
/// comparison when the key is empty. Grouping any other way would let the pool
/// hand out an id that name resolution never returns, which is the exact
/// mismatch this module exists to close.
///
/// Nameless rows are dropped rather than grouped: `itemstat_by_name` refuses an
/// empty needle, so an unnamed prefix cannot be resolved, displayed, saved, or
/// named to an LLM. It is not a prefix, it is a stat template with no identity.
fn display_name_key(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    let alnum = gw2_core::i18n::alnum_key(name);
    if alnum.is_empty() {
        // A fully non-ASCII name collapses to an empty alnum key, and
        // `itemstat_by_name` then falls back to comparing lowercase names.
        // Group the same way — no trimming, so the two agree exactly.
        Some(name.to_lowercase())
    } else {
        Some(alnum)
    }
}

/// The largest attribute multiplier on a row, or `None` when no attribute
/// carries a positive one.
///
/// `/v2/itemstats` prices a prefix as *multiplier × the slot's stat budget*, so
/// a positive multiplier is what makes a row a budget template at all. The
/// legacy band at 1041-1052 has `multiplier: 0.0` on every attribute and puts
/// the real numbers in the flat `value` field instead; those rows are item-level
/// stat blocks, not prefixes, and the budget model cannot price them.
///
/// This is the single definition of "has a positive multiplier" in the crate.
/// [`canonical_itemstats`] uses it to keep unpriceable rows out of the prefix
/// pool, and `engine::add_budget_stats_for_itemstat` uses it to classify
/// majors — the classifier's old `max == 0.0` reading made *every* attribute a
/// major, which is how Berserker's #1046 scored 1507/1507/1507 (4521 points)
/// against the real #161's 1507/1050/1050 (3607).
///
/// A row with no attributes at all has no multiplier to report and so also
/// yields `None`; it is unpriceable for a different reason (nothing to price)
/// and is *not* dropped from the pool — see [`canonical_itemstats`].
///
/// `NaN` is not positive, so a corrupt multiplier makes the row unpriceable
/// rather than silently winning the `max` comparison.
pub fn max_positive_multiplier(stat: &ItemStat) -> Option<f64> {
    stat.attributes
        .iter()
        .map(|attr| attr.multiplier)
        .filter(|multiplier| *multiplier > 0.0)
        .fold(None, |best: Option<f64>, m| {
            Some(best.map_or(m, |b| b.max(m)))
        })
}

/// Sorted names of attributes that carry a positive multiplier.
///
/// The trinket `value` field is ignored: 628 and 1430 are the same Giver's
/// template, one without flats and one with them.
fn multiplier_attribute_set(stat: &ItemStat) -> Vec<&str> {
    let mut names: Vec<&str> = stat
        .attributes
        .iter()
        .filter(|attr| attr.multiplier > 0.0)
        .map(|attr| attr.attribute.as_str())
        .collect();
    names.sort_unstable();
    names
}

/// Wiki main-table Giver's: Toughness / Healing Power / Concentration.
/// `/v2/itemstats` spells those `Healing` and `BoonDuration`.
pub(crate) fn is_wiki_givers_three_stat(stat: &ItemStat) -> bool {
    display_name_key(&stat.name).as_deref() == Some("givers")
        && multiplier_attribute_set(stat) == ["BoonDuration", "Healing", "Toughness"]
}

/// True when `stat` should replace `incumbent` in a display-name group.
fn outranks(stat: &ItemStat, incumbent: &ItemStat) -> bool {
    match (
        is_wiki_givers_three_stat(stat),
        is_wiki_givers_three_stat(incumbent),
    ) {
        (true, false) => true,
        (false, true) => false,
        _ => stat.id < incumbent.id,
    }
}

/// The canonical prefix pool: one [`ItemStat`] per display name, ordered by id.
///
/// For each display name the lowest id wins, matching
/// [`GameDb::itemstat_by_name`] — except Giver's. Live `/v2/itemstats` ships
/// several Giver's multiplier shapes under one name; the lowest id (627) is
/// Toughness only. The wiki main table is Toughness / Healing Power /
/// Concentration (`Healing` + `BoonDuration` in the API). That three-stat
/// template wins the group so a search does not kit every slot as 627. Name
/// lookup is a different function and is not changed here.
///
/// Two rules, in this order:
///
/// 1. **Priceable.** A row that carries attributes but no positive multiplier
///    (see [`max_positive_multiplier`]) is dropped before grouping. Those are
///    the legacy 1041-1052 band: flat-`value` item-level stat blocks that the
///    slot-budget model cannot price. Dropping them *before* the id tie-break
///    is deliberate — a degenerate row must never be the survivor its name
///    group resolves to, and on live data ten of them (1041-1044, 1046-1048,
///    1050-1052) happen to sit above a healthy id and lose anyway. The hole
///    this closes is the degenerate row whose display name is *unique*, where
///    the id tie-break has nothing to prefer it to.
///    A row with no attributes at all is kept: it is unpriceable in the
///    trivial sense (there is nothing to add), it is still a real,
///    name-resolvable prefix, and dropping it would delete whole name groups
///    from a db that simply has not loaded attributes.
/// 2. **One id per display name.** Nameless rows are dropped; among the rest
///    the lowest surviving id wins, unless the name is Giver's and a wiki
///    three-stat row (Toughness / Healing / BoonDuration, ignoring trinket
///    flats) is in the group — that row wins, lowest id among those that
///    match. 628 beats 1070 and 1430; 627 / 629 / 630 / 631 lose.
///
/// The returned order is id-ascending and therefore independent of
/// `HashMap` iteration order — the same db yields the same pool on every run
/// and every machine. Callers that want a weight-driven order (radar prefix
/// first, say) sort this Vec; they do not re-enumerate `db.itemstats`.
///
/// Cost is O(n) with one small allocation per row. Call it once per
/// optimization and reuse the Vec — not once per candidate.
pub fn canonical_itemstats(db: &GameDb) -> Vec<&ItemStat> {
    let mut canonical: HashMap<String, &ItemStat> = HashMap::with_capacity(db.itemstats.len());
    for stat in db.itemstats.values() {
        if !stat.attributes.is_empty() && max_positive_multiplier(stat).is_none() {
            continue;
        }
        let Some(key) = display_name_key(&stat.name) else {
            continue;
        };
        match canonical.get(&key) {
            Some(incumbent) if !outranks(stat, incumbent) => {}
            _ => {
                canonical.insert(key, stat);
            }
        }
    }
    let mut pool: Vec<&ItemStat> = canonical.into_values().collect();
    pool.sort_by_key(|stat| stat.id);
    pool
}

#[cfg(test)]
mod tests {
    use super::*;
    use gw2_api::models::itemstats::StatAttribute;

    fn attr(attribute: &str, multiplier: f64, value: i32) -> StatAttribute {
        StatAttribute {
            attribute: attribute.to_string(),
            multiplier,
            value,
        }
    }

    fn named(id: u32, name: &str) -> ItemStat {
        ItemStat {
            id,
            name: name.to_string(),
            attributes: vec![],
        }
    }

    fn with_attrs(id: u32, name: &str, attributes: Vec<StatAttribute>) -> ItemStat {
        ItemStat {
            id,
            name: name.to_string(),
            attributes,
        }
    }

    /// Name key re-derived from scratch, on purpose. If the test reused
    /// [`display_name_key`] a bug in that function would make the duplicate
    /// check agree with itself and pass vacuously.
    fn independent_name_key(name: &str) -> String {
        name.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    /// A db shaped like the live `/v2/itemstats` cache: real ids, real
    /// duplicate groups, the all-zero legacy band, and unnamed rows.
    fn live_shaped_db() -> GameDb {
        let mut db = GameDb::empty_for_tests();
        let rows = vec![
            // Berserker's: five ids, one display name. 1046 is the legacy
            // all-zero-multiplier row; 161 is the real prefix.
            with_attrs(
                161,
                "Berserker's",
                vec![
                    attr("Power", 0.35, 0),
                    attr("Precision", 0.25, 0),
                    attr("CritDamage", 0.25, 0),
                ],
            ),
            with_attrs(
                584,
                "Berserker's",
                vec![
                    attr("Power", 0.35, 32),
                    attr("Precision", 0.25, 18),
                    attr("CritDamage", 0.25, 18),
                ],
            ),
            named(599, "Berserker's"),
            with_attrs(
                1046,
                "Berserker's",
                vec![
                    attr("Power", 0.0, 32),
                    attr("Precision", 0.0, 18),
                    attr("CritDamage", 0.0, 18),
                ],
            ),
            named(1077, "Berserker's"),
            // Apostrophe-less spelling of the same prefix. `itemstat_by_name`
            // treats it as an exact hit on "Berserker's", so the pool has to
            // group it too or the two disagree.
            named(9001, "Berserkers"),
            // Knight's: 1051 is another all-zero row.
            named(158, "Knight's"),
            named(657, "Knight's"),
            named(662, "Knight's"),
            named(1051, "Knight's"),
            // Giver's: nine live /v2/itemstats ids, five multiplier shapes.
            // 627 is Toughness only; 628/1070/1430 are the wiki main-table
            // three-stat (Toughness / Healing / BoonDuration); 1430 carries
            // the trinket flats. Empty-attribute stubs hid the collapse.
            with_attrs(627, "Giver's", vec![attr("Toughness", 0.35, 0)]),
            with_attrs(
                628,
                "Giver's",
                vec![
                    attr("Toughness", 0.35, 0),
                    attr("Healing", 0.25, 0),
                    attr("BoonDuration", 0.25, 0),
                ],
            ),
            with_attrs(
                629,
                "Giver's",
                vec![attr("Toughness", 0.35, 0), attr("Healing", 0.25, 0)],
            ),
            with_attrs(
                630,
                "Giver's",
                vec![
                    attr("Vitality", 0.25, 0),
                    attr("ConditionDuration", 0.35, 0),
                ],
            ),
            with_attrs(631, "Giver's", vec![attr("ConditionDuration", 0.35, 0)]),
            with_attrs(
                1030,
                "Giver's",
                vec![
                    attr("Vitality", 0.25, 0),
                    attr("ConditionDuration", 0.35, 0),
                ],
            ),
            with_attrs(1031, "Giver's", vec![attr("ConditionDuration", 0.35, 0)]),
            with_attrs(
                1070,
                "Giver's",
                vec![
                    attr("Toughness", 0.35, 0),
                    attr("Healing", 0.25, 0),
                    attr("BoonDuration", 0.25, 0),
                ],
            ),
            with_attrs(
                1430,
                "Giver's",
                vec![
                    attr("Toughness", 0.35, 32),
                    attr("Healing", 0.25, 18),
                    attr("BoonDuration", 0.25, 18),
                ],
            ),
            // Celestial: 1052 is all-zero.
            named(559, "Celestial"),
            named(588, "Celestial"),
            named(1052, "Celestial"),
            // Two-id groups.
            named(1826, "Demolisher's"),
            named(1827, "Demolisher's"),
            named(1345, "Harrier's"),
            named(1363, "Harrier's"),
            // Unnamed rows. 50 has no attributes; 1419 does — namelessness is
            // the filter, not emptiness.
            named(50, ""),
            with_attrs(1419, "", vec![attr("Power", 0.35, 32)]),
            // Whitespace-only name is just as unresolvable as an empty one.
            named(9002, "   "),
        ];
        for row in rows {
            db.itemstats.insert(row.id, row);
        }
        db
    }

    #[test]
    fn canonical_itemstats_one_id_per_name() {
        let db = live_shaped_db();
        let pool = canonical_itemstats(&db);

        // 1. The property the pool exists for: no display name survives twice.
        let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
        for stat in &pool {
            by_name
                .entry(independent_name_key(&stat.name))
                .or_default()
                .push(stat.id);
        }
        let mut collisions: Vec<(&String, &Vec<u32>)> =
            by_name.iter().filter(|(_, ids)| ids.len() > 1).collect();
        collisions.sort();
        assert!(
            collisions.is_empty(),
            "two ids sharing a display name both reached the pool: {collisions:?}"
        );

        // 2. The exact survivors, by ground truth from the live cache: the
        //    lowest id of each name group, except Giver's which is the wiki
        //    three-stat template (628), not Toughness-only 627.
        let ids: Vec<u32> = pool.iter().map(|stat| stat.id).collect();
        assert_eq!(
            ids,
            vec![158, 161, 559, 628, 1345, 1826],
            "wrong survivors (expected the lowest id of each display name, Giver's 628)"
        );

        // 3. The all-zero-multiplier legacy band never wins a name group,
        //    because every one of its rows sits above a healthy id.
        for degenerate in [1046, 1051, 1052] {
            assert!(
                !ids.contains(&degenerate),
                "legacy all-zero row {degenerate} outranked the real prefix"
            );
        }

        // 4. For every prefix whose healthy rows share one multiplier shape,
        //    the pool and name resolution are the same function. Giver's is
        //    the live exception: several shapes, wiki three-stat survivor
        //    628, while name lookup still returns the lowest id and is not
        //    this function.
        for stat in &pool {
            if independent_name_key(&stat.name) == "givers" {
                assert_eq!(stat.id, 628);
                continue;
            }
            let resolved = db
                .itemstat_by_name(&stat.name)
                .unwrap_or_else(|| panic!("pool entry {:?} does not resolve by name", stat.name));
            assert_eq!(
                resolved.id, stat.id,
                "pool holds {} for {:?} but itemstat_by_name resolves {}",
                stat.id, stat.name, resolved.id
            );
        }

        // 5. Deduplication is not deletion: every named row in the db is still
        //    represented by its group's survivor.
        for stat in db.itemstats.values() {
            if stat.name.trim().is_empty() {
                continue;
            }
            let key = independent_name_key(&stat.name);
            assert!(
                pool.iter()
                    .any(|kept| independent_name_key(&kept.name) == key),
                "display name {:?} (id {}) lost its whole group",
                stat.name,
                stat.id
            );
        }
    }

    /// Live Giver's is not one prefix. The wiki main table is Toughness /
    /// Healing Power / Concentration (API: Toughness / Healing / BoonDuration,
    /// id 628). Lowest-id grouping keeps 627's one-stat Toughness vector and
    /// a search then kits every slot as that row.
    #[test]
    fn givers_survivor_is_not_the_one_stat_toughness_row() {
        let db = live_shaped_db();
        let pool = canonical_itemstats(&db);
        let givers: Vec<&ItemStat> = pool
            .iter()
            .copied()
            .filter(|stat| independent_name_key(&stat.name) == "givers")
            .collect();
        assert!(!givers.is_empty(), "Giver's vanished from the prefix pool");

        for stat in &givers {
            let mut attrs: Vec<&str> = stat
                .attributes
                .iter()
                .filter(|a| a.multiplier > 0.0)
                .map(|a| a.attribute.as_str())
                .collect();
            attrs.sort_unstable();
            assert_ne!(stat.id, 627, "Giver's survivor is Toughness-only id 627");
            assert_ne!(
                attrs.as_slice(),
                ["Toughness"].as_slice(),
                "Giver's survivor {} is 627's one-stat vector: {attrs:?}",
                stat.id
            );
        }

        assert_eq!(
            givers.len(),
            1,
            "Giver's must remain one search prefix, the wiki three-stat shape"
        );
        let survivor = givers[0];
        let mut attrs: Vec<&str> = survivor
            .attributes
            .iter()
            .filter(|a| a.multiplier > 0.0)
            .map(|a| a.attribute.as_str())
            .collect();
        attrs.sort_unstable();
        assert_eq!(
            attrs.as_slice(),
            ["BoonDuration", "Healing", "Toughness"].as_slice(),
            "Giver's survivor {} is not the wiki three-stat vector",
            survivor.id
        );
        assert_eq!(survivor.id, 628);
        // Trinket flats must not mint a second prefix: 1430 shares 628's
        // multipliers and must lose to the lower id.
        assert!(!pool.iter().any(|stat| stat.id == 1430));
    }

    #[test]
    fn pool_drops_rows_with_no_display_name() {
        let db = live_shaped_db();
        let pool = canonical_itemstats(&db);

        for stat in &pool {
            assert!(
                !stat.name.trim().is_empty(),
                "unnamed row {} reached the pool",
                stat.id
            );
        }
        // 1419 is unnamed but does carry attributes — it is still out, and it
        // is unreachable by name, which is why.
        assert!(!pool.iter().any(|stat| stat.id == 1419));
        assert!(db.itemstat_by_name("").is_none());
    }

    #[test]
    fn pool_order_is_id_ascending_and_repeatable() {
        let db = live_shaped_db();

        let first: Vec<u32> = canonical_itemstats(&db)
            .iter()
            .map(|stat| stat.id)
            .collect();
        let second: Vec<u32> = canonical_itemstats(&db)
            .iter()
            .map(|stat| stat.id)
            .collect();

        assert_eq!(first, second, "pool order changed between calls");
        assert!(
            first.windows(2).all(|pair| pair[0] < pair[1]),
            "pool is not strictly id-ascending: {first:?}"
        );
    }

    #[test]
    fn apostrophe_spellings_collapse_into_one_prefix() {
        let db = live_shaped_db();
        let pool = canonical_itemstats(&db);

        let berserker: Vec<u32> = pool
            .iter()
            .filter(|stat| independent_name_key(&stat.name) == "berserkers")
            .map(|stat| stat.id)
            .collect();
        assert_eq!(berserker, vec![161]);
        assert_eq!(db.itemstat_by_name("Berserkers").map(|s| s.id), Some(161));
    }

    #[test]
    fn non_ascii_names_are_kept_and_grouped_by_lowercase() {
        // `alnum_key` keeps ASCII only, so a localized name collapses to an
        // empty key. Such a row must still be reachable, and two spellings
        // that differ only in case must still be one prefix.
        let mut db = GameDb::empty_for_tests();
        db.itemstats.insert(7, named(7, "ЗЕЛОТ"));
        db.itemstats.insert(9, named(9, "зелот"));
        db.itemstats.insert(11, named(11, "минстрел"));

        let pool = canonical_itemstats(&db);
        let ids: Vec<u32> = pool.iter().map(|stat| stat.id).collect();

        assert_eq!(ids, vec![7, 11]);
        assert_eq!(db.itemstat_by_name("зелот").map(|s| s.id), Some(7));
    }

    #[test]
    fn empty_db_yields_an_empty_pool() {
        let db = GameDb::empty_for_tests();
        assert!(canonical_itemstats(&db).is_empty());
    }

    /// The hole the id tie-break cannot close: a degenerate row whose display
    /// name nothing else shares. On live data every all-zero row happens to sit
    /// above a healthy sibling and loses the group anyway — so a pool that
    /// dedups on identity alone looks correct against the current cache and
    /// admits the row the moment ArenaNet ships one under its own name.
    #[test]
    fn a_uniquely_named_degenerate_row_is_not_a_prefix() {
        let mut db = GameDb::empty_for_tests();
        // Healthy control with a unique name: it must survive, so the test can
        // tell "the priceable rule fired" apart from "the pool is empty".
        db.itemstats.insert(
            300,
            with_attrs(
                300,
                "Marauder's",
                vec![
                    attr("Power", 0.35, 0),
                    attr("Precision", 0.25, 0),
                    attr("Vitality", 0.25, 0),
                    attr("CritDamage", 0.25, 0),
                ],
            ),
        );
        // Degenerate and alone under its name: no lower healthy id to lose to.
        db.itemstats.insert(
            1049,
            with_attrs(
                1049,
                "Settler's",
                vec![
                    attr("ConditionDamage", 0.0, 32),
                    attr("Toughness", 0.0, 18),
                    attr("HealingPower", 0.0, 18),
                ],
            ),
        );

        let ids: Vec<u32> = canonical_itemstats(&db)
            .iter()
            .map(|stat| stat.id)
            .collect();

        assert_eq!(
            ids,
            vec![300],
            "the flat-value row reached the prefix pool; it has no positive \
             multiplier, so the budget model would price every one of its \
             attributes as a major"
        );
        // The row is still resolvable by name — dropping it from the *pool* is
        // a statement about what a search may choose, not about the db.
        assert_eq!(db.itemstat_by_name("Settler's").map(|s| s.id), Some(1049));
    }

    #[test]
    fn max_positive_multiplier_reports_only_positive_multipliers() {
        assert_eq!(
            max_positive_multiplier(&with_attrs(
                1,
                "Healthy",
                vec![attr("Power", 0.35, 0), attr("Precision", 0.25, 0)],
            )),
            Some(0.35)
        );
        assert_eq!(
            max_positive_multiplier(&with_attrs(
                2,
                "AllZero",
                vec![attr("Power", 0.0, 32), attr("Precision", 0.0, 18)],
            )),
            None
        );
        assert_eq!(max_positive_multiplier(&named(3, "NoAttributes")), None);
        // A corrupt multiplier must not win the max comparison.
        assert_eq!(
            max_positive_multiplier(&with_attrs(
                4,
                "Corrupt",
                vec![attr("Power", f64::NAN, 0), attr("Precision", 0.25, 0)],
            )),
            Some(0.25)
        );
    }
}
