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
  height. Default view: Messages if any exist, else What's new.
- Left panel shows the existing Info block (product, version, provider) that
  Settings already renders at `main_view/mod.rs:490`.
- Changelog source: `include_str!` of `CHANGELOG.md` at build time; split on
  `## ` headings; render the first 5 entries. Heading in `CREAM`, body in
  `MUTED` at 85 % font scale. Markdown is shown as-is, no renderer.
- Row icons are draw-list glyphs (like the Choya mascots), colored with the
  category color. No image assets.
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
    { "id": "coffee",   "type": "link",   "label": "cat.coffee",   "icon": "coffee", "color": "gold",
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

Icons are named glyphs drawn in `theme.rs`; an unknown icon name falls back to
a colored dot so a taxonomy newer than the DLL still renders.

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
  game_build     INTEGER,
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
every send and resend carries it; the server does `INSERT OR IGNORE` on it and
returns the existing short id. Send → timeout → Resend yields exactly one row
even if the first request arrived.

Status refresh: on tab open and after each send, one
`GET /v1/reports/status?ids=…` for all non-final local rows, on the standard
bg thread.

- Success → rows updated; "Updated just now" in MUTED under the table.
- Failure → rows keep last known status; header shows "Status as of 2h ago ·
  Choya unreachable" in WARN. A failed refresh never blanks a known status.
- Id missing from the response → `unknown`.

When a refresh flips a row to `answered`, the About pill shows a gold dot until
the tab is opened. This is the only proactive signal in the feature.

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
- `GET  /v1/reports/status?ids=a3f9,b7c2` — `[{id,status,reply,replied_at}]`
  for ids the caller sent; no listing, no enumeration.
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

- `MainTab::About` in `state.rs`; pill after Settings in `main_view/mod.rs`;
  new `tabs/about.rs` (tab), `tabs/about/wizard.rs` (step runner),
  `tabs/about/glyphs.rs` (icons) — split from the start; `saveload.rs` at 500
  lines is the ceiling to stay under per file.
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
2. Addon: About tab + changelog only (no network) — ship as 1.6.0-rc behind
   nothing; it's inert.
3. Addon: wizard + send + status + local history → 1.6.0.
4. README: About/feedback section, Ko-fi link, privacy paragraph (what is and
   is not sent).

## 13. Open items

- Ko-fi URL (`https://ko-fi.com/specialplacerob` in taxonomy and README).
- SSH to `srv1640039` currently denied from this machine for both users; deploy
  needs a working key or is run from the user's shell.
- Choya quips per step: write with the user; placeholders are locale keys only.
