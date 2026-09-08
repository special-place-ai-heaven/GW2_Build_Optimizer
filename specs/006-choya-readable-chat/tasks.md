---

description: "Task list for Choya readable chat (006)"
---

# Tasks: Choya readable chat

**Input**: Design documents from `specs/006-choya-readable-chat/` (plan.md, spec.md, research.md R1–R8, data-model.md, contracts/reply-markup.md, contracts/live-output.md, quickstart.md)

**Tests**: Research R7 asks for unit tests on every pure piece; each story phase lists them before the code they guard. Rendering and streaming are checked in-game (quickstart §1–§6).

**Standing rules** (apply to every task): `crates/optimizer/examples/choya_live.rs` untouched; `crates/optimizer/src/prompts.rs` changes only the REPLY SHAPE paragraph and the three "2-4 sentences" phrases; `crates/optimizer/src/llm/` gains only `live.rs`, sink calls, and two reasoning flags; no scoring constant, gate, threshold or simulator rule moves; no push, no version bump, no release; SymForge for discovery; Python edits on this CRLF repo use `newline=""`.

**Organization**: Tasks grouped by user story. Stories 1–4 are independent; story 5 is serial after them (it edits chat_bar.rs and chat_flow.rs again).

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: different files, no dependency on an incomplete task
- **[Story]**: US1–US5 from spec.md

## Path conventions

- Addon UI: `crates/addon/src/ui/…`, state: `crates/addon/src/state.rs`
- Optimizer: `crates/optimizer/src/…`
- Locales: `locales/{en,de,es,fr,it,ja,ko,nl,pl,pt,ru,zh}.json`

---

## Phase 1: Setup

- [X] T001 Confirm branch `006-choya-readable-chat` is checked out on tip `2a2ed40`, tree clean, and record the pre-change gate baseline: `cargo test --workspace` counts and `cargo clippy --workspace --all-targets -- -D warnings` green; note results in `specs/006-choya-readable-chat/quickstart.md` under a new "Baseline 2026-09-08" line
- [X] T002 With SymForge (`find_references`) list every caller of `add_plated_response`, `wrap_text`, `draw_bubble_text`, `render_comparison`, `apply_loaded_suggestion`, `refresh_provider_picks`, `fallback_reference`, `spawn_worker`, `CLOSING_REQUEST_TIMEOUT`; paste the list as a comment block at the top of `specs/006-choya-readable-chat/tasks.md` Notes section so later tasks edit every site

---

## Phase 2: Foundational

**Purpose**: two data-shape changes every story reads.

- [X] T003 Add `stopped: bool` and `retry_of: Option<String>` with `#[serde(default)]` to `ChatMessage` in `crates/addon/src/ui/chat_bar.rs`; add a test that an old `kitchen.json` message object without the fields deserializes
- [X] T004 [P] Add the sixteen locale keys from research R6 (`chat.stop`, `chat.retry`, `chat.stopped`, `chat.retrying_over`, `chat.stall`, `chat.step_lookup`, `chat.step_scoring`, `chat.step_writing`, `chat.step_handshake`, `chat.live_reasoning`, `chat.live_content`, `chat.live_tools`, `chat.live_at_once`, `cmp.tab_current`, `choya.fallback_verdict`, `choya.fallback_chat`) to `locales/en.json` with English text; run locale parity test #21 and confirm it now fails for the other eleven files (seen-failing; the eleven are filled in T048)

**Checkpoint**: `cargo check -p gw2-build-optimizer` green; parity test red for eleven locales only.

---

## Phase 3: User Story 1 - A reply is shown whole and can be read (Priority: P1) 🎯 MVP

**Goal**: no cap anywhere; bubble draws bullets, bold, italic, names, warnings, rotation arrows, links; prompt asks for that shape.

**Independent Test**: quickstart §1. Unit: `cargo test -p gw2-build-optimizer --lib -- chat_markup` and `cargo test -p gw2-optimizer --lib -- prompts::reply_shape`.

### Tests (write first, watch fail)

- [X] T005 [P] [US1] Create `crates/addon/src/ui/chat_markup.rs` with only the `Block`, `Span`, `SpanStyle` types from data-model.md and a `#[cfg(test)]` module holding the fixtures from contracts/reply-markup.md: 3,000-character reply keeps every character; nested bullets (two-space depth); rotation of six parts; `**` inside a bullet; unknown name stays plain; URL with trailing punctuation excludes the punctuation; unmatched `**` renders literally; `->` in prose with one part is not a rotation; `1. item` is Numbered; `! text` is Warning; `_x_` and `*x*` are Italic. Register `pub mod chat_markup;` in `crates/addon/src/ui/mod.rs`
- [X] T006 [P] [US1] Add test `reply_shape_paragraph_present_in_all_three_prompts` in `crates/optimizer/src/prompts.rs` asserting `chat_refinement_prompt_with_tools`, `new_build_prompt_with_tools` and `improve_build_prompt_with_tools` outputs each contain "REPLY SHAPE" and none contain "2-4 sentences"

### Implementation

- [X] T007 [US1] Implement `parse(text: &str, names: &dyn Fn(&str) -> bool) -> Vec<Block>` in `crates/addon/src/ui/chat_markup.rs` per data-model.md rules (line order: bullet `- `/`* `/`• ` with depth, `N. ` numbered, `!`/`⚠` warning, `->`/`→` rotation with ≥2 parts, else paragraph; inline `**x**`, `_x_`/`*x*`, `http(s)://` link, longest-match Name unless Bold or Link); nothing dropped; T005 tests green
- [X] T008 [US1] Implement `wrap_spans(measure: &dyn Fn(&str) -> f32, blocks: &[Block], max_w: f32, line_h: f32) -> Vec<Line>` in `crates/addon/src/ui/chat_markup.rs` (word-boundary splits, mid-word only when a word exceeds `max_w`, bullet indent per depth, `•`/`N.` prefix, `→` joiner for rotation parts); test with a fixed-width measure that a 3,000-char paragraph yields lines all ≤ `max_w` and concatenates back to the input
- [X] T009 [US1] Remove the 600-char cut and `...` append in `add_plated_response` (`crates/addon/src/ui/chat_bar.rs` ~line 709); keep `fold_punctuation`; `CHAT_HISTORY_CAP` stays the only bound; add test `plated_response_keeps_whole_text` with a 3,000-char input
- [X] T010 [US1] Add `italic_font_id()` to `crates/addon/src/ui/fonts.rs`: beside the chosen family file look for `segoeuii.ttf`, `ariali.ttf`, `georgiai.ttf` (same directory, family-matched), load through the Nexus `add_font_from_file` path as the existing faces are, store `Option`; atlas-rebuild null font rule applies (store `None`, skip push)
- [X] T011 [US1] Replace `draw_bubble_text` in `crates/addon/src/ui/chat_bar.rs` with a span draw pass over `wrap_spans` output: Plain in `pal().cream`; Bold drawn twice 1 px apart (offset rounded by `ui_scale`); Italic pushes `italic_font_id()` when `Some`, else muted colour; Name in accent; Warn line in `WARN`; Link underlined in accent with a 1 px line and an invisible button that opens the URL via the existing `render_source_link` path; rotation joiner `→` in accent. Bubble height = sum of line heights + padding. Cache `Vec<Line>` per message keyed by `(text hash, width)` so layout runs once per width
- [X] T012 [US1] Wire the name predicate: build `names: HashSet<String>` from the `GameDb` name index the chips already use (skills, traits, specializations, runes, sigils, relics, weapons) once per `MainState` load in `crates/addon/src/ui/chat_bar.rs`; pass it to `parse`
- [X] T013 [US1] In `crates/optimizer/src/prompts.rs` add the REPLY SHAPE paragraph (contracts/reply-markup.md "Asked of the model", verbatim rules) once as a `const REPLY_SHAPE: &str` and append it to the three prompts; replace the three "2-4 sentences" phrases with "in the REPLY SHAPE below"; T006 green; no other line in the file changes
- [X] T014 [US1] Run `cargo test -p gw2-build-optimizer --lib -- chat_markup` and `cargo test -p gw2-optimizer --lib -- prompts` green; `cargo fmt --all`; commit "Choya chat: whole replies drawn as spans; REPLY SHAPE in the prompts (006 US1)" with the session footer

**Checkpoint**: US1 complete; DLL would already show whole, formatted replies.

---

## Phase 4: User Story 2 - The build tabs say whose build each is (Priority: P2)

**Goal**: Current / Optimized / Published tabs tinted by kind; Save-load pushes.

**Independent Test**: quickstart §2. Unit: `cargo test -p gw2-build-optimizer --lib -- tab_kind theme::tab_tint`.

### Tests

- [X] T015 [P] [US2] Add tests in `crates/addon/src/ui/comparison.rs`: `tab_kind_from_suggestion` (empty `source_url` → Optimized; GuildJen URL → Published("GuildJen"); Current pseudo-tab when a character is selected); `published_label_is_site_and_build`
- [X] T016 [P] [US2] Add test `tab_tints_keep_contrast_over_every_preset` in `crates/addon/src/ui/theme.rs`: for each preset and the custom-theme extremes (all-black, all-white base), the three fills differ pairwise and from `panel_bg` by at least the RGB distance the existing WCAG gate tests use
- [X] T017 [P] [US2] Add test `loaded_suggestion_pushes_not_replaces` in `crates/addon/src/ui/main_view/tabs/saveload.rs`: two suggestions in the strip, load a third → three tabs; load one with an existing label → still three, that tab replaced

### Implementation

- [X] T018 [US2] Add `enum TabKind { Current, Optimized, Published(String) }` and `fn tab_kind(s: &BuildSuggestion) -> TabKind` (derived, never stored) in `crates/addon/src/ui/comparison.rs`; site from `source_url` via the existing `site_colour` host mapping; T015 green
- [X] T019 [US2] Add `pub fn tab_tint(kind_colour: [f32;4]) -> TabTint { fill, rim, selected_fill, selected_text }` to `crates/addon/src/ui/theme.rs` per research R2 (`lerp(pal().chip_idle_fill, kind, 0.30)`, rim at chip rim alpha, selected kind at 0.85 with `gold_button_text`); T016 green
- [X] T020 [US2] Rewrite the strip in `render_comparison` (`crates/addon/src/ui/comparison.rs` ~line 519): always render when a character is selected or >1 suggestion; first tab Current (`cmp.tab_current`) → `show_optimized = false`; each suggestion tab styled with `tab_tint(CURRENT | OPTIMIZED | site_colour)` via `push_style_color` on `Selectable` header/hovered/active; Published label `"{site} · {build}"`; strip wraps like `render_result_pane_tabs`
- [X] T021 [US2] Change `apply_loaded_suggestion` in `crates/addon/src/ui/main_view/tabs/saveload.rs` to push (replace same label) instead of clearing the strip; T017 green
- [X] T022 [US2] Run the US2 tests, `cargo fmt --all`; commit "Comparison: current, optimized and published tabs tinted by kind; loads push (006 US2)"

**Checkpoint**: US2 complete and independently testable.

---

## Phase 5: User Story 3 - Published cards stay until the next build (Priority: P3)

**Goal**: cards keyed to the plate, rendered under the newest plated reply.

**Independent Test**: quickstart §3. Unit: `cargo test -p gw2-build-optimizer --lib -- provider_picks`.

### Tests

- [X] T023 [P] [US3] Add tests in `crates/addon/src/ui/main_view/provider_picks.rs`: `picks_key_from_newest_plate` (key ignores selection, ignores published tabs, changes when a new empty-`source_url` suggestion arrives, empty when none)
- [X] T024 [P] [US3] Add test `cards_anchor_is_newest_open_result_message` in `crates/addon/src/ui/chat_bar.rs`: history [plate reply (open_result), question, answer] → anchor index 0; [plate, new plate] → 1; no plate → None

### Implementation

- [X] T025 [US3] Add `fn picks_key(suggestions: &[BuildSuggestion]) -> Option<String>` building `"{profession}|{mode}|{role}|{specs}"` from the newest suggestion with empty `source_url`; use it in `refresh_provider_picks` (`crates/addon/src/ui/main_view/provider_picks.rs` ~line 21) instead of the selected suggestion; T023 green
- [X] T026 [US3] Add `fn cards_anchor(history: &[ChatMessage]) -> Option<usize>` (newest message with `open_result == true`) in `crates/addon/src/ui/chat_bar.rs` and render the cards under that message in `render_chat_bar` (~line 222) instead of only the last; `Clear` still empties picks with history; T024 green
- [X] T027 [US3] Run the US3 tests, `cargo fmt --all`; commit "Provider cards belong to the plate, not the last message (006 US3)"

**Checkpoint**: US3 complete.

---

## Phase 6: User Story 4 - When the model fails, the fallback answers the question asked (Priority: P4)

**Goal**: fallback by `RequestKind`; scoring questions get the referee's verdict; build requests get the wished profession.

**Independent Test**: quickstart §4. Unit: `cargo test -p gw2-build-optimizer --lib -- request_kind`.

### Tests

- [X] T028 [P] [US4] Add tests in `crates/addon/src/ui/main_view/chat_flow.rs`: `classify_scoring_question_with_plate` ("Score that exact build and tell me the gates and what was not simulated", has_plate → AboutPlate); `classify_same_without_plate` → Chat; `classify_build_with_elite` ("make me a reaper build" → Build{elite: Some("Reaper")}); `classify_ambiguous_is_chat`; `profession_for_elite` ("Reaper" → Necromancer via `db.specializations`)
- [X] T029 [P] [US4] Add test `verdict_bullets_shape` in `crates/addon/src/ui/main_view/chat_flow.rs`: given a `score_full_build` verdict JSON fixture, the formatted reply has lines for viable, gates, score and coverage and starts with `choya.fallback_verdict`

### Implementation

- [X] T030 [US4] Add `enum RequestKind { Build { elite: Option<String> }, AboutPlate, Chat }` and `fn classify(message: &str, has_plate: bool) -> RequestKind` in `crates/addon/src/ui/main_view/chat_flow.rs` reusing `wants_a_build` (~1411), `asks_about_own_build` (~1436), `wished_elite_spec` (~1497); AboutPlate keywords: score, gate, simulate, why, explain, rate, compare, "what was not"; ambiguous → Chat; T028 green
- [X] T031 [US4] (already in place: `send_chat_message` resolves the wished elite's profession before spawning, so `reference_build` gets it) Add `fn profession_for_elite(db: &GameDb, elite: &str) -> Option<Profession>` in `crates/addon/src/ui/main_view/chat_flow.rs`; make `reference_build` (~1125) take the profession and an elite lock instead of always the character's
- [X] T032 [US4] Add `fn verdict_reply(verdict: &serde_json::Value) -> String` in `crates/addon/src/ui/main_view/chat_flow.rs` formatting `gemini_tools::score_full_build` output as bullets (viable, gates, score, what was not simulated) prefixed by `choya.fallback_verdict`; T029 green
- [X] T033 [US4] Replace the single `fallback_reference` branch (`crates/addon/src/ui/main_view/chat_flow.rs` ~line 237) with a match on `classify`: Build → `reference_build(profession_for_elite)` with `build_failed = true`; AboutPlate → `verdict_reply` on the newest plate, no suggestion pushed, `build_failed = false`; Chat → `choya.fallback_chat` line with the two next steps (ask for a build, score the plate), nothing run; "no build to score" line when AboutPlate has no plate
- [X] T034 [US4] Run the US4 tests, `cargo fmt --all`; commit "Choya fallback matches the request: verdict for scoring questions, wished profession for builds (006 US4)"

**Checkpoint**: US4 complete.

---

## Phase 7: User Story 5 - The player can see Choya thinking, stop it, or nudge it (Priority: P5)

**Goal**: live sink from the stream readers, thinking bubble with expand/stall/Stop, Retry with continuation brief, one log line per step, open-ended wait.

**Independent Test**: quickstart §5–§6. Unit: `cargo test -p gw2-optimizer --lib -- llm::live` and `cargo test -p gw2-build-optimizer --lib -- live_output continuation log_line`.

### Tests

- [ ] T035 [P] [US5] Create `crates/optimizer/src/llm/live.rs` with `Step`, `LiveMode`, the `LiveSink` trait and a `#[cfg(test)]` module: `no_sink_is_noop` (calls without `install` do nothing and do not panic); `installed_sink_receives_in_order` (step, reasoning, content, tool_call, mode); `sink_cleared_after_uninstall`. Register `pub mod live;` in `crates/optimizer/src/llm/mod.rs`
- [ ] T036 [P] [US5] Add tests in `crates/addon/src/state.rs` for `LiveOutput`: `stall_after_20s` (last_activity 21 s ago → `stalled_for() == Some(21)`), `reset_on_send` clears text and tools, `elapsed_in_step`
- [ ] T037 [P] [US5] Add tests in `crates/addon/src/ui/main_view/chat_flow.rs`: `continuation_brief_with_partial` (contains the partial and "Do not repeat"), `continuation_brief_without_partial_is_none`, `log_line_format` ("Choya lookup 2: tools: search_upgrades in 37.5s")

### Implementation (optimizer crate)

- [ ] T038 [US5] Implement `crates/optimizer/src/llm/live.rs`: thread-local `RefCell<Option<Box<dyn LiveSink>>>`, `install(sink)`, `uninstall()`, free fns `step`, `reasoning`, `content`, `tool_call`, `mode` per contracts/live-output.md; T035 green
- [ ] T039 [US5] Add sink calls: `crates/optimizer/src/llm/sse.rs` (content delta → `live::content`, `reasoning_details` delta → `live::reasoning`, tool-call name → `live::tool_call`); `crates/optimizer/src/llm/anthropic.rs` `read_anthropic_stream` (`text_delta` → content, `thinking_delta` → reasoning); `crates/optimizer/src/llm/gemini.rs` stream reader (`text` parts → content, `thought: true` parts → reasoning); `crates/optimizer/src/llm/tool_loop.rs` `run` (`live::step(Lookup(n))` per round, `Writing` on the closing call; `live::mode(ToolsOnly)` when the driver does not stream). Nothing else in these files changes
- [ ] T040 [US5] Reasoning flags: `crates/optimizer/src/llm/gemini.rs` sends `thinking_config.include_thoughts: true` only when the model profile reasons (same gate as OpenRouter `reasoning.effort`); `crates/optimizer/src/llm/anthropic.rs` sends extended thinking when the profile has it on and routes `thinking_delta`; `live::mode(Reasoning | Content)` set from the first delta kind seen
- [ ] T041 [US5] In `crates/optimizer/src/llm/openai_compat.rs` turn `CLOSING_REQUEST_TIMEOUT` (150 s) and `TOOL_PHASE_BUDGET` into a read-idle bound on the reqwest client (`read_timeout`), not a wall-clock cap; the closing call has no total timeout; add a comment naming the bubble's stall line as the player-facing signal

### Implementation (addon)

- [ ] T042 [US5] Add `LiveOutput` (fields from data-model.md) and `chat_live: Arc<Mutex<LiveOutput>>` to `MainState` in `crates/addon/src/state.rs`; `spawn_worker` (~line 392) installs a `LiveSink` writing into it beside the cancellation predicate and uninstalls on exit; `LiveOutput::reset()` on every send; `stalled_for()`; T036 green
- [ ] T043 [US5] Replace the "Choya is thinking… mm:ss" bubble in `crates/addon/src/ui/main_view/tabs/kitchen.rs` (~line 61) and `crates/addon/src/ui/chat_bar.rs` with the thinking bubble per contracts/live-output.md: collapsed = animation + `{step} · mm:ss` + stall line when `stalled_for() >= 20` + Stop button; click toggles `expanded`; expanded = last 12 lines of reasoning, else content, else tool lines (`chat.live_*` captions), newest at bottom, `chat.live_at_once` when mode is AtOnce; steps and captions through the locale keys from T004
- [ ] T044 [US5] Stop in `crates/addon/src/ui/main_view/chat_flow.rs`: `state.cancel_and_renew()`, `chat_epoch += 1`, `waiting = false`; partial `content` non-empty → reply with `stopped = true`, `retry_of = Some(user message)` and `chat.stopped` marker line, else one-line `chat.stopped` reply with `retry_of`; log `Choya {step}: stopped in {secs}s`
- [ ] T045 [US5] Retry: a Retry button on any reply with `stopped == true` or a fallback reply (`crates/addon/src/ui/chat_bar.rs`); handler in `crates/addon/src/ui/main_view/chat_flow.rs` re-sends `retry_of` with `continuation_brief(partial)` prepended to the kitchen brief (contracts/live-output.md text); empty partial → plain re-send and the bubble shows `chat.retrying_over`; T037 continuation tests green
- [ ] T046 [US5] Log lines: in the chat worker (`crates/addon/src/ui/main_view/chat_flow.rs`) emit `nexus::log` Info `Choya {step}: {outcome} in {secs:.1}s` for handshake, reference, each lookup round (tool names from the sink), writing, fallback kind, stopped; failed step names the error text the player saw; `live::step(Handshake | Reference | Scoring | Fallback)` calls placed at those points; T037 log test green
- [ ] T047 [US5] Run `cargo test -p gw2-optimizer --lib -- llm` and `cargo test -p gw2-build-optimizer --lib`, `cargo fmt --all`; commit "Choya thinking bubble: live output, stall line, Stop, Retry, one log line per step (006 US5)"

**Checkpoint**: all five stories implemented.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [ ] T048 Fill the sixteen keys in the other eleven locale files `locales/{de,es,fr,it,ja,ko,nl,pl,pt,ru,zh}.json` (one commit); parity test #21 green
- [ ] T049 [P] Add a CHANGELOG.md subsection under the unreleased version: whole replies with markup, tinted tabs, persistent cards, fallback by request kind, thinking bubble with Stop and Retry, per-step log lines (no version bump)
- [ ] T050 [P] Sweep the diff for machine paths, `poslj`, scratchpad, DEBUG, mock and any secret-shaped string; confirm `crates/optimizer/examples/choya_live.rs` has no diff and `crates/optimizer/src/prompts.rs` diff is only the REPLY SHAPE paragraph and the three phrases
- [ ] T051 Gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (Sprint 2 experiments and `pve_output_unchanged_by_conditional_tagging` still green), `cargo build --release`
- [ ] T052 Copy `target/release/gw2_build_optimizer.dll` to the `addons_dir` from `dev.cfg`; write the in-game test instructions (quickstart §1–§6, pass/fail with text seen) for the user; no push, no bump, no release
- [ ] T053 Mark every task `[X]` in `specs/006-choya-readable-chat/tasks.md`; commit "Choya readable chat (006): locales, changelog, gates" with the session footer

---

## Dependencies & Execution Order

- **Phase 1 → 2 → {3, 4, 5, 6} → 7 → 8.**
- US1–US4 are independent of each other (different files) once Phase 2 is done.
- US5 depends on US1 (edits the span bubble in chat_bar.rs) and US4 (edits the fallback branch in chat_flow.rs).
- T048 depends on every locale key being used (after US5); T051–T053 last.

### Story dependency graph

```text
Setup ─ Foundational ─┬─ US1 (markup, cap, prompt) ─┐
                      ├─ US2 (tabs, tints, saveload)│
                      ├─ US3 (picks key, cards)     ├─ US5 (live sink, bubble, Stop, Retry, log) ─ Polish
                      └─ US4 (RequestKind, fallback)┘
```

## Parallel Examples

- **After Phase 2**: four agents on worktrees, one per story: US1 (`chat_markup.rs`, `chat_bar.rs`, `fonts.rs`, `prompts.rs`), US2 (`comparison.rs`, `theme.rs`, `saveload.rs`), US3 (`provider_picks.rs`, the `cards_anchor` fn in `chat_bar.rs`; merge after US1), US4 (`chat_flow.rs`). Cap compile-heavy agents at two concurrent.
- **Inside US5**: T035, T036, T037 (tests) in parallel; T038–T041 (optimizer crate) and T042 (state) in parallel; T043–T046 serial in chat_bar.rs / chat_flow.rs.
- **Polish**: T049 and T050 parallel with T048.

## Implementation Strategy

- **MVP = Phase 1–3 (US1).** A DLL after T014 already fixes the complaint the user raised first: whole, readable replies.
- Then US2, US3, US4 in any order, each a separate commit with its own unit tests.
- US5 last; it is the largest and the only one touching the optimizer's transports.
- Gates and DLL at the end; the user tests in-game before anything is pushed.

## Notes

<!-- T002 reference sites (SymForge find_references, 2026-09-08):
add_plated_response: chat_bar.rs:678 (add_ai_response), chat_flow.rs:1012 (send_chat_message), test :856
draw_bubble_text: chat_bar.rs:306, :360 (render_chat_bar)
wrap_text: chat_bar.rs:163 (bubble_size)
render_comparison: new_build.rs:80; provider_picks.rs:5 (import)
apply_loaded_suggestion: saveload.rs:501 (load_named)
refresh_provider_picks: kitchen.rs:80-81 (render_talk_tab), new_build.rs:68
fallback_reference: chat_flow.rs (local to send_chat_message; index has no symbol)
spawn_worker: 29 sites; chat one is chat_flow.rs:214 (send_chat_message)
CLOSING_REQUEST_TIMEOUT: defined and used in llm/openai.rs:97 (send_closing), not openai_compat.rs
-->
- Seen-failing discipline: each story's tests are written before its code and must fail first.
- `[P]` tasks touch different files; tasks on the same file are serial.
- Commit after every story checkpoint; footers per the session's attribution instruction.
