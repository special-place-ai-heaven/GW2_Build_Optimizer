# Story P1-01: CI Pipeline — Automated Test Runs

Status: done

## Story

As a developer maintaining GW2 Build Optimizer,
I want every push and pull request to automatically run `cargo check` and `cargo test`,
so that regressions are caught before they reach the main branch.

## Acceptance Criteria

1. A GitHub Actions workflow file exists at `.github/workflows/ci.yml`.
2. The workflow triggers on every push to `main` and on every pull request targeting `main`.
3. The workflow runs `cargo check` (fast type-check, no codegen) and `cargo test` (all non-ignored unit tests).
4. The workflow targets a stable Rust toolchain pinned by a `rust-toolchain.toml` or inline in the workflow.
5. Live API tests (`#[ignore]` tests in `crates/gw2api/tests/live_download.rs` and `crates/optimizer/tests/live_llm.rs`) are **not** run in CI — `cargo test` without `-- --include-ignored` is sufficient.
6. The CI job fails on any compilation error or test failure and reports failure on the GitHub PR status check.
7. On a clean run with no code changes, all existing 80+ tests pass.

## Non-Goals

- **No release build in CI**: `cargo build --release` is not run in CI — it produces the DLL artifact and is slow; dev workflow handles it manually.
- **No coverage reporting**: No `cargo tarpaulin` or similar — coverage is not tracked in CI in this story.
- **No MSRV enforcement**: No minimum-supported-Rust-version matrix — single `stable` toolchain only.
- **No `--include-ignored` tests**: Live API tests remain manual-only; this story does not add CI secrets or conditional API test runs.
- **No caching of build artifacts**: `sccache` or cargo registry caching is a nice-to-have; not in scope.
- **No deployment step**: CI does not copy the DLL or create a release artifact.

## Dependencies

- **None** — this is the first story and has no blocking predecessors. It is the foundation that all subsequent stories depend on (new tests added in P1-02, P2-01, P2-04 will run in CI automatically once this story is complete).

## Verification

Run these commands locally before pushing — CI must produce the same result:

```bash
# From project root
cargo check --workspace
cargo test --workspace
```

CI verification:
- After pushing the workflow file on a branch, open a PR targeting `main` — the "CI / test" status check must appear on the PR within ~2 minutes and turn green.
- To verify CI failure detection: introduce a deliberate compile error (e.g., add `let x: i32 = "oops";` to any file), push, confirm CI turns red, then revert.

## Tasks / Subtasks

- [ ] Create `.github/workflows/ci.yml` (AC: 1, 2, 3, 4, 5, 6)
  - [ ] Add `on: [push, pull_request]` trigger targeting the `main` branch
  - [ ] Add `jobs.test` with `runs-on: windows-latest` (DLL target is win32; `cargo build --release` produces `.dll`)
  - [ ] Add step: `cargo check --workspace`
  - [ ] Add step: `cargo test --workspace` (no `-- --include-ignored`)
  - [ ] Pin Rust stable toolchain via `dtolnay/rust-toolchain@stable` action
- [ ] Verify locally that `cargo test --workspace` passes without `-- --include-ignored` (AC: 7)
- [ ] Open a test PR to confirm CI status check appears on the PR (AC: 6)

## Dev Notes

- **No async/await**: the project is `reqwest` blocking. The CI job does not need Tokio or any async runtime.
- **Windows target**: the DLL is a `cdylib` for win32. Use `runs-on: windows-latest` to match the target platform. Cross-compiling from Linux is possible but would require extra setup for nexus-rs linking.
- **Live API tests are gated `#[ignore]`**: `crates/gw2api/tests/live_download.rs` and `crates/optimizer/tests/live_llm.rs` hit real APIs and require secrets. Do NOT add `-- --include-ignored` to the CI test step. These remain manual-only per project rules.
- **Workspace deps are hoisted**: the root `Cargo.toml` defines all dependency versions under `[workspace.dependencies]`. No per-crate version pinning is needed.
- **`cargo check`** is the fast path (type-check only, no codegen). Run it first; it's cheap and catches compile errors in seconds.

### Project Structure Notes

- New files: `.github/workflows/ci.yml` only. No source changes.
- No `Cargo.toml` changes required — workspace is already valid.

### References

- [Source: docs/architecture-assessment.md#3.2] — 80+ unit tests, 0 unsafe blocks
- [Source: _bmad-output/project-context.md#Development Workflow Rules] — `cargo build --release`, live API tests are `#[ignore]`
- [Source: CLAUDE.md] — "No CI pipeline — tests are run manually with `cargo test`."

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6

### Debug Log References

- `cargo check --workspace` → `Finished dev profile in 0.19s` (clean, no warnings)
- `cargo test --workspace` → 196 passed, 0 failed, 12 ignored
  - gw2_api: 22 passed, 2 ignored (live fetch tests — correctly excluded)
  - gw2_build_optimizer (addon): 7 passed
  - gw2_core: 8 passed
  - gw2_optimizer: 159 passed, 9 ignored (live LLM tests — correctly excluded)

### Completion Notes List

- Created `.github/workflows/` directory and `ci.yml` from scratch (no pre-existing CI).
- Used `dtolnay/rust-toolchain@stable` for stable toolchain pinning (no `rust-toolchain.toml` needed).
- Used `runs-on: windows-latest` — required for the win32 cdylib target; cross-compiling from Linux would require extra nexus-rs linking setup.
- Included prominent `# TODO(P1-02)` comment block inside `ci.yml` documenting the exact two-step split that must be applied when addon tests are added (deadlock risk from global static `STATE` mutex under parallel test threads).
- `cargo test --workspace` without `-- --include-ignored` correctly excludes all live API/LLM tests.
- 196 tests passing at baseline; AC7 pre-satisfied before CI was created.

### File List

- `.github/workflows/ci.yml` (new)
