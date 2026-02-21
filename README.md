# GW2 Build Optimizer

In-game Guild Wars 2 addon that optimizes character builds across all game modes (PvE, PvP, WvW). Uses the GW2 API for game/character data and Google Gemini for LLM-powered build reasoning.

Loaded via [Nexus](https://raidcore.gg/Nexus) addon manager.

## Features

- **Build Analysis**: Fetches your current character build from the GW2 API
- **Stat Calculation**: Computes full stat blocks from gear, runes, sigils, traits, and infusions
- **Optimization**: Deterministic gear/spec search scored by archetype (Power DPS, Condi DPS, Healer, Tank, Hybrid)
- **Gemini AI Reasoning**: Enriches suggestions with trait/sigil/rune/relic synergy analysis
- **Chat Refinement**: Ask follow-up questions to refine build suggestions
- **Comparison View**: Side-by-side current vs optimized build with stat diffs
- **Game Mode Support**: PvE, WvW (same gear system, competitive split context), PvP (amulet system)
- **Save/Load**: Persist and restore build suggestions

## Installation

1. Install [Nexus addon manager](https://raidcore.gg/Nexus) for Guild Wars 2
2. Build the DLL: `cargo build --release`
3. Copy `target/release/gw2_build_optimizer.dll` to your GW2 `addons/` directory
4. Launch GW2 — the addon loads automatically via Nexus

## First Run

1. Press the keybind (default: configured in Nexus) to open the optimizer window
2. **GW2 API Key**: Enter your key from [account.arena.net/applications](https://account.arena.net/applications) — needs `account`, `characters`, `builds` permissions
3. **Gemini API Key**: Enter your key from [aistudio.google.com/apikey](https://aistudio.google.com/apikey) (free tier: 250 requests/day)
4. **Data Download**: The addon downloads game data (~100k items, skills, traits, etc.) and caches it locally

## Usage

1. Select a character from the dropdown
2. Choose a game mode (PvE / WvW / PvP)
3. Go to **New Build** tab, pick an archetype, and click Optimize
4. Review the comparison view showing current vs suggested build
5. Use the chat bar to ask for refinements ("more sustain", "swap to condi", etc.)
6. Save builds you like in the **Save/Load** tab

## Building from Source

```bash
cargo build --release
# Output: target/release/gw2_build_optimizer.dll
```

Requires Rust 2021 edition. The workspace has 4 crates:
- `crates/addon` — Nexus DLL entry point, ImGui UI
- `crates/core` — Shared types, config, storage
- `crates/gw2api` — GW2 API client with rate limiting and cache
- `crates/optimizer` — Stat calc, scoring, search, Gemini integration

## Known Limitations

- PvP mode optimizes specializations/traits only (gear stats come from amulet, not from the optimizer)
- WvW competitive splits (reduced coefficients) are noted in Gemini context but not modeled numerically
- Stat calculation uses approximate attribute_adjustment values per slot type
- Gemini free tier limits to 250 requests/day and 10 requests/minute
- No support for Revenant legend swapping optimization yet

## License

MIT
