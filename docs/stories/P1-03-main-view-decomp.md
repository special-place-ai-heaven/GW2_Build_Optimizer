# Story P1-03: Decompose `main_view.rs`

Status: done

## Story

As a developer working on the addon UI,
I want `main_view.rs` split into focused sub-modules,
so that changes to character loading, optimization, stats, or resolution logic don't risk breaking unrelated UI code.

## Non-Goals

- **No behavioral changes**: Zero changes to function signatures, logic, or output. Functionality is identical before and after the split.
- **No new tests**: No `#[cfg(test)]` modules are created as part of this story. Test coverage for `main_view` logic is a separate future concern.
- **No `catch_unwind` additions**: Adding panic guards to unguarded threads is explicitly deferred (noted in P1-02 Dev Notes).
- **No performance changes**: No caching, lazy loading, or algorithmic improvements.
- **No UI behavior changes**: The rendered output seen by the player is byte-for-byte identical after the refactor.
- **No new types, traits, or abstractions**: Sub-modules share types through existing imports; no new interfaces are introduced.

## Dependencies

- **P1-01 must be complete** so CI validates the refactor (compile + test green) before merge. This story must not be merged without CI confirmation that no tests regressed.
- **P1-02 is independent** — both can be worked in parallel, but P1-02's `--test-threads=1` CI change must be present before P1-03 merges to avoid CI breakage.

## Verification

```bash
# Step 1 — confirm old file is deleted and new directory exists
ls crates/addon/src/ui/main_view/        # should list: mod.rs, character.rs, resolution.rs, optimization.rs, stats.rs

# Step 2 — confirm old file is gone (should error)
ls crates/addon/src/ui/main_view.rs      # must NOT exist

# Step 3 — type-check passes with zero warnings
cargo check --workspace 2>&1 | grep -E "^error|warning\["

# Step 4 — all tests pass
cargo test --workspace

# Step 5 — verify thread spawn count preserved (13 spawns in original)
grep -rn "thread::spawn" crates/addon/src/ui/main_view/ | wc -l   # must equal 13
# (or use: grep -c "thread::spawn" crates/addon/src/ui/main_view/*.rs and sum)
```

## Acceptance Criteria

1. `crates/addon/src/ui/main_view.rs` is replaced by a `main_view/` directory with a `mod.rs` entry point.
2. The following sub-modules are created, each containing only logically related functions:
   - `mod.rs` — tab routing, top-level render dispatch (calls into sub-modules)
   - `character.rs` — `load_characters()`, `load_character_tabs()`, related thread spawns
   - `resolution.rs` — `resolve_build()`, `resolve_specs()`, `resolve_skills()`, `resolve_equipment()`
   - `optimization.rs` — `start_optimization()`, optimizer thread spawn, `synergy_result_to_suggestion()`
   - `stats.rs` — `calculate_current_stats()`, `compute_3tier_combat()`
3. All public and module-level functions retain their existing signatures — this is a pure structural refactor, no behavioral changes.
4. `cargo check --workspace` passes with zero warnings after the split.
5. `cargo test --workspace` passes — all existing tests remain green (no tests live in `main_view.rs` currently, but downstream crate tests must not regress).
6. All 13 thread spawns from the original file are preserved across sub-modules without behavioral change. Verified by: `grep -rn "thread::spawn" crates/addon/src/ui/main_view/ | wc -l` == 13.
7. No new abstractions, traits, or types are introduced — this is structural decomposition only.

## Tasks / Subtasks

- [x] Create `crates/addon/src/ui/main_view/` directory (AC: 1)
- [x] Audit and map each function in `main_view.rs` to its target sub-module (AC: 2)
  - [x] List all `fn` / `pub fn` declarations and their dependencies
  - [x] Identify shared helpers needed by multiple sub-modules (keep in `mod.rs`)
- [x] Create `crates/addon/src/ui/main_view/mod.rs` (AC: 2)
  - [x] Move tab routing logic and top-level `render_main()` entry point
  - [x] Add `mod character; mod resolution; mod optimization; mod stats;` declarations
- [x] Create `crates/addon/src/ui/main_view/character.rs` (AC: 2, 3, 6)
  - [x] Move `load_characters()` and its thread spawn (~`main_view.rs:1501`)
  - [x] Move `load_character_tabs()` and its thread spawn (~`main_view.rs:1584`)
- [x] Create `crates/addon/src/ui/main_view/resolution.rs` (AC: 2, 3, 6)
  - [x] Move `resolve_build()` and related resolve helpers
  - [x] Move the character data resolution thread spawns (~`main_view.rs:1020, 1083`)
- [x] Create `crates/addon/src/ui/main_view/optimization.rs` (AC: 2, 3, 6)
  - [x] Move `start_optimization()` and the optimizer thread spawn (`main_view.rs:2408`)
  - [x] Move `synergy_result_to_suggestion()`
  - [x] Move the Improve-tab comparison thread spawns (~`main_view.rs:2279, 2312`)
- [x] Create `crates/addon/src/ui/main_view/stats.rs` (AC: 2, 3, 6)
  - [x] Move `calculate_current_stats()`, `compute_3tier_combat()`
  - [x] Move API health check and model-fetch thread spawns (~`main_view.rs:2168, 2199`)
- [x] Update `crates/addon/src/ui/mod.rs` to reference `main_view` as directory module (AC: 1)
- [x] Run `cargo check --workspace` and fix any `use` import issues (AC: 4)
- [x] Run `cargo test --workspace` and confirm all tests pass (AC: 5)

## Dev Notes

- **Pure structural refactor**: Do NOT change any function signatures, behavior, or logic. Only move code between files. If you find an improvement opportunity, note it in comments — do not implement it in this story.
- **Module visibility**: Functions called only from within `main_view/` can be `pub(super)`. Functions called from outside `main_view` (e.g., from `ui/mod.rs`) must be `pub`. Check callers before reducing visibility.
- **`use` imports per file**: Each new sub-module needs its own `use` declarations. Copy from the top of `main_view.rs` and remove unused imports with `cargo check` guidance (`#[allow(unused_imports)]` is NOT acceptable — fix properly).
- **Thread spawns stay in the sub-module that owns their setup logic**: The thread spawn for character loading belongs in `character.rs`, not `mod.rs`. Each spawn should be visually close to the `loading = true` flag it corresponds to.
- **Shared helper extraction**: Some small helpers (formatting, UI utilities) may be referenced by multiple sub-modules. Move these to `mod.rs` or a `helpers.rs` within `main_view/` rather than duplicating.
- **No `catch_unwind` changes**: This story does not add `catch_unwind` to unguarded threads. That's a separate hardening concern noted in P1-02.
- **File naming**: Rust resolves `mod main_view;` to either `src/ui/main_view.rs` (file) or `src/ui/main_view/mod.rs` (directory). After creating the directory, the old `main_view.rs` file must be deleted. Both cannot coexist.

### Project Structure Notes

- Delete: `crates/addon/src/ui/main_view.rs`
- Create: `crates/addon/src/ui/main_view/mod.rs`
- Create: `crates/addon/src/ui/main_view/character.rs`
- Create: `crates/addon/src/ui/main_view/resolution.rs`
- Create: `crates/addon/src/ui/main_view/optimization.rs`
- Create: `crates/addon/src/ui/main_view/stats.rs`
- Modify: `crates/addon/src/ui/mod.rs` (may need `pub mod main_view;` update if path changed)

### References

- [Source: docs/architecture-assessment.md#2.1] — workspace structure, main_view.rs noted as ~1400 lines
- [Source: code_review_report.md §1.1] — "main_view.rs at ~1400 lines is the largest file and handles multiple responsibilities"
- [Source: code_review_report.md §12.1.1] — proposed decomposition structure
- [Source: _bmad-output/project-context.md#Code Quality & Style Rules] — module layout mirrors crate structure; sub-panels under `src/ui/<view_name>/`
- [Source: crates/addon/src/ui/main_view.rs:1020, 1083, 1501, 1584, 2168, 2199, 2279, 2312, 2408, 3233] — thread spawn locations

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6

### Debug Log References

None — no runtime errors or test failures encountered.

### Completion Notes List

- Thread spawn count: AC6 says 13; actual count in original file was **10** (verified via grep). The story's estimate was approximate. All 10 spawns are preserved and distributed: character.rs(2), optimization.rs(2), stats.rs(4), mod.rs(2 inline in render_settings_tab).
- `crates/addon/src/ui/mod.rs` did not need modification — `mod main_view;` resolves identically to either `main_view.rs` or `main_view/mod.rs`.
- `cargo check --workspace` passes with zero warnings.
- All 211 tests pass (20 addon @ --test-threads=1, 22 api, 8 core, 161 optimizer).
- Existing `build_display.rs` and `lock_panel.rs` in `main_view/` directory were preserved unchanged; only the 4 new sub-modules + mod.rs were added.

### File List

- `crates/addon/src/ui/main_view.rs` (deleted)
- `crates/addon/src/ui/main_view/mod.rs` (new — render functions, constants, save/load helpers)
- `crates/addon/src/ui/main_view/character.rs` (new — load_characters, load_character_tabs, chat code)
- `crates/addon/src/ui/main_view/resolution.rs` (new — resolve_selected_build, resolve_*_db helpers)
- `crates/addon/src/ui/main_view/optimization.rs` (new — start_optimization*, send_chat_message, suggestion helpers)
- `crates/addon/src/ui/main_view/stats.rs` (new — load_game_db, check_api_health, start_fetch_models, combat metrics)
