# Existing Implementation Review: OpenRouter Model Selection

**Reviewed**: 2026-05-13
**Branch**: `001-openrouter-model-selection`
**Purpose**: Identify what Claude already implemented so follow-up work closes gaps instead of rebuilding completed OpenRouter paths.

## Existing Work To Preserve

- OpenRouter is present as an LLM provider with separate API key and model fields in `crates/core/src/config.rs`.
- The active provider routing path includes OpenRouter in `active_api_key()`, `active_model_id()`, `has_active_llm_key()`, and `create_client()`.
- `OpenRouterClient` exists in `crates/optimizer/src/llm/openrouter.rs` and implements validation, generation, tool-call generation, response caching, model listing, and usage persistence.
- The settings UI includes provider selection, key save/test behavior, dynamic model fetching, a searchable model dropdown, model persistence, refresh, and usage display.
- The first-run setup flow includes OpenRouter help text and validates an OpenRouter key before saving it.
- `cargo check` passes, and targeted OpenRouter unit tests pass.

## Gaps Found

1. **Invalid keys can be persisted before validation in Settings**

   The Settings "Save" path stores the entered key before validation finishes. `is_setup_complete()` only checks that the active provider has a non-empty key, so an invalid key can become persisted configuration and may let setup appear complete after restart if the cache is already present.

   Relevant code:

   - `crates/addon/src/ui/main_view/tabs/settings.rs`: key save stores provider key before validation.
   - `crates/core/src/config.rs`: `has_active_llm_key()` and `is_setup_complete()` treat non-empty key presence as sufficient.

   Plan around it: preserve the current save/test UX, but ensure invalid validation results cannot silently produce a complete setup state.

2. **Model fetch can retry every frame after an error or empty result**

   `render_model_picker_section()` auto-fetches whenever `available_models` is empty, loading is false, and a key exists. It ignores `models_error`, so a failed fetch can spawn repeated background fetches every render frame.

   Relevant code:

   - `crates/addon/src/ui/main_view/tabs/settings.rs:223`
   - `crates/addon/src/ui/main_view/stats.rs:5`

   Plan around it: add a retry gate so automatic fetch happens once per provider/key state, then requires explicit Refresh after failure.

3. **Fetched model results are not tied to the provider/key that requested them**

   `start_fetch_models()` captures a config snapshot, but when the background thread completes it writes results into the current state without checking that the active provider and key still match the snapshot. A provider/key change during fetch can populate the dropdown with stale models.

   Relevant code:

   - `crates/addon/src/ui/main_view/stats.rs:5`
   - `crates/addon/src/ui/main_view/tabs/settings.rs:79`

   Plan around it: tag model fetches with provider plus credential identity, and discard stale completions.

4. **Saving a new key does not clear stale model state**

   Provider changes clear `available_models`, `models_error`, and `settings_model_search`, but saving a replacement key for the same provider does not clear the fetched catalog or the selected model. This can leave a model selected from a previous key.

   Relevant code:

   - `crates/addon/src/ui/main_view/tabs/settings.rs:79`
   - `crates/addon/src/ui/main_view/tabs/settings.rs:209`

   Plan around it: on key replacement, clear fetched models, clear search, and require the selected model to be reconfirmed or validated against the new catalog.

5. **First-run setup validates OpenRouter but does not offer model search/selection**

   The onboarding flow validates the key and moves directly to data download. Searchable model selection exists later in Settings, so the requirement is partially satisfied but not in the immediate "provide key, then search/select model" setup path.

   Relevant code:

   - `crates/addon/src/ui/setup.rs:225`
   - `crates/addon/src/ui/setup.rs:428`

   Plan around it: decide whether first-run setup must include model selection before the app is considered ready. If yes, reuse the existing Settings model picker behavior instead of implementing a second picker from scratch.

6. **Model labels may be ambiguous**

   The dropdown searches by id and label, but it displays only the label. If multiple routed models have similar names, the user may not be able to distinguish vendor/id variants at selection time.

   Relevant code:

   - `crates/addon/src/ui/main_view/tabs/settings.rs:256`

   Plan around it: display enough identifying text in each model row, such as label plus stable id.

7. **Coverage does not prove the full OpenRouter user flow**

   Existing OpenRouter unit tests cover local serialization, tool-call wire shape, trimming, and rate tracker behavior. They do not cover config routing for OpenRouter, model-list parsing, validation status handling, stale model fetch discard, key-change clearing, or first-run model selection.

   Relevant code:

   - `crates/optimizer/src/llm/openrouter.rs:758`
   - `crates/core/src/config.rs:460`
   - `crates/optimizer/tests/live_llm.rs:295`

   Plan around it: add focused tests around the gaps rather than rewriting the provider.

## Verification

- `cargo check`: passed.
- `cargo test --all-targets -- --test-threads=1`: failed in an unrelated data consistency test:
  `data::consistency_tests::tests::consistency_test_interaction_operations_used_per_mode`.
- `cargo test -p gw2-optimizer llm::openrouter -- --test-threads=1`: passed, 13 tests.
- `cargo test -p gw2-core config::tests -- --test-threads=1`: passed, 9 tests.
- `OPENROUTER_API_KEY`: not set in this shell, so no live OpenRouter smoke test was run.

## Recommended Next Plan

1. Preserve the existing OpenRouter client, config fields, factory routing, settings model picker, and generation path.
2. Add tests that lock the desired behavior before editing:
   - OpenRouter active config routes to OpenRouter key and selected model.
   - Settings key replacement clears stale model list/search and invalidates stale selection.
   - Model fetch failure does not auto-retry every render frame.
   - Stale model fetch completions are discarded after provider/key changes.
   - OpenRouter model rows expose both display label and stable model id.
3. Patch only the failing gaps.
4. Run targeted tests, then rerun `cargo check` and the full test suite. Keep the existing unrelated data consistency failure separate unless the user chooses to handle it in this feature.
