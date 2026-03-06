# Story 3.08: Patch Manifest and Patch Ledger Infrastructure

Status: ready-for-dev

## Story

As a GW2 player,
I want the optimizer's data layer to track which game patch its balance data targets,
so that when a new GW2 patch lands, the addon can detect staleness and tell me whether its numbers are still verified for the current game build.

## Non-Goals

- **No DataQuality enum** -- `DataQuality::Provisional`/`Blocked` tiering is P3-09 scope.
- **No balance override data files** -- actual patch diff data files are P3-09.
- **No Unknown-value propagation** -- that is P3-09 scope.
- **No automatic quality downgrade** -- staleness detection is informational only; quality-downgrade behavior is P3-09.
- **No UI changes beyond informational indicator** -- no new tabs, panels, or screens.

## Dependencies

- **P3-01 through P3-06 (all done)** -- Phase A data files exist.
- **P3-07 (parallel)** -- P3-07 defines the typed loader infrastructure pattern. P3-08 can start in parallel but should follow the same `DataLoadError` pattern. If P3-07 lands first, P3-08 reuses its `DataLoadError` enum. If P3-08 lands first, define local error types that P3-07 can later unify.
- **Downstream**: P3-09 consumes manifests/ledgers for balance overrides and Unknown-value handling.

## Acceptance Criteria

1. `data/manifests/2026-01-13.json` exists as the initial patch manifest with fields: `patch_id` (ISO date = filename stem), `game_build_id` (integer from GW2 API `/v2/build`), `release_date`, `inherits_from` (null for baseline), `sources` (array with at least one `{kind, url}` entry), `supported_modes` (array of PvE/PvP/WvW), `status` ("active").
2. `data/patch_ledgers/2026-01-13.yaml` exists as the initial baseline ledger with: `patch_id`, `inherits_from: null`, `changes: []` (empty baseline).
3. Manifest loader validates: `patch_id` matches filename stem, `game_build_id` is positive integer, at least one source entry with URL.
4. Ledger loader validates: `patch_id` matches filename stem, every change entry has non-empty `source` URL, every `evidence_level` is valid enum variant.
5. Loaders return `Result<T, Vec<DataLoadError>>` with typed error variants (matching P3-07 pattern).
6. Manifest-set validation: no duplicate `patch_id` values, no two active manifests in same lineage, inheritance chain has no circular references, every referenced parent exists.
7. Staleness detection: compare live `/v2/build` integer (from existing `check_api_health`) against latest manifest's `game_build_id`. Mismatch emits informational indicator only.
8. Missing/corrupt manifest or ledger files result in `Degraded` state (not `Disabled`), since Phase A factual data is still usable.
9. `docs/optimizer-data-schemas.md` Schema 1 updated with `game_build_id: integer` field.
10. Ledger entries with `evidence_level: "Unknown"` are preserved in-memory for P3-09 consumption.
11. The `game_build_id` value in the initial manifest cites its source (wiki patch notes, API capture, or authoring baseline policy).

## Technical Context

### Manifest Schema (data/manifests/2026-01-13.json)

```json
{
  "patch_id": "2026-01-13",
  "game_build_id": 175218,
  "release_date": "2026-01-13",
  "inherits_from": null,
  "sources": [
    {
      "kind": "wiki",
      "url": "https://wiki.guildwars2.com/wiki/Game_updates/2026-01-14"
    }
  ],
  "supported_modes": ["PvE", "PvP", "WvW"],
  "status": "active"
}
```

Note: The `game_build_id` must be either the exact historical build number (if findable) or a documented baseline with authoring notes. Use GW2 API `/v2/build` to get the current live value as a reference point.

### Ledger Schema (data/patch_ledgers/2026-01-13.yaml)

```yaml
patch_id: "2026-01-13"
inherits_from: null
changes: []
# Baseline snapshot -- no prior diff. Future patches will have change entries like:
# - source_type: skill
#   source_id: 12345
#   source_name: "Fireball"
#   mode: PvE
#   field: "damage_coefficient"
#   old_value: "0.8"
#   new_value: "0.9"
#   evidence_level: Factual
#   source: "https://wiki.guildwars2.com/wiki/..."
```

### Staleness Detection Integration

The addon already has `check_api_health()` that periodically pings `GET /v2/build` (returns `{ "id": 175218 }`). The staleness check:

1. Load latest manifest (by `release_date` ordering)
2. Compare `manifest.game_build_id` with live `/v2/build` response
3. If mismatch: set informational flag in `AddonState` or `MainState`
4. UI shows "Balance data may be outdated (game build XXXXX, data verified for YYYYY)" or similar

This does NOT trigger DataQuality changes (that's P3-09).

### Loader Pattern (from P3-01 through P3-06)

All existing loaders use:
```rust
use std::sync::OnceLock;

static INSTANCE: OnceLock<T> = OnceLock::new();

pub fn accessor() -> &'static T {
    INSTANCE.get_or_init(|| {
        let json = include_str!("../../../data/path/file.json");
        let data: T = serde_json::from_str(json).expect("embedded data must parse");
        data.validate().expect("embedded data must validate");
        data
    })
}
```

For manifests, the pattern differs slightly because there can be multiple manifest files. Since we use `include_str!`, we embed the initial manifest at compile time. Future manifests would need a different loading strategy (external file reads at runtime), but for this story, a single embedded baseline manifest is sufficient.

### Where to Put Code

- `crates/optimizer/src/data/manifests.rs` -- manifest types, loader, validation
- `crates/optimizer/src/data/patch_ledger.rs` -- ledger types, loader, validation
- `data/manifests/2026-01-13.json` -- initial manifest data file
- `data/patch_ledgers/2026-01-13.yaml` -- initial baseline ledger

### game_build_id Discovery

To find the correct `game_build_id` for the initial manifest:
1. Check GW2 wiki patch notes around 2026-01-13 for build numbers
2. If unavailable, use current live `/v2/build` value with authoring notes explaining this is the first verified baseline
3. The manifest's `sources` array must cite where the value came from

## Tasks

- [ ] 1. Create `data/manifests/2026-01-13.json` with initial manifest data (AC: 1, 11)
- [ ] 2. Create `data/patch_ledgers/2026-01-13.yaml` with baseline empty ledger (AC: 2)
- [ ] 3. Create `crates/optimizer/src/data/manifests.rs` with `PatchManifest` struct, serde, validation (AC: 3, 5, 6)
- [ ] 4. Create `crates/optimizer/src/data/patch_ledger.rs` with `PatchLedger` struct, serde, validation (AC: 4, 5, 10)
- [ ] 5. Add manifest-set validation: uniqueness, lineage, circular reference detection (AC: 6)
- [ ] 6. Add staleness detection function comparing `game_build_id` against live build (AC: 7)
- [ ] 7. Register modules in `data/mod.rs` with re-exports (AC: 5)
- [ ] 8. Handle missing manifest/ledger as `Degraded` not `Disabled` (AC: 8)
- [ ] 9. Update `docs/optimizer-data-schemas.md` Schema 1 with `game_build_id` field (AC: 9)
- [ ] 10. Add tests: manifest validation, ledger validation, inheritance chain, error paths (AC: 3, 4, 5, 6)

## Verification

```bash
cargo test --package gw2-optimizer -v
cargo check

# Verify manifest file is valid JSON
python -c "import json; json.load(open('data/manifests/2026-01-13.json')); print('OK')"

# Verify ledger file is valid YAML
python -c "import yaml; yaml.safe_load(open('data/patch_ledgers/2026-01-13.yaml')); print('OK')"
```

## Dev Notes

- Use `serde_yaml` for ledger files (YAML is more readable for change diffs). Add `serde_yaml` to workspace dependencies if not already present.
- The ledger's `changes` array is empty for the baseline. Future stories (P3-09) will populate it.
- `game_build_id` is metadata for live-staleness detection. It does NOT participate in manifest ordering (that uses `release_date`/`patch_id`).
- The informational staleness indicator should be lightweight -- a string or flag in the state, not a full UI overhaul.
- For the `include_str!` pattern with YAML: use `include_str!` for the .yaml file and `serde_yaml::from_str()` to parse.
- If P3-07 has already landed and defined `DataLoadError`, reuse it. If not, define compatible error types locally.

### References

- [Source: _bmad-output/planning-artifacts/epics.md, Story 3.8]
- [Source: docs/optimizer-data-schemas.md, Schema 1]
- [Source: crates/gw2api/src/client.rs, check_api_health / /v2/build]
- [Source: crates/optimizer/src/data/mod.rs, existing loader infrastructure]
