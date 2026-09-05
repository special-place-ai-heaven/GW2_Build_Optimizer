//! Guild Wars 2 build template chat links (`[&DQ...]`), decoded.
//!
//! A build template is the game's own serialization of a build: profession,
//! three specializations with the trait chosen in each tier, and the skill
//! bar. It is authoritative in a way scraped prose never is — the site
//! publishes the exact thing the player would paste into chat, and every
//! field is a number rather than a name that has to be matched back against
//! the game data.
//!
//! This exists because the benchmark scraper was reading trait names out of
//! page text and getting them from a navigation menu: measured 2026-09-06,
//! 431 of 739 scraped builds recorded `Firebrand / Willbender / Dragonhunter`
//! whatever the profession, and 407 recorded the same `[&BPcAAAA=]` — a
//! five-byte *item* link, taken because the extractor kept the first `[&...]`
//! on the page and only checked that it was ten characters long.
//!
//! ## Layout
//!
//! All multi-byte values are little-endian. Skill slots interleave
//! terrestrial and aquatic; only the terrestrial ones are kept here.
//!
//! | Offset | Contents                                          |
//! |--------|---------------------------------------------------|
//! | 0      | `0x0D`, the build-template link type              |
//! | 1      | Profession code (1 Guardian … 9 Revenant)         |
//! | 2..8   | Three pairs of (specialization id, packed traits) |
//! | 8..28  | Heal, three utilities, elite — land then water    |
//! | 28..   | Profession-specific: ranger pets, revenant legends|
//!
//! Each packed-trait byte holds three 2-bit fields, adept first: `0` means no
//! trait chosen in that tier, `1..=3` selects one of the tier's three traits.

use base64::Engine;

/// The link type byte that marks a build template.
const BUILD_TEMPLATE_KIND: u8 = 0x0D;
/// Header, three specialization pairs, and the ten skill slots.
const MIN_LEN: usize = 28;

/// One specialization line of a build template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateSpec {
    /// Specialization id, matching `GameDb::specializations`. `0` = empty slot.
    pub id: u32,
    /// Chosen trait per tier, adept first. `0` = none, `1..=3` = which of the
    /// three traits in that tier.
    pub choices: [u8; 3],
}

/// A decoded `[&DQ...]` build template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTemplate {
    /// Profession code as the API's `Profession::code`.
    pub profession: u32,
    /// The three specialization slots, in order. Slot 3 may be elite.
    pub specs: [TemplateSpec; 3],
    /// Terrestrial skill palette ids: heal, three utilities, elite. `0` =
    /// empty. These are *palette* ids — `GameDb::palette_to_skill` maps them
    /// to skill ids.
    pub skills: [u32; 5],
}

impl BuildTemplate {
    /// Trait ids for one specialization, resolved against its major traits.
    ///
    /// `major_traits` is the specialization's nine majors in tier order, as
    /// the API gives them. A tier with no choice, or a specialization whose
    /// major list is not nine long, contributes nothing rather than guessing.
    pub fn trait_ids(spec: &TemplateSpec, major_traits: &[u32]) -> Vec<u32> {
        if major_traits.len() != 9 {
            return Vec::new();
        }
        spec.choices
            .iter()
            .enumerate()
            .filter_map(|(tier, &choice)| {
                let pick = usize::from(choice.checked_sub(1)?);
                (pick < 3).then(|| major_traits[tier * 3 + pick])
            })
            .collect()
    }
}

/// Decode a chat link, or `None` if it is not a build template.
///
/// Deliberately strict. Item, skill and trait links share the `[&...]`
/// syntax and appear all over a build page; accepting one produces a
/// confidently wrong build rather than an obvious failure.
pub fn decode(code: &str) -> Option<BuildTemplate> {
    let inner = code
        .trim()
        .strip_prefix("[&")
        .and_then(|s| s.strip_suffix(']'))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(inner.trim())
        .ok()?;
    if bytes.first() != Some(&BUILD_TEMPLATE_KIND) || bytes.len() < MIN_LEN {
        return None;
    }

    let spec = |i: usize| {
        let packed = bytes[3 + i * 2];
        TemplateSpec {
            id: u32::from(bytes[2 + i * 2]),
            choices: [packed & 0b11, (packed >> 2) & 0b11, (packed >> 4) & 0b11],
        }
    };
    // Land slot of each pair: heal, three utilities, elite.
    let skill = |i: usize| {
        let at = 8 + i * 4;
        u32::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
    };

    Some(BuildTemplate {
        profession: u32::from(bytes[1]),
        specs: [spec(0), spec(1), spec(2)],
        skills: [skill(0), skill(1), skill(2), skill(3), skill(4)],
    })
}

/// The first real build template in `text`, ignoring other chat links.
///
/// A build page carries item and skill links too, and the build's own code is
/// rarely the first one on the page.
pub fn find_in_text(text: &str) -> Option<(String, BuildTemplate)> {
    let mut pos = 0;
    while let Some(rel) = text[pos..].find("[&") {
        let start = pos + rel;
        let Some(end_rel) = text[start..].find(']') else {
            break;
        };
        let end = start + end_rel + 1;
        let candidate = &text[start..end];
        if let Some(template) = decode(candidate) {
            return Some((candidate.to_string(), template));
        }
        pos = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Necromancer template, and the item link that was being taken
    /// instead of it. Both were pulled from the live benchmark store on
    /// 2026-09-06, where the item link had been recorded as the build code of
    /// 407 of 739 builds.
    const NECRO: &str = "[&DQgnNjI1PCp+FgAAgAAAAHUBAABvAQAAkgAAAAAAAAAAAAAAAAAAAAAAAAA=]";
    const ITEM_LINK: &str = "[&BPcAAAA=]";
    /// Verbatim from guildjen.com/support-troubadour-cloud-build/ on
    /// 2026-09-06, where the page prints it under a "Chat Code:" heading.
    const TROUBADOUR: &str =
        "[&DQcXFi02STqJHQ8BhQFmAYQdfwFrHWQBbR2aAQAAAAAAAAAAAAAAAAAAAAADVQBaADEAAA==]";

    #[test]
    fn decodes_profession_specs_and_traits() {
        let t = decode(NECRO).expect("a build template");
        assert_eq!(t.profession, 8, "8 is Necromancer");
        assert_eq!(
            t.specs.map(|s| s.id),
            [39, 50, 60],
            "Death Magic, Blood Magic, Reaper"
        );
        // 0x36 = 0b00110110: adept 2, master 1, grandmaster 3.
        assert_eq!(t.specs[0].choices, [2, 1, 3]);
        assert!(t.skills.iter().any(|&s| s != 0), "a skill bar was decoded");
    }

    /// A second real page, a different profession, and a code long enough to
    /// carry the profession-specific tail. Whatever else a build page does
    /// or does not print, this one field carries the build.
    #[test]
    fn decodes_a_live_guildjen_page_code() {
        let t = decode(TROUBADOUR).expect("a build template");
        assert_eq!(t.profession, 7, "7 is Mesmer - the page is a Troubadour");
        assert!(
            t.specs.iter().all(|s| s.id != 0),
            "three specializations: {:?}",
            t.specs.map(|s| s.id)
        );
        assert!(
            t.specs.iter().all(|s| s.choices.iter().all(|&c| c <= 3)),
            "every trait choice is none or one of three: {:?}",
            t.specs.map(|s| s.choices)
        );
        assert!(
            t.skills.iter().filter(|&&s| s != 0).count() >= 4,
            "a real bar, not an empty one: {:?}",
            t.skills
        );
    }

    #[test]
    fn rejects_every_other_kind_of_chat_link() {
        // The whole bug in one assertion: this is an item, not a build.
        assert_eq!(decode(ITEM_LINK), None);
        assert_eq!(decode("[&BkgAAAA=]"), None, "skill link");
        assert_eq!(decode("not a link"), None);
        assert_eq!(decode("[&notbase64!]"), None);
        // Right kind, truncated: a short body cannot carry a skill bar.
        assert_eq!(decode("[&DQgnNjI1PCp+]"), None);
    }

    #[test]
    fn finds_the_build_code_past_other_links() {
        let page = format!(
            "<p>Runs {ITEM_LINK} and {}</p><code>{NECRO}</code>",
            "[&BkgAAAA=]"
        );
        let (code, template) = find_in_text(&page).expect("the build, not the item");
        assert_eq!(code, NECRO);
        assert_eq!(template.profession, 8);
        assert_eq!(find_in_text("no links here at all"), None);
    }

    #[test]
    fn trait_ids_resolve_against_the_specialization() {
        let majors: Vec<u32> = (1..=9).collect();
        let spec = TemplateSpec {
            id: 39,
            choices: [2, 1, 3],
        };
        // Second of tier one, first of tier two, third of tier three:
        // majors[0*3+1], majors[1*3+0], majors[2*3+2].
        assert_eq!(BuildTemplate::trait_ids(&spec, &majors), vec![2, 4, 9]);

        // An empty tier contributes nothing.
        let partial = TemplateSpec {
            id: 39,
            choices: [1, 0, 2],
        };
        assert_eq!(BuildTemplate::trait_ids(&partial, &majors), vec![1, 8]);

        // A specialization we cannot resolve is skipped, not guessed at.
        assert!(BuildTemplate::trait_ids(&spec, &[1, 2, 3]).is_empty());
    }
}
