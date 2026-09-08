# Choya latency: current status and historical evidence

Reviewed 2026-09-08 against the primary checkout at `0e50a51` on
`007-trait-triggers`. Source paths below refer to that checkout, not this older
`choya-work` worktree. This update is a source review; no new live timings or
in-game acceptance were collected.

## Current behavior

### Prompt and reference data

`crates/optimizer/src/prompts.rs::BUILD_DISCIPLINE` still requires
`find_synergies`, followed by `simulate_rotation` or `simulate_combat`, and
directs trait-detail and condition-source investigation. Prefilling profession
names and upgrade rankings does not remove those instructions. Reducing mandatory
investigation remains a possible latency experiment, with build quality as an
acceptance condition.

The primary checkout does not contain the experimental conditional direct-plate
instructions, additional complete-object prompt check, or `STAT PREFIX REFERENCE`
generated from `db.itemstats`. Its
`crates/optimizer/src/gemini_tools.rs::upgrade_reference` supplies rune, sigil and
relic rankings. Exact gear-prefix evidence remains relevant to test: the historical
experiment encountered a validator rejection for the model's spelling
`Marauder's`.

`score_build` now accepts a whole build and returns the referee's verdict for the
current scenario; its prefix-only mode remains available. This and subsequent
simulator/referee changes mean the old experiment does not measure today's
evaluation path.

### Tool turns and deadlines

`crates/optimizer/src/llm/tool_loop.rs::run` already returns a text answer from an
Explore turn without a closing request, subject to its narration nudge. Closing
is used when gathering ends without a returned answer. There is no extra
post-answer closing request to remove.

Lookup results require a subsequent model turn before the model can use them.
Text emitted alongside tool calls cannot be assumed to incorporate their results.
Duplicate-call caching avoids local execution, not the model request that
produced the duplicate call.

`crates/optimizer/src/llm/openai_compat.rs` now sets
`CLOSING_REQUEST_TIMEOUT` to 1,800 seconds, increased from 150 seconds; the tool
gathering budget remains 150 seconds. The longer closing ceiling accommodates
continued output and user-controlled stopping; it is not a latency improvement.
Historical timings predate this change.

### Live progress is implemented

The shared OpenAI-compatible SSE reader emits content, reasoning and tool-call
events through `llm::live` while accumulating the final response. Native Gemini
and Anthropic readers also emit live events. The chat worker installs a
`LiveScope` sink. The thinking bubble displays recent reasoning, content or tool
lines, elapsed time and a stall indicator. Stop and Retry are present.

Relevant sources: `crates/optimizer/src/llm/{sse,live,anthropic}.rs`,
`crates/optimizer/src/gemini.rs`, and
`crates/addon/src/ui/main_view/{chat_flow.rs,tabs/kitchen.rs}`.

A structured preview of complete build fields is distinct from the existing text
display and remains a possible enhancement. Such a preview must remain separate
from an accepted build and handle incomplete JSON, repair and cancellation.
Streaming cannot show answer content before the provider emits it.

## Current measurement limits

`crates/optimizer/examples/choya_live.rs` already checks all reported
`ValidatedBuild.errors`, three surviving specializations, main-hand weapons in
both sets, rune, relic, four sigils and nonempty gear-prefix assignments. It
supports `CHOYA_REQUEST` and reports HTTP attempts including retries. These
checks are implemented, not outstanding harness fixes.

The harness still forces a cold profile probe for each run and omits the addon's
deterministic reference/referee path. Its displayed round count combines handshake
estimates and progress callbacks and can miss a direct final Explore answer; use
HTTP attempts for request counts. A harness pass does not prove the full addon
path or in-game quality. Report deterministic fallback separately from successful
model-generated builds.

Elapsed model-turn time combines client pacing/retries, transport and provider
work; these measurements do not isolate reasoning time. Sending conversation
history does not prove every prefix token is recomputed without caching.
Reasoning-effort changes have not been validated by this experiment and need a
separate comparison.

## Historical experiment: evidence, not a current benchmark

The 2026-09-07/08 experiment used baseline `1612f96` in `choya-work`.
Records remain in `autoresearch/loop-260907-2209/`, including
[evals-summary.md](../autoresearch/loop-260907-2209/evals-summary.md).

Direct plating without mandatory simulation (`3ac6d64`) reduced strict validity
from 3/6 to 1/6 and was reverted by `34ba85e`. Prefix evidence and a complete-object
prompt check were added in `2d11cb6`; conditional verification was combined with
them in `fb0136f` and tightened in `efd83fe` to require all three references.
That combined candidate remains worktree experiment code, not the primary
checkout's prompt.

| Historical sample | Gemini via OpenRouter | Sante via OpenRouter |
|---|---|---|
| Strict baseline | PASS 102.9 s; PASS 64.1 s; FAIL 57.4 s | FAIL 32.6 s; PASS 42.1 s; FAIL 30.8 s |
| Discarded direct-plate prompt | FAIL 50.4 s; FAIL 55.1 s; FAIL 43.6 s | FAIL 71.8 s; FAIL 36.8 s; PASS 31.9 s |
| Combined candidate | PASS 51.1 s; PASS 48.2 s; PASS 85.2 s | FAIL 52.7 s; PASS 29.9 s; PASS 21.1 s |

The combined candidate reached 5/6 ordinary valid builds and used 3/3/6 HTTP
attempts for Gemini and 3/3/3 for Sante, including the cold handshake.

| Historical combined-candidate holdout | Outcome | Seconds / HTTP attempts |
|---|---|---|
| OpenRouter Gemini: Scourge | PASS, named specialization retained | 100.7 / 10 |
| OpenRouter Gemini: Ritualist | PASS, named specialization retained | 25.9 / 6 |
| Native Gemini: Scourge | FAIL, unclassified | 231.3 / 5 |
| Native Gemini: Ritualist | FAIL, transport | 314.9 / 7 |

An earlier quota error does not establish the cause of the later native failures.
The prefix-only experiment was incomplete: Gemini FAIL 290.0 s and PASS 95.9 s;
Sante PASS 30.5 s and FAIL 58.5 s. Its final pair was interrupted. Separate native
Gemini 2.5 Flash prefix-only runs passed ordinary and Reaper requests and failed
Harbinger; they do not validate the combined candidate. OpenAI and Anthropic live
behavior was not verified in this experiment.

These small samples span two days without randomized interleaving. Failed-attempt
duration is not time to a valid build, and the results establish no reliable
percentage speedup. They support testing exact prefix evidence and warn against
removing verification without checking validity and request completeness.

The historical run ended BLOCKED. Its pinned acceptance predicate required three
passing baseline runs per model, which the recorded baseline lacked; candidate
and native holdout failures also prevented acceptance. The old experiment's test
results and acceptance status are not verification of current code.

## Follow-up work

1. Define a fresh comparison on current code before running it. Permit baseline
   failures in the analysis, retain every attempt, and report validity, request
   completeness, fallback rate and time to valid builds separately.
2. Test exact gear-prefix evidence and complete-object prompting, then evaluate
   conditional investigation independently. Preserve grounding and measure
   referee quality as well as schema validity.
3. Measure cold and warm profile paths separately, along with first visible
   output, final answer, HTTP attempts and addon validation/referee time. Account
   for the current closing timeout and user cancellations.
4. Test reasoning effort independently with supported model settings. Investigate
   native transport failures separately from prompt quality.