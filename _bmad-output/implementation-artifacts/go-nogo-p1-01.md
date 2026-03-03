# Go / No-Go Checklist — Start Dev Story on P1-01

_Clear every item before opening a new context window and invoking `/bmad-bmm-dev-story`._

---

## Environment checks

- [ ] **Git status clean**: `git status` shows no uncommitted changes that would conflict with `.github/workflows/ci.yml` creation.
- [ ] **GitHub repo is accessible**: You have push access to a branch and can open PRs.
- [ ] **Local build passes**: `cargo check --workspace` exits 0.
- [ ] **Local tests pass**: `cargo test --workspace` exits 0 (confirms baseline before CI is added).

## Story artifact checks

- [ ] **Story file exists**: `docs/stories/P1-01-ci-pipeline.md` is present and `Status: ready-for-dev`.
- [ ] **Sprint status reflects slot 1**: `_bmad-output/implementation-artifacts/sprint-status.yaml` shows `p1-01-ci-pipeline: ready-for-dev`.
- [ ] **Story has all required sections**: Non-Goals ✅, Dependencies ✅, Verification ✅, ACs 1-7 ✅.

## Scope confirmation (no-go if any are true)

- [ ] **NOT planning** to add `cargo build --release` to CI in this story (that's out of scope).
- [ ] **NOT planning** to add `-- --include-ignored` to the test step (live API tests stay manual).
- [ ] **NOT planning** to modify any `.rs` source files (this story creates only `.github/workflows/ci.yml`).

## Forward-compatibility check

- [ ] **Aware of P1-02 consequence**: The ci.yml created in this story will need a follow-up edit in P1-02 to split test steps (`--test-threads=1` for gw2_addon). Draft the initial ci.yml with a single `cargo test --workspace` step; P1-02 will refine it. Document the pending split in a comment inside ci.yml.

---

## **DECISION**

All boxes checked? → **GO** — open a fresh context window and run `/bmad-bmm-dev-story`.

Any box unchecked? → **NO-GO** — resolve the item first, then re-check.

---

_Reference: docs/stories/P1-01-ci-pipeline.md_
_Sprint status: _bmad-output/implementation-artifacts/sprint-status.yaml_
