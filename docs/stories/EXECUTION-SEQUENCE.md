# Story Execution Sequence

_Optimized for risk reduction: CI first → tests second → quick correctness wins → large refactor last_

---

## Sequence

```
P1-01  →  P1-02  →  P2-04  →  P1-03  →  P2-01
  CI       Tests    Fuzzy     Decomp    Condi
 (1h)     (3-4h)   (30min)    (4h)     (2-3h)
```

### Why this order

| Step | Story | Rationale |
|------|-------|-----------|
| 1 | **P1-01 CI** | Enables automated verification for every subsequent story. Nothing else should merge without CI. |
| 2 | **P1-02 addon tests** | Validates the state machine before the large refactor (P1-03). If `clear()` or `with_state()` are broken, we want to know now, not after moving 1400 lines across 5 files. Also requires a CI workflow edit (`--test-threads=1`), which must be in place before P1-03 merges. |
| 3 | **P2-04 trait fuzzy-match** | Independent 30-minute correctness fix. No file overlap with any other story. Delivers immediate risk reduction for Tier 2 optimizer path at near-zero cost. Ideal slot for a quick win before the riskier stories. |
| 4 | **P1-03 main_view decomp** | Largest blast-radius refactor (deletes and recreates ~1400 lines across 5 files). Runs last among P1s so CI and state tests are both green first. Any merge conflict with P1-02 is minimal (different files). |
| 5 | **P2-01 condition weights** | Touches optimizer core (`combat.rs`, `synergy_pipeline.rs`). Runs after all structural work is done so the combat test suite is stable and the function signature change is not complicated by parallel refactors. |

---

## Dependency Graph

```
P1-01 (CI)
  └── P1-02 (addon tests)       [needs CI workflow update for --test-threads=1]
  │     └── P1-03 (decomp)      [wants state tests green before large refactor]
  │
  ├── P2-04 (fuzzy-match)       [fully independent, any order after P1-01]
  │
  └── P2-01 (cond. weights)     [fully independent, any order after P1-01]
```

**Parallel opportunities**: P2-04 and P1-02 can be worked simultaneously (zero file overlap). P2-01 and P1-03 can be worked simultaneously (zero file overlap). P1-01 is the only strict blocker.

---

## Per-Story Entry/Exit Criteria

### P1-01 — CI Pipeline
- **Entry**: Repository access; ability to push to a branch and open a PR.
- **Exit**: `ci.yml` exists; CI status check appears green on a test PR; CI turns red when a deliberate compile error is introduced and reverted.

### P1-02 — addon tests
- **Entry**: P1-01 complete (CI active).
- **Exit**: `cargo test -p gw2_addon -- --test-threads=1` runs 10+ tests, all green; CI workflow updated to run addon tests single-threaded; no new `unwrap()` on external data outside of tests.

### P2-04 — trait fuzzy-match
- **Entry**: P1-01 complete (CI active). No other dependencies.
- **Exit**: `cargo test --package gw2_optimizer -- validation --list` shows 6 tests; all 6 green; `git diff --name-only` shows only `validation.rs` modified.

### P1-03 — main_view decomp
- **Entry**: P1-01 complete; P1-02 complete (state tests must be green so regression is visible); P1-02's CI `--test-threads=1` change must be merged.
- **Exit**: `grep -rn "thread::spawn" crates/addon/src/ui/main_view/ | wc -l` == 13; `cargo check --workspace` zero warnings; `cargo test --workspace` all green; `main_view.rs` file deleted; 5 sub-module files present.

### P2-01 — condition weights
- **Entry**: P1-01 complete. P1-03 recommended-complete (avoids merge conflict if `main_view` calls `calculate_combat_performance`).
- **Exit**: `grep -rn "condition_weights_for_profession\|necro_group\|firebrand_group" crates/optimizer/src/` shows at least one call site outside definitions; `cargo test --package gw2_optimizer -- combat` shows 3 new tests passing; all existing combat tests pass with identical expected values.

---

## CI Workflow Note (P1-01 + P1-02 interaction)

After P1-02, `.github/workflows/ci.yml` must contain **two separate test steps** instead of one:

```yaml
- name: Run addon tests (single-threaded — global static STATE)
  run: cargo test --package gw2_addon -- --test-threads=1

- name: Run all other tests
  run: cargo test --workspace --exclude gw2_addon
```

This ensures the global `STATE` mutex tests never deadlock in CI while all other crates continue to benefit from parallel test execution.

---

_Total estimated effort: ~11-12 hours across 5 stories._
_Sequence last updated: 2026-03-03_
