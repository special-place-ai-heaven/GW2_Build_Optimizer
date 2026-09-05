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
