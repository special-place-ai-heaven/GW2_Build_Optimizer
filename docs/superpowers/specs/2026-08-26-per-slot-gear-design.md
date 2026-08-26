# Per-Slot Gear Prefixes — Design Spec

**Date:** 2026-08-26
**Status:** Approved direction (slot-vector rewrite, approach B), pending spec review
**Branch target:** `fix/foundations` → feature branch `feat/per-slot-gear`

## 1. Goal

Give the optimizer and the user the same gear flexibility the game gives a player: **every equipment piece carries its own single-stat prefix**, independently chosen, independently lockable, independently searchable — enabling true hybrid builds (Berserker's weapons, Cavalier's chest, Cleric's rings) at full granularity. No combined-stat pieces (no Celestial-style); each piece holds exactly one prefix, like the game.

## 2. Non-goals

- Combined-stat (multi-stat) pieces — out of scope by definition.
- Chat-code encoding of gear — GW2 build templates do not carry gear; unchanged.
- Relic stat prefixes — relics have no stat prefix; they stay a fixed selection.
- PvP gear slots — PvP keeps its existing model (amulet prefix + rune drive stats; other slots are normalized by the mode).

## 3. Canonical slot model

New enum in `crates/core/src/types.rs`:

```rust
pub enum GearSlot {
    // armor (6)
    Helm, Shoulders, Coat, Gloves, Leggings, Boots,
    // trinkets (7)
    Back, Accessory1, Accessory2, Amulet, Ring1, Ring2,
    // weapons (4) — a two-handed weapon fills Main; Off stays None
    WeaponSet1Main, WeaponSet1Off, WeaponSet2Main, WeaponSet2Off,
}
```

- `GearSlot::ALL: [GearSlot; 16]`, `Copy + Clone + PartialEq + Eq + Hash + Serialize + Deserialize` (serde as kebab-case strings for readable saves).
- Mapping to the existing `SlotType` budget enum (`data/slot_budgets.rs`): armor slots map 1:1; `Back/Accessory/Amulet/Ring` map to their budget types; `WeaponSet*Main` maps to `WeaponTwoHand` when the set's weapon is two-handed, else `WeaponOneHand`; `WeaponSet*Off` maps to `WeaponOneHand` and is skipped when the set is two-handed.
- **Relic is excluded** from the slot map (no stats). It remains what it is today.

## 4. Data model (breaking internally, compatible on disk)

### ValidatedBuild (optimizer)

Replace `gear_prefix: Option<ValidatedGearPrefix>` and
`gear_groups: ValidatedGearGroups` with:

```rust
pub gear_slots: GearSlots,
```

```rust
pub struct GearSlots {
    /// Indexed by `GearSlot::ALL` position. `None` = two-handed off-hand empty.
    pub map: [Option<ValidatedGearPrefix>; 16],
}
```

- `GearSlots::get(slot) -> Option<&ValidatedGearPrefix>`, `set(slot, prefix)`, `prefix_id(slot) -> Option<u32>`.
- No inheritance tiers: after migration, every non-empty slot holds its own prefix (full resolution happened once at load — the slot vector is the single source of truth).
- `ValidatedGearGroups` and the build-wide `gear_prefix` field are deleted.

### SavedBuild (disk, backward compatible)

`SavedBuild` gains:

```rust
#[serde(default)]
pub slot_prefixes: Option<SlotPrefixSave>,
```

where `SlotPrefixSave` maps kebab-case slot names → `{ itemstat_id, name }`.

**Load migration:** when `slot_prefixes` is `None`, the loader expands the legacy
fields exactly as the game would have distributed them:

- legacy `gear_prefixes.armor` (or `stat_prefix`) → all 6 armor slots
- legacy `.trinkets` → Back, Accessory1, Accessory2, Amulet, Ring1, Ring2
- legacy `.weapons` → WeaponSet1Main and WeaponSet2Main (off-hands None)

After any save, `slot_prefixes` is always written; legacy fields continue to be
written for one release as a downgrade path, then removed.

### ResolvedBuild / ResolvedGearPiece

Mirror the slot map; `resolution.rs` resolves each piece's prefix name against
the DB per slot. `GearPrefixGroups` is retained **only** as a legacy-deserialization
shape for old saves, marked `#[deprecated]`, and never constructed by new code.

## 5. Stat application

`calculate_validated_stats` (engine) iterates `GearSlot::ALL`:

1. Skip `None` slots (two-hander off-hand).
2. Skip non-stat contributions in PvP mode **except** the Amulet slot, which
   continues to drive `match_pvp_amulet` (existing PvP behavior, now keyed by
   slot).
3. Otherwise: `add_budget_stats_for_itemstat(stats, itemstat_for(prefix_id),
   budget_for(slot_type))` — the per-slot budget JSON already models each
   slot's cheaper/more expensive budgets and major/minor shapes.

Because every slot resolves independently, mixed totals are exact: a
Berserker's helm adds Berserker's helm budget; a Cavalier's chest adds
Cavalier's chest budget.

## 6. Search (beam operators)

Replace `swap_gear_prefix` / `swap_gear_groups` with three operators over the
slot vector. All respect `BuildLocks.gear_locks` (new: `HashMap<GearSlot, u32>`,
slot → required itemstat id; locked slots are never mutated by any operator):

1. **uniform-all** — set every unlocked, non-empty slot to prefix P, for the
   top-10 radar-prioritized prefixes.
2. **per-group** — set all unlocked slots of one group (armor / trinkets /
   weapons-groups derived from the slot enum) to prefix P, top-10 each → 30.
3. **per-slot** — one slot × prefix, top-4 radar-prioritized prefixes → 64
   combinations, **rotated deterministically**: member generation `g` exposes
   slots where `(slot_index + g) % 16 < 10`, so every slot surfaces across
   beam generations while the per-member neighbor budget stays inside the
   existing ~80 cap. Ordering is `(slot_index, prefix_priority, itemstat_id)`
   — the same determinism discipline the current neighbors use.

Candidate identity/dedup: the slot map serializes into the candidate key
(replacing the group-based identity).

## 7. LLM / Choya surface

- `GeminiBuildResponse` (the plate) gains an optional per-slot map using the
  same kebab-case slot names; values are prefix **names**, resolved against
  `db.itemstats` with the normalized-name matcher already used by the scraper.
  Unknown names fall back to the weight-profile prefix for that slot.
- `parse_gemini_build` accepts builds with or without the map (old plates
  keep working; a plate without gear keeps today's profile-prefix behavior).
- Gear-guidance prompt text stays profile-level (unchanged).
- The `gear_prefix` calculator/simulator tools in `gemini_tools.rs` keep
  single-prefix semantics (they are calculators, not builders).

## 8. UI

- **Gear sheet** (`gear_sheet.rs`): the per-piece rows already exist — each row
  now displays its own slot's prefix (from the slot map) instead of the group
  name. Locked pieces show their lock.
- **Locks panel** (`lock_panel.rs`): new "Gear" section listing the 16 slots;
  each can be locked to its current prefix (or set + locked). Locked gear is
  respected by Optimize/Improve/Choya alike.
- **Comparison / optimized view**: shows the winning per-slot mix (same per-piece
  rows).
- **Saves UI**: unchanged (loads/saves transparently through the migration).

## 9. Error handling

- Unknown prefix names in plates/saves: fall back to the weight-profile prefix
  for that slot and record a validation warning (existing warning channel from
  L3.1 renders it).
- A slot map containing an itemstat id absent from the DB: validation error on
  that slot (build marked provisional, per existing provisional-build rules).
- Two-handed sets with a non-None off-hand: loader drops the off-hand entry
  with a warning.

## 10. Testing

- **Migration**: legacy save (uniform + grouped variants) → slot map → byte-equal
  stats vs the old path; old save files from real usage in fixtures.
- **Stat math**: hand-computed totals for mixed builds (e.g., Berserker's helm +
  Cavalier's chest) against `slot_budgets` fixtures.
- **Search**: same weights → same slot vector (determinism); locked slots never
  change across beam generations; rotation covers all 16 slots over generations.
- **Plates**: parse per-slot maps (full, partial, unknown names).
- **UI data**: gear sheet row → correct per-piece prefix name.

## 11. Implementation order (layers, each green before the next)

1. Core types: `GearSlot`, `GearSlots`, `SavedBuild` field + load migration
   (+ tests).
2. Optimizer model: `ValidatedBuild.gear_slots`, per-slot validation,
   per-slot stat application (+ math tests).
3. Search: three operators + identity + lock respect (+ determinism tests).
4. LLM: plate fields + parse + apply (+ tests).
5. UI: gear sheet, locks panel, comparison (+ addon tests).
6. Legacy cleanup: delete `gear_prefix`/`gear_groups`/`GearPrefixGroups`
   construction paths, keep legacy save deserialization.
