# Synergy-Driven Build Optimization Design

**Date**: 2026-02-22
**Status**: Approved
**Goal**: Replace stat-only optimization with intelligent synergy-driven build selection

## Problem

The current optimizer is a stat calculator — it picks gear prefixes and traits by number-crunching, producing builds that maximize raw attributes. It does NOT recommend:
- Heal/utility/elite skill selection
- Weapon set choices
- Sigils (per weapon)
- Rune SET (6-piece bonus)
- Relic

The Gemini enrichment pipeline that was supposed to fill these gaps is broken — the "Optimized Build" UI shows only traits and gear prefix, with everything else empty. Even if it worked, the approach is backwards: bolting on skills/gear after stat optimization ignores that **synergies between traits, skills, sigils, runes, and relics are what makes a build good**.

A Trapper rune is worthless without trap skills. A Sigil of Doom is worthless without poison-synergy traits. Eclipse trait is pointless without celestial avatar uptime from the right utility skills. Optimization IS about finding these synergies.

## Design

### Architecture: Hybrid (Rust Stats + Gemini Synergy)

**Rust (deterministic):**
- Gear prefix selection via hierarchical tier tables (already implemented)
- Gather and format ALL profession data from GameDb into structured context
- Validate Gemini's output (all items exist, traits valid, skills available)
- Calculate final stats, combat performance (3-tier), rotation simulation

**Gemini (synergy reasoning):**
- Receives pre-computed comprehensive context (~40-50K tokens) with ALL data
- Has tools available for verification/deep-dives if needed
- Reasons about synergy chains across all build components
- Produces a complete build with every slot filled and synergy explanation

### Pipeline

```
User sets radar weights + game mode
         |
Rust: tier-based gear prefix selection (existing)
Rust: build comprehensive context from GameDb
         |
Gemini receives:
  1. Rich context (ALL profession data — traits, skills, runes, sigils, relics)
  2. Engineered prompt (synergy reasoning, GW2 build rules)
  3. Tools (still available for verification)
         |
Gemini reasons freely -> produces complete build
         |
Rust: validate all items exist in GameDb
Rust: calculate final stats + combat performance
Rust: simulate rotation
         |
Display complete build with synergy explanation
```

### Pre-computed Context

Rust builds a structured text document from GameDb containing:

1. **Profession info**: available specs, weapon types
2. **ALL specializations**: each spec with minor traits, and Adept/Master/Grandmaster choices — each trait includes full description + facts (buffs, conditions, stat adjustments, procs)
3. **ALL profession skills**: grouped by type (heal, utility, elite) and category (trap, glyph, survival, etc.) — each with description, cooldown, facts
4. **ALL Superior Runes** (~168): name + all 6 tier bonuses (the 6-piece bonus is the critical synergy piece)
5. **ALL Superior Sigils** (~98): name + effect description + cooldown
6. **ALL Relics** (~34): name + full effect description
7. **Radar weights**: what the user prioritizes (Power/Disable/Condition/Heal/Sustain)
8. **Game mode**: PvE/WvW/PvP (affects what matters — CC important in WvW, etc.)
9. **Gear prefixes**: from tier selection (stat foundation)
10. **Current build** (for Improve Character flow): what the player currently has equipped

Token estimate: ~40-50K tokens. Gemini 2.5 Flash has 1M context window — well within limits.

### Engineered Prompt

The prompt tells Gemini:

1. **What it's optimizing for** — radar weights, game mode, playstyle
2. **What makes a GW2 build good** — synergy chains, not just stats:
   - Traits that proc on conditions paired with skills that apply those conditions
   - Rune 6-piece bonuses that amplify the build's core mechanic
   - Sigils that trigger on weapon swap paired with swap-benefit traits
   - Relics that complete the synergy loop
3. **GW2 build rules**:
   - 3 specializations (slot 3 can be elite, slots 1-2 must be core)
   - 1 trait per column (Adept/Master/Grandmaster) per spec
   - Runes: ALWAYS 6 of the same type (for the set bonus)
   - Sigils: 1 per weapon (2 for 2-handed), different per weapon set
   - 1 heal skill, 3 utility skills, 1 elite skill
   - 2 weapon sets (or 1 for engineer/elementalist)
4. **The complete data** — all specs/traits/skills/runes/sigils/relics (pre-computed)
5. **How to think** — "For each choice, explain what it synergizes with. Build synergy chains: trait -> skill -> rune -> sigil -> relic."
6. **Output format** — strict JSON (see below)

### Gemini Output Format (Enforced)

```json
{
  "specializations": [
    {"name": "Druid", "elite": true, "traits": ["Cultivated Synergy", "Natural Stride", "Verdant Etching"]},
    {"name": "Nature Magic", "elite": false, "traits": ["Allies' Aid", "Evasive Purity", "Invigorating Bond"]},
    {"name": "Wilderness Survival", "elite": false, "traits": ["Taste for Danger", "Ambidexterity", "Poison Master"]}
  ],
  "weapons": {
    "set1": {"main_hand": "Staff", "off_hand": null},
    "set2": {"main_hand": "Axe", "off_hand": "Warhorn"}
  },
  "skills": {
    "heal": "Healing Spring",
    "utility1": "Seed of Life",
    "utility2": "Glyph of Rejuvenation",
    "utility3": "Vine Surge",
    "elite": "Spirit of Nature"
  },
  "rune": "Superior Rune of the Monk",
  "sigils": {
    "set1_main": "Superior Sigil of Water",
    "set1_off": "Superior Sigil of Transference",
    "set2_main": "Superior Sigil of Renewal",
    "set2_off": "Superior Sigil of Concentration"
  },
  "relic": "Relic of Karakosa",
  "gear_prefix": "Apothecary's",
  "gear_mix": "weapons: Celestial",
  "synergy_explanation": "Cultivated Synergy grants Might to allies when applying regeneration. Healing Spring pulses regeneration on each tick, triggering this trait repeatedly. Rune of the Monk amplifies outgoing healing per boon on the target — and the Might from Cultivated Synergy counts as a boon, creating a positive feedback loop. Sigil of Water triggers a heal on weapon swap, which builds Astral Force via the Celestial Being minor trait, enabling more frequent Celestial Avatar uptime for additional healing.",
  "changes": [
    {"slot": "Druid Adept", "from": "Blood Moon", "to": "Cultivated Synergy", "reason": "Synergizes with regen from Healing Spring — grants Might on regen application"},
    {"slot": "Heal", "from": "Troll Unguent", "to": "Healing Spring", "reason": "AoE regen pulses trigger Cultivated Synergy repeatedly"},
    {"slot": "Rune", "from": "Trapper", "to": "Monk", "reason": "Bonus healing per boon on target — amplified by the Might from Cultivated Synergy"},
    {"slot": "Sigils", "from": "Doom/Geomancy", "to": "Water/Transference", "reason": "Heal triggers build Astral Force via Celestial Being minor trait"},
    {"slot": "Relic", "from": "none", "to": "Karakosa", "reason": "Extends boon duration on heal, amplifying the regen->Might->healing loop"}
  ]
}
```

Every field required. No nulls. Each change has from/to/reason for actionable UI rendering.

### Validation Pipeline (Rust)

After parsing Gemini's JSON:

1. **Spec validation**: all spec names exist in GameDb, at most 1 elite, rest core
2. **Trait validation**: each trait belongs to the named spec, 1 per column (Adept/Master/Grandmaster)
3. **Skill validation**: all skills available to the profession, correct slot types (heal/utility/elite)
4. **Weapon validation**: weapon types available to the profession (respecting elite spec unlocks)
5. **Rune validation**: name matches a Superior rune in GameDb
6. **Sigil validation**: names match Superior sigils in GameDb
7. **Relic validation**: name matches a relic in GameDb
8. **Stat calculation**: compute StatBlock from gear prefix + trait bonuses + rune stats + sigil stats
9. **Combat simulation**: 3-tier (Solo/Party/Squad) CombatPerformance
10. **Rotation simulation**: 30s DPCT-optimal with selected skills

If any validation step fails: log specific failures, display partial result with warnings.

### Frontend: Comparison View Redesign

Side-by-side layout showing COMPLETE builds:

**Left column: Current Build**
- Specializations with selected traits
- Weapons (both sets)
- Skills (heal, 3 utilities, elite)
- Rune (with set bonus note)
- Sigils (per weapon set)
- Relic
- Gear prefix
- Stats
- Combat Performance (3 tiers)
- Rotation Breakdown

**Right column: Optimized Build** (identical structure)
- Same sections, all filled from Gemini output
- Diff highlighting: green for improvements, red for trade-offs

**Bottom: Synergy Explanation**
- Why This Build: Gemini's synergy reasoning
- Changes to Make: each change with from/to/reason (actionable checklist)

### Files to Create/Modify

| File | Change |
|------|--------|
| `crates/optimizer/src/context.rs` | **NEW** — `build_gemini_context()`: gathers ALL profession data from GameDb into structured text for Gemini prompt |
| `crates/optimizer/src/validation.rs` | **NEW** — Validate Gemini output against GameDb: specs, traits, skills, weapons, rune, sigils, relic |
| `crates/optimizer/src/prompts.rs` | Rewrite: synergy-focused prompt with pre-computed context, strict output format |
| `crates/optimizer/src/engine.rs` | New `optimize_with_gemini()` entry point using pre-computed context + single Gemini call |
| `crates/optimizer/src/gemini_tools.rs` | Keep all 18 tools available for Gemini verification calls |
| `crates/addon/src/ui/main_view.rs` | Wire new flow: build context -> Gemini call -> validate -> display |
| `crates/addon/src/ui/comparison.rs` | Redesign: full build display on both sides, synergy explanation, changes list |

### Tools (Kept Available)

All 18 existing tools remain available for Gemini to use if it wants to verify details:
- `get_trait_details` — drill into specific trait facts
- `find_synergies` — check traited_facts activation for a trait combo
- `get_build_synergy_report` — full synergy analysis
- `simulate_rotation` — validate rotation DPS
- `simulate_combat` — check combat performance under different buff profiles
- Others as needed

The difference: tools are now OPTIONAL verification, not REQUIRED data gathering.
