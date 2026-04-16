# Todo

- **Round-trip snapshot coverage for `BuildLocks::describe_constraints`** —
  The LLM tentacle reads this string verbatim. Add a test covering every
  (spec-locked, trait-locked, both, neither) combination asserting a stable
  snapshot. Prevents silent prompt drift.

- **Forward-compat: `SavedBuild` ignores unknown fields** — verify serde is
  configured to ignore, not reject. If a future field is added and a user
  downgrades the DLL, their saved builds must still load.

- **Audit `StatBlock` alias normalization coverage** — confirm every new
  attribute introduced in recent patches normalizes correctly through
  `add` / `get`. Add regression tests that feed both the old and new API
  spelling and assert equal reads.

- **`BuildStorage` concurrent-save stress test** — `save_new` + `save_overwrite`
  rely on `.tmp + rename` atomicity. Add a test with two threads racing
  `save_new` on colliding names, asserting exactly one wins.

- **Document `GameMode::ALL` ordering contract** — at least one lookup
  assumes `[Pve, Pvp, WvW]`. Add a doc comment and a pinned-slice test.

- **Extract `#[serde(default)]` default-fn boilerplate** — `config.rs:120-137`
  has six near-identical `default_*` helpers. Consider a single generic
  helper with `const` defaults, or a macro. Keep behavior identical; add a
  test that an empty-JSON config round-trips to current defaults.
