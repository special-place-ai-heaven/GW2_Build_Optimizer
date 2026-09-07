# Quickstart: WvW proc firing sites

Validation guide for the finished sprint. Commands run from the repo root in Git Bash; prefix `MSYS_NO_PATHCONV=1` whenever a test filter contains `::`.

## Prerequisites

- Rust stable toolchain, the workspace builds on the Sprint 1 tip.
- `dev.cfg` with `addons_dir` pointing at the game's addons directory (the cached API skills and the Reaper character tabs live under it). Only SC-005 needs it; every other check runs on the synthetic fixture.

## 1. The four firing sites, on the fixture

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_ -- --nocapture
```

Expected: every `reaper_*` experiment passes, including the new ones:

- `reaper_oncrit_positive_control_fires_from_crits` (US1.1), `reaper_oncrit_zero_precision_never_fires` (US1.2), `reaper_oncrit_icd_bounds_rate` (US1.3), `reaper_oncrit_trials_bracket_expected_value` (US1.5, SC-009)
- `reaper_swap_loads_set_two_sigils` (US2.1, 2.2), `reaper_swap_keeps_icd_across_sets` (US2.3)
- `reaper_scholar_applies_only_above_threshold` (US3.1, 3.2), `reaper_thief_stacks_cap_and_expire` (US3.3), `reaper_unresolved_conditional_stays_named` (US3.4)
- `reaper_dark_whirl_life_steals` (US4.1), `reaper_expired_field_makes_no_combo` (US4.2)
- `reaper_shroud_refused_without_life_force` (US6.2), `reaper_shroud_drains_and_exits` (US6.3), `reaper_life_force_gain_capped` (US6.1)

The `unsupported control` from Sprint 1 (`reaper_unsupported_oncrit_is_named_not_zeroed`) is inverted: it now asserts the sigil is absent from the coverage line and present in the trace (SC-001).

## 2. Seen-failing evidence

Each control was run once with its mechanic disabled by the scripted edit under `docs/audit/` (restore from copies, not from git). The quoted failures are in `docs/simulator-connection-audit.md` section 8. Spot-check one:

```bash
python docs/audit/disable_and_run.py oncrit   # disables the OnCrit call, runs the control, restores
```

Expected: the control fails with the trace showing zero `ProcFired` events for the sigil, then the file is byte-identical to before.

## 3. Determinism (SC-004)

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib wvw_timeline::reaper_experiments::reaper_results_repeat_identically
```

Expected: ten runs of the fixture give identical `WvwCombatReport` values and identical trial means.

## 4. Real build, real records (SC-005)

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_cached_build_executes_recorded_sources -- --ignored --nocapture
```

Requires `dev.cfg`. Expected output lists Superior Sigil of Fire, Superior Rune of the Scholar and Relic of the Thief under "executed" and the coverage line without them. If the cache holds no Reaper WvW tab the test reports that and passes vacuously; say so in the completion report.

## 5. Completeness rule (FR-015)

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib resource_model_completeness_matches_previous_list
```

Expected: Thief, Revenant, Warrior, Mesmer and Necromancer complete; Guardian, Elementalist, Engineer, Ranger incomplete.

## 6. Gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

Expected: all green; optimizer lib harness within 10 s of the Sprint 1 baseline (16.8 s); addon and core counts unchanged plus the new tests; no locale change needed (no new UI string).

## 7. What not to see

- No edit under `crates/optimizer/src/prompts.rs`, `crates/optimizer/src/llm/`, `crates/optimizer/examples/choya_live.rs`.
- No change to any `*_NORM`, `WEIGHT_BUDGET`, gate threshold or objective profile.
- No fixture id or value inside `data/`.
- No `git push`, no version bump, no release.
