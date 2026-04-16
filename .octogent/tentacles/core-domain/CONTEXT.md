# core-domain

Shared vocabulary — every other tentacle imports from here.

## Agent Toolbelt (required)

- **Use SymForge MCP tools for all code work in this tentacle.** `health` once
  per session to confirm the index is live, then `search_symbols`,
  `search_text`, `get_file_context`, `get_symbol`, `find_references`,
  `edit_within_symbol`, `replace_symbol_body`, `analyze_file_impact`,
  `what_changed`. Raw `Read` / `Grep` / `Glob` on source files burns 70–95%
  more tokens and skips the index. Reserve raw reads for docs or files
  SymForge flags `not indexed`.
- **Optional: Obsidian vault** via `obsidian_simple_search` /
  `obsidian_batch_get_file_contents` if broader project context exists there.
  (Currently no GW2-specific notes — may grow.)

## Scope

- `crates/core/src/types.rs` — build/stat/metrics/saved-build structs (29 symbols)
- `crates/core/src/config.rs` — `AppConfig`, multi-provider LLM config, atomic save
- `crates/core/src/storage.rs` — `BuildStorage` crash-safe JSON persistence
- `crates/core/src/lib.rs` — module re-exports
- `crates/core/Cargo.toml`

## Mission Link

This crate is the *vocabulary* the whole addon speaks. `ResolvedBuild`,
`StatBlock`, `CombatMetrics`, `BuildLocks` are the nouns every other crate
ships across thread boundaries, serializes to disk, and hands to the LLM.
Change a field here and the blast radius is the whole workspace.

## Key Decisions

- **`StatBlock::add` / `StatBlock::get` normalize old-vs-new GW2 attribute names.**
  The API still emits `ConditionDuration` (old) alongside `Expertise` (new).
  Never read/write stats via raw map access — always go through the
  `StatBlock` API. Note: the alias-normalizing `StatBlock` lives in
  `crates/optimizer/src/stats.rs:29-70` (not in this crate — `core::StatBlock`
  in `types.rs` is a plain data struct with no alias logic).
- **`AppConfig` carries per-provider keys + models**, with `active_llm_key()`
  / `active_model_id()` routing through the `LlmProvider` enum.
  `is_setup_complete()` = `has_gw2_key()` ∧ `has_active_llm_key()` ∧
  cached-data-present. The addon-ui Setup driver depends on this exact
  predicate. (`crates/core/src/config.rs:254-265`.)
- **Atomic save: `<path>.tmp` → `rename`.** Used for both `AppConfig::save`
  and `BuildStorage::save_new` / `save_overwrite`. `test_crash_safe_save_new`
  and `test_backward_compat_old_config` lock this in.
- **Serde backward-compat via `#[serde(default)]`.** New fields on
  `AppConfig` and `SavedBuild` must default, never break existing user
  configs on disk. Enforced by `test_backward_compat_old_config` and
  `test_backward_compat_no_new_fields`.
- **`BuildLocks::describe_constraints()` renders human-readable constraint
  text** that the LLM tentacle injects into prompts verbatim. Changing its
  shape requires updating `llm-integration` prompt tests too.
- **`GameMode::ALL` = `[Pve, Pvp, WvW]`** — iteration order matters for some
  default lookups. Don't reorder without checking consumers.

## Conventions

- **Never `#[derive(Eq)]` types containing floats.** `StatBlock`,
  `CombatMetrics`, `DamageModifiers` use `f32`/`f64`. Compare via
  domain-specific helpers.
- **Fields default through `#[serde(default)]` helper fns** (e.g.
  `default_opacity`, `default_font_scale`) — this is how old configs keep
  working.
- **`BuildStorage::save_new` rejects collisions** (returns `Err` on existing
  filename); `save_overwrite` is the explicit overwrite path. Callers must
  choose.
- **`sanitize_filename` strips non-alphanumeric chars and lowercases.** Saved
  builds on disk are named after the build's user-facing name.
- Tests next to source. No global state in this crate, so parallel test
  execution is fine.

## Cross-Tentacle Contracts

- **addon-ui** reads `AppConfig` on startup, writes it after Settings
  changes, and compares `is_setup_complete()` to decide Setup-vs-Main routing.
- **optimizer-engine** consumes `ResolvedBuild`, emits `CombatMetrics` +
  `RotationBreakdown` back.
- **gw2-api-client** hydrates `ResolvedBuild` from API `Character` +
  `BuildTab` + `EquipmentTab` responses.
- **llm-integration** serializes `ResolvedBuild` into prompt context and
  parses LLM JSON back into `ResolvedBuild`-shaped data.

## Non-Goals

- No game-data logic. The 12 `data/` loaders live in `patch-aware-data`.
- No HTTP / networking. All I/O goes through `gw2-api-client` or LLM crates.
- No UI state. `MainState` / `AddonState` live in `addon-ui`.

<!-- octogent:suggested-skills:start -->
## Suggested Skills

You can use these skills if you need to.

- `code-review`
- `refactor`
<!-- octogent:suggested-skills:end -->
