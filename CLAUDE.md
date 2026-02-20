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
crates/addon/     — cdylib: Nexus entry point, ImGui UI, keybinds (nexus-rs)
  src/lib.rs      — export! macro, on_load/on_unload, keybind + render registration
  src/state.rs    — global AddonState (Mutex<Option<T>>), window visibility toggle
  src/ui/mod.rs   — ImGui render fn, Window::new() conditional display
crates/core/      — Shared types (types.rs), config (config.rs), storage (storage.rs)
crates/gw2api/    — GW2 API v2 client (client.rs), serde models (models/), local cache (cache.rs)
crates/optimizer/ — engine.rs (pipeline orchestration), gemini.rs (LLM client), scoring.rs, search.rs, stats.rs
```

**Key dependency**: `nexus` crate from [nexus-rs](https://github.com/Zerthox/nexus-rs) — provides Nexus addon API bindings with ImGui (via `imgui-rs`), keybinds, events, logging.

### GW2 API Models (`crates/gw2api/src/models/`)

Each file maps to one API endpoint family. All structs derive `Debug, Clone, Serialize, Deserialize`.

| File | API Endpoint | Key Types |
|------|-------------|-----------|
| `characters.rs` | `/v2/characters` (auth) | `Character`, `BuildTab`, `Build`, `EquipmentTab`, `EquipmentPiece`, `EquipmentPvp` |
| `facts.rs` | shared | `Fact` (18-variant tagged enum), `TraitedFact` |
| `items.rs` | `/v2/items` | `Item`, `ItemDetails` (untagged enum), `ArmorDetails`, `WeaponDetails`, `UpgradeDetails`, `InfixUpgrade` |
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

Full plan at `~/.claude/plans/reflective-churning-quail.md`. Sprint format: S##-T##.

| Sprint | Status | Focus |
|--------|--------|-------|
| S01 | DONE | Project scaffolding, minimal Nexus addon |
| S02 | DONE | GW2 API data models (serde structs) |
| S03 | TODO | API client, rate limiter, local cache |
| S04 | TODO | Setup wizard UI (API keys + data download) |
| S05 | TODO | Character loading & current build display |
| S06 | TODO | Stat calculation engine |
| S07 | TODO | Optimization engine |
| S08 | TODO | Gemini LLM integration |
| S09 | TODO | Comparison view & results UI |
| S10 | TODO | Polish, testing, release prep |

<!-- AUTO-MANAGED: conventions -->
## Conventions

- Rust 2021 edition, workspace dependencies in root `Cargo.toml`
- `cdylib` crate type for addon DLL output
- nexus-rs macros: `export!`, `render!`, `keybind_handler!`
- Nexus addon signature: unique negative i32 derived from addon name (e.g. `"GW2B"` as hex, negated = `-0x47573242`)
- Global state via `Mutex<Option<T>>` static (Nexus runs single-threaded but callbacks need Send)
- ImGui windows via `Window::new().build(ui, || { ... })` with `Condition::FirstUseEver` for initial sizing
- Keybind IDs are SCREAMING_SNAKE_CASE strings (e.g. `"GW2_BUILD_OPT_TOGGLE"`)
<!-- END AUTO-MANAGED -->

<!-- AUTO-MANAGED: patterns -->
## Detected Patterns

- **Stub module pattern**: unimplemented files contain a single comment `// <purpose>.\n// Will be populated in S##.` — do not add placeholder code
- **Workspace dep hoisting**: all shared deps (serde, serde_json, reqwest) declared once in root `[workspace.dependencies]`, crates reference with `.workspace = true`
- **State accessor pattern**: global state exposed via free functions (`init`, `toggle_window`, `is_window_visible`) rather than direct static access
- **Crate internal visibility**: `mod state; mod ui;` kept private in addon; `pub mod` used in library crates (core, gw2api, optimizer)
- **Tagged enum serde**: `#[serde(tag = "type")]` on `Fact` — API `"type"` field selects the variant; all variants carry `text` and `icon` plus variant-specific fields
- **Untagged enum serde**: `#[serde(untagged)]` on `ItemDetails` — variant selected by field presence, not a discriminator field
- **Flattened embed**: `#[serde(flatten)]` on `TraitedFact.fact` — inlines `Fact` fields directly into the parent JSON object
- **Shared fact module**: `facts.rs` re-exported selectively (`Fact`, `TraitedFact`) and imported by `skills.rs` and `traits.rs` via `use super::facts::{Fact, TraitedFact}`
- **Inline model tests**: each model file has a `#[cfg(test)] mod tests` block with representative JSON payloads exercising deserialization edge cases
<!-- END AUTO-MANAGED -->
