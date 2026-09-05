# Objective wiring ledger

Plan for making the build objective *see* what actually makes a Guild Wars 2 build
good. Written to be attacked before any code changes.

## The evidence this rests on

**Measured in-game, 2026-09-04, release build, one PvE and one WvW-roam optimise:**

```
search_v2: 1500  evals /  3 generations in 0.28s  best_rank=[1,1,716475,1568515,0,0,0,0,0]
search_v2: 50000 evals / 64 generations in 8.08s  best_rank=[1,1,716475,1568515,0,0,0,0,0]
A/B: at 1500 evals (gen 3) rank was Some([1,1,716475,1568515,0,0,0,0,0]);
     at 50000 evals (gen 64) rank is      [1,1,716475,1568515,0,0,0,0,0]
```

WvW likewise identical: `[1,8,1,1,653682,13000,2600,91707,1078345]` at both budgets.

**33x the evaluations, 21x the generations, zero improvement in either mode.**

**What this proves, stated narrowly:** no candidate scored after evaluation 1500 on
this one continuation path produced a lexicographically greater retained rank. The
beam keeps elites (`search_v2.rs:508`), so best-rank is monotonic and cannot fall.

**What it does NOT prove — an earlier draft of this ledger over-concluded here.**
Four explanations remain open:
- **Local optimum.** Truncation keeps ten candidates (`search_v2.rs:542`); a valley
  deeper than one generation is uncrossable at any budget. That is an ALGORITHM
  limit, not an objective one.
- **The seed may already be optimal** for this objective. Fully consistent with an
  expressive scorer and an unchanged winner.
- **Lexicographic hiding.** In WvW, static `user_intent_score` precedes execution,
  tempo and sustain (`referee.rs:176`); a build with far better fight execution
  loses to a one-unit gain in an earlier million-scaled key. Per-axis caps at
  `scoring.rs:544` can saturate the leading key.
- **Tied-winner identity.** We logged the rank, not the build. A different build may
  have replaced the checkpoint winner with the same tuple.

A distinct-rank count does **not** separate these. See Phase 0.

## What the funnel actually showed (2026-09-04, later the same day)

```
generated=31977 admitted=1500 scored=1500 distinct_ranks=297
vs seed: beat=1056 tied=60 worse=384
```

**The objective was not flat.** 297 distinct ranks; 70% of scored neighbours beat
the seed. The defect was **admission**: `take(80)` over a round-robin interleave
of ~14 operators scored the same first ~6 options of every operator, every
generation, at every budget. Every rune/trait/prefix past the sixth was
unreachable — which is exactly why 50,000 evaluations bought nothing.

Shipped in 1.11.20 (`search_v2.rs`): per-operator quotas with the priority head
always admitted and a generation-rotated stride; patience stop measured in
rotation cycles (budget is a ceiling); exhaustive, clocked seed repair before the
beam; and `ViabilityReport.shortfall` so a failing gate can be climbed gradually
(a cleanse rate rising 0.5 -> 3.9 was invisible to `search_rank`). Same seed:
21 generations, 12 of them improving, +10% intent.

An independent review caught three defects in the first version of the sampler
before it shipped (small operators starved, repair unclocked, patience shorter
than one rotation). Phase 0 below is therefore **done**; the ordering of the
remaining phases stands, with the objective ("Flow Key") specified separately.

The budget was reverted to 1500 in `search_v2.rs`; 50_000 cost 8-9s for nothing.

## What the objective currently sees

Verified by reading the source, with anchors:

| Fact | Anchor | Status |
|---|---|---|
| **PvE** rank came only from `primary_combat`, a closed-form attribute formula | `referee.rs` `evaluate_validated_build` | **superseded 1.11.24** — every mode now ranks on `scoring::realized_axes` from a 60s flow simulation; the closed-form direction is the last tie-break key only |
| **PvE rank could not see a skill at all.** Measured 2026-09-04 on the live database (Necromancer, PvE, Roamer): the search handed back Ritualist with three empty utility slots and no elite; 36 different utilities tried in the first empty slot changed the rank 0 times out of 36. The holes came from the elite-spec swap emptying every Reaper shout and nothing being obliged to refill them | `search_v2.rs` `retarget_after_elite_swap`, `examples/necro_holes_check.rs` | **fixed 1.11.23 (refill) and 1.11.24 (objective)** — same run now: seed 0.579 -> 0.816, emptied bar 0.554 |
| **PvP DOES consume the rotation** — rotation is passed to viability evaluation and PvP enables rotation-derived gates, which are the FIRST rank keys | `referee.rs:719`, `:235`, `:132` | **corrected** — an earlier draft said PvE/PvP both ignore it |
| `DamageModifiers` are computed **twice** per candidate, not once | `referee.rs:681` and `engine.rs:1285` | **corrected** — also a free perf win |
| Conditional percent facts are **discarded**, not folded into a static multiplier: `percent_text_is_conditional(text) -> return`. Only the 90%-HP case survives, with a guessed 0.9 uptime | `combat.rs:755-762` | **corrected** — the defect is omission, not static evaluation |
| Combo fields contribute nothing **in the generic simulator only**. The WvW timeline already tracks a `combo_field`, resolves `SkillEffect::ComboField`, and counts `combo_activations`; live Might from it raises later strike damage | `simulator.rs:517-519` vs `wvw_timeline.rs:332`, `:589-593`, `:1118-1122`, `:1071` | **corrected** — WvW combo support is incomplete and hard-coded, but NOT zero |
| `EnemyDummy` is `{protection, stability, hp}`, but the WvW timeline separately carries conditions, disable state, boons, fields and damage events | `combat_model.rs:252-257` vs `wvw_timeline.rs:316` | **corrected** — Phase 3 should extend that model, not invent one |
| Consumables are **unmodelled in the optimizer path**. They appear in repo docs as a required dimension | `docs/optimizer-source-of-truth.md:143` | **corrected wording** — not "absent from the workspace" |
| Infusions: arithmetic exists off the candidate path, AND `ValidatedBuild` has no infusion representation at all | `stats.rs:319`, `:619`, `validation.rs:49` | **corrected** — needs schema, locks, legality, serialization; not "just reconnect it" |
| `ScenarioSpec.target_profile` is constructed and never read | `scenario.rs:13`, `:85`, `engine.rs:1268` | verified |
| Mobility is a boolean gate, not a scored quantity | `referee.rs:409-424` | verified |

For the closed-form `primary_combat` only, `stats` and `modifiers` vary while
`buffs` and `condition_weights` are constants. That is why swapping a rune, a
sigil or a combo-relevant trait frequently does not move the PvE score. It is NOT
true in general: skills and weapons change the rotation, and therefore change
PvP/WvW rank through the viability gates and the timeline.

## What it must see, and why

From the mechanics rulebook (`docs/combat-mechanics-reference.md`,
`data/formulas/combos.json`) and from 130 published expert builds:

1. **Target-state amplification.** Vulnerability is +1% to all strike AND condition
   damage per stack, cap 25. Torment does more to non-moving targets. Symbols hit
   harder against disabled enemies. All are read at the moment a skill lands.
2. **Combo fields and finishers.** 9 fields x 4 finishers = 36 outcomes, now
   tabulated. Published builds treat them as load-bearing ("a water field which you
   can blast and leap finisher in for extra sustain"). Currently score zero.
3. **Ordering.** Cleansing is first-in-last-out, so applying damaging conditions
   before cover conditions makes them survive cleanses. "The last Forge skill used
   decides what passive you keep." Value that exists only in a sequence.
4. **Self-generated boon loops feeding gear.** Fury and Resolution uptime from the
   rotation is why a published build can spend gear on Vitality instead of
   Precision. The pipeline picks gear *before* skills.
5. **Consumables and infusions.** Real power, entirely unmodelled.
6. **Conditions split into damaging and control, and the gate must too.** The
   product owner's framing: damaging conditions (bleed, burn, torment, poison,
   confusion) are a cleanse-*rate* problem; control conditions (chill, weakness,
   slow, immobilize, blind, cripple) are what actually end fights, and they are
   countered by cleanse **or Resistance** (which ignores exactly the
   non-damaging set) **or a stunbreak**. Shipped 1.11.22: the CleanseRate gate
   now credits sigil/rune/relic/trait cleanses (it counted only bar skills; a
   Sigil of Cleansing at ~2.2/20s was worth zero) and lowers its requirement by
   self-Resistance uptime, capped at 75%. Still owed: a real split — damaging
   cleanse rate as one gate, control-condition *coverage* (cleanse | Resistance
   | stunbreak, per profession mechanism) as another — so the objective values
   what ends fights rather than one blended number.
7. **Skills must be visible to the PvE objective.** A PvE build with an empty
   bar scored the same as a full one (measured: 0 of 36 fillers moved the
   rank). 1.11.23 refilled the holes the elite-spec swap left. **1.11.24 wired
   the objective** (Phase 1, option (a), all modes): see the Phase 1 record
   below for what it measures and the numbers it was calibrated on.

## The plan

### Phase 0 — Diagnose before wiring (1 run)

Count **distinct ranks** among the evaluated candidates in one optimise. Three
outcomes, three different fixes:

- Few distinct ranks -> the score cannot distinguish builds. Objective work, as
  assumed here.
- Many distinct ranks, none beating the seed -> the seed is in a local optimum the
  beam cannot escape. That is an *algorithm* problem (truncation selection cannot
  cross a fitness valley) and this ledger is aimed at the wrong target.
- Few distinct ranks but high stat variance -> the caps in `score_with_weights` are
  saturating and hiding real differences.

**Nothing below should start until this number exists.** Cost: a few lines, one run.

### Phase 1 — Decide whether the rotation feeds the PvE/PvP objective

**Decided and shipped 1.11.24: option (a), all modes.** What it does:

- `engine::simulate_flow` runs every candidate for 60 s (`FLOW_WINDOW_MS`) on
  the scenario's dummy with no downstate. `SimParams::intent` carries the radar
  weights, and the scheduler ranks each cast by `intent_value`: every axis in
  "seconds at its realized norm" (a stun, a heal and a cleave are commensurable),
  weighted by the user. Without intent the scheduler is the old pure DPCT, so the
  gate simulation and the WvW timeline are untouched.
- The simulator now tracks healing and barrier (same conservative model as the
  WvW timeline), hard-CC disabled time (non-overlapping, blocked by Stability)
  plus soft control at half weight while present, time-averaged Might, and
  boon-equivalents.
- `scoring::realized_axes` turns that into six axis fractions against
  `REALIZED_*` norms calibrated on 2026-09-04 against synergy seeds for all nine
  professions on the live database (`examples/flow_calibration.rs`), re-measured after the review fixes:
  strike 45k, condition 8k, healing 400/s, boons 3.5, control 1.5. Sustain keeps the
  closed-form effective health and adds realized Protection uptime.
- `search_rank` PvE/PvP: `[viable, gates, realized capped, realized uncapped,
  closed-form stat direction, ...]`. WvW keeps its timeline keys; its intent and
  final direction keys are now realized too.
- A kit with nothing to simulate scores `realized_axes_no_rotation` (sustain
  only), never the old closed-form score, so every build is on one scale.
- Cost: the flow simulation was 2.6 ms per evaluation until buffs and conditions
  became slot-indexed and per-skill cast values were precomputed; now 0.47 ms.
  The Necromancer PvE search runs 42 generations in 18 s and stops by patience.

What it changed on the live database (Necromancer, PvE, Roamer, same seed):
rank 578998 -> 816088, and the same build with an emptied bar 553798. Ten of
thirty-six calibration seeds had carried a racial skill (Battle Roar, Shrapnel
Mine, Reaper of Grenth, Healing Seed); `gamedb::profession_skill_index` now
drops skills listing more than one profession, and the Guardian, Warrior and
Mesmer healer seeds heal for real.

Adversarial review before shipping (three lenses, every finding refuted by
two independent readers, 2026-09-05): fifteen confirmed and fixed in the same
release — duplicate utilities scored twice (`swap_utility_skills` and the
gate-repair variant now skip held skills; `prepare_validated_rotation` dedups);
zero-priority weapon skills pinned the weapon set (`should_weapon_swap` uses
the scheduler's priority); soft control followed Concentration instead of
Expertise; Stability stacks summed as parallel instances (buff uptime is now
presence per tick, one rule for every boon); every boon counted 1.0 (now
`boon_value`: damage boons, Protection and Stability 1.0, Aegis 0.75, comfort
boons 0.5, Swiftness 0.25); hard CC was still scheduled into a Stability dummy
(`StaticCast` zeroes it); the flow simulation ran for candidates the gates had
already sent to -1 (now only when viable); `boon_equivalents` summed in
HashMap order; racial skills named by an LLM stopped resolving
(`GameDb::skills_usable_by` keeps them resolvable, the search pools still
exclude them); Revenant elite swaps produced legend-less bars (now skipped,
see below). Left as documented limits: Protection is folded into sustain
additively, the same shape the closed-form sustain axis has always used; the
60 s window with all cooldowns ready at t=0 makes an elite's credit a step
function of its cooldown (a 59 s elite casts twice, a 61 s or a 180 s elite
once).

What it does not do, and is now load-bearing: the simulator's fidelity IS the
objective. Condition output is under-modelled (Engineer 20k, everyone else
3-8k), the WvW dummy's Stability blocks every hard CC so booned scenarios read
control from soft conditions only, and the norms are one set across modes (WvW
strike lands at 0.4-0.85 of the PvE norm). Phases 3-5 below are no longer
"would be nice": each one moves the ranking directly. Also owed: the search
does not swap a Revenant's elite specialization at all, because a legend
package cannot be rebuilt by the skill operators; carrying
`ValidatedBuild.legends` through `retarget_after_elite_swap` is the fix.

The original two options, kept for the record:

- **(a) Wire it in** for all modes. Large. Moves every existing score with no oracle
  to say the new answer is better.
- **(b) Adopt the discretize position explicitly**: PvE optimisation is an attribute
  model, the rotation panel is illustrative, rotation-dependent effects are a
  documented limitation.

Choosing silently is the only wrong answer, and silence is the current state.

### Phase 2 — Consumables and infusions

Promoted above the harder mechanics because they pay off immediately, in every
mode, with no fidelity risk: they feed the static attribute model that *is* the
PvE/PvP objective. Both are separable — pure stat and multiplier effects, no timing,
no ordering. **Do not add them to the coupled search space**; solve them as a cheap
inner argmax per candidate. Infusions need only reconnecting existing arithmetic.

### Phase 3 — Target-state modifiers at cast time

Extend the enemy model beyond `{protection, stability, hp}` to carry conditions and
stacks, and evaluate conditional damage modifiers against it when a skill resolves,
instead of folding them into a build-level constant.

### Phase 4 — Combo fields and finishers

The table exists (`data/formulas/combos.json`, wiki-sourced). Track active fields
with positions and expiry, resolve a finisher against the field under it, apply the
tabulated effect. Note the rules that constrain this: max 5 combatants per field,
leap takes the first field passed and produces nothing if interrupted, projectiles
may proc at 20%, and **output scales with the finisher user's stats**, so combos
couple back to gear.

### Phase 5 — Rotation state predicates

Give the priority list guards (`requires_buff`, `requires_target_condition`,
`is_finisher_for_field`) so "charge F5 before the burst" and "leap into the water
field" become expressible. This is what SimulationCraft does and it is far cheaper
than sequence search. Only escalate to a finite-horizon MDP if predicates prove
insufficient.

## Cleanse registry (2026-09-05)

Measured on the live database after 1.11.24 shipped: WvW, Roam, Roamer,
Necromancer. The seed (Death Magic / Spite / Reaper with "Suffer!") failed the
CleanseRate gate with `cleanse_count=0, rate=-0.0/20s`; seed repair spent 644
evaluations and the beam 24 generations without a single viable candidate,
and the search served the non-viable head. Cause: `text_describes_condition_cleanse`
knew remove / cleanse / cure. Necromancer cleanses by transferring ("Suffer!",
Deathly Swarm, Putrid Mark: fact "Conditions Transferred"), sending (Plague
Signet: "Conditions Sent"), consuming (Consume Conditions, Spectral Walk) and
converting (Well of Power: "Conditions Converted to Boons"). Every profession
has its own verbs, so the fix is a table, not a wider regex.

`data/cleanse_sources.json` (embedded, `data::cleanse_sources::registry()`):
385 sources — 280 skills, 77 traits, 28 sigils/relics — and 354 judged
non-cleanses. Built by ten cataloguers (one per profession, one for gear)
from the API cache facts with the wiki Condition/Boon pages as the
completeness check, each file then re-derived by an adversarial verifier; 20
agents, 3.7 M tokens. `examples/cleanse_registry_check.rs` audits every entry
against the cache (id, name, profession, specialization, slot) and lists
what the text heuristic still flags outside the table: 0 problems, 0 unknown
ids at build 205780.

Where it is read: `builder::enrich_with_cleanse` (registry, then
NormalizedEffects, then text), `synergy_pipeline::skill_cleanse_count`
(registry, then facts, then text), `referee::kit_cleanse_rate_from_gear`
(registry, then tooltip text). An id the table knows never reaches the text
heuristic, including the 354 non-cleanses. `gate_count()` is the self count,
0 for movement-only cleanses; `rate_per_20s()` keeps the gate's convention
(count x 20 / cooldown, one activation per 20 s when no cooldown is stated).
99 skills cleanse only with a trait equipped (`requires_trait`; Cleansing Ire,
Restorative Illusions, Blurred Inscriptions, Hardening Persistence, Stainless
Steel...) and count only when `prepare_validated_rotation` finds that trait
on the build; without build context (`skill_cleanse_count`) they count as
none.

Known limits, recorded rather than hidden: "all conditions" is stored as 99;
counts without a fact (Consume Conditions, Elixir of Bliss, Preservation) are
wiki-sourced and say so in `evidence`; pulse skills store duration x rate
(Well of Power 6, Spectral Walk 5, Weapon of Remedy 5/3); an enabler trait
(Cleansing Ire) carries its own conservative 1 per 20 s while the bursts it
unlocks carry the real counts, a mild double credit when both are present;
ally counts assume the caster stands in its own radius; pet skills and
consumables are not catalogued (the rotation does not simulate either).
Same-name id variants (PvP splits, underwater, legend variants) each have an
entry. The text heuristic stays as the safety net for ids a future patch
adds, now with every verb (transfer / send / consume / convert-into-boons /
purge) bound to a 48-character window around "condit" and a veto for
"boons ... into conditions".

Found by the probe's new cleanse trace (`examples/necro_holes_check.rs`
prints every skill the gate counts): the seed's Dagger/Dagger + Scepter/Dagger
carried Deathly Swarm three times. `add_weapon_skill_ids` took every skill the
API lists under a weapon regardless of hand, so a main-hand dagger brought
slots 4-5 as well; and the same skill on both sets was two independent
cooldowns. Now a one-hander contributes its hand's slots only (two-handers all
five, by the `TwoHand` flag), and `builder::merge_weapon_sets` keeps one
instance of a skill shared by both sets, usable on either (`weapon_set` 0).
The seed's cleanse rate fell from 14.0 to the honest 9.0 per 20 s; this also
removes the same triple credit from strike and condition damage.

## The oracle problem

Objective changes have no ground truth. A score that moves is not a score that
improved. Proposed validation:

- **Corpus calibration.** 130 published builds carry expert labels: 26 Meta, 24
  Great, 53 Good, 25 Average. A correct objective should rank Meta above Average.
  Measure that correlation before and after every phase. This is the closest thing
  to an oracle available.
- **Reproduction.** For a fixed profession/scale/role, does the optimiser reach the
  published build's skeleton, or something demonstrably better on the same objective?
- **Regression.** Search changes get a free test (hold objective, compare ranks).
  Objective changes do not — hence the corpus.

## Known risks

- **Goodhart.** Every phase adds something to optimise against. `search_rank` is
  lexicographic with viability first, which bounds the damage, but a wired-in combo
  term will be gamed by the search the moment it exists.
- **No oracle for PvE.** If Phase 1 chooses (a), every PvE score changes at once.
- **Cost.** Phases 3-5 make each evaluation dearer, on a path that already runs
  1500 evaluations per optimise. Multi-fidelity screening becomes mandatory, not
  optional.
- **Corpus over-fitting.** The 130 builds are one author's opinions, updated per
  patch. As a calibration set, good. As a target to imitate, it caps the optimiser
  at reproducing what a human already wrote — the opposite of the goal.
