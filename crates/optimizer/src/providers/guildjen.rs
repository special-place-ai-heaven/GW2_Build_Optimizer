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
    let embed =
        ::html::Selector::parse(r#"[data-gw2-embed="items"][data-gw2-id]"#).expect("valid selector");
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
                SpecLine { id: 45, trait_ids: vec![675, 673, 1687] },
                SpecLine { id: 23, trait_ids: vec![738, 751, 2005] },
                SpecLine { id: 73, trait_ids: vec![2326, 2367, 2441] },
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
        assert_eq!(build.rune_id, Some(72415), "first embedded item is the rune");
        assert_eq!(build.relic_id, Some(99965), "second is the relic");
        assert_eq!(
            build.sigil_ids,
            vec![21152, 94901],
            "one embed carries both, and the repeat is not a third sigil"
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
