# Quickstart: Trait triggers are the build (007)

Validation guide for a finished increment. Commands run from the repo root in Git Bash; prefix `MSYS_NO_PATHCONV=1` whenever a test filter contains `::`. Acceptance is automated (spec clarification 2): an increment ships when every section below is green.

## Prerequisites

- Rust stable; the workspace builds on `255371f`.
- `dev.cfg` with `addons_dir` (the cache and the character tabs live under it). Needed by §4 and §5 only.
- `dev.cfg` with `crawl_url` (the crawl4ai base URL) for §5's wiki check; without it the check skips and says so.

## 1. The mechanism, on the fixture

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib necro_ -- --nocapture
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_ -- --nocapture
```

Expected: every `necro_*` and `reaper_*` experiment passes, including:

- shroud triggers: `necro_shroud_enter_fires_once_at_entry` (US1.1), `necro_in_shroud_bonus_active_only_inside` (US1.2), `necro_shroud_exit_fires_for_every_why` (US1.3), `necro_desert_shroud_is_the_scourge_entry` (US1.4), `necro_removed_trait_changes_results` (US1.5), `necro_results_repeat_identically` (US1.6)
- prerequisites and scopes: `necro_chilled_prerequisite_gates_chilling_nova` (US2.1), `necro_shout_scope_fires_on_shouts_only` (US2.2), `necro_fear_applied_fires_dread` (US2.2), `necro_periodic_and_exit_life_force` (US2.3), `necro_prerequisite_never_met_is_traced_not_listed` (edge)
- population: `population_roam_credits_one`, `population_havoc_credits_five_or_cap`, `population_cloud_caps_at_record`, `support_builds_rank_apart_on_ally_trait` (SC-003)
- coverage: `coverage_line_never_names_executed_weapon_skills` (US4.1), `coverage_entry_carries_its_class` (US4.2), `nothing_skipped_is_verified` (US4.3)

## 2. Seen-failing evidence

```bash
python docs/audit/disable_and_run.py --list
python docs/audit/disable_and_run.py shroud_enter prereq scope population coverage
```

Expected: each control fails with the quoted block in `docs/audit/sprint3-failures.md`, then the file is byte-identical. Section 9 of `docs/simulator-connection-audit.md` quotes every control.

## 3. Determinism, trace cap, timing

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib wvw_timeline::reaper_experiments::reaper_results_repeat_identically
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib wvw_timeline::reaper_experiments::reaper_trace_fits_under_cap
cargo test -p gw2-optimizer --lib 2>&1 | tail -3
```

Expected: identical runs; trace under 512 on both profiles; the lib harness within 10 percent of the Sprint 2 baseline (16.6–19.0 s), SC-006.

## 4. The cached Reaper build and the audit table (SC-001, SC-002)

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib reaper_cached_build_traits_are_simulated -- --ignored --nocapture
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib trait_coverage_audit_lists_every_trait -- --ignored --nocapture
```

Expected: the first prints the nine traits with their state and at most two on the "Not simulated" line, each with a class. The second rewrites `docs/audit/trait-coverage.md`; the summary shows `NoRecord 0` for every shipped profession and 999 rows in total. Commit the regenerated table with the increment.

## 5. Records against the wiki (FR-010a, SC-007)

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib records_match_their_wiki_pages -- --ignored --nocapture
```

Expected: `ok` for every record of the shipped professions; no `MISMATCH`. Paste the run's summary line into the audit section for the increment.

## 6. PvE and PvP pins (FR-009, SC-004)

```bash
MSYS_NO_PATHCONV=1 cargo test -p gw2-optimizer --lib pve_output_unchanged
cargo test -p gw2-optimizer --test scoring_regression
```

Expected: identical to the last printed digit.

## 7. Gates and release of an increment

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

Then: bump the patch version, CHANGELOG section naming the profession and the trigger kinds it added, commit `release: x.y.z`, push, PR, `gh release create` with the DLL and `SHA256SUMS.txt`. The user merges the PR. No in-game check is asked for this sprint.

## 8. What not to see

- No edit under `crates/optimizer/src/prompts.rs`, `crates/optimizer/src/llm/`, `crates/optimizer/examples/choya_live.rs`, or the addon's UI.
- No change to any `*_NORM`, `WEIGHT_BUDGET`, gate threshold or objective profile; the only rank-key change is the support-kind ally-boon slot in `contracts/wvw-report.md`.
- No fixture id or value inside `data/`; no wiki page text committed.
- No machine path in code; `dev.cfg` supplies `addons_dir` and `crawl_url`.
