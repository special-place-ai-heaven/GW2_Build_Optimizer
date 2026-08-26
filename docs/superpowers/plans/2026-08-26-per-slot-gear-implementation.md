# Per-Slot Gear Prefixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every gear piece carries its own single-stat prefix, independently searchable, lockable, savable, and plateable — full in-game gear flexibility.

**Architecture:** Slot-vector rewrite (approach B): `ValidatedBuild`/`SavedBuild` replace build-wide + group prefix fields with a 16-slot map; three-tier inheritance collapses into one load-time migration; search gains a per-slot operator alongside uniform/group coarse moves; all consumers migrate per the inventory below. Weapon semantics adopt model C (active-set aware) everywhere.

**Tech Stack:** Rust workspace, serde, existing beam search.

**Spec:** `docs/superpowers/specs/2026-08-26-per-slot-gear-design.md` (includes §12 red-team addendum — weapon semantics, rotation deletion, Choya gear policy).

## Global Constraints

- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all` green at every task boundary.
- Old saves load with byte-identical stats (except the weapon-model alignment, which is asserted separately per T2.2).
- Locked slots never change under any operator.
- Determinism: same inputs → same slot vector.
- Never print API keys; mock servers only in tests.

## Appendix: consumer inventory (from adversarial review — the authoritative work list)

### crates/core
- src/types.rs
  - GearPrefixGroups struct def:315-338 (+ inherit_empty 325-338)
  - SavedBuild.stat_prefix:361; SavedBuild.gear_prefixes:364-365
  - ResolvedWeaponSet.stat_prefix:169-176 (line 172)
  - ResolvedGearPiece.stat_prefix:194-200 (196)
  - tests:393-407 empty_gear_groups_inherit_stat_prefix
  - BuildLocks struct 57+, describe_constraints 93-... (gear_locks ripple)
- src/storage.rs
  - save_overwrite:87-91; test fixture 207-211 round-trip using GearPrefixGroups::default + stat_prefix; legacy JSON fixtures 269-271, 304-306
- src/feedback/report.rs
  - BuildSnapshot.stat_prefix:36, .gear_prefixes:37 (wire schema, serde Serialize/Deserialize)
  - test sample 147-153; allowlist list 321-324

### crates/optimizer
- src/types? none (gw2_core).
- src/validation.rs
  - ValidatedBuild.gear_prefix:35, gear_groups:38; ValidatedGearPrefix 86-90; ValidatedGearGroups 92-97; effective_prefix_ids 99-125; gear_identity 122-125
  - validate_gemini_build calls validate_gear_prefix:351
  - validate_gear_prefix:809-866 sets result.gear_prefix:853; RejectCode::GearPrefixNotFound:860
  - test construction 1528-1531; tests/:live_llm.rs:32/451-471 prints gear_prefix name
- src/engine.rs
  - optimize:119+ gear_candidates=search_gear_prefixes:164; optimize_pvp:344-349 amulet-replaces-gear comment, empty GearCandidate "(PvP Amulet)" :397
  - calculate_candidate_stats:537-563 (skips B1/B2:549; from_api_slot:551; add_budget:560)
  - apply_optimized_gear_stats:566-600 (PvP branch match_pvp_amulet:580; add_budget:597; skips B1/B2)
  - match_pvp_amulet pub:603-618
  - add_budget_stats_for_itemstat pub:713-731
  - parse path stat_prefix override:1045-1049
  - apply_validated_gear_stats:~1518-1551 (fallback gear_prefix:1523; groups read:1524; PvP branch:1525-1527; equipment loop w/ active_land_weapon_budget:1540-1547; group lookup:1550-1560ish; add_budget:1558)
  - active_land_weapon_budget:~1553-1570
  - llm_advisor:2139-2260 (current.gear_prefix prompt:2141-2144; SWAP parse gear_prefix=:2210-2225 sets candidate.gear_prefix:2217; referee eval)
  - select_gemini_gear_prefixes:922-927; build_pre_computed_gemini_context:931-950
- src/search.rs
  - GearCandidate{slot_stats HashMap<String,u32>, stat_prefix_name}:14-20
  - STAT_SLOTS:23-38; TRINKET/WEAPON/ARMOR consts:41-44
  - search_gear_prefixes:52-115; build_mixed_candidate:117-135
- src/search_v2.rs
  - BeamCandidate:28-31; generate_neighbors registers swap_gear_prefix:86 / swap_gear_groups:87; neighbor-cap comment:66-69
  - optimize_v2_search beam dedup via gear_identity:235-239
  - swap_gear_prefix:291-324 (dedup check 302-308; sets gear_prefix:317 + three groups:318-320)
  - swap_gear_groups:329-357 (group read/write 342-355)
  - normalized_prefix_name:390-...; prioritized_itemstats:365-388 uses select_gear_prefix:369
  - test Plaguedoctor assertion:1477-1481
- src/scoring.rs
  - select_gear_prefix:753-790 (+tests 1356-1411); GEAR_PROFILES stem matcher:808-812
- src/synergy_pipeline.rs
  - optimize_synergy takes gear_prefix_name:90,159,175 (from select_gear_prefix doc:4)
  - rank_and_select:1185-1265 resolves gear_prefix_id:1206 → compute_candidate_stats:1227
  - compute_candidate_stats:1267-1297 calls engine::apply_optimized_gear_stats:1272
  - sets validated.gear_prefix:1318-1322
- src/gemini_tools.rs
  - decl_calculate_stats required gear_prefix:328-333; decl_simulate_combat:345-355; decl_score_build:367-372; decl_get_build_synergy_report:499+
  - handle ctx gear_prefix param use:817-821; candidates JSON out gear_prefix:991
  - estimate_prefix_stats:1558-1560 (+budget loop/add_budget call:1550-1552)
- src/benchmark.rs
  - BenchmarkBuild.gear_prefix:26; BenchmarkDelta.ref_gear_prefix:67; score_benchmark_build:146-175; compute_benchmark_delta:191-210
- src/prompts.rs
  - GeminiBuildResponse.stat_prefix:842 (+sigils_map pre-existing per-slot pattern 845-848)
  - parse:stat_prefix:946-949; enforcement strings:94-99, 348-351; templates embed stat_prefix:154-158, 240-244, 300-303, 425-442, 511-515, 583-587, 648-652; kitchen/chef prompts embed candidate.gear.stat_prefix_name:697-699; chat_refinement tool list context:445-522
  - parse_gemini_build:855-...
- src/data/slot_budgets.rs
  - include_str json; SlotType::ALL:63-76; from_api_slot:78-114 (A1/B1→TwoHand:95-96); EQUIPMENT_SLOTS:146-166; get/major_for_api_slot/get_for_attr_count; stat_shape_from_attr_count:208+
- src/grouped_sheet.rs (cfg(test) mod, lib.rs:10-11)
  - prefix():34-38; ranger_grouped:218-230; all_strong_groups:260-270; display_groups:275-296; inactive-set test:403-470; JSON round-trip test:477-486
- src/tests/math_permutations.rs
  - match_pvp_amulet test:538-542; pvp base-stats check:542+
- src/referee.rs
  - test ValidatedBuild{gear_prefix Some:1257, gear_groups default:1261}
  - EHP floor comments reference gear budgets:25-56

### crates/addon
- src/ui/main_view/optimization.rs
  - synergy_result_to_suggestion reads v.gear_prefix:231-235 & v.gear_groups:237-253 → BuildSuggestion.stat_prefix:264-268, gear_summary text:256-259
  - candidate suggestions:459-466 (uniform groups from stat_prefix_name)
  - improve-parse fill raw.stat_prefix from v.gear_prefix:1131-1132
  - kitchen_brief:1167+; gemini_from_validated stat_prefix copy:1029-1032; apply_radar_prefix/prefix_named_in_text:1194-1196; current-armor[0] summary:985-988; estimate_prefix_stats call:1047-1048; chat-code builders:1344-1348
- src/ui/main_view/chat_flow.rs
  - parse_gemini_build:199; validate_gemini_build:236-250; kitchen_brief feed:92-158; imports
- src/ui/main_view/optimize_flow.rs
  - select_gear_prefix log:87; parse+validate plate:447-452; direct engine::optimize call:262
- src/ui/comparison.rs
  - BuildSuggestion.stat_prefix:20, gear_prefixes:21-23; label templating prefix:444-448; benchmark delta gear text:1282-1284
- src/ui/gear_sheet.rs
  - operates on ResolvedBuild: render_resolved_sheet:111-137; sug_prefix from suggestion.stat_prefix:130; per-piece stat_prefix reads:139,163; changed-tint logic:142-144, 300/324 cur_prefix != sug.stat_prefix; suggestion sheet other-formatting:276-295; weapon set stat_prefix:242
- src/ui/main_view/build_display.rs
  - groups Armor/Trinkets/Weapons render:185-198; simple stat_prefix render:656-659
- src/ui/main_view/resolution.rs
  - resolve_equipment_db:376-523 — per-piece stat_prefix from db.itemstats:400-404; ws1/ws2 stat_prefix fill:428-473; armor/trinket pieces construct:483-501; resolve_pvp_amulet_db:526-542
- src/ui/main_view/tabs/saveload.rs
  - list column stat_prefix:501-503; suggestion→saved mirror:680-683; saved_to_suggestion inherit_empty:750-753; fixture 976-979; tests 1212-1219
- src/ui/main_view/tabs/improve.rs — option labels prefix:85-88
- src/feedback/mod.rs
  - snapshot_from copies stat_prefix+gear_prefixes:427-431; bait-suggestion fixture 841-850; allowlist want-set incl stat_prefix/gear_prefixes:882-884; assertions 898-901, 933-936
- src/feedback/tasks.rs — small_snapshot:787-792

Findings table severity assignment:
- BLOCKER 1: double-count inactive weapon set (spec §4×§5×§10)
- BLOCKER 2: byte-equal migration false for lone one-handed main; plus unnamed candidate-stat path A/B inconsistency
- HIGH 1: PvP amulet seeding ambiguity after gear_prefix removal (plate flow only populates top-level stat_prefix→gear_prefix)
- HIGH 2: llm_advisor SWAP grammar/port omitted
- HIGH 3: BuildSuggestion + feedback wire schema (report.rs) not covered by any layer
- HIGH 4: three coexisting weapon-budget models (A/B/C); spec must name which paths migrate (calculate_candidate_stats + apply_optimized_gear_stats live in engine, gemini_tools estimate loop)
- MED 5: bare stat_prefix distribution rule ambiguous → armor-only reading breaks PvP/display parity
- MED 6: gemini_tools calculator budget loop unchanged → divergent numbers (single-prefix across ALL slots incl A1+A2) vs per-slot optimized view; spec asserts OK without reconciling
- MED 7: validation: per-slot rejection codes/warning dedupe/provisional wiring unspecified; multi-slot fuzzy matches explode warning volume
- MED 8: gear_locks ripple: describe_constraints prompt text + snapshot tests + lock_panel; LLM guidance for locked gear absent from §7
- MED 9: spec §7 misattributes name resolution to "scraper normalized matcher" (doesn't exist as such); real helpers db.itemstat_by_name/validate_gear_prefix
- MED 10: which entry points honor per-slot map vs override parsed.stat_prefix (engine.rs:1045-1049 new_build synergy path; radar-prefix appliers optimization.rs:1194-1196) unspecified — Chat flow vs Optimize/New-build/improve flows differ
- LOW 11: saves list column stat_prefix only
- LOW 12: grouped_sheet cfg(test) — fixtures reusable; hint language implies prod surface
- LOW 13: neighbor-budget arithmetic restated but eval-budget interplay unstated (OK note)
- LOW 14: benchmark/community scrape stays single-prefix by design; verify §2 non-goal covers BenchmarkDelta display (it does implicitly)
- LOW 15: match_pvp_amulet bidirectional substring heuristic — recommend exact-first when keyed by single authoritative slot

Also positive observation requirement: name what the code does well — e.g., deterministic candidate ordering discipline (sort_by_key ids search.rs:83-85; itemstat_by_name tiebreak), active_land_weapon_budget making the OLD validated path two-hand aware, atomic temp-write saves, inactive-set invariant test existing already (good anchor for new tests), and spec's rotation determinism idea echoing existing interleaving comment.

Severity table format per instructions: severity | spec section | gap/wrong claim | code evidence | spec fix.

I'll write it compactly but completely. Then yield as final structured result with type string[] sections? Instructions: findings table + inventory appendix. I'll emit an incremental section then terminal result with full content in result.data so Main gets JSON. Better: single terminal yield with result.data containing the whole report structured, PLUS the visible message containing the human-readable report. The last assistant turn becomes raw final if data omitted — safer to provide data explicitly: {findings:[...], inventory:{...}, verification:[...]}. And print readable markdown in the reply body too since caller sees only final message... "The caller sees ONLY your final message" — the yield result.data IS what orchestrator reads presumably. I'll put full report inside result.data fields, and also render it as the message text for safety.


---

### Task 1: Core slot model + serialization

**Files:**
- Modify: `crates/core/src/types.rs` (add `GearSlot`, `GearSlots`; `SavedBuild` gains `slot_prefixes`)
- Test: `crates/core/src/types.rs` tests mod

**Interfaces:**
- Produces: `GearSlot` (16-variant Copy enum, `ALL: [GearSlot; 16]`, kebab-case serde, `budget_slot_type()` mapping to `optimizer::data::SlotType` semantics by name), `GearSlots { map: [Option<PrefixRef>; 16] }` with `get/set/prefix_id/is_empty`, `PrefixRef { itemstat_id: u32, name: String }`, `GearSlots::from_legacy(stat_prefix: &str, groups: &GearPrefixGroups) -> GearSlots` (Set1-only weapon expansion per §12.1), `SavedBuild.slot_prefixes: Option<...>` with load migration.

- [ ] Write failing test: legacy save with `stat_prefix="Berserker's"` + groups expands to 13 populated slots, weapons only Set1.
- [ ] Implement enum + struct + migration until green.
- [ ] `cargo test -p gw2-core` green; commit `feat(core): GearSlot model + legacy save migration`.

### Task 2: ValidatedBuild slot vector + per-slot validation

**Files:**
- Modify: `crates/optimizer/src/validation.rs` (replace `gear_prefix`/`gear_groups`/`ValidatedGearGroups` with `gear_slots: GearSlots`; delete `effective_prefix_ids` 3-tuple in favor of per-slot accessors; `gear_identity()` → slot-map identity; per-slot validation with `RejectCode::GearSlotPrefixNotFound { slot, name }`)
- Modify: `crates/optimizer/src/validation.rs` consumers inside validate_gemini_build

**Interfaces:** `ValidatedBuild::gear_slots: GearSlots`; `gear_identity()` returns slot-map identity string; `ValidatedBuild::prefix_for(slot) -> Option<&ValidatedGearPrefix>`.

- [ ] Port `ValidatedBuild` construction sites (grep list in appendix under validation.rs/engine.rs/referee.rs tests).
- [ ] Per-slot validation: unknown itemstat id → provisional slot error; fuzzy name → warning (deduped per slot).
- [ ] `cargo test -p gw2-optimizer` green (all construction sites compile; validation tests updated).
- [ ] Commit `refactor(optimizer): ValidatedBuild carries a per-slot gear vector`.

### Task 3: Stat application — single weapon model C

**Files:**
- Modify: `crates/optimizer/src/engine.rs` (`apply_validated_gear_stats` per-slot; align `calculate_candidate_stats` and `apply_optimized_gear_stats` to model C; `match_pvp_amulet` keyed by Amulet slot prefix)

**Interfaces:** `apply_gear_slot_stats(stats, db, slot, prefix, ctx)` — one slot's budget × one prefix; shared by all three paths.

- [ ] Write failing test: mixed build (Berserker's helm + Cavalier's coat) totals = hand-computed from slot_budgets fixtures.
- [ ] Implement per-slot application; align A/B paths; PvP parity test green.
- [ ] `cargo test -p gw2-optimizer` green; commit `feat(optimizer): per-slot stat application with unified weapon model`.

### Task 4: Search operators + locks

**Files:**
- Modify: `crates/optimizer/src/search_v2.rs` (`swap_gear_prefix`/`swap_gear_groups` → `swap_uniform`, `swap_group`, `swap_slot` per spec §12.2 — NO rotation; gear identity via slot map; lock respect)
- Modify: `crates/core/src/types.rs` (`BuildLocks.gear_locks: HashMap<GearSlot, u32>` serde default; `describe_constraints` extension)
- Modify: `crates/optimizer/src/search.rs` (legacy `search_gear_prefixes`/`build_mixed_candidate` → slot vector)
- Modify: `crates/optimizer/src/engine.rs` (`llm_advisor` SWAP grammar → slot-level)

**Interfaces:** `swap_slot(candidate, slot, prefix)`; operators skip `gear_locks`-locked slots; identity string = slot map.

- [ ] Write failing determinism test: same weights → same final slot vector across two runs.
- [ ] Write failing lock test: locked slot unchanged across all beam generations.
- [ ] Implement operators + locks; legacy path migration.
- [ ] `cargo test -p gw2-optimizer` green; commit `feat(optimizer): per-slot search operators with gear locks`.

### Task 5: LLM plate per-slot map

**Files:**
- Modify: `crates/optimizer/src/prompts.rs` (`GeminiBuildResponse.gear_slots: Option<HashMap<String, String>>` kebab slot → prefix name; `parse_gemini_build` accepts with/without)
- Modify: `crates/optimizer/src/validation.rs` (`validate_gemini_build` resolves names via `db.itemstat_by_name` exact-else-shortest-fuzzy, per-slot fallback to profile prefix + warning)
- Modify: `crates/optimizer/src/engine.rs` (`parse_and_override_gear_prefix` policy per spec §12.3: plate map applied, optimizer refines unlocked slots)

- [ ] Failing tests: full map, partial map, unknown names → fallback warnings.
- [ ] Implement; `cargo test -p gw2-optimizer` green; commit `feat(llm): per-slot gear maps in Choya plates`.

### Task 6: Addon surfaces

**Files:**
- Modify: `crates/addon/src/ui/main_view/chat_flow.rs`, `optimize_flow.rs` (plate apply/lock wiring)
- Modify: `crates/addon/src/ui/gear_sheet.rs` (per-piece prefix from slot map), `crates/addon/src/ui/main_view/lock_panel.rs` (gear lock section), `crates/addon/src/ui/comparison.rs`, `crates/addon/src/ui/main_view/tabs/saveload.rs`, `crates/addon/src/feedback/mod.rs`+`report.rs` (wire schema slot map)
- Modify: `crates/addon/src/ui/main_view/optimization.rs` (`synergy_result_to_suggestion` per-slot mapping)

- [ ] Gear sheet: each piece row renders its own prefix (failing UI-data test first where feasible).
- [ ] Locks panel gear section (set + lock per slot).
- [ ] Feedback wire: slot map replaces group snapshot; allowlist updated.
- [ ] `cargo test --workspace` green; commit `feat(addon): per-slot gear surfaces`.

### Task 7: Cleanup

- [ ] Delete `GearPrefixGroups`/`ValidatedGearGroups` construction paths (legacy deserialization only), unused legacy fields stop being written.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --all` + full workspace tests.
- [ ] Commit `chore: remove legacy group-prefix construction paths`.
