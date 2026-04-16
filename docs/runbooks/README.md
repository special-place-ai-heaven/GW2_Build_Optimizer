# Runbooks

Step-by-step operational procedures for recurring tasks. Each runbook is a single markdown file; add new ones as `verb-noun.md` (e.g. `deploy-dll.md`).

## Suggested Runbooks

These are the high-value procedures for this project. Write them as you encounter the work for the first time:

### `deploy-dll.md` — Build and deploy DLL to GW2 addons folder

Cover: `cargo build --release`, locate `target/release/gw2_build_optimizer.dll`, copy to `C:\GAMES\Guild Wars 2\addons\`, restart Nexus from in-game menu (or relaunch GW2), verify load via Nexus log. Include the "did it actually load" check.

### `add-llm-provider.md` — Add a new LLM provider to the `LlmClient` trait

Cover: implement the `LlmClient` trait in a new file under `crates/optimizer/src/llm/`, wire it into `create_client(config, addon_dir)` factory, extend `LlmProvider` enum in `crates/core/src/config.rs`, implement `validate_key_detailed()` honoring billing-tolerant semantics (401 = invalid; 400/403/429 with billing keywords = valid), implement `list_models()` with hardcoded fallback, add Settings tab UI bindings, smoke-test with a real key.

### `refresh-gw2-cache.md` — Refresh the GW2 API cache after a game patch

Cover: when ArenaNet ships a balance patch or new content, the local cache (skills, traits, items) goes stale. Document: how to invalidate the cache (delete cache files under the addon data dir? trigger via UI button?), how to monitor the re-download (rate limiter respects 300 burst / 5/sec refill, max 200 IDs per bulk request), expected duration (~100k items), and how to verify success.

### `debug-rate-limiter-429s.md` — Debug GW2 API 429 (rate limit) errors

Cover: where to inspect the rate-limiter state in `crates/gw2api/`, how to verify the burst/refill math against actual request timing, common causes (parallel downloads bypassing the limiter, manual query-string bug re-encoding `,` as `%2C` causing extra requests), and recovery steps. Cross-reference the "Manual query strings for GW2 API" gotcha in `CLAUDE.md`.

## Format

Each runbook should follow this structure:

```markdown
# Title

**When to use:** one-line trigger.

**Prerequisites:** required tools, keys, access.

## Steps

1. ...
2. ...

## Verification

How to confirm the procedure worked.

## Rollback

How to undo if it didn't.

## Common Failures

Known error messages and their fixes.
```
