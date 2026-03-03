# GW2 Build Optimizer — Finalization Plan

> Generated from code review + live codebase exploration (2026-02-21).
> Format: S##-T## (sprint-task). Sprints S01–S09 are DONE; S10 partially done.

---

## Actual State vs Review Agent Claims

The review agent's findings were partially stale. Before planning, here's the corrected picture:

| Area | Review Agent Said | Reality |
|------|------------------|---------|
| Optimizer → UI wiring | Not wired | **DONE** — `start_optimization()` calls `engine::optimize()`, converts to `BuildSuggestion` |
| GameDb loading | Needs implementation | **DONE** — `load_game_db()` bg thread, stored in `MainState.game_db` |
| `ComparisonState` | Not populated | **DONE** — `render_comparison()` full 2-col layout, stat diff, suggestion tabs |
| `chat_bar.rs` | Not implemented | **DONE** — `ChatBarState`, render, history, submit; but Gemini call is TODO |
| `build_display.rs` | Not implemented | **DONE** — Full specs/skills/weapons/armor/trinkets/stats display |
| `engine.rs` | Partially wired | **DONE** — Full pipeline: gear search → spec combos → trait stats → scoring |
| `gemini.rs` | Needs LLM wiring | **DONE** — `GeminiClient`, caching, rate limiting; but not called from UI yet |
| `prompts.rs` | Needs implementation | **DONE** — All 3 prompt templates exist |
| Storage | Needs implementation | Stub, but **not critical** — config handled by `AppConfig` directly |

### What's Actually Missing

1. **Chat bar → Gemini**: `main_view.rs:199` has TODO — input collected, Gemini not called
2. **LLM explanation in suggestions**: `BuildSuggestion.explanation` always empty string — Gemini prompt not sent after optimizer
3. **Gemini rate limits**: in-memory only, resets on addon reload (could hit daily quota)
4. **Thread tracking in setup**: rapid "Validate" clicks spawn multiple threads
5. **Traited facts**: conditional stat bonuses (`BuffConversion` on traits) partially deferred in `stats.rs`
6. **Build persistence**: no save/load, `storage.rs` is a 2-line stub
7. **PvP stat path**: amulets loaded in GameDb but `resolve_build` doesn't use them, stats.rs has no PvP branch
8. **WvW**: no WvW-specific modifier path
9. **Polish**: no error toasts, no cache size display, mutex poisoning silent

---

## Sprint Plan (S11–S15)

### S11 — Gemini → UI Integration

**Goal**: Wire the Gemini client to the two remaining call sites: post-optimization explanation and chat bar.

| Task | Description | File(s) | Notes |
|------|-------------|---------|-------|
| S11-T01 | After `start_optimization()` gets top candidates, send `improve_build_prompt()` / `new_build_prompt()` to Gemini and parse explanation into `BuildSuggestion.explanation` | `addon/src/ui/main_view.rs`, `optimizer/src/prompts.rs` | Gemini client + prompt templates already exist; just wire the call in the bg thread, after `candidate_to_suggestion()` |
| S11-T02 | In chat bar submit handler (main_view.rs:199 TODO), call `chat_refinement_prompt()` with current context, send to Gemini, push result via `add_ai_response()` | `addon/src/ui/main_view.rs` | `chat_bar.rs` already collects input and returns it; just add the Gemini call in the TODO block |
| S11-T03 | Handle Gemini errors in both call sites: show error in `ComparisonState.error` / chat history rather than panicking | `addon/src/ui/main_view.rs`, `addon/src/ui/comparison.rs` | `GeminiError` variants already defined; map them to user-facing strings |
| S11-T04 | Display `BuildSuggestion.explanation` in comparison view below the stat diff table | `addon/src/ui/comparison.rs` | The field exists; render it in `render_comparison()` using `ui.text_wrapped()` |

---

### S12 — Robustness Fixes

**Goal**: Address thread safety and rate-limit correctness before polish.

| Task | Description | File(s) | Notes |
|------|-------------|---------|-------|
| S12-T01 | Add `validating_gw2: bool` / `validating_gemini: bool` fields to `SetupState`; disable "Validate" button while true to prevent duplicate thread spawns | `addon/src/state.rs`, `addon/src/ui/setup.rs` | Simple bool guard; set to true before spawn, false in callback |
| S12-T02 | Persist Gemini rate tracker to `{addon_dir}/gemini_usage.json` on every `generate()` call and reload on `GeminiClient::new()` | `optimizer/src/gemini.rs` | `RateTracker` already has per-minute + per-day fields; serialize with serde_json to disk |
| S12-T03 | Log a warning when mutex is recovered from poisoned state in `lock_state()` | `addon/src/state.rs` | One-liner: `nexus::log::warn!("State mutex was poisoned, recovering")` before `into_inner()` |
| S12-T04 | Add `traited_facts` handling in `calculate_trait_stats()`: process `BuffConversion` facts that are currently skipped | `optimizer/src/stats.rs` | Deferred TODO at top of stats.rs; `TraitedFact` already parsed by serde |

---

### S13 — Build Persistence

**Goal**: Let users save optimizer results and load them later.

| Task | Description | File(s) | Notes |
|------|-------------|---------|-------|
| S13-T01 | Define `SavedBuild` struct (name, timestamp, character, game_mode, suggestion: `BuildSuggestion`) | `crates/core/src/types.rs` | Keep it simple; `BuildSuggestion` already has all display data |
| S13-T02 | Implement `BuildStorage` in `storage.rs`: `save(build: &SavedBuild)`, `list() -> Vec<SavedBuild>`, `delete(name: &str)` backed by one JSON file per save (`{addon_dir}/saves/{name}.json`) | `crates/core/src/storage.rs` | Replace the 2-line stub; use `serde_json` already in workspace deps |
| S13-T03 | Wire "Save Build" button to `BuildStorage::save()` in the comparison view after optimization completes | `addon/src/ui/comparison.rs`, `addon/src/ui/main_view.rs` | Small button below suggestion; prompt for name via ImGui `InputText` |
| S13-T04 | Populate the Save/Load tab: list saved builds, "Load" button replaces comparison state, "Delete" button removes the file | `addon/src/ui/main_view.rs` | `render_saveload_tab()` is currently a placeholder |
| S13-T05 | Add `MainState.saved_builds: Vec<SavedBuild>` and refresh on tab switch | `addon/src/state.rs`, `addon/src/ui/main_view.rs` | Load list lazily when Save/Load tab is selected |

---

### S14 — Game Mode Support

**Goal**: Make PvP and WvW game modes functional beyond just the selector radio button.

| Task | Description | File(s) | Notes |
|------|-------------|---------|-------|
| S14-T01 | Add `GameMode` parameter to `resolve_build()`; when PvP, skip gear resolution and instead set `ResolvedBuild.pvp_amulet` from `EquipmentPvp` on the character | `addon/src/ui/main_view.rs` | `EquipmentPvp` is already parsed in `characters.rs`; `ResolvedBuild.pvp_amulet` field exists |
| S14-T02 | Add PvP stat calculation branch in `stats.rs`: use `PvpAmulet.attributes` directly instead of gear-based formula | `optimizer/src/stats.rs` | `PvpAmulet` model already has attributes; formula is just the amulet values as base |
| S14-T03 | Filter optimizer gear search to exclude PvE gear when PvP mode active; for PvP only search spec combos + traits | `optimizer/src/search.rs`, `optimizer/src/engine.rs` | Add `game_mode: GameMode` param to `optimize()`; early return gear phase if PvP |
| S14-T04 | Add WvW stat modifier: WvW is PvE gear + optional WvW sigils/runes; treat as PvE with a note in the explanation | `optimizer/src/engine.rs`, `optimizer/src/prompts.rs` | Minimal change: pass `game_mode` to prompt so Gemini can note WvW context; no separate stat math needed |
| S14-T05 | Show current amulet in `render_build()` when game mode is PvP | `addon/src/ui/build_display.rs` | Field already in `ResolvedBuild.pvp_amulet`; just add a render branch |

---

### S15 — Polish & Release

**Goal**: UX improvements, error surfaces, cache management, and release packaging.

| Task | Description | File(s) | Notes |
|------|-------------|---------|-------|
| S15-T01 | Add error toast / status bar at bottom of main window for transient errors (API failures, Gemini errors) | `addon/src/ui/mod.rs`, `addon/src/state.rs` | Add `last_error: Option<(String, Instant)>` to `MainState`; render if within 5s of Instant |
| S15-T02 | Add cache size display and "Clear Cache" button to Settings tab | `addon/src/ui/main_view.rs` | Walk `{addon_dir}/cache/` with `std::fs::read_dir`, sum file sizes; existing "Clear Cache" button already present but likely no size display |
| S15-T03 | Add Gemini quota display to Settings tab: show requests today / daily limit remaining | `addon/src/ui/main_view.rs`, `optimizer/src/gemini.rs` | Expose `RateTracker` read accessor on `GeminiClient` |
| S15-T04 | Add "Reset Setup" confirmation: `ChildWindow` overlay asking "Are you sure?" before wiping config | `addon/src/ui/main_view.rs` | Add `confirm_reset: bool` to `MainState`; render overlay when true |
| S15-T05 | Refactor `resolve_build()` (currently ~140 lines) into smaller helpers: `resolve_specs()`, `resolve_skills()`, `resolve_weapons()`, `resolve_armor()`, `resolve_trinkets()` | `addon/src/ui/main_view.rs` | Pure refactor, no behavior change |
| S15-T06 | Write `README.md`: installation (copy DLL to addons/), first-run (API key setup), usage, known limitations | `README.md` | Plain text, ~1 page |
| S15-T07 | Version bump to 1.0.0, set `NexusAddonVersion` fields in `lib.rs`, verify DLL loads in Nexus | `Cargo.toml`, `crates/addon/src/lib.rs` | Final release prep |

---

## Dependency Graph

```
S11 (Gemini → UI)
 └─ S12-T02 (rate persist)   [can overlap S11 — different file]
 └─ S12-T01 (thread guard)   [can overlap S11 — different file]

S11 complete
 └─ S12 complete
     └─ S13 (persistence)
     └─ S14 (game modes)
         └─ S15 (polish + release)
```

S12-T01 and S12-T02 can be done in parallel with S11 since they touch different files. Otherwise sprints are sequential due to shared `main_view.rs` and `state.rs`.

---

## Prioritized Critical Path

To get to a **functional demo** fastest: **S11 → S12-T01, T02 → S15-T06, T07**

That gives: LLM-explained optimizer results + chat refinement + robust rate limits + release DLL.

S13 (persistence) and S14 (PvP/WvW) are quality-of-life, not blocking for a v1.0 demo.

---

## Task Count

| Sprint | Tasks | Priority |
|--------|-------|----------|
| S11 | 4 | **Critical** — core feature gap |
| S12 | 4 | High — correctness |
| S13 | 5 | Medium — persistence |
| S14 | 5 | Medium — game mode coverage |
| S15 | 7 | Low/Release — polish |
| **Total** | **25** | |
