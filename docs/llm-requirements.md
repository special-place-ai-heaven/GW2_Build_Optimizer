# LLM requirements and interaction contract

What the addon asks of a language model, how it talks to each provider, what
the provider must be able to do, and what breaks when one of those holds only
partly. Every number is measured in this tree or against the live APIs on
2026-09-07; the evidence line under each item says where.

Read this before touching `crates/optimizer/src/llm/`, `gemini.rs`,
`prompts.rs`, or `chat_flow.rs`.

## 1. The job, in one paragraph

A player types a request ("WvW roaming power build") into Choya. The addon
builds ONE user message containing the character sheet prompt, the player's
words, a *kitchen* (the equipped build, the deterministic optimizer's own
reference build with its referee verdict, and the complete profession
reference: every specialization, trait and slot skill), and a list of 19
tools. The model may call tools for several rounds, then must answer with
ONE JSON object (the *plate*). The addon parses the plate, validates every
name against the game database, ranks it with the referee, and shows it. If
the model never produces a plate, the optimizer's own reference build is
shown instead, labelled as such.

The run must end in a build. That is the only hard requirement on the
harness. Everything below exists to make that true on as many models as
possible.

## 2. What the model receives

| Item | Size | Evidence |
|---|---|---|
| Instructions (character, rules, discipline, JSON shape) | ~2,000 tokens | `prompts.rs::chat_refinement_prompt_with_tools` + `BUILD_DISCIPLINE` |
| Profession reference (9 specs, all traits) | 25.6 KB, ~6,400 tokens | `cargo run --example prompt_prefill_size -- Necromancer` |
| Slot skills with descriptions | 16.3 KB, ~4,100 tokens | same |
| Reference build + referee verdict | ~600 tokens | `chat_flow.rs` `reference_build` |
| 19 tool definitions with JSON-schema parameters | ~3,000 tokens | `gemini_tools.rs::tool_declarations` |
| Player message | ≤ 500 chars | `prompts.rs::sanitize_order` |
| **Whole first request** | **~16,000 tokens** | sum |
| Conversation ceiling before trimming | 100,000 tokens | `trim.rs::SAFE_PROMPT_BUDGET_TOKENS` |

So a model needs a **32k context minimum**; 64k is comfortable once tool
results (a `get_spec_traits` reply is ~700 tokens, a `get_skill_info` reply
~300) accumulate over eight rounds.

**Defect found 2026-09-07:** `prompts.rs::sanitize_build_summary` cut the
kitchen to 2,000 *characters* before it reached the prompt, and
`get_current_build` applied the same cut. The prompt told the model "the
PROFESSION REFERENCE is complete and already in front of you" while the model
saw its first 2,000 characters. Every run therefore started by refetching
specs and traits through tools, which is what pushes a run past a 5-per-minute
limit. Fixed in this branch (cap raised to 80,000 characters; the 100k-token
trimmer remains the real ceiling).

## 3. What the model must be able to do

| Capability | Required? | Without it |
|---|---|---|
| Chat completion, text output | yes | unusable; filtered out of the picker (`ModelInfo::usable`) |
| ≥ 32k context | yes | the first request is refused or silently truncated |
| Function calling (OpenAI `tools`, Gemini `functionDeclarations`, Anthropic `tools`) | preferred | handshake marks `ToolSupport::None`; run is one request with the whole kitchen, no lookups |
| `tool_choice: required` on the first turn | nice | a model that narrates instead of calling gets one nudge (`CONTINUE_TURN`) |
| JSON output on demand | yes | the plate is parsed from prose; failing that, one repair request asks for the plate again |
| `response_format: json_schema` | nice | `json_object` if listed, else nothing; the repair covers the gap |
| ≥ 8,192 output tokens on the closing request | yes | the plate is cut mid-object (`finish_reason: length`) |
| Reasoning that can be capped (`reasoning.effort`) | nice | a reasoning model may spend the whole output budget thinking and return `content: null` (measured on `cohere/north-mini-code:free`) |

Capabilities come from three sources, in this order of trust: the provider's
own catalog (`/models` `supported_parameters` on OpenRouter), models.dev
(`models_dev.rs`, refreshed daily), and the handshake probe (`profile.rs`),
which measures the model doing a two-line version of the job.

## 4. The conversation, turn by turn

`tool_loop.rs::run` drives every provider through a `TurnDriver`.

1. **Handshake** (once per model per 7 days, `profile.rs::probe`): one
   request with a `ping` tool and "call it, then reply with exactly this
   JSON". Two requests if the model calls the tool. Result: tools
   Native/Sloppy/None, JSON Strict/Wrapped/Prose, narrates yes/no.
   **Defect found 2026-09-07:** a probe that failed on transport (404 from a
   guardrail, 429 upstream) was saved as "no tools, prose" for seven days.
   Fixed: a probe that never got an answer is not persisted.
2. **Explore turns** (up to `max_turns`: 0 / 4 / 6 / 8 by profile): the model
   calls tools, the addon executes them locally (no network), results go back
   as tool messages. Duplicate calls are answered from a cache without a
   request. Argument errors go back as `{"error": ...}`. Gathering stops when
   `TOOL_PHASE_BUDGET` (150 s) would be exceeded by another round of the last
   round's length.
3. **Closing turn**: `CLOSING_TURN` ("stop calling tools, serve the plate"),
   no tools declared, `max_tokens` 8,192, timeout 150 s, `json_schema` where
   supported.
4. **Repair** (chat_flow, at most two more requests): prose instead of JSON →
   "serve that as the plate"; JSON with empty `specializations` on a build
   request → same.
5. **Second attempt** (chat_flow): a plate the referee refuses gets the reason
   appended and the whole loop runs once more.

### Request count per run

| Phase | Requests |
|---|---|
| Handshake (first run on a model) | 1–2 |
| Explore | 0–8 |
| Closing | 1 |
| Repairs | 0–2 |
| Second attempt | same again |
| **Typical successful run** | **5–9** |
| **Worst case** | **24** |

This table is the whole reason free tiers fail. See §6.

## 5. Provider wire formats

| Provider | Endpoint | Tools | Tool results | Structured output | Notes |
|---|---|---|---|---|---|
| Gemini | `generateContent` (streamed), `x-goog-api-key` | `tools[].functionDeclarations` | `functionResponse` parts, matched **by name** | none wired | Gemini 3 signs calls with `thoughtSignature`; it must be echoed back or the next turn is a 400. Carried since `b26729d`. No `maxOutputTokens` is sent. |
| OpenAI | Chat Completions, Bearer | `tools[]`, args as JSON string | `role: tool` + `tool_call_id` | `json_schema` | |
| Anthropic | Messages, `x-api-key` | content blocks | `tool_result` blocks | none | `max_tokens` mandatory; 529 retried |
| OpenRouter | Chat Completions, Bearer | as OpenAI | as OpenAI | `json_schema` if `structured_outputs` listed, else `json_object` if `response_format` listed | `provider.sort: throughput` on free models; `require_parameters` when tools are sent; upstream 429 can arrive **inside a 200** SSE body and is retried |

Timeouts: 420 s per request on tool turns (`CHAT_REQUEST_TIMEOUT`), 150 s on
the closing request, 20 s for catalog calls.

## 6. Rate limits, the actual failure

Measured limits, 2026-09-07:

| Provider / tier | Requests per minute | Requests per day | Source |
|---|---|---|---|
| Gemini free tier, `gemini-3.8-flash` | **5 per model** | **20 per model** | 429 bodies: `GenerateRequestsPerMinutePerProjectPerModel-FreeTier` `quotaValue: 5`, `retryDelay: 39s`; `GenerateRequestsPerDayPerProjectPerModel-FreeTier` `quotaValue: 20` (measured 21:05, key exhausted for the day). Resets midnight Pacific. |
| Gemini free tier, older Flash models | 10 | 250 | our tracker's assumptions, `gemini.rs::RateTracker` |
| OpenRouter `:free`, < $10 credits ever | 20 | 50 | openrouter.ai/docs/api-reference/limits |
| OpenRouter `:free`, ≥ $10 credits | 20 | 1,000 | same; this account is on this row (`GET /api/v1/key` → `is_free_tier: false`) |
| OpenRouter paid | provider's | provider's | |
| Anthropic / OpenAI | our own 50 / 60 | none enforced | `RPM_LIMIT` constants |

Put §4 against the first row: a run of 6–9 requests inside ~60 s on a 5-per-
minute model **always** hits a 429 on request six, and a 20-per-day cap is
**two Choya runs a day** on that key. Google's free tier is a demo tier for
this workload; the addon can pace to it but cannot stretch it. Google answers with the
exact wait. `gemini.rs::classify_status` treated every 429 as terminal, so
the run ended and the fallback build was shown. This is the "Google free does
not work" report, and it is a regression from 2026-09-05, when explore rounds
went from 3 to 8 (a 3-round run was 4 requests and fit).

### Required behaviour (implemented in this branch)

- A 429 whose body names a **per-minute** quota with a `retryDelay` ≤ 60 s is
  waited out (cancellable) and retried, counting against the same attempt
  budget. A per-day quota is terminal.
- The quota value in that body becomes the tracker's per-minute limit for that
  model and is persisted, so the next run paces itself instead of tripping.
- When the local tracker is at its limit and the window resets in ≤ 60 s, the
  request waits for the window instead of failing.
- OpenRouter's 429 body reaches the UI, so "temporarily rate-limited upstream"
  (their pool, not the player) reads differently from "rate limit exceeded"
  (the player's own cap).

## 7. OpenRouter free models, measured for this account (2026-09-07 20:20)

15 `:free` models list `tools`. One 200-token ping with a tool each:

| Model | Result |
|---|---|
| `inclusionai/ling-3.0-flash-sante:free` | tool call, 1.1 s |
| `inclusionai/ling-3.0-flash-fin:free` | tool call, 0.8 s |
| `dots-studio/dots-3-note-preview:free` | tool call, 2.4 s; but its endpoint rejects `tool_choice: required` with 404 "No endpoints found that support the provided 'tool_choice' value" although the catalog lists `tool_choice`. The forced first turn now falls back to a plain tool turn. |
| `cohere/north-mini-code:free` | tool call, 2.6 s (reasoning model; needs `reasoning.effort: low` or it thinks through its whole budget) |
| `google/gemma-4-31b-it:free`, `google/gemma-4-26b-a4b-it:free` | 429 "temporarily rate-limited upstream" (Google's shared pool) |
| `nvidia/nemotron-3-*:free` (3), `poolside/laguna-*:free` (2), `liquid/lfm-2.5-2.6b:free` | 404 "0 endpoints … Free model training violation (guardrail) … configurable at openrouter.ai/workspaces/default/guardrails". A **workspace guardrail**, separate from Settings > Privacy (which the player had already set to allow) |
| `thinkingmachines/inkling*:free` (2) | 403 "only available on agentic harnesses" |
| `minimax/minimax-m3:free` | 404 "This model is unavailable for free" (withdrawn; the paid slug remains) |
| `openrouter/free` | routes to whichever of the above is up; today it landed on north-mini-code and the closing request timed out at 150 s while it reasoned |

So "no free model works" was: one withdrawn, two throttled upstream, seven
excluded by the player's own data-policy choice, two harness-gated, and four
that work. The picker showed the blocked ones with no hint; `err.data_policy`
now names the setting.

Re-swept at 21:40 after the player allowed free-endpoint training in the
workspace guardrail: **10 of 15 answer a tool call** (`inclusionai/ling-3.0-flash-sante`,
`inclusionai/ling-3.0-flash-fin`, `dots-studio/dots-3-note-preview`,
`liquid/lfm-2.5-2.6b`, `nvidia/nemotron-3.5-lightning`, `poolside/laguna-xs-2.1`,
`cohere/north-mini-code`, `nvidia/nemotron-3-ultra-550b-a55b`,
`nvidia/nemotron-3-nano-omni-30b-a3b-reasoning`, all `:free`); two Gemma and one
Laguna 429 upstream; two Inkling 403; `nvidia/nemotron-3-super-120b-a12b:free`
"Service temporarily overloaded" inside a 200. The free landscape changes by the
hour; the picker cannot know, only the run can.

## 8. Failure taxonomy and the recovery each one has

| Symptom | Cause | Recovery |
|---|---|---|
| 429, per-minute quota | run faster than the tier | wait `retryDelay`, retry; learn the limit |
| 429, per-day quota | tier exhausted | stop; tell the player which quota |
| 429 "upstream" (OpenRouter) | provider pool busy | retry with backoff (2 attempts); then tell the player it is the pool |
| 404 guardrail / data policy | account setting | `err.data_policy` |
| 404 model unavailable for free | withdrawn | raw message shown; catalog refresh drops it |
| 400 "missing thought_signature" | Gemini 3 signature dropped | signature carried in `Part` |
| text-only turn that narrates a plan | model talks instead of calling | one `CONTINUE_TURN` nudge |
| prose instead of JSON | model ignores the shape | repair request |
| JSON with empty `specializations` on a build request | model "spoke" the build | repair request |
| `content: null`, `finish_reason: length` | reasoning ate the budget | `reasoning.effort: low` on free models; closing cap 8,192 |
| closing request > 150 s | slow or reasoning model | deadline → last text; else fallback reference build |
| duplicate tool calls | model loops | cached result + note; identical round twice ends gathering |
| bad tool arguments | model guesses | `{"error": …}` back to the model |
| any error after tools ran | anything | referee-checked optimizer build served, labelled |

## 9. Acceptance test

`cargo run -p gw2-optimizer --example choya_live -- <provider> <model> [more models]`

Runs the real contract (handshake → prompt with the real kitchen and all 19
tools → tool loop → parse → validate) against the configured keys in the
addon's `config.json` and prints one line per model:

Measured 2026-09-07 (prompt 52,219 chars, ~13k tokens, 20 tools):

```
PASS  inclusionai/ling-3.0-flash-sante:free   6 req   46.2s  Reaper [Augury of Death, Soul Eater, Reaper's Onslaught] | Blood Magic [...] | Soul Reaping [...]
PASS  cohere/north-mini-code:free              4 req  351.6s  Spite [...] | Soul Reaping [...] | Reaper [...]   (one 205 s tool round; a reasoning model)
FAIL  inclusionai/ling-3.0-flash-fin:free     11 req   79.8s  plate has 1 specialization, not 3 (model quality; in-game the referee refuses it and the second attempt runs)
FAIL  dots-studio/dots-3-note-preview:free     1 req    0.3s  404 tool_choice (fixed: forced turn falls back to a plain one; not re-run)
FAIL  openrouter/free                          4 req  248.2s  HTTP error: response body error (router picked a slow endpoint; the stream dropped)
FAIL  gemini-3.8-flash                         6 req  108.7s  429: per-minute quota waited out twice, then the 20-per-day quota
```

PASS means a plate with three specializations that validates against the
game database. This is the definition of "works" for a model. A green unit
suite says nothing about it; this does.

Run it after any change to the loop, the prompt, or a provider, on at least
one model per provider the player can reach.
