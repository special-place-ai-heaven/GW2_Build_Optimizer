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

- ~~**Normalize `ApiError` formatting across call sites**~~ — done. `Api`
  now carries `{ status, url_path, body_snippet }` with a `body_snippet()`
  helper (≤200 chars, UTF-8 safe, strips HTML). `RateLimited` carries
  `{ retries, url_path }`. Added `Cache(String)` and `Internal(String)`;
  removed the `status: 0` sentinel from all 11 call sites. Convention
  documented in `code-review` skill. 3 new `body_snippet` unit tests.

- ~~**Expand live-test coverage to exotic itemstats**~~ — done. Added three
  `#[ignore]` live tests alongside `test_live_fetch_berserkers_itemstat`:
  `test_live_fetch_pvp_amulets_all` (all amulets via `ids=all`),
  `test_live_fetch_legendary_dual_stat_weapon` (Sunrise id=30704, asserts
  `stat_choices` non-empty), `test_live_fetch_relic` (Relic of the Thief
  id=100947, asserts `item_type == "Relic"`). All pass against live API.

- ~~**`DataCache::clear_all` atomicity contract**~~ — done. Chose
  **best-effort with surfaced error list** (honest given POSIX/Windows
  file semantics — deletions can't be rolled back). Signature changed to
  `Result<(), CacheError>`. New `CacheError::PartialClear { failures }`
  carries per-entry failure messages; missing cache directory is OK
  (nothing to clear); non-`.json`/`.tmp` files are left untouched. Three
  new tests cover missing-dir, non-cache-file preservation, and
  portable partial-failure simulation (directory with `.json` extension
  — `remove_file` rejects on both Windows and Unix).
