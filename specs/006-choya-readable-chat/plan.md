# Implementation Plan: Choya readable chat

**Branch**: `006-choya-readable-chat` (off the Sprint 2 tip `515cc29`) | **Date**: 2026-09-08 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/006-choya-readable-chat/spec.md`; research in [research.md](research.md); in-game findings of 2026-09-08.

## Summary

Make Choya's chat usable: replies shown whole and laid out as bullets, bold names, italics, warnings, rotation arrows and links (R1) with the model asked for that shape (prompt boundary released); comparison tabs tinted by kind, current, optimized, published, with no silent strip replacement (R2); published cards bound to the plate rather than to the last message (R3); a fallback chosen by request kind so a scoring question gets the referee's verdict and a build request gets the right profession (R4); a thinking bubble fed by a thread-local live sink from the three stream readers, with stall detection, Stop, Retry and one log line per step, and no time limit on the wait (R5).

## Technical Context

Rust 2021; Windows MSVC Nexus cdylib; ImGui through nexus-rs with draw-list text. Crates touched: `gw2-build-optimizer` (chat_bar.rs, new chat_markup.rs, comparison.rs, theme.rs, fonts.rs, main_view/chat_flow.rs, provider_picks.rs, tabs/kitchen.rs, tabs/saveload.rs, state.rs), `gw2-optimizer` (`llm/live.rs` new, `llm/sse.rs`, `llm/anthropic.rs`, `llm/gemini.rs`, `llm/openai_compat.rs`, `llm/tool_loop.rs`, `prompts.rs` REPLY SHAPE only), `locales/*.json` (12). No new dependency. Storage: `kitchen.json` history keeps whole replies. Testing: `cargo test` unit tests for every pure piece; in-game quickstart for rendering and streaming. Performance: the span layout runs per frame per visible bubble, measured spans cached per message by width. Constraints: FR-009 (no scoring or simulator change), FR-010 (locale parity), release gate unchanged, `choya_live.rs` untouched.

## Constitution Check

The constitution file is an unfilled template. Project gates applied before research and after design: no secrets in artifacts; SymForge for discovery (`find_references` on `ChatMessage`, `BuildSuggestion`, `LlmClient` callers, `add_plated_response`, `apply_loaded_suggestion`); every behaviour change guarded by a unit test where the code is pure and by a quickstart step where it is not; owned files: `choya_live.rs` untouched, `prompts.rs` only the REPLY SHAPE paragraph and the three sentence-count phrases, `llm/` only the live sink calls and two reasoning flags; user tests in-game before release; user merges PRs. Post-design: the live sink is thread-local with no-op defaults, so every existing caller and test compiles unchanged; `ChatMessage` new fields are `serde(default)`, so old `kitchen.json` files load; `TabKind` is derived, so no saved data changes. No violations.

## Project Structure

### Documentation (this feature)

```text
specs/006-choya-readable-chat/
├── plan.md
├── spec.md
├── research.md            # R1–R8
├── data-model.md
├── contracts/
│   ├── reply-markup.md    # what the model writes, what the bubble draws
│   └── live-output.md     # sink, bubble, Stop, Retry, log lines
├── quickstart.md
└── tasks.md               # /speckit-tasks output
```

### Source code (repository root)

```text
crates/addon/src/ui/chat_markup.rs               # new: parser, span wrapper, draw pass (pure parts tested)
crates/addon/src/ui/chat_bar.rs                  # no cap; span bubble; thinking bubble with expand/Stop; Retry button
crates/addon/src/ui/fonts.rs                     # italic face beside the chosen family when present
crates/addon/src/ui/comparison.rs                # TabKind, tinted strip with Current tab, published labels
crates/addon/src/ui/theme.rs                     # tab tint derivation + contrast test
crates/addon/src/ui/main_view/provider_picks.rs  # key from the plate
crates/addon/src/ui/main_view/tabs/saveload.rs   # push, do not replace
crates/addon/src/ui/main_view/chat_flow.rs       # RequestKind, fallback by kind, live steps, log lines, Stop, Retry, continuation brief
crates/addon/src/ui/main_view/tabs/kitchen.rs    # bubble wiring, Stop/Retry actions
crates/addon/src/state.rs                        # chat_live, cancel_and_renew for chat
crates/optimizer/src/llm/live.rs                 # new: thread-local sink, Step, LiveMode
crates/optimizer/src/llm/{sse,anthropic,gemini,openai_compat,tool_loop}.rs  # sink calls; include_thoughts / thinking flags
crates/optimizer/src/prompts.rs                  # REPLY SHAPE paragraph; "2-4 sentences" → the shape
locales/{en,de,es,fr,it,ja,ko,nl,pl,pt,ru,zh}.json  # keys from research R6
```

**Structure Decision**: one new addon module for markup, one new optimizer module for the live sink; everything else lands in the files that own the behaviour today.

## Execution order

1. **Whole replies and markup (US1).** `chat_markup.rs` parser and wrapper with tests; remove the 600-character cap; bubble draws spans; italic face lookup; REPLY SHAPE in the three prompts with a prompt test that the paragraph is present. Commit.
2. **Tabs (US2).** `TabKind`, tint derivation with the contrast test, Current tab, published labels, Save-load pushes. Commit.
3. **Cards (US3).** Key from the plate; cards under the newest plated reply. Commit.
4. **Fallback (US4).** `RequestKind` with tests; three fallback branches; `build_failed` only for builds. Commit.
5. **Live sink and bubble (US5).** `llm::live` with no-op defaults and tests; sink calls in the three readers and the tool loop; reasoning flags for Gemini and Anthropic; `LiveOutput` in state; the bubble with expand, stall line, Stop; Retry with the continuation brief; log lines; the 150 s constant becomes the read-idle bound. Commit.
6. **Locales, gates, quickstart, changelog.** Twelve locale files in one commit; fmt, clippy, workspace tests, release build; DLL copied for the in-game run; no bump, no push.

Steps 1 to 4 are independent of 5 and can be reviewed separately. Step 5 is the largest and touches the optimizer crate; it goes last.

## Risks and implementation decisions

- **Faux bold on a variable font** can look heavy at small sizes; the offset is 1 px at UI scale 1 and rounds with `ui_scale`. Checked in-game at the three scales the Settings tab offers.
- **Name detection** by longest match against thousands of names per reply is done once per message and cached; the chips already keep a name index, reuse it.
- **Thread-local sink and the tool loop**: `tool_loop::run` is generic over `TurnDriver`; the sink calls go in `run`, not in each driver, so the three providers get steps for free and only the stream readers add deltas.
- **Gemini `include_thoughts`** adds reasoning tokens to the response and may cost quota on paid keys; it is sent only when the model profile says the model reasons, the same gate OpenRouter's `reasoning.effort` uses.
- **Stop on a silent socket**: the thread lives until the read-idle timeout; the epoch retires its result. Unload still waits through `join_workers` as today.
- **Retry as continuation** depends on the model honouring "do not repeat"; the test only checks the brief is built; the in-game check reads the result.
- **Provider picks key from the plate**: a player who selects a published tab and asks a question keeps the plate's cards, which is the intended reading of "until the next build".
- **`apply_loaded_suggestion` pushing** means a strip can grow; the strip wraps like `render_result_pane_tabs` does.

## Parallel opportunities

Steps 1–4 touch different files and can be drafted in parallel by separate agents on worktrees (chat_markup / comparison+theme / provider_picks+chat_bar cards / chat_flow fallback); step 5 is serial after them because it edits chat_bar.rs and chat_flow.rs again. Locale keys are collected across steps and written once in step 6.

## Complexity Tracking

No constitution violations. Additions: one markup module, one live-sink module, two `ChatMessage` fields, one `LiveOutput` state, one derived `TabKind`, one `RequestKind`, sixteen locale keys, one prompt paragraph. No new crate, dependency or format.
