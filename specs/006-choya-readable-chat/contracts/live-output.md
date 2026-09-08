# Contract: live output, Stop and Retry

## `llm::live` (optimizer crate)

Thread-local sink, installed by the addon's `spawn_worker` beside the cancellation predicate and cleared when the worker ends. Free functions the stream readers call; no-ops when no sink is installed (examples, tests, the `choya_live` harness):

```
live::step(Step)               // Handshake | Reference | Lookup(n) | Scoring | Writing | Fallback
live::reasoning(&str)          // a reasoning delta, appended
live::content(&str)            // an answer delta, appended
live::tool_call(&str)          // a tool name as it is called
live::mode(LiveMode)           // Reasoning | Content | ToolsOnly | AtOnce
```

Callers: `sse.rs` (content and `reasoning_details` deltas), `anthropic::read_anthropic_stream` (`text_delta`, `thinking_delta`), `gemini` stream reader (`text` parts, `thought: true` parts), `tool_loop::run` (steps and tool names), the chat worker (Handshake, Reference, Scoring, Fallback).

Reasoning is requested where the provider allows it: OpenRouter/OpenAI-compatible through the existing `reasoning.effort`; Gemini through `thinking_config.include_thoughts: true`; Anthropic through extended thinking when the model profile has it on. A provider that returns no reasoning sets `mode(Content)`; one that does not stream sets `mode(AtOnce)` before the request.

## `LiveOutput` (addon)

Read every frame by the thinking bubble; written only through the sink. Stall = `now - last_activity >= 20 s`, shown as "nothing for N s".

## Thinking bubble

Collapsed: Choya's animation, `{step name} · {mm:ss}`, the stall line when stalled, a Stop button. Expanded (click): the last 12 lines of the mode's text (reasoning, else content, else tool lines), newest at the bottom, plus the same Stop button. Clicking again collapses. No limit ends the request.

## Stop

`state.cancel_and_renew()` (the existing cancel-and-replace), `chat_epoch += 1`, `waiting = false`; if `content` is non-empty it becomes the reply with `stopped = true` and the marker line `chat.stopped`; else a one-line `chat.stopped` reply. Log line `Choya {step}: stopped in {secs}s`. The transport ends at the next SSE line or at the read-idle timeout; either way the epoch has retired it.

## Retry

A Retry button on any reply with `stopped == true` or on a fallback reply. It re-sends `retry_of` with a continuation note in the kitchen brief:

```
Continuation. You were answering this and stopped after:
<partial content>
Continue from there. Do not repeat what is above.
```

With empty partial content the note is omitted and the bubble shows `chat.retrying_over`.

## Log lines (`nexus::log`, Info, source `GW2BuildOpt`)

```
Choya handshake: ok in 1.2s
Choya reference: ok in 3.4s
Choya lookup 1: tools: list_sigils, score_build in 41.0s
Choya lookup 2: tools: search_upgrades in 37.5s
Choya writing: no answer (HTTP error: … (no reply within 150s)) in 150.2s
Choya fallback: AboutPlate in 2.1s
Choya writing: stopped in 63.0s
```
