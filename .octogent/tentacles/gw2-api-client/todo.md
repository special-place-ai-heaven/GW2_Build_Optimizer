# Todo

- ~~**Add `Retry-After`-aware rate handling**~~ — done. `parse_retry_after`
  (integer seconds, HTTP-date intentionally unsupported) + `RETRY_AFTER_CAP
  = 30s` short-circuit to `ApiError::RateLimited`. 5xx stays on exponential
  backoff. Mock-server tests via `mockito` dev-dep cover 429→200 retry and
  over-cap short-circuit.

- ~~**Cache invalidation on build-number rollover**~~ — confirmed correct.
  `is_stale` uses `meta.build != current_build`, so rollback (cached >
  current) already invalidates. Tightened doc-comment to state the
  rollback contract explicitly and added `test_staleness_rollback_is_stale`
  regression test.

- **Normalize `ApiError` formatting across call sites** — variants exist but
  shape differs. Pick one (`status`, `body_snippet`, `url_path`), update
  tests, document in `code-review` skill.

- ~~**Expand live-test coverage to exotic itemstats**~~ — done. Added three
  `#[ignore]` live tests alongside `test_live_fetch_berserkers_itemstat`:
  `test_live_fetch_pvp_amulets_all` (all amulets via `ids=all`),
  `test_live_fetch_legendary_dual_stat_weapon` (Sunrise id=30704, asserts
  `stat_choices` non-empty), `test_live_fetch_relic` (Relic of the Thief
  id=100947, asserts `item_type == "Relic"`). All pass against live API.

- **`DataCache::clear_all` atomicity contract** — iterates and deletes. Add
  a test that defines and verifies behavior on partial failure. Either
  "all-or-nothing" or "best-effort with surfaced error list" is fine, but
  one must be the contract.
