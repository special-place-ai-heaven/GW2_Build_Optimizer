# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**GW2 Build Optimizer v1.0.0** — In-game Guild Wars 2 addon (Nexus plugin) that optimizes character builds across all game modes (PvE, PvP, WvW). Uses the GW2 API for game/character data and Google Gemini for LLM-powered build reasoning. Full feature complete (S01-S15).

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
  src/state.rs      — AddonState: screen (Screen/SetupStep state machine), setup (SetupState transient wizard data), main (MainState with GameDb, ComparisonState, ChatBarState, optimization state, aggression_index: i32 for 5-stage playstyle slider, saved_builds/saved_builds_loaded/save_name_input/save_status for Save/Load, build_tabs/equipment_tabs/selected_build_tab/selected_equipment_tab/build_chat_code for template selection, confirm_reset: bool for Settings dialog, save_status_frames/confirm_delete/chat_wait_frames/copy_feedback_frames for UX feedback), cancel_token (CancellationToken backed by Arc<AtomicBool>); MainTab enum includes SaveLoad variant; clear() cancels token then drops state; with_state() accessor; init() routes to correct SetupStep based on config
  src/ui/mod.rs     — render() dispatches on Screen::Setup vs Screen::Main; calls render_main() for main screen
  src/ui/setup.rs   — 4-step setup wizard: GW2 key validation, Gemini key validation, data download with ProgressBar, completion; background thread validation writes back via with_state()
  src/ui/main_view.rs — full main view: left menu (character picker, Build/Equipment Template dropdowns, chat code with Copy button, game mode radio, aggression slider, tab nav), render_new_build_tab() (archetype selector + comparison + chat bar), render_improve_tab(), render_saveload_tab() (lists/loads/deletes saved builds), render_settings_tab(); start_optimization() → engine::optimize() → enrich_with_gemini() → simulate_suggestion_rotation() pipeline; enrich_with_gemini() uses generate_with_tools() with ToolContext (db, profession, candidates, aggression_level) giving Gemini tools access; send_chat_message() also tool-enabled; simulate_suggestion_rotation() auto-runs rotation sim after Gemini populates skills — resolves weapon skills from both weapon sets (parse_weapon_sets() + tag_weapon_set()) and heal/utility/elite by name (parse_skill_names()); infer_profession_from_specs() used to match weapon skills to correct profession; saved_to_suggestion() recomputes combat metrics + rotation from saved stats; apply_gemini_response() non-destructive merge; compute_3tier_combat()/perf_to_combat_metrics() DRY helpers; start_optimization_with_profession() avoids borrow conflicts
  src/ui/comparison.rs — BuildSuggestion (+ combat_solo/combat_party/combat_squad Option<CombatMetrics> + rotation: Option<RotationBreakdown>) + ComparisonState (+ current_combat_solo/party/squad); render_comparison() with side-by-side columns, collapsing headers: "Primary Attributes", "Combat Performance" (3 buff tiers), "Defenses & Resistances", "Rotation Breakdown" (simulated DPS, condition/buff uptimes, skill usage, stunbreak/stability), "Why This Build?", "Changes Made"; render_stat_diff() 4-col ImGui table with green/red/gray color coding by diff sign
  src/ui/chat_bar.rs — ChatBarState + render_chat_bar()
  src/ui/main_view/build_display.rs — render_build() for current build display; mode-aware: shows PvP amulet section OR full gear (armor+trinkets+relic) based on build.pvp_amulet.is_some()
crates/core/        — Shared types (types.rs: ResolvedBuild, StatBlock, CombatMetrics, RotationBreakdown, SavedBuild + all sub-types), config (config.rs), storage (storage.rs: BuildStorage — save/list/delete SavedBuild as JSON files in {addon_dir}/saves/; sanitize_filename() for safe filenames; corrupt-save-file tolerance via skip-and-warn)
crates/gw2api/      — GW2 API v2 client + cache + download orchestration
  src/client.rs     — Gw2Client: token-bucket rate limiter, get/fetch_all/fetch_by_ids, validate_api_key; `get_with_params` builds query string manually (not via reqwest `.query()`) to avoid URL-encoding commas as %2C in bulk ID requests; MAX_BULK_IDS=200; `fetch_by_ids` parallelizes with up to 5 concurrent threads via `std::thread::scope`; 429/502/503/504 all trigger exponential backoff retry (MAX_RETRIES=5); connection errors (`.send()` failures) and body-read errors (`.text()` failures) caught with `match`+`continue` to enter retry loop (not `?` which would bypass it); backoff sleep at top of loop before rate-limiter `take()`; `last_error: Option<ApiError>` tracks most recent error for final return when retries exhausted; HTML error bodies stripped to clean messages; exhausted-retries returns `ApiError::Api` (not `RateLimited`); `T: Send` bound required on `fetch_all`/`fetch_by_ids`
  src/cache.rs      — DataCache: JSON file cache keyed by build number, is_stale/save/load/clear_all
  src/download.rs   — download_all: 8-endpoint orchestration with DownloadProgress callback (carries optional `detail: Option<String>` for sub-step info); `report()` is a free function (not closure) to avoid borrow conflicts; items step calls `fetch_by_ids_with_progress("items", &ids, |fetched, total| ...)` with live progress updates in format `"{N} / {M} items fetched"` (concurrency handled inside `fetch_by_ids_with_progress`); lenient item deserialization skips malformed items via `serde_json::from_value(...).ok()`; filters ~100k items to Exotic/Ascended/Legendary Armor/Weapon/Trinket/Back/UpgradeComponent/Relic
  src/models/       — serde structs per API endpoint (see GW2 API Models table below)
crates/optimizer/   — engine.rs (pipeline orchestration; BuildCandidate carries combat: CombatPerformance + modifiers: DamageModifiers + equipped_traits: Vec<u32>; scoring via score_combat_weighted() using CombatPerformance + AggressionLevel, not raw stats; select_best_major_traits() picks 1 best per column), gamedb.rs (GameDb: pre-indexed HashMap lookups for all game data; primary + derived indexes including pvp_amulets, reverse indexes traits_by_condition/skills_by_condition/traits_by_buff/skills_by_buff for synergy discovery), combat.rs (DamageModifiers: multiplicative strike/condi/crit/healing modifiers + additive duration modifiers; BuffProfile: might_stacks/fury/protection/resolution/vulnerability_stacks/label; CombatPerformance: effective_power, crit_chance, strike/condition/total DPS indexes, condition_ticks: ConditionTicks, healing_power_index, boon/condi duration, effective_health, damage_reduction_pct; calculate_combat_performance() applies buff stats + modifiers using GW2 published formulas; calculate_condition_ticks() applies 5 tick formulas with per-condition modifiers; extract_damage_modifiers() parses Fact::AttributeAdjust/Buff from traits+items; default_buff_profiles() returns [Solo, Party (Might x15 + Fury), Full Squad (Might x25 + Fury + Vuln x25)]; REFERENCE_ARMOR=2597.0, REFERENCE_WEAPON_STRENGTH=1100.0), scoring.rs (Archetype enum with score_combat/score_combat_weighted; AggressionLevel 5-stage enum: FullDefense/Defensive/Balanced/Aggressive/FullOffense with damage_weight()/survival_weight()/default_for_mode()), gemini.rs (GeminiClient: gemini-2.5-flash; generate/generate_cached/generate_with_tools; FunctionCall/FunctionResponse/Tool/FunctionDeclaration types for Gemini function calling; multi-turn loop with max_turns cap; RateTracker: 10 RPM, 240/250 RPD), gemini_tools.rs (ToolContext with db+profession_name+candidates+aggression_level; tool_declarations() returning 18 function declarations + execute_tool() dispatcher; tools: profession info, spec traits, trait details, skill info, runes/sigils/relics listing, stat calc, combat sim, score_build, current build, optimizer results, search_traits_by_effect, find_condition_sources, search_skills_by_effect, find_synergies, get_build_synergy_report, simulate_rotation), prompts.rs (tool-aware prompts with aggression guidance; aggression_context()/aggression_description() helpers; parse_gemini_build returns typed GeminiBuildResponse), search.rs (6 mix strategies per prefix pair), stats.rs (StatBlock with AddAssign impl + alias normalization), rotation/ (mod.rs: RotationSkill/SkillSlot/SkillEffect/SimulationResult/SkillUsage types; builder.rs: build_rotation_skills from GameDb + tag_weapon_set(); simulator.rs: 100ms tick DPCT-optimal simulation with weapon swap (10s CD), condition stacking with 1s tick decay, buff uptime tracking, control metrics (stunbreak_count/has_stability/stability_uptime); skill_timings.rs: slot-based default cast+aftercast times, HUMAN_DELAY_MS=80, MIN_SKILL_GAP_MS=100)
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
| `professions.rs` | `/v2/professions` | `Profession` (+ `skills_by_palette: Vec<Vec<u32>>` for palette↔skill mapping, requires API schema 2019-12-19), `WeaponInfo` (elite spec gate via `specialization` field), `WeaponSkillRef`, `Training`, `TrainingTrack` |
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

- **Stub module pattern**: unimplemented files used to contain a single comment `// <purpose>.\n// Will be populated in S##.` — all stubs are now fully implemented as of S15; do not add placeholder code to stub files
- **Workspace dep hoisting**: all shared deps (serde, serde_json, reqwest, thiserror, urlencoding, chrono, base64) declared once in root `[workspace.dependencies]`, crates reference with `.workspace = true`
- **State accessor pattern**: global state exposed via free functions (`init`, `toggle_window`, `is_window_visible`) rather than direct static access
- **Crate internal visibility**: `mod state; mod ui;` kept private in addon; `pub mod` used in library crates (core, gw2api, optimizer)
- **Tagged enum serde**: `#[serde(tag = "type")]` on `Fact` — API `"type"` field selects the variant; all variants carry `text` and `icon` plus variant-specific fields; `#[serde(other)] Unknown` fallback handles future API additions without breaking deserialization
- **Flat optional struct**: `ItemDetails` is a single struct with all fields `Option<T>` covering all item types (Armor, Weapon, Trinket, UpgradeComponent, Back); use `Item::item_type` to know which fields are populated — no enum needed because the API object has no discriminant tag
- **Flattened embed**: `#[serde(flatten)]` on `TraitedFact.fact` — inlines `Fact` fields directly into the parent JSON object
- **Shared fact module**: `facts.rs` re-exported selectively (`Fact`, `TraitedFact`, `deserialize_facts`, `deserialize_traited_facts`) and imported by `skills.rs` and `traits.rs`; lenient deserializers skip facts missing a `type` discriminator via `filter_map(serde_json::from_value(...).ok())`
- **Inline model tests**: each model file has a `#[cfg(test)] mod tests` block with representative JSON payloads exercising deserialization edge cases
- **Token bucket rate limiter**: `Mutex<TokenBucket>` inside `Gw2Client` — 300 burst, 5/sec refill; `take()` sleeps only when empty; exponential backoff on HTTP 429/502/503/504 (MAX_RETRIES=5); connection errors (`.send()`) and body-read errors (`.text()`) also retried via `match`+`continue` rather than `?` (which would bypass the loop); `last_error: Option<ApiError>` pattern returns most recent error on exhaustion; backoff sleep at loop top before rate-limiter `take()`; HTML error bodies stripped to clean messages; exhausted retries return `ApiError::Api`
- **Build-number cache invalidation**: `DataCache` stores `build: u32` in every cache entry; `is_stale(key, current_build)` reads metadata only (no full deserialize) to check staleness
- **Progress callback pattern**: download orchestration accepts `impl FnMut(DownloadProgress)` — decouples UI from data layer; `DownloadProgress` carries step index, total, name, done flag, and optional `detail: Option<String>` for sub-step granularity (e.g. "batch 5/500"); UI merges detail into the ProgressBar overlay as `"{step_name} ({detail})"`
- **Fetch-then-filter pattern**: `fetch_equipment_items` fetches all item IDs first, batch-fetches 200 at a time, filters by type+rarity in-process — used when API has no server-side filter
- **Screen state machine pattern**: `Screen::Setup(SetupStep)` drives render dispatch; `init()` routes to the correct step based on `AppConfig` completeness (`has_gw2_key`, `has_gemini_key`, `cache_build_number`)
- **Background thread + state callback pattern**: UI spawns `std::thread::spawn` for blocking API calls; results written back via `crate::state::with_state(|s| ...)` — avoids channels, works with `Mutex<Option<T>>` global
- **Wizard transient state pattern**: `SetupState` (input buffers, key status, download progress) embedded in `AddonState`, initialized via `#[derive(Default)]` — cleared implicitly on re-init
- **Config completeness guard**: `AppConfig::is_setup_complete()` requires all three fields (gw2_api_key, gemini_api_key, cache_build_number) — single source of truth for setup routing and post-update cache validation
- **Left menu + content split pattern**: fixed 180px `ChildWindow::new("##left_menu")` + fill-remaining `ChildWindow::new("##main_content").size([0.0, 0.0])` — standard main UI layout
- **GameDb pre-index pattern**: `GameDb::load()` builds all `HashMap<u32, T>` indexes once from DataCache; optimizer queries by ID in O(1); derived indexes (skills_by_profession, traits_by_spec, items_by_type, runes/sigils/relics Vecs, skill_to_palette/palette_to_skill bidirectional maps) all built in same pass from raw data
- **Borrow conflict avoidance pattern**: collect owned snapshots (`profession_name.clone()`, `stats_snapshot`) before block containing mutable state borrow — avoids simultaneous mut+immut borrow of `AddonState`
- **Optimizer-to-UI type conversion**: `candidate_to_suggestion()` bridges `gw2_optimizer::engine::BuildCandidate` to `crate::ui::comparison::BuildSuggestion`; stat block f32 fields rounded to i32 via `.round() as i32`
- **Prompt injection guard**: `chat_refinement_prompt` sanitizes user input — 300-char truncation, backtick filtering, XML delimiters (`<player_request>...</player_request>`) — treats player text as data, not instructions
- **Gemini enrichment pattern**: post-optimization bg thread calls `enrich_with_gemini()` → builds context string from GameDb spec_names + candidate summaries → selects `new_build_prompt` or `improve_build_prompt` based on whether `current_build_summary` is present → calls `generate_cached()` → `parse_gemini_build()` → `apply_gemini_response()` onto first suggestion; Gemini errors logged as Warning and skipped, not surfaced as fatal
- **Non-destructive Gemini merge**: `apply_gemini_response()` only overwrites `BuildSuggestion` fields when Gemini returned a non-empty value — preserves optimizer-computed stats while enriching explanation, skills, weapons, rune, sigils, relic, stat_prefix, changes_made
- **Chat refinement as suggestion tab**: `send_chat_message()` creates a fresh `BuildSuggestion { label: "Chat Refinement", ..Default::default() }`, applies parsed Gemini response, pushes to `comparison.suggestions` and advances `selected_suggestion` — each chat turn becomes a selectable build tab in the comparison view
- **Stat diff color coding**: `diff_color()` in `comparison.rs` uses a 0.5 dead-zone threshold — diff > 0.5 → green `[0,1,0,1]`, diff < -0.5 → red `[1,0,0,1]`, -0.5 to 0.5 → gray `[0.7,0.7,0.7,1]`; integer rows use `render_int_row()`, percentage rows use `render_pct_row()`; both prepend "+" on positive diffs
- **Condition tick breakdown pattern**: `render_combat_performance()` appends a "Condition Ticks (per tick, Solo)" section only when at least one of bleeding/burning/poison/torment/confusion is non-zero in either build — filtered via `ticks_to_show` predicate; each row shows tick value + inline `(+N)`/`(-N)` colored annotation on the same cell rather than a dedicated diff column; avoids cluttering the UI for non-condition builds
- **Mutex poison recovery logging**: `lock_state()` uses `unwrap_or_else` to recover from poisoned mutex; logs `nexus::log::LogLevel::Warning` before calling `into_inner()` — recovers state rather than panicking, visible in Nexus log
- **Gemini rate persistence pattern**: `GeminiClient::with_persistence()` loads prior `PersistedUsage` (day + requests_today) from JSON on construction; saves after every successful `generate()` call; day mismatch resets counter — survives addon reload
- **BuildStorage JSON persistence pattern**: `BuildStorage::new(addon_dir)` writes one JSON file per saved build to `{addon_dir}/saves/{sanitized_name}.json`; `list()` reads all `.json` files and sorts by `timestamp` descending; `delete()` removes by sanitized name — no database, plain filesystem
- **SavedBuild mirrors BuildSuggestion**: `SavedBuild` in `crates/core/src/types.rs` mirrors the `BuildSuggestion` display fields (label, stat_prefix, specializations, weapons, skills, rune, sigils, relic, explanation, changes_made, estimated_stats) plus persistence metadata (name, timestamp, character_name, game_mode) — allows round-trip save/load without data loss
- **Error toast bar pattern**: `main_view.rs` checks `state.main.error.clone()` at render start; if present, renders colored text `[!] message` + dismiss button above separator; errors are set by API threads and cleared by user or on navigation — UI-friendly error surfacing without modal dialogs; optimization errors use a separate `comparison.error: Option<String>` field rendered outside the `!suggestions.is_empty()` guard so they are visible even when no results were produced; both error fields have inline "Dismiss" buttons
- **Settings tab info display pattern**: `render_settings_tab()` shows cache size (via `DataCache::estimate_size()`), clear cache button, Gemini quota remaining (via `client.remaining_today()`), and reset setup with confirmation dialog — aggregates diagnostic/utility functions in one place
- **Resolve build refactoring pattern**: `resolve_build()` decomposed into `resolve_specs()`, `resolve_skills()`, `resolve_equipment()`, `resolve_pvp_amulet()` helper functions — each handles one subsystem, returns `Result<T, String>`, called in sequence to build up `ResolvedBuild` — improves testability and reduces function size
- **Lenient fact deserializer pattern**: `deserialize_facts()` and `deserialize_traited_facts()` in `facts.rs` handle GW2 API responses that occasionally return fact objects without a `type` field — deserializes to `Vec<serde_json::Value>` first, then `filter_map(serde_json::from_value(...).ok())` silently skips unparseable entries; applied via `#[serde(default, deserialize_with = "...")]` on `facts` and `traited_facts` fields in `Skill`, `Trait`, and `TraitSkill`
- **Lenient item batch deserialization pattern**: `download_all` items step fetches all items as `Vec<serde_json::Value>` via `fetch_by_ids` then individually tries `serde_json::from_value::<models::Item>(val)` per entry, silently skipping items that fail — tolerates GW2 API inconsistencies in the ~100k item catalog without aborting the batch
- **Live integration test pattern**: `crates/gw2api/tests/live_download.rs` is `#[ignore]`d and network-gated; run with `cargo test -p gw2-api --test live_download -- --ignored --nocapture`; tests all endpoints end-to-end with `Instant` timing and minimum-count assertions; uses a 2000-item subset for items to avoid the full ~100k download during testing
- **Concurrent batch fetch pattern**: `fetch_by_ids` groups 200-ID batches into sets of 5 and spawns them concurrently via `std::thread::scope` — each scoped thread calls `get_with_params(endpoint, &[("ids", &joined)])`; results collected after all threads join; `std::thread::scope` guarantees threads finish before the scope exits so no `Arc` needed for references; `T: Send` bound required because results cross thread boundaries
- **Manual query string pattern**: `get_with_params` builds query strings as `key=value&...` manually rather than using reqwest's `.query()` — reqwest encodes commas as `%2C` which triples separator length and can exceed URL limits on bulk ID lists; plain concatenation produces `ids=1,2,3` which GW2 API requires
- **PvP optimization branch**: `optimize()` checks `GameMode::PvP` first and dispatches to `optimize_pvp()` — skips gear search entirely, uses empty `GearCandidate { stat_prefix_name: "(PvP Amulet)" }`, evaluates spec+trait combos only; PvE/WvW path runs full gear+spec search unchanged
- **Skill-palette mapping pattern**: `GameDb` builds `skill_to_palette: HashMap<u32, u32>` and `palette_to_skill: HashMap<u32, u32>` from `Profession.skills_by_palette` (each entry is `[palette_id, skill_id]`); used to encode/decode build template chat codes
- **Mode-aware build display**: `render_build()` in `main_view/build_display.rs` branches on `build.pvp_amulet.is_some()` — shows PvP amulet name+stats section when true, full gear (armor+trinkets+relic) when false
- **Submodule extraction pattern**: `main_view.rs` refactored into module directory `main_view/` with `build_display.rs` as a private submodule declared via `mod build_display` in `main_view.rs`; applies when a single file grows too large
- **Combat-performance scoring pattern**: `engine.rs` scores candidates via `score_combat(&CombatPerformance, archetype)` rather than raw stat heuristics; two-phase approach: (1) gear-only pre-score with empty `DamageModifiers` + Solo profile to prune candidates to `top_n*2`, then (2) full gear×spec cross-product with `extract_damage_modifiers()` (traits) + `calculate_combat_performance()` for final scoring; `BuildCandidate` stores `combat: CombatPerformance` and `modifiers: DamageModifiers` so the UI can recalculate with different buff profiles without re-running the optimizer
- **Stat alias normalization pattern**: `StatBlock::add()` and `get()` normalize multiple GW2 API attribute name variants to single internal fields — e.g. `"ConditionDuration"` and `"Expertise"` both map to `expertise`, `"CritDamage"` and `"Ferocity"` to `ferocity`, `"Healing"` and `"HealingPower"` to `healing_power` — handles GW2 API inconsistency where both old and new attribute names appear across different endpoints
- **Traited-fact override pattern**: `extract_damage_modifiers()` first collects `overrides: Option<u32>` indices from active `traited_facts` (those whose `requires_trait` is equipped), then skips those base fact indices when iterating `t.facts` — correctly handles conditional trait upgrades where a stronger version replaces the base fact when a synergistic trait is active
- **Rune bonus string parsing pattern**: `extract_damage_modifiers()` parses rune bonus tier strings (e.g. `"+7% Burning Duration"`, `"+5% damage"`) via `parse_rune_modifier()` — accommodates GW2 API returning rune 6-tier bonuses as unstructured text in `details.bonuses: Vec<String>` rather than structured `Fact` objects like traits
- **Multi-strategy gear search pattern**: `search_gear_prefixes()` generates 6 mix strategies per prefix pair: full set, secondary on trinkets, rings only, accessories only, weapons only, and armor-only primary — captures common GW2 breakpoint patterns like Berserker+Assassin for crit cap; `build_mixed_candidate()` helper generates candidates with primary stat on most slots and secondary on specified slot groups (TRINKET_SLOTS, RING_SLOTS, ACCESSORY_SLOTS, WEAPON_SLOTS)
- **Search-space pruning via relevant_prefixes**: `Archetype::relevant_prefixes()` returns a `&[&str]` slice of stat prefix names (e.g. `["Berserker's", "Assassin's", "Dragon's"]` for PowerDPS); gear search filters `itemstats_cache` to only those prefixes before the expensive gear×spec cross-product — reduces search space from ~50 prefixes to 3–5 per archetype; CelestialHybrid includes Diviner's, Trailblazer's, Minstrel's for meaningful hybrid mixing
- **score_combat normalization pattern**: each `score_combat()` branch divides raw `CombatPerformance` metrics by hand-tuned divisors (e.g. `strike_dps_index / 50000.0`, `effective_health / 200000.0`) so scores across different archetypes occupy a comparable 0–2 range; divisors are not derived formulas but empirically chosen constants — do not "normalize" them without validating cross-archetype ordering
- **Gemini function calling pattern**: `generate_with_tools()` sends `tools: [{ functionDeclarations }]` in request, loops on `functionCall` parts in response, executes via `execute_tool` closure, sends back `functionResponse` parts, repeats until Gemini returns text or `max_turns` exceeded; `ToolContext` holds `&GameDb`, `&str` profession, `&[BuildCandidate]`, `Option<&str>` current build summary, and `AggressionLevel`; tool responses are concise JSON (<500 tokens) to stay within context; `tool_declarations()` returns all 18 tools in a single `Tool` block (12 core + 5 synergy discovery + 1 simulate_rotation); prompts instruct Gemini to use tools step-by-step before producing final JSON answer
- **Multi-tier combat display pattern**: `BuildSuggestion` stores pre-computed `combat_solo/combat_party/combat_squad: Option<CombatMetrics>` (Solo = gear+traits only; Party = Might x15 + Fury; Full Squad = Might x25 + Fury + Vulnerability x25); `ComparisonState` mirrors these for the current build as `current_combat_solo/party/squad`; `render_comparison()` shows all three tiers in a "Combat Performance" collapsing header — allows direct tier-by-tier comparison without live recalculation
- **CombatMetrics display bridge pattern**: `CombatMetrics` in `crates/core/src/types.rs` bridges `optimizer::combat::CombatPerformance` (f64 fields, internal calc) to UI display (i32 rounded values: effective_power, strike/condition/total DPS indexes, healing_index, boon/condi duration, effective_health, damage_reduction_pct, per-condition tick values); derives `Serialize, Deserialize` for save/load persistence — avoids exposing optimizer internals to UI and core crates
- **Condition tick formula pattern**: five GW2 level-80 tick functions in `combat.rs`: bleeding = `0.06*CD+22`, burning = `0.155*CD+131`, poison = `0.06*CD+33.5`, torment = `0.06*CD+22`, confusion = `0.195*CD+95.5`; each multiplied by `DamageModifiers::total_condi_mult_for(condition_name)` to apply global + per-condition modifiers; combined in `calculate_condition_ticks()` returning `ConditionTicks` struct
- **Cancellation token pattern**: `CancellationToken` wraps `Arc<AtomicBool>`; stored in `AddonState.cancel_token` and cloned cheaply into every background thread at spawn time; threads check `is_cancelled()` at entry and between expensive operations (API calls, Gemini generate); `clear()` calls `cancel_token.cancel()` before dropping state — ensures all 8 spawn sites (3 setup, 5 main) exit early on addon unload, preventing dangling threads and use-after-free
- **Frame-counter auto-dismiss pattern**: `save_status_frames: u32` in `MainState` incremented each render tick; after ~180 frames (~3s at 60fps) `save_status` is cleared automatically — avoids requiring user to manually dismiss transient status messages; `copy_feedback_frames: u32` applies the same approach for "Copied!" tooltip feedback (~120 frames)
- **Chat timeout recovery pattern**: `chat_wait_frames: u32` counts render frames while `chat.waiting = true`; after ~1800 frames (~30s at 60fps) waiting flag is cleared and a timeout message is injected into chat history via `add_ai_response()` — prevents permanent UI lockout on failed or hung Gemini responses
- **Inline confirm dialog pattern**: `confirm_delete: Option<usize>` in `MainState` stores the index of a saved build pending deletion; Save/Load tab renders inline Yes/No buttons for that index only — no modal, consistent with `confirm_reset: bool` approach used in Settings tab; set to `None` on dismiss or completion
- **Disabled state with alpha + tooltip pattern**: buttons disabled during optimization or missing prerequisites use `ui.disabled(|| ...)` block with `ui.set_next_item_width` / alpha 0.4 + `ui.item_hovered()` tooltip — provides visual feedback and explanation without removing the button from layout
- **AggressionLevel scoring pattern**: 5-stage enum (FullDefense→FullOffense) with `damage_weight()` (0.1–0.95) + `survival_weight()` (1-dw); `score_combat_weighted()` blends archetype-specific damage_score and EHP-based survival_score by these weights; `default_for_mode()` maps PvE→Aggressive, WvW/PvP→Balanced; wired through engine.rs, gemini_tools.rs ToolContext, and Gemini prompts with `aggression_context()` guidance text
- **Synergy discovery tool pattern**: 5 Gemini tools (search_traits_by_effect, find_condition_sources, search_skills_by_effect, find_synergies, get_build_synergy_report) use GameDb reverse indexes (traits_by_condition, skills_by_condition etc.) for O(1) lookups; enables Gemini to cross-reference trait↔skill↔gear interactions instead of blind selection
- **Rotation simulation pattern**: `rotation/simulator.rs` runs 100ms tick DPCT-optimal simulation with weapon swap (10s CD); tracks condition stacks with duration decay, buff uptime, control metrics (stunbreaks, stability); `SimulationResult` carries DPS breakdown + per-skill usage; `simulate_suggestion_rotation()` in main_view.rs auto-runs after Gemini enrichment — resolves weapon skills from both weapon sets via `parse_weapon_sets()`+`tag_weapon_set()`, heal/utility/elite by name via `parse_skill_names()`; attaches `RotationBreakdown` to `BuildSuggestion` for UI display
- **Saved build rehydration pattern**: `saved_to_suggestion()` converts SavedBuild's `estimated_stats` (i32) back to optimizer StatBlock (f64), runs `compute_3tier_combat()` for combat metrics and `simulate_suggestion_rotation()` for rotation data — loaded builds show full Combat Performance and Rotation Breakdown sections instead of empty values
<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: git-insights -->
## Git Insights

Recent design decisions from commit history:
- API client retry loop fixed: `.send()` and `.text()` errors now caught with `match`+`continue` instead of `?` — previously connection and body-read errors bypassed retries entirely; backoff moved to loop top; `last_error` pattern tracks most recent error for final return; `download_all` items step simplified to call `fetch_by_ids` directly (concurrency handled inside `fetch_by_ids`, redundant grouping removed from download.rs); live integration test added (`tests/live_download.rs`, `#[ignore]`) covering all endpoints end-to-end
- API client hardened: MAX_BULK_IDS restored to 200 (manual query string building avoids reqwest %2C URL-encoding, making full 200-ID batches safe); `fetch_by_ids` parallelized with up to 5 concurrent `std::thread::scope` threads; 502/503/504 server errors added to retry loop alongside 429; MAX_RETRIES increased to 5; HTML error bodies stripped to "Server error (HTTP {status})"; exhausted-retries error returns `ApiError::Api` instead of `RateLimited`
- Real combat performance model replaced raw stat heuristics: `DamageModifiers` (multiplicative strike/condi/crit/healing), `BuffProfile` (might/fury/protection/vuln), and `CombatPerformance` added to optimizer; 3-tier buff profiles (Solo/Party/Full Squad) computed at candidate evaluation time and stored on `BuildCandidate` for UI reuse without re-running the optimizer
- 3-tier combat display added to comparison view: `BuildSuggestion` carries pre-computed `combat_solo/combat_party/combat_squad: Option<CombatMetrics>`; `ComparisonState` mirrors these for the current build — direct tier-by-tier comparison without live recalculation
- Lenient API deserialization hardened: items download batch-deserializes each entry individually (skipping failures) and download progress carries `detail: Option<String>` for per-batch sub-step visibility in UI
- `resolve_build()` refactored into subsystem helpers (`resolve_specs`, `resolve_skills`, `resolve_equipment`, `resolve_pvp_amulet`) for testability; Settings UI and confirm-reset dialog added in polish pass (S15)
- `CancellationToken` system added: replaced static SHUTDOWN flag with `Arc<AtomicBool>` token stored in `AddonState`; all 8 background thread spawn sites clone and check token; `clear()` cancels before drop — prevents dangling threads on addon unload
- Comprehensive UX overhaul: frame-counter auto-dismiss for save status (~3s) and copy feedback (~2s); chat timeout recovery (~30s unblocks waiting state); inline delete confirmation dialog (`confirm_delete: Option<usize>`); disabled button states with alpha dimming + tooltips during optimization; comparison columns auto-size (200–400px); suggestion tabs show stat prefix; "Refresh Game Data" button fixed to clear cache and re-download
- Optimizer engine accuracy fixes: `StatBlock` alias normalization (old+new GW2 API attribute names unified); `extract_damage_modifiers()` now handles traited-fact overrides (skips base facts superseded by conditional upgrades) and rune bonus string parsing; `Archetype::relevant_prefixes()` prunes gear search space per archetype; `score_combat()` uses hand-tuned normalization divisors for comparable cross-archetype ordering; engine uses two-phase scoring (gear-only pre-score to prune, then full trait+modifier score)
- Trait selection fix: optimizer now picks 1 best major trait per column (Adept/Master/Grandmaster) via `select_best_major_traits()` instead of using all 9 — scores each trait's `AttributeAdjust`, `Percent`, `BuffConversion` facts against archetype weights; `BuildCandidate.equipped_traits` tracks selected IDs; UI shows selected traits instead of first 3
- Gemini function calling integration: `generate_with_tools()` multi-turn loop gives Gemini native tool access to 12 game data queries (professions, traits, skills, runes, sigils, relics) and calculations (stat calculation, combat simulation, scoring); `gemini_tools.rs` contains `ToolContext`, tool declarations, and execution dispatcher; tool-aware prompts instruct Gemini to use tools before answering; both enrichment and chat use tool-enabled generation
- Expanded gear search with 6 mix strategies per prefix pair (full, trinkets, rings, accessories, weapons, armor-only); `StatBlock` implements `AddAssign` replacing manual `stats_add()`; archetype inference from stats now subtracts base values and covers all 7 archetypes
- Combat model accuracy fixes: EHP formula removed bogus `armor_dr = armor / (armor + 2600)` (WoW-style DR, not GW2) — GW2 armor is a linear divisor, only Protection boon contributes DR; `crit_chance` field added to `CombatPerformance` and `CombatMetrics` for display; current build combat metrics now computed (were always None before) via `calculate_current_stats()` returning `CombatBundle`; DRY helpers `perf_to_combat_metrics()` and `compute_3tier_combat()` extracted to eliminate 3 duplicated closures
- Gemini tool output enhancements: `get_optimizer_results` now includes effective_power, crit_chance, boon_duration, condi_duration in combat section; `summarize_build()` includes CritChance in combat line
- Rotation simulator upgraded to DPCT-optimal scheduling with weapon swap support: `simulator.rs` picks skills by damage-per-cast-time, auto-swaps weapon sets on 10s cooldown when active set is exhausted, tracks control metrics (stunbreak_count, has_stability, stability_uptime); buff skills (Might/Fury/Quickness) estimated in DPCT using heuristic DPS value over their duration; `builder.rs` extracts stunbreak via `Fact::StunBreak { value: Some(true) }`; `tag_weapon_set()` marks weapon skills by set number
- Aggression wired through Gemini and prompt system: `ToolContext.aggression_level` passes `AggressionLevel` into `execute_tool`; `aggression_context()` produces guidance text injected into prompts; Confusion tick formula fixed (`0.195*CD+95.5` was incorrect); rotation breakdown UI section added to comparison view showing simulated DPS, condition/buff uptimes, skill cast counts, and CC metrics
- Rotation simulator upgraded to DPCT-optimal scheduling with full weapon set resolution: `simulate_suggestion_rotation()` now parses `suggestion.weapons` strings to resolve both weapon sets' skills (tagged with set 1/2 via `tag_weapon_set()`), plus heal/utility/elite from skill name strings via `parse_skill_names()`; `simulator.rs` adds `skill_dps_efficiency()` DPCT metric, `WEAPON_SWAP_COOLDOWN_MS`=10s swap logic, `DEFAULT_DURATION_MS`=30s benchmark window; buff skills estimated with heuristic DPS value over their duration; backward-compatible with `weapon_set=0` legacy rotation skills
- API reliability hardening: `get_with_params` builds query strings manually (avoids reqwest URL-encoding commas as %2C); items download uses 5 concurrent 200-ID fetches via `std::thread::scope` per group; optimization errors (`comparison.error`) now rendered outside the suggestions guard so they display even when the optimizer returns empty results; thread panics in optimization bg thread caught with `catch_unwind` and surfaced as error messages rather than silently poisoning the state mutex
- Saved build rehydration: `saved_to_suggestion()` converts `SavedBuild.estimated_stats` (i32) back to `StatBlock` (f64), then runs `compute_3tier_combat()` and `simulate_suggestion_rotation()` — loaded builds now show full Combat Performance and Rotation Breakdown sections; compiler warning pass cleaned up unused imports and fields
<!-- END AUTO-MANAGED -->
