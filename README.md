# GW2 Build Optimizer

In-game Guild Wars 2 addon that optimizes character builds across all game modes (PvE, PvP, WvW). Uses the GW2 API for game/character data and Google Gemini for LLM-powered build reasoning.

Loaded via [Nexus](https://raidcore.gg/Nexus) addon manager.

## Features

- **Build Analysis**: Fetches your current character build from the GW2 API (gear, traits, runes, sigils, relic)
- **Real Combat Performance Model**: Calculates strike DPS, condition DPS, healing output, boon/condi durations, and survivability using GW2's published formulas — not just raw stat weights
- **3-Tier Buff Profiles**: Solo (no buffs), Party (Might x15, Fury), Full Squad (Might x25, Fury, Vulnerability x25)
- **Condition Damage Breakdown**: Per-condition tick damage (Bleeding, Burning, Poison, Torment, Confusion) using wiki-accurate formulas
- **Damage Modifier Extraction**: Parses percentage modifiers from traits, runes, and sigils (e.g. Sigil of Force +5%, trait damage bonuses)
- **Optimization Engine**: Deterministic gear/spec search scored by combat output across 7 archetypes (Power DPS, Condi DPS, Sustain Hybrid, Tank, Boon Support, Heal Support, Celestial Hybrid)
- **Gemini AI Reasoning**: Enriches suggestions with trait/sigil/rune/relic synergy analysis
- **Chat Refinement**: Ask follow-up questions to refine build suggestions
- **Comparison View**: Side-by-side current vs optimized build with stat diffs and combat metrics across all 3 buff tiers
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
- `crates/optimizer` — Stat calc, combat performance model, scoring, search, Gemini integration

## Combat Performance Model

The optimizer uses GW2's published combat formulas to score builds by actual combat output:

- **Strike DPS Index**: `Effective Power * Vulnerability / Reference Armor` where Effective Power = `Power * (1 + CritChance * (CritDmg% - 1))` with all trait/sigil/rune modifiers applied multiplicatively
- **Condition DPS Index**: Sum of per-condition tick damage weighted by duration — e.g. Burning = `0.155 * CondDmg + 131` per tick
- **Scoring**: Archetypes are scored by combat output (not raw stat weights), so Berserker correctly beats Celestial for Power DPS and Viper beats Berserker for Condition DPS

## Known Limitations

- PvP mode optimizes specializations/traits only (gear stats come from amulet, not from the optimizer)
- WvW competitive splits (reduced coefficients) are noted in Gemini context but not modeled numerically
- Stat calculation uses approximate attribute_adjustment values per slot type
- Damage modifier extraction covers common patterns; complex conditional modifiers (e.g. "while above 90% HP") are noted but not dynamically modeled
- Gemini free tier limits to 250 requests/day and 10 requests/minute
- No support for Revenant legend swapping optimization yet

## License

MIT
