# Feature Specification: Trait triggers are the build

**Feature Branch**: `007-trait-triggers`

**Created**: 2026-09-08

**Status**: Draft

**Input**: User description: the player's rule of 2026-09-08. "Traits provide conditions or effects that define a build. Without those effects we cannot have any valid workable build which can be called optimized or meta. The effects and conditions that arise from triggering those are the single most important thing in GW2; this is the sole purpose why people create meta builds and theorycraft." Sprint 2 (`specs/005-wvw-proc-sites`) built the firing mechanism and proved it on sigils, a rune, a relic, dark combos and shroud. Its task T042 found that not one of the nine traits on the player's cached Reaper build could be given a record, for three named reasons. This sprint makes trait effects fire.

## Problem

A build is its trait choices, and the simulator today executes only the arithmetic half of a trait: its stat lines, its permanent percent modifiers, and the changes it makes to a skill's own numbers. The conditional half, "when you enter shroud", "while in shroud", "on a critical hit against a chilled foe", "when you use a skill of this kind", "when you leave shroud", never changes a simulated event, because the trigger kinds those traits need do not exist and the trait records were never written. Two builds that differ only in such a trait score the same. The coverage line names the trait as "(no record)" and the build is marked Provisional, which is honest, but it means nothing the optimizer calls optimized has been judged on the thing that makes it a build.

## Clarifications

### Session 2026-09-08

- Q: Where do the per-profession test builds come from for the eight professions after Necromancer? → A: Both: one published WvW build per profession from the synced sites for the ranking checks, and a small hand-authored opener per profession that reaches every trigger kind the profession's traits use.
- Q: When do the increments reach the player, and who accepts them? → A: Acceptance is automated, not in-game: this sprint changes no screen, and every record is arithmetic against a named wiki page. A profession ships (release and DLL) when its automated gates pass; the player is not asked to test.
- Q: How does the simulation count trait effects that land on allies or enemies? → A: It simulates all of them: every corrupting and damaging effect on the enemies present, every boon and healing effect on the allies present and on the player, within the mode and scale's target caps. An optimized build is the build with the highest potential ceiling, and that ceiling is not measurable if any target-facing effect is left out.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A shroud trait fires when shroud is entered, held, or left (Priority: P1)

As a WvW Necromancer, I want the traits that key off shroud to change the simulated fight at the moment they trigger, so that a Soul Reaping line that grants swiftness on entry, raises damage on entry and exit, or raises critical damage while inside is scored for what it does and not only for its stat line.

**Why this priority**: Shroud is the Necromancer's profession mechanic and every elite line hangs traits on it. Sprint 2 built shroud entry and exit into the timeline; the triggers are the missing hook. It is also the trait family the player's own builds use.

**Independent Test**: Run the Reaper fixture opener with a shroud-entry trait record equipped and with it removed; the trace shows the effect applied at the entry event, and the strike or boon totals differ in the expected direction; the trait leaves the "Not simulated" line.

**Acceptance Scenarios**:

1. **Given** a trait whose effect triggers on entering shroud, **When** the opener enters shroud, **Then** the trace carries a trait-fired event at that instant and the effect (a boon, a damage window, a cleanse) is applied to the player.
2. **Given** a trait whose effect lasts while in shroud, **When** the player is in shroud, **Then** the effect is active from entry to exit and inactive outside, visible in the trace and in the strike results of shroud skills.
3. **Given** a trait whose effect triggers on leaving shroud, **When** shroud ends by choice, by drain or by damage, **Then** the effect is applied once at that instant.
4. **Given** a Scourge, **When** Desert Shroud is used, **Then** traits that trigger on entering shroud trigger then, and never on Manifest Sand Shade.
5. **Given** the same build with the trait removed, **When** the opener runs, **Then** no trait-fired event is attributed to it and the results differ from the equipped run.
6. **Given** two runs with identical inputs, **When** both complete, **Then** their results are identical.

---

### User Story 2 - Prerequisites on the foe and trait-owned skill triggers (Priority: P2)

As a player whose trait says "on critical hit against a chilled foe" or "when you use a shout", I want the trigger to fire only when the prerequisite holds and to count the skill kind the trait names, so that a Reaper's Chilling Nova and Dread are simulated instead of skipped.

**Why this priority**: These are the two schema holes Sprint 2's T042 named as the reason no Reaper trait could be recorded. Without them the catalogue in Story 3 cannot be complete for even one build.

**Independent Test**: Run the fixture with a chilled foe and with an unchilled one; the enemy-condition trait fires only in the first. Run the fixture with a shout in the opener; the trait-owned skill trigger fires on the shout and not on other skills.

**Acceptance Scenarios**:

1. **Given** a trait with a prerequisite condition on the foe, **When** the foe carries that condition at the moment of the hit, **Then** the trait fires; **When** it does not, **Then** it does not, and the trace says why.
2. **Given** a trait that triggers when the player uses a skill of a named kind, **When** such a skill is cast, **Then** the trait fires once per cast, with its cooldown honoured; other skills do not fire it.
3. **Given** a trait whose life force effect is stated as a periodic gain or a gain when shroud ends, **When** the timeline runs, **Then** the life force pool reflects it at the stated interval or instant.

---

### User Story 3 - Every trait is either executed or explained (Priority: P3)

As a player of any profession, I want every trait my build wears to be one of three things, executed from its numbers, executed from a dated record of what it does, or named in the coverage line with the reason it cannot be, so that "Provisional" means a listed, explained remainder and "Verified" means the build was judged on all of its traits.

**Why this priority**: The catalogue is the product. Necromancer first because it is what the player tests in-game and what the fixture, the shroud model and the two prior sprints already cover; the other eight professions follow on the same mechanism, one per increment.

**Independent Test**: For the cached Reaper build, the coverage line's trait entries drop from nine to at most two, each of those two carrying a reason class. For every trait of every profession in the game data the audit table lists which of the three states it is in.

**Acceptance Scenarios**:

1. **Given** a profession's core lines and elite lines, **When** its increment is complete, **Then** each trait has a record, is executed from facts, or is classified with one of a fixed set of reasons (passive with no simulated effect, needs a mechanic named as out of scope).
2. **Given** a record, **When** it is read, **Then** it names the wiki page and the date it was read, and its numbers match that page for the mode it applies to.
3. **Given** the player's cached Reaper build, **When** it is scored in WvW, **Then** at most two of its nine traits remain in the "Not simulated" line, each with its reason.
4. **Given** two WvW builds that differ only in one recorded trigger trait, **When** both are ranked, **Then** the ranking tells them apart in the direction the trait's effect implies.
5. **Given** a PvE or PvP scoring of the same build, **When** it runs, **Then** its result is unchanged by this sprint.

---

### User Story 4 - The coverage line says what was not simulated and why (Priority: P4)

As a player reading the coverage line, I want it to name only sources whose mechanic was skipped, with the reason, and never a weapon skill whose strikes were executed, so that the line is a list of real gaps I can weigh.

**Why this priority**: Sprint 1's CONN-01-06 finding. Today the line lists forty names for the fixture, most of them skills that did run. Once traits fire, the line must be trustworthy or the Verified mark means nothing.

**Independent Test**: Score the fixture; every name in the line is a trait, upgrade or skill mechanic that was skipped, each with its reason; no executed weapon skill appears.

**Acceptance Scenarios**:

1. **Given** a weapon skill executed from its facts, **When** the coverage line is built, **Then** it is not listed.
2. **Given** a trait or skill with a skipped mechanic, **When** the line is built, **Then** it is listed with a reason from the fixed set.
3. **Given** a build with nothing skipped, **When** it is scored, **Then** the line is absent and the build is Verified.

---

### Edge Cases

- A trait that triggers on entering shroud when the build cannot enter shroud: it never fires, and the coverage line says the shroud floor was the reason, not the record.
- Harbinger Shroud: entry, hold and exit triggers apply the same way; its health-exposed rule is unaffected.
- A trait with a cooldown longer than the fight: fires once at most, and the trace shows the refused second attempt.
- A trait whose prerequisite is a condition the fixture never applies: never fires, recorded as "prerequisite never met" in the trace, not as "no record".
- A trait with both a stat line and a trigger: the stat line stays in the stat block and the trigger fires; nothing is counted twice.
- A record whose wiki page has split numbers per mode: the mode's number is used; a mode without a number leaves that mode's execution as "unread", never a guess.
- An ally-facing boon with a five-target cap in a Cloud fight: five allies receive it, the player among them; a foe-facing corrupt with the WvW cap strikes that many foes and no more.
- A Scourge with no shade out: shade skills still count as shroud-skill-1 for traits that say so, as the wiki states.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The effect record format MUST support triggers for entering shroud, leaving shroud, and being in shroud, and the WvW timeline MUST fire records of each kind at the matching instant or over the matching interval.
- **FR-002**: The record format MUST support a prerequisite on the foe (a named condition present at the moment of the trigger) and the timeline MUST evaluate it per trigger.
- **FR-003**: Trait-owned skill-use triggers MUST fire on the skill kinds the record names, with their cooldown, and on no other skill.
- **FR-003a**: Trait effects MUST be applied to every target they are meant for: damage, conditions and corrupts to the foes present, boons and healing to the allies present and to the player, bounded only by the mode and scale's target caps. The number of foes and allies present follows the selected scale (Roam, Havoc, Cloud; Open World, Group, Squad); no target-facing effect is skipped or reduced to the player's own share.
- **FR-004**: Life force gains stated as periodic or as ending-shroud gains MUST be applied by the resource model.
- **FR-005**: Every trait of every profession MUST be in exactly one state: executed from facts, executed from a record, or classified with a reason from a fixed set; the audit MUST list the state of each.
- **FR-006**: Every record MUST carry its wiki source and read date, and its numbers MUST match the page for its mode; no fixture value is ever copied into the data.
- **FR-007**: The coverage line MUST list only skipped mechanics, each with its reason, and MUST NOT list skills executed from facts.
- **FR-008**: A build with no skipped mechanic MUST be marked Verified; any skipped mechanic keeps it Provisional.
- **FR-009**: PvE and PvP results MUST be unchanged by this sprint, pinned by the existing guard.
- **FR-010**: Every behaviour change MUST be introduced against a failing experiment or unit test that passes afterwards, in the seen-failing discipline of Sprint 2.
- **FR-010a**: Every record MUST be covered by an automated check that reads its numbers against the wiki page and mode it names; a profession's increment MUST NOT be released with a record that has no such check.
- **FR-012**: Each profession's increment MUST carry two test builds: one published WvW build from the synced sites, scored as the site wrote it, for the ranking checks; and one hand-authored opener that reaches every trigger kind the profession's traits use, for trigger coverage. Fixture values are never copied into the data.
- **FR-011**: The catalogue covers all nine professions, every core and elite line. Necromancer goes first because its fixture, shroud model and trigger work already exist; each further profession lands as its own reviewable increment with a fixture-grade test build, so the triggers ship before the catalogue is complete and the coverage line stays truthful at every step.

### Key Entities

- **Trigger**: the event a record fires on; this sprint adds entering shroud, leaving shroud, being in shroud, and refines skill-use with a skill kind and an owner.
- **Prerequisite**: a condition on the foe that must hold at the trigger instant.
- **Trait record**: a dated statement of what a trait does when it triggers, in the mode it applies to.
- **Coverage state**: for each equipped source, one of executed from facts, executed from a record, or skipped with a reason.
- **Fight population**: the foes and allies present for the selected scale, each a target for the effects meant for them, capped per effect by the mode's target cap.
- **Reason class**: the fixed set of explanations a skipped source may carry.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the player's cached Reaper build, the "Not simulated" trait entries fall from nine to at most two, each with a reason.
- **SC-002**: Every trait of all nine professions in the game data has a listed coverage state; zero are unclassified.
- **SC-003**: Two WvW builds differing only in one recorded trigger trait rank in the order the trait implies, in every case the audit table lists.
- **SC-004**: The PvE and PvP guard values are unchanged to the last printed digit.
- **SC-005**: No executed weapon skill appears in any coverage line for the fixture or the cached build.
- **SC-006**: The optimizer harness stays within 10 percent of its Sprint 2 timing.
- **SC-007**: Every new record names a wiki page and a read date; a reviewer can check each number against the page in under a minute.

## Assumptions

- The Sprint 2 firing mechanism, trace, fixture and seen-failing controls are the base; no scoring constant, gate or threshold moves.
- Acceptance is automated for this sprint: a profession's increment is released when its record checks, seen-failing experiments, ranking-direction tests and the PvE/PvP pins pass. No in-game test is asked of the player; the in-game gate applies only to sprints that change what is on screen.
- PvE and PvP execution of records (CONN-01-03) and the enemy cooldown model (CONN-00-08) stay later sprints.
- Profession order after Necromancer follows the player's characters, then the remaining professions.
- Records are written from the wiki; where the wiki flags a number as awaiting verification, the record says so.
- The Scourge rule that shade skills count as shroud skill 1 and that Desert Shroud is its shroud entry comes from the wiki `Shade` and `Manifest Sand Shade` pages.
