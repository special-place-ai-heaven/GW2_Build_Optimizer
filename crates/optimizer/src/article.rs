//! Telling a build article apart from the page around it.
//!
//! A community build page is mostly not the build. Around the article sit a
//! navigation bar, a cookie notice, share buttons, an author card, and — the
//! one that actually hurt us — a sidebar listing *other* builds by name.
//! Measured on 2026-09-06, a whole-page text scan recorded the
//! specializations `Firebrand / Willbender / Dragonhunter` for 431 of 739
//! scraped builds, whatever the profession, because those words appear in
//! GuildJen's "POPULAR POSTS" column on every page.
//!
//! This is a port of the scoring in crawl4ai's `PruningContentFilter`
//! (`crawl4ai/content_filter_strategy.py`), which is not a model or a
//! library but one recursive pass of arithmetic. Each element is scored on
//! how much of it is text, how much of that text is inside links, what kind
//! of tag it is, whether its class or id names it as furniture, and how much
//! text it holds; a subtree scoring below [`PRUNE_THRESHOLD`] is dropped and
//! the rest is walked.
//!
//! Link density is the term that does the work here. A sidebar of build
//! titles is almost entirely anchor text with no prose between; an article
//! body is the reverse. One ratio separates them, without a per-site
//! selector to maintain.
//!
//! Scoring only. Walking a real document needs a parser, and the caller
//! supplies the numbers — which also means the arithmetic can be tested
//! against measured pages without one.

/// Below this, an element and everything under it is not the article.
///
/// crawl4ai's default. Left alone until a page argues otherwise: the point
/// of porting a tuned heuristic is to inherit the tuning.
pub const PRUNE_THRESHOLD: f64 = 0.48;

/// Class and id fragments that name an element as furniture rather than
/// content. crawl4ai's list, matched anywhere in the value rather than only
/// at the start — see [`class_id_penalty`].
const FURNITURE: &[&str] = &[
    "nav", "footer", "header", "sidebar", "ads", "comment", "promo", "advert", "social", "share",
];

/// How much a tag suggests prose. Unlisted tags score 0.5, as in crawl4ai.
fn tag_weight(tag: &str) -> f64 {
    match tag {
        "article" => 1.5,
        "h1" => 1.2,
        "h2" => 1.1,
        "p" | "section" | "h3" => 1.0,
        "h4" => 0.9,
        "h5" => 0.8,
        "h6" => 0.7,
        "div" | "li" | "ul" | "ol" => 0.5,
        "span" => 0.3,
        _ => 0.5,
    }
}

/// `-0.5` per attribute naming the element as furniture, `0.0` otherwise.
///
/// Deliberately unlike the original in two ways.
///
/// crawl4ai wraps this in `max(0, class_score)`, and the score it wraps can
/// only ever be `0`, `-0.5` or `-1.0` — so the clamp makes the term always
/// zero and the signal never fires. Here the penalty is allowed through,
/// which is what the metric was named for.
///
/// It also uses Python's `re.match`, which anchors at the start, so
/// `class="widget sidebar"` does not match while `class="sidebar widget"`
/// does. Matched anywhere here: WordPress themes put the meaningful word
/// second at least as often as first.
fn class_id_penalty(class_and_id: &str) -> f64 {
    let lower = class_and_id.to_ascii_lowercase();
    let hits = FURNITURE.iter().filter(|w| lower.contains(**w)).count();
    if hits == 0 {
        0.0
    } else {
        -0.5
    }
}

/// One element, reduced to what the score needs.
#[derive(Debug, Clone, Copy)]
pub struct NodeMetrics<'a> {
    /// Lowercased tag name.
    pub tag: &'a str,
    /// `class` and `id` values, joined however the caller likes.
    pub class_and_id: &'a str,
    /// Length of all descendant text, whitespace-trimmed.
    pub text_len: usize,
    /// Length of the element's inner HTML, markup included.
    pub inner_html_len: usize,
    /// Text length of the element's *direct* `<a>` children only.
    ///
    /// Direct, not descendant: an article whose paragraphs contain links is
    /// still an article, while a list whose every child is a link is a menu.
    /// Counting descendants would erase that difference.
    pub direct_link_text_len: usize,
}

/// Text length at which the size term is considered maxed out.
///
/// crawl4ai adds a raw `ln(text_len + 1)`, which is unbounded while its four
/// other metrics are ratios in `0..1`. At realistic sizes that one term
/// swamps the rest: a 320-character sidebar contributes `0.1 * ln(321)` =
/// 0.58 on its own, clearing the 0.48 threshold before anything has been
/// judged, and the composite scores it 0.666 — kept, links and furniture
/// class and all. Under a fixed threshold the other four metrics cannot
/// outvote it, so the filter only ever drops near-empty elements.
///
/// Normalising against a full article's worth of prose puts the term back in
/// `0..1` with the others, which is what the published weights describe.
const FULL_ARTICLE_CHARS: f64 = 5_000.0;

/// Composite score in `0.0..=1.0` — higher is more article-like.
///
/// crawl4ai's metrics and weights, with its length term normalised (see
/// [`FULL_ARTICLE_CHARS`]) and its class/id penalty allowed to fire (see
/// [`class_id_penalty`]).
pub fn prune_score(m: &NodeMetrics) -> f64 {
    let text_density = if m.inner_html_len > 0 {
        m.text_len as f64 / m.inner_html_len as f64
    } else {
        0.0
    };
    // Inverted: all-link text scores 0, link-free text scores 1.
    let prose_density = if m.text_len > 0 {
        1.0 - (m.direct_link_text_len as f64 / m.text_len as f64).min(1.0)
    } else {
        0.0
    };
    let size = (((m.text_len + 1) as f64).ln() / FULL_ARTICLE_CHARS.ln()).min(1.0);

    0.4 * text_density
        + 0.2 * prose_density
        + 0.2 * tag_weight(m.tag)
        + 0.1 * class_id_penalty(m.class_and_id)
        + 0.1 * size
}

/// Whether this element and everything under it should be dropped.
pub fn should_prune(m: &NodeMetrics) -> bool {
    prune_score(m) < PRUNE_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes measured from guildjen.com/support-troubadour-cloud-build/ on
    /// 2026-09-06. The sidebar is the element that has to lose.
    #[test]
    fn the_sidebar_loses_and_the_article_survives() {
        // "POPULAR POSTS": ten build titles, each its own link, no prose.
        let sidebar = NodeMetrics {
            tag: "div",
            class_and_id: "widget-area sidebar",
            text_len: 320,
            inner_html_len: 4_800,
            direct_link_text_len: 300,
        };
        // The build's own text: several paragraphs, a couple of links.
        let article = NodeMetrics {
            tag: "article",
            class_and_id: "post-content entry",
            text_len: 2_400,
            inner_html_len: 5_200,
            direct_link_text_len: 40,
        };

        assert!(
            should_prune(&sidebar),
            "sidebar scored {:.3}, needed < {PRUNE_THRESHOLD}",
            prune_score(&sidebar)
        );
        assert!(
            !should_prune(&article),
            "article scored {:.3}, needed >= {PRUNE_THRESHOLD}",
            prune_score(&article)
        );
        assert!(prune_score(&article) > prune_score(&sidebar));
    }

    /// The distinction the whole thing rests on: a menu is links with nothing
    /// between them; an article is prose that happens to link out.
    #[test]
    fn link_density_separates_a_menu_from_prose() {
        let base = NodeMetrics {
            tag: "div",
            class_and_id: "",
            text_len: 400,
            inner_html_len: 1_200,
            direct_link_text_len: 0,
        };
        let prose = prune_score(&base);
        let menu = prune_score(&NodeMetrics {
            direct_link_text_len: 400,
            ..base
        });
        assert!(
            prose > menu,
            "prose {prose:.3} should beat all-link {menu:.3}"
        );
        // The term is worth exactly its weight, end to end.
        assert!((prose - menu - 0.2).abs() < 1e-9);
    }

    /// crawl4ai clamps this term to zero, so it never fires. Ours does.
    #[test]
    fn furniture_names_are_penalised() {
        let plain = NodeMetrics {
            tag: "div",
            class_and_id: "entry-content",
            text_len: 500,
            inner_html_len: 1_000,
            direct_link_text_len: 0,
        };
        for furniture in ["sidebar", "widget nav", "post-footer", "social-share"] {
            let scored = prune_score(&NodeMetrics {
                class_and_id: furniture,
                ..plain
            });
            assert!(
                scored < prune_score(&plain),
                "{furniture} should score below plain content"
            );
        }
        assert_eq!(class_id_penalty("entry-content"), 0.0);
        // Matched anywhere, not just at the start - the original misses this.
        assert_eq!(class_id_penalty("widget sidebar"), -0.5);
    }

    /// An empty wrapper has no text to be article-like with, and must not be
    /// rescued by the link-density term scoring a perfect 1.0 for having no
    /// links to divide by.
    #[test]
    fn empty_elements_do_not_score_as_content() {
        let empty = NodeMetrics {
            tag: "div",
            class_and_id: "",
            text_len: 0,
            inner_html_len: 0,
            direct_link_text_len: 0,
        };
        assert!(should_prune(&empty), "scored {:.3}", prune_score(&empty));
    }

    /// A single "POPULAR POSTS" entry: one link, its own title, nothing else.
    /// The container may survive on bulk; the leaves are what must not.
    #[test]
    fn a_sidebar_link_item_is_pruned() {
        let item = NodeMetrics {
            tag: "li",
            class_and_id: "",
            // "Power Zeal Dragonhunter Cloud Build" inside an anchor.
            text_len: 35,
            inner_html_len: 105,
            direct_link_text_len: 35,
        };
        assert!(should_prune(&item), "scored {:.3}", prune_score(&item));
    }

    /// Every term stays inside its published weight, so the threshold means
    /// what it says. crawl4ai's raw `ln(text_len + 1)` does not: it reaches
    /// 0.58 on a 320-character element and 0.78 on a full article, out of a
    /// nominal 0.1.
    #[test]
    fn no_single_term_can_outvote_the_rest() {
        let huge = NodeMetrics {
            tag: "span",
            class_and_id: "sidebar",
            text_len: 500_000,
            inner_html_len: 1_000_000,
            direct_link_text_len: 500_000,
        };
        let score = prune_score(&huge);
        assert!(
            (0.0..=1.0).contains(&score),
            "score left its range: {score:.3}"
        );
        assert!(
            should_prune(&huge),
            "all-link furniture must lose however large: {score:.3}"
        );
    }

    #[test]
    fn tag_weight_prefers_prose_containers() {
        assert!(tag_weight("article") > tag_weight("div"));
        assert!(tag_weight("p") > tag_weight("span"));
        assert_eq!(tag_weight("aside"), 0.5, "unlisted tags take the default");
    }
}
