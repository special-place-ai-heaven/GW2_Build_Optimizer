# Data model: Choya readable chat

## Reply markup (parsed from text, never stored)

`Block` = `Paragraph(Vec<Span>)` | `Bullet { depth, spans }` | `Numbered { n, spans }` | `Rotation(Vec<Vec<Span>>)` | `Warning(Vec<Span>)` | `Blank`.

`Span { text: String, style: SpanStyle }`, `SpanStyle = Plain | Bold | Italic | Name | Warn | Link(url)`.

Parsing rules, in order per line: `- `/`* `/`• ` → Bullet (two leading spaces per depth); `N. ` → Numbered; leading `!` or `⚠` → Warning; a line whose tokens are joined by `->` or `→` with two or more parts → Rotation; else Paragraph. Inside a line: `**x**` → Bold, `_x_` or `*x*` → Italic, `http(s)://…` → Link, longest-match against the game name index → Name (unless already Bold or Link). Anything unparsed stays Plain; nothing is dropped.

Layout: `wrap_spans(ui, blocks, max_w) -> Vec<Line>` where `Line { spans: Vec<(Span, x)>, height }`; a span that does not fit is split at word boundaries, never mid-word unless longer than the width. Bubble size is the sum of line heights plus padding; there is no length cap.

## ChatMessage (extended)

Existing: `from_user`, `text`, `chips`, `open_result`, `build_failed`.

| New field | Type | Meaning |
|---|---|---|
| `stopped` | `bool` | the reply is partial output kept after Stop or a failure; shows the Retry button |
| `retry_of` | `Option<String>` | the user message to re-send on Retry |

`text` holds the whole reply (FR-001).

## LiveOutput (in-flight, `state.main.chat_live: Arc<Mutex<LiveOutput>>`)

| Field | Type | Meaning |
|---|---|---|
| `step` | `Step` | `Handshake` \| `Reference` \| `Lookup(n)` \| `Scoring` \| `Writing` \| `Fallback` |
| `step_started` | `Instant` | for the elapsed seconds |
| `last_activity` | `Instant` | last byte or tool event; stall = now − last_activity ≥ 20 s |
| `reasoning` | `String` | appended reasoning deltas |
| `content` | `String` | appended answer deltas |
| `tools` | `Vec<String>` | "round 2: score_build, list_sigils" lines |
| `mode` | `LiveMode` | `Reasoning` \| `Content` \| `ToolsOnly` \| `AtOnce` (provider neither streams nor reasons) |
| `expanded` | `bool` | player toggled the bubble |

Reset on every send; read by the bubble each frame; frozen into the reply on Stop.

## RequestKind (pure)

`Build { elite: Option<String> }` | `AboutPlate` | `Chat`, from `classify(message, has_plate)`. Drives the fallback only; the model still receives every message unchanged.

## TabKind (derived per tab)

`Current` | `Optimized` | `Published { site }`. Colour: `theme::CURRENT`, `theme::OPTIMIZED`, `site_colour(site)`. Fill/rim/selected derived from the palette as in research R2; a test pins minimum contrast over every preset.

## Provider picks key

`"{profession}|{mode}|{role}|{specs}"` built from the newest suggestion with empty `source_url`; unchanged by selection, by follow-up replies, or by published tabs. Cards render under the newest message with `open_result == true`.

## Log line

`Choya {step}: {outcome} in {secs:.1}s` with `outcome` one of `ok`, `no answer ({error})`, `stopped`, `tools: a, b`, `fallback {kind}`. One per step, `nexus::log` at Info.
