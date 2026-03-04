# Definition of Done

_Last updated: 2026-03-04 (Epic 1 retrospective)_

## Story Definition of Done

A story is **done** when ALL of the following are true:

### Core Criteria (pre-existing)

- [ ] All Acceptance Criteria verified and marked `[x]` in the story file
- [ ] `cargo check --workspace` exits clean — zero errors, zero warnings
- [ ] `cargo test --package gw2-build-optimizer -- --test-threads=1` passes (addon crate, single-threaded)
- [ ] `cargo test --workspace --exclude gw2-build-optimizer` passes
- [ ] Code reviewed (or self-reviewed with `/bmad-bmm-code-review`) and all Mx/Lx findings addressed or backlogged
- [ ] PR merged to `main`

### Added in Epic 1 Retrospective (DoD-1 through DoD-4)

- [ ] **DoD-1** — Story `Status:` header (line 3 of story file) updated to `done` in the **same commit** as the sprint-status.yaml entry. Never in separate commits.
- [ ] **DoD-2** — If this is the **last story in an epic**, the epic key in sprint-status.yaml is also updated to `done` in that same commit.
- [ ] **DoD-3** — Any **unresolved Mx code-review finding** has a named entry in the sprint backlog (story file drafted or stub added to sprint-status.yaml) before the story is merged.
- [ ] **DoD-4** — `cargo test --workspace` pass count and failure count are **recorded in the Dev Agent Record** (Debug Log References section) before marking the story done.

---

## Epic Definition of Done

An epic is **done** when:

- [ ] All stories in the epic have story status `done`
- [ ] epic key in sprint-status.yaml is `done`
- [ ] Retrospective has been run (`epic-N-retrospective: done` in sprint-status.yaml)

---

## SM Story-Drafting Checklist

Apply before moving any story to `ready-for-dev`:

### Added in Epic 1 Retrospective (SM-1 through SM-3)

- [ ] **SM-1** — For every `cargo test -p <pkg>` or `cargo build -p <pkg>` command in the story: run `grep '^name' crates/<crate>/Cargo.toml` and use the **exact output string** verbatim in commands. Do not use crate directory names.
  - _Prevents: package-name mismatch (e.g., `gw2_addon` vs `gw2-build-optimizer`)_

- [ ] **SM-2** — For every **count-based AC** (e.g., "output must equal N", "`wc -l` must show N"): run the count command against the **current codebase** and use the actual output as N. Do not estimate.
  - _Prevents: spawn-count drift, test-count drift_

- [ ] **SM-3** — For every command in the **Verification section**: execute it against the current codebase and confirm it runs without error before the story moves to `ready-for-dev`.
  - _Prevents: unrunnable verification commands discovered during implementation_

---

## References

- Epic 1 Retrospective: `_bmad-output/implementation-artifacts/epic-1-retro-2026-03-04.md`
- Sprint Status: `_bmad-output/implementation-artifacts/sprint-status.yaml`
