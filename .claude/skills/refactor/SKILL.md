---
name: refactor
description: Idiomatic refactoring patterns for GW2 Build Optimizer. Invoke before restructuring code in crates/{addon,core,gw2api,optimizer}. Codifies the project's idioms (with_state closure, CancellationToken thread pattern, ImGui Window builder, Screen routing, atomic config save) and lists what NOT to "improve" because it's empirically calibrated.
---

# Refactor — GW2 Build Optimizer

Before restructuring code here, match the existing idioms below. The "Do Not Refactor" list at the bottom protects calibration that looks like it could be cleaned up but cannot.

## Idioms to Match

### Global state — `Mutex<Option<AddonState>>` + `with_state(|s| ...)`

The `AddonState` static is `Mutex<Option<AddonState>>`. All access goes through a closure helper:

```rust
with_state(|s| {
    // mutate or read s here
});
```

Refactors that introduce raw `STATE.lock().unwrap()` calls or split the option/mutex apart break every call site. Stay inside the closure.

### Background work — `std::thread::spawn` + `with_state` callback

No channels, no `tokio`, no `crossbeam`. The pattern is:

```rust
let token = state.cancellation_token.clone(); // Arc<AtomicBool>
std::thread::spawn(move || {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // ... do work, periodically check token.is_cancelled() ...
    }));
    with_state(|s| {
        // post result back into AddonState
    });
});
```

Refactors that introduce async runtimes or message channels here are over-engineering and will conflict with the Nexus addon thread model.

### ImGui via nexus-rs — `Window::new(...).build(ui, || { ... })`

UI lives in `crates/addon/src/ui/`. Window construction is a builder ending in `.build(ui, closure)`. Refactors that try to abstract over the closure (returning views, event objects, etc.) fight the `nexus-rs` API surface.

### Screen routing — `Screen` enum dispatch

`AddonState.screen: Screen` is the source of truth for what renders. Add a variant, dispatch in the render match. Do not introduce a parallel routing system or trait-object screen registry.

### Config — `AppConfig` atomic save (`.tmp` + rename)

`AppConfig` persistence writes to a `.tmp` file then renames over the target. Refactors that "simplify" to a direct write corrupt config on crash. Keep the temp+rename.

### LLM providers — `LlmClient` trait

To add or restructure a provider, implement `LlmClient` (`Send + Sync`, `&self`) and wire into `create_client(config, addon_dir)`. Each provider owns its wire format internally. Do not push wire-format details (headers, auth, retry codes) into the trait — the provider hides them.

### GW2 API — manual query strings, bulk chunked at 200

Any new GW2 API helper builds query strings by hand to preserve commas. Bulk ID requests chunk at 200.

### UTF-8-safe truncation

`text.chars().take(N).collect::<String>()`. Never byte-slice strings.

## What NOT to Refactor

These look like they could be tidied. They cannot. Touching them silently breaks scoring or behavior calibrated against real builds.

- **`STRIKE_DPS_NORM = 3000`** and the family of `*_NORM` constants in `crates/optimizer/src/scoring.rs`. Empirically tuned against real combat output across all 7 archetypes. Do not "round to a nicer number" or "extract to config".
- **`WEIGHT_BUDGET = 2.0`** in the gear-trade-off model. `set_constrained()` proportionally scales other axes against this. Changing it invalidates every archetype's scoring.
- **JT_ROUND-style empirically tuned constants** elsewhere in `combat.rs` / `scoring.rs` — any literal that lacks an obvious physical meaning is probably calibrated. Leave it.
- **`select_gear_prefix()` cosine-sim authority.** Do not "trust the LLM's gear prefix" as a simplification — Gemini ignores gear constraints, so the override is load-bearing.
- **`catch_unwind` wrapping the optimization thread.** Looks like belt-and-suspenders; it actually prevents `Mutex` poisoning when Gemini parsing panics. Removing it makes the addon fragile.
- **The 3-tier fallback chain.** It looks redundant ("why two Gemini paths?"). The legacy `optimize` + `enrich_with_gemini` exists as the safety net for when the new pipeline fails. Do not collapse the tiers.

## Cleanup You Are Allowed to Do

- Renaming locals for clarity inside a single function.
- Extracting a helper used 3+ times in one file.
- Replacing `match x { Some(y) => ..., None => ... }` with `if let` / `let else` where it shortens and reads better.
- Removing imports/variables your refactor itself orphaned.

## Cleanup You Are NOT Allowed to Do

- Touching unrelated files because "while I was here".
- Reformatting code outside your diff.
- Deleting pre-existing dead code (mention it, do not remove it — see the global rule in `~/.claude/CLAUDE.md`).
