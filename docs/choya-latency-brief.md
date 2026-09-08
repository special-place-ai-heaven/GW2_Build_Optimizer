# Choya build latency: briefing for a second opinion

Written 2026-09-07 22:10 for review by another model. Everything below is
measured in this tree or against live APIs today; nothing is assumed. The
companion contract document is `docs/llm-requirements.md` in the same folder.

## 1. What the product does

A Guild Wars 2 in-game addon (Rust, Nexus plugin, ImGui) with a chat
assistant, "Choya". The player types a build request. The addon sends ONE
user message to a language model (Gemini, OpenAI, Anthropic, or any model via
OpenRouter), with 20 tools the model may call in rounds; the addon executes
tools locally (no network, milliseconds) and returns results; the model then
answers with one JSON object (the "plate"), which the addon validates against
the game database and shows.

Providers are behind a shared loop (`crates/optimizer/src/llm/tool_loop.rs`)
and per-provider drivers (`llm/openrouter.rs`, `llm/openai.rs`,
`llm/anthropic.rs`, `gemini.rs`). The chat caller is
`crates/addon/src/ui/main_view/chat_flow.rs`. Prompt text is in
`crates/optimizer/src/prompts.rs` (`chat_refinement_prompt_with_tools`,
`BUILD_DISCIPLINE`). Tools and the data handed to the model are in
`crates/optimizer/src/gemini_tools.rs`.

## 2. The complaint

Builds take too long or time out. Today's in-game runs on the player's
machine:

| Model | Route | Outcome | Time |
|---|---|---|---|
| `openrouter/free` (router) | OpenRouter | rounds 36 s, 48 s, 179 s, then closing timed out | ~7 min, no build |
| `inclusionai/ling-3.0-flash-sante:free` | OpenRouter | handshake, one 42 s round, plate | ~1 min, valid Ritualist build |
| `google/gemini-3.8-flash` (paid) | OpenRouter | 7 rounds in 87 s, then round 8 never answered | ~7 min, UI backstop fired, no build |

The player's position, which is correct: capping rounds or time only makes
runs fail faster. The goal is builds that arrive fast.

## 3. What the request looks like

| Item | Size |
|---|---|
| Instructions + discipline block | ~2k tokens |
| Profession reference (all specs, traits, slot skills, handed in the prompt since today) | ~10.5k tokens |
| Upgrade reference (top 12 runes, sigils, relics for the radar; added today, measuring now) | ~2–3k tokens (estimate) |
| 20 tool definitions | ~3k tokens |
| First request total | ~16–19k tokens |

Every tool round re-sends the whole conversation. Output on the closing turn
is ~600 tokens of JSON; `max_tokens` on that turn is 8,192 because reasoning
tokens count against it on OpenRouter.

## 4. Where the time goes (measured)

1. **Each lookup round is a full model turn**: read ~16k tokens, think, emit
   calls. Sante 5–15 s; Gemini 3.8 Flash 6–35 s; free-router endpoints
   80–180 s. Tool execution is ~0.
2. **The rounds are spent on things we already have in memory.** The Gemini
   run's rounds: `search_upgrades` ×3 + `calculate_stats`; `search_upgrades`
   ×2; `simulate_combat`; `get_skill_info` ×3; `get_skill_info` ×2;
   `score_build` ×2; `search_upgrades`. Before today, runs also started with
   `get_profession_info` and `get_spec_traits` ×3 because the profession
   reference was silently truncated to 2,000 characters (fixed today; cap now
   80,000).
3. **A single request can hang.** Round 8 on Gemini never returned; the
   lookup request deadline was 420 s. The UI backstop (400 s) fired first.
   A free endpoint took 179 s on one lookup.
4. **The closing request on a thinking model.** It was sent with no
   `reasoning.effort`, so Gemini deliberated at its default depth before
   writing JSON; 2.5 min observed. Now sent with the same named effort as the
   lookups ("medium"; "low" on free models).

## 5. Rate limits (also measured today; the other half of "free models don't work")

- Gemini free tier, `gemini-3.8-flash`: **5 requests/min and 20 requests/day
  per model**. A run is 4–9 requests. Fixed: the client learns the stated
  limit from the 429 body, paces, waits out `retryDelay`; per-day is terminal.
- OpenRouter free: 20/min; 50/day under $10 credits, 1000/day above. This
  account is above. Many `:free` models were blocked by a workspace
  guardrail (player has since allowed it); `minimax/minimax-m3:free` was
  withdrawn; Gemma free variants are throttled upstream for everyone.

## 6. What has been changed today (all local commits on branch `choya-work`, not pushed)

- Prompt: profession reference reaches the model whole; discipline block
  accepts the reference as evidence and tells the model to call tools only
  for what it lacks; a named specialization is binding.
- Loop: no forced first tool call; duplicate calls served from cache;
  identical round twice ends gathering; narration nudged once.
- Budget: `max_turns` = 0/4/6/8 by measured model speed, 2 when the client
  says requests are scarce (free OpenRouter, Gemini on a 5/min quota).
- Deadlines: lookup request 120 s (was 420); free-model lookup 90 s; closing
  120 s... (closing constant is `CLOSING_REQUEST_TIMEOUT`, currently 150 s);
  UI backstop 400 s.
- Upgrade reference in the prompt. Measured right after (prompt 65,516 chars,
  ~16.4k tokens):

  ```
  google/gemini-3.8-flash (paid, OpenRouter): 4 rounds, 7 wire, 86.9 s, PASS
      rounds: calculate_stats x2 + search_upgrades x2; calculate_stats + search_upgrades;
              get_skill_info x4; simulate_combat
  inclusionai/ling-3.0-flash-sante:free:      2 rounds, 5 wire, 75.4 s, PASS
  inclusionai/ling-3.0-flash-sante:free:      body error from the endpoint at 99.8 s, FAIL
  ```

  Note the model still calls `search_upgrades` and `calculate_stats` with
  the rankings in front of it. That is question 1 below.
- Failure recovery: 429 pacing, prose→plate repair, empty-plate repair,
  fallback to the deterministic optimizer's own build with a referee note.

Acceptance harness: `cargo run -p gw2-optimizer --example choya_live --
<provider> <model...>` runs the real contract against the configured keys and
prints PASS/FAIL with rounds, wire requests and seconds.

Measured with the profession reference only (before the upgrade reference):

```
baseline (16cf4aa), Sante, 4 runs:  6/8/3/10 requests, 46/170/45/199 s, 4 PASS
after loop+prompt changes, Sante:   5/5/5/4/4 requests, 86/53/30/28/36 s, 5 PASS
dots-3-note-preview:free:           5 req 90 s PASS; 4 req 96 s FAIL (one-spec plate)
nemotron-3.5-lightning:free:        upstream 504 after 490 s; body error after 110 s
cohere/north-mini-code:free:        4 req 352 s PASS (one 205 s round)
gemini-3.8-flash (free key):        429 per-minute waited out twice, then the 20/day cap
```

## 7. Questions for the reviewer

1. Is there a request shape that gets a correct plate in ONE model turn for a
   ~16k-token prompt that already contains every legal name and the ranked
   upgrades? What would you remove from the prompt or the tool list to make
   the model stop calling `simulate_combat` / `score_build` / `calculate_stats`
   before plating? (The discipline block still tells it to "verify before you
   commit".)
2. Is the closing turn better as a separate request (current) or should the
   loop ask for the plate in the same turn as the last lookups?
3. Should reasoning effort on lookup rounds be "low" everywhere, with
   "medium" only on the closing turn, or the reverse?
4. Streaming: the addon streams responses but shows only round progress.
   Is there a way to show the plate as it is written so a 40 s closing turn
   does not read as a hang?
5. Anything in §4 you think is misattributed.

Constraints: no new dependencies; Rust, blocking reqwest, background thread
per request; every provider must keep working; the run must always end in a
build (the deterministic fallback exists for that).
