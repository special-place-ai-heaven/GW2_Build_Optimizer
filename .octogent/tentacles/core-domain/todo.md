# Todo

## Done 2026-04-16

- ~~**Round-trip snapshot coverage for `BuildLocks::describe_constraints`**~~ —
  Verified: the 7 existing tests in `crates/core/src/types.rs:324-422`
  already cover the (none, spec-only, trait-only, both) matrix plus
  trait-lock sort-by-spec-id edge cases. No new tests needed.

- ~~**Forward-compat: `SavedBuild` ignores unknown fields**~~ — added
  `test_forward_compat_ignores_unknown_fields` in
  `crates/core/src/storage.rs`. Feeds JSON with three unknown fields
  (scalar, string, nested object) and asserts successful load.

- ~~**Audit `StatBlock` alias normalization coverage**~~ — added
  `test_stat_add_all_alias_pairs` and `test_stat_get_unknown_attr_returns_zero`
  in `crates/optimizer/src/stats.rs` (alias logic lives there, not in
  `core`; `CONTEXT.md:38` updated with the correct path). Covers the
  previously-untested `ConditionDuration/Expertise` and
  `Healing/HealingPower` pairs plus `add`→`get` round-trip symmetry.

- ~~**Document `GameMode::ALL` ordering contract**~~ — doc comment added
  to the constant in `crates/core/src/types.rs`, plus pinned-slice test
  `game_mode_all_order_is_pinned` and `game_mode_default_is_pve` in
  the tests module.

- ~~**Extract `#[serde(default)]` default-fn boilerplate**~~ — done 2026-04-16.
  Six `default_*` fns replaced with a `default_f32!` macro in `config.rs`;
  added `test_empty_json_round_trips_to_defaults` regression test.

## Done with caveats 2026-04-16

- ⚠️ **`BuildStorage` concurrent-save stress test** — added
  `test_save_new_concurrent_race` in `crates/core/src/storage.rs`.
  Marked `#[ignore]` because the test surfaces a real TOCTOU bug in
  `save_new`: two threads can both pass `path.exists()`, both write the
  same `.tmp`, and both renames succeed (the later writer silently
  replaces the earlier one). The uniqueness guarantee is not atomic.
  Un-ignore once `save_new` reserves the final path with atomic
  exclusive creation. Reproduce with
  `cargo test -p gw2-core -- --ignored`.

## Open

- **Fix `save_new` TOCTOU race** — discovered by the stress test above.
  Replace the `path.exists()` + write-to-tmp + rename pattern with
  atomic exclusive creation. Minimum viable fix: open the destination
  with `OpenOptions::create_new(true)` as the first step; on
  `AlreadyExists` return the collision error; otherwise write through
  that handle. This loses the `.tmp` → rename crash-safety, so consider
  reserving a zero-byte placeholder with `create_new`, then writing
  `.tmp`, then replacing. Un-ignore `test_save_new_concurrent_race`
  after fixing.
