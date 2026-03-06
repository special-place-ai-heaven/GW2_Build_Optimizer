# Story 3.11: PvP Optimizer Path Separation

Status: ready-for-dev

## Story

As a GW2 PvP player,
I want the optimizer to use the PvP amulet system instead of gear-prefix optimization when I'm optimizing for PvP,
so that my PvP build recommendations are based on the actual PvP stat system and not impossible PvE gear combinations.

## Non-Goals

- **No rune/sigil/relic/skill optimization for PvP** -- this story carries them as inputs, not optimized. Full PvP upgrade optimization needs P3-10b+ effect data.
- **No PvP-specific balance override data** -- P3-09 infrastructure handles those.
- **No WvW-specific behavior** -- that is P3-12.
- **No PvP amulet data files** -- PvP amulet stats come from the existing GW2 API cache (GameDb), not data files.
- **No new UI screens** -- PvP results use the existing output pipeline.

## Dependencies

- **P3-02 (done)** -- BalanceContext for mode dispatch.
- **P3-07 (done)** -- DataState/DataLoadError for Blocked gate.
- **Downstream**: P3-10b (effect data for PvP upgrade optimization).

## Acceptance Criteria

1. When `BalanceContext.game_mode == PvP`, optimizer routes to a distinct PvP path that bypasses gear-prefix search entirely.
2. PvP path optimizes over: PvP amulet selection + specialization/trait evaluation.
3. PvP amulets (`PvpAmulet`) are distinct from PvE/WvW trinket-slot amulets. Never conflated.
4. PvP amulet stats come from GameDb (`/v2/pvp/amulets` already cached). No new data files.
5. Each PvP amulet's stats replace gear stats (not add to them) in stat block.
6. Scoring uses BalanceContext-parameterized formulas (PvP-specific Fury=20% not 25%, etc.).
7. If no PvP amulet data available in GameDb, returns Blocked (not zero-stat fallback).
8. Result includes: selected PvP amulet + specs/traits + stat block + combat metrics + score.
9. BuildCandidate extended with optional `pvp_amulet` field (not `amulet`).
10. Rune/sigil/relic/skill carried through from input, not optimized.
11. Slot-budget data not loaded or consulted during PvP optimization.
12. `docs/optimizer-data-schemas.md` updated to note `/v2/pvp/amulets` as canonical PvP data source.

## Technical Context

### Current State

Read `crates/optimizer/src/engine.rs` and find `optimize_pvp`. Currently it's likely a placeholder or minimal implementation that uses an empty `GearCandidate`. This story replaces it with real PvP amulet-based optimization.

### PvP Amulet Data Source

The GW2 API has `/v2/pvp/amulets` which returns amulet objects with `id`, `name`, and `attributes` (a map of stat name to value). These are already cached in `GameDb` via `crates/gw2api/`. Check:
- `crates/gw2api/src/models.rs` for `PvpAmulet` type
- `crates/gw2api/src/db.rs` for `GameDb.pvp_amulets`

The `ResolvedPvpAmulet` type already exists in `crates/core/src/types.rs` with `id`, `name`, `stats` fields.

### PvP Optimization Flow

```
1. Check GameDb has pvp_amulets data → if empty, return Blocked
2. Get available specializations/traits (same as PvE path)
3. For each PvP amulet candidate:
   a. Apply amulet stats to StatBlock (replacing gear stats, since PvP has no gear)
   b. Add base stats (profession base health/defense via profession profiles)
   c. Score using PvP-mode combat performance
4. Return best-scoring combination
```

### Stat Application

PvP amulets provide flat stat values. In PvP mode, the stat block is:
- Base stats (1000 Power, 1000 Precision, etc. from universal formulas)
- + PvP amulet stats (directly from the amulet's `attributes` map)
- + Rune bonuses (if rune equipped, carried through)
- NO gear stats, NO slot budgets, NO attribute_adjustment calculations

### BuildCandidate Extension

Add to `BuildCandidate` (in engine.rs):
```rust
pub pvp_amulet: Option<PvpAmuletCandidate>,
```

Where:
```rust
pub struct PvpAmuletCandidate {
    pub id: u32,
    pub name: String,
    pub stats: HashMap<String, i32>,
}
```

### Mode Dispatch

In the optimizer entry point (likely `engine.rs`), dispatch based on game mode:
```rust
match ctx.game_mode {
    GameMode::PvP => optimize_pvp(db, profession, weights, ctx, ...),
    _ => optimize_with_gemini(db, profession, weights, ctx, ...) // PvE/WvW
}
```

## Tasks

- [ ] 1. Read current `optimize_pvp` implementation to understand what exists (AC: 1)
- [ ] 2. Read GameDb PvP amulet data types and access patterns (AC: 4)
- [ ] 3. Add `PvpAmuletCandidate` type and `pvp_amulet` field to BuildCandidate (AC: 9)
- [ ] 4. Implement real PvP amulet enumeration from GameDb (AC: 4, 7)
- [ ] 5. Implement PvP stat block calculation (amulet stats replace gear stats) (AC: 5, 11)
- [ ] 6. Wire PvP-mode scoring with BalanceContext (AC: 6)
- [ ] 7. Return Blocked if no PvP amulet data (AC: 7)
- [ ] 8. Ensure mode dispatch routes PvP to new path (AC: 1)
- [ ] 9. Update schema docs (AC: 12)
- [ ] 10. Add tests: PvP path selection, amulet stat application, Blocked on missing data, no slot budget usage (AC: 1, 5, 7, 11)

## Verification

```bash
cargo test --package gw2-optimizer -v
cargo check
```

## Dev Notes

- The PvP path can be much simpler than the PvE path since there's no gear search -- just iterate all PvP amulets and score each.
- PvP amulet stats use GW2 API attribute names (e.g., "Power", "Precision"). Use the existing `StatBlock.add()` which handles name normalization.
- Be careful: PvP mode uses different Fury values (20% crit instead of 25%). The existing boon formulas (P3-04) already handle this via `fury_crit_bonus(game_mode)`.
- The `ResolvedPvpAmulet` in core types already has `stats: HashMap<String, i32>`. This is the API-native format.
- Don't add P3-09's DataQuality to this story's output unless P3-09 has already landed. Use a simple error return for the Blocked case.

### References

- [Source: _bmad-output/planning-artifacts/epics.md, Story 3.11]
- [Source: crates/core/src/types.rs, ResolvedPvpAmulet]
- [Source: crates/gw2api/src/models.rs, PvpAmulet]
- [Source: crates/optimizer/src/engine.rs, optimize_pvp]
