# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**GW2 Build Optimizer** — In-game Guild Wars 2 addon (Nexus plugin) that optimizes character builds across all game modes (PvE, PvP, WvW). Uses the GW2 API for game/character data and Google Gemini for LLM-powered build reasoning.

## Build & Development

```bash
# Check compilation (fast, no output)
cargo check

# Debug build
cargo build

# Release build (produces DLL)
cargo build --release
# Output: target/release/gw2_build_optimizer.dll

# Install DLL into GW2
# Copy gw2_build_optimizer.dll to <GW2 install>/addons/
```

<!-- AUTO-MANAGED: architecture -->
## Architecture

Rust workspace with 4 crates, compiles to a single DLL loaded by Nexus addon manager:

```
crates/addon/       — cdylib: Nexus entry point, ImGui UI, keybinds (nexus-rs)
  src/lib.rs        — export! macro with UpdateProvider::GitHub + update_link, on_load/on_unload, keybind + render registration; ui module is pub
  src/state.rs      — AddonState: screen (Screen/SetupStep state machine), setup (SetupState transient wizard data), main (MainState with GameDb, ComparisonState, ChatBarState, optimization state, saved_builds/saved_builds_loaded/save_name_input/save_status for Save/Load tab); MainTab enum includes SaveLoad variant; with_state() accessor; init() routes to correct SetupStep based on config
  src/ui/mod.rs     — render() dispatches on Screen::Setup vs Screen::Main; calls render_main() for main screen
  src/ui/setup.rs   — 4-step setup wizard: GW2 key validation, Gemini key validation, data download with ProgressBar, completion; background thread validation writes back via with_state()
  src/ui/main_view.rs — full main view: left menu (character picker, game mode radio, tab nav), render_new_build_tab() (archetype selector + comparison + chat bar), render_improve_tab(), render_saveload_tab() (lists/loads/deletes saved builds), render_settings_tab(); render_save_build_ui() shown below comparison when suggestions exist; start_optimization() → engine::optimize() → enrich_with_gemini() pipeline; start_optimization_with_profession() avoids borrow conflicts; load_game_db() bg thread; resolve_build(), candidate_to_suggestion(); apply_gemini_response() non-destructive merge onto BuildSuggestion; enrich_with_gemini() selects new_build_prompt/improve_build_prompt based on current_build presence; send_chat_message() → chat_refinement_prompt → GeminiClient → pushes "Chat Refinement" BuildSuggestion; summarize_resolved_build() produces text block for LLM context; infer_archetype_from_build() uses max() over 5 stat pairs
  src/ui/comparison.rs — BuildSuggestion + ComparisonState structs; render_comparison() with 2-column layout (current vs optimized), stat diff table, collapsing headers for "Why This Build?" and "Changes Made"; render_stat_diff() 4-col ImGui table with green/red/gray color coding by diff sign
  src/ui/chat_bar.rs — ChatBarState + render_chat_bar()
  src/ui/build_display.rs — render_build() for current build display
crates/core/        — Shared types (types.rs: ResolvedBuild, StatBlock, SavedBuild + all sub-types), config (config.rs), storage (storage.rs: BuildStorage — save/list/delete SavedBuild as JSON files in {addon_dir}/saves/; sanitize_filename() for safe filenames)
crates/gw2api/      — GW2 API v2 client + cache + download orchestration
  src/client.rs     — Gw2Client: token-bucket rate limiter, get/fetch_all/fetch_by_ids, validate_api_key
  src/cache.rs      — DataCache: JSON file cache keyed by build number, is_stale/save/load/clear_all
  src/download.rs   — download_all: 8-endpoint orchestration with DownloadProgress callback; fetch_equipment_items filters ~100k items to Exotic/Ascended/Legendary Armor/Weapon/Trinket/Back/UpgradeComponent/Relic
  src/models/       — serde structs per API endpoint (see GW2 API Models table below)
crates/optimizer/   — engine.rs (pipeline orchestration), gamedb.rs (GameDb: pre-indexed HashMap lookups for all game data, derived indexes), gemini.rs (GeminiClient: gemini-2.5-flash via generativelanguage.googleapis.com; new()/with_persistence() constructors — with_persistence() loads/saves usage to {addon_dir}/gemini_usage.json; validate_key/generate/generate_cached; RateTracker: 10 RPM hard limit, 240/250 RPD guard; maps 401/403→InvalidKey, 429→RateLimited; remaining_today() exposes quota to UI), prompts.rs (4 prompt builders: new_build_prompt, improve_build_prompt, chat_refinement_prompt with injection-guard, compare_builds_prompt; plus summarize_build, build_game_context; parse_build_response handles markdown fences + raw JSON; parse_gemini_build returns typed GeminiBuildResponse; GeminiBuildResponse struct mirrors BuildSuggestion fields), scoring.rs, search.rs, stats.rs
```

**Key dependency**: `nexus` crate from [nexus-rs](https://github.com/Zerthox/nexus-rs) — provides Nexus addon API bindings with ImGui (via `imgui-rs`), keybinds, events, logging.

### GW2 API Models (`crates/gw2api/src/models/`)

Each file maps to one API endpoint family. All structs derive `Debug, Clone, Serialize, Deserialize`.

| File | API Endpoint | Key Types |
|------|-------------|-----------|
| `characters.rs` | `/v2/characters` (auth) | `Character`, `BuildTab`, `Build`, `EquipmentTab`, `EquipmentPiece`, `EquipmentPvp` |
| `facts.rs` | shared | `Fact` (18-variant tagged enum + `Unknown` fallback), `TraitedFact`, `BuffPrefix` |
| `items.rs` | `/v2/items` | `Item`, `ItemDetails` (flat struct, all fields optional), `InfixUpgrade`, `InfixAttribute`, `InfixBuff`, `InfusionSlot` |
| `itemstats.rs` | `/v2/itemstats` | `ItemStat`, `StatAttribute` (multiplier + value) |
| `legends.rs` | `/v2/legends` | `Legend` (Revenant: swap/heal/elite/utilities) |
| `professions.rs` | `/v2/professions` | `Profession`, `WeaponInfo` (elite spec gate via `specialization` field) |
| `pvp.rs` | `/v2/pvp/amulets` | `PvpAmulet` (stat source in PvP mode) |
| `skills.rs` | `/v2/skills` | `Skill` (profession-specific fields: `cost`, `initiative`, `attunement`, `toolbelt_skill`) |
| `specs.rs` | `/v2/specializations` | `Specialization` (9 major traits = 3 columns × 3 choices) |
| `traits.rs` | `/v2/traits` | `Trait`, `TraitSkill` |

`Fact` variants used by both `Skill` and `Trait`: `Damage` (hit_count + dmg_multiplier), `Buff`, `AttributeAdjust`, `Recharge`, `BuffConversion`, `ComboField/Finisher`, and 12 others.
<!-- END AUTO-MANAGED -->

## GW2 Domain Context

See `~/.claude/projects/.../memory/gw2-domain.md` for full build system reference. Key points:
- 9 professions, each with 5 core + ~4 elite specializations
- 3 spec slots (slot 3 can be elite), 3 trait columns per spec (pick 1 of 3)
- Gear: 2 weapon sets + sigils, 6 armor + runes, 6 trinkets, 1 relic, buffs
- PvP uses amulet system instead of gear
- Stat formula: `attribute_adjustment * multiplier + value`
- GW2 API rate limit: 300 burst, 5/sec refill, max 200 IDs per bulk request

## Sprint Plan

Full finalization plan at `plan.md` in repo root. Sprint format: S##-T##.

| Sprint | Status | Focus |
|--------|--------|-------|
| S01 | DONE | Project scaffolding, minimal Nexus addon |
| S02 | DONE | GW2 API data models (serde structs) |
| S03 | DONE | API client, rate limiter, local cache |
| S04 | DONE | Setup wizard UI (API keys + data download) |
| S05 | DONE | Character loading & current build display |
| S06 | DONE | Stat calculation engine |
| S07 | DONE | Optimization engine |
| S08 | DONE | Gemini LLM integration (GeminiClient, prompts, rate limiting) |
| S09 | DONE | Comparison view & results UI |
| S10 | DONE | Build display, GameDb loading, optimizer wiring |
| S11 | DONE | Gemini→UI wiring (post-optimization enrichment + chat bar) |
| S12 | DONE | Robustness (mutex poison logging, rate persistence, thread guards) |
| S13 | DONE | Build persistence (Save/Load tab) |
| S14 | DONE | PvP/WvW game mode support |
| S15 | DONE | Polish & release prep |

<!-- AUTO-MANAGED: conventions -->
## Conventions

- Rust 2021 edition, workspace dependencies in root `Cargo.toml`
- `cdylib` crate type for addon DLL output
- nexus-rs macros: `export!`, `render!`, `keybind_handler!`
- Nexus addon signature: unique negative i32 derived from addon name (e.g. `"GW2B"` as hex, negated = `-0x47573242`)
- Global state via `Mutex<Option<T>>` static (Nexus runs single-threaded but callbacks need Send)
- ImGui windows via `Window::new().build(ui, || { ... })` with `Condition::FirstUseEver` for initial sizing
- Keybind IDs are SCREAMING_SNAKE_CASE strings (e.g. `"GW2_BUILD_OPT_TOGGLE"`)
- Background API validation: `std::thread::spawn` + `crate::state::with_state(|s| ...)` writes results back to global state — no channels needed
- UI screen routing via `AddonState.screen: Screen` — set directly in response to button presses or init logic
- Nexus `export!` macro supports auto-update distribution: include `provider: UpdateProvider::GitHub` and `update_link` for Nexus update metadata
<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: patterns -->
## Detected Patterns

- **Stub module pattern**: unimplemented files contain a single comment `// <purpose>.\n// Will be populated in S##.` — all stubs are now implemented; do not add placeholder code to stub files
- **Workspace dep hoisting**: all shared deps (serde, serde_json, reqwest, thiserror, urlencoding, chrono) declared once in root `[workspace.dependencies]`, crates reference with `.workspace = true`
- **State accessor pattern**: global state exposed via free functions (`init`, `toggle_window`, `is_window_visible`) rather than direct static access
- **Crate internal visibility**: `mod state; mod ui;` kept private in addon; `pub mod` used in library crates (core, gw2api, optimizer)
- **Tagged enum serde**: `#[serde(tag = "type")]` on `Fact` — API `"type"` field selects the variant; all variants carry `text` and `icon` plus variant-specific fields; `#[serde(other)] Unknown` fallback handles future API additions without breaking deserialization
- **Flat optional struct**: `ItemDetails` is a single struct with all fields `Option<T>` covering all item types (Armor, Weapon, Trinket, UpgradeComponent, Back); use `Item::item_type` to know which fields are populated — no enum needed because the API object has no discriminant tag
- **Flattened embed**: `#[serde(flatten)]` on `TraitedFact.fact` — inlines `Fact` fields directly into the parent JSON object
- **Shared fact module**: `facts.rs` re-exported selectively (`Fact`, `TraitedFact`) and imported by `skills.rs` and `traits.rs` via `use super::facts::{Fact, TraitedFact}`
- **Inline model tests**: each model file has a `#[cfg(test)] mod tests` block with representative JSON payloads exercising deserialization edge cases
- **Token bucket rate limiter**: `Mutex<TokenBucket>` inside `Gw2Client` — 300 burst, 5/sec refill; `take()` sleeps only when empty; exponential backoff on HTTP 429
- **Build-number cache invalidation**: `DataCache` stores `build: u32` in every cache entry; `is_stale(key, current_build)` reads metadata only (no full deserialize) to check staleness
- **Progress callback pattern**: download orchestration accepts `impl FnMut(DownloadProgress)` — decouples UI from data layer; `DownloadProgress` carries step index, total, name, done flag
- **Fetch-then-filter pattern**: `fetch_equipment_items` fetches all item IDs first, batch-fetches 200 at a time, filters by type+rarity in-process — used when API has no server-side filter
- **Screen state machine pattern**: `Screen::Setup(SetupStep)` drives render dispatch; `init()` routes to the correct step based on `AppConfig` completeness (`has_gw2_key`, `has_gemini_key`, `cache_build_number`)
- **Background thread + state callback pattern**: UI spawns `std::thread::spawn` for blocking API calls; results written back via `crate::state::with_state(|s| ...)` — avoids channels, works with `Mutex<Option<T>>` global
- **Wizard transient state pattern**: `SetupState` (input buffers, key status, download progress) embedded in `AddonState`, initialized via `#[derive(Default)]` — cleared implicitly on re-init
- **Config completeness guard**: `AppConfig::is_setup_complete()` requires all three fields (gw2_api_key, gemini_api_key, cache_build_number) — single source of truth for setup routing and post-update cache validation
- **Left menu + content split pattern**: fixed 180px `ChildWindow::new("##left_menu")` + fill-remaining `ChildWindow::new("##main_content").size([0.0, 0.0])` — standard main UI layout
- **GameDb pre-index pattern**: `GameDb::load()` builds all `HashMap<u32, T>` indexes once from DataCache; optimizer queries by ID in O(1); derived indexes (skills_by_profession, traits_by_spec, items_by_type, runes/sigils/relics Vecs) built in same pass
- **Borrow conflict avoidance pattern**: collect owned snapshots (`profession_name.clone()`, `stats_snapshot`) before block containing mutable state borrow — avoids simultaneous mut+immut borrow of `AddonState`
- **Optimizer-to-UI type conversion**: `candidate_to_suggestion()` bridges `gw2_optimizer::engine::BuildCandidate` to `crate::ui::comparison::BuildSuggestion`; stat block f32 fields rounded to i32 via `.round() as i32`
- **Prompt injection guard**: `chat_refinement_prompt` sanitizes user input — 300-char truncation, backtick filtering, XML delimiters (`<player_request>...</player_request>`) — treats player text as data, not instructions
- **Gemini enrichment pattern**: post-optimization bg thread calls `enrich_with_gemini()` → builds context string from GameDb spec_names + candidate summaries → selects `new_build_prompt` or `improve_build_prompt` based on whether `current_build_summary` is present → calls `generate_cached()` → `parse_gemini_build()` → `apply_gemini_response()` onto first suggestion; Gemini errors logged as Warning and skipped, not surfaced as fatal
- **Non-destructive Gemini merge**: `apply_gemini_response()` only overwrites `BuildSuggestion` fields when Gemini returned a non-empty value — preserves optimizer-computed stats while enriching explanation, skills, weapons, rune, sigils, relic, stat_prefix, changes_made
- **Chat refinement as suggestion tab**: `send_chat_message()` creates a fresh `BuildSuggestion { label: "Chat Refinement", ..Default::default() }`, applies parsed Gemini response, pushes to `comparison.suggestions` and advances `selected_suggestion` — each chat turn becomes a selectable build tab in the comparison view
- **Stat diff color coding**: `render_stat_diff()` colors each diff value green (`[0,1,0,1]`) for positive, red (`[1,0,0,1]`) for negative, gray (`[0.7,0.7,0.7,1]`) for zero; prepends "+" on positive values
- **Mutex poison recovery logging**: `lock_state()` uses `unwrap_or_else` to recover from poisoned mutex; logs `nexus::log::LogLevel::Warning` before calling `into_inner()` — recovers state rather than panicking, visible in Nexus log
- **Gemini rate persistence pattern**: `GeminiClient::with_persistence()` loads prior `PersistedUsage` (day + requests_today) from JSON on construction; saves after every successful `generate()` call; day mismatch resets counter — survives addon reload
- **BuildStorage JSON persistence pattern**: `BuildStorage::new(addon_dir)` writes one JSON file per saved build to `{addon_dir}/saves/{sanitized_name}.json`; `list()` reads all `.json` files and sorts by `timestamp` descending; `delete()` removes by sanitized name — no database, plain filesystem
- **SavedBuild mirrors BuildSuggestion**: `SavedBuild` in `crates/core/src/types.rs` mirrors the `BuildSuggestion` display fields (label, stat_prefix, specializations, weapons, skills, rune, sigils, relic, explanation, changes_made, estimated_stats) plus persistence metadata (name, timestamp, character_name, game_mode) — allows round-trip save/load without data loss
<!-- END AUTO-MANAGED -->
