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

- ~~**`BuildStorage` concurrent-save stress test**~~ — added
  `test_save_new_concurrent_race` in `crates/core/src/storage.rs`
  (initially landed `#[ignore]`d because it surfaced a real TOCTOU
  race in `save_new`). Un-ignored and passing after the fix below.

- ~~**Fix `save_new` TOCTOU race**~~ — `save_new` now atomically claims
  the destination with `OpenOptions::create_new(true)` before writing
  the `.tmp` + renaming over the claim. Preserves crash-safety (partial
  writes still land in `.tmp`, not the published `.json`) and closes
  the race: two concurrent callers can no longer both succeed.
  `test_save_new_concurrent_race` passes 5/5 stress runs locally.

## Open

_(no open core-domain items — add new ones here.)_
