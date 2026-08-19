---
name: release
description: Release procedure for GW2 Build Optimizer — local DLL build and deploy to the Guild Wars 2 addons folder. Invoke when the user asks to ship a release, cut a version, deploy the addon, or update the in-game DLL. There is no CI, no crates.io publish, no GitHub release artifact — release means "build the DLL and copy it into the game directory."
---

# Release — GW2 Build Optimizer

This project ships as a single Windows DLL loaded by Nexus inside Guild Wars 2. There is no remote registry, no CI pipeline, and no released artifact other than the file you copy onto your own machine.

Current version: **1.4.7** (see `Cargo.toml` `[workspace.package] version`).

Standing order: **every code fix is a patch release.** Build the DLL, copy it to `C:\GAMES\Guild Wars 2\addons\`, push, and `gh release create` with `gw2_build_optimizer.dll` + `SHA256SUMS.txt`. README Download URLs stay on `/releases/latest` — never paste a SHA into README.

## Pre-Release Checklist

Run these from the repo root before building the release DLL.

```bash
cargo check          # fast compile sanity check across the workspace
cargo test           # run unit + integration tests
cargo build --release  # produces target/release/gw2_build_optimizer.dll
```

If `cargo test` has new failures, stop. Do not deploy a DLL that does not pass tests.

## Version Bump (only if shipping a version change)

1. Edit `Cargo.toml` `[workspace.package] version = "X.Y.Z"`.
2. Re-run `cargo check` so `Cargo.lock` updates.
3. (Optional) note the change in `CLAUDE.md` "Project" line so future sessions see the new version.

Semver guidance for an in-game addon:
- **Patch (1.0.x)** — bug fixes, no UI/keybind changes, no config schema changes.
- **Minor (1.x.0)** — new features, new UI tabs, new optimizer archetypes, additive config fields (must remain backward-compatible with existing `AppConfig` JSON).
- **Major (x.0.0)** — breaking config schema, removed features, breaking changes to saved-build format.

## Build

```bash
cargo build --release
```

Output: `target/release/gw2_build_optimizer.dll`.

## Deploy

Copy the DLL into the GW2 addons directory:

```
target/release/gw2_build_optimizer.dll
  -> C:\GAMES\Guild Wars 2\addons\
```

If GW2 is running, restart Nexus from the in-game menu (or relaunch GW2) to pick up the new DLL.

Do **not** paste the SHA256 into `README.md`. The Download section uses `/releases/latest/download/…` so it always hits the current DLL and `SHA256SUMS.txt`. Attach `SHA256SUMS.txt` to the GitHub Release only.

## Smoke Test In-Game

After the addon reloads:

1. Open the optimizer window via the configured keybind.
2. Confirm `is_setup_complete()` still passes (GW2 key, active LLM key, cache present).
3. Run one optimization on a known character and confirm the result renders.
4. If any tier fell back, check the Nexus log for the Warning message — that is expected, but a *new* fallback after an update is a smell.

## Rollback

Keep the previous `gw2_build_optimizer.dll` somewhere outside `target/` (which `cargo` may overwrite). To roll back: copy the prior DLL back into `C:\GAMES\Guild Wars 2\addons\` and restart Nexus.

## What This Project Does NOT Do

- **No CI.** There is no GitHub Actions workflow that builds or tests on push. Run `cargo check` / `cargo test` locally.
- **No `cargo publish`.** This is a `cdylib` for an addon manager, not a crate.
- **No auto-update.** Players replace the DLL from GitHub Releases or a local copy.
