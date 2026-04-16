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

- **Expand live-test coverage to exotic itemstats** —
  `test_live_fetch_berserkers_itemstat` covers one prefix. Add ignored
  tests for relics, legendary dual-stat weapons, PvP amulets — categories
  most likely to shape-shift in API responses.

- **`DataCache::clear_all` atomicity contract** — iterates and deletes. Add
  a test that defines and verifies behavior on partial failure. Either
  "all-or-nothing" or "best-effort with surfaced error list" is fine, but
  one must be the contract.
