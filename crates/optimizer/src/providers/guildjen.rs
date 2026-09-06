//! GuildJen — ids in a WordPress plugin's `data-gw2-*` attributes.
//!
//! Nothing on a GuildJen build page is prose. `gw2-embeddings-patched` emits
//! the id and the browser resolves the name, which is why a text scan of the
//! 808 KB server response finds "Superior Rune" zero times and why the three
//! name extractors returned empty strings on every page.
//!
//! The gear table is the whole build, and its rows are self-describing:
//!
//! ```html
//! <tr><td><span data-gw2-embed="items" data-gw2-id="73970"></span></td>
//!     <td><strong>Helm</strong><br><em>Minstrel</em></td>
//!     <td><span data-gw2-embed="items" data-gw2-id="24839"></span></td></tr>
//! ```
//!
//! item · slot and stat · upgrades. So the row says what its upgrade *is*:
//! attached to armour it is the rune, attached to a weapon it is a sigil. No
//! item type lookup, no counting, no stride.
//!
//! That matters because the two heuristics tried before it are both unsafe.
//! Position holds for the armour block and stops: the stride is 2 for
//! armour, 3 for a two-handed weapon, 1 for trinkets. And "the item repeated
//! most often is the rune" scores 18 right, 3 wrong and 6 no-answer over 27
//! pages — 18 of 18 on PvE and WvW, but 0 of 9 on PvP, where it silently
//! returns a sigil as the rune and breaks ties by picking the larger id.
//!
//! PvP is a different table, and column one carries amulet, rune, relic in
//! that order with both sigils of a set packed into one embed.

use super::{build_code_in, ids_in, GearRow, ProviderBuild, SpecLine};

/// How many specialization lines a build has.
const SPEC_SLOTS: usize = 3;

/// One build as a category index lists it.
///
/// GuildJen's WordPress theme prints its taxonomy as classes on the row:
///
/// ```html
/// <tr id="post-row-77896" class="post-row … category-gw2-builds-open-world
///     category-gw2-builds-raid tag-alacrity tag-luminary tag-support
///     post_class-guardian post_difficulty-easy post_playstyle-cooperative
///     post_role-support post_role-tank">
/// ```
///
/// So the index states the profession, the role and the playstyle of every
/// build it lists, and there is nothing left to infer from the slug or from
/// the body text of the build page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexRow {
    /// Absolute URL of the build page.
    pub url: String,
    /// Link text — "Heal DPS Luminary", "Power Vengeance Dragonhunter".
    pub name: String,
    /// Profession, from `post_class-*`. Title-cased.
    pub profession: String,
    /// Roles the site assigns, from `post_role-*`. Often empty in open
    /// world, where the playstyle is the distinction instead.
    pub roles: Vec<String>,
    /// Playstyles, from `post_playstyle-*`. Multi-valued: a WvW build is
    /// commonly both `havoc` and `cloud`.
    pub playstyles: Vec<String>,
    /// Difficulty, from `post_difficulty-*`.
    pub difficulty: String,
}

/// Every build a category index lists, deduplicated by URL.
///
/// Deduplication is not optional: each index repeats its whole catalogue in
/// a searchable table at the foot of the page, so the open-world index has
/// 162 rows for 86 builds and a naive walk fetches each one twice.
pub fn index_rows(html: &str) -> Vec<IndexRow> {
    let document = ::html::Html::parse_document(html);
    let row = ::html::Selector::parse("tr[id]").expect("valid selector");
    let link = ::html::Selector::parse(r#"a[href*="guildjen.com/"]"#).expect("valid selector");

    let mut rows: Vec<IndexRow> = Vec::new();
    for tr in document.select(&row) {
        let element = tr.value();
        if !element.id().is_some_and(|id| id.starts_with("post-row-")) {
            continue;
        }
        let Some(anchor) = tr.select(&link).next() else {
            continue;
        };
        let Some(url) = anchor.value().attr("href") else {
            continue;
        };
        if rows.iter().any(|seen| seen.url == url) {
            continue;
        }

        let mut found = IndexRow {
            url: url.to_string(),
            name: anchor.text().collect::<String>().trim().to_string(),
            ..Default::default()
        };
        for class in element.classes() {
            if let Some(profession) = class.strip_prefix("post_class-") {
                found.profession = crate::providers::title_case(profession);
            } else if let Some(role) = class.strip_prefix("post_role-") {
                found.roles.push(role.to_string());
            } else if let Some(playstyle) = class.strip_prefix("post_playstyle-") {
                found.playstyles.push(playstyle.to_string());
            } else if let Some(difficulty) = class.strip_prefix("post_difficulty-") {
                found.difficulty = difficulty.to_string();
            }
        }
        // `classes()` yields in document order per element but the set is
        // unordered across parses; sorting keeps a run reproducible.
        found.roles.sort();
        found.playstyles.sort();
        rows.push(found);
    }
    rows
}

/// Read one GuildJen build page.
///
/// Takes the raw response. Pruning to the article would be pointless here —
/// the payload is in attributes, not text — and is measurably harmful on the
/// other two sites, so no parser in this module does it.
pub fn parse(html: &str) -> ProviderBuild {
    let document = ::html::Html::parse_document(html);
    let mut build = ProviderBuild {
        build_code: build_code_in(html),
        specs: specs(&document),
        ..Default::default()
    };
    build.gear = gear_rows(&document);
    assign_upgrades(&mut build);
    build
}

/// The three specialization lines, in the order the page lists them.
///
/// `data-gw2-traits` carries the three chosen majors, adept first, on every
/// traitline embed of every page measured — nine trait ids straight off the
/// markup with no `major_traits` table needed. It agrees with the build
/// template on 26 of 27 pages.
///
/// Only the first three are taken. Three pages carry a fourth, always under
/// a "Variants" or "Alternative" heading, and it is a swap suggestion rather
/// than part of the build.
///
/// Not to be confused with `data-gw2-embed="traits"`, which is a *single*
/// alternative trait mentioned in prose — reading that instead is what
/// recorded one stray trait id and no specializations at all.
fn specs(document: &::html::Html) -> Vec<SpecLine> {
    let selector = ::html::Selector::parse(r#"[data-gw2-embed="traitline"][data-gw2-id]"#)
        .expect("valid selector");
    document
        .select(&selector)
        .filter_map(|line| {
            let id = line.value().attr("data-gw2-id")?.trim().parse().ok()?;
            Some(SpecLine {
                id,
                trait_ids: line
                    .value()
                    .attr("data-gw2-traits")
                    .map(ids_in)
                    .unwrap_or_default(),
            })
        })
        .take(SPEC_SLOTS)
        .collect()
}

/// Every gear row the page prints, in page order.
fn gear_rows(document: &::html::Html) -> Vec<GearRow> {
    let row = ::html::Selector::parse("tr").expect("valid selector");
    let cell = ::html::Selector::parse("td").expect("valid selector");
    let embed = ::html::Selector::parse(r#"[data-gw2-embed="items"][data-gw2-id]"#)
        .expect("valid selector");
    let slot_label = ::html::Selector::parse("strong").expect("valid selector");
    let stat_label = ::html::Selector::parse("em").expect("valid selector");

    let mut rows = Vec::new();
    for tr in document.select(&row) {
        let cells: Vec<_> = tr.select(&cell).collect();
        if cells.len() < 2 {
            continue;
        }
        let items_in = |at: usize| -> Vec<u32> {
            cells
                .get(at)
                .map(|c| {
                    c.select(&embed)
                        .filter_map(|e| e.value().attr("data-gw2-id"))
                        .flat_map(ids_in)
                        .collect()
                })
                .unwrap_or_default()
        };
        let text_of = |at: usize, what: &::html::Selector| -> String {
            cells
                .get(at)
                .and_then(|c| c.select(what).next())
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default()
        };

        let equipped = items_in(0);
        // A gear row equips exactly one thing. The skill-bar rows pack a
        // whole bar into one attribute, and the row that captions them
        // (`<strong><em>Heal / Utility / Elite</em></strong>`) equips
        // nothing at all.
        let Some(item_id) = (equipped.len() == 1).then(|| equipped[0]) else {
            continue;
        };
        // A gear label is `<strong>Helm</strong><br><em>Minstrel</em>`, in
        // that order. The infusion row uses the same two tags the other way
        // round — `<em>Optional</em><br><strong>x18</strong>` — which read
        // naively yields slot "x18" with stat "Optional" and lets a
        // non-stat into the build's prefix count. Order is what tells them
        // apart, so an unordered pair is treated as no label: the row is
        // still recorded, and what it holds is settled by item type at read
        // time, alongside the relic and the food.
        let label = cells.get(1).map(|c| c.inner_html()).unwrap_or_default();
        let labelled = match (label.find("<strong"), label.find("<em")) {
            (Some(slot_at), Some(stat_at)) => slot_at < stat_at,
            _ => false,
        };
        let (slot, stat) = if labelled {
            (text_of(1, &slot_label), text_of(1, &stat_label))
        } else {
            (String::new(), String::new())
        };
        // Cell 2 on a labelled (PvE/WvW) row, cell 3 on PvP where the middle
        // cell is present but blank.
        let upgrade_ids = if items_in(2).is_empty() {
            items_in(3)
        } else {
            items_in(2)
        };
        rows.push(GearRow {
            slot,
            stat,
            item_id: Some(item_id),
            upgrade_ids,
        });
    }
    rows
}

/// Name the rune and the sigils from what the rows say they are.
///
/// PvE and WvW label their rows, so the answer is structural. PvP labels
/// nothing and instead fixes the order — amulet, rune, relic down the first
/// column — with the amulet rendered as a plain `<img>` rather than an
/// embed, so the first two *embedded* items are the rune and the relic.
fn assign_upgrades(build: &mut ProviderBuild) {
    let labelled = build.gear.iter().any(|row| !row.slot.is_empty());
    if labelled {
        build.rune_id = build
            .gear
            .iter()
            .filter(|row| row.is_armour())
            .find_map(|row| row.upgrade_ids.first().copied());
        for row in build.gear.iter().filter(|row| row.is_weapon()) {
            build.sigil_ids.extend(row.upgrade_ids.iter().copied());
        }
        return;
    }

    // PvP. Both sigils of a set are packed into one embed and the same pair
    // is repeated on every row of the table, so they are deduplicated.
    let mut equipped = build.gear.iter().filter_map(|row| row.item_id);
    build.rune_id = equipped.next();
    build.relic_id = equipped.next();
    for row in &build.gear {
        for id in &row.upgrade_ids {
            if !build.sigil_ids.contains(id) {
                build.sigil_ids.push(*id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows verbatim from guildjen.com/support-troubadour-cloud-build/ as the
    /// server sent it on 2026-09-06: six armour pieces all carrying rune
    /// 24839, a two-handed rifle with two sigils, a one-hand pair with one
    /// each, and trinkets with none.
    const PVE: &str = r#"
      <table><tbody>
        <tr><td><span data-gw2-embed="items" data-gw2-id="73970"></span></td>
            <td><strong>Helm</strong><br><em>Minstrel</em></td>
            <td><span data-gw2-embed="items" data-gw2-id="24839"></span></td></tr>
        <tr><td><span data-gw2-embed="items" data-gw2-id="73670"></span></td>
            <td><strong>Shoulders</strong><br><em>Minstrel</em></td>
            <td><span data-gw2-embed="items" data-gw2-id="24839"></span></td></tr>
        <tr><td><span data-gw2-embed="items" data-gw2-id="73497"></span></td>
            <td><strong>Rifle</strong><br><em>Minstrel</em></td>
            <td><span data-gw2-embed="items" data-gw2-id="67340"></span>
                <span data-gw2-embed="items" data-gw2-id="74326"></span></td></tr>
        <tr><td><span data-gw2-embed="items" data-gw2-id="76730"></span></td>
            <td><strong>Sword</strong><br><em>Minstrel</em></td>
            <td><span data-gw2-embed="items" data-gw2-id="81045"></span></td></tr>
        <tr><td><span data-gw2-embed="items" data-gw2-id="79980"></span></td>
            <td><strong>Amulet</strong><br><em>Minstrel</em></td></tr>
      </tbody></table>
      <p><center>
        <span data-gw2-embed="traitline" data-gw2-id="45" data-gw2-traits="675,673,1687"></span>
        <span data-gw2-embed="traitline" data-gw2-id="23" data-gw2-traits="738,751,2005"></span>
        <span data-gw2-embed="traitline" data-gw2-id="73" data-gw2-traits="2326,2367,2441"></span>
      </center></p>
      <h3>Variants</h3>
      <span data-gw2-embed="traitline" data-gw2-id="1" data-gw2-traits="1,2,3"></span>
      <li><strong>Inspiration:</strong>
        <span data-gw2-embed="traits" data-gw2-id="756" data-gw2-size="20"></span></li>"#;

    #[test]
    fn the_rune_comes_from_the_armour_row_not_from_counting() {
        let build = parse(PVE);
        assert_eq!(build.rune_id, Some(24839), "the armour rows say so");
        assert_eq!(
            build.sigil_ids,
            vec![67340, 74326, 81045],
            "two off the rifle, one off the sword, in weapon order"
        );
    }

    #[test]
    fn every_row_keeps_its_slot_and_its_own_stat_prefix() {
        let build = parse(PVE);
        let slots: Vec<&str> = build.gear.iter().map(|r| r.slot.as_str()).collect();
        assert_eq!(
            slots,
            ["Helm", "Shoulders", "Rifle", "Sword", "Amulet"],
            "page order, labels intact"
        );
        assert!(build.gear.iter().all(|r| r.stat == "Minstrel"));
        assert_eq!(build.dominant_stat(), Some("Minstrel".into()));
        let amulet = build.gear.last().expect("a trinket row");
        assert!(amulet.is_trinket());
        assert!(amulet.upgrade_ids.is_empty(), "trinkets carry no upgrade");
    }

    /// Three lines, and the fourth under "Variants" is a swap suggestion.
    /// The lone `data-gw2-embed="traits"` is an alternative named in prose,
    /// not one of the build's nine traits.
    #[test]
    fn three_traitlines_with_their_traits_and_nothing_from_the_variants() {
        let build = parse(PVE);
        assert_eq!(
            build.specs,
            vec![
                SpecLine {
                    id: 45,
                    trait_ids: vec![675, 673, 1687]
                },
                SpecLine {
                    id: 23,
                    trait_ids: vec![738, 751, 2005]
                },
                SpecLine {
                    id: 73,
                    trait_ids: vec![2326, 2367, 2441]
                },
            ]
        );
        assert!(
            !build.specs.iter().any(|s| s.trait_ids.contains(&756)),
            "756 is the prose alternative, not a chosen trait"
        );
    }

    /// PvP labels no rows and instead fixes the order. Verbatim shape from
    /// guildjen.com/support-firebrand-pvp-build/: the amulet is a plain
    /// `<img>` with no embed, so the first two embedded items are the rune
    /// and the relic, and one embed packs both sigils of the set.
    #[test]
    fn pvp_reads_amulet_rune_relic_down_the_column() {
        const PVP: &str = r#"
          <table><tbody>
            <tr><td><img alt="Avatar Amulet"> PvP Amulet</td><td></td>
                <td><span data-gw2-embed="items" data-gw2-id="21152,94901"></span></td></tr>
            <tr><td><span data-gw2-embed="items" data-gw2-id="72415"></span></td><td></td>
                <td><span data-gw2-embed="items" data-gw2-id="21152,94901"></span></td></tr>
            <tr><td><span data-gw2-embed="items" data-gw2-id="99965"></span></td><td></td>
                <td></td></tr>
          </tbody></table>"#;

        let build = parse(PVP);
        assert_eq!(
            build.rune_id,
            Some(72415),
            "first embedded item is the rune"
        );
        assert_eq!(build.relic_id, Some(99965), "second is the relic");
        assert_eq!(
            build.sigil_ids,
            vec![21152, 94901],
            "one embed carries both, and the repeat is not a third sigil"
        );
    }

    /// A category index row verbatim from guildjen.com/gw2-open-world-builds/
    /// on 2026-09-06, plus the sidebar and the repeat that a naive scan
    /// picks up.
    const INDEX: &str = r#"
      <table><tbody>
      <tr id="post-row-77896" entity_id="77896" class="post-row post-77896 post type-post
          status-publish category-gw2-builds-open-world category-gw2-builds-raid
          tag-alacrity tag-luminary tag-support post_class-guardian post_difficulty-easy
          post_playstyle-cooperative post_role-support post_role-tank">
        <td><img alt=""></td>
        <td><p><a href="https://guildjen.com/heal-dps-luminary-support-build/">Heal DPS Luminary</a></p></td></tr>
      <tr id="post-row-77900" class="post-row post_class-thief post_difficulty-hard
          post_playstyle-cloud post_playstyle-havoc post_role-dps
          category-gw2-builds-wvw">
        <td><p><a href="https://guildjen.com/power-staff-daredevil-havoc-build/">Power Staff Daredevil</a></p></td></tr>
      <tr id="post-row-77896" class="post-row post_class-guardian">
        <td><p><a href="https://guildjen.com/heal-dps-luminary-support-build/">Heal DPS Luminary</a></p></td></tr>
      <tr class="not-a-post-row"><td>
        <a href="https://guildjen.com/power-reaper-roaming-build/">Trending row</a></td></tr>
      </tbody></table>
      <aside class="main-sidebar">
        <a href="https://guildjen.com/celestial-weaver-open-world-build/">Popular Posts</a></aside>"#;

    /// The index states the profession, the role and the playstyle, so
    /// nothing downstream has to read them out of a slug or page text.
    #[test]
    fn an_index_row_states_profession_role_and_playstyle() {
        let rows = index_rows(INDEX);
        assert_eq!(rows.len(), 2, "two distinct builds: {rows:?}");

        let first = &rows[0];
        assert_eq!(first.name, "Heal DPS Luminary");
        assert_eq!(first.profession, "Guardian");
        assert_eq!(first.roles, ["support", "tank"], "a build can hold two");
        assert_eq!(first.playstyles, ["cooperative"]);
        assert_eq!(first.difficulty, "easy");

        let second = &rows[1];
        assert_eq!(second.profession, "Thief");
        assert_eq!(second.roles, ["dps"]);
        assert_eq!(
            second.playstyles,
            ["cloud", "havoc"],
            "a WvW build commonly carries two"
        );
    }

    /// Each index repeats its whole catalogue in a searchable table at the
    /// foot of the page — 162 rows for 86 builds on the open-world index —
    /// so a walk that does not deduplicate fetches every page twice. The
    /// "Trending" sidebar is excluded structurally: it is not a `post-row`.
    #[test]
    fn the_repeat_table_and_the_sidebar_are_not_extra_builds() {
        let rows = index_rows(INDEX);
        assert_eq!(
            rows.iter()
                .filter(|r| r.url.contains("heal-dps-luminary"))
                .count(),
            1,
            "the repeat is the same build"
        );
        assert!(
            !rows.iter().any(|r| r.url.contains("power-reaper-roaming")),
            "a row that is not a post-row is not a build: {rows:?}"
        );
        assert!(
            !rows.iter().any(|r| r.url.contains("celestial-weaver")),
            "the Popular Posts sidebar links builds from other categories, \
             which is how a WvW build got filed under PvP: {rows:?}"
        );
    }

    /// A page that publishes nothing yields nothing, rather than a build
    /// assembled out of whatever happened to be on it.
    #[test]
    fn a_page_without_a_build_produces_no_build() {
        let build = parse("<html><body><p>No build here.</p></body></html>");
        assert!(build.is_empty());
        assert_eq!(build.rune_id, None);
        assert_eq!(build.dominant_stat(), None);
    }
}
