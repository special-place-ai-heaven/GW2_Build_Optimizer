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

## Done 2026-04-16 (second pass — audit-driven)

- ~~**`AppConfig::load` parse-error path**~~ — added
  `test_load_parse_error_resets_to_defaults` in `crates/core/src/config.rs`.
  Writes malformed JSON to a temp file and asserts `load` returns
  `Some(msg containing "could not be parsed")` with a pristine
  `AppConfig::default()` (no partial recovery leaks through). Covers the
  user-visible "settings reset" branch at `config.rs:208-215`.

- ~~**`BuildStorage::list` corrupt-file skip**~~ — added
  `test_list_skips_corrupt_json_files` in `crates/core/src/storage.rs`.
  Drops one good + one malformed `.json` + one `.txt` into the saves dir
  and asserts `list()` returns exactly the good build. Pins the silent-
  skip contract at `storage.rs:146-154` and the `.json`-extension filter.

- ~~**`BuildStorage::delete` missing-file error path**~~ — added
  `test_delete_missing_returns_err` in `crates/core/src/storage.rs`.
  Asserts the `!path.exists()` branch returns
  `Err("... not found on disk")` — previously covered only indirectly
  via `test_save_and_list`.

- ~~**`AppConfig` active-provider routing**~~ — added
  `test_active_routing_matches_provider` in `crates/core/src/config.rs`.
  Populates all three provider slots with distinct sentinels and flips
  `active_provider` across Gemini/OpenAI/Anthropic, asserting
  `active_api_key()` and `active_model_id()` route by enum, not by
  whichever key is set. Also pins the "empty slot → `None` + default
  model id" fallback.

- ~~**`StatBlock::compute_derived` formula pinning + stale-doc fix**~~ —
  added `statblock_compute_derived_pins_formula` in
  `crates/core/src/types.rs` that locks the hardcoded constants
  `(895, 21, 150, 15, 10)` against silent drift from
  `data/formulas/universal.json` (four cases: baseline, non-zero fero +
  above-threshold precision, below-threshold floor at 0%, saturation at
  100%). Dropped the stale *"If callers are added, inject constants…"*
  line — the 6 optimizer call sites already exist — and replaced it
  with a pointer to the new pinning test so future drift fails loudly.

## Open

_(no open core-domain items — add new ones here.)_
