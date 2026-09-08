# Verification and handoff: Simulator trust (Sprint 1)

Work on branch `004-simulator-trust` in the main checkout. Do not edit `crates/optimizer/src/prompts.rs`, `crates/optimizer/src/llm/`, or `crates/optimizer/examples/choya_live.rs`; those belong to the latency work on `fix/wiki-timing-facts`. Serialize Cargo commands; another agent may hold the worktree's target directory, and this checkout has its own.

## Prerequisites

- Rust stable toolchain; no `dev.cfg` needed for anything in this sprint (all experiments use `GameDb::empty_for_tests()`).
- `dev.cfg` with `addons_dir` only for the optional in-game step.

## Per-step checks

1. Audit document present: `docs/simulator-connection-audit.md` has the seven sections of contracts/audit-document.md and a Baseline naming this commit.
2. Focused tests for the touched module first:
   `MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib rotation::wvw_timeline::` and `… referee::`
   Expected: every `reaper_*` experiment listed by name; each kind present at least once.
3. Each experiment was seen failing once: for each kind, the audit's Experiments section records the disabling change (record removed, ICD set to 0, trigger changed) and the failing assertion text.
4. Parity: `cargo test -p gw2-optimizer --lib reaper_parity` passes; the assertion compares referee, Optimize suggestion and `score_build` on `user_intent_score`, six axes, `is_viable`, `quality` with eps 1e-9 (1.0 for rounded indices).
5. Coverage line: `cargo test -p gw2-build-optimizer --lib coverage_note` (addon) shows the line set on both the Optimize and the Choya suggestion; locale parity test passes for `quality.coverage_line`.
6. Budget: time `cargo test -p gw2-optimizer --lib` before and after; the difference is at most 10 s on the same machine. Heavier tests carry `#[ignore = "slow: run on demand"]`.
7. Workspace gates, in this order:
   `cargo fmt --all --check`
   `cargo clippy --workspace --all-targets -- -D warnings`
   `cargo test --workspace`
   `cargo build --release`
8. Update the audit document's Findings statuses and the Remedy section after each check.

## In-game acceptance (only when the release condition in spec FR-014 / Story 4 holds)

- Optimize a Reaper in WvW: the comparison view shows the quality marker and, beside it, `Not simulated: Superior Sigil of Fire (on-crit)` when that sigil is equipped; no line when it is not.
- Ask Choya for a Reaper WvW build: the plate's concerns text carries the same line; the suggestion's marker is Provisional, not Verified.
- Hover the marker: the tooltip still lists the full reasons.

## Handoff

Local commits per verified step. Push and release only together with the in-game acceptance of the Choya and free-model work, as the spec records. Report which findings are demonstrated, refuted or unconfirmed, the measured test-time delta, and the exact remedy commit.
