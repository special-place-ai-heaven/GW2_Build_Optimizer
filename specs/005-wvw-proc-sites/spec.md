# Feature Specification: WvW proc firing sites

**Feature Branch**: `005-wvw-proc-sites`

**Created**: 2026-09-08

**Status**: Draft

**Input**: User description: "Sprint 2 on CONN-03 C WvW first" (Simulator connections and build-quality plan, phase CONN-03 C, minimal mechanic completion). Sprint 1 (`specs/004-simulator-trust`) demonstrated, with a failing experiment for each, which mechanics the WvW timeline cannot execute. This sprint makes those mechanics execute in WvW. PvE and PvP execution of the same records (CONN-01-03), the reporting semantics of "(no record)" (CONN-01-06, CONN-03 B), and the enemy cooldown model (CONN-00-08) are later sprints.

## Problem

The Sprint 1 audit (`docs/simulator-connection-audit.md`) proved that four kinds of equipped source are named in the coverage line but never change a simulated event in WvW: on-crit procs (CONN-00-06), sigils on the second weapon set after a swap (CONN-00-07), conditional and health-threshold rune and relic bonuses, which today are flattened into permanent modifiers or dropped (CONN-01-01), and dark-field combos (CONN-01-02). A build built around any of them is scored as if the mechanic did not exist, and a player reading the coverage line learns only that it was skipped. The Reaper slice from Sprint 1 exercises all four, so every remedy here has a fixture, a failing experiment and a trace to start from.

## Clarifications

### Session 2026-09-08

- Q: How does an on-crit proc's probability enter the result? → A: Expected value in search and ranking (each landed hit contributes crit chance times the proc effect, cooldown applied to the expected arrival rate); seeded deterministic trials only in the diagnostic trace, where the mean and spread over the seeds are reported.
- Q: Which real sources get records this sprint? → A: Only the Reaper slice's first verified cases: the on-crit sigil, the health-threshold rune, the stacking relic, and the Reaper traits the fixture uses. Other professions wait for the data sprint.
- Q: Is Necromancer life force in this sprint? → A: Yes, in full: life force generation and drain derived from skill facts, shroud entry and exit, for all Necromancer specialisations.
- Q: Do the elite specialisations need their own shroud models? → A: No. Shroud is maintained the same way across specialisations; only some individual mechanics change, and those come from the facts. One shared shroud shape, per-specialisation numbers.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An on-crit proc changes the fight (Priority: P1)

As a player whose WvW build carries an on-crit sigil, I want its proc to fire in the simulation at a rate that follows the build's critical chance and the proc's cooldown, so that a crit-heavy build and a no-crit build are told apart by the result and not only by the coverage line.

**Why this priority**: On-crit sigils are on almost every WvW damage build. It is the first break Sprint 1 demonstrated and the one the plan named as the reference case for CONN-03 C.

**Independent Test**: Run the Reaper fixture opener with the on-crit sigil equipped and with it removed; the fired-event trace and the strike total differ in the expected direction, and the sigil no longer appears in the coverage line.

**Acceptance Scenarios**:

1. **Given** the fixture build with its on-crit sigil, **When** the opener runs in WvW, **Then** the trace contains proc-fired events attributed to the sigil and the sigil is absent from the "Not simulated" line.
2. **Given** the same build with zero critical chance, **When** the opener runs, **Then** no proc-fired event is attributed to the sigil and the result equals the no-sigil result within the stated tolerance.
3. **Given** the sigil's cooldown and a burst of critical hits inside it, **When** the opener runs, **Then** the sigil fires at most once per cooldown and the skipped attempts are visible in the trace.
4. **Given** two runs of identical inputs, **When** both complete, **Then** their results are identical.
5. **Given** the diagnostic trace is requested, **When** the opener runs, **Then** the trace shows per-seed proc firings and reports the mean and spread of the proc count across the seeds, while the ranking result stays the expected-value figure.

---

### User Story 2 - Swapping weapons swaps sigils (Priority: P2)

As a player who runs different sigils on each weapon set, I want the simulation to fire the sigils of the set I am currently holding, so that a swap-based rotation is scored with the sigils it really uses.

**Why this priority**: Every WvW build with two sets is affected, and Sprint 1 showed the reverse gap is already correct (a stowed sigil never fires), so only the swap-in direction is missing.

**Independent Test**: Put a supported on-hit proc only on set 2, run an opener that swaps after the first hit; the proc fires only after the swap, and moving the same sigil to set 1 makes it fire only before the swap.

**Acceptance Scenarios**:

1. **Given** a proc sigil socketed on set 2 only, **When** the opener lands hits before and after a weapon swap, **Then** proc-fired events for that sigil occur only after the swap event in the trace.
2. **Given** a proc sigil on set 1 only, **When** the same opener runs, **Then** its events occur only before the swap, and none after.
3. **Given** the same sigil on both sets, **When** the opener runs, **Then** its cooldown carries across the swap and it does not fire twice inside one cooldown because the set changed.
4. **Given** the coverage line for a build with set-2 sigils, **When** the build is evaluated, **Then** set-2 sigils that have a supported record are not listed as not simulated.

---

### User Story 3 - Conditional bonuses apply only when their condition holds (Priority: P3)

As a player using a rune or relic whose bonus depends on state (health above a threshold, a recent action, a stack count), I want the bonus applied only during the ticks where the condition is true, so that a build that keeps the condition true is rewarded and one that does not is not.

**Why this priority**: Today these bonuses are either permanent or absent, so two builds that differ exactly in whether they satisfy the condition score the same. It is the first "conditional effect" the plan asks CONN-03 C to specify fully: prerequisite, source and target, stacking, activation and expiry, cooldown, and applicable modes.

**Independent Test**: Run the Reaper fixture with the health-threshold rune in a scenario where the player stays above the threshold and in one where incoming damage takes them below it; the modifier is present in the first and drops out at the crossing in the second, visible in the trace.

**Acceptance Scenarios**:

1. **Given** a health-threshold bonus and a player who stays above the threshold, **When** the opener runs, **Then** every landed strike carries the bonus and the trace shows it active from the start.
2. **Given** the same bonus and incoming damage that crosses the threshold at a known time, **When** the opener runs, **Then** strikes after the crossing do not carry the bonus and the trace shows the deactivation at that time.
3. **Given** a stacking bonus with a maximum, **When** the triggering action repeats past the maximum, **Then** the stack count stops at the maximum and expires after its stated duration.
4. **Given** a conditional source whose numbers the record leaves unresolved, **When** it is evaluated, **Then** the source stays on the coverage line as not simulated rather than being executed with an invented number.

---

### User Story 4 - A dark field combo produces its effect (Priority: P4)

As a Reaper player, I want the whirl and leap finishers in my own dark fields to produce their combo outcome in the simulation, so that a rotation that stacks fields and finishers is scored for the interaction it creates.

**Why this priority**: The fixture already detects the combo and counts it as degraded; the only missing piece is the dark arm's outcome. It is the smallest of the four and validates that combo arms can be added one at a time.

**Independent Test**: Run the opener with the dark field and whirl finisher in sequence and with the field removed; the combo outcome appears in the trace only in the first run and the combo is no longer counted as degraded.

**Acceptance Scenarios**:

1. **Given** a dark field active and a whirl finisher cast inside it, **When** the opener runs, **Then** the trace records the combo and its outcome and the report's degraded-combo count for it is zero.
2. **Given** the finisher cast after the field has expired, **When** the opener runs, **Then** no combo is recorded.

---

### User Story 5 - The first verified cases are real, not fixture values (Priority: P5)

As a player, when the mechanic executes, I want the numbers behind it to come from a recorded, dated game source, so that the improvement reaches my real build and not only the test fixture.

**Why this priority**: Sprint 1 forbade copying fixture values into the shipped data. A firing site with no real record behind it changes nothing for a player.

**Independent Test**: Evaluate a real Reaper WvW build (from the cache, not the fixture) that carries the on-crit sigil, the health-threshold rune and the stacking relic; the three appear as fired or active in the trace and are absent from the coverage line.

**Acceptance Scenarios**:

1. **Given** the shipped WvW effect data, **When** a maintainer looks up each source this sprint made executable, **Then** it has a record with a source reference and a source date, and its evidence level says how the number was obtained.
2. **Given** a number the source does not state, **When** the record is written, **Then** the field is left unresolved and the source remains on the coverage line, never filled with a guess.
3. **Given** the record scope (the on-crit sigil, the health-threshold rune, the stacking relic, the Reaper traits the fixture uses), **When** a build from another profession is evaluated, **Then** its sources of the same kinds stay on the coverage line until the data sprint records them.

---

### User Story 6 - Life force governs shroud (Priority: P6)

As a Necromancer player, I want the simulation to build life force from my skills and spend it while I am in shroud, so that a rotation that enters shroud without enough life force is rejected and a shroud-heavy build is scored on the time it can really stay in shroud.

**Why this priority**: Shroud is the Necromancer's core mechanic and the fixture's opener enters it. Today shroud is an ordinary cooldown skill with unlimited uptime, which overstates every Necromancer build.

**Independent Test**: Run the fixture opener with its life-force-generating skills removed; shroud entry is refused and the trace names the shortfall. Restore them; shroud enters, drains over time, and exits when life force runs out or the exit skill is used.

**Acceptance Scenarios**:

1. **Given** a skill whose facts state life force gain, **When** it is used, **Then** the simulated life force rises by that amount, capped at the maximum, and the trace records it.
2. **Given** life force below the entry floor, **When** the opener tries to enter shroud, **Then** entry is refused, the shroud skills are not pressed, and the result carries a reason a player can read.
3. **Given** shroud entered with a known amount, **When** time passes and shroud skills are used, **Then** life force drains at the stated rate plus the skills' stated costs, and shroud ends when it reaches zero.
4. **Given** each Necromancer specialisation's shroud, **When** the same rules run, **Then** entry, drain and exit follow the one shared shroud shape with that specialisation's stated numbers and its one or two changed mechanics taken from the facts, with no separate model per specialisation.
5. **Given** a life force number the facts do not state, **When** the rule is built, **Then** the value stays unresolved, the resource is reported incomplete for that build, and no number is invented.

---

### Edge Cases

- A proc record with a cooldown of zero must still fire at most once per landed hit.
- A multi-hit skill that crits on some hits and not others must attribute each proc to a hit, not to the cast.
- A weapon swap during a channel: the set changes at the swap, hits already scheduled keep the sigils they were scheduled with.
- A conditional bonus whose condition is already true at the start of the fight activates at time zero without a triggering event.
- A conditional bonus whose threshold is crossed twice (below, then healed above) re-activates on the way back up.
- A combo finisher inside two overlapping fields resolves by the game's field priority; when that priority is not recorded, the older field wins and the choice is logged.
- A source whose record exists but whose trigger kind is still unsupported must remain on the coverage line as before; no source may silently move from "not simulated" to "executed" without a record of a kind this sprint added.
- The diagnostic trace stays bounded and truncation is indicated; the added events must not push the fixture opener over the existing cap.
- Life force gained while already in shroud, or at the cap, is discarded, not banked.
- Leaving shroud by force (life force reaching zero) mid-channel cancels the channel's later hits, the same way an interrupt does.
- Every specialisation's shroud follows the same shape (entry floor, drain, skill costs, exit at zero); a specialisation changes only individual numbers or one mechanic (for example a different drain rate, a health substitution, or a cost paid on use instead of over time), and those differences come from its facts rather than from a separate model per specialisation.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: In WvW, an on-crit proc record MUST fire from landed hits as an expected value (crit chance times effect, cooldown applied to the expected arrival rate) in search and ranking, and MUST respect its cooldown.
- **FR-001a**: The diagnostic trace MUST additionally run seeded deterministic trials for on-crit procs and report the per-seed firings with the mean and spread of the proc count; the seeds are fixed so the trace is reproducible, and the trials never run during search or ranking.
- **FR-014**: Life force MUST be a simulated resource for every Necromancer specialisation: generated from skill facts, capped at the stated maximum, required above the stated floor to enter shroud, drained at the stated rate and by shroud skill costs while in shroud, and ending shroud at zero. A rotation that cannot enter shroud MUST report that as a readable reason. Unstated numbers stay unresolved and mark the resource incomplete for that build.
- **FR-015**: Once life force is modelled, the Necromancer MUST be reported as resource-complete by the same rule that covers the other modelled resources, derived from the rules present rather than from a fixed list of professions.
- **FR-002**: The set of active sigil procs MUST follow the currently held weapon set: a swap loads the new set's sigils and unloads the old set's, with per-sigil cooldowns preserved across the swap.
- **FR-003**: A conditional bonus record MUST state its prerequisite, source and target, stacking rule and cap, activation and expiry, cooldown, and applicable modes, and the runtime MUST apply it only while the prerequisite holds.
- **FR-004**: Health-threshold prerequisites MUST be evaluated against the simulated player's health at the time of each affected event, including re-activation after the health returns across the threshold.
- **FR-005**: The dark field combo arm MUST produce its recorded outcome for whirl and leap finishers and MUST no longer be counted as degraded once it does.
- **FR-006**: Every mechanic this sprint adds MUST be guarded by a positive control, a negative control (unequipped, wrong mode, stowed set, condition false) and a timing control (cooldown, expiry, ordering) that fail when the mechanic is disabled, following the Sprint 1 seen-failing discipline.
- **FR-007**: Results MUST be deterministic: identical inputs give identical outputs on repeated runs, for every probability model, including seeded trials.
- **FR-008**: A source that becomes executable MUST leave the coverage line; a source with an unresolved number or an unsupported trigger MUST stay on it. No source may be executed with a fabricated number.
- **FR-009**: Records that make a real source executable MUST carry a source reference, a source date and an evidence level, and MUST NOT be copied from the test fixture.
- **FR-010**: Existing calibrated fixtures and thresholds MUST be unchanged; no scoring constant, gate or normalisation value may move in this sprint.
- **FR-011**: The default optimizer test run MUST grow by at most 10 seconds from this sprint's experiments; heavier variants (multi-seed spreads) run on demand.
- **FR-012**: The diagnostic trace MUST name the source for every new event kind (proc fired, proc skipped for cooldown, conditional activated or expired, combo resolved) and MUST remain bounded with truncation indicated.
- **FR-013**: This sprint MUST NOT change the prompt text, the model client, or the live acceptance example owned by the latency work, and MUST NOT push or release until that work is accepted in-game.

### Key Entities

- **Proc record**: an equipped source's trigger, effect, cooldown, evidence and applicable modes; this sprint adds the on-crit trigger to the executable set.
- **Conditional bonus**: a rune or relic effect with a prerequisite, stacking rule, activation and expiry; replaces the flattened permanent modifier for the sources it covers.
- **Active set**: the sigils loaded for the currently held weapon set, with cooldowns that persist across swaps.
- **Trace event**: a timestamped, source-attributed record of a hit, proc, activation, expiry, swap or combo, capped in length.
- **Coverage line**: the player-facing list of equipped sources the simulation did not execute; shrinks as sources become executable.
- **Life force**: the Necromancer resource with a maximum, an entry floor, generation from skill facts, a drain rate in shroud and per-skill costs; determines whether and how long shroud is available.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the Reaper WvW fixture, the four source kinds named in the problem statement (on-crit sigil, set-2 sigil, conditional rune or relic bonus, dark combo) each move from the coverage line to the trace, and the coverage line for the fixture build shrinks by at least those four names.
- **SC-002**: Every mechanic added has three controls (positive, negative, timing) that were each observed failing with the mechanic disabled before the edit, and all pass after.
- **SC-003**: A crit-heavy variant and a zero-crit variant of the same build produce different strike totals, and the direction matches the sigil's effect, for the on-crit case.
- **SC-004**: Repeated evaluation of the same build gives identical results 10 out of 10 times.
- **SC-005**: A real Reaper WvW build from the character cache shows the sources covered by the chosen record scope as executed, without any fixture data present.
- **SC-006**: The default optimizer test run finishes within 10 seconds of the Sprint 1 baseline, and no existing test changes its expected values.
- **SC-007**: A player reading the coverage line for a build that uses only executable sources sees no "Not simulated" entry for those sources.
- **SC-008**: For the fixture opener, removing every life-force-generating skill makes shroud entry fail with a readable reason, and restoring them makes shroud enter, drain and exit at the times the facts predict; the Necromancer no longer carries the "outside the bounded resource ledger" qualification.
- **SC-009**: The on-crit diagnostic trace reports a mean proc count within one proc of the expected-value figure for the fixture opener, with its spread stated.

## Assumptions

- The WvW timeline is the only runtime touched; PvE and PvP keep the adaptive scheduler and execute no records until CONN-01-03 has its own spec.
- The four firing sites are profession-agnostic by construction; the Reaper fixture is the proving ground, and a second profession is not required to close this sprint.
- The "(no record)" listing of executed weapon skills (CONN-01-06) stays as it is; only sources this sprint makes executable leave the coverage line.
- The enemy cooldown model for outgoing chill (CONN-00-08) and cancellation inside the referee run (CONN-00-11) stay deferred.
- Life force numbers (maximum, entry floor, drain rate, per-skill gains and costs) come from skill facts and the wiki at implementation time; where the fixture's synthetic skills need them, the fixture states them explicitly and the shipped data never copies fixture values.
- Release gating is unchanged from Sprint 1: commits stay local on this branch until free models answer and a player has produced a build with Choya on the latency DLL; then the addon ships with a patch bump after the user's in-game test.
- Numbers for new records come from the wiki or the official API at implementation time, dated in the record; the existing evidence-level vocabulary is reused unchanged.
- The Sprint 1 fixture, trace and experiment harness are reused; new experiments follow the existing seven-kind table in the audit.
