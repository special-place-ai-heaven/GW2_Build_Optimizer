# Story 2.06: project-context.md Refresh

Status: ready-for-dev

## Story

As an AI dev agent starting a new session,
I want `_bmad-output/project-context.md` to reflect the current codebase accurately,
so that I don't apply stale rules or miss current constraints.

## Non-Goals

- **No code changes**: This is documentation only. No Rust source files are modified.
- **No new rules invented from scratch**: Rules must be derived from actual codebase patterns, not speculative best practices.
- **No architecture.md or CLAUDE.md changes**: Only `_bmad-output/project-context.md` is updated.
- **No Epic 3 content**: The project-context reflects the current codebase state (through Epic 2), not future Epic 3 architecture plans.

## Dependencies

- **None** — independent of all other stories. Good session-opener task.
- P2-03 (catch_unwind hardening) and P2-05 (condition presets) are done, so their patterns should be captured.

## Verification

```bash
# No code tests — documentation only story
# Verify the file exists and has been updated
cat _bmad-output/project-context.md | head -10
# Date should be current, rule_count may change

# Verify no Rust source files were modified
git diff --name-only -- '*.rs'  # should be empty
```

## Acceptance Criteria

1. **Stale `catch_unwind` rule updated**: The current rule (line ~81) says "the optimizer bg thread is wrapped in `catch_unwind`". After P2-03, ALL 12 background threads are wrapped. Rule must reflect this: "All background thread spawn bodies are wrapped in `std::panic::catch_unwind`. Each `Err` arm logs via `log::warn!` and clears the relevant loading flag via `with_state`."
2. **`reset_state()` test isolation pattern added** to Testing Rules: `reset_state()` (defined in `crates/addon/src/lib.rs`) clears the global `STATE` mutex between tests. All addon crate tests that touch global state must call it in setup. Tests must run with `--test-threads=1` because the global static is shared.
3. **`--test-threads=1` constraint promoted** from buried CI section to Testing Rules: `cargo test --package gw2-build-optimizer -- --test-threads=1` is mandatory because addon tests share a global `Mutex<Option<AddonState>>` static. Running multi-threaded causes intermittent deadlocks.
4. **`ConditionWeights` parameter rule added** to Critical Rules: `calculate_combat_performance()` takes `&ConditionWeights` as a parameter. Callers must pass the result of `condition_weights_for_profession(profession_name)`, not `ConditionWeights::default_pve()` directly, to get profession-specific condition scoring.
5. **Module layout post-P1-03 updated**: The `crates/addon/src/ui/main_view/` submodule structure must be documented — `mod.rs` (render dispatch), `stats.rs`, `improve.rs`, `character.rs`, `optimization.rs`, `lock_panel.rs`, `setup.rs`. This replaced the monolithic `main_view.rs`.
6. **Harbinger dispatch noted**: `condition_weights_for_profession()` now dispatches `"Harbinger"` to `harbinger_preset()` separately from `"Necromancer" | "Scourge"` which use `necro_group()`. This is a codebase fact, not a rule — but it should appear in the condition tick/weight context.
7. **Run `/bmad-bmm-generate-project-context`** in a fresh session to regenerate the full file from current code, then manually verify the above items are captured. If the generator misses any, add them manually.
8. **Outdated rules removed or updated**: Review all 71 existing rules against the current codebase. Remove or update any that no longer apply. The rule count may change.
9. **YAML frontmatter updated**: `date` field set to current date, `rule_count` updated to actual count, `status: complete`.
10. **File committed to repo** with message `docs: refresh project-context.md with post-Epic-2 rules`.

## Tasks / Subtasks

- [ ] Read current `_bmad-output/project-context.md` thoroughly (AC: 8)
  - [ ] Identify every rule that references pre-P2-03 state (single catch_unwind, missing test patterns)
  - [ ] Identify rules that are now redundant or outdated
- [ ] Run `/bmad-bmm-generate-project-context` to regenerate from current code (AC: 7)
  - [ ] Execute in a fresh session context for clean analysis
  - [ ] Compare generated output against current file to find gaps
- [ ] Update stale `catch_unwind` rule to reflect all 12 bg threads (AC: 1)
- [ ] Add `reset_state()` test isolation pattern to Testing Rules (AC: 2)
- [ ] Promote `--test-threads=1` constraint to Testing Rules section (AC: 3)
- [ ] Add `ConditionWeights` parameter rule to Critical Rules (AC: 4)
- [ ] Add module layout documentation for `main_view/` submodule structure (AC: 5)
- [ ] Add Harbinger dispatch note to condition scoring context (AC: 6)
- [ ] Remove or update any outdated rules found in review (AC: 8)
- [ ] Update YAML frontmatter (date, rule_count) (AC: 9)
- [ ] Commit to repo (AC: 10)

## Dev Notes

- **This is a documentation-only story.** No `*.rs` files should be modified. The only file changed is `_bmad-output/project-context.md`.
- **The generator command is `/bmad-bmm-generate-project-context`** — this is a BMAD skill that analyzes the codebase and regenerates the project-context file. It should be run first, then the output reviewed and manually patched for any gaps.
- **Rule source verification**: Every rule in project-context.md must be traceable to actual code. Don't invent rules from architectural aspirations. Verify against `grep` in the codebase.
- **Preserve AUTO-MANAGED markers if present** — the project-context.md template may use markers. Don't break them.

### Specific Stale Items Found (as of 2026-03-06)

| Line | Current Text | Issue | Fix |
|------|-------------|-------|-----|
| ~81 | "the optimizer bg thread is wrapped in `catch_unwind`" | P2-03 wrapped ALL 12 bg threads | Update to reflect all threads + loading flag cleanup |
| Missing | — | No `reset_state()` pattern documented | Add to Testing Rules |
| ~102 | `--test-threads=1` in CI section only | Not in Testing Rules where devs look first | Add to Testing Rules, keep in CI section too |
| Missing | — | No `ConditionWeights` parameter guidance | Add to Critical Rules |
| Missing | — | No `main_view/` submodule structure | Add to Module layout rule |
| Missing | — | No Harbinger dispatch note | Add to condition scoring context |

### Files to Modify

- `_bmad-output/project-context.md` — the only file

### What NOT to Change

- Do not modify `CLAUDE.md` (root or project-level) — those are separate documents
- Do not modify any `*.rs` source files
- Do not add Epic 3 architecture rules — those belong in future stories
- Do not change the project-context template in `_bmad/bmm/` — only the generated output

### Project Structure Notes

- `_bmad-output/project-context.md` is the generated output file
- `_bmad/bmm/workflows/generate-project-context/` contains the generator workflow
- `_bmad/bmm/data/project-context-template.md` is the template used by the generator

### References

- [Source: _bmad-output/implementation-artifacts/epic-2-planning-seed.md#P2-06] — original story spec and AC draft
- [Source: _bmad-output/project-context.md] — current file to be refreshed (71 rules, dated 2026-03-03)
- [Source: docs/stories/P2-03-catch-unwind-hardening.md] — P2-03 established the all-threads catch_unwind pattern
- [Source: docs/stories/P2-05-per-elite-spec-condition-presets.md] — P2-05 added Harbinger dispatch
- [Source: docs/stories/P2-02-condition-dispatch-integration-test.md] — P2-02 established ConditionWeights parameter pattern
- [Source: docs/stories/P1-03-main-view-decomp.md] — P1-03 established main_view/ submodule structure
- [Source: crates/addon/src/lib.rs] — `reset_state()` function location
- [Source: crates/addon/src/ui/main_view/] — submodule structure post-P1-03
- [Source: crates/optimizer/src/combat.rs:235-248] — `condition_weights_for_profession()` dispatch with Harbinger

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List
