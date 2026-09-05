# Verification and handoff

User override (2026-09-05): work in small sprints and commit often. Verified local sprint commits may precede in-game acceptance, superseding wait-before-commit wording below. Stage only owned changes; another agent is actively modifying this shared tree. No push/release is implied.
Use Terminal Commander direct argv for long commands, cwd repository root. Inspect exit status and failure signals, not just a completion message.

1. Targeted regression tests for each changed subsystem.
2. cargo clippy --workspace --all-targets -- -D warnings
3. cargo test --workspace
4. cargo build --release
5. For server changes separately: cargo test --manifest-path server/feedback/Cargo.toml and cargo clippy --manifest-path server/feedback/Cargo.toml --all-targets -- -D warnings. Never run local Docker.
6. Formatting: rustfmt only changed Rust files while existing unrelated formatting remains; schedule workspace formatting under W119.
7. Update ledger.md and tasks.md with command outcomes and exact remaining limitations.
8. Increment patch once per coherent executable batch, add CHANGELOG.md entry, build the versioned DLL, resolve addons_dir from dev.cfg, preserve the previous DLL and copy the new one.
9. In-game checks: About/history survives reload; Lock All locks exactly three specs; public radio still plays; mixed Sentinel/Dragon Choya plate reflects per-slot toughness/vitality and ranking. No request to submit feedback is implied.
10. Stop for user in-game testing before commit/push/release, as required by CLAUDE.md. Keep subsequent ledger tasks open.

## First batch acceptance
Automated: corruption/read failure refusal, first-run save, repeated overwrite, bounded Lock All, reserved IPv6 literal screening, strict Clippy.
In-game: addon loads, About/history remains intact, Lock All works, radio behavior remains normal. B001 has a separate mixed-gear regression and in-game case once its implementation lands.
