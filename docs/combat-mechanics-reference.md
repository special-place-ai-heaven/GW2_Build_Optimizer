# Combat mechanics reference — combo system, boons, conditions

Numbers the optimizer's objective needs and does not currently model. Derived from
the sources below; verify against the wiki before shipping anything load-bearing.

Sources (read 2026-09-04):
- Combo fields and finishers — <https://guildjen.com/what-are-combo-fields-and-finishers/>
- Boons and conditions — <https://guildjen.com/what-are-boons-and-conditions-in-guild-wars-2/>

Why this file exists: `rotation/simulator.rs` currently carries
`// Combo fields tracked but not simulated for damage`, and damage modifiers are
resolved once per build in `engine.rs::calculate_validated_stats` rather than
against live state. Everything below is what that costs us.

## Combo system — 9 fields × 4 finishers = 36 interactions

Fields do nothing alone. The **field** decides the effect type, the **finisher**
decides its shape and who it reaches.

| Field | Effect family |
|---|---|
| Dark | Dark Aura (Torment on hit, −20% condi damage taken), life steal ~170–200 |
| Ethereal | Confusion, Chaos Aura (random Weakness/Confusion/Cripple to attacker, random Protection/Regeneration/Swiftness to holder) |
| Fire | Burning, Might. Blast = **3 stacks Might for 20s**; leap = Fire Aura |
| Ice | Chill, Frost Aura (−10% strike damage taken) |
| Light | Condition cleanse, Light Aura |
| Lightning | Leap = **daze**; blast = swiftness; projectile/whirl = Vulnerability |
| Poison | Poison, Weakness |
| Smoke | Blind, **stealth** (leap = self, blast = area) |
| Water | Healing (blast and leap are strong; projectile/whirl give Regeneration) |

| Finisher | Rule |
|---|---|
| Blast | 360-unit circle, **max 5 targets** |
| Leap | Self, or the target hit. **Interrupted ⇒ no finisher.** The **first field passed through** decides the effect |
| Projectile | Hits enemies, or allies near the target. No-cooldown / multi-projectile skills proc at **20%** |
| Whirl | Multiple projectiles in various directions; can multi-hit one target at point-blank |

Rules that constrain any implementation:

- **Combo output scales with the FINISHER user's stats.** A water blast heals more
  from a player with more Healing Power. So combo value couples to gear — it is not
  a flat bonus.
- **Max 5 separate players may finish in one field.**
- **Field priority:** your own fields first, then oldest first.
- **Ordering rule, stated outright:** fields first, finishers after. This is a
  rotation-ordering constraint derived from the combo system itself.
- Stealth caveat: a blast finisher that hits a target while stealthing always
  reveals; a leap does not reveal unless already stealthed.

## Boons

| Boon | Effect | Stacking |
|---|---|---|
| Fury | +25% crit chance PvE, **+20% PvP/WvW** | duration |
| Might | **+30 power AND +30 condition damage** per stack | intensity, cap 25 |
| Protection | −33% strike damage taken; **multiplicative** with other DR | duration |
| Resolution | −33% condition damage taken | duration |
| Regeneration | heal per second; scales with **applier's** healing power (highest applier wins) | duration |
| Stability | prevents next disable; **only one stack removable per 0.75s** | intensity |
| Aegis | blocks next strike — **one instance**, and negates that attack's conditions/disables | duration |
| Alacrity | **+25% cooldown recharge rate** | duration |
| Quickness | **+50% skill animation speed** | duration |
| Swiftness | +33% movement; does not stack with other movement effects | duration |
| Vigor | +50% endurance regeneration (more dodges) | duration |
| Resistance | ignores non-damaging conditions | duration |

Alacrity and Quickness change *rotation timing itself*, so they cannot be modelled
as flat multipliers — they alter how many casts fit in a window.

## Conditions

| Condition | Effect |
|---|---|
| Bleeding | slight damage/s |
| Burning | moderate damage/s |
| Torment | slight damage/s, **more against non-moving targets** — pairs with disables |
| Poison | slight damage/s, **−33% healing received** |
| Confusion | slight damage/s, **extra damage whenever the target activates a skill** |
| Cripple | −50% movement |
| Chill | −66% movement **and −66% cooldown recharge** |
| Immobilize | no movement, turning or dodging |
| Blind | next attack misses, then removed |
| Weakness | −50% endurance regen; **50% of attacks fumble** (−50% damage, cannot crit) |
| Slow | +50% skill animation time |
| Taunt / Fear | forced movement toward / away from the applier |
| **Vulnerability** | **+1% to ALL strike and condition damage per stack**, cap 25 |

Vulnerability is the universal amplifier and the clearest case of target-state
conditional damage the current static-modifier model cannot represent.

## Ordering mechanics — value that only exists in a sequence

- **Cleansing is first-in-last-out.** Most recently applied conditions cleanse first.
- **Cover conditions:** apply damaging conditions FIRST, then non-damaging ones on
  top, so the damaging stack sits deeper and survives more cleanses. Pure sequencing
  value, invisible to any unordered model.
- **Cleanse priority:** specific cleanses (movement-inhibiting / damaging) before
  general ones, or a general cleanse wastes itself on a condition the specific one
  could have taken.
- **Conversion / corruption:** Stability → Fear, Burning → Aegis,
  Vulnerability ↔ Protection. Momentum swings that remove and apply simultaneously.
- **Boon removal** strips all stacks and duration of a boon at once.

## What this implies for the model

1. Combo fields/finishers are a 36-entry lookup, gated on a field being active at
   the finisher's position and time. Cheapest high-value fix available.
2. Vulnerability, Might, Fury, Protection, Weakness and Chill are all *state* that
   must be tracked over time and read at the moment a skill resolves — not folded
   into a build-level constant.
3. Alacrity, Quickness, Chill and Slow change cast and recharge timing, so they
   feed back into the rotation rather than scaling its output.
4. Condition ordering (cover conditions) and cleanse ordering give sequences value
   that no unordered representation can express.
