# Todo

- **Ship a schema check in CI** — add a `cargo test -p gw2-optimizer data::`
  filter in CI that loads *all* bundled JSON/YAML. Failure should block
  merge. Today it runs as part of the full suite; make it a first-class
  gate. Wiring lives in `.github/workflows/ci.yml` (owned by `addon-ui`),
  but the test logic and the gate's intent are this tentacle's.

- **HP-class vs armor-class cross-reference test** — CLAUDE.md and
  `code-review` both flag as a hazard. Add a test asserting every
  profession's HP class and armor class come from distinct tables and
  that `profession_profiles.json` never conflates the two.

- **Cross-dataset consistency test** — every profession in
  `rotation_profiles/<mode>.json` must appear in
  `profession_profiles.json`. Every boon referenced in
  `objective_profiles/*.json::boon_priorities` must exist in
  `formulas/boons.json`. Put these in `consistency_tests.rs`.

- **Balance-override coverage audit** — confirm every profession has at
  least one `pvp.json` and `wvw.json` entry where the real split has one.
  Silent "no override" path is indistinguishable from a missed entry. Add
  a coverage test backed by a known-good checklist.

- **Extract `populate_heuristic_uptimes` inputs into JSON** — the function
  at `rotation_profiles.rs:504-583` carries scenario buff tables that
  should live in `data/`, not baked into Rust. Migrate, then this
  function becomes a pure transform.

- **`EvidenceLevel::Verified` path with stricter validators** — heuristic
  is the floor. When a number is sourced from an official wiki
  change-note (not approximated), tag it verified and enforce
  `source_url` points to `wiki.guildwars2.com`. Drives `validate_ledger`
  rules.

- **Patch-ledger inheritance** —
  `test_baseline_ledger_has_no_inherits_from` hints at an `inherits_from`
  design but no ledger currently uses it. When the next patch lands,
  author it as a delta ledger and verify load behavior with an
  integration test.

- **Consolidate `include_str!` + load pattern into a macro** — 12 loaders
  repeat the same shape. `load_dataset!(name, path, validator)` removes
  40+ lines of duplication without losing clarity. Keep malformed-input
  tests unchanged.
