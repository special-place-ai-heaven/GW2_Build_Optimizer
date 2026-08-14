# TASK — Research Guild Wars 2 combat for a build-optimizer score function

This file is the entire assignment. Execute it. Do not wait for another prompt.

**Project root:** `C:\AI_STUFF\PROGRAMMING\GW2_Build_Optimizer`  
**Research date:** 2026-08-14 (live game, current patch)  
**You are:** a combat researcher. Another engineer will code from your numbers. You do not write code.

## Deliverable (exactly one file)

Write:

`C:\AI_STUFF\PROGRAMMING\GW2_Build_Optimizer\research\kimi-combat-model.md`

Create the `research` folder if needed. Overwrite that file if it already exists.

When that file is complete, **stop**. Do not commit. Do not push. Do not create any other files. Do not modify `crates\`, `data\`, `.cursor\`, git, or this prompt file.

## Do not

- Propose Rust architecture, MCP, LLM prompts, UI, or how the plugin should work.
- Copy MetaBattle / Snow Crows / Hardstuck **prose**. Extract **structured facts** only.
- Invent numbers. If unknown, write **unknown**. Cite every number: URL + page title + date accessed.
- Whitelist professions. ArenaNet left traits, sigils, skills, and relics so **any spec can** fill a role. Typical kits are **calibration examples**, not exclusive lists.

---

## What we are trying to score

**META = damage (or support/disable effect) you actually get to finish**, not tooltip DPS.

Two independent axes:

1. **Scale** (the fight): **Roam** / **Havoc** (small group) / **Zerg**.
2. **Task** (the job): **DPS** / **Support** / **Disabler** / **Harasser** / **Tank (commander only)**.

Do not fuse them. A zerg DPS and a roam DPS are different dummies. A zerg harasser and a roam harasser are different lives.

### Working combat loop (challenge with sources if wrong)

- Live players **evade/block** until **locked** (hard CC) or the attacker is **covered** (brief invuln / distortion / evade window / Stability / Aegis / Resistance).
- **Alpha:** a short window (we guessed **~2 seconds** — confirm or replace) vs a **single player HP pool** (we guessed **15k–30k** — confirm by profession). Disable them **or** cover yourself, **then** dump. If that pool dies → spike/DPS success.
- If they live: **zerg/havoc** may **brawl**; **roam does not brawl** — roam already failed and must **leave**.
- **Attrition:** no sharp spike; win when their heal / stab / cleanse exhausts.

---

## Scale

### Zerg (WvW blob)

A zerg is built from **four slots**. **Tank is not a standard slot.**

| Slot | Job |
|---|---|
| **DPS** | Burn the tag / the clump. Kinds exist (strike spike, condi ramp, hybrid, ranged pressure) and **do not share one clock**. |
| **Support** | Keep *your* people able to play: heal, boons, cleanse, Stability. Supports **also** cover group survivability; that is why a dedicated tank is usually unnecessary. |
| **Disabler** | Make the *enemy* blob stop working: **boon corrupt, strip, convert**, plus nondamaging pressure. Typical current identities: **Necromancer** (corrupt / wells) and **Spellbreaker** (strip / interrupt). Confirm live names; do not treat “DPS that also CCs” as this slot. |
| **Harasser** | Dual role, still **with their blob**: (1) neutralize **enemy supports**, or (2) quickly kill the enemy’s **most offensive backline**. Needs **boon steal or equivalent strip** so Stability and Protection can be removed **before** damage. |

**Tank** in a zerg ≈ the **commander / tag**. That person must survive no matter what, because if they drop the group has no direction. This is an **opt-in commander-survival** dummy (incoming damage while **receiving** support), not “build a tank for every member.”

Zerg assumes **allied boons** (Might/Fury/etc. already on the bar). Do not reward stacking more Might as if it were unique. Unique jobs (Stability, strip, resurrect, heal, corrupt) matter more.

### Havoc (small group)

A few allies. Some shared boons, but you still need a real **out** and more self-heal/cleanse than in a zerg. Middle of roam and zerg — say what boons you can assume vs not.

### Roam (true self-sufficiency)

Roam is **not** a job. It is a **scale constraint** on any job, most often a **harasser variation with much more mobility**.

**Playstyle:** come in, **one** player, brief window of **invulnerability + damage**, **disable**, **kill**, **escape**. Life is hard.

Roamers must:

- **Circumvent** zergs (never take the 20-man), or
- **Pick a single player off a zerg fringe** and leave before the blob turns, or
- **Hunt the map**: other roamers and **small groups**.

**Mandatory on every roam kit:**

- **Self-sufficient** — no squad heals, no incoming Stability, no assumed Might. If you need it, you brought it.
- **Mobility / disengage** — you must be able to **leave**. A kit that wins the burst and cannot escape is **not roam**.
- **One target**, not a clump. If they live, **do not brawl** — leave.

Typical live roam spikes: **Willbender, Thief, Mesmer**. Use as calibration. **Any specialization** can have a roam kit if traits/skills/sigils/relics provide mobility + cover window + spike + out.

**Roam vs zerg harasser:** same idea (isolate, crack cover, burst). Zerg harasser stays with **their** group. Roamer has **no peel**, needs **much larger mobility**, and treats the zerg as terrain to go around or steal one body from.

---

## Tasks (kinds)

For each task, name **kinds** that need **different scores** (do not collapse them):

- **DPS:** strike spike vs condi ramp vs hybrid vs ranged/pressure.
- **Support:** healer vs boon-priority vs cleanse-heavy vs Stability-priority (a kit may do two; label the **primary** job).
- **Disabler:** corrupt vs strip vs convert vs high uptime of **nondamaging** conditions (immob, chill, cripple, weakness, blind, fear, daze). Not “extra DPS.”
- **Harasser:** enemy-support hunter vs backline-DPS hunter. Always: steal/strip **then** dump.
- **Tank/commander:** survive under support. Not a solo immortal roam set.

---

## Report sections (required)

Start with a **~10 line executive summary**: what to simulate per scale; recommended alpha **T** in seconds; HP band; which effects are **mandatory** in the score (CC, cover/invuln, strip/corrupt, cleanse, mobility/out).

### 1. HP pools

Typical PvP/WvW player HP by profession (and armor class if it matters). Min / typical / max. Confirm or correct 15k–30k.

### 2. Lock vs cover

For each hard CC (daze, stun, knockdown, knockback, pull, float, sink, fear, taunt, immobilize): does it stop evasion? Typical durations. Stunbreak vs Stability vs Aegis vs Resistance vs **true invuln** (distortion, hide, etc.): what each prevents.

Soft CC (chill, cripple, blind, weakness): alpha clock or not?

Roam **cover window**: which skills actually give a brief “cannot be acted upon” while damage is live?

### 3. Alpha window

Is ~2s grounded? Cast times for burst elites vs auto. Time to first hard CC. Time for stunbreak + evade. Recommend **0–T** we should simulate, **per scale** if they differ (roam dive vs zerg spike).

### 4. After the alpha

- **Roam:** escape requirements (teleport, stealth, superspeed, leap). What “can leave” means in skills/traits.
- **Havoc/zerg:** brawl vs attrition. eHP, cleanse rate, Stability uptime, Protection, healing. Commander survival vs blob DPS.

### 5. Scale assumptions

Table: roam / havoc / zerg × which boons you may assume from allies, which jobs are redundant (e.g. stacking Might in zerg), which are unique.

### 6. API / fact encoding (implementer section)

How the official GW2 API encodes: hard CC, boon strip, corrupt, convert, steal, condition cleanse, blocks, invuln, Stability, mobility (teleport, superspeed). Exact fact `type` strings and fields (e.g. “Boons Removed” as `Number`). What the API does **not** give (rune bonus strings, traited overrides).

### 7. Calibration set

**12–18 current kits**, as a table. Cover:

- Zerg: DPS (at least two kinds), Support (heal and Stability/boon), Disabler (Necro **and** Spellbreaker if still true), Harasser (support-hunter **and** backline-hunter).
- Commander/tag survival: **one** row, labeled tank/commander, not a generic tank.
- Roam: Willbender, Thief, Mesmer, **plus at least two other professions** that have a real roam kit, to prove “any spec can.” Each roam row must name **invuln/cover**, **disable**, **escape**.
- Havoc: 1–2 rows if distinct from roam/zerg.

Columns: profession, elite spec, game mode, **scale**, **task**, **kind**, weapons (if known), **dummy** (naked 15–30k vs support with stab+prot vs allied blob vs commander under fire vs zerg fringe pick), **one-line** why it is meta, sources.

A high-damage kit with **no out** must **not** appear as roam. A DPS that CCs a little must **not** appear as disabler.

### 8. What would falsify our model

Concrete cases where “CC/cover then dump vs 15–30k in ~2s” would pick the **wrong** kit (condi delayed ramp, quickness-gated burst, downstate, condi cleanse before spike, roam that must brawl, zerg that needs corrupt not HP damage).

## End with

**“Numbers we can code”** — constants + units + source. At minimum: HP band, alpha T, which CC stops evade, which cover blocks the answer, roam mobility gate (what counts as an out), zerg assumed boons, strip/corrupt as a **gate** for harasser (not a nice-to-have).

When `C:\AI_STUFF\PROGRAMMING\GW2_Build_Optimizer\research\kimi-combat-model.md` is written and complete, stop. Do not ask for confirmation.
