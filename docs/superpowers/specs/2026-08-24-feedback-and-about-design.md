# About tab, in-game changelog, and Message Developer — design

Date: 2026-08-24
Status: draft for review
Ships as: v1.6.0 (new tab, new network path, new server)

## 1. Why

Nexus shows players a version number and a "restart to update" toast, nothing
else. Players cannot read what changed and cannot tell the developer anything
without leaving the game. This adds one tab that does both, and a small server
that receives what players send.

The product is free. Donations are welcome and never solicited beyond a link.

## 2. Scope

In:

- New `About` tab: Choya header, changelog, Message Developer wizard, list of
  sent messages with server-side status, Ko-fi link.
- Data-driven category taxonomy (embedded, server-refreshable).
- `feedback` server: one Docker container on `srv1640039` behind Traefik at
  `feedback.robagentic.tech`, PostgreSQL 16 storage, admin reply endpoint.
- Local message history in the addon dir.

Out (explicitly, for now):

- Screenshots or file attachments.
- Player accounts, login, or any identity beyond an opt-in account name.
- Public voting / roadmap. (Taxonomy `type` leaves room for it.)
- Admin web UI beyond a token-gated JSON API. (Reply from a curl/script first;
  a page can come later without touching the addon.)
- Any donation prompt, counter, gate, or "supporter" flag. See §9.

## 3. Tab layout

Template is the Saves tab (`crates/addon/src/ui/main_view/tabs/saveload.rs`):
mascot hero, one action row, row-plate table. Same helpers, same colors.

```
 New Build   Improve Build   Choya   Saves   Settings   About
──────────────────────────────────────────────────────────────────────────
  [choya      CHOYA'S MAILBAG
   hero]      GW2 Build Optimizer · v1.6.0 · AI: Gemini
              Free to use. If it saved you gold, Choya takes coffee.
              3 sent · 1 answered

              [ Message developer ]   [ ☕ Buy Choya a coffee ]

  [ Messages ] [ What's new ]                       ← segment_row toggle

  (Messages view)
   ⌂  MESSAGE                                  SENT              STATUS       ACTIONS
  ┌──────────────────────────────────────────────────────────────────────────────┐
  │ 🐞  Optimize picks Trident on land          2026-08-24 18:41  ● Answered    View   │
  │     Bug · Optimize · Wrong result                                                   │
  │ 👊  Improve tab is great                    2026-08-23 21:02  ○ Received    View   │
  │     Fistbump · Optimizer                                                            │
  └──────────────────────────────────────────────────────────────────────────────┘

  (What's new view)
  ┌ scrollable child, wrapped, body at font_scale × 0.85 ─────────────────────────┐
  │ 1.5.3 · 2026-08-24                                                             │
  │   The rotation scheduler now values Fury by game mode. …                       │
  └────────────────────────────────────────────────────────────────────────────────┘
```

Decisions:

- Changelog and message list are a two-way `segment_row` toggle, each gets full
  height. Default view: What's new, unless a non-`local` message row exists;
  the last chosen segment is remembered for the session.
- Left panel matches Settings: **no character picker**, the Info block
  (product, version, provider) that `main_view/mod.rs:490` already renders.
  About is not a build tab.
- The About pill joins the Saves/Settings group on the right. Verify no wrap
  at `MIN_WINDOW_SIZE` (640 px) with `font_scale` 1.25 in `fr` and `pt`; if it
  wraps, the label shortens to the locale's short form (`tab.about_short`).
- Changelog source: `include_str!` of `CHANGELOG.md` at build time; split on
  `## ` headings; render the first 5 entries. Heading in `CREAM`, body in
  `MUTED` at 85 % font scale. Markdown marks are stripped (`###`, `**`, list
  dashes become `•`); the body is capped at 1200 chars per version with a
  trailing `…` — 1.5.0 is 70 lines of `###`/bullets and would read as noise
  raw. Text stays English.
- Row icons **and wizard chip icons** are draw-list glyphs (like the Choya
  mascots), colored with the category color — the overlay fonts have no color
  emoji, so the emoji in the mockups are notation, never UI. One exception: the coffee category and the header button
  use Ko-fi's official logomark (`crates/addon/assets/kofi_cup.png`, from
  storage.ko-fi.com/cdn/logomarkLogo.png; their creator kit permits use to
  link to your page), embedded with `include_bytes!` and loaded through
  `get_texture_or_create_from_memory` exactly like the mystic-coin icons.
- `View` expands the row inline: full body, attached context, and the reply if
  answered. `Resend` appears only on rows whose `status == failed`, in `WARN`.

## 4. Message Developer wizard

Guided steps, one question per screen, Choya asks. Built from
`select_chip` / `segment_row` / `gold_button_sized`. Opens inline below the
action row and pushes the table down; the header stays.

Step 1 is the same for everyone:

```
  [choya]  What's on your mind?

   🐞 Report a bug        ⚠ Wrong build       💡 Suggest something
   ❔ Ask a question      👊 Fistbump Choya    ☕ Buy Choya a coffee

   ( Same as last time: Bug › Optimize › Wrong result )   ← shown after first report
```

Then the steps listed in the taxonomy for that category. Every step: Choya
line on the left, choices or text on the right, `‹ Back` and `Step n of m`,
`Send` only on the last step and only when every required step has a value.
Send's tooltip names what is missing.

Last step of every `report` category is the summary card:

```
  🐞 Bug › Optimize › Wrong result
  "Optimize picks Trident on land …"

  Attached   v1.6.0 · game 174122 · en · WvW / Roam / Damage
             Druid → Untamed · Gemini
  [ ] include my last optimize result           (Wrong build only)
  [ ] include my GW2 account name  Name.1234    (default off)
  Reach me   [ optional — email or Discord ]

                                  [ Send to Choya ]   [ Cancel ]
```

After send: form collapses to a plate `Sent · #a3f9 · Choya has it`. On
failure: form stays, error in `ERR`, nothing typed is lost, row saved locally
as `failed` for Resend.

`link` categories (coffee) do not run steps: open the URL, add a local row
`☕ Sent Choya for coffee`, no server call.

`praise` runs one chip step (what you liked) and an optional line, then thanks
and stops. It never leads to the coffee tile.

## 5. Taxonomy (data, not code)

`data/feedback_taxonomy.json`, embedded with `include_str!` as fallback, and
refreshed from `GET /v1/taxonomy` when the tab opens (cached in the addon dir,
`taxonomy_version` compared). Adding a category or a step is a JSON change; a
new category reaches players without a DLL release.

```json
{
  "taxonomy_version": 1,
  "categories": [
    { "id": "bug",      "type": "report", "label": "cat.bug",      "icon": "bug",    "color": "red",
      "steps": ["area_screen", "severity", "describe"] },
    { "id": "wrong_build", "type": "report", "label": "cat.wrong_build", "icon": "broken", "color": "orange",
      "steps": ["area_screen", "severity", "describe"], "attach_build": true },
    { "id": "wish",     "type": "report", "label": "cat.wish",     "icon": "bulb",   "color": "green",
      "steps": ["area_feature", "describe"] },
    { "id": "question", "type": "report", "label": "cat.question", "icon": "question", "color": "blue",
      "steps": ["area_feature", "describe"] },
    { "id": "praise",   "type": "report", "label": "cat.fistbump", "icon": "fist",   "color": "gold",
      "steps": ["liked", "note_optional"] },
    { "id": "coffee",   "type": "link",   "label": "cat.coffee",   "icon": "kofi",   "color": "gold",
      "url": "https://ko-fi.com/specialplacerob" }
  ],
  "steps": {
    "area_screen":  { "prompt": "step.where",    "choices": ["optimize","improve","choya","saves","chat_code","settings","setup","data","other"] },
    "area_feature": { "prompt": "step.about",    "choices": ["optimizer","choya","ui","data","other"] },
    "severity":     { "prompt": "step.how_bad",  "choices": ["crash","blocks","wrong","cosmetic"] },
    "liked":        { "prompt": "step.liked",    "choices": ["optimizer","choya","design","speed","everything"] },
    "describe":     { "prompt": "step.describe", "text": { "min": 10, "max": 4000 } },
    "note_optional":{ "prompt": "step.note",     "text": { "min": 0,  "max": 1000 } }
  }
}
```

`label` and `prompt` are locale keys; choice ids map to `choice.<id>` keys.
Choya's line per step is `prompt` plus an optional `quip.<category>.<step>`.

`type` is the extension point: `report` runs steps and posts; `link` opens a
URL. A future `vote` or `discord` is one new match arm.

Icons are named glyphs drawn in `theme.rs` (`icon: "kofi"` maps to the
embedded texture instead); an unknown icon name falls back to a colored dot
so a taxonomy newer than the DLL still renders. An unknown `type` renders as
an inert, muted chip with a tooltip "Needs a newer addon" — never a panic.

The taxonomy in use is frozen for the life of an open draft; a fetched update
applies only when no wizard is open. The last successful `category` + `path`
is persisted in `messages.json` (`last_path`) for the "Same as last time"
shortcut.

## 6. Payload and schema

Addon → server, `POST /v1/reports`:

```json
{
  "schema_version": 1,
  "client_id": "uuid-v4 minted once per install, stored in config",
  "category": "bug",
  "path": ["optimize", "wrong"],
  "title": "first 120 chars of body, or step choice labels if body short",
  "body": "…",
  "contact": null,
  "account": null,
  "context": {
    "addon_version": "1.6.0", "game_build": 174122, "locale": "en",
    "mode": "WvW", "scale": "Roam", "role": "Damage",
    "profession": "Ranger", "elite": "Untamed", "llm_provider": "gemini"
  },
  "build_snapshot": null
}
```

`build_snapshot`, when the player ticks the box, is a **slim allowlist** built
from the last `BuildSuggestion`: `stat_prefix`, specs (ids + selected trait
ids), weapons (types + sigil ids), skills (ids), rune, relic, chat code. No
rotation, no combat profiles, no explanations, no `character_name`. Its
serialized size is capped at 6 KB client-side; over that, the box is disabled
with a tooltip. Total request stays under the 16 KB server cap even at 4000
CJK chars of body.

`account`, when ticked, comes from a `/v2/account` call the addon does not
make today — the checkbox performs that fetch on demand and **hides itself if
it fails**. `TokenInfo.name` (the API-key label) is never sent.

`path` holds choice ids only (`["optimize","wrong"]`); the category is its own
field. The UI renders "Bug › Optimize › Wrong result" from both.

Addon version travels in the `X-Addon-Version` header on every request; the
server compares it to `MIN_ADDON_VERSION` (semver, numeric compare) and
answers 426 when below.

Never sent: API keys, character names, account name unless the box is ticked.

Server responds `201 { "id": "a3f9…", "status": "received" }`.

PostgreSQL 16 (`migrations/0001_reports.sql`, applied by sqlx on start):

```sql
CREATE TABLE reports (
  id             BIGSERIAL PRIMARY KEY,
  short_id       TEXT NOT NULL UNIQUE,            -- 8-char base32 shown to the player
  report_id      UUID NOT NULL UNIQUE,            -- minted by the addon; idempotency key
  received_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  client_id      UUID NOT NULL,
  schema_version SMALLINT NOT NULL,
  category       TEXT NOT NULL,
  path           TEXT[] NOT NULL,
  title          TEXT NOT NULL,
  body           TEXT NOT NULL,
  contact        TEXT,
  account        TEXT,
  addon_version  TEXT NOT NULL,
  game_build     BIGINT,                          -- get_build_number() is u32
  status         TEXT NOT NULL DEFAULT 'received'
                 CHECK (status IN ('received','read','answered','closed')),
  reply          TEXT,
  replied_at     TIMESTAMPTZ,
  closing_note   TEXT,
  unvalidated    BOOLEAN NOT NULL DEFAULT false,  -- ids not in the served taxonomy
  payload        JSONB NOT NULL,                  -- full request as received
  ip_hash        TEXT NOT NULL                    -- sha256(ip + daily salt), rate limiting only
);
CREATE INDEX reports_status_idx  ON reports (status, received_at DESC);
CREATE INDEX reports_client_idx  ON reports (client_id, received_at DESC);
CREATE INDEX reports_payload_gin ON reports USING GIN (payload jsonb_path_ops);

CREATE TABLE taxonomy (
  version     INTEGER PRIMARY KEY,
  body        JSONB NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

`payload` as JSONB with a GIN index is the no-migration seam: a field the
addon starts sending next year is captured on day one and queryable
(`payload->'context'->>'elite'`) without a column. Promote to a column only
when it needs a constraint or a hot index.

Rate limits: `ip_hash` is the real limit. `client_id` is honor-system (an
attacker mints UUIDs) and its 50/day is best-effort; both live in process
memory and reset on restart, which is acceptable. Admin `GET` auto-flipping
`received` → `read` is a mutating read by design — a future admin UI must not
treat listing as side-effect-free. No delete and no retention policy in v1;
`contact` is stored plaintext; the README privacy paragraph says all three.

`payload` is the no-migration seam: a field added to the addon next year is
captured on day one and promoted to a column only when it needs indexing.

Local, addon dir `messages.json` (same pattern as saved builds):
`{ id, sent_at, category, path, title, body, status, reply, failed_payload? }`.

## 6a. Message lifecycle and statuses

Every state a player can see, what the row shows, and what they can do.

| State | Row shows | Player can |
|---|---|---|
| `draft` | form open, not a row; survives closing the tab | Cancel |
| `sending` | `◌ Sending…` pulsing; Send locked | nothing (prevents double-send) |
| `received` | `○ Received · #a3f9` | View |
| `read` | `○ Read` | View |
| `answered` | `● Answered` in GOLD, reply inline on View | View |
| `closed` | `○ Closed` MUTED, closing note if any | View |
| `failed` | `⚠ Not sent — <reason>` in WARN | Resend, Discard |
| `local` | link categories (coffee); never sent | — |
| `unknown` | `? No longer on server` | Discard |

Failure reasons are distinct plain messages, never a bare "error":

| Cause | Detection | Message | Resend |
|---|---|---|---|
| No network / DNS | connect error | Couldn't reach Choya. Check your connection. | yes |
| Server down | 500/502/503 | Choya's mailbox is down. Your message is saved — try again later. | yes |
| Timeout | > 5 s | Took too long. Saved — try again. | yes |
| Rate-limited | 429 + Retry-After | Slow down — try again in N min. Resend disabled with countdown | after countdown |
| Too large | 413 | Message too long (limit 4000). Form reopens with text | after edit |
| Rejected | 400 + reason | server reason verbatim | after edit |
| Addon too old | 426 | Update the addon to send messages. | no |

Idempotency: the addon mints `report_id` (UUID v4) when the draft is created;
every send and resend carries it; the server does
`INSERT … ON CONFLICT (report_id) DO UPDATE SET report_id = EXCLUDED.report_id
RETURNING short_id, status` (a no-op update so RETURNING yields the existing
row) and answers 201 with the original short id on replay. Send → timeout →
Resend yields exactly one row even if the first request arrived.

Resend replays `failed_payload` byte-for-byte. Editing a failed message is
Discard + new draft (new `report_id`); the wizard offers "Edit and resend" as
exactly that. A 400/413 never stored a row, so the same `report_id` after an
edit would be safe there — but the client does not special-case it; it always
mints anew on edit.

Status refresh: on tab open, after each send, and every 5 minutes while the
overlay is visible — one `GET /v1/reports/status?ids=…&client_id=…` for local
rows in `received`, `read`, or `answered` only (never `failed`, `local`,
`draft`, `sending`), on the standard bg thread. The server returns a row only
when both `short_id` and `client_id` match, and rate-limits GET per `ip_hash`
exactly like POST.

- Success → rows updated; "Updated just now" in MUTED under the table.
- Failure → rows keep last known status; header shows "Status as of 2h ago ·
  Choya unreachable" in WARN. A failed refresh never blanks a known status.
- Id missing from a **successful** 200 response → `unknown`. A failed refresh
  never produces `unknown`.
- On load, any row still in `sending` (crash mid-send) becomes `failed` with
  reason "Interrupted", so Resend is offered.

When a refresh flips a row to `answered`, the About pill pulses via the
existing `tab_alert` + `pill_pulse` mechanism (the same breathe New Build /
Improve / Choya use when a result lands) until the tab is opened. Because the
5-minute poll runs on any tab, the pulse can appear while the player is
elsewhere — that is the point. This is the only proactive signal in the
feature.

Server statuses use the same words: `received → read → answered → closed`.
`read` is set automatically when the report is fetched through the admin API.

Local rows add `report_id`, `last_error`, `retry_after`. (Server columns
`report_id` and `closing_note` are in the schema in §6.)

## 7. Server

Two containers in one compose on `srv1640039`, next to agentmemory:

- `feedback` — Rust + axum + sqlx (compile-time checked queries), migrations
  in `server/feedback/migrations/`, applied on start. Traefik labels for TLS
  at `feedback.robagentic.tech`. Only container on the public edge.
- `db` — `postgres:16-alpine`, bound to the compose network only (no host
  port), `pgdata` named volume, password from `.env`.

Backups: nightly `pg_dump -Fc` via a cron on the host into
`/docker/feedback/backups/`, 30-day retention. Restore is one `pg_restore`.

DNS: one A record `feedback` → same IP as `memory`.

Public:

- `POST /v1/reports` — 16 KB body cap, 10/min per `ip_hash`, 50/day per
  `client_id`. Validates category and choice ids against the current taxonomy;
  unknown ids are accepted and stored (older DLL vs newer taxonomy and vice
  versa must not lose reports), flagged `unvalidated`.
- `GET  /v1/reports/status?ids=a3f9,b7c2&client_id=<uuid>` —
  `[{id,status,reply,replied_at,closing_note}]` only where `short_id` **and**
  `client_id` match; no listing, no enumeration; same per-`ip_hash` limit as
  POST.
- `GET  /v1/taxonomy` — current taxonomy JSON with `taxonomy_version`.
- `GET  /healthz`.

Admin, `Authorization: Bearer <FEEDBACK_ADMIN_TOKEN>` (env, never in repo):

- `GET  /v1/admin/reports?status=received&limit=50`
- `POST /v1/admin/reports/:id/reply` `{ "reply": "…", "status": "answered" }`
- `POST /v1/admin/reports/:id/status` `{ "status": "read" }`
- `PUT  /v1/admin/taxonomy` — replaces the served taxonomy (bumps version).

Replying is what turns a player's row `Answered` in-game on next tab open.

Repo layout: `server/feedback/` with its own `Cargo.toml` (not a workspace
member — it must not be linked into the DLL), `Dockerfile`, `compose.yml`.
Deploy is `docker compose up -d` in `/docker/feedback/` on the VPS.

## 8. Addon side

- `crates/addon`: `MainTab::About` in `state.rs`; pill in the Saves/Settings
  group in `main_view/mod.rs`; new `tabs/about.rs` (tab),
  `tabs/about/wizard.rs` (step runner), `tabs/about/glyphs.rs` (icons) — split
  from the start so no one file carries tab + wizard + drawing.
- `open_url(&str)` helper over `ShellExecuteW`, no new crate.
- `crates/core`: `FeedbackTaxonomy`, `Report`, `LocalMessage` types; storage
  for `messages.json` and cached taxonomy.
- Network: existing `reqwest` blocking client, on a `std::thread::spawn` with
  `CancellationToken`, results back through `with_state`. Same pattern as
  every other background call in the addon.
- Ko-fi: `ShellExecuteW("open", url)`; Windows-only and the addon already is.
- Locale: every new UI string in all 12 files (`every_locale_parses_and_covers_english_keys`
  enforces it). Changelog body stays English.

## 9. Tone constraints (hard)

- No donation popup, reminder, counter, or timing rule. Ko-fi is reachable in
  exactly two places: the header button and the step-1 tile.
- No feature reads whether anyone donated. No supporter flag anywhere.
- One line of copy, in the header: "Free to use. If it saved you gold, Choya
  takes coffee."
- Fistbump thanks and stops. It never routes to coffee.

## 10. Errors

- Offline / server down: send fails fast (5 s timeout), row saved `failed`,
  Resend in `WARN`. Status refresh failure is silent; last known status stays.
- Taxonomy fetch failure: embedded copy is used; no message shown.
- Server rejects (400/413/429): error text under Send, form intact.
- Local `messages.json` unreadable: start empty, log a warning, do not delete.

## 11. Testing

Addon (unit, `cargo test`):

- Changelog splitter: 7 entries in → 5 out, version/date/body correct; CRLF
  and LF both.
- Taxonomy: embedded JSON parses; unknown `icon` and `type` do not panic;
  step-runner over a 3-step category yields Send only when all required
  steps are set; `min` text length enforced.
- Payload builder: never includes API keys or character names; `account` is
  `null` unless opted in; `build_snapshot` only for `attach_build` categories.
- `messages.json` round-trip; `failed` rows survive restart.

Server (unit + integration against a throwaway Postgres via `sqlx::test`):

- `POST` happy path → 201 and row; body over 16 KB → 413; 11th request in a
  minute from one ip_hash → 429; unknown category stored with `unvalidated`.
- `status` returns only requested ids; empty for ids not owned — no leak.
- Admin without token → 401; reply sets `answered` and is visible via `status`.

Live (Charlotte is not applicable — ImGui in-game): after deploy, one real
report from the running addon, visible via admin GET, replied to, and the row
turns Answered in-game. That is the acceptance test.

## 12. Rollout

1. Server: build image, deploy compose (feedback + db), DNS record, migrations
   applied, healthz green, admin token + DB password set in the VPS `.env`. Verify with curl before any addon work depends on it.
2. Addon: About tab + changelog only (no network) — merged to main, not
   released (no rc on `/releases/latest`).
3. Addon: wizard + send + status + local history → one release, 1.6.0.
4. README: About/feedback section, Ko-fi link, privacy paragraph (what is and
   is not sent).

## 13. Open items

- Ko-fi URL: https://ko-fi.com/specialplacerob (resolved).
- SSH to `srv1640039` currently denied from this machine for both users; deploy
  needs a working key or is run from the user's shell.
- Choya quips per step: write with the user; placeholders are locale keys only.

## 14. Review rulings (2026-08-24)

A spec review before implementation raised the items below; each was ruled
and folded into the sections above. Recorded here so the reasoning survives.

| # | Finding | Ruling |
|---|---|---|
| 1 | Gold dot can never fire if refresh only runs on tab open | **A**: poll every 5 min while overlay visible, non-final rows only; reuse `tab_alert`/`pill_pulse` (§6a) |
| 2 | Status GET had no ownership and no rate limit | `client_id` required, both must match; per-`ip_hash` limit as POST; poll only received/read/answered; `unknown` only after a 200 (§6a, §7) |
| 3 | `INSERT OR IGNORE` is SQLite | `ON CONFLICT DO UPDATE … RETURNING`, 201 on replay (§6a) |
| 4 | Resend + edit fights idempotency | Resend replays bytes; edit = Discard + new `report_id` (§6a) |
| 5 | `build_snapshot` too big, leaks names | Slim allowlist, 6 KB client cap, no `character_name` (§6) |
| 6 | Account name not available in client | On-demand `/v2/account`; checkbox hides on failure; never `TokenInfo.name` (§6) |
| 7 | About showed the character picker | Matches Settings: no picker (§3) |
| 8 | Emoji as UI | Glyphs for rows **and** chips; emoji in mockups are notation (§3, §5) |
| 9 | Coffee `local` row steals default view | Default What's new unless a non-`local` row; remember segment (§3) |
| 10 | Raw markdown changelog | Strip marks, 1200-char cap (§3) |
| 11 | Seventh pill may wrap at 640 px | Verify fr/pt at 1.25; `tab.about_short` fallback (§3) |
| 12 | 500-line file ceiling was false (`saveload.rs` is 1234) | Ceiling dropped; split still stands (§8) |
| 13 | Taxonomy swap under an open draft | Frozen per draft (§5) |
| 14 | 426 undefined | `X-Addon-Version` header vs `MIN_ADDON_VERSION` (§6) |
| 15 | Crash during `sending` sticks forever | Load maps `sending` → `failed` (§6a) |
| 16 | Unknown taxonomy `type` | Inert muted chip, no panic (§5) |
| 17 | `game_build` type | `BIGINT`, matches `u32` (§6) |
| 18 | rc release on `/releases/latest` | No rc; one 1.6.0 (§12) |
| 19 | Rate-limit honesty, admin GET mutates, retention | Written down (§6) |
| 20 | sqlx tests never run in CI | Own workflow with a Postgres service (plan Task 8) |
