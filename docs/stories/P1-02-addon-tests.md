# Story P1-02: `addon` Crate — Unit Test Coverage

Status: done

## Story

As a developer maintaining the addon crate,
I want unit tests covering the state machine, screen routing, cancellation, and `with_state` access,
so that future changes to UI lifecycle code can be validated without running GW2.

## Non-Goals

- **No tests for UI rendering**: This story does not test any `main_view.rs`, `setup.rs`, or other ImGui render functions — only `state.rs` logic.
- **No `catch_unwind` on non-optimizer threads**: Adding `catch_unwind` to the 9 unguarded background threads in `main_view.rs` is a separate hardening concern, explicitly deferred.
- **No tests for config parsing or storage**: Those live in `crates/core/` and already have coverage. Not in scope here.
- **No integration tests**: All tests are unit tests in `state.rs` itself. No `tests/` integration directory is created.
- **No mock or external frameworks**: Native `#[test]` only.

## Dependencies

- **P1-01 must be complete** before this story is marked done, so the new tests are verified by CI automatically on merge. Technically the tests can be written without CI, but they should not be merged until CI is green on them.

## Verification

**Critical**: Tests that call `init()`, `clear()`, or `with_state()` modify the global `static STATE: Mutex<Option<AddonState>>`. Rust's default test runner executes tests on multiple threads in parallel — this **will deadlock** if two tests hold the STATE lock simultaneously.

Run addon tests with a single thread:

```bash
# All addon tests, single-threaded to prevent STATE deadlock
cargo test -p gw2-build-optimizer -- --test-threads=1

# Verify specific test names match expected list
cargo test -p gw2-build-optimizer -- --test-threads=1 --list
```

Note: the addon crate's package name is `gw2-build-optimizer` (matches `crates/addon/Cargo.toml [package] name`).

CI will also need to use `--test-threads=1` for the addon package:

```yaml
- run: cargo test --package gw2-build-optimizer -- --test-threads=1
- run: cargo test --workspace --exclude gw2-build-optimizer
```

## Acceptance Criteria

1. `crates/addon/src/state.rs` contains a `#[cfg(test)]` module with at minimum 5 test functions.
2. `CancellationToken` behavior is tested: a new token is not cancelled; after `cancel()` it reports `is_cancelled() == true`; clones of a token see the cancellation.
3. `init()` screen routing is tested for all 4 config states: (a) setup complete → `Screen::Main`; (b) gw2 key + llm key but no cache → `Screen::Setup(DataDownload)`; (c) gw2 key only → `Screen::Setup(LlmApiKey)`; (d) no keys → `Screen::Setup(Gw2ApiKey)`.
4. `MainState::default()` is tested to confirm key fields initialize to expected zero/empty/false values (`characters` is empty, `optimizing` is false, `weights` is the PvE default, `build_locks` is empty).
5. `with_state` is tested: returns `None` when state is not initialized; invokes the closure and returns `Some(R)` when initialized.
6. `clear()` is tested: after `clear()`, the `CancellationToken` that was cloned before clear reports `is_cancelled() == true`, and `with_state` returns `None`.
7. All new tests follow naming convention `test_verb_condition` (e.g., `test_cancel_token_propagates_to_clones`).
8. No external test frameworks — native `#[test]` only per project rules.
9. Every test that calls `init()`, `clear()`, or `with_state()` calls `reset_state()` as its first line to reset the global `STATE` to `None`, ensuring test isolation regardless of execution order.
10. `cargo test -p gw2-build-optimizer -- --test-threads=1` produces zero failures and zero panics. (`--test-threads=1` is required because tests share a global static mutex; parallel execution deadlocks.)

## Tasks / Subtasks

- [ ] Update `.github/workflows/ci.yml` to run addon tests single-threaded (AC: 10)
  - [ ] Replace `cargo test --workspace` with two steps: `cargo test --package gw2-build-optimizer -- --test-threads=1` and `cargo test --workspace --exclude gw2-build-optimizer`
- [ ] Add `#[cfg(test)]` module to `crates/addon/src/state.rs` (AC: 1, 7, 8, 9)
- [ ] Implement `CancellationToken` tests (AC: 2)
  - [ ] `test_cancel_token_new_is_not_cancelled`
  - [ ] `test_cancel_token_cancel_sets_flag`
  - [ ] `test_cancel_token_clone_sees_cancellation`
- [ ] Implement `init()` screen routing tests (AC: 3)
  - [ ] `test_init_routes_to_main_when_setup_complete`
  - [ ] `test_init_routes_to_data_download_when_keys_present_no_cache`
  - [ ] `test_init_routes_to_llm_key_when_only_gw2_key`
  - [ ] `test_init_routes_to_gw2_key_when_no_keys`
- [ ] Implement `MainState::default()` tests (AC: 4)
  - [ ] `test_main_state_default_fields`
- [ ] Implement `with_state` tests (AC: 5)
  - [ ] `test_with_state_returns_none_when_uninitialized`
  - [ ] `test_with_state_invokes_closure_when_initialized`
- [ ] Implement `clear()` tests (AC: 6)
  - [ ] `test_clear_cancels_token`
  - [ ] `test_clear_drops_state`

## Dev Notes

- **Global static state**: `STATE` is `static Mutex<Option<AddonState>>` at `state.rs:12`. Tests that mutate STATE must not run in parallel. Use `#[test]` (not `#[test] async`) — Rust's test runner may run tests in parallel threads. Safest approach: wrap each test that touches STATE in a `lock_state()` guard and ensure it resets STATE to None before and after.
- **Test isolation pattern for STATE**: Each test that calls `init()` or `clear()` should reset STATE to `None` before running:
  ```rust
  fn reset_state() {
      *lock_state() = None;
  }
  ```
  Call `reset_state()` at the top of each such test.
- **`init()` requires a real `PathBuf` and valid `AppConfig`**: Use `std::env::temp_dir()` for `addon_dir`. `AppConfig::load()` on a non-existent path returns a default config with no error — useful for controlled test setup.
- **Avoid calling `with_state` inside a closure that already holds `lock_state()`**: `with_state` is non-reentrant (`state.rs:283-288`). Tests that inspect state after `init()` should use `with_state(|s| ...)` directly, not nest calls.
- **`is_setup_complete()`** returns true only when GW2 key + active LLM key + cache build number are all present. For the "setup complete" routing test, use `AppConfig` with all three fields populated.
- **Adversarial finding**: The 9 non-optimizer background threads in `main_view.rs` (1501, 1584, 2168, 2199, 2279, 2312, 3233) have no `catch_unwind`. A future hardening task (P3 candidate) should add `catch_unwind` to these threads so loading flags are cleared on panic. This story does NOT need to fix that — it only adds tests for `state.rs`.

### Project Structure Notes

- Modify: `crates/addon/src/state.rs` — add `#[cfg(test)]` module at end of file.
- No new files required (tests live in same file as code under test, per project rules).
- `lock_state()` is `pub(crate)` — accessible from the test module.

### References

- [Source: docs/production-readiness-backlog.md#P1-02] — adversarial findings re: stuck loading flags
- [Source: _bmad-output/project-context.md#Testing Rules] — native `#[test]` only, `test_verb_condition` naming, temp dirs, no mocking
- [Source: crates/addon/src/state.rs:12] — static STATE declaration
- [Source: crates/addon/src/state.rs:199-207] — lock_state() with poison recovery
- [Source: crates/addon/src/state.rs:210-256] — init() screen routing logic
- [Source: crates/addon/src/state.rs:274-280] — clear() implementation
- [Source: crates/addon/src/state.rs:283-288] — with_state() non-reentrant contract

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6

### Debug Log References

- Discovery: addon crate package name is `gw2-build-optimizer` (not `gw2_addon`). Updated story verification commands and ci.yml to use the correct name.
- `cargo test --package gw2-build-optimizer -- --test-threads=1` → **20 passed, 0 failed** (13 new state tests + 7 pre-existing tests)
- `cargo test --workspace --exclude gw2-build-optimizer` → **189 passed, 0 failed, 21 ignored**

### Completion Notes List

- Added 13 tests in `#[cfg(test)] mod tests` at the bottom of `state.rs`. All tests follow `test_verb_condition` naming.
- `reset_state()` helper accesses `super::lock_state()` (private fn visible to child modules in Rust). Called as first line of every test that touches STATE.
- `config_in_tempdir()` helper writes a config.json via `AppConfig::save()` to a temp dir path keyed by `std::process::id()` + label suffix — unique per process, unique per test within the same run.
- `CancellationToken::new()` and `token.cancel()` are private methods but accessible from the test module (child module sees parent's private items in Rust).
- `test_main_state_default_fields` and `test_init_loading_flags_start_false` together document the stuck-loading-flag adversarial risk boundary. The risk comment cites the specific line numbers in `main_view.rs` where the unguarded threads live.
- `ci.yml` updated: single `cargo test --workspace` replaced with two steps — addon crate single-threaded, rest of workspace parallel. The P1-01 `TODO(P1-02)` comment was removed and replaced with an explanatory comment.

### File List

- `crates/addon/src/state.rs` (modified — added `#[cfg(test)]` module, 13 tests)
- `.github/workflows/ci.yml` (modified — split test steps, resolved P1-01 TODO)
- `docs/stories/P1-02-addon-tests.md` (modified — corrected package name in Verification + AC10 + Tasks)
