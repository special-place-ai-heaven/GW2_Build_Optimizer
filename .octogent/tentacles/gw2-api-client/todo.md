# Todo

- **Extract a tested bulk-IDs query helper** — `get_with_params`
  (`client.rs:111-238`) manually escapes bulk IDs to avoid `%2C`. Extract
  `build_bulk_ids_query(ids: &[u32]) -> String`, add fuzz tests (empty,
  single, over 200, `u32::MAX`), and reuse in every bulk fetcher.

- **Add `Retry-After`-aware rate handling** — current `TokenBucket::take`
  spin-waits with `MAX_RETRIES = 3`. Detect HTTP 429 and honor
  `Retry-After` instead of burning retries. Add a mock-server test.

- **Cache invalidation on build-number rollover** — confirm `is_stale`
  behaves correctly when the server returns a *lower* build number (rare,
  but possible during rollback). Today's check may treat it as fresh.

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
