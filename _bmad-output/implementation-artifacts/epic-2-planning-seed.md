# Epic 2 Planning Seed

_Generated: 2026-03-04 from Epic 1 retrospective_
_Status: Draft — requires sprint-planning session to create story files and update sprint-status.yaml_

---

## Epic 2 Theme

**Quality Hardening + Domain Accuracy**

Pays the two items of technical debt carried out of Epic 1 (M-3 integration test, bg-thread hardening), then addresses domain-accuracy and tooling improvements. Epic 2 keeps the project on the "production-ready" trajectory before new user-facing features are added.

**Prerequisite (before Epic 2 kickoff):**
- Action items 1–3 from Epic 1 retrospective complete (DoD published, SM checklist published, project-context.md stale line fixed)

---

## Starter Stories — Priority Order

### P2-02: End-to-End Condition Dispatch Integration Test (M-3)

**Priority**: Medium — draft and implement first
**Dependency**: None
**Scope**: Small — single test, single file

**Story Summary:**
As a developer maintaining the condition scoring pipeline,
I want an integration test that verifies `condition_weights_for_profession()` dispatch flows through to a meaningfully different `condition_dps_index` output,
so that a future accidental reversion to `default_pve()` at any call site is immediately caught by CI.

**Acceptance Criteria (draft):**
1. A new `#[test]` in `crates/optimizer/src/combat.rs` (or `scoring.rs`) named `test_profession_dispatch_affects_condi_score`.
2. Test constructs a condition-heavy `ConditionTicks` profile (high bleeding + torment stacks).
3. Calls `calculate_combat_performance()` twice: once with `&ConditionWeights::necro_group()`, once with `&ConditionWeights::default_pve()`.
4. Asserts `necro_result.condition_dps_index > default_result.condition_dps_index`.
5. Test is self-contained — no API calls, no file I/O, no GameDb required.
6. `cargo test --package gw2_optimizer -- test_profession_dispatch_affects_condi_score` passes.

**Verification:**
```bash
cargo test --package gw2_optimizer -- test_profession_dispatch_affects_condi_score
```

**Dev Notes:**
- Use existing `StatBlock` construction patterns from `combat.rs` test module.
- `ConditionWeights::necro_group()` has Bleeding=8.0, Torment=6.0 vs default_pve Bleeding=3.0, Torment=1.5. A profile with non-zero bleeding/torment ticks will produce a measurably higher score.
- No changes to production code needed — this is test-only.

---

### P2-03: Background Thread `catch_unwind` Hardening

**Priority**: Medium — must complete before any user-facing feature story
**Dependency**: Independent of P2-02
**Scope**: Medium — 10 thread spawns across 3 files + tests

**Story Summary:**
As a GW2 player using the addon,
I want background thread panics to clear their loading flags gracefully,
so that a bug in character loading, stats calculation, or model fetching does not cause a permanent loading spinner that requires a game restart.

**Context:**
The optimizer thread is already wrapped in `catch_unwind` (correct). The following threads are not:
- `optimization.rs`: start_optimization helper threads (×2)
- `stats.rs`: check_api_health, start_fetch_models, load_game_db, compute stats threads (×4)
- `character.rs`: load_characters, load_character_tabs threads (×2)
- `mod.rs`: inline threads in render_settings_tab (×2)

**Acceptance Criteria (draft):**
1. All 10 background thread spawn bodies wrapped in `std::panic::catch_unwind(|| { ... })`.
2. In each `Err(_)` arm: log warning via `log::warn!("bg thread panicked: <context>")` and clear the relevant loading flag via `with_state`.
3. Each wrapped thread retains its existing `CancellationToken` check at start and after each blocking op.
4. At minimum one `#[test]` per logical thread group (optimization, stats, character) verifying the loading flag is cleared when the thread body panics.
5. `cargo test --package gw2-build-optimizer -- --test-threads=1` passes, including new panic-recovery tests.
6. `cargo check --workspace` zero warnings.

**Verification:**
```bash
cargo test --package gw2-build-optimizer -- --test-threads=1
cargo check --workspace
# Verify spawn count unchanged:
grep -rn "thread::spawn" crates/addon/src/ui/main_view/ | wc -l  # must equal 10
```

**Dev Notes:**
- Each thread body that currently sets `loading = true` before spawn must have a matching `loading = false` in the catch arm.
- `catch_unwind` requires the closure to be `UnwindSafe`. All types accessed via `with_state()` use `Mutex<Option<AddonState>>` which is `UnwindSafe`. Cloned values (Arc, String) are also fine.
- Do not add `catch_unwind` to the optimizer thread — it already has one.
- See P1-02 Dev Notes for the established `reset_state()` test isolation pattern.

---

### P2-05: Per-Elite-Spec Condition Weight Presets

**Priority**: Low — implement after P2-02 and P2-03
**Dependency**: P2-02 done (extends its integration test)
**Scope**: Small — extend `ConditionWeights` presets, update dispatch

**Story Summary:**
As a player using a condition Scourge or Harbinger build,
I want the optimizer to use distinct condition weights for my elite spec rather than a shared necro-group preset,
so that Scourge (bleeding/torment heavy) and Harbinger (poison/torment rotations) receive appropriately differentiated scoring.

**Context:**
P2-01 introduced `necro_group()` (Necromancer/Scourge/Harbinger all use same weights) and `firebrand_group()` (Guardian/Firebrand/Willbender). This story adds per-elite-spec granularity where rotation stacks meaningfully differ.

**Acceptance Criteria (draft):**
1. `ConditionWeights::harbinger_preset()` — Poison=3.0, Bleeding=5.0, Torment=5.0, Burning=0.5, Confusion=0.1 (Harbinger pistol rotation applies more poison)
2. `condition_weights_for_profession()` dispatch updated: "Harbinger" → `harbinger_preset()`, "Scourge" → existing `necro_group()`, base "Necromancer" → `necro_group()`.
3. Existing `test_condition_weights_for_profession_dispatch` updated to cover Harbinger path.
4. P2-02's integration test updated or supplemented to assert Harbinger produces Harbinger-appropriate scoring vs Scourge.
5. No changes to `necro_group()`, `firebrand_group()`, or `default_pve()` values.

---

### P2-06: project-context.md Refresh

**Priority**: Low — good session-reset task
**Dependency**: None
**Scope**: Documentation only

**Story Summary:**
As an AI dev agent starting a new session,
I want `_bmad-output/project-context.md` to reflect the current codebase accurately,
so that I don't apply stale rules or miss current constraints.

**Acceptance Criteria (draft):**
1. Stale "No CI pipeline" rule (line 102) updated (already done via OT-1).
2. Run `/bmad-bmm-generate-project-context` in a fresh session to regenerate from current code.
3. New rules captured for: `--test-threads=1` constraint, `reset_state()` pattern, `ConditionWeights` parameter on `calculate_combat_performance()`, module layout post-P1-03 decomp.
4. Outdated rules removed or updated (count of rules may change).
5. File committed to repo.

---

### P3-01: Coverage Reporting (`cargo tarpaulin`)

**Priority**: Low
**Dependency**: P2-03 done
**Scope**: CI/tooling

**Story Summary:**
Add `cargo tarpaulin` as a manual coverage report script (not a blocking CI gate) to identify untested paths in `crates/optimizer/` and `crates/core/`.

**Acceptance Criteria (draft):**
1. `scripts/coverage.sh` (or equivalent) runs `cargo tarpaulin --workspace --exclude gw2_build_optimizer` and produces an HTML report.
2. Not a blocking CI gate — informational only.
3. Initial baseline coverage % recorded in story Dev Agent Record.

---

## Epic 2 Sequencing

```
P2-02 (M-3 integration test)    ──┐
                                   ├── both must complete before any feature story
P2-03 (catch_unwind hardening)  ──┘

P2-05 (elite-spec presets)       ── after P2-02 (extends test)
P2-06 (project-context refresh)  ── anytime (good session-opener)
P3-01 (coverage)                 ── after P2-03 (addon threads now safe to instrument)
```

---

## Epic 1 Retrospective Action Items (tracking)

| # | Action | Owner | Status |
|---|--------|-------|--------|
| 1 | Fix stale `project-context.md` CI line | Rob | ✅ Done (2026-03-04) |
| 2 | Publish DoD additions to `docs/definition-of-done.md` | Bob (SM) | ✅ Done (2026-03-04) |
| 3 | SM story-drafting checklist in `docs/definition-of-done.md` | Bob (SM) | ✅ Done (2026-03-04) |
| 4 | Draft P2-02 story file | Bob (SM) | ⏳ Pending sprint-planning |
| 5 | Draft P2-03 story file | Bob (SM) | ⏳ Pending sprint-planning |
| 6 | Apply SM-1/SM-2/SM-3 to Epic 2 story drafts | Bob (SM) | ⏳ Ongoing |
| 7 | Apply DoD-1/DoD-2 to Epic 2 story closures | Dev + SM | ⏳ Ongoing |
| 8 | Build + deploy Epic 1 DLL | Rob | ⏳ Pending |

---

_Reference: `_bmad-output/implementation-artifacts/epic-1-retro-2026-03-04.md`_
