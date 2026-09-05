# Sprint verification

## Sprint 1 — ledger and plan

Commit: 0496b0b. Verified 273 unique tasks, 268 unique finding entries; git diff --cached --check passed. Specs are locally ignored, so only the eight named feature artifacts were explicitly force-added. Original docs/audit remains uncommitted.

## Sprint 2 — W011/W012 feedback history

- W011: fallible history load distinguishes NotFound from read/parse failure. Addon feedback state retains the load error across fresh FeedbackStore instances; dirty flushing refuses publication after failed load, including when the file later becomes readable. Error is visible through state and Nexus log.
- W012: feedback and taxonomy writes use the existing Windows-safe storage::replace_file routine.
- Red proof: Terminal Commander job job_01a0731133ea79c18ccf68fce1b268d4 ran the new addon regression: 1 failed, exit 101. Initial exact-name invocation matched zero tests and is not verification evidence.
- Green: core feedback store suite 7 passed (job_01a07312ef5e7d519105dd7df963d05c); addon feedback suite 60 passed (job_01a0731289f67fb3a9f596331e7e012b). Tests cover parse/read failure, first run, interrupted sends, repeated overwrite, taxonomy overwrite and session flush refusal.
- Scoped formatting and git diff --check pass. Strict core Clippy result is recorded in the sprint commit after completion.
- Limitation: a failed history load intentionally disables history writes until addon reload. Repair/recover the file before reloading; in-session new feedback state is not persisted while refusal is active.
- Workspace Clippy remains blocked by W001's pre-existing diagnostics; in-game acceptance and release build remain pending. These are scoped verified remedies, not campaign completion.
- Other agent owns concurrent Cargo version/lockfile and LLM/chat/scraper changes; they are excluded from this sprint commit.

## Sprint 3 — W006/W002 Lock All bounds and reserved-address guard

Committed as 1f85f27 by the other agent's session partner after that session hit its quota mid-sprint; the code and its tests were authored there, this entry records only what was independently checked before committing.

- W006: the Lock All spec/trait mutation moved into `lock_current_specs`, bounded by `locks.specs.len()`. Regression covers four input specs against a three-slot array and a specialization whose `major_traits` is not nine long.
- W002: `news_art::url_host_is_reserved` and `normalized_host` now serve news stills, station logos and the stream connect. `player::stream_host_reserved` and `logos::host_resolves_reserved` both delegate; the duplicated bracket/parse/resolve blocks are gone.
- Additional finding inside W002's remedy: `ip_is_reserved` did not unwrap IPv4-mapped IPv6, so `::ffff:127.0.0.1` answered false to `Ipv6Addr::is_loopback` and passed both guards. It now checks `to_ipv4_mapped` first, covered by `stream_guard_rejects_reserved_ipv6_literals` and `dns_screen_rejects_reserved_literals_and_garbage`.
- Green: `cargo test --workspace --no-fail-fast` at 1f85f27, all suites pass. Two wall-clock tests flake under parallel load and pass in isolation: `gw2-api client::tests::fetch_bytes_rejects_a_body_over_the_icon_cap` and `gw2-optimizer scraper::tests::scrape_guildjen_aborts_at_inner_loop_when_cancelled` (500 ms budget, mostly reqwest client construction; predates this campaign at d88cb32). Neither is a regression; the scraper one is an open flaky-test finding for this campaign.
- `cargo check` clean. Formatting left alone: the tree carries pre-existing `cargo fmt` drift across files outside this sprint, and reformatting them would bury the diff.
- Workspace Clippy still blocked by W001. In-game acceptance pending: the release build ships as v1.11.29 for the player to exercise.

## Sprint 4 — W001 strict Clippy

- W001: the ten deny-warnings diagnostics are gone. `quip_for` uses `is_multiple_of`; `toggle` boxes the large `Play` station; radio sort uses `sort_by_key`; gold-button width takes owned `t()` strings; scraper luminary guard uses `KNOWN_SPECS.contains`; news stills share `exceeds_max_edge` between `download` and the resize test.
- CI still runs `cargo clippy --workspace --all-targets -- -D warnings`. Local run of that command exits 0.
- Green: `news_art` lib tests 13 passed / 1 ignored; `extract_traits_includes_luminary_from_shared_known_specs` passed.
- No version bump: this sprint is the CI gate, not a player-facing DLL. In-game acceptance for US1 remains the existing 1.11.29/1.11.30 handoff.

## Sprint 5 — B001 validated Choya plate stats

- B001: `attach_chat_stats` takes the accepted `ValidatedBuild` and plates `calculate_validated_stats` plus the returned modifiers. Validator warnings and gear-quality reasons go on `quality_reasons`; chat narrative includes warnings with errors.
- Mixed Sentinel helm on a Berserker kit now matches the referee sheet (not a uniform prefix estimate). PvP uses the amulet, not a land kit.
- Green: `attach_chat_stats_uses_validated_mixed_slots`, `attach_chat_stats_pvp_uses_amulet_not_land_kit`, leftover-kit tests.
- Version 1.11.31. In-game still pending: mixed Sentinel/Dragon Choya plate toughness/vitality and ranking.

## Sprint 6 — W025 simulate_rotation prefix errors

- W025: `exec_simulate_rotation` no longer invents 2000 power / 1000 condition damage. Unresolved prefixes return `{"error":"No stat sheet for ..."}` with no `dps` object.
- The duration-clamp tool test now seeds a priceable Berserker's prefix so it still exercises the simulator after the fallback was removed.
- Green: `simulate_rotation_errors_when_prefix_cannot_be_priced`, `duration_seconds_is_clamped`.

## Sprint 7 — W035 simulate_rotation uses resolved SimParams

- W035: the LLM `simulate_rotation` tool builds `SimParams` from base+prefix (precision, ferocity, mode fury, duration mults, derived health/armor) and calls `simulate_with`. `simulate` / `simulate_against` are test-only wrappers around `SimParams::basic`.
- Green: `simulate_rotation_uses_resolved_crit_not_basic_defaults` (Berserker strike ≠ `basic`, equals `simulate_with`); `duration_seconds_is_clamped`; `simulate_rotation_errors_when_prefix_cannot_be_priced`; 34 simulator unit tests.
- No version bump: tool-path correctness, not a player-facing DLL. Other agent stays on Choya/referee.

## Sprint 8 — W044 normalized-effect patch mismatch

- W044: `effects_for_mode` no longer ignores patch. It resolves the active manifest, then `inherits_from`. Exact `effects_for("2026-07-15")` stays None; inherited rows keep patch_id `2026-01-13`. Historical JSON was not copied.
- Green: `test_normalized_effects_patch_ids_exist_in_manifests`, `test_effects_for_unknown_returns_none`, two wvw_timeline tests that still go through `effects_for_mode`.
- Did not touch `engine.rs` (dirty in this checkout), `scraper.rs`, `chat_flow.rs`, or locales.

## Sprint 9 — W013 manifest-backed BalanceContext.patch_id

- W013: `SNAPSHOT_PATCH_ID` deleted. `new()` reads `latest_manifest()`. `for_patch` is the historical constructor. Unknown patches are `!patch_is_known`. `live_build_mismatch` wraps `check_staleness` for W015.
- Green: `new_uses_the_active_manifest_not_a_hand_edited_literal`, `for_patch_keeps_historical_ids_and_flags_unknown`, `live_build_mismatch_is_observable`.
- Did not bump Cargo.toml — other agent owns 1.11.32.

## Sprint 10 — W015 live-build staleness is visible

- W015: `freshness()` / `ManifestFreshness` sit next to `check_staleness`. The API health check stores the warning on `MainState.manifest_staleness`. Status bar chip + About hero show it.
- Green: `freshness_is_current_or_stale`, existing staleness tests, `live_build_mismatch_is_what_the_health_check_stores`.
- Did not bump Cargo.toml or touch locales / scraper / chat_flow.

## Sprint 11 — W016 data initialize at startup

- W016: `state::init` runs `data::initialize()`. `Disabled` is stored, logged, shown, and blocks Optimize/Improve. `Degraded` is not treated as Ready.
- Green: `test_initialize_returns_ready`, `disabled_blocks_optimize_and_is_not_ready`.
- Did not bump Cargo.toml.

## Sprint 12 — W039 unmodeled procs + W038 Protection formula

- W039: unsupported `trigger_procs` categories and zero-duration `OutgoingHealingPct` increment `unmodeled_effect_sources` once per source, not once per tick.
- W038: incoming and outgoing Protection use `boons().protection_multiplier()` cached on `Timeline`.
- Green: `unsupported_proc_category_counts_once_per_source`, `protection_uses_formula_multiplier`; 35 `wvw_timeline` unit tests.
- Did not bump Cargo.toml — other agent owns 1.11.32. Leftovers in news/theme/comparison/fonts stay theirs.

## Sprint 13 — W008 one skill-string parser

- W008: `gear_diff::parse_suggestion_skills` is the only parser. It understands `Utils:` (comma-split) and `Utility:`. Rotation name lookup, chat-code skill selection, and pet rows call it. Did not type `BuildSuggestion` (lives on their dirty `comparison.rs`). Did not touch `fill_holes_from_loadout`.
- Green: 6 `gear_diff` tests including `test_parse_skills_utils_comma_list_and_unlabeled`.
- Did not bump Cargo.toml.

## Sprint 14 — W004 log config and chat-history save failures

- W004: toggle/persist window config saves and `save_history` log through `log_disk_error` instead of `let _ =`.
- Green: `toggle_persist_reset_do_not_save_under_state`, `kitchen_history_roundtrips_on_disk`, `save_history_logs_when_directory_is_missing`.
- Isolated `chat_bar.rs` from their rustfmt leftovers. Did not bump Cargo.toml.

## Sprint 15 — W082 ranch-notes spawn fallback

- W082: if `ranch-notes` also fails to spawn, `save_note_now` writes the dirty notes snapshot on the click frame.
- Green: `ranch_load_click_handler_spawns_worker_instead_of_inline_cpu`, `ranch_load_click_handler_does_not_persist_notes_on_click_frame`.
- Did not bump Cargo.toml.

## Sprint 16 — W182 palette gate matches the beam

- W182: `select_skills` no longer waives the palette gate on an empty map. The WvW diag fixture now gives each synthetic skill a palette id.
- Green: `optimize_synergy_wvw_selects_required_bar_utilities`, `select_skills_skips_heals_without_template_palette`.
- Did not bump Cargo.toml.

## Sprint 17 — W157 saturate timeline clocks + W231 SSE status clamp

- W157: `Timeline::at` saturates duration math; simulator next-action/control ends do the same.
- W231: provider error codes above `u16::MAX` become 502, not a wrapped 429.
- Green: `timeline_at_saturates_near_u32_max` (36 `wvw_timeline` tests), `out_of_range_error_code_falls_back_to_502` (16 `sse` tests).
- Did not bump Cargo.toml.

## Sprint 18 — US3 dead duplicates (W003, W010, W014, W017, W018, W020)

- W003: radio UI calls `player::saved_from_station` / `player::station_from_saved`; local copies gone.
- W010: no per-frame 800×600 migrate; `LEGACY_FIRST_WINDOW_SIZE` deleted.
- W014: unused no-op `check_wvw_quality` removed.
- W017/W018: unused `score_effect` / `map_legacy_effect` and exclusive helpers removed; live synergy scorer stays.
- W020: `patch_ledger` is test-only; `serde_yaml` is a dev-dependency.
- Green: `station_round_trips_through_the_saved_snapshot`, `window_init_is_only_missing_size_or_forced_snap`, `test_wvw_no_known_split_uses_base_value`, `test_baseline_data_loads_and_validates`, `test_ledger_for_patch_found`, `test_patch_ledger_ids_exist_in_manifests`, `test_initialize_returns_ready`.
- Did not bump Cargo.toml.

## Sprint 19 — W037/W042/W045 + US4 W046/W047

- W037: Timeline identity multipliers/duration bonuses removed; coverage counters stay.
- W042: unconstructible legacy `BenefitsFromStatus` / `ProcEffect` / `ProcTrigger` / `EnablerPayoff` removed.
- W045: docs name `optimize_v2` → deterministic cancellable → legacy cancellable.
- W046: `.gitignore` ignores every `.symforge/` directory.
- W047: `BOOTSTRAP_FAILED` doc matches the no-retry latch.
- Did not bump Cargo.toml.

## Sprint 20 — US4 docs and dead UI (W050–W052, W054, W057, W088)

- W050/W051: radio art docs match the foreground quip bubble and five head anchors; `EQ_WARM` / `let a = alpha` gone.
- W052: `finish_stopped` already had its own doc; no edit needed.
- W054: setup routing drops the unreachable Gw2ApiKey arm.
- W057: `strip_label_ci` already shared by both gear_diff parsers (W008).
- W088: unused `render_presets` deleted; `PRESETS` stays for tests.
- Did not bump Cargo.toml.
