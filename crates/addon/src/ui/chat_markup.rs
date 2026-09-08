//! Reply markup: the subset of what a model writes that a chat bubble can
//! draw. Parser and word-wrapper are pure so they can be tested without
//! ImGui; the draw pass lives in `chat_bar`.
//!
//! Nothing is ever dropped: markup the parser does not recognise stays as
//! plain text, and there is no length cap anywhere in this module.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum SpanStyle {
    Plain,
    Bold,
    Italic,
    /// A name the game data knows: accent colour, normal weight.
    Name,
    Warn,
    Link(String),
    /// The `→` between rotation parts, accent colour.
    Arrow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Span>),
    Bullet { depth: u8, spans: Vec<Span> },
    Numbered { n: u32, spans: Vec<Span> },
    Rotation(Vec<Vec<Span>>),
    Warning(Vec<Span>),
    Blank,
}

/// One span placed on a wrapped line, `x` relative to the text origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub span: Span,
    pub x: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    pub spans: Vec<Placed>,
    pub width: f32,
}

/// Names the game data knows, lowercased, for the accent-colour pass.
pub fn name_index(db: &gw2_optimizer::gamedb::GameDb) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut add = |n: &str| {
        // Short names ("Fire", "Might") collide with ordinary prose.
        if n.chars().count() >= 4 {
            names.insert(n.to_lowercase());
        }
    };
    for s in db.skills.values() {
        add(&s.name);
    }
    for t in db.traits.values() {
        add(&t.name);
    }
    for s in db.specializations.values() {
        add(&s.name);
    }
    for id in db.runes.iter().chain(&db.sigils).chain(&db.relics) {
        if let Some(item) = db.items.get(id) {
            add(&item.name);
        }
    }
    for p in db.professions.values() {
        for w in p.weapons.keys() {
            add(w);
        }
    }
    names
}

/// Parse a reply into blocks. `names` answers "does the game know this
/// name?" (case-insensitive) for the accent pass.
pub fn parse(text: &str, names: &dyn Fn(&str) -> bool) -> Vec<Block> {
    text.lines().map(|line| parse_line(line, names)).collect()
}

fn parse_line(line: &str, names: &dyn Fn(&str) -> bool) -> Block {
    if line.trim().is_empty() {
        return Block::Blank;
    }
    let indent = line.len() - line.trim_start().len();
    let body = line.trim_start();
    if let Some(rest) = body
        .strip_prefix("- ")
        .or_else(|| body.strip_prefix("* "))
        .or_else(|| body.strip_prefix("\u{2022} "))
    {
        return Block::Bullet {
            depth: (indent / 2).min(4) as u8,
            spans: inline(rest.trim_start(), names),
        };
    }
    if let Some((digits, rest)) = body.split_once(". ") {
        if !digits.is_empty() && digits.len() <= 3 && digits.bytes().all(|b| b.is_ascii_digit()) {
            return Block::Numbered {
                n: digits.parse().unwrap_or(0),
                spans: inline(rest.trim_start(), names),
            };
        }
    }
    if let Some(rest) = body
        .strip_prefix('!')
        .or_else(|| body.strip_prefix('\u{26A0}'))
    {
        if !rest.starts_with('!') {
            return Block::Warning(inline(rest.trim_start(), names));
        }
    }
    let parts: Vec<&str> = body.split("->").flat_map(|p| p.split('\u{2192}')).collect();
    if parts.len() >= 2 && parts.iter().all(|p| !p.trim().is_empty()) {
        return Block::Rotation(parts.iter().map(|p| inline(p.trim(), names)).collect());
    }
    Block::Paragraph(inline(body, names))
}

/// Inline markup: `**bold**`, `_italic_` / `*italic*`, bare URLs, ` -> `
/// arrows, then known names in the plain remainder.
fn inline(text: &str, names: &dyn Fn(&str) -> bool) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut plain = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let flush = |plain: &mut String, out: &mut Vec<Span>| {
        if !plain.is_empty() {
            out.extend(accent_names(&std::mem::take(plain), names));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        let prev_boundary = i == 0 || !chars[i - 1].is_alphanumeric();
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) = find_close(&chars, i + 2, "**") {
                if end > i + 2 {
                    flush(&mut plain, &mut out);
                    out.push(Span {
                        text: chars[i + 2..end].iter().collect(),
                        style: SpanStyle::Bold,
                    });
                    i = end + 2;
                    continue;
                }
            }
        } else if (c == '_' || c == '*') && prev_boundary {
            let marker = c.to_string();
            if let Some(end) = find_close(&chars, i + 1, &marker) {
                let after_ok = chars.get(end + 1).is_none_or(|n| !n.is_alphanumeric());
                if end > i + 1 && after_ok && chars[i + 1] != ' ' {
                    flush(&mut plain, &mut out);
                    out.push(Span {
                        text: chars[i + 1..end].iter().collect(),
                        style: SpanStyle::Italic,
                    });
                    i = end + 1;
                    continue;
                }
            }
        } else if c == 'h' && prev_boundary {
            let rest: String = chars[i..].iter().take(8).collect();
            if rest.starts_with("http://") || rest.starts_with("https://") {
                let mut end = i;
                while end < chars.len() && !chars[end].is_whitespace() {
                    end += 1;
                }
                while end > i && matches!(chars[end - 1], '.' | ',' | ';' | ':' | ')' | '!' | '?') {
                    end -= 1;
                }
                let url: String = chars[i..end].iter().collect();
                flush(&mut plain, &mut out);
                out.push(Span {
                    text: url.clone(),
                    style: SpanStyle::Link(url),
                });
                i = end;
                continue;
            }
        } else if c == '-' && chars.get(i + 1) == Some(&'>') || c == '\u{2192}' {
            while plain.ends_with(' ') {
                plain.pop();
            }
            flush(&mut plain, &mut out);
            out.push(Span {
                text: " \u{2192} ".into(),
                style: SpanStyle::Arrow,
            });
            i += if c == '-' { 2 } else { 1 };
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            continue;
        }
        plain.push(c);
        i += 1;
    }
    flush(&mut plain, &mut out);
    out
}

fn find_close(chars: &[char], from: usize, marker: &str) -> Option<usize> {
    let m: Vec<char> = marker.chars().collect();
    (from..chars.len().saturating_sub(m.len() - 1)).find(|&j| chars[j..j + m.len()] == m[..])
}

/// Split plain text into Plain and Name spans by longest match on word
/// boundaries; names are at most six words long.
fn accent_names(text: &str, names: &dyn Fn(&str) -> bool) -> Vec<Span> {
    const MAX_WORDS: usize = 6;
    // Word starts and ends as byte offsets.
    let words: Vec<(usize, usize)> = {
        let mut v = Vec::new();
        let mut start = None;
        for (i, c) in text.char_indices() {
            let is_word = c.is_alphanumeric() || c == '\'';
            match (is_word, start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    v.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            v.push((s, text.len()));
        }
        v
    };
    let mut out = Vec::new();
    let mut cursor = 0;
    let mut w = 0;
    while w < words.len() {
        let mut hit = None;
        for k in (1..=MAX_WORDS.min(words.len() - w)).rev() {
            let (s, e) = (words[w].0, words[w + k - 1].1);
            if names(&text[s..e]) {
                hit = Some((s, e, k));
                break;
            }
        }
        match hit {
            Some((s, e, k)) => {
                if s > cursor {
                    out.push(Span {
                        text: text[cursor..s].into(),
                        style: SpanStyle::Plain,
                    });
                }
                out.push(Span {
                    text: text[s..e].into(),
                    style: SpanStyle::Name,
                });
                cursor = e;
                w += k;
            }
            None => w += 1,
        }
    }
    if cursor < text.len() {
        out.push(Span {
            text: text[cursor..].into(),
            style: SpanStyle::Plain,
        });
    }
    out
}

/// Indent per bullet depth, in pixels at UI scale 1.
pub const INDENT: f32 = 14.0;

/// Wrap blocks to `max_w` using `measure` (text width in pixels). Words split
/// mid-word only when a single word is wider than `max_w`.
pub fn wrap_spans(measure: &dyn Fn(&str) -> f32, blocks: &[Block], max_w: f32) -> Vec<Line> {
    let mut lines = Vec::new();
    for block in blocks {
        let (indent, prefix, spans): (f32, Option<Span>, Vec<Span>) = match block {
            Block::Blank => {
                lines.push(Line::default());
                continue;
            }
            Block::Paragraph(s) => (0.0, None, s.clone()),
            Block::Warning(s) => (
                0.0,
                None,
                s.iter()
                    .map(|sp| Span {
                        text: sp.text.clone(),
                        style: match sp.style {
                            SpanStyle::Link(_) => sp.style.clone(),
                            _ => SpanStyle::Warn,
                        },
                    })
                    .collect(),
            ),
            Block::Bullet { depth, spans } => (
                INDENT * (*depth as f32 + 1.0),
                Some(Span {
                    text: "\u{2022} ".into(),
                    style: SpanStyle::Plain,
                }),
                spans.clone(),
            ),
            Block::Numbered { n, spans } => (
                INDENT * 1.6,
                Some(Span {
                    text: format!("{n}. "),
                    style: SpanStyle::Plain,
                }),
                spans.clone(),
            ),
            Block::Rotation(parts) => {
                let mut joined = Vec::new();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        joined.push(Span {
                            text: " \u{2192} ".into(),
                            style: SpanStyle::Arrow,
                        });
                    }
                    joined.extend(part.iter().cloned());
                }
                (INDENT, None, joined)
            }
        };
        wrap_block(measure, indent, prefix, &spans, max_w, &mut lines);
    }
    if lines.is_empty() {
        lines.push(Line::default());
    }
    lines
}

fn wrap_block(
    measure: &dyn Fn(&str) -> f32,
    indent: f32,
    prefix: Option<Span>,
    spans: &[Span],
    max_w: f32,
    lines: &mut Vec<Line>,
) {
    let mut line = Line::default();
    let mut x = 0.0;
    let first_x = if let Some(p) = prefix {
        let w = measure(&p.text);
        let px = (indent - w).max(0.0);
        line.spans.push(Placed { span: p, x: px });
        indent
    } else {
        indent
    };
    x += first_x;
    let push = |line: &mut Line, text: &str, style: &SpanStyle, x: f32, w: f32| {
        if let Some(last) = line.spans.last_mut() {
            if last.span.style == *style && !matches!(style, SpanStyle::Link(_)) {
                last.span.text.push_str(text);
                line.width = x + w;
                return;
            }
        }
        line.spans.push(Placed {
            span: Span {
                text: text.into(),
                style: style.clone(),
            },
            x,
        });
        line.width = x + w;
    };
    for span in spans {
        for token in tokens(&span.text) {
            let mut w = measure(token);
            let at_line_start = x <= indent + 0.01;
            if x + w > max_w && !at_line_start {
                lines.push(std::mem::take(&mut line));
                x = indent;
                if token.trim().is_empty() {
                    continue;
                }
            }
            if w > max_w - indent && !token.trim().is_empty() {
                // A single word wider than the line: split by characters.
                let mut piece = String::new();
                for c in token.chars() {
                    let trial = format!("{piece}{c}");
                    if !piece.is_empty() && x + measure(&trial) > max_w {
                        let pw = measure(&piece);
                        push(&mut line, &piece, &span.style, x, pw);
                        lines.push(std::mem::take(&mut line));
                        x = indent;
                        piece.clear();
                    }
                    piece.push(c);
                }
                w = measure(&piece);
                push(&mut line, &piece, &span.style, x, w);
                x += w;
                continue;
            }
            push(&mut line, token, &span.style, x, w);
            x += w;
        }
    }
    if !line.spans.is_empty() {
        lines.push(line);
    }
}

/// Words with their trailing whitespace attached, so a wrap never loses a
/// space and never breaks inside a word.
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            in_space = true;
        } else if in_space {
            out.push(&text[start..i]);
            start = i;
            in_space = false;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_names(_: &str) -> bool {
        false
    }

    fn known(s: &str) -> bool {
        matches!(
            s.to_lowercase().as_str(),
            "gravedigger" | "superior rune of the scholar"
        )
    }

    fn flat(blocks: &[Block]) -> String {
        let mut out = String::new();
        for b in blocks {
            match b {
                Block::Paragraph(s) | Block::Warning(s) => {
                    for sp in s {
                        out.push_str(&sp.text);
                    }
                }
                Block::Bullet { spans, .. } | Block::Numbered { spans, .. } => {
                    for sp in spans {
                        out.push_str(&sp.text);
                    }
                }
                Block::Rotation(parts) => {
                    for p in parts {
                        for sp in p {
                            out.push_str(&sp.text);
                        }
                    }
                }
                Block::Blank => {}
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn three_thousand_characters_keep_every_character() {
        let text: String = (0..500).map(|i| format!("word{i} ")).collect();
        assert!(text.len() >= 3000);
        let blocks = parse(&text, &no_names);
        assert_eq!(flat(&blocks).trim_end(), text.trim_end());
        let lines = wrap_spans(&|s| s.len() as f32 * 7.0, &blocks, 300.0);
        let joined: String = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|p| p.span.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            joined.split_whitespace().collect::<Vec<_>>(),
            text.split_whitespace().collect::<Vec<_>>()
        );
        let bad: Vec<f32> = lines
            .iter()
            .map(|l| l.width)
            .filter(|w| *w > 301.0)
            .collect();
        assert!(bad.is_empty(), "{bad:?} first line {:?}", lines[0]);
    }

    #[test]
    fn nested_bullets_carry_depth() {
        let blocks = parse("- top\n  - nested\n    - deeper\n* star", &no_names);
        assert!(matches!(blocks[0], Block::Bullet { depth: 0, .. }));
        assert!(matches!(blocks[1], Block::Bullet { depth: 1, .. }));
        assert!(matches!(blocks[2], Block::Bullet { depth: 2, .. }));
        assert!(matches!(blocks[3], Block::Bullet { depth: 0, .. }));
    }

    #[test]
    fn numbered_items() {
        let blocks = parse("1. first\n12. twelfth", &no_names);
        assert!(matches!(blocks[0], Block::Numbered { n: 1, .. }));
        assert!(matches!(blocks[1], Block::Numbered { n: 12, .. }));
    }

    #[test]
    fn rotation_of_six_parts() {
        let blocks = parse("A -> B -> C → D -> E -> F", &no_names);
        match &blocks[0] {
            Block::Rotation(parts) => {
                assert_eq!(parts.len(), 6);
                assert_eq!(parts[2][0].text, "C");
                assert_eq!(parts[5][0].text, "F");
            }
            other => panic!("not a rotation: {other:?}"),
        }
    }

    #[test]
    fn arrow_in_prose_with_one_part_is_not_a_rotation() {
        let blocks = parse("Swap -> then keep pressure", &no_names);
        // Two parts, so it is a rotation; one part with a dangling arrow is not.
        assert!(matches!(blocks[0], Block::Rotation(_)));
        let blocks = parse("-> nothing before", &no_names);
        assert!(matches!(blocks[0], Block::Paragraph(_)));
        let blocks = parse("nothing after ->", &no_names);
        assert!(matches!(blocks[0], Block::Paragraph(_)));
    }

    #[test]
    fn bold_inside_a_bullet() {
        let blocks = parse("- take **Gravedigger** now", &no_names);
        match &blocks[0] {
            Block::Bullet { spans, .. } => {
                assert_eq!(spans[0].text, "take ");
                assert_eq!(spans[1].text, "Gravedigger");
                assert_eq!(spans[1].style, SpanStyle::Bold);
                assert_eq!(spans[2].text, " now");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn known_name_without_markup_is_accented_and_unknown_stays_plain() {
        let blocks = parse("Open with Gravedigger, then Frostbite.", &known);
        let Block::Paragraph(spans) = &blocks[0] else {
            panic!()
        };
        assert!(spans
            .iter()
            .any(|s| s.text == "Gravedigger" && s.style == SpanStyle::Name));
        assert!(spans
            .iter()
            .any(|s| s.text.contains("Frostbite") && s.style == SpanStyle::Plain));
        // Longest match wins over the inner "Scholar".
        let blocks = parse("Wear Superior Rune of the Scholar.", &known);
        let Block::Paragraph(spans) = &blocks[0] else {
            panic!()
        };
        assert!(spans
            .iter()
            .any(|s| s.text == "Superior Rune of the Scholar" && s.style == SpanStyle::Name));
    }

    #[test]
    fn url_with_trailing_punctuation_excludes_the_punctuation() {
        let blocks = parse("See https://guildjen.com/reaper/. Then go.", &no_names);
        let Block::Paragraph(spans) = &blocks[0] else {
            panic!()
        };
        let link = spans
            .iter()
            .find(|s| matches!(s.style, SpanStyle::Link(_)))
            .unwrap();
        assert_eq!(link.text, "https://guildjen.com/reaper/");
        assert_eq!(
            link.style,
            SpanStyle::Link("https://guildjen.com/reaper/".into())
        );
        assert_eq!(
            flat(&blocks).trim_end(),
            "See https://guildjen.com/reaper/. Then go."
        );
    }

    #[test]
    fn unmatched_bold_marker_renders_literally() {
        let blocks = parse("this ** stays", &no_names);
        assert_eq!(flat(&blocks).trim_end(), "this ** stays");
        let Block::Paragraph(spans) = &blocks[0] else {
            panic!()
        };
        assert!(spans.iter().all(|s| s.style == SpanStyle::Plain));
    }

    #[test]
    fn warning_and_italic() {
        let blocks = parse("! Do not swap early\n_aside_ and *also*", &no_names);
        match &blocks[0] {
            Block::Warning(spans) => assert_eq!(spans[0].text, "Do not swap early"),
            other => panic!("{other:?}"),
        }
        let Block::Paragraph(spans) = &blocks[1] else {
            panic!()
        };
        assert_eq!(spans[0].style, SpanStyle::Italic);
        assert_eq!(spans[0].text, "aside");
        assert_eq!(spans.last().unwrap().style, SpanStyle::Italic);
        assert_eq!(spans.last().unwrap().text, "also");
    }

    #[test]
    fn blank_lines_are_kept_and_a_long_word_splits() {
        let blocks = parse("a\n\nb", &no_names);
        assert_eq!(blocks.len(), 3);
        assert!(matches!(blocks[1], Block::Blank));
        let word = "x".repeat(50);
        let lines = wrap_spans(&|s| s.len() as f32 * 10.0, &parse(&word, &no_names), 100.0);
        assert_eq!(lines.len(), 5);
        let back: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|p| p.span.text.clone()))
            .collect();
        assert_eq!(back, word);
    }
}
