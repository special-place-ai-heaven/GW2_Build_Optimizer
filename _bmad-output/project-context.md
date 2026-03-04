---
project_name: 'GW2_Build_Optimizer'
user_name: 'Rob'
date: '2026-03-03'
sections_completed: ['technology_stack', 'language_rules', 'framework_rules', 'testing_rules', 'quality_rules', 'workflow_rules', 'critical_rules']
status: 'complete'
rule_count: 71
optimized_for_llm: true
---

# Project Context for AI Agents

_This file contains critical rules and patterns that AI agents must follow when implementing code in this project. Focus on unobvious details that agents might otherwise miss._

---

## Technology Stack & Versions

- **Language**: Rust 2021 edition
- **Crate type**: `cdylib` (single DLL — `gw2_build_optimizer.dll`)
- **Workspace**: 4 crates (`addon`, `core`, `gw2api`, `optimizer`)
- **nexus** (git: `https://github.com/zerthox/nexus-rs`, features: `["serde"]`) — Nexus addon API + ImGui
- **serde** 1.x + **serde_json** 1.x — serialization
- **reqwest** 0.12 (features: `["blocking", "json"]`) — HTTP client (synchronous, **not** async)
- **thiserror** 2.x — error type derivation
- **chrono** 0.4 (feature: `["serde"]`) — timestamps
- **urlencoding** 2.x — manual query string building
- **base64** 0.22 — encoding
- **Build**: `cargo build --release` → `target/release/gw2_build_optimizer.dll`
- **Deploy**: copy DLL to `C:\GAMES\Guild Wars 2\addons\`
- **Tests**: native `#[test]` only — no external test framework

## Critical Implementation Rules

### Language-Specific Rules (Rust)

- **No async/await anywhere** — `reqwest` is used in blocking mode. There is no async runtime (Tokio etc.) in the DLL. Never introduce async.
- **No `unwrap()` on external data** — API responses, config loads, and cache reads all use `Result`/`Option` with explicit handling. `unwrap()` is only acceptable in tests or on values provably non-None (e.g., hardcoded literals).
- **UTF-8 safe string slicing** — never use `&text[..n]` (panics on multibyte chars). Use `text.chars().take(n).collect::<String>()` instead.
- **Clone before move into closures** — always extract/clone needed values from state before spawning threads; never hold the state lock across a thread spawn:
  ```rust
  let token = state.cancel_token.clone();
  let config = state.config.clone();
  // lock released here before spawn
  std::thread::spawn(move || { /* use token, config */ });
  ```
- **Workspace deps are hoisted** — all shared dependency versions live in root `Cargo.toml` under `[workspace.dependencies]`. Crate-level files use `.workspace = true`. Never pin a version in a crate `Cargo.toml` if it's already in the workspace.
- **`#[serde(default)]` on all new optional fields** — ensures backward compatibility when loading old saved configs/cache files that predate the field.
- **`thiserror` for all error types** — use `#[derive(thiserror::Error)]` + `#[error("...")]`. Don't implement `Display` or `Error` manually.
- **Poison recovery on Mutex** — the global state mutex uses `.unwrap_or_else(|e| e.into_inner())`. Never call `.unwrap()` directly on the state lock — a panicking background thread would permanently deadlock the addon.

### Framework-Specific Rules (Nexus / ImGui)

- **All UI code must run inside `Window::new().build(ui, || { })`** — ImGui context is only valid inside the build closure. Calling ImGui functions outside it causes a null context crash.
- **Screen routing via `s.screen` enum only** — never add `bool` flags for view switching; add a `Screen` variant instead. Two sources of truth always diverge.
- **Use `ChildWindow` for panels, not `Window`** — nested `Window` calls are draggable and closeable by the player. Use `ChildWindow::new()` for all embedded panels.
- **ImGui style tokens must be stored in named bindings, never `let _ =`**:
  ```rust
  let _color = ui.push_style_color(...);  // ✅ dropped at end of scope
  let _ = ui.push_style_color(...);       // ❌ drops IMMEDIATELY — style leaks every frame
  ```
- **`with_state()` is non-reentrant** — never call it inside a closure already inside `with_state()`. Deadlocks the render thread permanently.
- **No blocking I/O in render callbacks** — the render callback runs on GW2's render thread. Any blocking call freezes the game. Always spawn `std::thread`.
- **Progress flags prevent duplicate spawns** — set `loading = true` before spawning, clear in the thread's `with_state` callback. Missing this spawns N duplicate threads per frame.
- **Cancellation check: start AND after each blocking op** — check `token.is_cancelled()` at thread start and after every API call / file read. A single check at the top is insufficient.
- **All Nexus registrations (keybinds, events) must happen in `addon_load`** — late registration is silently ignored.
- **Use `Condition::FirstUseEver` for window size** — `Condition::Always` resets the user's custom size every frame, preventing resize.

### Testing Rules

- **Native `#[test]` only** — no external test frameworks (no `rstest`, `mockall`, etc.). Keep tests in the same file as the code under test or in `tests/` for integration tests.
- **Test naming convention**: `test_verb_condition` — e.g., `test_save_and_load`, `test_backward_compat_old_config`, `test_provider_mismatch_not_complete`.
- **Config tests are mandatory for every new field** — when adding a field to `AppConfig`:
  1. Test backward compat (old JSON without the field deserializes to correct default)
  2. Test save/load roundtrip (serialization is lossless)
  3. Test `is_setup_complete()` still behaves correctly with the new field
- **`is_setup_complete()` must be tested for provider mismatch** — having a Gemini key while `active_provider = OpenAI` must return `false`. Always test the cross-provider case.
- **Live API tests are gated with `#[ignore]`** — `crates/gw2api/tests/live_download.rs` and `crates/optimizer/tests/live_llm.rs` hit real APIs. Never remove `#[ignore]`; these are run manually only.
- **No mocking framework** — test real behavior with real structs. For HTTP-dependent code, use integration tests with real API keys (run manually) rather than mocked responses.
- **Temp paths in I/O tests** — use `std::env::temp_dir()` with a unique filename to avoid test collisions. Clean up after the test.
- **`catch_unwind` behavior** — the optimizer bg thread is wrapped in `catch_unwind` to prevent mutex poisoning. Tests exercising panic recovery should verify the mutex is still accessible after the panic.

### Code Quality & Style Rules

- **Line length**: 100 characters.
- **snake_case everywhere** — functions, variables, modules, and file names use `snake_case`. Types and traits use `PascalCase`. Constants use `SCREAMING_SNAKE_CASE`.
- **Module layout mirrors crate structure** — UI sub-modules live under `src/ui/`. Sub-panels of a view live in `src/ui/<view_name>/` with a `<view_name>.rs` parent module. Don't create deep nesting without clear justification.
- **File names match module names** — `main_view.rs` exports `pub fn render_main(...)`. The file name is the public API surface hint.
- **No doc comments on unchanged code** — don't add `///` or `//!` to code you didn't write or modify. Comments are added only where logic is non-obvious.
- **Inline comments for GW2 domain constants** — when a magic number comes from the GW2 game engine (e.g., `895` for base precision, `21.0` for crit scaling, `9212` for Guardian base HP), annotate with a comment explaining the source.
- **`#[allow(...)]` is explicit, never broad** — never `#![allow(warnings)]` at crate level. Suppress specific lints only at the specific item that needs it.
- **Error propagation with `?`** — use `?` for fallible operations in functions returning `Result`. Use `.unwrap()` / `.expect()` only in tests or on values guaranteed by construction.
- **`Default::default()` for zero-init structs** — structs with many fields use `#[derive(Default)]` and are initialized with `StructName { field: val, ..Default::default() }`.

### Development Workflow Rules

- **Build command**: `cargo build --release` — always use `--release` for the deploy artifact. `cargo check` for fast iteration (type-check only, no codegen).
- **Deploy by copy**: copy `target/release/gw2_build_optimizer.dll` to `C:\GAMES\Guild Wars 2\addons\`. GW2 must be restarted or Nexus hot-reload triggered to pick up the new DLL.
- **DLL name is fixed**: `gw2_build_optimizer.dll` is the Nexus addon identifier. Never rename it or change `[lib] name` in `Cargo.toml`.
- **Config atomic save pattern** — always write to `.tmp` then `std::fs::rename` to the final path. Direct writes corrupt saved configs on mid-write crash.
- **Cache lives in `{addon_dir}/cache/`** — all cached API data is stored relative to the runtime addon directory. Never hardcode `C:\GAMES\...` in source code.
- **CI pipeline**: `.github/workflows/ci.yml` runs on every push/PR to `main`. Two steps: `cargo test --package gw2-build-optimizer -- --test-threads=1` (addon, single-threaded — global static STATE mutex deadlock risk), then `cargo test --workspace --exclude gw2-build-optimizer` (all other crates). Live API tests (`#[ignore]`) are excluded from CI; run manually with `-- --include-ignored`.
- **Commit message style**: conventional commit prefixes (`fix:`, `feat:`, `refactor:`). Match the existing history style.

### Critical Don't-Miss Rules

#### GW2 Domain Correctness

- **HP class ≠ armor class — the #1 gotcha** — two completely separate lookup tables required. Never infer HP class from armor class. Necromancer = LIGHT armor but MEDIUM HP. Revenant = HEAVY armor but MEDIUM HP.
  - HP: HIGH = {Warrior, Guardian} | MEDIUM = {Revenant, Engineer, Ranger, Mesmer, Necromancer} | LOW = {Thief, Elementalist}
  - Armor: HEAVY = {Warrior, Guardian, Revenant} | MEDIUM = {Ranger, Engineer, Thief} | LIGHT = {Elementalist, Mesmer, Necromancer}
  - Formulas: `health = vitality * 10 + base_hp`, `armor = toughness + base_defense`
  - Base values: HP = 9212 / 5922 / 1645 · Defense = 1271 / 1118 / 967
- **Stat attribute aliases** — GW2 API uses both old and new names interchangeably. Both must map to the same `StatBlock` field:
  - `"ConditionDuration"` = `"Expertise"` · `"BoonDuration"` = `"Concentration"`
  - `"CritDamage"` = `"Ferocity"` · `"Healing"` = `"HealingPower"`
- **Traited facts override pattern** — 3-step process, order is mandatory:
  1. Collect indices of base facts overridden by active `traited_facts`
  2. Apply base facts, skipping overridden indices
  3. Apply active `traited_facts` (only where `requires_trait` is in equipped traits)
  Skipping step 1 or wrong ordering causes double-counted stat bonuses.
- **Condition tick rounding is `round_half_up`**, not `floor` — applies to tick accumulation and Quickness-modified cast times. Exact level-80 formulas live in **both** `crates/optimizer/src/combat.rs` AND `crates/optimizer/src/rotation/simulator.rs`. These files are **intentionally separate** (StatBlock scoring vs. real-time tick simulation) — do NOT merge them; always update both together:
  | Condition | Formula | Common Wrong Value |
  |-----------|---------|-------------------|
  | Bleeding | `0.06 * CD + 22.0` | — |
  | Burning | `0.155 * CD + 131.75` | `+ 131.0` (off by 0.75/tick) |
  | Poison | `0.06 * CD + 33.5` | — |
  | Torment (stationary) | `0.0375 * CD + 31.875` | `0.06 * CD + 22.0` (bleeding copy-paste error) |
  | Confusion (on-activation) | `0.0175 * CD + 11.0` | `0.195 * CD + 95.5` (pre-2016, ~10× overestimate) |
- **Confusion ticks on target skill use, not a timer** — `condition_importance` weight must reflect encounter skill-use frequency. Near-zero in PvE auto-attack scenarios. Do not model identically to Bleeding/Burning/Poison.
- **GW2 conditions cap at 25 stacks per condition type** — the cap is independent per condition: 25 Bleeding stacks AND 25 Burning stacks are separate limits, not a global total. `rotation/simulator.rs` enforces `CONDITION_STACK_CAP = 25` via `can_apply = stacks.min(25 - current_count_for_that_condition)`. Missing this cap causes high-application builds to overestimate condition DPS without bound.
- **Sigil deduplication is per-weapon-set, not global** — GW2 forbids duplicate sigils WITHIN one weapon set. The same sigil IS valid in both Set 1 AND Set 2 independently (e.g. Sigil of Force in Set 1 + Sigil of Force in Set 2 = cumulative effect). `synergy_pipeline.rs` must use per-set dedup tracking. A global `HashSet` across both sets incorrectly blocks this valid configuration.
- **Elite spec skill gating** — filter skills by `Skill::specialization`: only core skills (`None`) or the equipped elite spec's skills are valid. Never include off-spec elite skills.
- **GW2 API query strings** — `reqwest`'s `.query()` URL-encodes commas as `%2C`, breaking bulk requests. Build query strings manually: `format!("ids={}", ids.join(","))`.
- **PvP uses amulet system** — PvP builds have no gear. They use an amulet that sets all stats directly. All gear-based optimizer paths must be bypassed for PvP mode.
- **Rune/sigil bonuses are unstructured text** — bonus strings like `"+7% Bleeding Duration"` are parsed via `parse_rune_modifier()`, not deserialized from structured facts. In `parse_rune_bonus_to_effects()`, the `+X% Condition Damage` branch (`rest.contains("condition") && rest.contains("damage")`) must appear BEFORE the general `+X% damage` branch — the string matches both, so a catch-all strike damage check placed first silently drops all condition damage rune bonuses.

#### LLM Provider Rules

- **Gemini prefix is always overwritten** — Gemini's gear choice is ignored. `select_gear_prefix()` (cosine similarity) is authoritative. Never trust or forward Gemini's gear selection.
- **`validate_gemini_build()` is not optional** — it checks that spec names exist in `GameDb`, weapons are valid for the profession, and trait IDs are real. Skipping it causes panics on hallucinated names. Always call it before `apply_gemini_response()`.
- **Validation failure triggers tier 3 fallback** — when `validate_gemini_build()` rejects the LLM response, the engine falls back to legacy `optimize()` with a Warning log. It does not surface an error to the user.
- **Billing-tolerant key validation**:
  - HTTP 401 → `InvalidKey` (reject key)
  - HTTP 429 → `RateLimited` (valid key, quota exhausted)
  - HTTP 400/403 + body contains `"quota"`, `"billing"`, `"payment"`, or `"limit"` → `RateLimited`
  - All other 4xx/5xx → `Http` error (surface message, don't reject key)
- **LLM trait methods are `&self`, not `&mut self`** — internal mutable state uses `Mutex<...>`. `&mut self` breaks `Send + Sync` and prevents sharing across threads.
- **Gemini wire format uses `functionDeclarations`** (not `tools` as in OpenAI schema) — request body structure diverges between providers. Never share request structs across providers.
- **Anthropic requires `max_tokens`** — Anthropic's API returns an error if `max_tokens` is omitted. Handle HTTP 529 (overloaded) with a retry.
- **OpenAI `arguments` is always a JSON string** — always `serde_json::from_str::<Value>(&call.arguments)`. Deserializing the outer response as `Value` and using `arguments` directly gives a String variant, not an Object — silently producing empty tool results with no error.
- **`list_models()` has hardcoded fallback** — if the provider's model-list API call fails, return a hardcoded default list. The Settings tab must always show something.
- **Gemini rate limits are tracked internally**: 10 RPM / 250 RPD. Don't rely on API errors to surface quota exhaustion.
- **Elite-spec weapon gate is a hard error, not a warning** — `validation.rs::validate_weapon_set()` pushes weapons requiring an unequipped elite spec to `result.errors` AND nulls them out in the returned `ValidatedWeaponSet`. `engine.rs` rejects the LLM build when `validated.errors` is non-empty (not only when `validated.specializations` is empty). **Test implication**: any test checking `validated.warnings` for a spec-gated weapon must be updated to check `validated.errors`.

#### Optimization Pipeline Rules

- **3-tier fallback**: deterministic synergy → Gemini pipeline → legacy `optimize()`. Each tier falls back to the next on failure with a Warning log.
- **Synergy pipeline is primary; legacy `optimize()` is last-resort fallback** — new optimizer logic belongs in `synergy_pipeline.rs`, not the legacy path.
- **`GameDb` is authoritative for all build resolution** — all spec, skill, trait, and item lookups use O(1) `GameDb` HashMap lookups. "During resolve" means any code reachable from `optimize_*()`, including rotation simulator, synergy scorer, and trait-fact evaluator. No disk I/O anywhere in that call tree.
- **`BuildLocks` must be respected in ALL optimizer paths** — call `build_locks.is_spec_locked()` and `build_locks.is_trait_locked()` before mutating any spec or trait slot. This applies to `engine.rs`, `synergy_pipeline.rs`, `search.rs`, and any new path or helper added in the future.
- **`Fact::PrefixedBuff` maps to `NormalizedEffect::AppliesStatus` (same as `Fact::Buff`)** — PrefixedBuff (AoE/on-hit effects, e.g. "To nearby enemies") must be handled identically to Buff in two independent paths: (a) `synergy.rs::extract_effects_from_fact()` for synergy scoring, (b) `gemini_tools.rs::summarize_trait_facts()` for LLM prompt context. Silently dropping PrefixedBuff causes on-hit condition/buff skills to score zero in synergy optimization and appear absent from LLM trait summaries.
- **Might buffs both Power AND Condition Damage** — each Might stack = `+30 Power` AND `+30 Condition Damage`. `rotation/simulator.rs::estimate_buff_dps_value("Might")` must compute both contributions (`power_value + condi_value`). Using only Power systematically undervalues Might for condition builds in the DPCT scheduler.
- **Never adjust normalization constants without cross-build validation**:
  `STRIKE_DPS_NORM = 3000`, `CONDI_DPS_NORM = 3500`, `EFFECTIVE_HEALTH_NORM = 50000`, `HEALING_NORM = 1500` — empirically tuned ceilings. Changing them invalidates all relative build scores.
- **`WEIGHT_BUDGET = 2.0` is not arbitrary** — models GW2 gear stat trade-offs. `set_constrained()` proportionally scales other axes. Never change this constant.

#### Serde / API Resilience

- **All GW2 API struct fields that may be absent must be `Option<T>`** — the API omits fields rather than sending `null`. Non-optional fields cause silent deserialization failures.
- **Use `filter_map(from_value(...).ok())` for fact collections** — facts sometimes lack a `type` field. Skip unparseable entries silently rather than failing the whole response.
- **Atomic save applies to ALL persistent writes** — not just `AppConfig`. Any file the addon writes that must survive a mid-write crash (config, cache index, build saves) must use the `.tmp` + `std::fs::rename` pattern.

---

## Usage Guidelines

**For AI Agents:**
- Read this file before implementing any code in this project
- Follow ALL rules exactly — these are non-negotiable invariants, not suggestions
- When in doubt, prefer the more restrictive option
- The Critical Don't-Miss section has caused real bugs — treat it as a checklist

**For Humans:**
- Keep this file lean and focused on what agents actually get wrong
- Update when technology stack changes or new gotchas are discovered
- Remove rules that become obvious over time
- Re-run `/bmad-bmm-generate-project-context` in a fresh session to regenerate from code

_Last Updated: 2026-03-03 (updated with 8 new rules from recent fix commits)_
