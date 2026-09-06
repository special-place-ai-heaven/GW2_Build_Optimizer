//! Hardstuck — ids on a `<gw2object>` custom element.
//!
//! ```html
//! <gw2object class="gw2-build-item noembed" objid="21188" type="item">
//! <gw2object class="specialization" objid="19" selected_traits="0,2,0"
//!            type="specialization"> … </gw2object>
//! ```
//!
//! Three things about this site are not guessable from one page.
//!
//! **A page holds several builds.** Each `div.build-variant` is a complete
//! build — its own chat code, its own gear, its own traits — and pages carry
//! one to three of them. Scanning the whole document takes variant 0's code
//! and pairs it with whatever gear the scan happened to reach: on
//! `guardian/power-dragonhunter` the three codes decode to specs `[16,42,27]`,
//! `[42,46,27]` and `[16,42,27]`, which are different builds. Only the
//! active variant is read here.
//!
//! **Equipped and mentioned look alike.** `gw2object.gw2-build-item.noembed`
//! inside an equipment container is worn; `gw2object.gw2objectembed` inside
//! a `<p>` is prose. On `revenant/power-vindicator` the prose names Sigil of
//! Escape, Rune of the Dolyak, Rune of the Fighter and Relic of the
//! Daredevil, none of them equipped — a scan without the container split
//! records the Daredevil relic as the build's.
//!
//! **The chat code cannot survive pruning.** It lives only in an
//! `<input value>`, and `<input>` is a void element: `text_len` 0, score 0.1
//! against a 0.48 threshold. Measured, `<input>` 31 to 0 and `[&` 1 to 0.
//!
//! One typo is load-bearing: the traits wrapper is `gw2-biuld-traits`, not
//! `gw2-build-traits`. Selecting the correct spelling matches nothing.

use super::{GearRow, ProviderBuild, SpecLine};

/// Read one Hardstuck build page from the raw response.
///
/// Reads the variant the page opens on. The others are real builds too, but
/// nothing downstream can hold more than one build per URL yet, and taking
/// the visible one is at least the build a reader would see.
pub fn parse(html: &str) -> ProviderBuild {
    let document = ::html::Html::parse_document(html);
    let active = ::html::Selector::parse("div.build-variant.variant-is-active")
        .expect("valid selector");
    let any = ::html::Selector::parse("div.build-variant").expect("valid selector");
    let Some(variant) = document
        .select(&active)
        .next()
        .or_else(|| document.select(&any).next())
    else {
        return ProviderBuild::default();
    };

    let mut build = ProviderBuild {
        build_code: build_code(&variant),
        specs: specs(&variant),
        skill_ids: skill_bar(&variant),
        ..Default::default()
    };
    equipment(&variant, &mut build);
    build.amulet_id = amulet(&variant);
    build
}

/// Which container a piece sits in — the only thing on the page that says
/// whether an upgrade is a rune or a sigil.
///
/// The visible label cannot: it reads `Dragon's Helm` on a PvE page (stat
/// and slot together) but plain `Hammer` on a PvP one, so classifying by
/// label puts all six copies of the rune into the sigil list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Armour,
    Weapon,
    Trinket,
    /// PvP replaces armour and trinkets with one container holding the
    /// amulet, the rune and the relic. The rune is *worn* here rather than
    /// slotted into six armour pieces, so it is the item, not an upgrade.
    Pvp,
    Other,
}

impl Container {
    fn of(classes: &str) -> Self {
        for class in classes.split_whitespace() {
            match class {
                "armors" => return Self::Armour,
                "weapons" => return Self::Weapon,
                "trinkets" => return Self::Trinket,
                "pvp" => return Self::Pvp,
                _ => {}
            }
        }
        Self::Other
    }
}

/// The variant's own chat code, out of the `<input value>` beside its copy
/// button. Not entity-escaped — base64 has no `&` — but still validated,
/// because the value is only a build if it decodes as one.
fn build_code(variant: &::html::ElementRef<'_>) -> Option<String> {
    let selector =
        ::html::Selector::parse("div.copy-template input[type=text]").expect("valid selector");
    variant
        .select(&selector)
        .filter_map(|input| input.value().attr("value"))
        .find(|value| crate::build_template::decode(value).is_some())
        .map(str::to_string)
}

/// The three specialization lines with the majors this build chose.
///
/// The chosen trait is the one marked `trait_major` *without*
/// `trait_unselected`; `trait_minor` are the automatic ones. `selected_traits`
/// says the same thing as tier indices, but zero-based where the chat code
/// is one-based, so the ids are read directly instead.
///
/// `:not(.gw2objectembed)` matters: a specialization can also be named in
/// prose, and `mesmer/power-chrono` has a fourth that way.
fn specs(variant: &::html::ElementRef<'_>) -> Vec<SpecLine> {
    let line = ::html::Selector::parse(
        "div.gw2-biuld-traits gw2object.specialization[type=specialization]:not(.gw2objectembed)",
    )
    .expect("valid selector");
    let chosen = ::html::Selector::parse("gw2object[type=trait].trait_major:not(.trait_unselected)")
        .expect("valid selector");
    variant
        .select(&line)
        .filter_map(|spec| {
            Some(SpecLine {
                id: spec.value().attr("objid")?.trim().parse().ok()?,
                trait_ids: spec
                    .select(&chosen)
                    .filter_map(|t| t.value().attr("objid")?.trim().parse().ok())
                    .collect(),
            })
        })
        .collect()
}

/// Heal, three utilities, elite, as real skill ids.
///
/// A variant can hold several utility sets — a revenant's two legends, an
/// elementalist's attunements — so the first set is the bar, not the only
/// one.
fn skill_bar(variant: &::html::ElementRef<'_>) -> Vec<u32> {
    let selector = ::html::Selector::parse(
        "div.gw2-build-equipment-container.skills-utility \
         div.gw2-build-equipment-piece.skills-utility-set gw2object[type=skill]",
    )
    .expect("valid selector");
    variant
        .select(&selector)
        .filter_map(|s| s.value().attr("objid")?.trim().parse().ok())
        .take(5)
        .collect()
}

/// Every worn piece, plus the rune, sigils and relic its container names.
///
/// `slotted` is deliberately not read: it interleaves infusions with sigils
/// — `slotted="24597,43254,24548,43254"` is sigil, infusion, sigil, infusion
/// — while the rendered children are only the rune and the sigils.
fn equipment(variant: &::html::ElementRef<'_>, build: &mut ProviderBuild) {
    let container =
        ::html::Selector::parse("div.gw2-build-equipment-container").expect("valid selector");
    let piece = ::html::Selector::parse("div.gw2-build-equipment-piece").expect("valid selector");
    let worn =
        ::html::Selector::parse("gw2object.gw2-build-item.noembed[type=item]").expect("valid");
    let info = ::html::Selector::parse("div.gw2-build-equipment-info span").expect("valid");
    let relic_icon = ::html::Selector::parse(r#"img[alt="Relic icon"]"#).expect("valid selector");

    for group in variant.select(&container) {
        let kind = Container::of(group.value().attr("class").unwrap_or_default());
        for slot in group.select(&piece) {
            let worn_ids: Vec<u32> = slot
                .select(&worn)
                .filter_map(|o| o.value().attr("objid")?.trim().parse().ok())
                .collect();
            let Some((&item_id, upgrades)) = worn_ids.split_first() else {
                continue;
            };
            // The label reads `Dragon's Helm` on a PvE page and plain
            // `Hammer` on a PvP one, so the slot is its last word and
            // whatever precedes it is the stat. Outside the three gear
            // containers the label is an item name — "Relic of the
            // Dragonhunter", "Cilantro Lime Sous-Vide Steak" — and naming a
            // slot from it would invent one.
            // The relic's own alt is always the literal "Relic icon" whatever
            // relic it is, and its container moves with game mode — `.pvp` in
            // PvP, an extra piece after the six trinkets in PvE and WvW — so
            // it is found by that marker, never by position. A variant can
            // carry none at all.
            let is_relic = slot.select(&relic_icon).next().is_some();
            if is_relic {
                build.relic_id = Some(item_id);
            }

            // In PvP the rune is worn on its own rather than slotted into
            // six armour pieces, so it is the item itself. The amulet shares
            // the container but is a `pvp/amulet` object, which the worn
            // selector does not match, so it never reaches here.
            if kind == Container::Pvp && !is_relic && build.rune_id.is_none() {
                build.rune_id = Some(item_id);
            }

            let (slot_name, stat) = match kind {
                // The relic sits among the trinkets but its label is the
                // relic's name, so splitting it would yield "Dragonhunter"
                // as a slot and "Relic of the" as a stat prefix.
                _ if is_relic => ("Relic".to_string(), String::new()),
                // A PvP rune's label is its name, and the page states no
                // stat at all in this mode — the amulet is the stat source.
                Container::Pvp => ("Rune".to_string(), String::new()),
                Container::Other => (String::new(), String::new()),
                _ => {
                    let label = slot
                        .select(&info)
                        .next()
                        .map(|s| s.text().collect::<String>().trim().to_string())
                        .unwrap_or_default();
                    let mut words: Vec<&str> = label.split_whitespace().collect();
                    let name = words.pop().unwrap_or_default().to_string();
                    (name, words.join(" "))
                }
            };

            match kind {
                Container::Armour if build.rune_id.is_none() => {
                    build.rune_id = upgrades.first().copied();
                }
                Container::Weapon => build.sigil_ids.extend(upgrades.iter().copied()),
                _ => {}
            }
            build.gear.push(GearRow {
                slot: slot_name,
                stat,
                item_id: Some(item_id),
                upgrade_ids: upgrades.to_vec(),
            });
        }
    }
}

/// The PvP amulet, which is that mode's whole stat source.
fn amulet(variant: &::html::ElementRef<'_>) -> Option<u32> {
    let selector =
        ::html::Selector::parse(r#"gw2object[type="pvp/amulet"]"#).expect("valid selector");
    variant
        .select(&selector)
        .find_map(|a| a.value().attr("objid")?.trim().parse().ok())
}

/// The build's own name, which is where its role is stated.
///
/// `<h1>` reads `Power Dragonhunter` then a publication date on its own
/// line; the title says the same thing followed by " Build (Group PvE)".
/// Either names the job — Power, Condition, Heal Alacrity, Support — where
/// asking whether the page text contains "condi" answers yes on nearly
/// every page, which is why 142 of 157 stored Hardstuck rows are "Condi
/// DPS".
pub fn build_name(html: &str) -> Option<String> {
    let document = ::html::Html::parse_document(html);
    let h1 = ::html::Selector::parse("h1").expect("valid selector");
    let raw = document.select(&h1).next()?.text().collect::<String>();
    // `<h1>Heal Alacrity Tempest\t\t\t\t\tFebruary 2025</h1>` — the date
    // trails the name inside the same element, separated by a run of
    // whitespace. Splitting on that run is what tells them apart; splitting
    // on the first digit would keep "February", and normalising the
    // whitespace first would destroy the only separator there is.
    let name = raw
        .char_indices()
        .zip(raw.char_indices().skip(1))
        .find(|((_, a), (_, b))| a.is_whitespace() && b.is_whitespace())
        .map_or(raw.as_str(), |((at, _), _)| &raw[..at]);
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then_some(name)
}

/// Mode and scale as the page files them, from the game type in `<title>`.
///
/// Hardstuck's four game types are `open-world`, `group-pve`, `wvw` and
/// `pvp` — the same solo/group/competitive split GuildJen makes with its
/// category pages. The scale is empty outside PvE, where the mode already
/// says how many people are involved.
pub fn mode_and_scale(html: &str) -> Option<(&'static str, &'static str)> {
    let document = ::html::Html::parse_document(html);
    let title = ::html::Selector::parse("title").expect("valid selector");
    let text = document.select(&title).next()?.text().collect::<String>();
    // "Power Dragonhunter Build (Group PvE)  - Hardstuck"
    let inside = text.split_once('(')?.1.split_once(')')?.0.trim().to_ascii_lowercase();
    match inside.as_str() {
        "open world" | "open-world" => Some(("PvE", "Open World")),
        "group pve" | "group-pve" => Some(("PvE", "Group")),
        "pve" => Some(("PvE", "")),
        "wvw" => Some(("WvW", "")),
        "pvp" => Some(("PvP", "")),
        _ => None,
    }
}

/// The game mode the page states, rather than one inferred from its text.
///
/// The old heuristic asked whether the page contained "pvp", and every
/// Hardstuck page does — the filter nav names all three modes. Measured
/// across eleven pages it answered PvP eleven times while the class said six
/// PvE, four PvP and one WvW, which is why the store held nine
/// `hardstuck_*_pvp.json` files and no PvE or WvW ones at all.
pub fn game_mode(html: &str) -> Option<&'static str> {
    let document = ::html::Html::parse_document(html);
    let selector =
        ::html::Selector::parse("div.full-width-section-small.gw2-build").expect("valid selector");
    let classes = document.select(&selector).next()?.value().attr("class")?;
    classes.split_whitespace().find_map(|class| match class {
        "gamemode-pve" => Some("PvE"),
        "gamemode-pvp" => Some("PvP"),
        "gamemode-wvw" => Some("WvW"),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes verbatim from hardstuck.gg, 2026-09-06: the traits block from
    /// necromancer/blood-harbinger (typo and all) and the weapon piece from
    /// revenant/power-vindicator.
    const PAGE: &str = r#"
      <div class="full-width-section-small gw2-build gamemode-pvp">
      <div class="build-variant variant-is-active">
        <div class="copy-template"><input type="text" readonly
          value="[&DQgTHScaQCXnGgAAvQEAAC4BAADrGgAA6BoAAAAAAAAAAAAAAAAAAAAAAAA=]"></div>
        <div class="gw2-biuld-traits">
          <gw2object class="specialization" data-label="Blood_Magic" objid="19"
                     selected_traits="0,2,0" type="specialization">
            <gw2object type="trait" objid="792" class="trait_minor"></gw2object>
            <div class="traitwrapper">
              <gw2object type="trait" objid="780" class="skeleton trait_major"></gw2object>
              <gw2object type="trait" objid="788" class="skeleton trait_major trait_unselected"></gw2object>
              <gw2object type="trait" objid="1876" class="skeleton trait_major trait_unselected"></gw2object>
            </div>
          </gw2object>
        </div>
        <div class="gw2-build-equipment-container armors">
          <div class="gw2-build-equipment-piece armor">
            <gw2object class="gw2-build-item noembed" objid="80384" type="item"></gw2object>
            <gw2object class="gw2-build-item noembed" objid="24836" type="item"></gw2object>
            <div class="gw2-build-equipment-info"><span>Helm</span></div></div>
        </div>
        <div class="gw2-build-equipment-container weapons"><div class="weapon-set">
          <div class="gw2-build-equipment-piece weapon">
            <gw2object class="gw2-build-item noembed" objid="91652" type="item" slotted="21149,21152"></gw2object>
            <gw2object class="gw2-build-item noembed" objid="21149" type="item"></gw2object>
            <gw2object class="gw2-build-item noembed" objid="21152" type="item"></gw2object>
            <div class="gw2-build-equipment-info"><span>Hammer</span>
              <span><span>Sigil of Intelligence</span></span></div></div>
        </div></div>
        <div class="gw2-build-equipment-container pvp">
          <div class="gw2-build-equipment-piece pvp">
            <gw2object type="pvp/amulet" objid="34"></gw2object>
            <div class="gw2-build-equipment-info"><span>Amulet</span></div></div>
          <div class="gw2-build-equipment-piece pvp">
            <gw2object class="gw2-build-item noembed" objid="102245" type="item">
              <a href="…/Special:Search/Relic of Atrocity"><img alt="Relic icon"></a></gw2object>
            <div class="gw2-build-equipment-info"><span>Relic</span></div></div>
        </div>
        <div class="gw2-build-equipment-container skills-utility">
          <div class="gw2-build-equipment-piece skills-utility-set">
            <gw2object type="skill" objid="62667"></gw2object>
            <gw2object type="skill" objid="10685"></gw2object>
            <gw2object type="skill" objid="10543"></gw2object>
            <gw2object type="skill" objid="62514"></gw2object>
            <gw2object type="skill" objid="62655"></gw2object></div>
        </div>
        <p>Consider <gw2object class="gw2objectembed" type="item" objid="21176"></gw2object>
           Rune of the Dolyak instead.</p>
      </div></div>"#;

    #[test]
    fn the_chosen_major_is_the_one_not_marked_unselected() {
        let build = parse(PAGE);
        assert_eq!(
            build.specs,
            vec![SpecLine { id: 19, trait_ids: vec![780] }],
            "780 is chosen; 788 and 1876 are unselected and 792 is a minor"
        );
    }

    /// Prose names gear the build does not wear. Without the container
    /// split, "Rune of the Dolyak" becomes the build's rune.
    #[test]
    fn only_worn_gear_counts_not_what_the_prose_suggests() {
        let build = parse(PAGE);
        assert_eq!(build.rune_id, Some(24836), "the second object in the armour piece");
        assert_ne!(build.rune_id, Some(21176), "21176 is only mentioned");
        assert!(!build.gear.iter().any(|r| r.item_id == Some(21176)));
    }

    #[test]
    fn sigils_come_off_the_weapon_and_slotted_is_not_trusted() {
        let build = parse(PAGE);
        assert_eq!(build.sigil_ids, vec![21149, 21152]);
        let weapon = build.gear.iter().find(|r| r.slot == "Hammer").expect("weapon");
        assert_eq!(weapon.item_id, Some(91652), "the weapon itself is the first child");
    }

    /// The relic's alt is always the literal "Relic icon" whatever relic it
    /// is, and its container moves with the game mode.
    #[test]
    fn the_relic_is_found_by_its_marker_not_its_position() {
        assert_eq!(parse(PAGE).relic_id, Some(102245));
    }

    #[test]
    fn the_build_code_and_amulet_and_bar_are_read_from_the_active_variant() {
        let build = parse(PAGE);
        let code = build.build_code.as_deref().expect("a chat code");
        assert_eq!(
            crate::build_template::decode(code).expect("decodes").profession,
            8,
            "8 is Necromancer"
        );
        assert_eq!(build.amulet_id, Some(34));
        assert_eq!(build.skill_ids, vec![62667, 10685, 10543, 62514, 62655]);
    }

    /// Every page names all three modes in its filter nav, so asking whether
    /// the text contains "pvp" answered PvP for all eleven pages measured.
    #[test]
    fn the_game_mode_is_the_one_the_page_states() {
        assert_eq!(game_mode(PAGE), Some("PvP"));
        assert_eq!(
            game_mode(&PAGE.replace("gamemode-pvp", "gamemode-pve")),
            Some("PvE"),
            "even though the page still says 'pvp' in several places"
        );
        assert_eq!(game_mode("<div>no build here</div>"), None);
    }

    /// PvP has no armour, so the rune is worn on its own in the same
    /// container as the amulet and the relic. Shape and ids verbatim from
    /// revenant/power-vindicator, whose equipped rune is 21202.
    #[test]
    fn a_pvp_rune_is_worn_rather_than_slotted() {
        const PVP: &str = r#"
          <div class="build-variant variant-is-active">
            <div class="gw2-build-equipment-container pvp">
              <div class="gw2-build-equipment-piece pvp">
                <gw2object type="pvp/amulet" objid="8"></gw2object>
                <div class="gw2-build-equipment-info"><span>Amulet</span></div></div>
              <div class="gw2-build-equipment-piece pvp">
                <gw2object class="gw2-build-item noembed" objid="21202" type="item">
                  <img alt="PvP rune icon"></gw2object>
                <div class="gw2-build-equipment-info"><span>Rune of Infiltration</span></div></div>
              <div class="gw2-build-equipment-piece pvp">
                <gw2object class="gw2-build-item noembed" objid="99997" type="item">
                  <img alt="Relic icon"></gw2object>
                <div class="gw2-build-equipment-info"><span>Relic of Peitha</span></div></div>
            </div>
          </div>"#;

        let build = parse(PVP);
        assert_eq!(build.rune_id, Some(21202), "worn, not slotted into armour");
        assert_eq!(build.relic_id, Some(99997), "the relic shares the container");
        assert_eq!(build.amulet_id, Some(8));
        assert_eq!(
            build.dominant_stat(),
            None,
            "PvP states no gear prefix — the amulet is the stat source"
        );
    }

    /// Titles and headings verbatim from three Hardstuck pages, 2026-09-06.
    /// The heading carries the publication date after a run of tabs, and the
    /// title carries the game type in parentheses.
    #[test]
    fn the_name_and_game_type_are_read_from_what_the_page_states() {
        for (title, heading, name, mode, scale) in [
            (
                "Power Dragonhunter Build (Group PvE)  - Hardstuck",
                "Power Dragonhunter\t\t\t\t\t\tJune 2024",
                "Power Dragonhunter",
                "PvE",
                "Group",
            ),
            (
                "Blood Harbinger Build (PvP)  - Hardstuck",
                "Blood Harbinger\t\t\t\t\t\tFebruary 2025",
                "Blood Harbinger",
                "PvP",
                "",
            ),
            (
                "Heal Alacrity Tempest Build (Group PvE)  - Hardstuck",
                "Heal Alacrity Tempest\t\t\t\t\t\tFebruary 2025",
                "Heal Alacrity Tempest",
                "PvE",
                "Group",
            ),
        ] {
            let page = format!("<html><head><title>{title}</title></head><body><h1>{heading}</h1></body></html>");
            assert_eq!(
                build_name(&page).as_deref(),
                Some(name),
                "the date must not join the name: {heading:?}"
            );
            assert_eq!(mode_and_scale(&page), Some((mode, scale)), "{title:?}");
        }
    }

    /// A build whose name states no job says so, rather than defaulting to
    /// Power DPS the way the old text scan did.
    #[test]
    fn a_name_without_a_job_claims_none() {
        assert_eq!(super::super::role_in_name("Blood Harbinger"), None);
        assert_eq!(
            super::super::role_in_name("Heal Alacrity Tempest"),
            Some("Healer"),
            "a healer first, not a boon build"
        );
        assert_eq!(
            super::super::role_in_name("Power Dragonhunter"),
            Some("Power DPS")
        );
    }

    #[test]
    fn a_page_with_no_variant_yields_nothing() {
        assert!(parse("<html><body><p>nothing</p></body></html>").is_empty());
    }
}
