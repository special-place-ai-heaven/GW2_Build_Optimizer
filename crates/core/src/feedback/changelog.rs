//! Changelog splitter: turns the repo's `CHANGELOG.md` into per-version entries for the
//! About tab's "What's new" view. Pure string processing, no Markdown crate.

/// The changelog compiled into the DLL (`CHANGELOG.md` at the repo root).
pub const EMBEDDED: &str = include_str!("../../../../CHANGELOG.md");

/// Longest body kept per entry, counted in `chars()`; longer bodies end in `…`.
pub const MAX_BODY_CHARS: usize = 1200;

/// One `## <version> - <date>` section of the changelog with its cleaned body.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangelogEntry {
    pub version: String,
    /// Empty when the heading carries no ` - <date>` part.
    pub date: String,
    /// Plain text: `### ` marks dropped, `**` and backticks removed, `- `/`* ` bullets turned
    /// into `• `, blank runs collapsed, capped at [`MAX_BODY_CHARS`].
    pub body: String,
}

/// Splits Markdown changelog text into entries, in file order (newest first as written).
///
/// Text before the first `## ` line is ignored. `\r\n` and `\n` line endings parse alike.
pub fn parse(text: &str) -> Vec<ChangelogEntry> {
    let mut entries: Vec<ChangelogEntry> = Vec::new();
    let mut body_lines: Vec<String> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some(last) = entries.last_mut() {
                last.body = finish_body(&body_lines);
                body_lines.clear();
            }
            let (version, date) = match heading.split_once(" - ") {
                Some((v, d)) => (v.trim(), d.trim()),
                None => (heading.trim(), ""),
            };
            entries.push(ChangelogEntry {
                version: version.to_string(),
                date: date.to_string(),
                body: String::new(),
            });
        } else if !entries.is_empty() {
            body_lines.push(clean_line(line));
        }
    }
    if let Some(last) = entries.last_mut() {
        last.body = finish_body(&body_lines);
    }
    entries
}

/// One body line: `### ` mark dropped, `- `/`* ` bullet turned into `• `, `**` and backticks removed.
fn clean_line(line: &str) -> String {
    let line = line.strip_prefix("### ").unwrap_or(line);
    let line = match line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        Some(rest) => format!("• {rest}"),
        None => line.to_string(),
    };
    line.replace("**", "").replace('`', "")
}

/// Joins cleaned lines: leading/trailing blank lines dropped, blank runs collapsed to one,
/// then capped at [`MAX_BODY_CHARS`] with a trailing `…`.
fn finish_body(lines: &[String]) -> String {
    let mut body = String::new();
    let mut pending_blank = false;
    for line in lines {
        if line.trim().is_empty() {
            pending_blank = !body.is_empty();
            continue;
        }
        if !body.is_empty() {
            body.push('\n');
            if pending_blank {
                body.push('\n');
            }
        }
        pending_blank = false;
        body.push_str(line);
    }

    if body.chars().count() > MAX_BODY_CHARS {
        let mut capped: String = body.chars().take(MAX_BODY_CHARS).collect();
        capped.truncate(capped.trim_end().len());
        capped.push('…');
        return capped;
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
# Changelog

Intro line that must be ignored.

## 9.9.9 - 2030-01-01

### Highlights

- First **bold** thing with `code`.
- Second thing.

## 9.9.8 - 2029-12-31

Plain paragraph.

## 9.9.7 - 2029-12-30

* Star bullet.

## 9.9.6 - 2029-12-29

Body six.

## 9.9.5 - 2029-12-28

Body five.

## 9.9.4 - 2029-12-27

Body four.

## 9.9.3 - 2029-12-26

Body three.
";

    #[test]
    fn splits_on_h2_and_reads_version_and_date() {
        let entries = parse(FIXTURE);
        assert_eq!(entries.len(), 7);
        assert_eq!(
            entries[0],
            ChangelogEntry {
                version: "9.9.9".into(),
                date: "2030-01-01".into(),
                body: "Highlights\n\n• First bold thing with code.\n• Second thing.".into(),
            }
        );
        assert_eq!(entries[1].version, "9.9.8");
        assert_eq!(entries[1].body, "Plain paragraph.");
        assert_eq!(entries[2].body, "• Star bullet.");
        assert_eq!(entries[6].version, "9.9.3");
        assert_eq!(entries[6].date, "2029-12-26");
        assert_eq!(entries[6].body, "Body three.");
    }

    #[test]
    fn crlf_and_lf_both_parse() {
        let crlf = FIXTURE.replace('\n', "\r\n");
        assert!(crlf.contains("\r\n"));
        assert_eq!(parse(&crlf), parse(FIXTURE));
    }

    #[test]
    fn strips_h3_bold_bullets_and_backticks() {
        let text = "## 1.0.0 - 2020-01-01\n### Title\n**Bold** text\n- item `a`\n* item **b**\nkeep `this` line\n";
        let entries = parse(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].body,
            "Title\nBold text\n• item a\n• item b\nkeep this line"
        );
    }

    #[test]
    fn collapses_blank_runs_and_trims() {
        let text = "## 1.0.0 - 2020-01-01\n\n\n\nfirst\n\n\n\n\nsecond\n   \nthird\n\n\n";
        let entries = parse(text);
        assert_eq!(entries[0].body, "first\n\nsecond\n\nthird");
    }

    #[test]
    fn caps_body_at_1200_chars_with_ellipsis() {
        let body = "é".repeat(1500);
        let text = format!("## 1.0.0 - 2020-01-01\n{body}\n");
        let entries = parse(&text);
        assert_eq!(entries[0].body.chars().count(), MAX_BODY_CHARS + 1);
        assert!(entries[0].body.ends_with('…'));
        assert!(entries[0].body.starts_with("éé"));

        let exact = "é".repeat(MAX_BODY_CHARS);
        let text = format!("## 1.0.0 - 2020-01-01\n{exact}\n");
        assert_eq!(parse(&text)[0].body, exact);
    }

    #[test]
    fn heading_without_date() {
        let entries = parse("## Unreleased\nSomething.\n## 1.0.0 - 2020-01-01\nOld.\n");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].version, "Unreleased");
        assert_eq!(entries[0].date, "");
        assert_eq!(entries[0].body, "Something.");
        assert_eq!(entries[1].version, "1.0.0");
    }

    #[test]
    fn embedded_changelog_has_at_least_five_entries() {
        let entries = parse(EMBEDDED);
        assert!(entries.len() >= 5, "got {} entries", entries.len());

        let first_heading = EMBEDDED
            .lines()
            .find_map(|l| l.strip_prefix("## "))
            .expect("CHANGELOG.md has a `## ` heading");
        let (version, date) = first_heading.split_once(" - ").expect("heading has ` - `");
        assert_eq!(entries[0].version, version.trim());
        assert_eq!(entries[0].date, date.trim());

        let parts: Vec<&str> = entries[0].version.split('.').collect();
        assert_eq!(
            parts.len(),
            3,
            "version {} is not semver",
            entries[0].version
        );
        assert!(
            parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
            "version {} is not semver",
            entries[0].version
        );

        for e in &entries {
            assert!(!e.body.is_empty(), "empty body for {}", e.version);
            assert!(e.body.chars().count() <= MAX_BODY_CHARS + 1);
            assert!(!e.body.contains("**"));
            assert!(!e.body.contains("### "));
        }
    }
}
