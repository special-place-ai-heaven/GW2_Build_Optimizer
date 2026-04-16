# addon-ui

Nexus DLL + ImGui UI — owns the "amazing interface despite ImGui/Nexus constraints" mandate.

## Agent Toolbelt (required)

- **Use SymForge MCP tools for all code work in this tentacle.** Especially
  critical here because `main_view/mod.rs` is the single largest file in
  the repo (3225 lines, 37 top-level symbols). Prefer
  `get_file_context` + `get_symbol` over `get_file_content` for that file
  — whole-file reads cost ~31K tokens. `search_symbols` /
  `find_references` before every refactor; `edit_within_symbol` for
  surgical edits; `analyze_file_impact` after each change; `what_changed`
  on resume. Raw `Read`/`Grep`/`Glob` burns 70–95% more tokens.
- **Optional: Obsidian vault** for product/UX notes if present.

## Scope

- `crates/addon/src/lib.rs` — Nexus entry (`on_load`, `on_unload`)
- `crates/addon/src/state.rs` — `Mutex<Option<AddonState>>` global,
  `MainState`, `Screen`/`MainTab`/`SetupStep` enums, `CancellationToken`,
  `with_state(|s| ...)`, `init`, `toggle_window`, `clear`
- `crates/addon/src/ui/mod.rs` — `render` dispatcher
- `crates/addon/src/ui/setup.rs` — 4-step setup flow (GW2 key → LLM key
  → download → complete)
- `crates/addon/src/ui/chat_bar.rs` — chat bar for LLM refinement
- `crates/addon/src/ui/comparison.rs` — `ComparisonState`,
  `BuildSuggestion` grid
- `crates/addon/src/ui/gear_diff.rs` — `BuildDiff` before/after view
- `crates/addon/src/ui/radar_chart.rs` — 6-axis radar visualization
- `crates/addon/src/ui/main_view/` — 7-file subtree (3 files > 500
  lines): `mod.rs` (render dispatch + 7 sub-tabs), `build_display.rs`,
  `character.rs`, `lock_panel.rs` (GW2-style hexagon + 3×3 trait grid),
  `optimization.rs`, `resolution.rs`, `stats.rs`
- `.github/workflows/ci.yml` — CI wiring (belongs here because tests
  run `--test-threads=1` specifically for this crate's global `STATE`)
- `.claude/skills/release/SKILL.md` — local DLL-build + copy-to-addons
  release procedure

## Mission Link

The second headline goal: **an exceptional graphical interface despite
ImGui/Nexus constraints**. ImGui is immediate-mode, no layout engine, no
retained widgets, no styling system. Nexus provides a context and a
render hook — everything else is hand-drawn. The UX target is still
delightful: GW2-style lock hexagons, radar chart, gear-diff view,
inline chat refinement. This tentacle owns that bar.

## Key Decisions

- **Global state via `Mutex<Option<AddonState>>` + `with_state(|s| ...)`
  closure helper.** Never acquire the lock directly — always go through
  `with_state`. (`state.rs:324-329`.) Enforced by `refactor` skill.
- **Screen routing via `Screen` enum + `MainTab` sub-tab enum.**
  `ui::render` dispatches on `Screen`; `main_view::render_main` dispatches
  on `MainTab`. Adding a screen = variant + match arm in two places.
- **Background work pattern**: `std::thread::spawn` + clone a
  `CancellationToken` + use `with_state(|s| ...)` callback. No channels.
  Every spawned thread checks `token.is_cancelled()` at loop boundaries.
- **Panic recovery.** The optimization bg thread wraps the engine call
  in `catch_unwind` — prevents mutex poisoning on panic. Tests
  `test_catch_unwind_*` at `state.rs:752-900` enforce this contract.
  Removing `catch_unwind` is a regression.
- **Borrow-conflict pattern**: clone owned values *before* taking a
  mutable borrow on `AddonState`. New code fighting the borrow checker
  probably needs this.
- **UTF-8 truncation**: `text.chars().take(N).collect::<String>()`, never
  `&text[..N]` (panics on multibyte — player names, non-English skill
  descriptions).
- **Cache-first character loading**: JSON cache hit renders instantly,
  then background-refresh from API. `load_characters` /
  `load_character_tabs`. `ApiStatus` (Online/Degraded/Offline) refreshed
  via `/v2/build` ping every ~60s.
- **Setup-complete predicate** = `AppConfig::is_setup_complete()`.
  `init` routes to the right `SetupStep` or straight to `Screen::Main`
  based on it. (`state.rs:251-297`.)
- **Addon crate tests run `--test-threads=1`** — global `STATE` mutex
  deadlocks parallel tests. Encoded in `.github/workflows/ci.yml`.
  Other crates still run in parallel.
- **Release procedure**: `cargo build --release` → copy
  `target/release/gw2_build_optimizer.dll` to
  `C:\GAMES\Guild Wars 2\addons\`. No remote registry, no CI publish.
  (`.claude/skills/release/SKILL.md`.)

## Conventions

- **ImGui via nexus-rs**: `Window::new().build(ui, || { ... })` pattern.
  No retained state outside `MainState` / transient locals.
- **Styling constants at top of `main_view/mod.rs`**: `HEADER_BG`,
  `ACCENT_COLOR`. Keep theme tokens centralized.
- **Async work surfaces to UI through `MainState` fields**, not through
  callbacks or futures. The `*_loading: bool` flag pattern is standard
  (`characters_loading`, `models_loading`, `chat_waiting`).
- **Progress reporting** goes through `OptimizeProgress` / `DownloadProgress`
  structs read from `MainState`.
- **Setup-step functions live in `ui/setup.rs`**: `render_gw2_key_step`,
  `render_llm_key_step`, `render_download_step`, `render_complete_step`.
  Each step is ~150 lines. Adding a step means extending `SetupStep`
  enum and `render_setup` dispatch.
- **Tests use `TEST_STATE_LOCK` + `state_test_guard`** to serialize
  global-state access (`state.rs:340-358`). New tests that touch `STATE`
  must follow this pattern.

## Cross-Tentacle Contracts

- **core-domain**: consumes `AppConfig`, `ResolvedBuild`, `CombatMetrics`,
  `BuildLocks`, `SavedBuild`.
- **gw2-api-client**: drives `download_all`, fetches characters /
  build-tabs / equipment-tabs, reads `DownloadProgress`.
- **optimizer-engine**: spawns bg thread calling
  `optimize` / `optimize_deterministic` / `optimize_with_gemini` and
  reads `CombatMetrics` / `RotationBreakdown` / `BuildCandidate` back.
- **llm-integration**: chat bar posts to `LlmClient::generate_with_tools_progress`;
  key validation on the Settings tab calls `validate_key_detailed`;
  Settings model picker calls `list_models`.
- **patch-aware-data**: displays "current patch" (manifest date) and
  evidence levels on the Settings tab.

## Hotspots / Known Debt

- `main_view/mod.rs` is 3225 lines, known decomposition target (story
  P1-03). Split sub-tabs into their own files; keep `render_main` as
  the dispatcher.
- Hardstuck scraper UI sits in `render_settings_tab`. It's the only
  external-source integration besides GW2 API — if it grows, consider
  lifting it into `gw2-api-client` or a new `sources` tentacle.

## Non-Goals

- **No optimization logic.** Engine calls go through `optimizer-engine`.
- **No direct HTTP.** All network I/O goes through `gw2-api-client` or
  `llm-integration`.
- **No game data.** The `patch-aware-data` tentacle owns that.

<!-- octogent:suggested-skills:start -->
## Suggested Skills

You can use these skills if you need to.

- `code-review`
- `refactor`
- `release`
<!-- octogent:suggested-skills:end -->
