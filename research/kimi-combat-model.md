# GW2 Combat Model for Build-Optimizer Scoring — WvW/PvP, live patch

**Research date:** 2026-08-14 (all wiki/MetaBattle pages accessed 2026-08-14)
**Balance reference:** MetaBattle WvW sections are current for the **July 15, 2026** patch; some sources reference the July 20, 2026 WvW patch. (Source: "WvW — MetaBattle Guild Wars 2 Builds", https://metabattle.com/wiki/WvW, accessed 2026-08-14)
**Companion source in-repo:** `crates/gw2api/src/models/facts.rs` already mirrors every documented API fact type.

---

## Executive summary

1. Score **per (scale × task)**, never a single DPS number. Simulate: **roam** = 1 attacker vs 1 player dummy, **havoc** = 1–3 vs few with partial boons, **zerg** = attacker inside an allied blob vs a supported clump, plus one **commander-survival** dummy.
2. **Alpha window T: roam dive T = 2.0 s** (the prompt's ~2 s guess holds), **havoc T = 2–3 s**, **zerg strike spike T = 3 s**, **zerg condi ramp T = 6–8 s** (ramp kinds need a separate, longer clock — do not collapse).
3. **HP band: correct 15k–30k → ~12k–30k.** Naked level-80 totals are 11,645 / 15,922 / 19,212 by profession tier (low/mid/high); gear vitality adds roughly +0–10k on top. (§1)
4. **Mandatory score effects:** hard CC that stops dodge (stun/knockdown/launch/knockback/pull/fear/taunt, plus immobilize as the condition that also blocks dodging), **cover** (invuln/stealth/dodge frames/block/blind/stability), **strip/corrupt as a gate** for harasser and anti-zerg (Stability loses all stacks to one boon-removal), **cleanse**, and a **mobility/out gate** for roam. (§2, §4)
5. Daze does **not** stop dodges or movement — only cast-time skills. It is an **interrupt**, not a lock. All control effects interrupt on application, so interrupts are also *defensive counterplay* against the enemy's burst/heal casts. (§2)
6. Cover that actually blocks the alpha answer: true **invulnerability** (e.g. Distortion 1 s + 1 s/clone), **stealth** (targeting denial; breaks when the user deals damage), dodge frames (0.75 s), **block/Aegis** (single hit), **blind** (next hit misses). Stability/Aegis/Resistance are *boons* — all three are removable, so strip converts cover into lock. (§2)
7. Roam kit gate: self-sufficient boons/cleanse + **an out** = teleport/shadowstep, stealth, superspeed (+100% forward in combat, effect not boon), or leaps. No out ⇒ not roam, regardless of burst. (§4)
8. Zerg scoring must **assume allied Might/Fury/Protection/Stability**; stacking more Might is redundant. Unique jobs (Stability, strip/corrupt, cleanse, rez, heal) dominate. (§5)
9. API reality check: hard CC and cover are `Buff` facts keyed by `status` string; strips are `Number` facts with text `Boons Removed`; **boon→condition corrupt/convert is often NOT in facts at all** (Well of Corruption exposes zero corrupt facts) and must be parsed from `description`. **Cast/activation time is not exposed by the API** — it must come from the wiki or a hand table, and the alpha sim needs it. (§6)
10. Fastest falsifier: a quickness-gated or condi-ramp kit scored on a 2 s strike clock, and any "DPS that also CCs" mislabeled as disabler. (§8)

---

## 1. HP pools

Level-80 health = profession **base health** + **10 × Vitality**; every level-80 has 1,000 base Vitality (10,000 HP) before gear/traits. (Source: "Health — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/Health, accessed 2026-08-14)

| Tier | Professions | Base health | Naked total @80 |
|---|---|---:|---:|
| High | Warrior, Necromancer | 9,212 | **19,212** |
| Mid | Revenant, Engineer, Ranger, Mesmer | 5,922 | **15,922** |
| Low | Guardian, Thief, Elementalist | 1,645 | **11,645** |

(same source; per-level gain table rows)

**Gear on top:** Vitality from equipment adds 10 HP/point. Exact totals per stat combination: **unknown** (not fetched — wiki "Attribute combinations" page not pulled). [INFERENCE] From the base table + typical WvW gear (Marauder/Demolisher/Celestial/Soldier families seen on current MetaBattle builds), realistic live pools:

| Archetype | Realistic HP band |
|---|---|
| Glass low tier (Thief/Guardian/Ele burst) | ~12k–15k |
| Bruiser mid tier (Marauder-style power) | ~18k–23k |
| High tier with shroud mechanic (Necro/Reaper adds a second shroud HP bar — see note) | 19k–25k body + shroud |
| Support/commander (high-vit gear) | ~25k–30k |

**Verdict on the prompt's 15k–30k:** correct for mid/high tiers, **too high at the floor** — the low armor tier (Guardian, Thief, Elementalist) sits at 11.6k naked and ~12–15k in glass gear. Use **12k–30k**, and model dummies per tier, not one band. Necromancer is special: it combines the high HP tier with a second health pool (Death Shroud / Reaper Shroud), which is why it reads "tanky" despite no dodge-focused defenses. (Base HP: same Health page; shroud as separate bar is profession mechanic — shroud % scaling **unknown**, not fetched.)

Armor class does **not** set HP — profession does. Armor sets *Armor* (defense vs strike damage), a separate axis.

---

## 2. Lock vs cover

### 2.1 Hard control effects (disables)

All control effects **interrupt on application**. (Source: "Control effect — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/Control_effect, accessed 2026-08-14)

| Effect | Stops dodge? | Stops skills? | Stops movement? | Notes |
|---|---|---|---|---|
| **Stun** | **Yes** | Yes (cast-time) | Yes | Full lock. |
| **Knockdown** | Yes (actions) | Yes | Yes | Full lock, target grounded. |
| **Launch** | Yes | Yes | Yes | Full lock + displacement; can move downed bodies. |
| **Knockback** | Yes (during) | Yes (brief) | Yes | Displacement + brief disable after. |
| **Pull** | Yes (during) | Yes (brief) | Yes | Repositions target to caster/point + brief disable. |
| **Fear** | Yes | Yes | Forces flee | **Both control effect and condition** — removed by stun break *or* cleanse; suppressed by Resistance. |
| **Taunt** | Yes (forces auto-attack approach) | Only stun breaks usable | Forces approach | Same hybrid status as Fear. |
| **Daze** | **No** | Cast-time skills only | **No** | Interrupt, not a lock. Use to cut casts (heals, elites, stomps), not to hold a target for the dump. |
| **Float / Sink** | Yes | Yes | Yes | Underwater only. |
| **Immobilize** (condition, not a control effect) | **Yes** — immobilized characters cannot dodge | No | Yes | The cheapest "lock": a condition, so cleansed, not stun-broken; Stability does **not** stop it (see below). |

Dodge-stopping sources: stun explicitly; knockback (immediately after); fear; immobilize. (Sources: Control effect page above; "Dodge — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/Dodge, accessed 2026-08-14: "Some mechanics and control effects prevent characters from dodging, including being immobilized, immediately after being knocked back, or while under the effects of fear.")

**Durations are per-skill, not per-effect.** The wiki assigns no universal duration; typical player-facing daze/stun values are short (commonly ~0.25–2 s, set by each skill). (Source: Control effect page, "Duration is determined by the individual skill, trait, enemy ability, game mode, and modifiers.") Do not hardcode a duration table per CC type — read it per skill.

**Interrupts as counterplay (both directions):** because every control effect interrupts on application, an interrupt landed on the *attacker* during a cast-time burst (or on the defender's heal/stomp cast) prevents the alpha entirely. Daze is the dedicated interrupt tool (instant or near-instant casts, short recharge). Current build playbooks confirm this is live practice, e.g. the SA Rifle Deadeye usage notes describing utility used "in melee to disrupt enemy bursts or to interrupt key skills". (Source: "Build:Deadeye - SA Rifle Roamer — MetaBattle", https://www.metabattle.com/wiki/Build:Deadeye_-_SA_Rifle_Roamer, accessed 2026-08-14) Score implication: a burst whose key hit sits behind a long cast/channel is worth less vs players holding interrupt than its tooltip says.

### 2.2 Defensive answers — what each one actually prevents

| Defense | Type | Prevents | Does NOT prevent | Removable by |
|---|---|---|---|---|
| **Stability** | Boon (intensity stacks, max 25) | All 10 hard control effects | Soft conditions (Crippled, Immobile, Chilled, Slow); does not remove CC already on you | **Any boon strip/steal/corrupt voids ALL stacks at once**; otherwise 1 stack per incoming CC, max 1 stack stripped per 0.75 s |
| **Stun break** | Skill property (`StunBreak` fact) | Removes duration control effects incl. Fear/Taunt; usable while CC'd | Freeze, Signet of Humility transform | — |
| **Aegis** | Boon (duration stacks) | The **single next attack** (any one attack) | Multi-hit skills eat it on hit 1; conditions ticking | Boon removal; being hit |
| **Resistance** | Boon | **Nondamaging condition effects** — Immobile, Chill, Crippled, Slow, Weakness, Blind, Fear, Taunt (suppressed while active, resume after) | Hard CC (stun/daze/knockdown/…); **condition damage continues** (bleed/burn/poison ticks still hurt) | Boon removal |
| **Invulnerability / Distortion / Determined** | Effect (not a boon) | **All damage, all conditions, all control effects**; also pauses condi ticks | Fall damage, environmental traps | **Cannot be stripped** (not a boon) |
| **Stealth** | Effect | Targeting/detection; NPCs stop attacking | Ends when the user **deals damage**; breaking stealth applies **Revealed** (blocks re-stealth for several seconds). Timing exploit: an attack started before stealth ends and landing after expiry deals damage without applying Revealed | Reveal effects; dealing damage |
| **Dodge** | Innate | Evades attacks for **0.75 s**; moves 300 units; costs 50 of 100 endurance | CC that prevents dodging (stun, immobile, fear, post-knockback) | — |
| **Superspeed** | Effect (not a boon) | — | — | Cannot be stripped; +100% forward speed in combat, duration-stacks to 10 s max |
| **Blind** | Condition | Next single hit misses (100%) | Everything else | Cleanse; one hit |
| **Block** (skill property) / **Protection** | Skill frames / boon | Block: specific incoming strikes; Protection: −33% incoming damage | Unblockable-marked skills (`Unblockable` fact); condi damage ignores Protection's strike reduction | Protection is a boon → strippable |

Sources: "Stability", https://wiki.guildwars2.com/wiki/Stability (stack/0.75 s rule, strip-voids-all, soft-condi exemption); "Stun break", https://wiki.guildwars2.com/wiki/Stun_break; "Aegis", https://wiki.guildwars2.com/wiki/Aegis; "Resistance", https://wiki.guildwars2.com/wiki/Resistance (suppresses nondamaging condition effects — blind, chill, cripple, fear, immobile, slow, taunt, vulnerability, weakness — not hard CC; condi damage continues; Poison's healing-reduction and Necromancer Terror damage are explicitly NOT negated; corrupts into chill per version history); "Invulnerability", https://wiki.guildwars2.com/wiki/Invulnerability; "Stealth", https://wiki.guildwars2.com/wiki/Stealth; "Superspeed", https://wiki.guildwars2.com/wiki/Superspeed; "Protection", https://wiki.guildwars2.com/wiki/Protection (−33%, stacks duration); "Dodge", https://wiki.guildwars2.com/wiki/Dodge. All accessed 2026-08-14.

### 2.3 Soft CC and the alpha clock

Blinded / Chilled / Crippled / Immobile / Slow / Weakness are **"soft CC"** — conditions, not disables; Stability does not protect against them; cleanses and Resistance do. (Source: Control effect page, "Condition based non-disabling control effects".) Alpha relevance:

- **Immobile** — yes, alpha-relevant: prevents dodging, pins the target for the dump.
- **Blind** — yes: eats the first/biggest hit of the enemy's counter.
- **Chill** (−66% movement & recharge rate), **Crippled** (−50% move), **Weakness** (−50% endurance regen, 50% glancing) — attrition tools, not alpha clocks.
- **Slow** (animations 50% slower, incl. non-block/evade cast times) — stretches the *enemy's* alpha clock; anti-synergy with Quickness races.

### 2.4 Roam cover window — what actually gives "cannot be acted upon"

True brief cover while damage stays live:

- **Distortion** (Mesmer F4 shatter): invulnerable 1 s **+ 1 s per clone shattered**. (Source: "Distortion — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/Distortion, accessed 2026-08-14; API skill 10192 confirms a `Buff` fact `status: "Distortion"`, duration 1.)
- **Stealth**: untargetable while it lasts; the burst is delivered on the stealth attack or on the expiry-timing exploit above. Thief kits are built on this. (Stealth page; MetaBattle Deadeye build page.)
- **Invulnerability from skill effects** generally: immune to damage/conditions/CC; also granted automatically on entering downed state and on rally. (Invulnerability page.)
- **Dodge frames**: 0.75 s evade, 2 dodges from full endurance. (Dodge page.)
- **Aegis / block frames / blind**: partial cover — they delete one hit, not the window.
- **Stability**: cover against *counter-CC* during your own dump, nothing else.

---

## 3. Alpha window

**Is ~2 s grounded? Yes for roam/havoc single-target spikes; no as a universal constant.** Components:

- Defender reaction floor: dodge evade is **0.75 s**; a stun break + dodge needs the stun break (instant on most) + 0.75 s. So a **1–1.5 s hard CC ≈ one clean dump window** if the CC lands uncovered. (Dodge page; Stun break page.)
- Attacker cast budget: fast strikes cast in **¼ s** (Backstab: ¼ s activation; "Backstab — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/Backstab, accessed 2026-08-14); burst elites run to **1½ s** (Winds of Disenchantment: 1½ s cast, 90 s recharge; "Winds of Disenchantment — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/Winds_of_Disenchantment, accessed 2026-08-14).
- **Quickness** compresses the whole attacker clock: skills and actions activate **50% faster**. (Source: "Quickness — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/Quickness, accessed 2026-08-14.) A quickness-covered 2 s window delivers ≈ 3 s of nominal casts. Slow does the inverse to the defender.

**Recommended simulation windows 0–T per scale:**

| Scale | Kind | T | Why |
|---|---|---:|---|
| Roam | Strike spike dive | **2.0 s** | Lock (1–1.5 s CC or cover) + dump; if not dead, leave (no brawl). |
| Roam | Condi | **4–6 s** | Condi ticks are per-second; roam condi still needs the out, but kill pressure is ramped. |
| Havoc | Strike / hybrid | **2–3 s** | Partial friendly boons, some enemy peel; slightly longer contested window. |
| Zerg | Strike spike (the "bomb") | **3.0 s** | Squad-converged damage on the tag/clump; defender-side healing/stab during the window. |
| Zerg | Condi ramp | **6–8 s** | Pulse skills run 1 s intervals over 5–6 s (Well of Corruption: 1 s pulses × 5–6 s; API skill 10671 facts: `Pulse 1`, `Duration 6`); cover/cleanse cycles matter more than the first second. |
| Zerg | Harasser pick | **2–3 s** | Strip (one global vs Stability) then dump on the stripped support/backliner. |
| Commander | Survival dummy | **10 s+ sustained incoming** | Not an alpha window — measure eHP/s under focus with support running. |

Cast-time note for the implementer: the API does **not** expose activation time (§6), so the sim's per-skill cast table must come from the wiki skill pages or a hand-maintained table.

---

## 4. After the alpha

### 4.1 Roam — escape requirements ("can leave")

An out is one of:

- **Teleport / shadowstep** (instant reposition; often needs a target — target selection tricks are part of the skill: current Willbender playbook explicitly describes spamming teleports on far targets/objects to cross terrain). (Source: "Build:Willbender - Radiant Swordbender Roamer — MetaBattle", https://metabattle.com/wiki/Build:Willbender_-_Radiant_Swordbender_Roamer, accessed 2026-08-14)
- **Stealth** with Revealed management (LoS tricks to avoid self-reveal are documented in the Deadeye playbook). (Deadeye build page; Stealth page.)
- **Superspeed**: +100% forward in combat, effect (unstrippable), 10 s cap. (Superspeed page.)
- **Leaps/dashes**: e.g. every Willbender Virtue is a mobility skill; both its weapon sets carry teleports or leaps. (Swordbender build page.)
- Plus the universal dodge: 300 units per dodge, 2 per full endurance bar. (Dodge page.)

**Roam gate for the scorer:** kit must contain ≥1 of {teleport/shadowstep, stealth access, superspeed, leap/dash chain} **and** self-cleanse/stun-break, else classify as havoc-at-best. A no-out burst kit is excluded from roam by definition (prompt rule, confirmed by how MetaBattle separates Roaming from Zerg lists: roam builds "prioritize mobility, stealth, self-sustain, and disengagement" — MetaBattle WvW page).

### 4.2 Havoc / zerg — brawl vs attrition

When the alpha fails and the fight continues, the score becomes sustain-vs-sustain:

- **eHP multipliers:** Protection −33% incoming damage (duration-stacking boon — strippable); high-tier HP pools (§1); shroud second bar (Necro).
- **Cleanse rate:** count `Number` facts with text "Conditions Removed" per unit time across the kit (encoded in API, §6); Resistance uptime as soft-CC immunity window.
- **Stability uptime:** duration-stacking in practice (stacks intensity, consumed 1 stack / 0.75 s under fire, stripped wholesale); zerg supports exist largely to keep stab rolling. (Stability page.)
- **Healing throughput:** `Heal`/`HealingAdjust` facts give hit counts only — **the API does not expose heal coefficients** (Heal fact = `hit_count` only; API:2/skills doc). Heal values need the wiki or hand data.
- **Commander survival:** model as incoming blob DPS vs (self mitigation + incoming support heals/stab/prot) over a sustained window; the commander's death costs the group its direction, which is why this is the one legitimate "tank" dummy. Downed state grants brief invulnerability on falling and on rally — so spikes "reset" through downstate and the finishing mechanic (stomp, which is itself interruptible) is part of the real kill window. (Invulnerability page: "briefly applied to every player as they fall into downed state or recover with rally"; interrupts: Control effect page.)

---

## 5. Scale assumptions

| Assumption | Roam | Havoc | Zerg |
|---|---|---|---|
| Allied Might/Fury | **None — bring your own** | Partial (1–2 allies) | **Assume capped-ish uptime from supports; stacking more is redundant** |
| Allied Stability | None | Rare/self only | Assume available on the push; self-stab still valued (redundancy under strip) |
| Allied Protection/other boons | None | Some | Assume present |
| Incoming heals/cleanse | None — self only | Some | Yes — dedicated support slots |
| Cleanse needed | Self-cleanse mandatory (no squad cleanse) | High self-cleanse | Group cleanse provided; self-cleanse still scored |
| Mobility/out | **Mandatory gate** | Required | Not required (stay with blob) |
| Target shape | Exactly 1 player | Few (2–5) | Clump/blob; target caps matter (facts: "Number of Targets", Radius) |
| Redundant jobs | Damage-only stacking | — | Might/fury stacking; generic "more DPS" |
| Unique jobs that score high | Cover window + strip + out | Flex (fill missing boon) | Stability source, strip/corrupt, cleanse, rez, healing, commander survivability |

Boon mechanics behind the table: boons stack by intensity (Might, Stability) or duration (rest); boon removal comes in four mechanically distinct flavors — **remove/strip, steal (transfer to self), corrupt (boon→condition), convert (condition→boon)**. (Source: "Boon — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/Boon — "Skills that remove boons / transfer boons / convert conditions into boons / convert boons into conditions", accessed 2026-08-14.)

---

## 6. API / fact encoding (implementer section)

Canonical doc: "API:2/skills — Guild Wars 2 Wiki", https://wiki.guildwars2.com/wiki/API:2/skills, accessed 2026-08-14. Local mirror: `crates/gw2api/src/models/facts.rs` implements all 19 documented fact types plus an `Unknown` fallback. Live payloads below were pulled from `api.guildwars2.com/v2/skills` on 2026-08-14.

### 6.1 Fact types and combat mapping

| `type` | Fields | Combat meaning / how to read it |
|---|---|---|
| `Buff` | `status` (string), `duration` (s), `apply_count`, `description` | **The workhorse.** Boons, conditions, AND named effects incl. hard CC and cover. `status` is the effect name string: `"Stability"`, `"Quickness"`, `"Stun"`, `"Daze"`, `"Fear"`, `"Taunt"`, `"Immobile"`(as condition), `"Aegis"`, `"Resistance"`, `"Distortion"`, `"Stealth"`... Example (live): Distortion → `{"type":"Buff","status":"Distortion","duration":1,"description":"Immune to conditions and damage."}` (skill 10192). |
| `PrefixedBuff` | same as `Buff` + `prefix{}` | Same, with attunement/legend-style icon prefix. |
| `Number` | `text`, `value` | **Strip/cleanse counts:** text `"Boons Removed"`, `"Conditions Removed"`, `"Number of Targets"`. Live example — Winds of Disenchantment: `{"text":"Boons Removed","type":"Number","value":1}` + `{"text":"Interval","type":"Time","duration":1}` (skill 45333). |
| `Time` | `text`, `duration` (s) | Durations/intervals: `"Duration"`, `"Interval"`, `"Pulse"`. |
| `Duration` | `duration` (s) | Same family (e.g. "Venom Duration"). |
| `Recharge` | `value` (s) | Cooldown. |
| `Range` | `value` | Skill range (units). |
| `Distance` / `Radius` | `distance` | Displacement distance / AoE radius (units). |
| `Damage` | `hit_count`, `dmg_multiplier` | Strike coefficient (weapon-strength scaled), per mode unsplit (see 6.3). |
| `Heal` / `HealingAdjust` | `hit_count` only | **No heal coefficient exposed.** |
| `Percent` | `percent` | e.g. "Life Force: 1%" (WoC). |
| `AttributeAdjust` | `value`, `target` | Attribute deltas; `target:"CritDamage"` = Ferocity; `target:"Healing"` = heal. |
| `StunBreak` | `value: true` | **The stun-break flag.** Presence = skill usable while CC'd. |
| `Unblockable` | `value: true` | Ignores block/Aegis. |
| `ComboField` / `ComboFinisher` | `field_type`, `finisher_type`, `percent` | Combo system (smoke field + blast = stealth, etc. — relevant to roam cover). |
| `NoData` | — | Display-only flags ("Combat Only"). |

Skill-level fields that matter: `type` (Heal/Utility/Elite/Weapon/Profession/...), `slot` (`Weapon_1..5`, `Utility`, `Profession_1..5`, `Downed_1..4`), `weapon_type`, `professions[]`, `specialization` (elite spec id; WoD → 61 = Spellbreaker), `categories[]` (`StealthAttack` = thief stealth-only skill; `DualWield`), `flags[]` (`GroundTargeted`, `NoUnderwater`), `initiative` (thief cost), `cost` (energy/adrenaline), `flip_skill`, `next_chain`/`prev_chain`, `transform_skills`, `bundle_skills`, `toolbelt_skill`, `facts[]`, `traited_facts[]`. (API:2/skills doc.)

`traited_facts`: same shape as facts plus `requires_trait` (trait id) and `overrides` (index into `facts` to replace; omitted = append). **Traited overrides ARE available** — parse them; many WvW-relevant values hide there. (API:2/skills doc, "Traited Facts".)

### 6.2 What is NOT in the API (verified against live payloads 2026-08-14)

- **Boon corrupt / convert is silently absent from facts.** Well of Corruption (skill 10671) full fact list: Recharge, Unblockable, Damage, Number of Targets, Pulse, Duration, Radius, Life Force, Combo Field — **zero facts about converting boons to conditions**; the only place it exists is the free-text `description` ("Target area pulses, converting boons on foes into conditions."). ⇒ Corrupt/convert detection requires **description-text parsing** or a hand table. Strip, by contrast, IS structured (`Boons Removed` Number).
- **Cast/activation time: not exposed.** The documented field list has no activation field and no fact type carries it. The alpha sim's cast table must come from wiki skill pages (Semantic MediaWiki data) or hand entry. The wiki has the data (Backstab ¼ s, WoD 1½ s).
- **Game-mode splits: not exposed.** Wiki skill pages are mode-split (PvE / WvW / PvP rows; WoD version history shows repeated WvW-only changes, e.g. "Reduced incoming boon duration reduction from 100% to 33% in WvW only", 2022-03-29). The API returns a single fact set. [INFERENCE: no mode field exists in the documented schema or observed payloads — treat API numbers as PvE-biased and expect WvW deltas.]
- **Rune/relic/sigil bonuses:** item endpoints carry these as **unstructured text** (bonus/description strings), not facts — do not expect machine-readable triggers from them.
- **Invuln/block/evade frames on weapon skills:** often appear only as `Buff` facts with effect `status` names or in description text; there is no "Evade" fact type.

### 6.3 Practical extraction rules for the scorer

1. Hard CC: `Buff` facts with `status ∈ {Stun, Daze, Fear, Taunt, Knockdown, Knockback, Pull, Launch, Float, Sink}` + `duration` → lock time. (Knockback/pull displacement sometimes surfaces as `Number`/`Distance` instead — check both.)
2. Strip: `Number` text `"Boons Removed"` × `Interval`/`Pulse`/`Duration` Time facts → strip rate (WoD = 1 boon/s for 5 s).
3. Corrupt/convert/steal: description-text regex (`corrupt`, `convert`, `steal`, `transfer`) — facts will not save you.
4. Cleanse: `Number` text `"Conditions Removed"`; convert-to-boon cleanse → description text.
5. Cover: `Buff status ∈ {Distortion, Stealth, Invulnerability, Aegis, Stability, Resistance, Protection, Quickness, Superspeed}`; evade/block frames → description text ("evade", "block").
6. Stun break: presence of `StunBreak` fact.
7. Mobility: `Distance` facts on movement skills + description keywords (`shadowstep`, `teleport`, `leap`, `dash`, `retreat`); `Buff status = Superspeed/Swiftness`.
8. Always merge `traited_facts` with `requires_trait` resolution before scoring a traited build.

---

## 7. Calibration set (live, July 2026 patch)

Sources for tier placement: "WvW — MetaBattle" (zerg; Meta = used in ~100% of groups, sections current for July 15, 2026 patch) and "WvW Roaming — MetaBattle", https://metabattle.com/wiki/WvW_Roaming, both accessed 2026-08-14. Hardstuck coverage checked and largely absent for WvW roam (Thief: none; Guardian: only Dragonhunter/Firebrand WvW; Mesmer: one June-2024 Virtuoso) — MetaBattle is the live source. (Sources: https://hardstuck.gg/gw2/builds/thief/, /guardian/, /mesmer/, accessed 2026-08-14.)

| # | Profession | Elite spec | Mode | Scale | Task | Kind | Weapons | Dummy | Why meta (one line) | Sources |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Necromancer | Reaper | WvW | Zerg | DPS | Strike spike (melee cleave) | Axe/Focus + Greatsword (Spear variant) | Allied blob vs enemy clump | Durable melee burst + wells + built-in boon rip; forgiving frontline anchor | [MB WvW](https://metabattle.com/wiki/WvW) (Meta); [Build:Reaper - Power Reaper](https://metabattle.com/wiki/Build:Reaper_-_Power_Reaper) |
| 2 | Ranger | Untamed | WvW | Zerg | DPS | Strike spike | Hammer + Mace/Mace (GS variant: "high spike strip with the ambush") | Allied blob | Highest-impact zerg DPS; demanding positioning | [MB WvW](https://metabattle.com/wiki/WvW) (Meta); [Build:Untamed - DPS Untamed](https://metabattle.com/wiki/Build:Untamed_-_DPS_Untamed) |
| 3 | Elementalist | Evoker | WvW | Zerg | DPS | Ranged pressure | Spear (backline) | Allied blob | The ranged/backline zerg DPS reference | [MB WvW](https://metabattle.com/wiki/WvW) (Great); [slug](https://metabattle.com/wiki/Build:Evoker_-_Spear_Backline) |
| 4 | Guardian | Firebrand | WvW | Zerg | Support | Stability-priority | Staff + Mace/Shield (Axe variant for CC) | Allied blob / commander under fire | Stability + utility that enables the whole push | [MB WvW](https://metabattle.com/wiki/WvW) (Meta); [Build:Firebrand - Support Firebrand](https://metabattle.com/wiki/Build:Firebrand_-_Support_Firebrand) |
| 5 | Ranger | Druid | WvW | Zerg | Support | Healer / cleanse | Staff + Mace/Warhorn | Allied blob | The healing/sustain anchor | [MB WvW](https://metabattle.com/wiki/WvW) (Meta); [Build:Druid - Support Druid](https://metabattle.com/wiki/Build:Druid_-_Support_Druid) |
| 6 | Mesmer | Troubadour | WvW | Zerg | Support | Boon-priority / control | unknown | Allied blob | Inspiration/Chaos support-control is meta-tier | [MB WvW](https://metabattle.com/wiki/WvW) (Meta); [slug](https://metabattle.com/wiki/Build:Troubadour_-_Inspi/Chaos_Troub) |
| 7 | Necromancer | Core | WvW | Zerg | Disabler | Corrupt (wells) | Axe/Focus + Scepter/Sword | Enemy blob with boons | Well-of-Corruption identity: converts enemy boons to conditions on 1 s pulses; denies the enemy's stab/prot engine | [MB WvW](https://metabattle.com/wiki/WvW) (Meta); [Build:Necromancer - Core Necro](https://metabattle.com/wiki/Build:Necromancer_-_Core_Necro); [Well of Corruption](https://wiki.guildwars2.com/wiki/Well_of_Corruption) |
| 8 | Warrior | Spellbreaker | WvW | Zerg | Disabler | Strip + interrupt | Sword/Axe + Spear (GS variant) | Enemy blob with stab/prot | Winds of Disenchantment: 1 boon/s stripped in 360 radius for 5 s + Full Counter interrupts; boon-hate traits (Loss Aversion, Enchantment Collapse) | [MB WvW](https://metabattle.com/wiki/WvW) (Meta); [Build:Spellbreaker - Spear DPS](https://metabattle.com/wiki/Build:Spellbreaker_-_Spear_DPS); [Winds of Disenchantment](https://wiki.guildwars2.com/wiki/Winds_of_Disenchantment) |
| 9 | Revenant | Conduit | WvW | Zerg | Harasser | Support-hunter | unknown (Shiro/Razah) | Enemy support with stab | Shadowstep onto a single target + boon removal when isolating one enemy (Beguiling Haze) — the pick pattern | [MB WvW](https://metabattle.com/wiki/WvW) (Great); [Boon — skills that remove boons](https://wiki.guildwars2.com/wiki/Boon) |
| 10 | Guardian | Dragonhunter | WvW | Zerg | Harasser | Backline-DPS hunter | unknown | Enemy backline DPS | Spear of Justice tethers + cripples a backliner (¾ s cast, 20 s recharge) so the blob collapses on the pick | [MB WvW](https://metabattle.com/wiki/WvW) (Great, Power Dragonhunter); [Spear of Justice](https://wiki.guildwars2.com/wiki/Spear_of_Justice) |
| 11 | Guardian | Luminary | WvW | Zerg | **Tank (commander)** | Survive under support | unknown | Commander under fire, receiving heals/stab | Support-chassis tag: survives focus so the squad keeps direction | [MB WvW](https://metabattle.com/wiki/WvW) (Meta, Support Luminary) |
| 12 | Guardian | Willbender | WvW | **Roam** | Harasser (DPS dive) | Strike spike | Greatsword + Sword/Sword | Naked 12–16k single player | Every Virtue is mobility; cover = Renewed Focus invuln + Blind spam + Aegis; disable = GS pull; escape = teleports/leaps on both sets | [MB WvW Roaming](https://metabattle.com/wiki/WvW_Roaming) (Great); [Build:Willbender - Radiant Swordbender Roamer](https://metabattle.com/wiki/Build:Willbender_-_Radiant_Swordbender_Roamer) |
| 13 | Thief | Deadeye | WvW | Roam | Harasser | Ranged stealth burst | Rifle + Sword/Dagger | Single target / zerg fringe pick | Cover = stealth (Revealed management via LoS); disable = interrupt utility; escape = stealth + mobility | [MB WvW Roaming](https://metabattle.com/wiki/WvW_Roaming) (Great); [Build:Deadeye - SA Rifle Roamer](https://www.metabattle.com/wiki/Build:Deadeye_-_SA_Rifle_Roamer); [Stealth](https://wiki.guildwars2.com/wiki/Stealth) |
| 14 | Mesmer | Virtuoso | WvW | Roam | Harasser | Strike spike w/ invuln cover | Spear + Dagger/Shield | Single target | Cover = Distortion (1 s + 1 s/clone); disable = daze pressure; escape = teleports/stealth kit | [MB WvW Roaming](https://metabattle.com/wiki/WvW_Roaming); [Build:Virtuoso - Power Speartuoso Roamer](https://metabattle.com/wiki/Build:Virtuoso_-_Power_Speartuoso_Roamer); [Distortion](https://wiki.guildwars2.com/wiki/Distortion) |
| 15 | Revenant | Herald | WvW | Roam | Harasser | Power burst + chase | unknown | Single target | Shiro burst/mobility chase; thin sustain/cleanse is the accepted tradeoff | [Build:Herald - Shiro Roamer](https://www.metabattle.com/wiki/Build:Herald_-_Shiro_Roamer) |
| 16 | Ranger | Soulbeast | WvW | Roam | Harasser | Condi ramp | unknown | Single target | Pet-merged condi pressure + mobility; proves a third armor class roams | [MB WvW Roaming](https://metabattle.com/wiki/WvW_Roaming) (Great: "Poisonbeast Soulbeast") |
| 17 | Revenant | Herald (Celestial) | WvW | **Havoc** | DPS bruiser | Hybrid sustain | unknown | Small group, partial boons | Sustain + evade + mobility carries 2–5 man fights where pure roam burst can't brawl | [Build:Herald - Celestial Herald Roamer](https://www.metabattle.com/wiki/Build:Herald_-_Celestial_Herald_Roamer) |
| 18 | Engineer | Scrapper | WvW | Havoc | DPS bruiser | Strike (hammer) | Hammer | Small group / uneven fights | Durable + mobile; built for outnumbered small-scale | [MB WvW Roaming](https://metabattle.com/wiki/WvW_Roaming) (Great: "Explosive Hammer Scrapper") |

Excluded-on-purpose: no pure-DPS-without-out appears as roam; no "DPS that also CCs" appears as disabler (rows 7–8 are strip/corrupt/interrupt identities first). Rows 12–16 span five professions (Guardian, Thief, Mesmer, Revenant, Ranger) to satisfy "any spec can roam if the kit provides mobility + cover + spike + out".

---

## 8. What would falsify our model

Concrete cases where "CC/cover then dump vs 12–30k in ~2 s" picks the **wrong** kit:

1. **Condi ramp scored on a 2 s clock.** Wells pulse 1 s × 5–6 s (WoC facts: Pulse 1, Duration 6); condi ticks are per-second. A condi zerg DPS looks terrible at T=2 and dominant at T=6–8. ⇒ Per-kind clocks, §3.
2. **Quickness-gated burst.** Quickness makes activations 50% faster (Quickness page). Two kits with identical tooltips differ by whether their burst is quickness-covered; a model without quickness-aware cast tables mis-ranks them. Mirror case: **Slow** stretches the defender's clock.
3. **Interrupt-vulnerable channels.** All control effects interrupt on application (Control effect page). A burst riding on a 1½ s cast elite (WoD-shaped) loses to a ¼ s interrupt whenever the defender holds one — the model must price cast exposure, not just damage.
4. **Downstate reset.** Falling to downed and rallying both grant brief invulnerability (Invulnerability page). "Kill in T" is really "down in T, then finish through a second invuln-gated phase"; stomps are interruptible casts. Ignoring downstate overvalues raw spike vs secure-finish tools.
5. **Cleanse before the spike lands.** Condi burst into a kit with high "Conditions Removed" throughput (or Resistance uptime — condi *effects* suppressed, though condi damage keeps ticking) evaporates. Resistance specifically breaks the "immobilize → dump" roam pattern without stopping any hard CC.
6. **Roam that must brawl.** If the out is missing, the correct play after a failed alpha is impossible; a scorer without the mobility gate will rank a no-out burst kit above Celestial Herald-style bruisers that actually win map presence.
7. **Zerg that needs corrupt, not HP damage.** Vs a support-engined blob (perma Protection −33%, stab rolling), +5% strike DPS loses to one more corrupt/strip source — Stability evaporates to a single removal regardless of stack count (Stability page), so strip is a binary gate, not a gradient.
8. **Harasser without strip.** Stability/Protection on the target support makes the dump land at −33% or zero (stab-immune lock); steal/strip-first is mandatory — a harasser scored without the strip gate picks the wrong backline hunter.
9. **API-blind corrupt kits.** A scorer reading only `facts` sees Well of Corruption as a mediocre pulsing damage field and misses its entire disabler value (§6.2) — an encoding-level falsifier, not a combat one.

---

## Numbers we can code

| Constant | Value | Unit | Source (all accessed 2026-08-14) |
|---|---|---|---|
| HP naked @80, low tier (Guardian/Thief/Ele) | 11,645 | HP | wiki "Health" |
| HP naked @80, mid tier (Rev/Engi/Ranger/Mesmer) | 15,922 | HP | wiki "Health" |
| HP naked @80, high tier (Warrior/Necro) | 19,212 | HP | wiki "Health" |
| Vitality → HP | 10 | HP per point | wiki "Health" |
| Live HP band for dummies | 12,000–30,000 (tiered dummies: ~13k glass / ~20k bruiser / ~25–30k support-commander) | HP | wiki "Health" + [INFERENCE] from gear vitality |
| Alpha T — roam dive | 2.0 | s | derived: dodge 0.75 s + CC 1–1.5 s + casts 0.25–1.5 s (Dodge, Backstab, WoD pages) |
| Alpha T — havoc | 2–3 | s | same derivation, partial support |
| Alpha T — zerg strike spike | 3.0 | s | squad convergence + defender support |
| Alpha T — zerg condi ramp | 6–8 | s | WoC pulse facts (1 s × 5–6 s) |
| Commander survival window | 10+ | s sustained | model choice |
| Dodge evade duration / cost / distance | 0.75 s / 50 of 100 endurance / 300 units | — | wiki "Dodge" |
| Quickness activation speedup | 50% faster (≈1.5× actions per window) | — | wiki "Quickness" |
| Protection damage reduction | −33% incoming damage, duration-stacking, strippable | — | wiki "Protection" |
| Stability stacks / consumption / strip rule | max 25 intensity; ≤1 stack consumed per 0.75 s; **any boon removal voids all stacks** | — | wiki "Stability" |
| Resistance scope | suppresses nondamaging condition effects (incl. Immobile, Fear, Taunt); not hard CC; condi damage continues; Poison heal-reduction and Terror damage exempt | — | wiki "Resistance" |
| Aegis scope | blocks the single next attack | — | wiki "Aegis" |
| Invulnerability scope | all damage + conditions + CC; unstrippable (effect, not boon); granted on downed entry + rally | — | wiki "Invulnerability" |
| Distortion duration | 1 s + 1 s per clone | s | wiki "Distortion"; API skill 10192 |
| Stealth rule | breaks when the user deals damage; breaking applies Revealed (re-stealth blocked several seconds); expiry-timing attacks avoid Revealed | — | wiki "Stealth" |
| Superspeed | +100% forward in combat; effect (unstrippable); duration cap 10 s | — | wiki "Superspeed" |
| Which CC stops evade | Stun, Knockdown, Launch, Knockback, Pull, Fear, Taunt, Float, Sink + **Immobilize** (condition); **Daze does NOT** | — | wiki "Control effect", "Dodge" |
| Which cover blocks the alpha answer | Invuln/Distortion (all), Stealth (targeting), dodge frames (0.75 s), block/Aegis (1 hit), Blind (1 hit), Stability (counter-CC only) | — | pages above |
| Stun break scope | removes duration control effects incl. Fear/Taunt; usable while CC'd; not Freeze/transform | — | wiki "Stun break" |
| Interrupt rule | every control effect interrupts on application; interrupts counter cast-time bursts/heals/stomps | — | wiki "Control effect"; MetaBattle Deadeye usage |
| Roam mobility gate ("an out") | ≥1 of: teleport/shadowstep, stealth access, superspeed, leap/dash chain; plus self-cleanse + stun break | — | MetaBattle Swordbender/Deadeye playbooks; wiki "Superspeed", "Stealth" |
| Zerg assumed boons | Might, Fury, Protection, Stability from allied supports; extra Might stacking scores ~0; unique jobs (stab source, strip/corrupt, cleanse, rez, heal) score high | — | MetaBattle WvW (slot structure); wiki "Boon" |
| Harasser strip gate | strip/steal/corrupt **before** damage is mandatory (binary gate): Stability all-stacks-voided + Protection −33% otherwise eat the dump | — | wiki "Stability", "Protection", "Boon" |
| API: hard CC / cover encoding | `Buff` fact, key = `status` string, `duration` s, `apply_count` | — | wiki "API:2/skills"; live skills 10192/45333 |
| API: strip encoding | `Number` fact, text `"Boons Removed"`, value = count; pair with `Time` Interval/Pulse/Duration | — | live skill 45333 (WoD: 1/s × 5 s) |
| API: corrupt/convert encoding | **absent from facts — parse `description` text** (WoC skill 10671 has no corrupt fact) | — | live skill 10671 |
| API: cleanse encoding | `Number` fact, text `"Conditions Removed"` | — | wiki "API:2/skills" (Number example) |
| API: stun break / unblockable | `StunBreak`/`Unblockable` facts, `value: true` | — | wiki "API:2/skills" |
| API gaps | no cast time; no mode-split values (PvE-biased); no heal coefficients; rune/relic bonuses = unstructured text | — | wiki "API:2/skills"; WoD version history (WvW-only changes) |
