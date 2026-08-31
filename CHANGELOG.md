# Changelog

All notable changes to GW2 Build Optimizer are documented here.

## 1.8.3 - 2026-08-31

### Radio

- Real equalizer bars behind the player bar: 24 log-spaced frequency bands measured from the decoded audio itself (not a fake animation), drawn in translucent gold so the status and now-playing text stay readable. Bars rise with the music and sink gracefully when playback stops. Their height tracks the stream, not the volume slider — turning the game-side volume down does not flatten the show.

## 1.8.2 - 2026-08-31

### Radio

- Language and Country filters on the search row, applied to every search (name and genre alike). Language defaults to Auto — it follows your overlay language, so an English UI surfaces English stations and a French one surfaces French. Both persist in config.
- Station rows use `-` separators instead of the bullet the game font cannot draw (no more `?`).

## 1.8.1 - 2026-08-31

### Radio

- Clicking a station no longer crashes: the tune-in built tokio timers on the audio thread outside any runtime context ("there is no reactor running"). The audio thread now enters the runtime for the whole connect sequence, pinned by a regression test.

## 1.8.0 - 2026-08-31

### Radio (new tab)

- Internet radio while you game: search 30,000+ stations (radio-browser.info) by name, genre, or country and play them in the background.
- Choya DJ mans the decks in the corner — sleeps when idle, dances while tuning, bobs on air, with an ON AIR badge and EQ bars.
- Now-playing song titles from the stream (ICY metadata), favorites saved to your config, volume with a proper log taper, optional keybind toggle (assign it in Nexus).
- Streams that stall reconnect once quietly; a lost audio device tells you instead of dying silently.
- HLS-only and undecodable-codec stations (OGG/FLAC/Opus) are filtered out rather than offered and failing.
- If a station will not connect, your antivirus or firewall may be blocking it — the error says so.

## 1.7.26 - 2026-08-30

### Combat

- WvW viability no longer treats a missing dummy HP as 0 (any spike used to pass the 30% gate).
- Prefix search builds the itemstat pool once per neighbour generation instead of three times.

### Overlay

- Fetching the official news feed no longer starts the 30-minute cache clock for YouTube, GuildJen, or the other sources.
- Settings language list uses English names for Chinese/Japanese/Korean until a CJK font is active (no more `????`).
- Font combo lists every Windows face we can find (Segoe, YaHei, Yu Gothic, Malgun), not only ones already loaded this session.
- Data-quality notes use `--` instead of an em-dash the game font cannot draw.

## 1.7.25 - 2026-08-30

### Combat

- WvW barrier now expires after 5 seconds and caps at 25% of max health.
- Interrupted skills use a 5 second cooldown instead of their full recharge.
- A killed dummy no longer keeps punching you for the rest of the window.
- Condition leftover fractions tick (1.5s pays 1.5 ticks, not 1 or 2).
- Large-scale WvW profiles with no kill target no longer invent 18k/24k/35k HP.
- Rotation scheduler values strike crit the same way the evaluator does.
- Corrupt converts the stripped boon; Steal grants it to you.

### Overlay

- News feed text no longer eats words after a bare `&`.
- A failed feed fetch keeps the last good items and retries in 45 seconds, not 30 minutes of empty.
- Lock/news counts use the active language's plural rule (English "21 locks", not "21 lock").
- Localized weapon labels accept Harpoon / Harpoon Gun like English does.

### Other

- Game-data refresh no longer reports success when the icon folder cannot be created.
- API key requests reject lookalike loopback hosts (`127.0.0.1.evil.com`).
- GuildJen scrape treats an empty or blocked index as down, not "done 0".
- Scraper redirects stay on the same host; Luminary is on the shared spec list.

## 1.7.24 - 2026-08-30

### Fixed

- Duplicate specialization selections are now rejected (two of the same spec no longer validate as a build).
- LLM advisor gear swaps can no longer write a prefix onto a slot the build does not wear (e.g. an off-hand prefix on a Greatsword build).
- `get_skill_info` tool no longer fuzzy-matches needles shorter than 5 characters — exact names still resolve.

### Removed

- Dead suggestion-card rendering code and an unused API model.

## 1.7.23 - 2026-08-30

### Overlay

- News, What's new, and other prose wrap to the pane instead of clipping at the right edge.
- First run and Reset layout open at ~80% of the monitor. Ultrawide uses a 1920-wide box so the overlay does not span the desk.

## 1.7.22 - 2026-08-30

### Changed

- Optimizer: Torment tick now blends stationary/moving by the Generic rotation profile's `movement_fraction` (PvE f=0.2, PvP f=0.5, WvW f=0.6). Condi scores on moving-target modes drop accordingly — PvE Torment @1000 condi: 121.8 → 113.84. No build ranking logic changed.

### Tests

- Referee: new pin `unset_viability_gates_pin_hardcoded_ehp_floors` locks the six hardcoded EHP floors (PvE 11000, PvP 8000, WvW Roam 15000, Havoc 13000, Zerg 10000, Staller 15000) for profiles with no `viability_gates` key; extended override test asserts gate notes show profile floors.
- Combat: Torment blend endpoint pins (f=0 stationary, f=1 moving) and updated PvE/PvP mode-dispatch expectations.

## 1.7.21 - 2026-08-30

### Overlay

- The ImGui draw hook stays registered on load, same as 1.7.20. Registering it from a PostRender callback mutates Nexus's render list while that list is being walked and takes the game down (heap 0xC0000374).
- Quick-access icon textures wait for a PostRender after a 2s settle. English/`auto` still does not load CJK faces.

## 1.7.20 - 2026-08-30

### Overlay

- Load panic no longer takes the game down. Unload keeps the DLL mapped until workers finish.
- Settings LLM keys stay masked on paste. Show/Hide still works.
- Copy says Copied only after Windows has the text.
- Ranch Load runs off the draw pass and does not persist notes on the click frame.
- Weapon lock on Current keeps the equipped prefix. Improve with a weapon lock keeps that prefix.
- Ungated Improve does not stamp Improved.
- Empty leftover kits stay Blocked, not Verified.
- No game data: lock panel says (Load game data first). No character: (Select a character first).
- Missing combat metrics read (not computed), not 0/0/0. Viability gates use readable names.
- Condi/boon duration on the panel includes trait duration, not only Expertise/Concentration.
- Optimized spec panel majors match English trait names when the overlay is de/fr.
- Spanish leftover chrome is translated. Dutch titles: ARMOR to RUSTING, STATS to STATISTIEKEN, BOONS to ZEGENINGEN.

### Improve, Choya, plates

- Alacrity is +25% recharge (10s to 8s), not the old 33%.
- Confusion 1s pulse is over-time; on-skill-use fires on activation.
- Live Might raises dummy condition ticks (player Might; dummy stays unbooned).
- Trait duration percent sits outside the Expertise/Concentration cap; skill-specific trait duration stays inside the Expertise cap.
- Dummy prot+stab cover is WvW only.
- Intensity stack cap is 1500 in every mode (no old PvP 100 bleed clamp).
- Set-2 sigils are copied from set 1. Land bars drop the aquatic palette. Land Spear stays a terrestrial two-hander.
- Giver's prefers three-stat itemstat 628, not 627.
- Ranger pets stay on chat plates. Revenant legends parse and win on the plate.
- Infer-from-heal warns on overwrite; rest-pad does not warn inferred-from-heal.
- Spec/item/prefix apply is exact or fuzzy only if the needle is 5+ characters. Skill resolve is full-name equal, not substring. Garbage trait names do not autofill.
- Gemini 403/429 with billing language is a billing issue, not a bad key. Gemini RPM persists across client create. Anthropic key check uses GET /v1/models and should not spend Messages quota. Cancel during a Gemini stream actually stops.
- Cancel a scrape: last-good benchmarks stay. Failed stills retry on Refresh. Dire is a word, not a substring of directly. Immobilized aliases to Immobile.
- Improve stamps the role profile id for viability gates, but JSON still has no ehp_floor numbers — live rank does not jump from EHP this build.

### News, About, mail

- Article bodies are capped. Stills only load from the allowlisted host.
- If an art worker dies mid-flight, leftover Pending URLs are released.
- Slavic languages get proper one/few/many story counts.
- Mailbag titles and POST bodies strip leftover encode tokens.
- Rate-limit row and wizard copy talk in remaining minutes.
- About wrap uses window-local X.
- Status poll bodies are capped at 1 MiB.

### Saves and data

- Save-build write errors surface. Windows overwrite uses ReplaceFileW.
- Unknown GearSlots keys survive a save/load round-trip.
- Hollow game-data (empty skills/items/traits) fails closed.
- Bulk GW2 5xx skips the rotten id. Oversize JSON bodies fail closed.

### Already on the feedback site

Admin GET list is read-only; marking read is a POST. Empty admin password or session secret fails closed.

### Not in this build

GW2 API key first-8/last-4 hint was reverted. Scraper heal/Luminary patches were reverted. Dummy Torment moving vs stationary and movement_fraction are not wired.

## 1.7.19 - 2026-08-29

Choya's mailbag is one box: type, Enter for a new line, select text and press B or click Bold/a colour. The flipped B icon is a real B now, and the row of `?` faces is gone — overlay fonts could not draw them. Formatting still shows in Looks like; the box itself is plain ImGui text.

## 1.7.18 - 2026-08-28

Mailbag has a row of faces (the BMP symbols Segoe UI can actually draw — not colour emoji; Nexus owns the font atlas, so the ImGui FreeType colour-glyph flags do not apply). A reply from the developer opens Messages and expands that row; the About tab still pulses if you were elsewhere.

## 1.7.17 - 2026-08-28

Choya's mailbag is a notepad bar: normal, bold, five text colours, bullets, numbers, left/center/right. You type plain sentences; Enter starts the next paragraph. Hover an icon for its name. Overlay fonts still have no real bold face — the preview doubles the ink by a pixel.

## 1.7.16 - 2026-08-28

Detail news stills default to 3× the old 240px cap, with a zoom slider and Reset. Article bodies (patch notes included) keep headings, nested bullets, and numbered lists instead of one cream brick. Choya's mailbag has H1/H2/H3, list, and number buttons plus a preview — overlay fonts still cannot bold.

## 1.7.15 - 2026-08-28

News filters are square icon buttons (all / article / notes / video / book). Hover shows the name and what it includes. Layout is Compact, Card, or Detail — radios, not a second capsule bar.

## 1.7.14 - 2026-08-28

Settings News sits in one full-width band: Desk / Magazine / Reader and Show stills on a single row, then four source columns. Cache and Benchmarks sit side by side under that, so the tab no longer needs a long scroll. The News desk list takes most of the pane so titles fit; stills keep their aspect (letterboxed, not stretched). Benchmark sync stamps a date even when a site returns nothing, restores counts from disk on load, and shows live progress instead of a stuck Syncing… / Never synced pair.

## 1.7.13 - 2026-08-28

News is a reading desk, not a stack of teaser cards. Settings groups sources by type (articles, patch notes, video, guides) and keeps stills on or off. The News tab filters by type, searches, and switches Desk (list + reader), Magazine (cells), or Reader (full article). Previous / Next, copy link, and open in browser sit on the article. YouTube is a thumbnail plus description — the overlay cannot play video.

## 1.7.12 - 2026-08-28

Clicking a news card opens the full article text from the feed (lists, headings, paragraphs), not the two-line "Read More" teaser. Compact cards still show the short blurb.

## 1.7.11 - 2026-08-28

Settings News also lists official forum announcements. Cards use the short RSS description (not the HTML dump), and tracking junk is stripped from links.

## 1.7.10 - 2026-08-28

Setup game-data now fills the wait with official Guild Wars 2 RSS cards. Click a card to read it; the others shrink. Settings has a News checklist (official, patch notes, ArenaNet YouTube, GuildJen). Tick any source and a News tab appears so you can sort a timeline or group by source.

## 1.7.9 - 2026-08-28

What's new on the About tab uses the same text size as the rest of the overlay, gold version headings, and cream body text. The full changelog scrolls instead of the last five notes.

## 1.7.8 - 2026-08-28

Gear locks live on the ARMOR / TRINKETS / WEAPONS sheet. Click a piece to pin it (bright blue name, gold ring — same language as a selected trait) or click again to release (dim grey). The old checkbox list under Locks is gone. Lock All / Unlock All still covers gear.

## 1.7.7 - 2026-08-28

Pet portraits sit in a padded 256px canvas, so they looked half-size next to skill icons. They now crop-zoom to fill the same box as utilities.

## 1.7.6 - 2026-08-28

Pet skills take less leftover width (~30% narrower); that space goes to the utility skills.

## 1.7.5 - 2026-08-28

The SKILLS card title is gone. That header row is now three areas — PET SKILLS, UTILITY SKILLS, ELITE SKILL — each with its own gold tick. Slots sit directly under those titles.

## 1.7.4 - 2026-08-28

The skill bar is three groups — PET SKILLS, UTILITY SKILLS, ELITE SKILL — with centered headers. Utility and elite squeeze so ranger pets keep a full two-line name (Siege / Turtle) instead of cutting off. Skill names center in the box, or wrap to two lines when they are more than one word.

## 1.7.3 - 2026-08-28

Overlay fonts cannot draw colour emoji, so viability rows showed `?` instead of pass/fail and `?1` instead of `>= 1`. Those are now `OK`/`NO` and `>=`. Ranger pets sit on the skill row with skill-sized icons, utilities and elite squeeze to the right, and the SKILLS header uses the same gold-tick title as the other cards. Choya "some Plaguedoctor" no longer paints every slot (emits `gear_slots` for the mixed pieces).

## 1.7.2 - 2026-08-28

**Ranger pets are part of the build.** Current and Optimized now keep the equipped terrestrial pets, resolve `/v2/pets` names instead of `#66`, and show pet chips (icon + hover) on the skill bar. Refresh game data once so the pet catalog and icons land.

Gear lock "Click to lock" tooltips only appear while the cursor is on that row, so they no longer follow the mouse off the overlay.

## 1.7.0 - 2026-08-26

**Per-slot gear prefixes — full hybrid builds.** Every weapon, armor piece, and trinket now carries its own single-stat prefix, individually chosen by the optimizer, lockable by you, and proposed by Choya. Berserker's weapons with a Cavalier's chest and Cleric's rings is now one optimize click.

- 16-slot gear model (6 armor, back + 2 accessories + amulet + 2 rings, two weapon sets' main/off hands) replaces the build-wide and group prefixes; old saves load unchanged and migrate automatically.
- The optimizer explores per-slot swaps alongside uniform and per-group moves, respecting per-piece gear locks (Locks panel, new Gear section).
- Choya can plate per-slot gear mixes; unknown prefixes fall back to your profile prefix with a warning.
- All four providers (OpenRouter, OpenAI, Anthropic, Gemini) now stream their responses.


## 1.6.4 - 2026-08-26

The foundational transport rework: **every provider now streams.** A shared LLM transport layer replaces four hand-rolled clients, the optimizer surfaces stale locks instead of silently overriding them, and the addon's largest file is split by responsibility. Twelve commits on top of the v1.6.3 hardening sweep, executed bedrock-up with per-layer verification gates.

### All four providers stream

- **OpenAI and OpenRouter** share one chat core (`llm::openai_compat`): identical wire types, one streaming implementation, one retry policy (408/504/529 retryable, `Retry-After` honored, rate-tracker handshake inside). The OpenAI provider picks up streaming and the 900-second budget — its old non-streaming client carried the exact false-timeout class v1.6.1 fixed for OpenRouter. Completion budget aligned to 16,384 tokens for both.
- **Anthropic Messages** streams: `read_anthropic_stream` assembles content blocks from the event sequence — `text_delta` concatenation, `tool_use` `input_json_delta` fragments stitched and parsed to JSON, `message_delta` stop reason, and in-band `error` events mapped to typed errors (`overloaded_error` → 529).
- **Gemini** streams via `streamGenerateContent?alt=sse`: text parts concatenate in arrival order, `functionCall` parts pass through whole, and `error` payloads map to typed errors.
- Callers are unchanged everywhere — each stream still lands as the same response type the flows already consumed.

### Shared transport bedrock

- `llm::sse` — the streaming reader (keep-alive skipping, delta accumulation, fragmented parallel tool-call merging) is shared infrastructure with its own test suite, ready for any future provider.
- `llm::response_cache::ResponseCache` — one TTL + size-cap cache replaces four inlined copies.
- `gw2api::transport::read_body_capped` — response bodies are capped everywhere: scraper 2 MiB, GW2 API 8 MiB, feedback client 1 MiB, so no endpoint can stream unbounded bytes into the game process.

### Optimizer

- A trait lock whose id no longer exists in the spec's trait rows (stale after a game-data refresh) is now **reported** through the data-quality reasons that already render in the comparison panel, instead of being silently replaced by the archetype-best pick. Regression-tested: stale ids warn, valid locks stay silent.
- Determinism sweep came back clean — the v1.6.3 amulet fix was the last map-fed float accumulation.

### Addon

- `optimization.rs` (2,645 lines, three responsibilities) is split: `chat_flow.rs` owns the Choya pipeline, `optimize_flow.rs` owns Optimize/Improve, and the shared suggestion vocabulary stays in `optimization.rs`. Pure moves.
- Chat history is written on a background thread (snapshot under the lock, atomic temp+rename write) — disk latency can no longer stall the frame.
- Clipboard copies retry three times against transient clipboard contention.

### Verification

- CI: `cargo fmt --check`, `cargo clippy -D warnings`, and the full workspace test suite run on every push to main and every PR (windows-latest). The workspace builds **warning-free**.
- 1,397 tests passing, including streaming regression tests for all four providers; the OpenRouter path was additionally verified against the live API (validate, streamed generate, cache, streamed tool loop).

### Install

Download `gw2_build_optimizer.dll` below and drop it into your `Guild Wars 2/addons/` folder (replace the old DLL), then restart the game or reload Nexus. Verify the SHA-256 against `SHA256SUMS.txt` if you like.

## 1.6.3 - 2026-08-26

A hardening sweep: an adversarial multi-agent review of the whole codebase (correctness, security, performance, lock discipline) and every confirmed finding fixed in one pass.

### Security

- The build-site scraper (Snowcrows / Hardstuck / GuildJen benchmark sync) accepted **any TLS certificate**. A hostile network could serve forged pages whose gear prefixes, runes, sigils, relics, and trait lists are parsed into the persistent benchmark cache that feeds optimizer comparisons and LLM prompts. Certificate validation is restored.
- The GuildJen scraper followed **absolute links from the index page verbatim**, so a crafted `href` could point the in-game process at an arbitrary https URL and persist whatever came back as a benchmark. Links are now pinned to `https://guildjen.com/` — only relative paths on the real host are followed.
- Benchmark cache filenames were composed from **scraped URL text** without sanitization; on Windows a crafted path component containing `\` or `..` could write outside the addon's `benchmarks/` folder. Filename components now go through the same whitelist as saved builds.
- Model-generated **build chat codes became clipboard chips on a bare `[&` prefix check**. A code now only becomes a chip if it base64-decodes to a `0x0D` build template with a profession byte and a sane length, so prompt-injected garbage cannot turn into a pasteable in-game link.
- The GW2 API client carried the account key but accepted an **absolute URL as any endpoint**. It now rejects non-loopback absolute endpoints outright (loopback stays allowed for the test suite).
- Scraper responses are capped at 2 MiB instead of slurping unbounded bodies into the game process.

### Correctness

- Four scraper text extractors sliced HTML at fixed byte offsets — a multi-byte UTF-8 character landing on the boundary **panicked the process**. All truncation now respects character boundaries.
- The LLM context-trim budget counted **bytes ÷ 4**; CJK text runs ~1 token per character, so localized (Chinese/Japanese/Korean) conversations could blow past the context window and get requests rejected instead of trimmed. Estimation is now script-aware: ASCII ≈ 4 chars/token, everything else ≈ 1 token/char.
- PvP amulet attributes were accumulated in **HashMap iteration order**, and f64 addition is order-sensitive — scores could diverge at the ULP level between runs, contradicting the optimizer's determinism guarantee. Accumulation is now key-sorted, matching every other accumulation site.
- The buff-profile lookup is now **locally guaranteed to return exactly 3 profiles** (truncate + pad) instead of relying on a distant embedded-data validation, so five hot-path `[0]/[1]/[2]` index sites cannot panic if that validation ever relaxes.
- During a GW2 API outage with an empty cache, the overlay **respawned the character and game-data loader threads every frame** with no cooldown. Retries are now gated to once per 30 s (characters) and 60 s (game data).
- The LLM response caches in all four providers were insert-only: expired entries were never evicted and the maps grew without bound for the life of the process. Inserts now sweep expired entries and cap the map at 64 responses.

### Maintenance

- Removed ~30 dead items (an abandoned gear-diff rendering subsystem and its helpers — about 1,900 lines) that predated the comparison-tab rework. `cargo build` is now **warning-free across the workspace**.

### Known follow-ups (not in this release)

- A trait lock whose id no longer exists in the spec's trait column is still silently replaced by the archetype-best pick; surfacing that warning requires threading a warnings channel through the beam search.
- The Gemini, OpenAI, and Anthropic providers remain non-streaming (the streaming/timeout class of fixes in 1.6.1 was OpenRouter-specific).

## 1.6.2 - 2026-08-26

Choya's replies can no longer stall the frame loop. The v1.6.1 streaming fix made model answers arrive — and much richer ones, since reasoning models finally get to finish thinking — but the moment a reply landed, the background thread ran the whole serving pass (stat attachment, build-code encoding, rotation simulation, chip building) while holding the shared state mutex. ImGui draws only on the render thread, and the render callback shares that mutex, so a slow serving pass read to Windows as the entire game not responding. Every step of the serving pass now runs without the lock: the state mutex is taken once for a microsecond read (live game DB and game mode), released for the heavy work, and taken again only to append the finished reply and suggestion. A Clear pressed while a reply is still being prepared now correctly drops the result instead of resurrecting it.

The Gemini, OpenAI, and Anthropic providers are unchanged in this release.

## 1.6.1 - 2026-08-26

Choya stopped going silent. Every OpenRouter request — chat, Optimize, and Improve — now streams its answer instead of waiting minutes for a single buffered payload, reasoning models get a dedicated thinking budget that cannot starve the actual reply, and transient gateway failures retry with backoff instead of killing the turn. If a model appeared to "stop responding entirely" — the request eventually failing with *Request timed out. Try a larger/faster model.* — that was this bug, and it is fixed.

### Why requests timed out

- The OpenRouter client sent one non-streaming `POST` per request and enforced a hard 180-second wall clock. Nothing arrived until the model finished the entire generation, so the connection sat completely idle while the model worked.
- Reasoning models such as `z-ai/glm-5.3-flash` spend minutes on hidden thinking before their first output byte — measured at 220 seconds on a real request. OpenRouter's own gateway also aborts non-streaming requests whose provider does not answer in time, returning 408/504. Either way the request died as a false timeout on exactly the models that think the most, while quick non-reasoning models kept working — which is why only *some* models broke.

### Streamed replies

- All chat completions now use `stream: true`. OpenRouter interleaves `: OPENROUTER PROCESSING` keep-alive comments so the connection never idles, and the first bytes land in seconds (2.4 s measured, down from 220 s to anything at all).
- The SSE parser follows OpenRouter's streaming contract: keep-alive comments and blank lines are skipped, content and tool-call deltas accumulate (parallel calls merged by index, fragmented JSON arguments stitched back together), `[DONE]` ends the stream, and the usage-bearing final chunk is tolerated.
- Mid-stream failures arrive inside a 200 response, so they are now recognized in-band: a top-level `error` payload or `finish_reason: "error"` maps to the same typed messages as before (rate limited, billing, timeout, overloaded) instead of surfacing as a confusing empty reply.

### Answers no longer starved by thinking

- Reasoning models share one completion budget between hidden thinking and the visible answer. The old flat `max_tokens: 8192` let thinking consume the entire budget and return an empty message with `finish_reason: length` — reproduced live with 0 characters of content.
- Requests now cap hidden reasoning at 8,192 tokens (`reasoning.max_tokens`) inside a 16,384-token completion ceiling, so a long thinking phase leaves room for the build JSON and its explanation. Providers without thinking support ignore the parameter.

### Smarter retries and timeouts

- Gateway timeouts are now retryable: 408, 504, and 529 join the existing 500/502/503 retry list. Previously a single OpenRouter 408 killed the turn immediately with no retry at all.
- `Retry-After` is honored when OpenRouter sends one (capped at 60 seconds) instead of always guessing 5/10-second backoff.
- The connection budget grew from a 180-second whole-request kill to 900 seconds total with a 15-second connect timeout — a stalled request still fails, but a model that legitimately thinks for minutes now finishes.

### Tool-call routing

- Any request that carries function-calling tools now sets `provider.require_parameters: true`, so OpenRouter only routes to endpoints that implement tools natively — never to one that would fake them through a prompt template and return unparseable pseudo tool calls.

### Tests

- Five new unit tests cover the SSE reader: keep-alive skipping, content accumulation, fragmented parallel tool-call merging, mid-stream error mapping, and empty-stream finish-reason reporting; the existing tool-call round-trip test now exercises the streaming path.
- OpenRouter joined the live provider suite (`test_openrouter_validate_and_generate`): key validation, streamed generation, response caching, and a real streamed tool loop run against the production API. Full workspace suite: 785 passing.
- The Gemini, OpenAI, and Anthropic providers are unchanged in this release.

## 1.6.0 - 2026-08-25

New About tab. What's new shows the release notes for the last five versions in game. Message developer is a short guided form for a bug, a wrong build, a suggestion, a question, or a fistbump for Choya; each message shows its status (received, read, answered, closed) and the reply inline, refreshed on tab open and every five minutes, and the About pill pulses when an answer lands. Failed sends are kept locally with Resend, and nothing typed is lost. Ko-fi link in the header and on the first form step. Privacy: messages carry the category, choices, text, addon version, game build, language, mode/scale/role, profession and elite spec, and the AI provider name; a contact line, the account name, and a slim copy of the last optimize result are opt-in; API keys and character names are never sent.

## 1.5.3 - 2026-08-24

The rotation scheduler now values Fury by game mode. It priced every Fury application at the PvE +25% crit bonus, so Fury-granting skills were overvalued by a quarter when ordering PvP and WvW rotations; they now use the +20% those modes actually grant. A dropped  attribute in the i18n suite is restored, so named-placeholder substitution is verified again.

## 1.5.2 - 2026-08-24

A locked specialization now returns the best lock-respecting candidate even when no build passes every viability gate. The result stays on the locked spec and is marked provisional with the failed requirements instead of erroring out or swapping to another elite.

## 1.5.1 - 2026-08-23

Empty `gear_groups` now inherit the build prefix for armor, trinkets, and weapons, and every sheet path counts only the active land weapon set. Legacy saves with blank groups fill from `stat_prefix` on load. Empty inherited Strong and explicit all-Strong both lock at Power 2556 / Precision 2186 for the Ranger axe/axe fixture.

## 1.5.0 - 2026-08-23

This release moves WvW optimization away from isolated attribute totals and toward a mode-aware, two-sided exchange timeline. It also carries forward the v1.4.18 character-sheet corrections: passive attributes remain values a player can reproduce in Guild Wars 2, while temporary effects, modeled output, and incomplete mechanics are shown separately and labeled according to data quality.

### Reproducible character-sheet values

- The primary Stats comparison remains grounded in the level-80 Hero-panel attributes, profession health tier, armor-weight defense, equipped gear, active land weapon set, runes, and permanent sourced adjustments.
- Tooltip coefficients such as barrier amounts, direct heals, and life-siphon values are rejected by the shared permanent-stat classifier instead of inflating Power or Healing Power.
- Character-sheet calculation, optimizer scoring, synergy extraction, and model-facing build data use the same classification rules so a rejected tooltip amount cannot continue influencing search through another parser.
- PvE and competitive variants of duplicated attribute and percentage facts are resolved by the selected game mode instead of being stacked together.
- Synthetic evaluation values remain separate from visible attributes. The optimizer does not present internal scoring aids as numbers a player should expect to see in the Hero panel.

### Mode-aware balance and timing data

- Added an active 2026-07-15 balance manifest and patch ledger with explicit PvE, PvP, and WvW override files. The earlier 2026-01-13 manifest is retained as the inherited, superseded baseline.
- Rotation construction now receives the selected game mode. Activation time, recharge, resource cost, status duration, and combo-field duration can therefore resolve to different sourced values in PvE, PvP, and WvW.
- The first exact timing slice covers Black Powder, Heartseeker, and Steal, including competitive initiative and coefficient splits where the published values differ.
- Unsupported or ambiguous facts are not silently promoted to exact mechanics. They remain omitted from exact calculations or are surfaced as `Provisional` while source coverage is expanded.

### WvW exchange timeline

- WvW candidates are evaluated on a two-sided timeline with committed casts, incoming actions, interrupts, control, boon removal, defensive layers, recovery, and an exit instead of being ranked primarily by average dummy output.
- Secured sequences may be created by one applicable layer: control ownership, timed Stability, evade, block, invulnerability, or stealth. The evaluator does not require control and defensive cover simultaneously.
- Aegis and Blind are charge-based safeguards. Their mere presence does not create a multi-second protected interval; they preserve only the action or event they actually answer.
- Sequence completion now measures whether meaningful actions execute without interruption during secured slices. A generic mobility label no longer invents timed evade or stealth coverage.
- Enemy actions respect control state, and pending player actions can be interrupted. This makes ordering, short overlaps, and recovery materially affect the report.
- The WvW report exposes sequence completion, protected pressure, target threshold progress, tempo, sustain margin, exit availability, resource legality, and unmodeled-effect counts for ranking and diagnostics.

### Profession mechanics and resource legality

- Profession/F-slot skills can participate in the candidate and timeline instead of limiting evaluation to heal, utility, elite, and weapon actions.
- Added a bounded resource ledger for Initiative, Adrenaline, Energy, Illusions, and Blades. Higher-priority actions are skipped when their known cost cannot be paid, costs are spent at cast start, landed-hit gains respect caps, and over-cap gains are discarded.
- Resource accounting is intentionally a legality guard rather than a complete profession simulator. Attunements, legends, shroud, heat, life force, pets, kits, and other full state machines are outside this release.
- Weapon swaps use the candidate's actual weapon sets and sigils in search identity, while the passive character sheet continues to count only the active land set.

### Search, weighting, and grouped equipment

- WvW search ranking now reads the exchange report instead of leading with legacy dummy DPS.
- Role selection and fine-tune weights are propagated into candidate ranking so Condition, Control, Sustain, support, and direct-pressure preferences can influence which legal build is retained.
- Candidate identity includes weapon sets, sigils, elite specialization, and profession actions, preventing materially different loadouts from being discarded as duplicates.
- Mixed equipment is represented in grouped armor, trinket, and weapon prefixes. Search reachability was corrected so user-weighted alternatives can enter and survive the beam instead of repeatedly collapsing toward one direct-pressure prefix.
- Gate-repair neighbors remain available for candidates missing required defenses or utility, but passing viability does not override the user's selected role and weights.

### Corrected interaction rules

- Stability on the target prevents applicable control until it is removed; ordered removal can therefore change whether the following control action lands.
- Blind and Aegis no longer answer condition application, and their treatment is separated from evade and invulnerability when evaluating incoming actions.
- Resistance and condition application use their own timeline rules rather than borrowing strike-avoidance behavior.
- Incoming condition ticks use the shared condition-damage calculation instead of a max-health percentage stub.
- Might uses the mode-aware boon table rather than a fixed invented attribute increase.
- Combo fields carry explicit durations. Smoke, water, light, and fire finishers use the supported result for their finisher type; unsupported combinations are marked unmodeled instead of receiving a convenient fallback effect.
- Stealth is timed cover for relevant execution windows, not blanket immunity, and a mobility classification alone no longer grants a hardcoded duration.

### UI, reproducibility, and data quality

- Optimized gear display, comparison rows, save/load presentation, and generated build details preserve grouped-prefix and active-set choices so the shown result can be reconstructed instead of appearing as an unexplained aggregate.
- Mode-aware rotation previews use the visible candidate stats and selected mode rather than silently falling back to a generic PvE simulation.
- Build reports distinguish verified source-backed facts from provisional modeled behavior and count unmodeled sources for diagnosis.
- Exact passive attributes, simulated rotation output, boons, conditions, control, cleanses, and defensive utility remain separate concepts in the UI.

### Runtime and cancellation

- Long optimization work now observes cancellation and runtime limits throughout candidate generation and evaluation instead of only between coarse phases.
- Mode-aware evaluation is reused by search and addon previews, reducing disagreement between the candidate that was ranked and the result shown to the player.

### Validation and current boundaries

- Regression coverage includes the v1.4.18 Ranger character-sheet values, tooltip-stat rejection, competitive percentage classification, mode-isolated timing overrides, charge-versus-duration cover, interruption, ordered removal and control, resource paywalls, grouped-gear search identity, and user-weight reachability.
- The active data slice is not a claim that every Guild Wars 2 trait, skill, rune, sigil, relic, or profession mechanic is now exact. Exact competitive coverage is being expanded incrementally from authoritative sources.
- Timings and effects without a supported mode-specific fact remain `Provisional` or unmodeled. The optimizer will prefer an explicit gap over inventing a duration, coefficient, or interaction.
- Rotation results are deterministic model output, not a live combat record. Player positioning, facing, movement, opponent decisions, and profession state machines not listed above still require further modeling.
- Conditional damage thresholds are represented in the sourced balance layer, but complete conditional execution coverage continues to be expanded. A stored source value is not treated as active unless the timeline can justify its condition.

## 1.4.18 - 2026-08-23

This release replaces several optimistic stat assumptions with sourced, mode-aware character-sheet rules. Its first priority is trust: values shown as attributes should be values a player can reproduce in Guild Wars 2, while modeled rotation output is identified separately.

### Player-visible stat presentation

- The Stats pane now leads with the nine level-80 Hero-panel attributes: Power, Precision, Toughness, Vitality, Condition Damage, Expertise, Concentration, Ferocity, and Healing Power.
- Health and Armor remain visible derived Hero-panel values.
- Removed synthetic `Effective Power`, `Effective HP`, and `Healing Index` from the primary comparison. Those internal scoring aids are not character-sheet attributes and should not look like in-game values.
- Removed the synthetic three-scenario damage table from the primary attribute comparison. Rotation output remains labeled as a simulation rather than a live combat record.
- Boons, conditions, cleanses, stability, skill use, and viability remain separated from passive attributes so temporary effects are not presented as permanent gear stats.

### Permanent attribute corrections

- Tooltip coefficients no longer become permanent attributes. This fixes values such as barrier amounts, direct healing amounts, life-siphon damage, and life-siphon healing being added to Power or Healing Power.
- The same permanent-stat rule is shared by character-sheet calculation, synergy extraction, and the optimizer's LLM tool data. A tooltip amount can no longer disappear from the UI while still biasing candidate selection through another parser.
- Conditional named adjustments such as `Additional Power` are excluded from the unbuffed panel. They belong to timed activation modeling.
- Trait `BuffConversion` facts remain supported. For example, Wellspring's Power-to-Healing-Power conversion still changes the visible passive result.

### Game-mode corrections

- Known duplicated `AttributeAdjust` rows are resolved by trait, target attribute, and selected game mode rather than being summed.
- Lingering Magic now contributes 240 Concentration in PvE and 120 in PvP/WvW, instead of adding both API rows.
- Known conditional or pet-only duplicated rows are excluded from the standing player panel.
- Unknown duplicated rows are omitted instead of guessed. Missing data is safer than a confidently inflated attribute.
- Percentage tooltip facts now share one classifier between combat and synergy parsing.
- Tooltip-only 100% critical-chance facts, recharge reductions, incoming-damage reductions, and other defensive percentages no longer become outgoing damage multipliers.
- Two-value percentage pairs within one trait collapse to one mode value instead of stacking simultaneously. Conditional timing remains provisional and is handled separately from the passive panel.

### Equipment corrections

- Only the active land weapon set's two sigils contribute to the standing stat/modifier calculation. The second set is reserved for weapon-swap timeline handling.
- Rune tier bonuses continue to be summed from all six equipped tiers.
- Level-80 base attributes remain 1,000 for Power, Precision, Toughness, and Vitality, and 0 for the five secondary attributes.

### Profession and armor baselines

The optimizer uses separate sourced profession health and ascended-armor defense values:

| Baseline | Value | Professions |
| --- | ---: | --- |
| High base health | 9,212 | Warrior, Necromancer |
| Medium base health | 5,922 | Revenant, Engineer, Ranger, Mesmer |
| Low base health | 1,645 | Guardian, Thief, Elementalist |
| Heavy armor defense | 1,271 | Warrior, Guardian, Revenant |
| Medium armor defense | 1,118 | Engineer, Ranger, Thief |
| Light armor defense | 967 | Elementalist, Mesmer, Necromancer |

Visible Health is `profession base health + 10 × Vitality`. Visible Armor is `armor-set defense + Toughness`. Armor weight does not change the attribute budget of an otherwise identical armor prefix.

Sources: [Attribute](https://wiki.guildwars2.com/wiki/Attribute), [Health](https://wiki.guildwars2.com/wiki/Health), and [Armor](https://wiki.guildwars2.com/wiki/Armor).

### Reproduced failure and corrected result

The original Ranger case reached 7,156 Power and 13,696 Healing Power because effect coefficients were treated as passive stats. The exact offending tooltip amounts were removed from every permanent-stat consumer.

The cached WvW reproduction now resolves the tested optimized candidate to:

| Attribute | Value | Exact source |
| --- | ---: | --- |
| Power | 3,058 | level-80 base + Strong ascended gear + six Rune of Infiltration tiers |
| Precision | 2,544 | level-80 base + Strong ascended gear + six Rune of Infiltration tiers |
| Toughness | 1,000 | level-80 base |
| Vitality | 1,240 | level-80 base + Natural Fortitude |
| Concentration | 120 | Lingering Magic, WvW value |
| Healing Power | 214 | Wellspring: 7% of 3,058 Power, rounded for display |

The Rune of Infiltration contribution is exactly +175 Power and +225 Precision from its six API bonus lines.

### Validation

- All optimizer tests pass: 735 passed, 1 network test ignored.
- All 39 math permutation tests pass.
- All 29 objective-profile integration tests pass.
- Addon compilation passes.
- Clippy completes successfully; existing repository warnings remain non-blocking.

### Known boundaries

- Rotation output is still a model, not a live combat log. It remains labeled as simulated.
- Conditional trait and upgrade effects require timeline activation rules before their contribution can be called exact.
- A shield's separate defense rating is not yet added to the optimized Hero-panel Armor value. The armor-weight baseline itself is exact.
- Food, utility consumables, temporary boons, and triggered upgrade effects are intentionally not folded into the unbuffed attribute table.
