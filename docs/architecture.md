# Architecture

GW2 Build Optimizer is a Rust workspace producing a single Windows DLL loaded by the [Nexus](https://raidcore.gg/Nexus) addon manager inside Guild Wars 2.

## Workspace Layout

Four crates (see `Cargo.toml` `[workspace]`) compile into one `cdylib`:

| Crate | Type | Responsibility |
|-------|------|----------------|
| `crates/addon` | `cdylib` | Nexus entry point (`lib.rs`), global `AddonState` (`state.rs`), ImGui UI (`ui/`). Linked via the `nexus` crate from [nexus-rs](https://github.com/Zerthox/nexus-rs). |
| `crates/core` | lib | Shared types (`types.rs`: `ResolvedBuild`, `StatBlock`, `CombatMetrics`, `SavedBuild`), config (`config.rs`: `AppConfig`, `LlmProvider`, per-provider keys/models), persistence (`storage.rs`). |
| `crates/gw2api` | lib | GW2 API v2 client: rate limiter (300 burst, 5/sec refill), cache (`cache.rs`), bulk download orchestration (`download.rs`, max 200 IDs/request), serde models (`models/`). |
| `crates/optimizer` | lib | Combat math (`combat.rs`, `stats.rs`), scoring (`scoring.rs`), engine (`engine.rs`), synergy pipeline (`synergy.rs`, `synergy_pipeline.rs`), validation (`validation.rs`), rotation simulator (`rotation/`), LLM clients (`llm/`), prompts (`prompts.rs`), context builder (`context.rs`). |

## Optimization Pipeline (3-Tier Fallback)

Each tier falls back to the next on failure with a Warning log:

1. **`engine::optimize_deterministic()`** — pure-Rust synergy pipeline, no LLM call.
2. **`engine::optimize_with_gemini()`** — cosine-similarity gear-prefix selection + pre-computed context (~40-50K tokens) + a single LLM call + `validate_gemini_build()`.
3. **Legacy `engine::optimize()` -> `enrich_with_gemini()`** — deterministic gear+spec search, Gemini enriches post-hoc.

## LLM Provider Abstraction

`crates/optimizer/src/llm/` defines a `LlmClient` trait (`Send + Sync`, `&self`) with a `create_client(config, addon_dir)` factory. Providers handle wire format internally:

- **Gemini** — `functionDeclarations`, `x-goog-api-key`, rate tracking (10 RPM, 250 RPD).
- **OpenAI** — Chat Completions, JSON-string tool args, `tool_call_id`, `Authorization: Bearer`.
- **Anthropic** — Messages API, content blocks, `x-api-key`, 529 retry, mandatory `max_tokens`.

Each provider implements `list_models()` (Settings tab auto-fetches with hardcoded fallback) and `validate_key_detailed() -> KeyValidationResult { valid, message, warning }` to separate auth (401) from billing (400/403/429 with billing keywords).

## Data Flow

```
User input (UI tab)
  -> AddonState (Mutex<Option<...>>) via with_state(|s| ...)
  -> background std::thread::spawn (clones CancellationToken)
  -> optimizer::engine::optimize_*() (tiered)
        |- gw2api::client (rate-limited, cached)
        |- optimizer::context::build_context() (~40-50K tokens)
        |- optimizer::llm::LlmClient (Gemini/OpenAI/Anthropic)
        |- optimizer::validation::validate_gemini_build()
  -> result posted back via with_state() callback
  -> UI renders comparison view (current vs suggested)
  -> apply_gemini_response() writes to AppConfig (atomic .tmp + rename)
```

## External Dependencies

- **`nexus`** crate (nexus-rs) — addon API, ImGui bindings (`Window::new().build(ui, || { ... })`).
- **GW2 API v2** (`api.guildwars2.com`) — gear, traits, runes, sigils, characters, builds.
- **LLM providers** — Google Gemini (`generativelanguage.googleapis.com`), OpenAI Chat Completions, Anthropic Messages.

## Cross-Cutting Concerns

- **Global state** — `Mutex<Option<AddonState>>` static; access via `with_state(|s| ...)`.
- **Background work** — `std::thread::spawn` + `with_state()` callback (no channels). All threads clone a `CancellationToken` (`Arc<AtomicBool>`) and check `is_cancelled()`.
- **Panic isolation** — optimization background threads wrap work in `catch_unwind` to prevent mutex poisoning.
- **Screen routing** — `AddonState.screen: Screen` enum drives render dispatch.
- **Config** — `AppConfig` saves atomically (`.tmp` + rename); `is_setup_complete()` requires `gw2_key + active_llm_key + cache`.
- **UTF-8 safety** — always `text.chars().take(N).collect()`, never `&text[..N]` (multibyte panic).

## Domain Reference

See [`optimizer-data-schemas.md`](optimizer-data-schemas.md) for the persisted
optimizer schemas and [`crates/core/src/types.rs`](../crates/core/src/types.rs)
for the canonical Rust domain types.
