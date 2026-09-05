//! Builds published as GW2 API ids rather than prose.
//!
//! GuildJen renders every item, trait, specialization and skill through a
//! WordPress plugin (`gw2-embeddings-patched`) that ships the *id* in the
//! markup and resolves the name in the browser. That is why a text scan of
//! the server's 808 KB finds zero occurrences of "Superior Rune", and why
//! scraping names off that page was never going to work:
//!
//! ```html
//! <span data-gw2-embed="items"     data-gw2-id="24839">     <!-- the rune, once per armour piece -->
//! <span data-gw2-embed="traitline" data-gw2-id="73">        <!-- a specialization -->
//! <span data-gw2-embed="skills"    data-gw2-id="76695,10237,76611,77178,76971">
//! ```
//!
//! Ids are better than the names would have been. They need no fuzzy
//! matching, they do not shift with the site's wording, and they are the
//! same numbers `GameDb` is keyed on — so a benchmark stored as ids stays
//! correct when the game data updates underneath it.
//!
//! Resolution is deliberately not done here: the scraper has no `GameDb` and
//! should not carry one. This returns the numbers; whoever has the database
//! turns them into names.

use std::collections::BTreeSet;

/// Ids a build page published, by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmbeddedIds {
    /// Item ids: runes, sigils, relics, and any gear the page names.
    pub items: Vec<u32>,
    /// Specialization ids, in page order.
    pub traitlines: Vec<u32>,
    /// Individual trait ids.
    pub traits: Vec<u32>,
    /// Skill ids. One embed can list a whole bar, comma separated.
    pub skills: Vec<u32>,
}

impl EmbeddedIds {
    /// Whether the page published anything at all.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
            && self.traitlines.is_empty()
            && self.traits.is_empty()
            && self.skills.is_empty()
    }
}

/// Every `data-gw2-embed` id on the page, grouped by kind.
///
/// Duplicates are kept for items and dropped elsewhere: a rune appears once
/// per armour piece and that repetition is the evidence it is *the* rune
/// rather than an alternative mentioned in passing, while a specialization
/// listed twice is just the page repeating itself.
pub fn extract(html: &str) -> EmbeddedIds {
    let document = ::html::Html::parse_document(html);
    let selector =
        ::html::Selector::parse("[data-gw2-embed][data-gw2-id]").expect("valid selector");
    let mut found = EmbeddedIds::default();
    let mut traitlines = BTreeSet::new();
    let mut traits = BTreeSet::new();
    let mut skills = BTreeSet::new();

    for element in document.select(&selector) {
        let value = element.value();
        let (Some(kind), Some(raw)) = (value.attr("data-gw2-embed"), value.attr("data-gw2-id"))
        else {
            continue;
        };
        // One embed can carry a comma-separated bar.
        let ids = raw.split(',').filter_map(|id| id.trim().parse::<u32>().ok());
        match kind {
            "items" => found.items.extend(ids),
            "traitline" | "traitlines" => traitlines.extend(ids),
            "traits" => traits.extend(ids),
            "skills" => skills.extend(ids),
            _ => {}
        }
    }
    found.traitlines = traitlines.into_iter().collect();
    found.traits = traits.into_iter().collect();
    found.skills = skills.into_iter().collect();
    found
}

/// The item id repeated most often, which on a build page is the rune: it is
/// embedded once per armour piece while sigils appear once or twice.
///
/// `None` when nothing repeats, rather than guessing at the first id — a
/// page that names one item does not thereby name a rune.
pub fn most_repeated_item(ids: &[u32]) -> Option<u32> {
    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for id in ids {
        *counts.entry(*id).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .max_by_key(|(id, n)| (*n, *id))
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes taken verbatim from guildjen.com/support-troubadour-cloud-build/
    /// as the server sent it on 2026-09-06.
    const PAGE: &str = r#"
        <div class="entry">
          <span data-gw2-embed="items" data-gw2-id="24839"></span>
          <span data-gw2-embed="items" data-gw2-id="24839"></span>
          <span data-gw2-embed="items" data-gw2-id="24839"></span>
          <span data-gw2-embed="items" data-gw2-id="80058"></span>
          <span data-gw2-embed="items" data-gw2-id="80002"></span>
          <span data-gw2-embed="traitline" data-gw2-id="73"></span>
          <span data-gw2-embed="traitline" data-gw2-id="45"></span>
          <span data-gw2-embed="traitline" data-gw2-id="23"></span>
          <span data-gw2-embed="traits" data-gw2-id="756"></span>
          <span data-gw2-embed="skills" data-gw2-id="76695,10237,76611"></span>
          <span data-gw2-embed="unknown-kind" data-gw2-id="999"></span>
          <span data-gw2-embed="items">no id at all</span>
        </div>"#;

    #[test]
    fn every_kind_of_id_is_recovered() {
        let ids = extract(PAGE);
        assert_eq!(ids.items, vec![24839, 24839, 24839, 80058, 80002]);
        assert_eq!(ids.traitlines, vec![23, 45, 73]);
        assert_eq!(ids.traits, vec![756]);
        assert_eq!(ids.skills, vec![10237, 76611, 76695], "a bar is one embed");
        assert!(!ids.is_empty());
    }

    #[test]
    fn an_unknown_kind_or_a_missing_id_is_skipped_not_guessed() {
        let ids = extract(PAGE);
        assert!(!ids.items.contains(&999), "unknown kind must not be an item");
        assert_eq!(ids.items.len(), 5, "the id-less embed contributes nothing");
        assert!(extract("<p>no embeds here</p>").is_empty());
    }

    /// The rune is embedded once per armour piece; sigils are not.
    #[test]
    fn the_repeated_item_is_the_rune() {
        let ids = extract(PAGE);
        assert_eq!(most_repeated_item(&ids.items), Some(24839));
        // Nothing repeats: no claim is made.
        assert_eq!(most_repeated_item(&[80058, 80002]), None);
        assert_eq!(most_repeated_item(&[]), None);
    }
}
