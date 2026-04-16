# gw2-api-client

GW2 v2 API client — authoritative ground-truth feed for items, specs, traits, skills, characters, and account state.

## Agent Toolbelt (required)

- **Use SymForge MCP tools for all code work in this tentacle.** `health`
  once per session, then `search_symbols`, `search_text`, `get_file_context`,
  `get_symbol`, `find_references`, `edit_within_symbol`,
  `replace_symbol_body`, `analyze_file_impact`, `what_changed`. Raw
  `Read`/`Grep`/`Glob` on source burns 70–95% more tokens and skips the
  index.
- **Optional: Obsidian vault** — check for project notes before major work.

## Scope

- `crates/gw2api/src/client.rs` — `Gw2Client`, `TokenBucket`, `ApiError`, fetchers
- `crates/gw2api/src/cache.rs` — `DataCache` on-disk TTL cache + character cache
- `crates/gw2api/src/download.rs` — `download_all` orchestration + `DownloadProgress`
- `crates/gw2api/src/models/` — serde models: characters, facts, items,
  itemstats, legends, professions, pvp, skills, specs, traits
- `crates/gw2api/tests/live_download.rs` — `#[ignore]` live integration tests
- `crates/gw2api/Cargo.toml`

## Mission Link

Without the API there is no ground truth. Every resolved build, every `Fact`
the combat math reads, every item stat the scorer evaluates comes from here.
Silent API breakage = silently wrong builds.

## Key Decisions

- **Manual query-string construction for bulk `ids=` requests.**
  `reqwest::Client::query()` URL-encodes commas as `%2C` and breaks
  `ids=1,2,3`. Every helper that fetches by ID builds the query string by
  hand. See `Gw2Client::get_with_params` at `client.rs:111-238`.
- **Token bucket: 300 capacity, 5/sec refill.** Matches GW2's global rate
  limit. `MAX_RETRIES = 3`, `MAX_BULK_IDS = 200`. `client.rs:11-15`. Do not
  change without confirming server-side limits.
- **Bulk requests chunked at 200 IDs.** `fetch_by_ids` /
  `fetch_by_ids_with_progress` split automatically.
- **Lenient fact deserialization.** GW2 fact entries sometimes omit `type`.
  Use `filter_map(|v| from_value(v).ok())`, never
  `collect::<Result<_,_>>()` — one bad entry must not kill the whole skill.
  (Enforced by `code-review` skill.)
- **`DataCache` staleness keyed by build number.** Cache entries carry a
  build ID; a fresh `/v2/build` ping refreshes data only when the server
  build advanced. `is_stale` + `cached_build` at `cache.rs:60-88`.
- **Character data cached per-character.** `save_character` / `load_character`
  write `char_{sanitized_name}_*.json`. UI loads cache instantly, then
  background-refreshes.
- **`validate_api_key` returns `TokenInfo`** for permission introspection.
  Setup flow displays missing permissions.

## Conventions

- All public fetches return `Result<_, ApiError>`. `ApiError` wraps reqwest
  errors, non-2xx status with body, JSON parse errors, and rate-limit hits.
- Model types sit one-per-resource in `models/` and re-export from
  `models/mod.rs`. Keep `#[derive(Deserialize, Clone, Debug)]` minimal —
  add `Serialize` only when a caller needs round-trip.
- `Gw2Client::with_key` for authenticated endpoints; `without_key` for
  public ones (itemstats, specializations, traits, skills — bulk of
  resolution traffic).
- Live tests are `#[ignore]`-gated. CI does not run them. To run locally:
  `cargo test --workspace -- --include-ignored`. Gate on `GW2_API_KEY`.
- `DownloadProgress { step, total_steps = TOTAL_STEPS, label }` — UI reads
  this for the setup progress bar. Adding a step means bumping `TOTAL_STEPS`.

## Cross-Tentacle Contracts

- **core-domain** — no imports (leaf crate for HTTP).
- **optimizer-engine** consumes every model here via `GameDb::load`.
- **addon-ui** drives `download_all` during setup and displays
  `DownloadProgress`. Also invokes `fetch_characters` /
  `fetch_build_tabs` / `fetch_equipment_tabs`.

## Non-Goals

- No LLM / AI logic.
- No scraping. Wiki + Hardstuck data lives where it's used today (`addon-ui`
  settings, `optimizer` benchmark). If a scraper grows, either fold it in
  here as a `sources/` module or spin it out — do not quietly extend this
  crate's scope.

<!-- octogent:suggested-skills:start -->
## Suggested Skills

You can use these skills if you need to.

- `code-review`
- `refactor`
<!-- octogent:suggested-skills:end -->
