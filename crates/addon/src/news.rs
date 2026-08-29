//! Public RSS/Atom headlines for Setup (official GW2) and the News tab.

use std::time::{Duration, Instant};

use gw2_core::config::{NewsKind, NewsSource};
use reqwest::header::{HeaderMap, HeaderValue};

use crate::state::{with_state, AddonState, CancellationToken};

const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_BYTES: usize = 512 * 1024;
const MAX_ITEMS: usize = 20;
const MAX_BODY_CHARS: usize = 12000;
const TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone)]
pub struct NewsItem {
    pub source: NewsSource,
    pub title: String,
    pub url: String,
    pub published: String,
    pub published_ts: i64,
    /// One-line teaser (RSS `<description>`). Compact cards.
    pub snippet: String,
    /// Full article text (stripped `<content:encoded>`, else the description).
    pub body: String,
    /// First still in the item (thumbnail, enclosure, or `<img>`). HTTPS only.
    pub image_url: Option<String>,
}

#[derive(Debug, Default)]
pub struct NewsState {
    feeds: [Option<Vec<NewsItem>>; 5],
    pub loading: bool,
    pub fetched_at: Option<Instant>,
    official_lang: String,
    /// URL of the story in the reader, if any.
    pub expanded: Option<String>,
    pub search: String,
    /// `None` = all kinds.
    pub filter: Option<NewsKind>,
    pub art_loading: bool,
    /// Still height multiplier in Detail (1–5). Zero means “use 3” (~full pane).
    pub still_zoom: f32,
}

impl NewsState {
    pub fn items(&self, src: NewsSource) -> &[NewsItem] {
        self.feeds[src.index()].as_deref().unwrap_or(&[])
    }

    pub fn set_feed(&mut self, src: NewsSource, lang: &str, items: Vec<NewsItem>) {
        self.feeds[src.index()] = Some(items);
        if src == NewsSource::Official {
            self.official_lang = lang.to_string();
        }
        self.fetched_at = Some(Instant::now());
    }

    pub fn invalidate(&mut self, sources: &[NewsSource]) {
        for src in sources {
            self.feeds[src.index()] = None;
            if *src == NewsSource::Official {
                self.official_lang.clear();
            }
        }
        crate::news_art::clear_failed();
    }

    pub fn needs(&self, src: NewsSource, lang: &str) -> bool {
        if src == NewsSource::Official && self.official_lang != lang {
            return true;
        }
        if self.feeds[src.index()].is_none() {
            return true;
        }
        self.fetched_at.is_none_or(|t| t.elapsed() > TTL)
    }

    pub fn collected(&self, sources: &[NewsSource]) -> Vec<NewsItem> {
        let mut out = Vec::new();
        for src in sources {
            out.extend(self.items(*src).iter().cloned());
        }
        out.sort_by_key(|b| std::cmp::Reverse(b.published_ts));
        out
    }
}

pub fn feed_url(src: NewsSource, ui_lang: &str) -> String {
    match src {
        NewsSource::Official => {
            let lang = match ui_lang {
                "de" | "es" | "fr" => ui_lang,
                _ => "en",
            };
            format!("https://www.guildwars2.com/{lang}/feed/")
        }
        NewsSource::ForumNews => {
            "https://en-forum.guildwars2.com/forum/32-news-and-announcements.xml".into()
        }
        NewsSource::PatchNotes => {
            "https://en-forum.guildwars2.com/forum/6-game-update-notes.xml".into()
        }
        NewsSource::Youtube => {
            "https://www.youtube.com/feeds/videos.xml?channel_id=UCP_FgMqOxp_VsM0UfrL-DxA".into()
        }
        NewsSource::GuildJen => "https://www.guildjen.com/feed/".into(),
    }
}

/// Start a worker for any source that is missing or stale. Never blocks the caller.
pub fn kick(state: &mut AddonState, sources: &[NewsSource]) {
    if state.news.loading || sources.is_empty() {
        return;
    }
    let lang = gw2_core::i18n::current();
    let needed: Vec<NewsSource> = sources
        .iter()
        .copied()
        .filter(|s| state.news.needs(*s, &lang))
        .collect();
    if needed.is_empty() {
        return;
    }
    state.news.loading = true;
    let version = crate::VERSION.to_string();
    let spawned = state.spawn_worker("fetch-news", move |token| {
        let mut results = Vec::new();
        for src in needed {
            if token.is_cancelled() {
                break;
            }
            let url = feed_url(src, &lang);
            let items = match fetch_body(&url, &token, &version) {
                Some(body) => parse_feed(src, &body),
                None => Vec::new(),
            };
            results.push((src, items));
        }
        let _ = with_state(|s| {
            s.news.loading = false;
            for (src, items) in results {
                s.news.set_feed(src, &lang, items);
            }
        });
    });
    if !spawned {
        state.news.loading = false;
    }
}

fn headers(version: &str) -> HeaderMap {
    let mut map = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&format!("GW2BuildOptimizer/{version} news")) {
        map.insert(reqwest::header::USER_AGENT, v);
    }
    map
}

fn fetch_body(url: &str, token: &CancellationToken, version: &str) -> Option<String> {
    if token.is_cancelled() {
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .ok()?;
    let resp = client.get(url).headers(headers(version)).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().ok()?;
    if bytes.len() > MAX_BYTES {
        return None;
    }
    String::from_utf8(bytes.to_vec()).ok()
}

pub fn parse_feed(source: NewsSource, xml: &str) -> Vec<NewsItem> {
    let mut items = Vec::new();
    for block in iter_blocks(xml, "item").chain(iter_blocks(xml, "entry")) {
        if items.len() >= MAX_ITEMS {
            break;
        }
        let title = strip_html(&tag_text(block, "title").unwrap_or_default());
        if title.is_empty() {
            continue;
        }
        let url = canonical_url(&item_link(block));
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        let raw_date = tag_text(block, "pubDate")
            .or_else(|| tag_text(block, "published"))
            .or_else(|| tag_text(block, "updated"))
            .unwrap_or_default();
        let published_ts = parse_date(&raw_date).unwrap_or(0);
        let published = format_date(published_ts, &raw_date);
        let (snippet, body) = item_texts(block);
        items.push(NewsItem {
            source,
            title,
            url,
            published,
            published_ts,
            snippet,
            body,
            image_url: item_image(block),
        });
    }
    items
}

pub fn matches(item: &NewsItem, kind: Option<NewsKind>, q: &str) -> bool {
    if let Some(k) = kind {
        if item.source.kind() != k {
            return false;
        }
    }
    let q = q.trim();
    if q.is_empty() {
        return true;
    }
    let q = q.to_lowercase();
    item.title.to_lowercase().contains(&q)
        || item.snippet.to_lowercase().contains(&q)
        || item.body.to_lowercase().contains(&q)
}

/// Start still-image downloads for URLs that are not cached yet. Never blocks.
pub fn kick_art(state: &mut AddonState, urls: &[String]) {
    if state.news.art_loading {
        return;
    }
    let dir = state.addon_dir.join("cache").join("news");
    let batch = crate::news_art::take_batch(urls, 6);
    if batch.is_empty() {
        return;
    }
    state.news.art_loading = true;
    let version = crate::VERSION.to_string();
    let queued = batch.clone();
    let spawned = state.spawn_worker("fetch-news-art", move |token| {
        struct Done;
        impl Drop for Done {
            fn drop(&mut self) {
                let _ = with_state(|s| s.news.art_loading = false);
            }
        }
        let _done = Done;
        let mut leftover = batch.clone();
        for url in &batch {
            if token.is_cancelled() {
                crate::news_art::release_pending(&leftover);
                return;
            }
            leftover.retain(|u| u != url);
            match crate::news_art::download(url, &dir, &token, &version) {
                Some((path, aspect)) => crate::news_art::mark_ready(url, path, aspect),
                None => crate::news_art::mark_failed(url),
            }
        }
    });
    if !spawned {
        crate::news_art::release_pending(&queued);
        state.news.art_loading = false;
    }
}

fn iter_blocks<'a>(xml: &'a str, tag: &'a str) -> impl Iterator<Item = &'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut rest = xml;
    std::iter::from_fn(move || {
        let start = rest.find(&open)?;
        let after = &rest[start..];
        let inner_start = after.find('>')? + 1;
        let close_at = after.find(&close)?;
        if close_at < inner_start {
            rest = &after[1..];
            return None;
        }
        let block = &after[inner_start..close_at];
        rest = &after[close_at + close.len()..];
        Some(block)
    })
}

fn tag_text(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = block.find(&open)?;
    let after = &block[start..];
    let gt = after.find('>')?;
    let inner = &after[gt + 1..];
    let close = format!("</{tag}>");
    let end = inner.find(&close)?;
    Some(decode_cdata(&inner[..end]))
}

fn decode_cdata(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("<![CDATA[") {
        if let Some(inner) = rest.strip_suffix("]]>") {
            return inner.to_string();
        }
        if let Some(idx) = rest.find("]]>") {
            return rest[..idx].to_string();
        }
    }
    t.to_string()
}

fn item_link(block: &str) -> String {
    if let Some(t) = tag_text(block, "link") {
        let t = decode_entities(t.trim());
        if t.starts_with("http://") || t.starts_with("https://") {
            return t;
        }
    }
    href_in(block, "link")
        .map(|u| decode_entities(&u))
        .unwrap_or_default()
}

fn href_in(block: &str, tag: &str) -> Option<String> {
    let mut rest = block;
    let needle = format!("<{tag}");
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at..];
        let end = after.find('>').unwrap_or(after.len());
        let tag_src = &after[..end];
        if let Some(url) = attr(tag_src, "href") {
            if url.starts_with("http://") || url.starts_with("https://") {
                let rel = attr(tag_src, "rel").unwrap_or_default();
                if rel.is_empty() || rel == "alternate" {
                    return Some(url);
                }
            }
        }
        rest = &after[needle.len()..];
    }
    None
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn item_image(block: &str) -> Option<String> {
    tagged_attr(block, "media:thumbnail", "url")
        .or_else(|| enclosure_image(block))
        .or_else(|| tagged_attr(block, "media:content", "url"))
        .or_else(|| first_img_src(block))
        .and_then(|u| https_image(&decode_entities(&u)))
        .map(prefer_youtube_still)
}

/// YouTube `hqdefault` is 4:3 with letterbox bars. `mqdefault` is 16:9.
fn prefer_youtube_still(url: String) -> String {
    let Some(rest) = url
        .strip_prefix("https://i.ytimg.com/vi/")
        .or_else(|| url.strip_prefix("https://img.youtube.com/vi/"))
    else {
        return url;
    };
    let Some((id, _)) = rest.split_once('/') else {
        return url;
    };
    if id.is_empty() {
        return url;
    }
    format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")
}

fn https_image(url: &str) -> Option<String> {
    let u = canonical_url(url.trim());
    crate::news_art::url_ok(&u).then_some(u)
}

fn tagged_attr(block: &str, tag: &str, name: &str) -> Option<String> {
    let mut rest = block;
    let needle = format!("<{tag}");
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at..];
        let end = after.find('>').unwrap_or(after.len());
        if let Some(v) = attr(&after[..end], name) {
            if v.starts_with("http://") || v.starts_with("https://") {
                return Some(v);
            }
        }
        rest = &after[needle.len()..];
    }
    None
}

fn enclosure_image(block: &str) -> Option<String> {
    let mut rest = block;
    while let Some(at) = rest.find("<enclosure") {
        let after = &rest[at..];
        let end = after.find('>').unwrap_or(after.len());
        let tag = &after[..end];
        let typ = attr(tag, "type").unwrap_or_default();
        if typ.to_ascii_lowercase().starts_with("image/") {
            if let Some(u) = attr(tag, "url") {
                return Some(u);
            }
        }
        rest = &after["<enclosure".len()..];
    }
    None
}

fn first_img_src(block: &str) -> Option<String> {
    let lower = block.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<img") {
        let at = from + rel;
        let slice = &block[at..];
        let end = slice.find('>').unwrap_or(slice.len());
        if let Some(u) = attr_ci(&slice[..end], "src") {
            return Some(u);
        }
        from = at + 4;
    }
    None
}

fn attr_ci(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=\"");
    let start = lower.find(&key)? + key.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn item_texts(block: &str) -> (String, String) {
    let desc = stripped_tag(block, "description")
        .or_else(|| stripped_tag(block, "summary"))
        .unwrap_or_default();
    let article = stripped_tag(block, "content:encoded")
        .or_else(|| stripped_tag(block, "media:description"))
        .or_else(|| stripped_tag(block, "content"))
        .unwrap_or_default();
    let teaser = tidy_teaser(&desc);
    let body = if article.chars().count() > teaser.chars().count() {
        article
    } else if !teaser.is_empty() {
        teaser.clone()
    } else {
        article
    };
    let body: String = body.chars().take(MAX_BODY_CHARS).collect();
    let snippet = if teaser.is_empty() {
        clip_chars(&body, 140)
    } else {
        clip_chars(&teaser, 180)
    };
    (snippet, body)
}

fn stripped_tag(block: &str, tag: &str) -> Option<String> {
    let t = strip_html(&tag_text(block, tag)?);
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn tidy_teaser(s: &str) -> String {
    let t = s.trim();
    let cut = [
        "Read More",
        "read more",
        "Lire la suite",
        "Leer más",
        "Weiterlesen",
    ];
    for suffix in cut {
        if let Some(rest) = t.strip_suffix(suffix) {
            return rest.trim().to_string();
        }
    }
    t.to_string()
}

fn clip_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

/// Drop `utm_*` tracking so Open goes to the canonical article.
fn canonical_url(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let keep: Vec<&str> = query
        .split('&')
        .filter(|p| {
            let key = p.split('=').next().unwrap_or("");
            !key.is_empty() && !key.to_ascii_lowercase().starts_with("utm_")
        })
        .collect();
    if keep.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", keep.join("&"))
    }
}

pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut lists: Vec<(bool, u32)> = Vec::new();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            while let Some(&x) = chars.peek() {
                chars.next();
                if x == '>' {
                    break;
                }
                tag.push(x);
            }
            let trimmed = tag.trim();
            let closing = trimmed.starts_with('/');
            let name = trimmed
                .trim_start_matches('/')
                .split(|ch: char| ch.is_whitespace() || ch == '/')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(name.as_str(), "script" | "style" | "iframe") && !closing {
                skip_close(&mut chars, &name);
                continue;
            }
            match (closing, name.as_str()) {
                (_, "br") => ensure_nl(&mut out),
                (false, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
                    ensure_blank(&mut out);
                    let n = name.as_bytes().get(1).copied().unwrap_or(b'2') - b'0';
                    let n = n.clamp(1, 3);
                    for _ in 0..n {
                        out.push('#');
                    }
                    out.push(' ');
                }
                (true, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => ensure_blank(&mut out),
                (false, "ul") => {
                    lists.push((false, 0));
                    ensure_nl(&mut out);
                }
                (false, "ol") => {
                    lists.push((true, 0));
                    ensure_nl(&mut out);
                }
                (true, "ul" | "ol") => {
                    lists.pop();
                    ensure_nl(&mut out);
                }
                (false, "li") => {
                    ensure_nl(&mut out);
                    let depth = lists.len().saturating_sub(1);
                    for _ in 0..depth {
                        out.push_str("  ");
                    }
                    match lists.last_mut() {
                        Some((true, n)) => {
                            *n += 1;
                            out.push_str(&format!("{n}. "));
                        }
                        _ => out.push_str("- "),
                    }
                }
                (true, "li") => ensure_nl(&mut out),
                (_, "hr") => {
                    ensure_blank(&mut out);
                    out.push_str("---");
                    ensure_blank(&mut out);
                }
                (_, "p" | "div" | "blockquote" | "section" | "tr") => {
                    if lists.is_empty() {
                        ensure_blank(&mut out);
                    } else if !ends_with_list_mark(&out) {
                        ensure_nl(&mut out);
                    }
                }
                _ => {}
            }
            continue;
        }
        if c == '&' {
            out.push_str(&take_entity(&mut chars));
            continue;
        }
        if c == '\n' || c == '\r' {
            if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        if c.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    normalize_ws(&out)
}

fn ensure_nl(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn ends_with_list_mark(out: &str) -> bool {
    let line = out.rsplit('\n').next().unwrap_or(out).trim_start();
    line == "- "
        || line
            .strip_suffix(". ")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn ensure_blank(out: &mut String) {
    ensure_nl(out);
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
}

fn skip_close(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, name: &str) {
    let close = format!("/{name}");
    loop {
        while chars.peek().is_some_and(|&c| c != '<') {
            chars.next();
        }
        if chars.next().is_none() {
            return;
        }
        let mut tag = String::new();
        while let Some(&x) = chars.peek() {
            chars.next();
            if x == '>' {
                break;
            }
            tag.push(x);
        }
        if tag
            .trim()
            .trim_end_matches('/')
            .eq_ignore_ascii_case(&close)
        {
            return;
        }
    }
}

fn take_entity(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut ent = String::new();
    while let Some(&x) = chars.peek() {
        if x == ';' || ent.len() > 10 {
            break;
        }
        ent.push(x);
        chars.next();
    }
    if chars.peek() == Some(&';') {
        chars.next();
    }
    entity(&ent)
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            out.push_str(&take_entity(&mut chars));
        } else {
            out.push(c);
        }
    }
    out
}

fn normalize_ws(s: &str) -> String {
    let mut lines: Vec<String> = s.lines().map(|l| l.trim_end().to_string()).collect();
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let mut out = String::new();
    let mut blank = false;
    for line in lines {
        if line.trim().is_empty() {
            if !blank {
                out.push('\n');
                blank = true;
            }
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&line);
            blank = false;
        }
    }
    out
}

fn entity(name: &str) -> String {
    match name {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" | "#39" | "#x27" => "'".into(),
        "nbsp" | "#160" => " ".into(),
        "mdash" | "#8212" => "—".into(),
        "ndash" | "#8211" => "–".into(),
        "rsquo" | "#8217" => "'".into(),
        "ldquo" | "#8220" => "\"".into(),
        "rdquo" | "#8221" => "\"".into(),
        _ if name.starts_with('#') => numeric_entity(name),
        _ => String::new(),
    }
}

fn numeric_entity(name: &str) -> String {
    let digits = name.trim_start_matches('#');
    let n = if let Some(hex) = digits
        .strip_prefix('x')
        .or_else(|| digits.strip_prefix('X'))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        digits.parse().ok()
    };
    n.and_then(char::from_u32)
        .map(|c| c.to_string())
        .unwrap_or_default()
}

fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc2822(s)
        .ok()
        .map(|d| d.timestamp())
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|d| d.timestamp())
        })
}

fn format_date(ts: i64, raw: &str) -> String {
    if ts > 0 {
        if let Some(dt) = chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0) {
            return dt.format("%d %b %Y").to_string();
        }
    }
    raw.chars().take(16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?>
<rss><channel>
<item>
  <title>Anniversary Bonus Week &amp; Sales</title>
  <link>https://www.guildwars2.com/en/news/anniversary/</link>
  <pubDate>Tue, 25 Aug 2026 17:00:00 +0000</pubDate>
  <description><![CDATA[<p>Celebrate with <b>bonus</b> rewards.</p>]]></description>
</item>
<item>
  <title>Skip me</title>
  <link>javascript:alert(1)</link>
</item>
</channel></rss>"#;

    const ATOM: &str = r#"<feed>
<entry>
  <title>Code of Creation</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=abc"/>
  <published>2026-08-20T12:00:00+00:00</published>
    <media:group>
    <media:thumbnail url="https://i.ytimg.com/vi/abc/hqdefault.jpg"/>
    <media:description>A trailer about raids.</media:description>
  </media:group>
</entry>
</feed>"#;

    #[test]
    fn rss_item_strips_html_and_skips_bad_links() {
        let items = parse_feed(NewsSource::Official, RSS);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Anniversary Bonus Week & Sales");
        assert_eq!(
            items[0].url,
            "https://www.guildwars2.com/en/news/anniversary/"
        );
        assert_eq!(items[0].body, "Celebrate with bonus rewards.");
        assert_eq!(items[0].snippet, "Celebrate with bonus rewards.");
        assert_eq!(items[0].image_url, None);
        assert_eq!(items[0].published, "25 Aug 2026");
        assert!(items[0].published_ts > 0);
    }

    #[test]
    fn atom_entry_uses_href_and_media_description() {
        let items = parse_feed(NewsSource::Youtube, ATOM);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Code of Creation");
        assert_eq!(items[0].url, "https://www.youtube.com/watch?v=abc");
        assert_eq!(items[0].body, "A trailer about raids.");
        assert_eq!(items[0].published, "20 Aug 2026");
        assert_eq!(
            items[0].image_url.as_deref(),
            Some("https://i.ytimg.com/vi/abc/mqdefault.jpg")
        );
    }

    #[test]
    fn article_img_and_search_filter() {
        let xml = r#"<rss><channel><item>
  <title>14 Years of Guild Wars 2</title>
  <link>https://www.guildwars2.com/en/news/sale/</link>
  <content:encoded><![CDATA[<p>Hello</p><img src="https://www.guildwars2.com/wp-content/x.jpg"/>]]></content:encoded>
</item></channel></rss>"#;
        let items = parse_feed(NewsSource::Official, xml);
        assert_eq!(
            items[0].image_url.as_deref(),
            Some("https://www.guildwars2.com/wp-content/x.jpg")
        );
        assert!(matches(&items[0], None, "14 years"));
        assert!(!matches(&items[0], None, "ranger"));
        assert!(matches(&items[0], Some(NewsKind::Articles), ""));
        assert!(!matches(&items[0], Some(NewsKind::Video), ""));
        assert_eq!(NewsSource::PatchNotes.kind(), NewsKind::Notes);
        assert_eq!(
            prefer_youtube_still("https://i.ytimg.com/vi/abc/hqdefault.jpg".into()),
            "https://i.ytimg.com/vi/abc/mqdefault.jpg"
        );
    }

    #[test]
    fn official_feed_follows_overlay_language() {
        assert!(feed_url(NewsSource::Official, "de").contains("/de/"));
        assert!(feed_url(NewsSource::Official, "zh").contains("/en/"));
        assert!(feed_url(NewsSource::Official, "fr").contains("/fr/"));
        assert!(feed_url(NewsSource::ForumNews, "en").contains("32-news-and-announcements"));
    }

    #[test]
    fn expanded_card_uses_encoded_article_not_teaser() {
        assert_eq!(
            canonical_url("https://www.guildwars2.com/en/news/x/?utm_source=rss&utm_medium=feed"),
            "https://www.guildwars2.com/en/news/x/"
        );
        let xml = r#"<rss><channel><item>
  <title>14 Years of Guild Wars 2</title>
  <link>https://www.guildwars2.com/en/news/sale/?utm_source=rss&#038;utm_medium=news</link>
  <description><![CDATA[Whether you're new to Tyria!</p><p class="more"><a href="https://x">Read More</a></p>]]></description>
  <content:encoded><![CDATA[
<p>The 14th anniversary is here.</p>
<h2>Claim a Freebie in the Gem Store</h2>
<ul>
<li>Black Lion Chest Key</li>
<li>Prince Rurik's Vanguard Cape Skin</li>
</ul>
<p>Happy anniversary, Guild Wars 2!</p>
]]></content:encoded>
</item></channel></rss>"#;
        let items = parse_feed(NewsSource::Official, xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url, "https://www.guildwars2.com/en/news/sale/");
        assert!(
            items[0].snippet.contains("Whether you're new to Tyria"),
            "snippet: {}",
            items[0].snippet
        );
        assert!(
            !items[0].snippet.to_ascii_lowercase().contains("read more"),
            "snippet still has Read More: {}",
            items[0].snippet
        );
        assert!(items[0]
            .body
            .contains("## Claim a Freebie in the Gem Store"));
        assert!(items[0].body.contains("- Black Lion Chest Key"));
        assert!(items[0].body.contains("Happy anniversary"));
        assert!(items[0].body.len() > items[0].snippet.len());
    }

    #[test]
    fn strip_html_headings_nested_lists_and_numbers() {
        let html = r#"
<h1>Patch</h1>
<h2>Open World</h2>
<ul>
<li>Outer</li>
<ul><li>Nested</li></ul>
<li>Also outer</li>
</ul>
<ol>
<li>First</li>
<li>Second</li>
</ol>
<p>Bye.</p>
"#;
        let t = strip_html(html);
        assert!(t.contains("# Patch"), "{t}");
        assert!(t.contains("## Open World"), "{t}");
        assert!(t.contains("- Outer"), "{t}");
        assert!(t.contains("  - Nested"), "{t}");
        assert!(t.contains("1. First"), "{t}");
        assert!(t.contains("2. Second"), "{t}");
        assert!(t.contains("Bye."), "{t}");
        let li_p = strip_html("<ul><li><p>Outer</p></li></ul>");
        assert!(li_p.contains("- Outer"), "{li_p}");
        assert!(!li_p.contains("- \nOuter"), "{li_p}");
    }
}
