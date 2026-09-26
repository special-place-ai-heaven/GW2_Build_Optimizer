# GameDb subset for the fidelity budget gate

A generated cut of the synced game-data cache, frozen at game build 207318.
The files are `DataCache` files, and `GameDb::load` reads them unchanged. Do
not edit them by hand.

## Why

CI has no synced cache. With this subset, `no_observable_exceeds_its_fidelity_budget`
in `tests/fidelity_logs.rs` runs in every `cargo test`, together with the
fixture logs in `../ei_logs` and the fixture corpus in `../benchmarks`. It
reproduces the synced cache's budget rows bit for bit. The full cache is
24.5 MB, and this subset is 2.7 MB.

## What it holds

- **Professions.** The professions `EXPECTED_FIDELITY` budgets, which are
  Engineer and Necromancer today.
- **Traits.** Every trait of those professions' specializations.
- **Skills.** Every skill of those professions, plus the skills their traits
  and the budget rows' logs name. The set is closed over flips, chains,
  toolbelts, transforms and bundles.
- **Items.** Only the items the budget rows' kits name.
- **Guard.** The guard test checks every specialization, trait (minor and
  major), bar skill and item the budget kits use.
- **Small catalogs.** The item stats, specializations, legends, pets and PvP
  amulets are copied whole.
- **`uncached_kit_items.json`.** Real kit items that the addon cache's keep
  rule filters out by type and rarity. Today these are Fine infusions and a
  Jade Bot core, so the synced cache lacks them too. The kits are correct, so
  do not remove these ids from the corpus. The guard test allows exactly
  these ids.

## Regenerate

Regenerate after a game build changes the cache, or when a budget row is
added. Sync in-game first so that `dev.cfg` points at a fresh cache.

    cargo run -p gw2-optimizer --example log_compare -- --gamedb crates/optimizer/tests/fixtures/gamedb

The output is byte-identical for the same cache. A new budget row for another
profession also needs that profession in `GAMEDB_ROWS` in
`examples/log_compare.rs`. The guard test fails by name until it is there.

## Drift

The subset does not follow game patches. Before a release, run the parity
test against the synced cache:

    cargo test -p gw2-optimizer --test fidelity_logs -- --ignored

If `fixture_gamedb_reproduces_the_synced_cache` fails, regenerate the subset
and then re-measure the budgets.
