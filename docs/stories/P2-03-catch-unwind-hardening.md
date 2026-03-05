# Story P2-03: Background Thread catch_unwind Hardening

Status: review

## Story

As a GW2 player using the addon,
I want background thread panics to clear their loading flags gracefully,
so that a bug in character loading, stats calculation, chat response, or model fetching does not cause a permanent loading spinner that requires a game restart.

## Non-Goals

- **No changes to the optimizer thread** (`start_optimization_with_profession`, optimization.rs:70): it is already wrapped in `catch_unwind` — do not re-wrap it.
- ~~setup.rs threads are now in scope (expanded from code review finding — closes the full bug class).~~
- **No behavioral changes**: Wrapped threads must still run their existing logic identically when no panic occurs. Only the `Err(_)` arm is new.
- **No changes to `catch_unwind` in the optimizer crate** (`crates/optimizer/`): this scope is the addon crate only.
- **No UI changes**: No new user-visible indicators. The log warning and cleared flag are sufficient.

## Dependencies

- **P1-02 must be done** (it is: `reset_state()` test isolation pattern established; tests run with `--test-threads=1`).
- **Independent of P2-02** — no file overlap.

## Verification

```bash
# Full addon-crate test suite (includes new panic-recovery tests)
cargo test --package gw2-build-optimizer -- --test-threads=1

# Full workspace check — zero warnings required
cargo check --workspace

# Verify spawn count in main_view unchanged after the story (must remain 10):
grep -rn "thread::spawn" crates/addon/src/ui/main_view/ | wc -l
# Expected output: 10

# Verify spawn count in setup.rs unchanged (must remain 3):
grep -rn "thread::spawn" crates/addon/src/ui/setup.rs | wc -l
# Expected output: 3

# Confirm optimizer thread still has its existing catch_unwind (must stay):
grep -n "catch_unwind" crates/addon/src/ui/main_view/optimization.rs
# Expected: line ~72 shows AssertUnwindSafe wrapping the optimizer body
```

## Acceptance Criteria

1. The following 12 thread spawn bodies are wrapped in `std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { ... }))`:
   - `optimization.rs` — `send_chat_message` thread (line ~857)
   - `stats.rs` — `start_fetch_models` thread (line ~11)
   - `stats.rs` — `start_game_data_refresh` thread (line ~42)
   - `stats.rs` — `check_api_health` thread (line ~122)
   - `stats.rs` — `load_game_db` thread (line ~155)
   - `character.rs` — `load_characters` thread (line ~31)
   - `character.rs` — `load_character_tabs` thread (line ~114)
   - `mod.rs` — first `render_settings_tab` inline thread (line ~1023)
   - `mod.rs` — second `render_settings_tab` inline thread (line ~1086)
   - `setup.rs` — GW2 API key validation thread (line ~91) — flag: `state.setup.gw2_key_status`
   - `setup.rs` — LLM key validation thread (line ~268) — flag: `state.setup.llm_key_status`
   - `setup.rs` — game data download thread (line ~369) — flag: `state.setup.download_progress`
2. In each new `Err(_)` panic arm:
   - `log::warn!("bg thread panicked: <descriptive context>")` is emitted (e.g., `"bg thread panicked: send_chat_message"`, `"bg thread panicked: load_characters"`).
   - The relevant loading flag is cleared via `with_state(|s| { s.main.<flag> = false; })` (or equivalent to end the spinner state).
3. Each wrapped thread retains its existing `CancellationToken` check at the top of the spawn body and after each blocking operation — wrapping must not remove or reorder these.
4. After wrapping, `grep -rn "thread::spawn" crates/addon/src/ui/main_view/ | wc -l` still outputs **10** and `grep -rn "thread::spawn" crates/addon/src/ui/setup.rs | wc -l` outputs **3** (no spawns added or removed).
5. At minimum one `#[test]` per logical thread group, confirming the loading flag is cleared when the thread body panics:
   - **Optimization group** (1 test): a test that triggers a panic inside a mock `send_chat_message` body and asserts the loading flag is cleared.
   - **Stats group** (1 test): a test that triggers a panic inside a mock `start_fetch_models` (or any stats thread) body and asserts the relevant flag is cleared.
   - **Character group** (1 test): a test that triggers a panic inside a mock `load_characters` body and asserts `state.main.characters_loading` is cleared.
   - **Setup group** (1 test): a test that triggers a panic inside a mock setup key validation body and asserts `state.setup.gw2_key_status` (or `llm_key_status`) is reset from `Validating`.
6. `cargo test --package gw2-build-optimizer -- --test-threads=1` passes including all new panic-recovery tests.
7. `cargo check --workspace` exits with zero errors and zero warnings.

## Tasks / Subtasks

- [x] Wrap `send_chat_message` thread in `optimization.rs` (AC 1, 2, 3)
  - [x] Identify the loading flag used by `send_chat_message` (e.g., `state.main.chat_loading`)
  - [x] Wrap the spawn body in `std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { ... }))`
  - [x] Add `Err(_)` arm: `log::warn!` + `with_state` to clear the flag
- [x] Wrap `start_fetch_models` thread in `stats.rs` (AC 1, 2, 3)
- [x] Wrap `start_game_data_refresh` thread in `stats.rs` (AC 1, 2, 3)
- [x] Wrap `check_api_health` thread in `stats.rs` (AC 1, 2, 3)
- [x] Wrap `load_game_db` thread in `stats.rs` (AC 1, 2, 3)
- [x] Wrap `load_characters` thread in `character.rs` (AC 1, 2, 3)
- [x] Wrap `load_character_tabs` thread in `character.rs` (AC 1, 2, 3)
- [x] Wrap both `render_settings_tab` inline threads in `mod.rs` (AC 1, 2, 3)
- [x] Wrap GW2 API key validation thread in `setup.rs` (AC 1, 2, 3)
  - [x] On panic: set `state.setup.gw2_key_status = KeyStatus::Invalid("thread panicked")`
- [x] Wrap LLM key validation thread in `setup.rs` (AC 1, 2, 3)
  - [x] On panic: set `state.setup.llm_key_status = KeyStatus::Invalid("thread panicked")`
- [x] Wrap game data download thread in `setup.rs` (AC 1, 2, 3)
  - [x] On panic: set `state.setup.download_progress.error = Some("thread panicked")` and `done = true`
- [x] Add panic-recovery tests — optimization group (AC 5)
- [x] Add panic-recovery tests — stats group (AC 5)
- [x] Add panic-recovery tests — character group (AC 5)
- [x] Add panic-recovery tests — setup group (AC 5)
- [x] Verify spawn count = 10 in main_view, 3 in setup.rs (AC 4)
- [x] Run full verification commands (AC 6, 7)

## Dev Notes

- **`UnwindSafe` contract**: All types accessed inside a `catch_unwind` closure must be `UnwindSafe`. Cloned values (`Arc<_>`, `String`, `bool`, `CancellationToken = Arc<AtomicBool>`) are all `UnwindSafe`. The `with_state()` pattern accesses `Mutex<Option<AddonState>>` via a new lock acquisition — `Mutex` is `UnwindSafe`. Use `AssertUnwindSafe` to assert this for the closure.
- **Do NOT wrap `start_optimization_with_profession`** (`optimization.rs:70`): it already wraps its body in `catch_unwind` at line ~72. Adding a second wrapping would nest catch blocks unnecessarily.
- **`setup.rs` is now in scope**: Three thread spawns in `crates/addon/src/ui/setup.rs` (GW2 key validation at line ~91, LLM key validation at line ~268, data download at line ~369). Added during code review to close the full bug class.
- **Setup flags**: GW2 key validation uses `state.setup.gw2_key_status = KeyStatus::Validating`; LLM key uses `state.setup.llm_key_status = KeyStatus::Validating`; download uses `state.setup.download_progress = Some(DownloadState { ... })`. On panic, reset these to error states, not just cleared booleans.
- **Loading flag identification**: Before wrapping each thread, search for the `state.main.<flag> = true` set before the spawn — that same flag must be set to `false` in the `Err(_)` arm. For example, `start_fetch_models` probably sets `state.main.models_loading = true`; the catch arm clears it.
- **Test isolation**: Use the `reset_state()` pattern established in P1-02. All addon tests MUST run with `--test-threads=1` due to the global `Mutex<Option<AddonState>>` static.
- **Test pattern for panic-recovery**: Call the function under test (or inline an equivalent closure) in a way that forces a panic, then verify the state flag is cleared. Example approach:
  ```rust
  #[test]
  fn test_catch_unwind_clears_characters_loading_on_panic() {
      reset_state();
      with_state(|s| s.main.characters_loading = true);
      // Simulate what happens when catch_unwind catches a panic:
      let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
          // clear the flag as the Err arm would
          with_state(|s| s.main.characters_loading = false);
          panic!("simulated panic");
      }));
      // The flag must be cleared regardless
      with_state(|s| assert!(!s.main.characters_loading));
  }
  ```
  Adjust to match the actual Err-arm logic.
- **Warn context strings**: Use the function name (e.g., `"bg thread panicked: load_game_db"`) for easy log filtering. Do not include dynamic data in the warn string (avoid format! with runtime values that might themselves panic).
- **P1-02 test count**: P1-02 established that the addon test suite runs cleanly. The new tests must not break this. Confirm after adding tests.

### Project Structure Notes

- Modify: `crates/addon/src/ui/main_view/optimization.rs` — wrap `send_chat_message` thread body
- Modify: `crates/addon/src/ui/main_view/stats.rs` — wrap 4 thread bodies + add stats panic-recovery test
- Modify: `crates/addon/src/ui/main_view/character.rs` — wrap 2 thread bodies + add character panic-recovery test
- Modify: `crates/addon/src/ui/main_view/mod.rs` — wrap 2 inline thread bodies
- Modify: `crates/addon/src/ui/setup.rs` — wrap 3 thread bodies (GW2 key validation, LLM key validation, data download)

### References

- [Source: crates/addon/src/state.rs:342] — existing comment describing unwrapped threads
- [Source: crates/addon/src/ui/main_view/optimization.rs:70–72] — established `catch_unwind` pattern to follow
- [Source: docs/stories/P1-02-addon-tests.md] — `reset_state()` test isolation pattern
- [Source: epic-2-planning-seed.md] — acceptance criteria basis

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

None.

### Completion Notes List

- Wrapped all 12 background thread spawn bodies in `std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { ... }))` across 5 files
- Each `Err(_)` arm logs via `nexus::log::log(Warning, "GW2BuildOpt", "bg thread panicked: <context>")` and clears the relevant loading flag via `with_state`
- Loading flag identification per thread:
  - `send_chat_message` → `chat.waiting = false`
  - `start_fetch_models` → `models_loading = false`
  - `start_game_data_refresh` → `game_db_loading = false` + `game_refresh_stage = String::new()`
  - `check_api_health` → `api_health_checking = false`
  - `load_game_db` → `game_db_loading = false`
  - `load_characters` → `characters_loading = false`
  - `load_character_tabs` → `build_loading = false`
  - `settings_test_connection` → `settings_key_validating = false`
  - `settings_save_key_validation` → `settings_key_validating = false`
  - `setup_gw2_key_validation` → `gw2_key_status = KeyStatus::Invalid("thread panicked")`
  - `setup_llm_key_validation` → `llm_key_status = KeyStatus::Invalid("thread panicked")`
  - `setup_data_download` → `download_progress.error = Some("thread panicked")` + `done = true`
- Added 4 panic-recovery tests in `state.rs` (optimization, stats, character, setup groups)
- Spawn count verified: 10 in main_view, 3 in setup.rs (unchanged)
- All 219 tests pass (24 addon + 165 optimizer + 22 gw2api + 8 core), zero regressions
- `cargo check --workspace` exits with zero errors and zero warnings
- Used `nexus::log::log` instead of `log::warn!` since the `log` crate is not a dependency of the addon crate
- Existing `catch_unwind` in `start_optimization_with_profession` (optimization.rs:72) preserved unchanged

### File List

- `crates/addon/src/ui/main_view/optimization.rs` — wrapped `send_chat_message` thread
- `crates/addon/src/ui/main_view/stats.rs` — wrapped 4 threads (start_fetch_models, start_game_data_refresh, check_api_health, load_game_db)
- `crates/addon/src/ui/main_view/character.rs` — wrapped 2 threads (load_characters, load_character_tabs)
- `crates/addon/src/ui/main_view/mod.rs` — wrapped 2 inline threads (settings test connection, settings save key validation)
- `crates/addon/src/ui/setup.rs` — wrapped 3 threads (GW2 key validation, LLM key validation, data download)
- `crates/addon/src/state.rs` — added 4 panic-recovery tests

## Change Log

- 2026-03-05: Scope expanded — added 3 setup.rs thread spawns (lines ~91, ~268, ~369) to close full panic-hardening bug class. Added setup group panic-recovery test requirement. Updated ACs, tasks, verification, and dev notes.
- 2026-03-06: Implementation complete — all 12 threads wrapped in catch_unwind, 4 panic-recovery tests added, all 219 tests pass, zero warnings.
