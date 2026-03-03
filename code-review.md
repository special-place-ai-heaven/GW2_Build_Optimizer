# GW2 Build Optimizer — Full Code Review

Generated: 2026-02-21

## Critical Issues (Must Fix)

### C1. Major traits never included in spec search — optimizer is broken
**File:** `crates/optimizer/src/engine.rs:105-109, 193-204`
**Confidence: 88**

The spec combination loop only collects `spec.minor_traits` — never `spec.major_traits`. Major traits are where ~90% of build power comes from (damage modifiers, stat conversions, condition duration). Without them, `extract_damage_modifiers()` finds nothing, `apply_trait_conversions()` finds nothing, and all spec combos score nearly identically. **The spec search is effectively random.**

Fix: Include major traits (either best-per-column or enumerate combos).

---

### C2. Per-condition duration bonuses silently discarded
**File:** `crates/optimizer/src/combat.rs`
**Confidence: 100**

`total_condi_duration_for(condition)` method exists on `DamageModifiers` but is never called in `calculate_condition_ticks()` or anywhere in the pipeline. Per-condition duration bonuses (e.g. "+20% Burning Duration" from traits/runes) are extracted and stored but never applied.

Fix: Apply per-condition duration multiplier in `calculate_condition_ticks()`.

---

### C3. Effective health formula inverts damage reduction — produces infinity at high toughness
**File:** `crates/optimizer/src/combat.rs`
**Confidence: 95**

The effective health formula divides by `(1.0 - damage_reduction)`. If DR approaches 1.0 (high toughness + protection buff), effective health goes to infinity. Protection alone gives 33% DR, and toughness-based DR pushes the total higher. With full squad buffs (Protection), this can produce unreasonably large or infinite effective health values.

Fix: Use `health * (1.0 / (1.0 - DR))` capped, or use the standard GW2 formula: `Health * Armor / reference_power`.

---

### C4. Current build combat metrics not passed to render — shows 0 for all DPS/healing
**File:** `crates/addon/src/ui/comparison.rs:206-213`
**Confidence: 90**

`render_combat_performance()` hardcodes `0` for all current-build DPS/healing rows. The `ComparisonState` has `current_combat_solo/party/squad` fields populated by `candidate_to_suggestion()`, but the render function only accepts `current_stats: Option<&StatBlock>` — it never receives the combat metrics. All diffs show full green "+N" which is misleading.

Fix: Pass `&ComparisonState` to `render_combat_performance()` and read `current_combat_*` fields.

---

### C5. `StatBlock::compute_derived` hardcodes wrong health base for 6/9 professions
**File:** `crates/core/src/types.rs:152`
**Confidence: 92**

Hardcodes `1645` (Guardian/Thief/Ele base). Warriors/Necromancers show health ~7,500 HP too low. Medium-armor classes show ~4,300 too low.

Fix: Accept profession parameter or remove this method entirely (the authoritative version is in `optimizer::stats`).

---

### C6. Gemini quota counter incremented before body parse
**File:** `crates/optimizer/src/gemini.rs`
**Confidence: 100**

`requests_today` is incremented immediately after `reqwest::blocking::Client::post()` succeeds, before the response body is parsed. If the body is malformed/empty, the request counts against the daily quota but produces no useful output. With a 250/day limit, each wasted count matters.

Fix: Only increment after successful parse.

---

### C7. UTF-8 panic on non-ASCII Gemini responses
**File:** `crates/addon/src/ui/chat_bar.rs:76-78`
**Confidence: 90**

`&text[..200]` slices by byte offset. If byte 200 lands mid-character (em-dash, accented chars common in LLM output), this panics at runtime.

Fix: Use `text.chars().take(200).collect::<String>()` or `char_indices`.

---

## Important Issues (Should Fix)

### I1. Suggestion tab clicks are non-functional
**File:** `crates/addon/src/ui/comparison.rs:73-92`
**Confidence: 95**

`Selectable::new(...).build(ui)` return value is discarded. Comment says "Selection handled by caller" but `render_comparison` takes `&ComparisonState` (immutable) and returns `()`. Clicking tabs does nothing.

Fix: Return `Option<usize>` or take `&mut ComparisonState`.

---

### I2. Unbounded concurrent optimization threads
**File:** `crates/addon/src/ui/main_view.rs:1113-1134`
**Confidence: 90**

No guard prevents clicking another archetype while optimizing. Multiple threads race to write `comparison.suggestions`.

Fix: Disable archetype buttons while `state.main.optimizing == true`.

---

### I3. Silent build name collision — saves overwritten
**File:** `crates/core/src/storage.rs:23-34`
**Confidence: 95**

`save()` overwrites existing file if names sanitize to the same string. No warning.

Fix: Check existence before writing; return error if duplicate.

---

### I4. Cache writes not atomic — corruption on crash
**File:** `crates/gw2api/src/cache.rs:43-44`
**Confidence: 97**

`std::fs::write` is not atomic. Crash mid-write leaves truncated file. Next load gets `CacheError::Json`.

Fix: Write to `.tmp` then `fs::rename`.

---

### I5. Duplicate error display in left menu
**File:** `crates/addon/src/ui/main_view.rs:228-232`
**Confidence: 88**

Error shown both at top of main window (with dismiss) AND in the 180px left menu (without dismiss). Double render, left one overflows.

Fix: Remove the left-menu duplicate.

---

### I6. `format_timestamp` edge case at year boundaries
**File:** `crates/addon/src/ui/main_view.rs:1717-1745`
**Confidence: 95**

Hand-rolled date formatter: if month loop never breaks, `m = 0` and `remaining_days` is wrong. Produces garbage date.

Fix: Use `chrono` (already a dep) or initialize `m = 11` as fallback.

---

### I7. Items download progress shows 100% throughout
**File:** `crates/gw2api/src/download.rs:126-134`
**Confidence: 92**

Hardcoded `current_step: 8` with `total_steps: 8` in batch loop. Progress bar shows 8/8 = 100% for all 500+ batches.

Fix: Use `step + 1` for `current_step` or keep at `step` (=7) during batch loop.

---

### I8. `send_chat_message` missing concurrent-call guard
**File:** `crates/addon/src/ui/main_view.rs:1471-1537`
**Confidence: 88**

Soft race: `chat.waiting` can be reset between `render_chat_bar` and the check, allowing double-submit.

Fix: Check `chat.waiting` at start of `send_chat_message`.

---

### I9. Unsanitized `current_build_summary` in Gemini prompts
**File:** `crates/optimizer/src/prompts.rs:64-112, 116-162`
**Confidence: 85**

Character name (player-controlled via GW2 API) flows unsanitized into system prompt. `user_request` is properly sanitized, but `current_build_summary` is not.

Fix: Apply same sanitization (strip backticks, length cap) to build summaries.

---

### I10. Background threads survive addon unload with large allocations
**File:** `crates/addon/src/state.rs:176-178`
**Confidence: 85**

`clear()` sets state to `None` but background threads holding cloned `GameDb` (~100MB) keep running. DLL unload before thread completion can cause issues.

Fix: Use `Arc<AtomicBool>` cancellation token.

---

### I11. `delete()` silently succeeds on non-existent files
**File:** `crates/core/src/storage.rs:61-68`
**Confidence: 85**

If `SavedBuild.name` sanitizes differently than the filename on disk, delete succeeds but file remains. Reappears on next load.

Fix: Store canonical filename in `SavedBuild`.

---

### I12. OOM risk — full items JSON serialized to single String
**File:** `crates/gw2api/src/cache.rs:43-44`
**Confidence: 80**

50k+ items serialized to one heap String before writing. Use `serde_json::to_writer(BufWriter::new(file))` instead.

---

### I13. `render_save_build_ui` has no in-function empty-vec guard
**File:** `crates/addon/src/ui/main_view.rs:1553-1555`
**Confidence: 85**

Protected by outer `if !suggestions.is_empty()` but no guard inside the function itself. Index-out-of-bounds panic if ever called from a new location.

Fix: Early return if empty.

---

### I14. `generate_build_chat_code` silently truncates u32→u8
**File:** `crates/addon/src/ui/main_view.rs:959,969`
**Confidence: 88**

`profession.code as u8` and `spec_id as u8` silently truncate. Currently safe (all values < 256) but fragile.

Fix: Add bounds check.

---

### I15. RPM rate limit TOCTOU in Gemini client
**File:** `crates/optimizer/src/gemini.rs`
**Confidence: 90**

Two concurrent threads can both check `count < MAX_RPM`, pass, and submit simultaneously. The mutex is released between check and request.

Fix: Hold mutex through request or use atomic increment-and-check.

---

### I16. `search_weapon_combos` is dead code
**File:** `crates/optimizer/src/search.rs`
**Confidence: 95**

Exported but never called from any production code. Only used in its own test module.

Fix: Remove or integrate into engine.

---

## Low Priority (Nice to Have)

- Config save errors silently discarded in setup wizard (`let _ = config.save(...)`)
- Corrupt save files silently skipped in `list()` with no user feedback
- `score_stats` in scoring.rs only used in tests — dead exported API
- `Fact::Unknown` discards all fields silently (documented tradeoff)
- API error body logged verbatim (no key echoed currently, but fragile)

---

## Priority Ranking for Fixes

**Round 1 — Algorithmic correctness (the optimizer actually works):**
1. C1 — Include major traits in search
2. C2 — Apply per-condition duration bonuses
3. C3 — Fix effective health formula

**Round 2 — User-visible bugs:**
4. C4 — Pass combat metrics for current build display
5. C5 — Fix health base per profession
6. C7 — Fix UTF-8 panic
7. I1 — Make suggestion tabs clickable
8. I2 — Guard concurrent optimization

**Round 3 — Data integrity:**
9. I3 — Save name collision check
10. I4 — Atomic cache writes
11. C6 — Fix Gemini quota counting

**Round 4 — Polish:**
12-16. Remaining important issues
