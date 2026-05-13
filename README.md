# GW2 Build Optimizer

GW2 Build Optimizer is an in-game Guild Wars 2 addon for Nexus. It reads your
characters through the official GW2 API, compares your current build, and suggests
optimized builds for PvE, WvW, and PvP.

The addon can generate new builds, improve your current build, explain tradeoffs,
show viability checks, and copy a GW2 build-template chat code for the optimized
result.

## Download

Download the prebuilt DLL from GitHub Releases:

- [Latest release](https://github.com/special-place-administrator/GW2_Build_Optimizer/releases/latest)
- [Direct DLL download](https://github.com/special-place-administrator/GW2_Build_Optimizer/releases/latest/download/gw2_build_optimizer.dll)
- [Checksum file](https://github.com/special-place-administrator/GW2_Build_Optimizer/releases/latest/download/SHA256SUMS.txt)

Current release DLL SHA256:

```text
8DA076F161D9D84EB25C7723616AE16F13CC03030D1E007917AC856B12E82433
```

If you build from source, the DLL is generated at:

```text
target/release/gw2_build_optimizer.dll
```

## Requirements

- Guild Wars 2 on Windows.
- [Nexus addon manager](https://raidcore.gg/Nexus).
- Nexus GitHub project: [RaidcoreGG/Nexus](https://github.com/RaidcoreGG/Nexus).
- A GW2 API key from ArenaNet.
- An AI provider API key for the setup wizard. Supported providers are Google Gemini,
  OpenAI, Anthropic, and OpenRouter.

## Install for Players

1. Install Nexus first.
   - Use the Nexus website: <https://raidcore.gg/Nexus>
   - The source project is here: <https://github.com/RaidcoreGG/Nexus>
   - Launch GW2 once and make sure the Nexus menu appears in game.

2. Download `gw2_build_optimizer.dll`.
   - Use the DLL in this repo under `release/`.
   - Do not download the Rust source code if you only want to play.

3. Copy the DLL into your GW2 addons folder.
   - Example:

```text
C:\GAMES\Guild Wars 2\addons\gw2_build_optimizer.dll
```

   - If your GW2 install is elsewhere, use that install's `addons` folder.
   - The DLL should be directly inside `addons`, not inside a nested folder.

4. Start or restart Guild Wars 2.
   - Nexus loads the addon on game startup.
   - If GW2 was already open, fully restart it after copying the DLL.

## First-Time Setup

The addon has a setup wizard the first time it opens.

### 1. Create a GW2 API Key

Go to:

<https://account.arena.net/applications>

Create a new key and enable these scopes:

- Required: `account`, `characters`, `builds`
- Recommended: `inventories`, `unlocks`

> [!IMPORTANT]
> When ArenaNet asks which permissions to include, select these checkboxes:
> `account`, `characters`, `builds`, `inventories`, and `unlocks`.
> The first three are required. `inventories` and `unlocks` let the optimizer
> see more of what your account owns so its suggestions are more useful.

Paste the key into the addon when asked.

### 2. Add an AI Provider Key

Pick one provider in the addon setup screen and paste its key.

- Gemini: <https://aistudio.google.com/apikey>
- OpenAI: <https://platform.openai.com/api-keys>
- Anthropic: <https://console.anthropic.com/settings/keys>
- OpenRouter: <https://openrouter.ai/keys>

Gemini is the default and is usually the easiest option for casual setup.

### 3. Download Game Data

The addon downloads professions, traits, skills, items, runes, sigils, relics,
legends, PvP amulets, and other game data. This can take a little while the first
time. It is cached locally after download.

When Guild Wars 2 patches, use the addon settings to refresh game data if build
or skill data looks wrong.

## What the Addon Does

### Character Build Reading

The addon loads your selected character's current build and equipment from the
GW2 API. It resolves traits, skills, gear, runes, sigils, relics, and stats into
a readable in-game view.

### Optimize a New Build

Use the New Build tab when you want the addon to create a build for a role. Pick
a mode and role, then run optimization. Example goals include power DPS, condi
DPS, sustain, healer, boon support, WvW roaming, WvW zerg DPS, and PvP burst.

### Improve Current Build

Use the Improve tab when you already have a build equipped and want the addon to
improve it. You can lock parts of the current build so the optimizer keeps them.
This is useful if you want to keep an elite specialization, trait line, or build
identity while improving the weaker pieces.

### Copy the Optimized Chat Code

Optimized results include a GW2 build-template chat code when the required skill
and trait IDs are known. Click `Copy`, paste the code into GW2 chat, and use the
in-game build-template UI from there.

### Viability Checks

For competitive modes, the addon checks whether a build is viable before treating
it as a good result. WvW and PvP builds are checked for things like stunbreaks,
Stability access, condition cleanse, and effective health.

### Combat and Stat Comparison

The comparison view shows:

- Current build vs optimized build.
- Primary stat changes.
- Strike damage, condition damage, healing, boon support, control, and sustain.
- Solo, party, and squad-style combat assumptions.
- Rotation and utility signals when available.
- Data quality warnings if a result depends on incomplete data.

### Chat Refinement

After a result, use the chat box to ask for follow-up changes, such as:

- "Make this tankier."
- "Keep sword/shield."
- "More condition cleanse."
- "Use this for WvW roaming."

The deterministic optimizer and referee checks remain the authority; AI text is
used as an assistant for explanation and refinement.

### Save and Load Builds

Save builds you like from the result screen. Saved builds can be loaded later from
the Save/Load tab.

## Settings

Use Settings for:

- GW2 API key validation.
- AI provider selection and model choice.
- Provider key testing.
- Game-data refresh after GW2 patches.
- Benchmark sync for community-meta comparisons.
- UI preferences such as opacity, font scale, and layout sizing.

## Troubleshooting

### The addon does not appear in game

- Make sure Nexus is installed and working first.
- Make sure the DLL is named exactly `gw2_build_optimizer.dll`.
- Make sure it is directly in the GW2 `addons` folder.
- Restart Guild Wars 2 after copying the DLL.

### The setup wizard says the GW2 key is missing scopes

Create a new ArenaNet API key with at least:

```text
account, characters, builds
```

The addon also recommends:

```text
inventories, unlocks
```

### The addon shows stale or strange skill data after a GW2 patch

Open Settings and refresh game data. GW2 patches can change skill, trait, item,
or legend IDs and balance data.

### Optimization gives no useful result

- Confirm the selected game mode and role are correct.
- Refresh game data.
- Try fewer locks in Improve Build.
- For WvW/PvP, make sure the selected role matches what you actually want to play.

### AI requests fail

- Test the provider key in Settings.
- Check provider rate limits.
- Try a smaller or cheaper model if your provider supports model selection.

## Build from Source

Install Rust, then run:

```bash
cargo build --release
```

The built DLL will be:

```text
target/release/gw2_build_optimizer.dll
```

Useful checks for contributors:

```bash
cargo test --workspace --all-targets
cargo check
cargo clippy --workspace --all-targets
```

Workspace crates:

- `crates/addon` - Nexus DLL entry point and ImGui UI.
- `crates/core` - Shared config, storage, and types.
- `crates/gw2api` - GW2 API client, cache, and endpoint models.
- `crates/optimizer` - Build search, combat math, viability checks, and AI integrations.

## Known Limits

- The optimizer is a recommendation tool. You still need to test the build in
  actual GW2 content.
- Some conditional trait and skill effects are approximated.
- Chat-code generation can only include build-template data, not complete gear.
- Saved builds are addon records, not official GW2 account templates.
- AI provider limits and pricing depend on the provider you choose.

## License

MIT
