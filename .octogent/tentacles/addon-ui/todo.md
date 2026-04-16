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
  `lock_panel.rs` is the signature visual. Add keyboard focus states,
  subtle hover animations within ImGui's capabilities, and a
  "drag-to-reorder" affordance for spec slots if feasible.

- **Reduce `render_settings_tab` size** — currently ~450 lines
  (`mod.rs:1291-1742`). Break into `render_api_keys_section`,
  `render_model_picker_section`, `render_theme_section`,
  `render_cache_section`, `render_hardstuck_section`. Each
  independently testable.

- **Cancellation coverage for every bg thread** — audit every
  `std::thread::spawn` site; confirm the spawned closure holds a
  `CancellationToken` clone and exits at the next loop boundary.
  Add a test per spawn site (the `test_cancel_token_*` tests exist —
  extend to each real consumer). Pair with the engine-side loop audit
  in `optimizer-engine`.

- **Setup-flow UX: recoverable errors** — if key validation fails
  mid-setup, users currently restart. Add "retry this step" behavior
  with clear error surface. Billing-warning path from
  `KeyValidationResult` should show in orange, not block completion.

- **Keybind surface audit** — `register_keybind_with_string` registers
  the toggle. Confirm no collisions with common GW2 keybinds by
  documenting the chosen default, exposing it in Settings, and writing
  it to `AppConfig`.
