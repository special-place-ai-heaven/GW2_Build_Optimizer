# Story P2-04: Trait Fuzzy-Match Minimum Length Guard

Status: done

## Story

As a user running the Gemini-powered optimization pipeline,
I want trait name matching to reject overly short search terms before falling back to contains-matching,
so that a short LLM-hallucinated trait name like "Force" or "Swift" cannot silently match an incorrect longer trait.

## Non-Goals

- **No ratio-based guard**: The absolute `needle.len() >= 5` guard is sufficient. The proportional guard (60% ratio) is a known alternative but is not implemented in this story.
- **No changes to `find_skill_by_name()`**: The skill fuzzy-match function is separate and not touched.
- **No changes to spec or relic matching**: Only `find_trait_by_name()` is modified.
- **No changes to exact-match logic**: The exact-match path (lines 475-481) is unchanged and has no length restriction.
- **No UI changes**: The guard is internal to validation; no error messages or fallback behavior changes surface to the user beyond a failed trait match being handled by the existing validation error flow.

## Dependencies

- **P1-01 should be complete** so CI validates this change.
- **P1-02, P1-03, P2-01 are fully independent** — no file overlap whatsoever. This story can be worked at any point in the sequence.

## Verification

```bash
# Existing tests pass without modification
cargo test --package gw2_optimizer -- validation

# New tests specifically
cargo test --package gw2_optimizer -- test_find_trait_short_needle_no_contains_match
cargo test --package gw2_optimizer -- test_find_trait_needle_ge5_contains_match

# Full count: validation module should now have 6 tests (4 existing + 2 new)
cargo test --package gw2_optimizer -- validation --list | grep "test " | wc -l
# Expected: 6

# Confirm only validation.rs was modified
git diff --name-only  # should show only crates/optimizer/src/validation.rs
```

## Acceptance Criteria

1. `find_trait_by_name()` in `crates/optimizer/src/validation.rs` (lines 473-492) adds a minimum needle length guard before the contains fallback at line 488.
2. The guard rejects the contains fallback when `needle.len() < 5`. Needles shorter than 5 characters fall through with no match (return `None`) rather than over-matching on long trait names.
3. Exact-match behavior (`validation.rs:475-481`) is unchanged — exact matches always succeed regardless of needle length.
4. The existing 4 unit tests in `validation.rs` all pass without modification.
5. Exactly 2 new tests verify the guard:
   - `test_find_trait_short_needle_no_contains_match`: needle `"swif"` (4 chars) is a substring of trait name `"Swift Retribution"`. Exact match fails (not equal). Contains fallback is skipped by the length guard. Result: `None`.
   - `test_find_trait_needle_ge5_contains_match`: needle `"valor"` (5 chars) is a substring of trait name `"Valorous Recovery"`. Exact match fails. Contains fallback fires (length >= 5). Result: `Some(trait for "Valorous Recovery")`.
6. No behavioral changes to `find_skill_by_name()` or any other function in `validation.rs`.

## Tasks / Subtasks

- [x] Add minimum length guard to `find_trait_by_name()` contains fallback (AC: 1, 2)
  - [x] In `validation.rs:486-491`, wrap the `.find(|t| ...)` call with a `needle.len() >= 5` check
  - [x] If needle is shorter than 5 chars and no exact match, return `None`
- [x] Verify existing tests still pass (AC: 4)
  - [x] Run `cargo test --package gw2-optimizer -- validation` and confirm existing tests pass
- [x] Add new tests (AC: 5)
  - [x] `test_find_trait_short_needle_no_contains_match` — needle of length 4, substring of a trait name, should return None
  - [x] `test_find_trait_needle_ge5_contains_match` — needle of length ≥ 5, substring of exactly one trait, should return Some

## Dev Notes

- **Exact implementation** (minimal change):
  ```rust
  // Contains match: only check if trait name contains the search needle.
  // Do NOT check the reverse (needle contains trait name) — that causes
  // "Empowered" to match input "Power" or "Swift" to match "Swift Empowerment".
  // Minimum needle length guard: short needles (< 5 chars) over-match on long trait names.
  if needle.len() < 5 {
      return None;
  }
  major_traits
      .iter()
      .find(|t| t.name.to_lowercase().contains(&needle))
      .copied()
  ```

- **Why 5 characters**: Shortest meaningful GW2 trait names are typically 4-5 characters (e.g., "Force", "Valor", "Swift"). A 5-char minimum rejects 1-4 char hallucinations ("Pow", "Fire", "Bleed") while preserving legitimate short-name contains matches for names like "Valor" → "Valorous Recovery".

- **Alternative approach** (ratio guard): Instead of absolute length, require `needle.len() * 100 / trait_name.len() >= 60` (needle ≥ 60% of trait name length). This is more precise but adds complexity for a 30-minute fix. The absolute `>= 5` guard is sufficient for the identified risk.

- **Do NOT change exact-match logic** at `validation.rs:475-481`. Exact matches work correctly and should not have a length restriction.

- **Test setup**: The test module in `validation.rs` already creates mock `Trait` objects inline. Follow the same pattern for the new tests — construct a `Vec<Trait>` with known trait names and pass them directly to `find_trait_by_name()` rather than using a full `GameDb`.

- **No impact on `find_skill_by_name()`** at `validation.rs:494-520` — that function is separate and has its own matching logic. Do not modify it in this story.

### Project Structure Notes

- Modify: `crates/optimizer/src/validation.rs` only — 2-line guard addition + 2 new test functions.
- No other files need changes.

### References

- [Source: docs/production-readiness-backlog.md#P2-04] — blast-radius assessment, "contains-match without minimum length guard"
- [Source: crates/optimizer/src/validation.rs:475-492] — find_trait_by_name() implementation
- [Source: crates/optimizer/src/validation.rs:486-491] — contains fallback (the specific lines to modify)
- [Source: _bmad-output/project-context.md#LLM Provider Rules] — "validate_gemini_build() is not optional"
- [Source: _bmad-output/project-context.md#Testing Rules] — native #[test], test_verb_condition naming

## Dev Agent Record

### Agent Model Used

claude-sonnet-4-6

### Debug Log References

- Compile error: `make_trait` helper was missing the `skills: Vec<TraitSkill>` field added to the `Trait` struct. Fixed by adding `skills: vec![]`.
- `cargo test --package gw2-optimizer -- validation` → 7 passed (5 pre-existing + 2 new), 0 failed.
- Full workspace: 211 passed, 0 failed, 12 ignored.
- `git diff --name-only` confirms only `crates/optimizer/src/validation.rs` modified by this story (`state.rs` is a prior-story P1-02 uncommitted change).

### Completion Notes List

- Added 2-line minimum-length guard (`if needle.len() < 5 { return None; }`) before the `contains` fallback in `find_trait_by_name()` at `validation.rs:486-492`. Exact-match path is unchanged.
- Added `make_trait()` helper and 2 boundary tests to the existing `#[cfg(test)] mod tests` in `validation.rs`.
- `test_find_trait_short_needle_no_contains_match`: needle `"swif"` (4 chars) — guard fires, returns `None`.
- `test_find_trait_needle_ge5_contains_match`: needle `"valor"` (5 chars) — guard passes, contains match fires, returns `Some(trait)`.
- No changes to `find_skill_by_name()`, exact-match logic, or any other file.

### File List

- `crates/optimizer/src/validation.rs` (modified — guard + 2 tests + `make_trait` helper)
