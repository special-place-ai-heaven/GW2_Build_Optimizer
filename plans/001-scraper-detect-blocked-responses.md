# Plan 001 — Detect non-OK / wrong-content scrape responses; give a clear error

**Written against commit:** `5406273` (run `git rev-parse --short HEAD`; if it
differs, re-read the cited code before editing — line numbers may have drifted).

**Category:** correctness + developer/user experience
**Effort:** S–M · **Risk:** Low (additive error detection + tests; no scoring,
optimizer, or build-resolution behavior changes)
**Crate:** `gw2-optimizer` · **File in scope:** `crates/optimizer/src/scraper.rs`
**Files explicitly OUT of scope:** every other file in the repo. Do not touch
combat/scoring/optimizer code, the UI/addon crate, or any other scraper logic
beyond what the steps below name.

---

## Why this matters (full context — executor has none)

The addon scrapes three community build sites (Snowcrows, Hardstuck, GuildJen)
to populate "benchmark data" used for comparisons. The user reported that
benchmark sync "does not work, never worked really." The Settings panel shows:

```
Synced: 2026-06-17  SC:45  HS:0  GJ:30
[!] hardstuck: Hardstuck: no build links found on any profession page
```

Snowcrows (45) and GuildJen (30) work. **Hardstuck returns 0** with that error.

### Root cause (already investigated live — do NOT re-investigate the network)

A real HTTP GET to `https://hardstuck.gg/gw2/builds/guardian/` (the exact URL
the scraper builds) returns **HTTP 200 OK** with a **563 KB Ubiquiti/UniFi
network block page**, whose body contains:

> "**Blocked** ... blocked because the domain is restricted. Contact your
> administrator for more information."

It is branded with `ubiquiti` / `UniFi` markers and served by `lighttpd`. In
other words: the user's network gateway (a UniFi UDMPro with a content filter)
is intercepting `hardstuck.gg` and returning a block page **with a 200 status**.
The real builds page never arrives, so there are no `href=".../gw2/builds/..."`
links to find, and the scraper reports "no build links found on any profession
page."

### Two separate issues — this plan fixes ONLY the second

1. **Environmental (NOT in scope, no code change).** For Hardstuck data to
   actually load, the user must allowlist `hardstuck.gg` on their UDMPro content
   filter. Nothing in the repo can fix a network-level block. **Do not** try to
   bypass it, add a proxy, add a headless browser, or remove the Hardstuck
   scraper. Just make the failure legible (issue 2).

2. **Real code defect (THIS PLAN).** `fetch_html` ignores the HTTP status code
   and never checks whether the returned page is plausibly from the expected
   site. A 200-OK block page — or any captive-portal / redirect / wrong-content
   response — is silently accepted as valid HTML, and the only symptom the user
   sees is the misleading "no build links found." The scraper cannot tell
   "blocked / site moved / captive portal" apart from "genuinely zero builds."

   Fixing this turns a confusing dead-end into an actionable message and hardens
   **all three** scrapers: Snowcrows and GuildJen would fail identically if they
   were ever blocked, moved, or returned an error page with a 200.

This is worth doing even though the immediate trigger is environmental: the
diagnostic is generic, low-risk, and the next person (or the user, post-allowlist)
gets a message that points at the real cause instead of a wrong one.

---

## Current state (exact code, read at commit `5406273`)

### `fetch_html` — the defect. `crates/optimizer/src/scraper.rs` ~L592

```rust
fn fetch_html(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    client
        .get(url)
        .send()
        .map_err(|e| format!("HTTP error fetching {}: {}", url, e))?
        .text()
        .map_err(|e| format!("UTF-8 error reading {}: {}", url, e))
}
```

It calls `.send()` then `.text()` and returns the body. It **never inspects
`.status()`**, so a `200` block page, a `403`, a `404`, or a `503` maintenance
page all return `Ok(body)` and flow downstream as if valid. (Note: reqwest does
*not* error on non-2xx by default — only on transport failures.)

### How the misleading error is produced. `scrape_hardstuck` ~L425

```rust
    if all_links.is_empty() {
        return Err("Hardstuck: no build links found on any profession page".into());
    }
```

`all_links` is empty because `extract_build_links` found no build hrefs in the
block-page HTML. The error blames "no builds" when the truth is "we got the
wrong page."

### Sibling scrapers with the same shape (in scope for the status check only)

- `scrape_snowcrows` ~L242 → its own `if links.is_empty()` at ~L273:
  `"Snowcrows: no build links found on any profession page"`.
- `scrape_guildjen` ~L507 → uses `extract_build_links` per index page.
- All three call `fetch_html` for both index and per-build fetches.

### Existing test pattern to copy. `crates/optimizer/src/scraper.rs` ~L993

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_build_links_finds_hrefs() {
        let html =
            r#"<a href="/builds/guardian/firebrand">Firebrand</a><a href="/other">Other</a>"#;
        let links = extract_build_links(html, "/builds/", 10);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "/builds/guardian/firebrand");
    }
    // ... more #[test] fns follow the same plain-unit-test style: build an
    //     input, call the private fn, assert. No async, no network, no fixtures.
}
```

New tests go in this same `mod tests` block, same style.

---

## What to build

Two changes, both in `crates/optimizer/src/scraper.rs`:

### Change A — make `fetch_html` reject non-success HTTP statuses

Inspect the status before reading the body. On a non-success status, return a
descriptive `Err` that names the status code and URL.

Replace the body of `fetch_html` with:

```rust
fn fetch_html(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("HTTP error fetching {}: {}", url, e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!(
            "HTTP {} fetching {} (expected 2xx)",
            status.as_u16(),
            url
        ));
    }

    resp.text()
        .map_err(|e| format!("UTF-8 error reading {}: {}", url, e))
}
```

This alone does NOT fix the Hardstuck symptom (the block page is `200`), but it
is the correct, generic hardening and catches the common 403/404/503 cases. Do
it anyway — it is part of the same defect (status was ignored).

### Change B — detect a "wrong-content / blocked" page and report it clearly

Add a small private helper that recognises an interstitial / block / captive
page, and call it inside each scraper's "no links found" branch so the error
distinguishes "blocked or wrong page" from "genuinely no builds."

1. Add this helper (place it next to `fetch_html`, after it):

```rust
/// Heuristic: does this look like a network/security interstitial (block page,
/// captive portal, WAF challenge) rather than the real site content?
///
/// These pages return HTTP 200 with a body that mentions being blocked, so a
/// status check alone won't catch them. We only flag high-confidence markers to
/// avoid false positives on legitimate build pages.
fn looks_like_blocked_page(html: &str) -> bool {
    let lower = html.to_lowercase();
    // "blocked" + a gateway/filter vendor or "restricted"/"administrator" cue.
    let mentions_blocked = lower.contains("blocked because")
        || lower.contains("access denied")
        || lower.contains("this site is blocked")
        || lower.contains("domain is restricted");
    let mentions_gateway = lower.contains("ubiquiti")
        || lower.contains("unifi")
        || lower.contains("contact your administrator")
        || lower.contains("content filter");
    (mentions_blocked && mentions_gateway) || lower.contains("domain is restricted")
}
```

> Rationale for the marker set: the observed UniFi block page contains
> "blocked because the domain is restricted", "Contact your administrator",
> and "ubiquiti"/"UniFi". Requiring a *blocked* cue AND a *gateway* cue (or the
> very specific "domain is restricted" phrase) keeps this from misfiring on a
> normal build page that happens to use the word "blocked" (e.g. a skill that
> "blocks" attacks). Do not broaden these markers without re-checking that the
> scraper unit tests for real build HTML still pass.

2. In `scrape_hardstuck`, change the empty-links branch (~L425) from:

```rust
    if all_links.is_empty() {
        return Err("Hardstuck: no build links found on any profession page".into());
    }
```

to:

```rust
    if all_links.is_empty() {
        // Distinguish "we were served a block page / wrong content" from
        // "the site genuinely listed no builds" — the former is almost always a
        // network filter (e.g. a UniFi/UDMPro content block on hardstuck.gg) and
        // needs an allowlist change, not a code fix.
        if last_html.as_deref().map(looks_like_blocked_page).unwrap_or(false) {
            return Err(
                "Hardstuck: request was intercepted by a network block/filter page \
                 (the domain appears restricted by your gateway/firewall — allowlist \
                 hardstuck.gg, e.g. on your UniFi/UDMPro content filter)"
                    .into(),
            );
        }
        return Err("Hardstuck: no build links found on any profession page".into());
    }
```

   To make `last_html` available, capture the most recent fetched profession-page
   HTML in the loop. Find the loop in `scrape_hardstuck` (~L398) that does:

```rust
    for profession in HS_PROFESSIONS {
        if should_cancel() {
            return Ok((Vec::new(), true));
        }
        let prof_url = format!("https://hardstuck.gg/gw2/builds/{}/", profession);
        let Ok(html) = fetch_html(client, &prof_url) else {
            continue;
        };
        // ... extract_build_links(&html, ...) ...
    }
```

   Declare `let mut last_html: Option<String> = None;` just above the
   `let mut all_links: Vec<String> = Vec::new();` line (~L395), and inside the
   loop, after a successful `fetch_html`, set `last_html = Some(html.clone());`
   BEFORE the link extraction consumes `html` (the existing code passes `&html`
   to `extract_build_links`, so cloning once per profession is fine — this loop
   runs at most 9 times). Keep the existing `let Ok(html) = ... else { continue }`
   binding; just add the assignment after it.

   > If capturing `last_html` per-profession turns out to require more than a
   > one-line clone (e.g. the binding is restructured), STOP and report — do not
   > refactor the loop.

3. Apply the **same** block-page detection to `scrape_snowcrows`'s empty-links
   branch (~L273) and to `scrape_guildjen` if it has an equivalent
   "no links / no builds" terminal error. Mirror the Hardstuck change: capture
   the last fetched index HTML, and in the empty/`Err` branch, if
   `looks_like_blocked_page` is true, return a message of the form
   `"<Source>: request was intercepted by a network block/filter page (allowlist <domain> on your gateway/firewall)"`.
   Use each source's own domain in the message (`snowcrows.com`, `guildjen.com`).

   > If a given scraper's control flow makes capturing the last HTML awkward
   > (e.g. GuildJen builds links across multiple index URLs differently), it is
   > acceptable to add the block-page check ONLY where an index fetch happens and
   > the result is empty. Prefer the Hardstuck pattern; if it does not map
   > cleanly onto Snowcrows/GuildJen, implement it for Hardstuck (the reported
   > case) and the one other that maps cleanly, and note in your report which you
   > skipped and why. Do NOT contort the control flow.

---

## Step-by-step

1. Confirm commit: `git rev-parse --short HEAD` → expect `5406273`. If different,
   re-read `fetch_html`, `scrape_hardstuck`, `scrape_snowcrows`, `scrape_guildjen`
   in `crates/optimizer/src/scraper.rs` and adjust line references.
2. Apply **Change A** (status check in `fetch_html`).
3. Build: `cargo check -p gw2-optimizer` → expect `Finished` with no errors.
4. Apply **Change B.1** (add `looks_like_blocked_page` helper).
5. Apply **Change B.2** (Hardstuck: capture `last_html`, branch on block page).
6. Apply **Change B.3** (Snowcrows, and GuildJen if it maps cleanly).
7. Build again: `cargo check -p gw2-optimizer` → no errors.
8. Add tests (see Test plan).
9. Run gates (see Done criteria).

---

## Test plan

Add to the existing `#[cfg(test)] mod tests` block in `scraper.rs`, same plain
unit-test style as `test_extract_build_links_finds_hrefs`. These test the pure
helper — **no network**.

```rust
#[test]
fn test_looks_like_blocked_page_detects_unifi_block() {
    // Markers taken from the observed UniFi/Ubiquiti block page.
    let html = r#"<html><head><title>Blocked</title></head><body>
        <div id="info-box"><svg id="ubiquiti-logo"></svg>
        <p>This domain is restricted. Contact your administrator for more information.</p>
        <p>blocked because the domain is restricted</p></body></html>"#;
    assert!(looks_like_blocked_page(html));
}

#[test]
fn test_looks_like_blocked_page_ignores_real_build_page() {
    // A normal build page that happens to use the word "blocks" must NOT trip it.
    let html = r#"<html><body>
        <a href="/gw2/builds/guardian/firebrand">Firebrand</a>
        <p>Shield of Courage blocks the next attack. Great sustain build.</p>
        </body></html>"#;
    assert!(!looks_like_blocked_page(html));
}

#[test]
fn test_looks_like_blocked_page_ignores_empty() {
    assert!(!looks_like_blocked_page(""));
    assert!(!looks_like_blocked_page("<html><body>No builds here yet.</body></html>"));
}
```

If you also want to assert the message wiring without network, that is optional
and likely needs refactoring to inject HTML — do NOT refactor the scrapers just
to test the message. The helper tests above are the required coverage.

---

## Done criteria (machine-checkable)

Run from repo root:

1. `cargo check -p gw2-optimizer` → exits 0, prints `Finished`.
2. `cargo test -p gw2-optimizer scraper 2>&1 | grep "test result"` → every line
   shows `0 failed`. The three new `looks_like_blocked_page` tests appear and
   pass.
3. `cargo clippy --workspace --all-targets 2>&1 | grep -cE "^warning: \[a-z\]"`
   → prints `0` (the repo is currently clippy-clean; keep it that way).
4. `cargo fmt --all -- --check` → exits 0 (no diff). If it fails, run
   `cargo fmt --all` and re-run gates.
5. Grep proof the misleading-only path is gone for the blocked case:
   `cargo test -p gw2-optimizer test_looks_like_blocked_page_detects_unifi_block`
   → `1 passed`.

Do NOT run the live scraper as a verification step — the target domain is
blocked on this network, so it cannot succeed here regardless of the code. The
unit tests are the gate.

---

## Stop conditions (report back instead of improvising)

- If `fetch_html`, `scrape_hardstuck`, `scrape_snowcrows`, or `scrape_guildjen`
  no longer match the excerpts above (significant drift since `5406273`), STOP
  and report the actual current code.
- If adding `last_html` capture requires restructuring a scraper loop beyond a
  single `.clone()` assignment, STOP and report.
- If making the Snowcrows/GuildJen change cleanly is not possible without
  contorting control flow, implement Hardstuck + whichever maps cleanly, and
  report which you skipped (this is acceptable, not a failure).
- If any existing scraper test starts failing because of a `looks_like_blocked_page`
  false positive, STOP — do not weaken the assertion in the existing test;
  instead tighten the helper's markers and report.

---

## Maintenance note (for the reviewer)

- This is purely diagnostic hardening. It changes **no** build-scoring,
  optimizer, or resolution behavior — only the error text and the HTTP-status
  gate on scrape fetches. Review should confirm: (a) `fetch_html` still returns
  the body unchanged on a real 200, (b) the `looks_like_blocked_page` markers are
  conservative (require both a "blocked" and a "gateway" cue, or the specific
  "domain is restricted" phrase), (c) no `unwrap`/`expect` was introduced.
- The underlying user-facing fix is environmental: allowlist `hardstuck.gg` on
  the UniFi UDMPro content filter. After that, Hardstuck may sync normally; if it
  is unblocked and *still* returns no links, that is a separate investigation
  (Hardstuck may have moved to fully client-side rendering) and a new plan — not
  this one.
- If a future change broadens the block-page markers, re-run the
  `test_looks_like_blocked_page_ignores_real_build_page` test against a few real
  saved build HTML samples to guard against false positives that would mask real
  "no builds" results.
