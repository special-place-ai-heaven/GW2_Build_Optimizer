# Choya latency: source review and bounded experiment

2026-09-07/08. Scope: `choya-work` directory, baseline `1612f96`. Experiment records are in
`autoresearch/loop-260907-2209/`. Outcome: BLOCKED, not converged. The combined
candidate remains committed locally for review; it is not an accepted latency win.

## Answers to the brief

**Measured correction to the initial hypothesis:** removing mandatory simulation
and telling the model to plate directly reduced request counts but lowered strict
validity from 3/6 baseline builds to 1/6. Commit `3ac6d64` was reverted by
`34ba85e`. A diagnostic isolated a missing authority source: the local validator
rejected `Marauder's` as an unknown gear prefix. Profession names and upgrade
rankings do not supply gear-prefix names. The second experiment (`2d11cb6`) adds
exact prefix names from `db.itemstats` and a complete-object check, while retaining
the original verification discipline. Do not adopt the first experiment merely
because its failed replies arrived sooner. The third candidate combines those
prefix references with conditional verification (`fb0136f`, tightened by
`efd83fe` to require profession, upgrade and stat-prefix references). It produced
5/6 valid ordinary builds versus baseline 3/6, and passed both OpenRouter holdouts.
One Sante build still omitted required slots; both native Gemini holdouts failed.

1. **Make a grounded plate the default action.** The original `BUILD_DISCIPLINE`
   explicitly ordered `find_synergies`, then simulation. Other bullets required
   skill/trait detail and condition-source calls. Prefilling names does not satisfy
   those numerical investigations. The first experiment makes additional tool
   work conditional on decisive missing evidence or an explicit numerical request.
   The combined candidate retains this only when all three references are present.
   All tools remain available, including when references are absent. Names must
   still come from current evidence, trait columns must be legal, and unsupported
   numerical claims must be omitted. One turn is an empirical objective, not a
   guarantee. A later controlled experiment could omit evaluation tools from the
   ordinary plate request, while retaining them for explicit analysis requests.

2. **Do not add a closing request after a finished answer.** The implementation
   already behaves this way: `tool_loop::run` returns non-narration text from
   Explore immediately. Closing is used only when gathering ends without an
   answer. If the model needs the last lookup results, it cannot base its plate on
   those results until a subsequent model turn. That next turn should be able to
   finish the plate; no extra ceremonial closing request is necessary. Do not
   accept text emitted beside tool calls as if it already incorporated their
   results.

3. **Reasoning effort needs its own experiment.** Start by holding it constant
   while changing the prompt. Lookup-only factual selection and formatting are
   candidates for low effort, but a lookup turn can also become the final answer.
   Reserve extra reasoning for demonstrated difficult selections rather than
   blindly assigning it to a phase. Keep capability checks: supported levels vary
   by model. Hiding reasoning output does not disable reasoning. See
   [OpenRouter reasoning documentation](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens).

4. **Streaming can improve visible progress after content starts.** The shared
   OpenAI-compatible SSE reader currently accumulates the entire message before
   returning it; it exposes no content-delta callback. A bounded, throttled text
   callback through the driver and background/UI channel could show complete
   parsed fields as a provisional preview. Keep it separate from the validated,
   applicable build, replace it on repair, and clear it on cancellation/failure.
   JSON fragments are not complete JSON; use incremental structural parsing rather
   than regular expressions or reparsing arbitrary fragments as complete plates.
   Streaming cannot display plate content before the provider emits any content,
   so it will not remove a long reasoning or queue delay.

5. **Correct the attribution and the measurement.** The clock around a model turn
   combines queuing, transport, prompt processing, reasoning, output generation,
   and any retry/pacing in the client. The current measurements do not isolate
   these components. Resending the full conversation does not establish that the
   provider recomputes every prefix token without caching. Duplicate-call caching
   avoids local execution, not the HTTP turn that produced the duplicate call.
   Provider throughput routing also differs from minimizing time to first token;
   [OpenRouter documents separate latency/performance considerations](https://openrouter.ai/docs/guides/best-practices/latency-and-performance).

## Acceptance corrections

The old live harness forced a cold handshake each run, omitted the addon's
deterministic reference/referee path, and accepted three surviving specializations
without checking the rest of `ValidatedBuild.errors`. Its reported round count
mixes handshake estimates and progress callbacks and misses a direct final Explore
answer; use the HTTP-attempt counter for request counts.

The experiment retains the same cold-handshake timing and adds checks for all
validation errors and missing weapon sets, upgrades and gear prefixes. The
`CHOYA_REQUEST` environment variable permits fresh request scenarios without
editing the binary. Neither the old nor strengthened harness proves referee
quality or in-game acceptance. Deterministic fallback remains separate from model
success and must not be counted as a faster model answer.

## Measurement protocol

One exploratory run before tightening validation: Gemini 78.7 seconds / 8 wire
requests; Sante 32.3 seconds / 4 wire requests, both old-gate PASS. These are not
included in the stricter baseline sample.

Compare three stricter baseline runs and three candidate runs per model, all on
the same machine and cached data. Require all to pass and each model's median to
improve. Verify fresh named-specialization/condition requests separately. Keep
outages and failed builds visible; do not replace them with conveniently fast
successful retries. Unit tests protect local behavior, not model quality.

Baseline optimizer guard: 1112 passed, 0 failed, 4 ignored. Candidate validation
and final disposition are recorded in the experiment handoff.

| Durable sample | Gemini via OpenRouter | Sante via OpenRouter |
|---|---|---|
| Strict baseline | PASS 102.9 s; PASS 64.1 s; FAIL 57.4 s | FAIL 32.6 s; PASS 42.1 s; FAIL 30.8 s |
| Discarded direct-plate prompt | FAIL 50.4 s; FAIL 55.1 s; FAIL 43.6 s | FAIL 71.8 s; FAIL 36.8 s; PASS 31.9 s |
| Combined candidate | PASS 51.1 s; PASS 48.2 s; PASS 85.2 s | FAIL 52.7 s; PASS 29.9 s; PASS 21.1 s |

The combined candidate used 3/3/6 HTTP attempts for Gemini and 3/3/3 for Sante,
including the cold handshake. All-attempt medians were 51.1 and 29.9 seconds,
versus baseline 64.1 and 32.6. Failed-attempt times are not valid-build latency.
These small samples span two days, with provider variability and no randomized
interleaving; they do not establish a reliable percentage speedup.

The prefix-only candidate was interrupted after four completed attempts:
Gemini FAIL 290.0 s and PASS 95.9 s; Sante PASS 30.5 s and FAIL 58.5 s
(unknown sigil Night). Its final pair is incomplete, not silently excluded.

| Fresh request, combined candidate | Route | Outcome | Time / HTTP attempts |
|---|---|---|---|
| Scourge | OpenRouter Gemini 3.8 | PASS, named specialization retained | 100.7 s / 10 |
| Ritualist | OpenRouter Gemini 3.8 | PASS, named specialization retained | 25.9 s / 6 |
| Scourge | Native Gemini 3.8 | FAIL, unclassified | 231.3 s / 5 |
| Ritualist | Native Gemini 3.8 | FAIL, transport | 314.9 s / 7 |

Native Gemini initially hit a diagnosed daily quota limit. Later failures above
must not be attributed to that earlier quota without evidence. Native Gemini 2.5
Flash accepted the prefix-only candidate's ordinary request (16.3 s / 3 attempts)
and Reaper holdout (19.5 s / 4); its Harbinger holdout failed (9.4 s / 3).
Those runs do not validate the combined candidate on native Gemini. OpenAI and
Anthropic credentials were absent; their live behavior is unverified. Existing
OpenRouter credentials were used without copying or displaying their values.

## Disposition and remaining work

The combined prompt is a provisional local candidate, not a release acceptance.
Its optimizer guard passed: 1112 tests, 0 failures, 4 ignored. The example build,
strict Clippy for library/examples, and workspace formatting check passed. No
dependencies, provider drivers, deadlines or fallback behavior were changed.
No DLL release or in-game/referee acceptance was performed.

The pinned predicate remains PENDING (exit 1). It requires three passing baseline
runs per model, which the immutable baseline lacks, as well as passing candidate
and holdout runs. Further candidate edits cannot repair that historical condition.
The run stops BLOCKED with all failures preserved, rather than changing the gate
or substituting successful retries. A follow-up needs a prospectively defined
comparison that permits baseline failures, measures time to valid builds and
request completeness, and investigates native transport failures separately.
Low reasoning effort and incremental plate streaming remain unmeasured proposals.
