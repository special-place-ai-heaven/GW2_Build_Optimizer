# Story 3.06: Canonical Slot-Budget Dataset

Status: ready-for-dev

## Story

As a GW2 player,
I want the optimizer's gear search to use verified stat budgets per equipment slot,
so that stat comparisons between gear prefixes are based on real item data, not fabricated constants.

## Non-Goals

- **No runtime wiring** -- this story delivers the data file and loader only. Replacing the hardcoded `attribute_adjustment_for_slot()` and `SLOT_ADJUSTMENTS` constants with loaded data is P3-07 scope.
- **No PvP amulet data** -- PvP amulets use a completely different stat system (flat attributes, no slot budgets). That is P3-11.
- **No exotic-rarity budgets** -- the current optimizer targets Ascended gear exclusively. Exotic budgets are out of scope.
- **No generic loader infrastructure** -- this story delivers a concrete loader for slot budgets only (same pattern as P3-01). Generic loader traits are P3-07.
- **No BalanceContext plumbing** -- slot budgets are mode-invariant (same values in PvE/PvP/WvW).

## Dependencies

- **P3-01 (done)** -- establishes the `crates/optimizer/src/data/` module, `include_str!` + `OnceLock` pattern, and `EvidenceLevel` enum.
- **No dependency on P3-02** -- slot budgets are mode-invariant.
- **Downstream**: P3-07 (typed loaders + runtime wiring) consumes this dataset to fully resolve D5.

## Acceptance Criteria

1. `data/slot_budgets/level80_ascended.json` exists with entries for every equipment slot type: helm, shoulders, coat, gloves, leggings, boots, one-handed weapon, two-handed weapon, amulet, accessory, ring, back item (12 slot types).
2. Each slot entry declares stat values for ThreeStat and FourStat shapes. CelestialLike shape included if current search supports Celestial prefixes (it does -- Celestial is always in the candidate pool).
3. Values are final integer stat modifiers per slot and stat shape -- NOT raw `attribute_adjustment` values. Verified on representative ascended items via `API:2/items`.
4. Primary source: concrete `API:2/items` item IDs (e.g., Zojja's Blade for 1H Berserker). Wiki is secondary confirmation. The data file records derivation item IDs.
5. ThreeStat spot-checks: 1H weapon major=125, minor=90. 2H weapon major=251, minor=179. Amulet major=157, minor=108.
6. Armor stat budgets are the same across weight classes (Heavy/Medium/Light) -- weight only affects defense, not attribute bonuses.
7. Loader validates: no missing slots (all 12 required), no duplicate slot+shape combos, no zero values. Returns typed `SlotBudgetError`.
8. `evidence_level: "Factual"` with `sources` citing API item IDs used for derivation.
9. D5 is NOT fully resolved by this story -- runtime consumption is P3-07.
10. GR-2: test expected values cite specific `API:2/items` item IDs in test comments.

## Verification

```bash
# Run optimizer crate tests (loader + validation + spot-check tests)
cargo test --package gw2-optimizer -v

# Verify data file exists and has entries for all 12 slots
cat data/slot_budgets/level80_ascended.json | python -c "
import json, sys
d = json.load(sys.stdin)
slots = {e['slot'] for e in d['entries']}
expected = {'Helm','Shoulders','Coat','Gloves','Leggings','Boots',
            'WeaponOneHand','WeaponTwoHand','Amulet','Accessory','Ring','BackItem'}
missing = expected - slots
assert not missing, f'Missing slots: {missing}'
print(f'OK: {len(d[\"entries\"])} entries covering {len(slots)} slots')
"

# Verify no zero values in any entry
cat data/slot_budgets/level80_ascended.json | python -c "
import json, sys
d = json.load(sys.stdin)
for e in d['entries']:
    assert e['major'] > 0, f'{e[\"slot\"]} {e[\"shape\"]} has zero major'
    assert e['minor'] > 0, f'{e[\"slot\"]} {e[\"shape\"]} has zero minor'
print('OK: no zero values')
"

# Verify loader module compiles and is registered
grep 'slot_budgets' crates/optimizer/src/data/mod.rs
```

## Tasks / Subtasks

### T1: Create `data/slot_budgets/level80_ascended.json` (AC: 1, 2, 3, 4, 5, 6, 8)

- [ ] Create `data/slot_budgets/` directory
- [ ] Create JSON file with top-level metadata: `rarity`, `level`, `sources`, `derivation_items`
- [ ] Add ThreeStat entries for all 12 slot types with verified final stat values
- [ ] Add FourStat entries for all 12 slot types with verified final stat values
- [ ] Add CelestialLike entries for all 12 slot types (Celestial is always in the candidate pool per `select_prefixes_by_tiers()` at `crates/optimizer/src/scoring.rs:444`)
- [ ] Verify armor slot values are identical across weight classes (AC: 6)
- [ ] Record derivation item IDs in the data file (AC: 4)

### T2: Create `crates/optimizer/src/data/slot_budgets.rs` loader (AC: 7)

- [ ] Define `SlotBudgetEntry` struct: `slot`, `shape` (enum), `major`, `minor`, `evidence_level`
- [ ] Define `StatShape` enum: `ThreeStat`, `FourStat`, `CelestialLike`
- [ ] Define `SlotBudgets` wrapper with `HashMap<(SlotType, StatShape), SlotBudgetEntry>` for O(1) lookup
- [ ] Define `SlotType` enum: `Helm`, `Shoulders`, `Coat`, `Gloves`, `Leggings`, `Boots`, `WeaponOneHand`, `WeaponTwoHand`, `Amulet`, `Accessory`, `Ring`, `BackItem`
- [ ] Define `SlotBudgetError` typed error enum: `ParseError`, `ValidationError(String)`
- [ ] Implement `load_slot_budgets(json: &str) -> Result<SlotBudgets, SlotBudgetError>`
- [ ] Implement `validate_slot_budgets()`: all 12 slots present for each shape, no duplicates, no zero values
- [ ] `OnceLock<SlotBudgets>` + `include_str!("../../../../data/slot_budgets/level80_ascended.json")` pattern (same as P3-01)
- [ ] Public `slot_budgets()` function for global access
- [ ] Lookup helpers: `fn get(&self, slot: SlotType, shape: StatShape) -> Option<&SlotBudgetEntry>`

### T3: Register module in `crates/optimizer/src/data/mod.rs` (AC: 7)

- [ ] Add `pub mod slot_budgets;` to `crates/optimizer/src/data/mod.rs`
- [ ] Add `pub use slot_budgets::SlotBudgets;` re-export

### T4: Write tests with source citations (AC: 5, 7, 10)

- [ ] `test_embedded_slot_budgets_load_successfully` -- validates the embedded JSON parses and passes validation
- [ ] `test_three_stat_1h_weapon_values` -- major=125, minor=90, cites API item ID (e.g., Zojja's Blade item 46762)
- [ ] `test_three_stat_2h_weapon_values` -- major=251, minor=179, cites API item ID (e.g., Zojja's Claymore item 46774)
- [ ] `test_three_stat_amulet_values` -- major=157, minor=108, cites API item ID (e.g., item 81908)
- [ ] `test_three_stat_accessory_values` -- major=110, minor=74, cites API item ID
- [ ] `test_three_stat_ring_values` -- major=126, minor=85, cites API item ID
- [ ] `test_three_stat_back_item_values` -- major=63, minor=40, cites API item ID
- [ ] `test_three_stat_helm_values` -- cites API item ID
- [ ] `test_three_stat_coat_values` -- cites API item ID
- [ ] `test_armor_slots_weight_invariant` -- all armor slots have same stat values regardless of weight class (AC: 6)
- [ ] `test_four_stat_1h_weapon_values` -- cites API item ID (e.g., Viper's item)
- [ ] `test_celestial_1h_weapon_values` -- cites API item ID
- [ ] `test_all_12_slots_present_for_each_shape` -- exhaustive slot coverage
- [ ] `test_missing_slot_rejected` -- loader rejects file with fewer than 12 slots per shape
- [ ] `test_duplicate_slot_shape_rejected` -- loader rejects duplicate slot+shape entry
- [ ] `test_zero_value_rejected` -- loader rejects entry with major=0 or minor=0
- [ ] `test_malformed_shape_rejected` -- loader rejects unknown shape string

## Dev Notes

### Current Hardcoded Slot Budget Locations (3 duplicated sites)

The current codebase stores slot attribute adjustments as hardcoded constants in THREE separate locations. All three must be replaced by P3-07, but P3-06 only creates the data file and loader.

| Location | Constant/Function | Lines | Notes |
|----------|-------------------|-------|-------|
| `crates/optimizer/src/engine.rs` | `attribute_adjustment_for_slot()` | 382-398 | Match arm returning `f64` per slot name |
| `crates/optimizer/src/synergy_pipeline.rs` | `SLOT_ADJUSTMENTS` (pub const) | 573-580 | `&[(&str, f64)]` array, consumed at line 819 |
| `crates/optimizer/src/gemini_tools.rs` | `SLOT_ADJUSTMENTS` (private const) | 1229-1236 | Duplicate of synergy_pipeline's array |

All three sites use the same values and the same raw `attribute_adjustment` approach. They store the intermediate `attribute_adjustment` value, NOT the final stat values. The stat formula is:

```
final_stat = attribute_adjustment * multiplier + value
```

Where `multiplier` and `value` come from the `ItemStat` (prefix definition, e.g., Berserker's Power: multiplier=0.35, value=32).

### Key Architectural Difference: attribute_adjustment vs final stats

The current code stores **attribute_adjustment** per slot (e.g., Helm=141.0), which is then combined with per-prefix multipliers to get final stats. The P3-06 data file stores **final integer stat values** per slot per stat shape. This means:

- Current: `Helm attr_adj=141.0` + `Berserker Power mult=0.35, val=32` => `141*0.35+32 = 81.35 => 81`
- P3-06: `Helm ThreeStat major=63, minor=44` (pre-computed final values)

The data file eliminates the intermediate formula step for stat budget comparison. The `attribute_adjustment` values are still used by the runtime stat calculation engine (`stats::calculate_gear_stats()`) which reads from actual equipped items. P3-07 will decide how to bridge these two representations.

### Current attribute_adjustment Values (for reference/derivation)

| Slot | Current attr_adj | Source |
|------|-----------------|--------|
| Helm | 141.0 | `engine.rs:385` |
| Shoulders | 141.0 | `engine.rs:385` |
| Coat | 225.0 | `engine.rs:386` |
| Gloves | 141.0 | `engine.rs:385` |
| Leggings | 171.0 | `engine.rs:387` |
| Boots | 141.0 | `engine.rs:385` |
| WeaponA1/B1 (main/2H) | 251.0 | `engine.rs:389` |
| WeaponA2/B2 (off-hand) | 125.0 | `engine.rs:390` |
| Backpack | 63.0 | `engine.rs:392` |
| Accessory1/2 | 110.0 | `engine.rs:393` |
| Amulet | 157.0 | `engine.rs:394` |
| Ring1/2 | 126.0 | `engine.rs:395` |

### Expected ThreeStat Values Per Slot (Ascended, Level 80)

Derived from the GW2 stat formula: `final = round(attribute_adjustment * multiplier + value)`

For ThreeStat (e.g., Berserker's): major multiplier=0.35/value=32, minor multiplier=0.25/value=18.

| Slot | attr_adj | Major (0.35*adj+32) | Minor (0.25*adj+18) | Verification Item |
|------|----------|---------------------|---------------------|-------------------|
| Helm | 141 | 81 | 53 | Zojja's Visor (item 80248) |
| Shoulders | 141 | 81 | 53 | Zojja's Pauldrons (item 80131) |
| Coat | 225 | 111 | 74 | Zojja's Breastplate (item 80296) |
| Gloves | 141 | 81 | 53 | Zojja's Grips (item 80252) |
| Leggings | 171 | 92 | 61 | Zojja's Legguards (item 80281) |
| Boots | 141 | 81 | 53 | Zojja's Greaves (item 80205) |
| 1H Weapon | 125 | 125 | 90 | Zojja's Blade (item 46762) |
| 2H Weapon | 251 | 251 | 179 | Zojja's Claymore (item 46774) |
| Back Item | 63 | 54 | 34 | various crafted ascended backs |
| Accessory | 110 | 110 | 74 | various ascended accessories |
| Amulet | 157 | 157 | 108 | various ascended amulets |
| Ring | 126 | 126 | 85 | various ascended rings |

**Note on weapons/trinkets**: For weapons and trinkets, the attribute_adjustment IS the major stat value (multiplier=1.0, value=0 for major; multiplier varies for minor). For armor, the formula uses the standard ThreeStat multipliers. The data file stores the **final rounded integer** regardless of which formula path produced it. The dev agent MUST verify each value against a concrete API item before committing.

**Note on back items**: Back item attr_adj=63 produces major=54 via (63*0.35+32=54.05=>54) for armor-style ThreeStat. However, back items may use trinket-style multipliers. The dev agent must verify against an actual ascended back item from the API.

### GW2 API Item IDs for Verification

The dev agent should query these items via `API:2/items` to verify the final stat values. These are representative Berserker's (ThreeStat) Ascended items:

| Slot | Item Name | Item ID | Notes |
|------|-----------|---------|-------|
| 1H Sword | Zojja's Blade | 46762 | Berserker's Ascended 1H |
| 2H Greatsword | Zojja's Claymore | 46774 | Berserker's Ascended 2H |
| Helm (Heavy) | Zojja's Visor | 80248 | Heavy Berserker's |
| Helm (Medium) | Zojja's Goggles | 80384 | Medium Berserker's -- verify same stats as Heavy |
| Helm (Light) | Zojja's Circlet | 80399 | Light Berserker's -- verify same stats as Heavy |
| Coat (Heavy) | Zojja's Breastplate | 80296 | Verify attr_adj=225 |

For FourStat verification, use Viper's items (CondDmg/Power/Precision/Expertise):

| Slot | Item Name | Item ID | Notes |
|------|-----------|---------|-------|
| 1H Weapon | Yassith's Razor | 76158 | Viper's Ascended 1H |
| 2H Weapon | Yassith's Claymore | 76453 | Viper's Ascended 2H |

For CelestialLike verification, use Celestial items (all 9 stats):

| Slot | Item Name | Item ID | Notes |
|------|-----------|---------|-------|
| 1H Weapon | Wupwup Blade | 72435 | Celestial Ascended 1H |

**Important**: The dev agent should fetch these items from the GW2 API and extract the actual stat values from `details.infix_upgrade.attributes` to populate the data file. Do NOT rely on formula derivation alone.

### Data File Schema (from `docs/optimizer-data-schemas.md` Schema 6)

```json
{
  "rarity": "Ascended",
  "level": 80,
  "entries": [
    {
      "slot": "WeaponOneHand",
      "shape": "ThreeStat",
      "major": 125,
      "minor": 90,
      "evidence_level": "Factual"
    },
    {
      "slot": "WeaponTwoHand",
      "shape": "ThreeStat",
      "major": 251,
      "minor": 179,
      "evidence_level": "Factual"
    }
  ],
  "sources": [
    "https://wiki.guildwars2.com/wiki/Attribute_combinations",
    "https://api.guildwars2.com/v2/items"
  ],
  "derivation_items": {
    "WeaponOneHand_ThreeStat": 46762,
    "WeaponTwoHand_ThreeStat": 46774,
    "Helm_ThreeStat": 80248,
    "Amulet_ThreeStat": 81908
  }
}
```

**Slot name convention**: Use normalized slot names (not runtime UI names like "WeaponA1"). Mapping:
- `Helm`, `Shoulders`, `Coat`, `Gloves`, `Leggings`, `Boots` (armor)
- `WeaponOneHand`, `WeaponTwoHand` (weapons)
- `Amulet`, `Accessory`, `Ring`, `BackItem` (trinkets)

The runtime slot names (`WeaponA1`, `WeaponB1`, `Accessory1`, `Ring1`, etc.) distinguish between equipped positions. The data file uses abstract slot TYPES because the stat budget is the same regardless of which slot position the item occupies (Ring1 and Ring2 have the same budget).

### Architecture Decisions

- **Loader location**: `crates/optimizer/src/data/slot_budgets.rs` alongside `profession_profiles.rs`. Follows the same `include_str!` + `OnceLock` pattern established by P3-01.
- **Data file location**: `data/slot_budgets/level80_ascended.json` per the architecture doc's directory layout.
- **Loading strategy**: Same as P3-01 (ADR-02) -- parse JSON once at startup into typed in-memory struct. No file I/O on hot paths.
- **Reuse existing enums**: Reuse `EvidenceLevel` from `profession_profiles.rs` if it is public, or define locally if not yet shared. P3-07 will consolidate shared types.
- **minor field**: The schema uses a single `minor` field (not `minor_1`/`minor_2`) because for ThreeStat and CelestialLike, both minor stats are equal. For FourStat, both minor stats are also equal (both secondaries get the same value). If a future stat shape has unequal minors, the schema can be extended.

### Guardrails

- **GR-1 (no heuristic contamination)**: This story is purely factual. Slot budgets are fixed by the game engine, not assumptions.
- **GR-2 (source verification)**: All test expected values must cite API item IDs in comments. The data file itself records derivation items.
- **GR-5 (status state principle)**: Not applicable to this story (no boon/condition modeling).

### What NOT to Change

- Do not modify `engine.rs` `attribute_adjustment_for_slot()` -- that replacement is P3-07.
- Do not modify `synergy_pipeline.rs` `SLOT_ADJUSTMENTS` -- that replacement is P3-07.
- Do not modify `gemini_tools.rs` `SLOT_ADJUSTMENTS` -- that replacement is P3-07.
- Do not modify condition formulas, boon values, or scoring logic -- those are other stories.
- Do not add PvP amulet data -- that is P3-11.

### Relationship to Current Code

The current gear search (`search.rs`) does NOT use per-slot stat values directly. It assigns itemstat IDs to slots and scores candidates via `calculate_candidate_stats()` in `engine.rs`, which applies the `attribute_adjustment_for_slot()` formula. The P3-06 data file provides a parallel truth source that P3-07 will wire in.

The three `SLOT_ADJUSTMENTS` / `attribute_adjustment_for_slot()` sites all produce the same result:

```rust
// engine.rs:368-372
let adj = attribute_adjustment_for_slot(slot);  // e.g., 141.0 for Helm
for attr in &itemstat.attributes {
    let value = adj * attr.multiplier + attr.value as f64;
    stats.add(&attr.attribute, value.round());
}
```

P3-07 can either: (a) replace `attribute_adjustment_for_slot()` with a lookup into loaded slot budgets (changing the formula to use pre-computed values), or (b) keep the formula approach but load `attribute_adjustment` from data instead of hardcoding. That design decision is deferred to P3-07.

## References

- [Source: docs/optimizer-source-of-truth.md Section 8] -- slot attribute budget requirements
- [Source: docs/optimizer-data-schemas.md Schema 6] -- JSON schema and validation rules
- [Source: _bmad-output/planning-artifacts/epics.md Story 3.6] -- epic-level AC and requirements
- [Source: _bmad-output/planning-artifacts/epics.md D5] -- defect: gear search uses local slot constants
- [Source: _bmad-output/planning-artifacts/epics.md FR8] -- functional requirement for slot-budget dataset
- [Source: _bmad-output/planning-artifacts/architecture.md] -- data directory layout and loader architecture
- [Source: crates/optimizer/src/engine.rs:382-398] -- `attribute_adjustment_for_slot()` hardcoded values
- [Source: crates/optimizer/src/synergy_pipeline.rs:573-580] -- `SLOT_ADJUSTMENTS` const array
- [Source: crates/optimizer/src/gemini_tools.rs:1229-1236] -- duplicate `SLOT_ADJUSTMENTS` const
- [Source: crates/optimizer/src/data/profession_profiles.rs] -- P3-01 loader pattern to follow
- [Source: crates/optimizer/src/scoring.rs:444] -- Celestial always included in candidate pool
- [Source: crates/gw2api/src/models/itemstats.rs] -- `ItemStat` and `StatAttribute` structs (formula inputs)
