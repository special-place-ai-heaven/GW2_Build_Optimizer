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
//! Two things separate the article from the page, in this order.
//!
//! **Landmarks do the work.** GuildJen marks its sidebar
//! `<aside class="col-4 main-sidebar">`, its menus `<nav>`, its foot
//! `<footer>`. A landmark tag is a statement of intent by the page author,
//! which beats any ratio we could compute. Dropping [`NEVER_CONTENT`] takes
//! the "POPULAR POSTS" column with it.
//!
//! **Scoring is cleanup.** A port of crawl4ai's `PruningContentFilter`
//! (`crawl4ai/content_filter_strategy.py`) scores what remains on text
//! density, link density, tag and class, and drops the near-empty wrappers a
//! landmark rule cannot name. It is a weak filter by construction — see
//! [`prune_score`] — and is not what removes the sidebar.
//!
//! Nothing here keys on a per-site selector. Three sites redesign
//! independently, and a selector that breaks does so silently.

/// Below this, an element and everything under it is not the article.
///
/// crawl4ai's default, kept with its scoring.
pub const PRUNE_THRESHOLD: f64 = 0.48;

/// Tags that are never the article, whatever they score.
///
/// This list, not the scoring, is what removes the furniture. Measured on a
/// GuildJen build page 2026-09-06: one `<aside>`, two `<nav>`, one
/// `<footer>`, and the sidebar is the `<aside>`.
///
/// `<article>` is deliberately not treated as a content marker anywhere: the
/// same page uses it 32 times, for the *cards inside that sidebar*. A tag
/// means whatever the theme wants it to mean, so only the negative signals
/// are trusted.
pub const NEVER_CONTENT: &[&str] = &[
    "script", "style", "noscript", "svg", "iframe", "form", "button", "template", "nav", "aside",
    "footer", "header",
];

/// Class and id fragments naming an element as furniture. crawl4ai's list.
const FURNITURE: &[&str] = &[
    "sidebar", "footer", "ads", "comment", "promo", "advert", "social", "share",
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

/// `-0.5` when an attribute names the element as furniture, else `0.0`.
///
/// crawl4ai wraps this in `max(0, class_score)` over a value that can only be
/// `0`, `-0.5` or `-1.0`, so the term is always zero and the signal never
/// fires. Here it is allowed through.
///
/// `nav` and `header` are in [`NEVER_CONTENT`] but deliberately not in
/// [`FURNITURE`]: as substrings they hit `navigation` on a content wrapper
/// and `header` on the article's own title block. The list is applied to
/// every element including page wrappers, and GuildJen's `<body>` carries
/// `right-sidebar` as a *layout* class — matched on an ancestor, one word
/// would delete the document. Only the score is at stake here, never a
/// hard drop.
fn class_id_penalty(class_and_id: &str) -> f64 {
    let lower = class_and_id.to_ascii_lowercase();
    if FURNITURE.iter().any(|w| lower.contains(w)) {
        -0.5
    } else {
        0.0
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
    pub direct_link_text_len: usize,
}

/// crawl4ai's composite score — higher is more article-like.
///
/// Its `ln(text_len + 1)` term is unbounded while the other four are ratios
/// in `0..1`, and that asymmetry looks like a defect. It is not: it is what
/// makes a top-down walk possible. A page wrapper is mostly markup with its
/// text spread across descendants, so it scores badly on density and
/// survives only on bulk. Normalising the term to `0..1` scored the
/// outermost `div` of a real GuildJen page at 0.44 against a 0.48 threshold
/// and deleted the whole document — 60,441 bytes in, 99 out.
///
/// The cost of keeping it is that this filter is weak: past a hundred or so
/// characters the length term alone clears the threshold, so in practice
/// only near-empty elements are dropped. That is why [`NEVER_CONTENT`] does
/// the real work and this is cleanup.
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
    0.4 * text_density
        + 0.2 * prose_density
        + 0.2 * tag_weight(m.tag)
        + 0.1 * class_id_penalty(m.class_and_id)
        + 0.1 * ((m.text_len + 1) as f64).ln()
}

/// Whether this element and everything under it should be dropped.
pub fn should_prune(m: &NodeMetrics) -> bool {
    prune_score(m) < PRUNE_THRESHOLD
}

/// The page with everything that is not the article removed.
///
/// Returns HTML rather than text: what survives still has to yield a gear
/// table and the `alt` attributes of rune and sigil icons, so the tags have
/// to come through. Attributes are re-emitted as they arrived — this output
/// is read by our own extractors, never by a browser.
pub fn prune_to_article(document: &str) -> String {
    let parsed = ::html::Html::parse_document(document);
    let Some(body) = parsed
        .select(&::html::Selector::parse("body").expect("body is a valid selector"))
        .next()
    else {
        return String::new();
    };
    let mut out = String::with_capacity(document.len() / 4);
    emit_children(body, &mut out, 0);
    out
}

/// The article's readable text, one line per text node.
///
/// [`prune_to_article`] keeps the tags because our id extractors need them.
/// This is for the other reader: the prose a site writes around the build —
/// the rotation (`Sword 2 > Dagger 5 > Sword 111`) and the description of
/// what the role is supposed to do. Markup is ~half the bytes of a build
/// page and none of that half is readable, so text is what gets stored.
///
/// Node boundaries become line boundaries rather than spaces: a published
/// rotation sits inside one element, so this keeps it on one line instead of
/// welding it to the heading above it.
pub fn article_text(document: &str) -> String {
    let pruned = prune_to_article(document);
    let parsed = ::html::Html::parse_fragment(&pruned);
    let mut out = String::with_capacity(pruned.len() / 4);
    for node in parsed.root_element().text() {
        let line = node.trim();
        if line.is_empty() {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Deepest nesting this walk will follow.
///
/// The walk is mutual recursion over attacker-shaped input, on a worker
/// thread with the 2 MiB default stack, inside the game process. html5ever
/// imposes no nesting limit of its own: 110 KB of `<div>` produces a tree
/// 10,005 levels deep, and a release build of this walk overflows its stack
/// at around 3,500 levels — 39 KB of input, far under the 2 MiB body cap
/// that was supposed to bound this.
///
/// A stack overflow is not a panic. Windows aborts the process, so the
/// `catch_unwind` around the optimize worker cannot save it and the player's
/// game closes. Browsers cap nesting near this figure for the same reason;
/// no real build page comes close.
const MAX_DEPTH: u32 = 512;

/// Append the surviving children of `parent`.
fn emit_children(parent: ::html::ElementRef<'_>, out: &mut String, depth: u32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in parent.children() {
        match child.value() {
            ::html::Node::Text(text) => out.push_str(&text.text),
            ::html::Node::Element(_) => {
                if let Some(element) = ::html::ElementRef::wrap(child) {
                    emit_element(element, out, depth + 1);
                }
            }
            _ => {}
        }
    }
}

/// Length of the `alt` and `title` text on descendant images.
///
/// An icon with a name is text to a reader, and on these pages it is the
/// only text there is: the rune, the four sigils and the relic are images
/// whose names live in `alt`. Without this a cell holding one icon has
/// `text_len` 0, scores 0.1, and is pruned — deleting exactly the fields
/// this whole exercise exists to recover.
fn alt_text_len(element: ::html::ElementRef<'_>) -> usize {
    let own = element.value();
    let self_alt = if own.name() == "img" {
        own.attr("alt")
            .or_else(|| own.attr("title"))
            .map_or(0, |t| t.trim().len())
    } else {
        0
    };
    // `select` walks descendants only, so an `<img>` scored on its own has
    // to add its own attributes or it reads as empty and is dropped.
    self_alt
        + element
            .select(&::html::Selector::parse("img").expect("img is a valid selector"))
            .filter_map(|img| {
                let v = img.value();
                v.attr("alt").or_else(|| v.attr("title"))
            })
            .map(|text| text.trim().len())
            .sum::<usize>()
}

/// Elements with no closing tag. Emitting `</img>` would be malformed, and
/// recursing into a void element's children is meaningless.
const VOID_ELEMENTS: &[&str] = &[
    "img", "br", "hr", "input", "meta", "link", "source", "track", "area", "base", "col", "embed",
    "param", "wbr",
];

/// Append one element and its surviving subtree, or nothing if it is
/// furniture by tag or scores below the threshold.
fn emit_element(element: ::html::ElementRef<'_>, out: &mut String, depth: u32) {
    if depth >= MAX_DEPTH {
        return;
    }
    let el = element.value();
    let name = el.name();
    if NEVER_CONTENT.contains(&name) {
        return;
    }
    let class_and_id = format!(
        "{} {}",
        el.attr("class").unwrap_or_default(),
        el.attr("id").unwrap_or_default()
    );
    let direct_link_text_len: usize = element
        .children()
        .filter_map(::html::ElementRef::wrap)
        .filter(|c| c.value().name() == "a")
        .map(|a| a.text().collect::<String>().trim().len())
        .sum();
    let metrics = NodeMetrics {
        tag: name,
        class_and_id: &class_and_id,
        text_len: element.text().collect::<String>().trim().len() + alt_text_len(element),
        inner_html_len: element.inner_html().len(),
        direct_link_text_len,
    };
    if should_prune(&metrics) {
        return;
    }
    out.push('<');
    out.push_str(name);
    for (attr, value) in el.attrs() {
        out.push(' ');
        out.push_str(attr);
        out.push_str("=\"");
        // Escaped, because our own extractors read this back: an `alt`
        // holding a quote would otherwise close the attribute early and the
        // rune name after it would be read as markup.
        for ch in value.chars() {
            match ch {
                '"' => out.push_str("&quot;"),
                '&' => out.push_str("&amp;"),
                _ => out.push(ch),
            }
        }
        out.push('"');
    }
    out.push('>');
    if VOID_ELEMENTS.contains(&name) {
        return;
    }
    emit_children(element, out, depth);
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The landmark drop is the fix, so this is the test that matters.
    #[test]
    fn landmarks_are_dropped_and_the_article_survives() {
        let page = "<html><body>\
             <nav><a href=\"/x\">Menu</a></nav>\
             <div class=\"content\"><p>Minstrel gear on every slot for this build.</p></div>\
             <aside class=\"main-sidebar\"><h4><a>Power Zeal Dragonhunter Cloud Build</a></h4></aside>\
             <footer><a>Legal</a></footer>\
             <script>var popular = 'Firebrand';</script>\
             </body></html>";
        let article = prune_to_article(page);
        assert!(article.contains("Minstrel gear"), "kept: {article}");
        for gone in ["Dragonhunter", "Firebrand", "Menu", "Legal"] {
            assert!(!article.contains(gone), "{gone} survived: {article}");
        }
    }

    /// Attributes have to come through: the runes, sigils and relic are icon
    /// names living in `alt`, and the gear is a table.
    #[test]
    fn tags_and_attributes_survive_for_later_extraction() {
        let page = "<html><body><div class=\"entry\">\
             <table><tr><td>Helm</td><td>Minstrel</td>\
             <td><img alt=\"Superior Rune of the Water\" src=\"r.png\"></td></tr></table>\
             </div></body></html>";
        let article = prune_to_article(page);
        assert!(
            article.contains("<table"),
            "table structure kept: {article}"
        );
        assert!(
            article.contains("Superior Rune of the Water"),
            "image alt kept: {article}"
        );
    }

    /// The distinction the scoring rests on: a menu is links with nothing
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
        assert!(prose > menu, "prose {prose:.3} beats all-link {menu:.3}");
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
        for furniture in ["main-sidebar", "post-footer", "social-share"] {
            let scored = prune_score(&NodeMetrics {
                class_and_id: furniture,
                ..plain
            });
            assert!(scored < prune_score(&plain), "{furniture} should score low");
        }
        assert_eq!(class_id_penalty("entry-content"), 0.0);
        // Matched anywhere, not only at the start - the original misses this.
        assert_eq!(class_id_penalty("widget sidebar"), -0.5);
        // And `nav`/`header` are absent on purpose: as substrings they would
        // hit "navigation" and the article's own header block.
        assert_eq!(class_id_penalty("main-navigation"), 0.0);
        assert_eq!(class_id_penalty("entry-header"), 0.0);
    }

    /// Deeply nested input must return, not die.
    ///
    /// The walk is mutual recursion over remote HTML on a worker thread with
    /// the default 2 MiB stack, inside the game process. Measured before the
    /// cap: a release build overflowed at roughly 3,500 levels, which is
    /// 39 KB of input — nowhere near the 2 MiB body cap meant to bound it.
    /// A stack overflow aborts on Windows rather than unwinding, so the
    /// `catch_unwind` around the worker cannot catch it and the player's game
    /// closes.
    #[test]
    fn deep_nesting_returns_instead_of_overflowing() {
        let deep = format!(
            "<html><body>{}<p>the build</p>{}</body></html>",
            "<div>".repeat(20_000),
            "</div>".repeat(20_000)
        );
        // Returning at all is the assertion; the cap truncates past 512.
        let article = prune_to_article(&deep);
        assert!(article.len() < deep.len());
    }

    /// A quote inside an attribute must not close it early: our own
    /// extractors read this output back, and `alt` is where the rune names
    /// live.
    #[test]
    fn attribute_values_are_escaped_on_the_way_out() {
        let page = "<html><body><div class=\"entry\">\
             <img alt='Superior Rune &amp; &quot;Water&quot; of some length here' src=\"r.png\">\
             </div></body></html>";
        let article = prune_to_article(page);
        assert!(!article.contains("\"Water\""), "unescaped quote: {article}");
        assert!(article.contains("&quot;Water&quot;"), "escaped: {article}");
        assert!(article.contains("&amp;"), "ampersand escaped: {article}");
    }

    /// The one thing the scoring reliably removes.
    #[test]
    fn empty_wrappers_are_dropped() {
        let empty = NodeMetrics {
            tag: "div",
            class_and_id: "",
            text_len: 0,
            inner_html_len: 0,
            direct_link_text_len: 0,
        };
        assert!(should_prune(&empty), "scored {:.3}", prune_score(&empty));
        assert!(prune_to_article("<html><body><div></div></body></html>").is_empty());
    }

    #[test]
    fn tag_weight_prefers_prose_containers() {
        assert!(tag_weight("article") > tag_weight("div"));
        assert!(tag_weight("p") > tag_weight("span"));
        assert_eq!(tag_weight("main"), 0.5, "unlisted tags take the default");
    }

    /// The whole point, against a real page.
    ///
    /// Ignored: it needs a capture, which is 60 KB and goes stale.
    ///   GUILDJEN_BUILD_HTML=troubadour.html \
    ///     cargo test -p gw2-optimizer article_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn article_live_page_keeps_the_build_and_drops_the_furniture() {
        let path = std::env::var("GUILDJEN_BUILD_HTML").expect("set GUILDJEN_BUILD_HTML");
        let page = std::fs::read_to_string(&path).expect("readable capture");
        let article = prune_to_article(&page);
        println!(
            "-- {} bytes in, {} out ({:.0}% kept) --",
            page.len(),
            article.len(),
            100.0 * article.len() as f64 / page.len() as f64
        );
        for needle in ["Troubadour", "Minstrel", "[&"] {
            assert!(article.contains(needle), "{needle:?} was pruned away");
        }
        // Not asserted, and deliberately so: the rune, sigil and relic names
        // are not in this page's markup at any encoding. GuildJen renders
        // them in the browser with its `gw2-embeddings-patched` plugin, so a
        // fetch of the real page contains zero occurrences of "Superior
        // Rune", "Superior Sigil" or "Relic of" (measured 2026-09-06 against
        // 808 KB of server HTML). No parser, reader mode or scoring can
        // recover text a server never sent. The 71 `[&...]` chat links on the
        // page belong to an event-timer widget, not the gear table.
        for needle in ["Dragonhunter", "Willbender", "Firebrand"] {
            assert!(
                !article.contains(needle),
                "{needle:?} survived - another build, from the furniture"
            );
        }
    }
}
