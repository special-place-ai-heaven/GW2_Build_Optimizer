# Epic 1: Production Readiness Sprint

**Project**: GW2_Build_Optimizer
**Status**: in-progress
**Source of truth**: docs/stories/ (5 approved stories with quality-gated ACs)

## Overview

Addresses all production-readiness gaps identified in `docs/architecture-assessment.md` and prioritized in `docs/production-readiness-backlog.md`. The project is feature-complete (S01–S15); this epic covers CI infrastructure, test coverage, correctness fixes, and a structural refactor.

**Total effort estimate**: ~11-12 hours across 5 stories.

---

## Story 1.1: CI Pipeline (P1-01)

Story file: `docs/stories/P1-01-ci-pipeline.md`
Status: ready-for-dev
Effort: ~1 hour

Add `.github/workflows/ci.yml` running `cargo check` + `cargo test` on every push/PR.
Blocks: all other stories (nothing merges without CI).

---

## Story 1.2: addon Crate Tests (P1-02)

Story file: `docs/stories/P1-02-addon-tests.md`
Status: ready-for-dev
Effort: ~3-4 hours

Add `#[cfg(test)]` module to `crates/addon/src/state.rs` covering CancellationToken, init() routing, with_state(), and clear(). Also updates ci.yml to run gw2_addon single-threaded.

**Special constraint**: `cargo test -p gw2_addon -- --test-threads=1` REQUIRED (global static STATE deadlock risk with parallel test threads).

Blocked by: Story 1.1 (P1-01)
Blocks: Story 1.4 (P1-03)

---

## Story 1.3: Trait Fuzzy-Match Guard (P2-04)

Story file: `docs/stories/P2-04-trait-fuzzy-match.md`
Status: ready-for-dev
Effort: ~30 minutes

Add `needle.len() >= 5` guard before the contains-match fallback in `validation.rs:486-491`. Prevents short LLM-hallucinated trait names from silently matching wrong traits in the Tier 2 optimizer path.

Blocked by: Story 1.1 (P1-01) only
Independent of: Stories 1.2, 1.4, 1.5

---

## Story 1.4: main_view.rs Decomposition (P1-03)

Story file: `docs/stories/P1-03-main-view-decomp.md`
Status: ready-for-dev
Effort: ~4 hours

Pure structural refactor: split `main_view.rs` (~1400 lines, 13 thread spawns) into `main_view/mod.rs`, `character.rs`, `resolution.rs`, `optimization.rs`, `stats.rs`. No behavioral changes.

Blocked by: Stories 1.1 (P1-01) + 1.2 (P1-02) — state tests must be green before the large refactor

---

## Story 1.5: Condition Stack Weights (P2-01)

Story file: `docs/stories/P2-01-condition-weights.md`
Status: ready-for-dev
Effort: ~2-3 hours

Introduce `ConditionWeights` struct + 3 presets (necro_group, firebrand_group, default_pve). Update `calculate_combat_performance()` signature. Wire `condition_weights_for_profession()` in `synergy_pipeline.rs`.

Blocked by: Story 1.1 (P1-01); Story 1.4 (P1-03) recommended-complete before merging

---

## Epic Retrospective

Key: `epic-1-retrospective`
Status: optional (conduct after all 5 stories reach `done`)
