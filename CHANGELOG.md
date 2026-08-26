# Changelog

All notable changes to GW2 Build Optimizer are documented here.

## 1.6.2 - 2026-08-26

Choya's replies can no longer stall the frame loop. The v1.6.1 streaming fix made model answers arrive — and much richer ones, since reasoning models finally get to finish thinking — but the moment a reply landed, the background thread ran the whole serving pass (stat attachment, build-code encoding, rotation simulation, chip building) while holding the shared state mutex. ImGui draws only on the render thread, and the render callback shares that mutex, so a slow serving pass read to Windows as the entire game not responding. Every step of the serving pass now runs without the lock: the state mutex is taken once for a microsecond read (live game DB and game mode), released for the heavy work, and taken again only to append the finished reply and suggestion. A Clear pressed while a reply is still being prepared now correctly drops the result instead of resurrecting it.

The Gemini, OpenAI, and Anthropic providers are unchanged in this release.

## 1.6.1 - 2026-08-26

Choya stopped going silent. Every OpenRouter request — chat, Optimize, and Improve — now streams its answer instead of waiting minutes for a single buffered payload, reasoning models get a dedicated thinking budget that cannot starve the actual reply, and transient gateway failures retry with backoff instead of killing the turn. If a model appeared to "stop responding entirely" — the request eventually failing with *Request timed out. Try a larger/faster model.* — that was this bug, and it is fixed.

### Why requests timed out

- The OpenRouter client sent one non-streaming `POST` per request and enforced a hard 180-second wall clock. Nothing arrived until the model finished the entire generation, so the connection sat completely idle while the model worked.
- Reasoning models such as `z-ai/glm-5.3-flash` spend minutes on hidden thinking before their first output byte — measured at 220 seconds on a real request. OpenRouter's own gateway also aborts non-streaming requests whose provider does not answer in time, returning 408/504. Either way the request died as a false timeout on exactly the models that think the most, while quick non-reasoning models kept working — which is why only *some* models broke.

### Streamed replies

- All chat completions now use `stream: true`. OpenRouter interleaves `: OPENROUTER PROCESSING` keep-alive comments so the connection never idles, and the first bytes land in seconds (2.4 s measured, down from 220 s to anything at all).
- The SSE parser follows OpenRouter's streaming contract: keep-alive comments and blank lines are skipped, content and tool-call deltas accumulate (parallel calls merged by index, fragmented JSON arguments stitched back together), `[DONE]` ends the stream, and the usage-bearing final chunk is tolerated.
- Mid-stream failures arrive inside a 200 response, so they are now recognized in-band: a top-level `error` payload or `finish_reason: "error"` maps to the same typed messages as before (rate limited, billing, timeout, overloaded) instead of surfacing as a confusing empty reply.

### Answers no longer starved by thinking

- Reasoning models share one completion budget between hidden thinking and the visible answer. The old flat `max_tokens: 8192` let thinking consume the entire budget and return an empty message with `finish_reason: length` — reproduced live with 0 characters of content.
- Requests now cap hidden reasoning at 8,192 tokens (`reasoning.max_tokens`) inside a 16,384-token completion ceiling, so a long thinking phase leaves room for the build JSON and its explanation. Providers without thinking support ignore the parameter.

### Smarter retries and timeouts

- Gateway timeouts are now retryable: 408, 504, and 529 join the existing 500/502/503 retry list. Previously a single OpenRouter 408 killed the turn immediately with no retry at all.
- `Retry-After` is honored when OpenRouter sends one (capped at 60 seconds) instead of always guessing 5/10-second backoff.
- The connection budget grew from a 180-second whole-request kill to 900 seconds total with a 15-second connect timeout — a stalled request still fails, but a model that legitimately thinks for minutes now finishes.

### Tool-call routing

- Any request that carries function-calling tools now sets `provider.require_parameters: true`, so OpenRouter only routes to endpoints that implement tools natively — never to one that would fake them through a prompt template and return unparseable pseudo tool calls.

### Tests

- Five new unit tests cover the SSE reader: keep-alive skipping, content accumulation, fragmented parallel tool-call merging, mid-stream error mapping, and empty-stream finish-reason reporting; the existing tool-call round-trip test now exercises the streaming path.
- OpenRouter joined the live provider suite (`test_openrouter_validate_and_generate`): key validation, streamed generation, response caching, and a real streamed tool loop run against the production API. Full workspace suite: 785 passing.
- The Gemini, OpenAI, and Anthropic providers are unchanged in this release.

## 1.6.0 - 2026-08-25

New About tab. What's new shows the release notes for the last five versions in game. Message developer is a short guided form for a bug, a wrong build, a suggestion, a question, or a fistbump for Choya; each message shows its status (received, read, answered, closed) and the reply inline, refreshed on tab open and every five minutes, and the About pill pulses when an answer lands. Failed sends are kept locally with Resend, and nothing typed is lost. Ko-fi link in the header and on the first form step. Privacy: messages carry the category, choices, text, addon version, game build, language, mode/scale/role, profession and elite spec, and the AI provider name; a contact line, the account name, and a slim copy of the last optimize result are opt-in; API keys and character names are never sent.

## 1.5.3 - 2026-08-24

The rotation scheduler now values Fury by game mode. It priced every Fury application at the PvE +25% crit bonus, so Fury-granting skills were overvalued by a quarter when ordering PvP and WvW rotations; they now use the +20% those modes actually grant. A dropped  attribute in the i18n suite is restored, so named-placeholder substitution is verified again.

## 1.5.2 - 2026-08-24

A locked specialization now returns the best lock-respecting candidate even when no build passes every viability gate. The result stays on the locked spec and is marked provisional with the failed requirements instead of erroring out or swapping to another elite.

## 1.5.1 - 2026-08-23

Empty `gear_groups` now inherit the build prefix for armor, trinkets, and weapons, and every sheet path counts only the active land weapon set. Legacy saves with blank groups fill from `stat_prefix` on load. Empty inherited Strong and explicit all-Strong both lock at Power 2556 / Precision 2186 for the Ranger axe/axe fixture.

## 1.5.0 - 2026-08-23

This release moves WvW optimization away from isolated attribute totals and toward a mode-aware, two-sided exchange timeline. It also carries forward the v1.4.18 character-sheet corrections: passive attributes remain values a player can reproduce in Guild Wars 2, while temporary effects, modeled output, and incomplete mechanics are shown separately and labeled according to data quality.

### Reproducible character-sheet values

- The primary Stats comparison remains grounded in the level-80 Hero-panel attributes, profession health tier, armor-weight defense, equipped gear, active land weapon set, runes, and permanent sourced adjustments.
- Tooltip coefficients such as barrier amounts, direct heals, and life-siphon values are rejected by the shared permanent-stat classifier instead of inflating Power or Healing Power.
- Character-sheet calculation, optimizer scoring, synergy extraction, and model-facing build data use the same classification rules so a rejected tooltip amount cannot continue influencing search through another parser.
- PvE and competitive variants of duplicated attribute and percentage facts are resolved by the selected game mode instead of being stacked together.
- Synthetic evaluation values remain separate from visible attributes. The optimizer does not present internal scoring aids as numbers a player should expect to see in the Hero panel.

### Mode-aware balance and timing data

- Added an active 2026-07-15 balance manifest and patch ledger with explicit PvE, PvP, and WvW override files. The earlier 2026-01-13 manifest is retained as the inherited, superseded baseline.
- Rotation construction now receives the selected game mode. Activation time, recharge, resource cost, status duration, and combo-field duration can therefore resolve to different sourced values in PvE, PvP, and WvW.
- The first exact timing slice covers Black Powder, Heartseeker, and Steal, including competitive initiative and coefficient splits where the published values differ.
- Unsupported or ambiguous facts are not silently promoted to exact mechanics. They remain omitted from exact calculations or are surfaced as `Provisional` while source coverage is expanded.

### WvW exchange timeline

- WvW candidates are evaluated on a two-sided timeline with committed casts, incoming actions, interrupts, control, boon removal, defensive layers, recovery, and an exit instead of being ranked primarily by average dummy output.
- Secured sequences may be created by one applicable layer: control ownership, timed Stability, evade, block, invulnerability, or stealth. The evaluator does not require control and defensive cover simultaneously.
- Aegis and Blind are charge-based safeguards. Their mere presence does not create a multi-second protected interval; they preserve only the action or event they actually answer.
- Sequence completion now measures whether meaningful actions execute without interruption during secured slices. A generic mobility label no longer invents timed evade or stealth coverage.
- Enemy actions respect control state, and pending player actions can be interrupted. This makes ordering, short overlaps, and recovery materially affect the report.
- The WvW report exposes sequence completion, protected pressure, target threshold progress, tempo, sustain margin, exit availability, resource legality, and unmodeled-effect counts for ranking and diagnostics.

### Profession mechanics and resource legality

- Profession/F-slot skills can participate in the candidate and timeline instead of limiting evaluation to heal, utility, elite, and weapon actions.
- Added a bounded resource ledger for Initiative, Adrenaline, Energy, Illusions, and Blades. Higher-priority actions are skipped when their known cost cannot be paid, costs are spent at cast start, landed-hit gains respect caps, and over-cap gains are discarded.
- Resource accounting is intentionally a legality guard rather than a complete profession simulator. Attunements, legends, shroud, heat, life force, pets, kits, and other full state machines are outside this release.
- Weapon swaps use the candidate's actual weapon sets and sigils in search identity, while the passive character sheet continues to count only the active land set.

### Search, weighting, and grouped equipment

- WvW search ranking now reads the exchange report instead of leading with legacy dummy DPS.
- Role selection and fine-tune weights are propagated into candidate ranking so Condition, Control, Sustain, support, and direct-pressure preferences can influence which legal build is retained.
- Candidate identity includes weapon sets, sigils, elite specialization, and profession actions, preventing materially different loadouts from being discarded as duplicates.
- Mixed equipment is represented in grouped armor, trinket, and weapon prefixes. Search reachability was corrected so user-weighted alternatives can enter and survive the beam instead of repeatedly collapsing toward one direct-pressure prefix.
- Gate-repair neighbors remain available for candidates missing required defenses or utility, but passing viability does not override the user's selected role and weights.

### Corrected interaction rules

- Stability on the target prevents applicable control until it is removed; ordered removal can therefore change whether the following control action lands.
- Blind and Aegis no longer answer condition application, and their treatment is separated from evade and invulnerability when evaluating incoming actions.
- Resistance and condition application use their own timeline rules rather than borrowing strike-avoidance behavior.
- Incoming condition ticks use the shared condition-damage calculation instead of a max-health percentage stub.
- Might uses the mode-aware boon table rather than a fixed invented attribute increase.
- Combo fields carry explicit durations. Smoke, water, light, and fire finishers use the supported result for their finisher type; unsupported combinations are marked unmodeled instead of receiving a convenient fallback effect.
- Stealth is timed cover for relevant execution windows, not blanket immunity, and a mobility classification alone no longer grants a hardcoded duration.

### UI, reproducibility, and data quality

- Optimized gear display, comparison rows, save/load presentation, and generated build details preserve grouped-prefix and active-set choices so the shown result can be reconstructed instead of appearing as an unexplained aggregate.
- Mode-aware rotation previews use the visible candidate stats and selected mode rather than silently falling back to a generic PvE simulation.
- Build reports distinguish verified source-backed facts from provisional modeled behavior and count unmodeled sources for diagnosis.
- Exact passive attributes, simulated rotation output, boons, conditions, control, cleanses, and defensive utility remain separate concepts in the UI.

### Runtime and cancellation

- Long optimization work now observes cancellation and runtime limits throughout candidate generation and evaluation instead of only between coarse phases.
- Mode-aware evaluation is reused by search and addon previews, reducing disagreement between the candidate that was ranked and the result shown to the player.

### Validation and current boundaries

- Regression coverage includes the v1.4.18 Ranger character-sheet values, tooltip-stat rejection, competitive percentage classification, mode-isolated timing overrides, charge-versus-duration cover, interruption, ordered removal and control, resource paywalls, grouped-gear search identity, and user-weight reachability.
- The active data slice is not a claim that every Guild Wars 2 trait, skill, rune, sigil, relic, or profession mechanic is now exact. Exact competitive coverage is being expanded incrementally from authoritative sources.
- Timings and effects without a supported mode-specific fact remain `Provisional` or unmodeled. The optimizer will prefer an explicit gap over inventing a duration, coefficient, or interaction.
- Rotation results are deterministic model output, not a live combat record. Player positioning, facing, movement, opponent decisions, and profession state machines not listed above still require further modeling.
- Conditional damage thresholds are represented in the sourced balance layer, but complete conditional execution coverage continues to be expanded. A stored source value is not treated as active unless the timeline can justify its condition.

## 1.4.18 - 2026-08-23

This release replaces several optimistic stat assumptions with sourced, mode-aware character-sheet rules. Its first priority is trust: values shown as attributes should be values a player can reproduce in Guild Wars 2, while modeled rotation output is identified separately.

### Player-visible stat presentation

- The Stats pane now leads with the nine level-80 Hero-panel attributes: Power, Precision, Toughness, Vitality, Condition Damage, Expertise, Concentration, Ferocity, and Healing Power.
- Health and Armor remain visible derived Hero-panel values.
- Removed synthetic `Effective Power`, `Effective HP`, and `Healing Index` from the primary comparison. Those internal scoring aids are not character-sheet attributes and should not look like in-game values.
- Removed the synthetic three-scenario damage table from the primary attribute comparison. Rotation output remains labeled as a simulation rather than a live combat record.
- Boons, conditions, cleanses, stability, skill use, and viability remain separated from passive attributes so temporary effects are not presented as permanent gear stats.

### Permanent attribute corrections

- Tooltip coefficients no longer become permanent attributes. This fixes values such as barrier amounts, direct healing amounts, life-siphon damage, and life-siphon healing being added to Power or Healing Power.
- The same permanent-stat rule is shared by character-sheet calculation, synergy extraction, and the optimizer's LLM tool data. A tooltip amount can no longer disappear from the UI while still biasing candidate selection through another parser.
- Conditional named adjustments such as `Additional Power` are excluded from the unbuffed panel. They belong to timed activation modeling.
- Trait `BuffConversion` facts remain supported. For example, Wellspring's Power-to-Healing-Power conversion still changes the visible passive result.

### Game-mode corrections

- Known duplicated `AttributeAdjust` rows are resolved by trait, target attribute, and selected game mode rather than being summed.
- Lingering Magic now contributes 240 Concentration in PvE and 120 in PvP/WvW, instead of adding both API rows.
- Known conditional or pet-only duplicated rows are excluded from the standing player panel.
- Unknown duplicated rows are omitted instead of guessed. Missing data is safer than a confidently inflated attribute.
- Percentage tooltip facts now share one classifier between combat and synergy parsing.
- Tooltip-only 100% critical-chance facts, recharge reductions, incoming-damage reductions, and other defensive percentages no longer become outgoing damage multipliers.
- Two-value percentage pairs within one trait collapse to one mode value instead of stacking simultaneously. Conditional timing remains provisional and is handled separately from the passive panel.

### Equipment corrections

- Only the active land weapon set's two sigils contribute to the standing stat/modifier calculation. The second set is reserved for weapon-swap timeline handling.
- Rune tier bonuses continue to be summed from all six equipped tiers.
- Level-80 base attributes remain 1,000 for Power, Precision, Toughness, and Vitality, and 0 for the five secondary attributes.

### Profession and armor baselines

The optimizer uses separate sourced profession health and ascended-armor defense values:

| Baseline | Value | Professions |
| --- | ---: | --- |
| High base health | 9,212 | Warrior, Necromancer |
| Medium base health | 5,922 | Revenant, Engineer, Ranger, Mesmer |
| Low base health | 1,645 | Guardian, Thief, Elementalist |
| Heavy armor defense | 1,271 | Warrior, Guardian, Revenant |
| Medium armor defense | 1,118 | Engineer, Ranger, Thief |
| Light armor defense | 967 | Elementalist, Mesmer, Necromancer |

Visible Health is `profession base health + 10 × Vitality`. Visible Armor is `armor-set defense + Toughness`. Armor weight does not change the attribute budget of an otherwise identical armor prefix.

Sources: [Attribute](https://wiki.guildwars2.com/wiki/Attribute), [Health](https://wiki.guildwars2.com/wiki/Health), and [Armor](https://wiki.guildwars2.com/wiki/Armor).

### Reproduced failure and corrected result

The original Ranger case reached 7,156 Power and 13,696 Healing Power because effect coefficients were treated as passive stats. The exact offending tooltip amounts were removed from every permanent-stat consumer.

The cached WvW reproduction now resolves the tested optimized candidate to:

| Attribute | Value | Exact source |
| --- | ---: | --- |
| Power | 3,058 | level-80 base + Strong ascended gear + six Rune of Infiltration tiers |
| Precision | 2,544 | level-80 base + Strong ascended gear + six Rune of Infiltration tiers |
| Toughness | 1,000 | level-80 base |
| Vitality | 1,240 | level-80 base + Natural Fortitude |
| Concentration | 120 | Lingering Magic, WvW value |
| Healing Power | 214 | Wellspring: 7% of 3,058 Power, rounded for display |

The Rune of Infiltration contribution is exactly +175 Power and +225 Precision from its six API bonus lines.

### Validation

- All optimizer tests pass: 735 passed, 1 network test ignored.
- All 39 math permutation tests pass.
- All 29 objective-profile integration tests pass.
- Addon compilation passes.
- Clippy completes successfully; existing repository warnings remain non-blocking.

### Known boundaries

- Rotation output is still a model, not a live combat log. It remains labeled as simulated.
- Conditional trait and upgrade effects require timeline activation rules before their contribution can be called exact.
- A shield's separate defense rating is not yet added to the optimized Hero-panel Armor value. The armor-weight baseline itself is exact.
- Food, utility consumables, temporary boons, and triggered upgrade effects are intentionally not folded into the unbuffed attribute table.
