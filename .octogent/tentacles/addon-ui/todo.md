# Todo

- **Decompose `main_view/mod.rs` (story P1-03)** — 3225 lines, 7 sub-tabs
  (`render_role_picker`, `render_new_build_tab`, `render_improve_tab`,
  `render_settings_tab`, `render_saveload_tab`, etc.). Each sub-tab moves
  to its own file under `main_view/tabs/`. Keep `mod.rs` as the dispatcher.
  Tests first — pin the reconstruction + saveload round-trip behavior
  before moving code.

- **Extract UI test harness** — tests today cover `AddonState` transitions
  but nothing renders-end-to-end. Even a headless "render once to a
  ImGui test context and snapshot draw commands" pass would catch
  layout regressions. Investigate `imgui-rs`'s test surface. Land before
  the UX pass so visual iteration has a regression net.

- **UX pass on radar_chart + gear_diff** — The mission bar is
  "exceptional graphical interface". Both components are functional but
  visually plain. Define a visual-quality checklist (grid lines, axis
  labels, color palette alignment, hover tooltips, empty states) and
  iterate. Compare against GW2 in-game UI aesthetic.

- **Lock-panel polish** — GW2-style hexagon + 3×3 trait grid in
  `lock_panel.rs` is the signature visual.
  - ✅ Subtle hover animations (2026-04-16): single `LockElementId` + `t`
    on `MainState.locks_hover`, lerp driven by `tick_hover` (unit-tested).
    Modulates hex/circle radius, outline thickness, colour brightness, and
    a fade-in glow ring. Verified by `cargo check` + 7 pure-logic tests.
    Not verified in-game — no headless ImGui test harness yet (see next
    todo). A DLL smoke-test before shipping is still required.
  - ⏭ Keyboard focus states: deferred. Requires input routing (Tab
    cycling, arrow nav inside a slot, focus ring rendering, guard against
    chat-bar text-input collisions) that is larger than "polish" and
    warrants its own session.
  - ❌ Drag-to-reorder spec slots: skipped by design. `BuildLocks.specs[2]`
    is the elite-privileged slot — both `locked_elite_id()` and the
    "Locked to: <elite>" badge in `render_improve_tab` read it directly.
    Reordering would silently break that invariant. Decision recorded on
    the doc comment of `BuildLocks.specs` in `crates/core/src/types.rs`.

- **Reduce `render_settings_tab` size** — currently ~450 lines
  (`mod.rs:1291-1742`). Break into `render_api_keys_section`,
  `render_model_picker_section`, `render_theme_section`,
  `render_cache_section`, `render_hardstuck_section`. Each
  independently testable.

- ~~**Cancellation coverage for every bg thread**~~ — audited 2026-04-16.
  Live `std::thread::spawn` sites in `crates/addon/src/ui/`:
  `stats.rs` (`start_fetch_models`, `start_game_data_refresh`,
  `check_api_health`, `load_game_db`), `character.rs`
  (`load_characters`, `load_character_tabs`), `optimization.rs`
  (`start_optimization_with_profession`, `send_chat_message`),
  `setup.rs` (all three setup-step render fns), and `mod.rs`
  (benchmark sync in `render_settings_tab`; API-key Test/Save pair
  duplicated in `render_settings_tab` and `render_api_keys_section`
  during in-progress P1-03 extraction). Every live site clones
  `CancellationToken` and checks `is_cancelled()` at entry, after each
  long op, and before the final `with_state` write. The shared pattern
  is pinned by `test_cancel_token_worker_loop_exits_on_pulse`
  (`crates/addon/src/state.rs`). Per-consumer tests were deferred —
  they would require network-layer stubs for `gw2_api`, `llm`, and
  `scraper` that don't exist, and would mostly duplicate the pattern
  assertion. Pair still pending with the engine-side loop audit in
  `optimizer-engine`.

- **Panic recovery gaps in the settings-tab API-key spawns** —
  surfaced by the cancellation audit. The Test and Save buttons in
  `render_api_keys_section` (and their duplicate originals still in
  `render_settings_tab` until P1-03 finishes) use
  `let _ = std::panic::catch_unwind(...)` — on panic the closure's
  error is discarded and `settings_key_validating` stays `true`
  forever. The benchmark-sync spawn in `render_settings_tab` has no
  `catch_unwind` at all — a scraper panic strands `benchmark_running
  = true` and can poison the mutex if it fires inside a `with_state`
  callback. Align these with the `panic_result.is_err() → clear flag
  + log` pattern used in every `stats.rs` / `character.rs` /
  `optimization.rs` / `setup.rs` spawn.

- **Delete the legacy-settings block comment in `mod.rs`** — 720 lines
  wrapped in `/* LEGACY_SETTINGS_LAYOUT_START ... LEGACY_SETTINGS_LAYOUT_END */`
  carry three dead `std::thread::spawn` sites that show up in every
  text search and already cost one audit re-inspection. Delete the
  block outright; git history preserves it if ever needed.

- **Setup-flow UX: recoverable errors** — if key validation fails
  mid-setup, users currently restart. Add "retry this step" behavior
  with clear error surface. Billing-warning path from
  `KeyValidationResult` should show in orange, not block completion.

- **Keybind surface audit** — `register_keybind_with_string` registers
  the toggle. Confirm no collisions with common GW2 keybinds by
  documenting the chosen default, exposing it in Settings, and writing
  it to `AppConfig`.
