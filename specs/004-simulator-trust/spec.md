# Feature Specification: Simulator trust

**Feature Branch**: `004-simulator-trust`

**Created**: 2026-09-07

**Status**: Draft

**Input**: User description: "docs/simulator-trust-plan.md" (Simulator connections and build-quality plan, phases CONN-00 to CONN-06). This specification covers Sprint 1 of that plan: CONN-00, CONN-01 and CONN-02, the full-build evaluation tool for Choya from CONN-03 A, plus the smallest demonstrated CONN-03 remedy if Sprint 1 produces one. Later phases get their own specification once this sprint's evidence exists.

## Problem

The addon already has a simulator, a WvW timeline, a referee and a search. A build can come out of them as a valid plate and still be useless: a relic whose trigger the simulator never fires, a sigil counted while stowed, a synergy the search cannot reach, a coverage warning that never reaches the player. Nobody can currently say, for a given build, which of its mechanics were actually executed, which were approximated, and which were silently ignored. Until that chain is inspectable, adding mechanics or engines is guesswork.

## Clarifications

### Session 2026-09-07

- Q: If the first demonstrated break is that on-crit sigil procs never fire, does the sprint's smallest remedy include implementing on-crit procs? → A: No. Reporting only: the build shows an explicit "on-crit procs not modeled" qualification everywhere; implementation goes to the CONN-03 C spec.
- Q: When the sprint's remedy changes addon code, does it ship (version bump, DLL, in-game test) before the sprint closes? → A: Only if the free-model and Choya work in flight is accepted in-game first, meaning free models work and a player can produce a build with Choya. Then the remedy ships with that release: bump, DLL, push and release. Otherwise it stays as local commits.
- Q: Choya's simulation tool is skill-only; is "no full-build AI wrapper yet" an acceptable finding for the parity check, or must this sprint add a full-build evaluation tool? → A: Add it now. Parity in this sprint includes Choya's full-build evidence; CONN-03 A is in scope.
- Q: How much time may the causal experiments add to the default test run? → A: At most 10 seconds added to the default optimizer test run; heavier experiments are marked ignored and run on demand. All experiments run locally with no model involved; the model only receives the computed verdict for choosing and explaining.
- Q: Must the "not modeled" qualification for the unsupported trigger reach the overlay screen in this sprint? → A: Yes: one plain-language line on the build's existing quality marker. The fuller reason categories and their layout stay in CONN-03 B.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A maintainer can see what the simulator really does (Priority: P1)

As the maintainer, I want one audit document that records, for the current tree, the difference between the mechanics the data vocabulary can describe and the mechanics the runtime actually executes, with a reproduction for every finding, so that later work fixes demonstrated breaks instead of suspected ones.

**Why this priority**: Every later phase depends on knowing which gaps are real. The plan forbids speculative engine work; this story is what makes that rule enforceable.

**Independent Test**: Open the audit document, pick any finding, follow its reproduction steps on the recorded baseline, and observe the stated behavior. Delivers a trustworthy list of real gaps and refuted suspicions.

**Acceptance Scenarios**:

1. **Given** the recorded baseline (commit, dirty paths, data manifest, mode and source snapshots), **When** a reader follows a finding's reproduction, **Then** they observe the documented behavior without extra setup or private data.
2. **Given** a suspected gap that could not be reproduced, **When** the reader looks it up, **Then** it is marked as unconfirmed or refuted, never as fixed or as a defect.
3. **Given** the existing audit remediation ledger, **When** a new finding relates to an old one, **Then** it links to the old ID under its own CONN identifier without renumbering or reopening the old finding.

---

### User Story 2 - One build's mechanics are traced end to end (Priority: P2)

As the maintainer, I want one WvW scenario and one build family traced through every stage, from the player's constraints to what the screen shows, with each selected mechanic classified at each stage, so that a gap can be located to a stage rather than to "the simulator".

**Why this priority**: A stage-by-stage map turns a vague complaint ("the build is bad") into a locatable defect (data, runtime, search or reporting). It is the input to every causal test in Story 3.

**Independent Test**: Read the trace matrix; for any cell marked exact or approximate, find the named observable event or value it points to; for any cell marked missing, find the stated reason.

**Acceptance Scenarios**:

1. **Given** the first slice (Reaper in a fixture-supported WvW scenario, plus a small Reaper PvE comparison), **When** the matrix is complete, **Then** every row (a selected skill, trait, rune, sigil, relic, resource rule and combo) has a cell for every stage (intent, parsing, data selection, derived parameters, rotation preparation, runtime, evaluation, exposure), each classified as exact, approximate, missing, intentionally inapplicable or not yet checked.
2. **Given** the paths the plan lists (equipped build, deterministic Optimize and Improve, Choya proposal, imported published build, comparison display, each simulation and scoring tool), **When** a reader asks which code path a cell describes, **Then** the matrix names the actual caller, not a similarly named function.
3. **Given** the plan's open questions (weapon swap and stowed sigils, passive double counting, meaning of "on hit", support boons mistaken for self uptime, starved non-damage skills, assumption parity between gate, flow, tooltip and AI tools, survival of warnings into Choya and the UI), **When** the trace is done, **Then** each question is answered with evidence or explicitly left open.

---

### User Story 3 - Causal experiments separate real gaps from noise (Priority: P3)

As the maintainer, I want a small set of behavioral experiments that prove a supported mechanic changes a downstream event, that an unequipped or wrong-mode source cannot, that timing rules hold, and that identical inputs give identical results through every full-build path, so that each suspected gap is classified as a data gap, a runtime gap, a search gap or a reporting gap.

**Why this priority**: Enum membership and copied arithmetic prove nothing. Only an experiment that would fail if the logic broke tells the next sprint what to fix.

**Independent Test**: Run the experiment set in the normal test suite; each experiment names the event it expects and fails when that event does not occur.

**Acceptance Scenarios**:

1. **Given** a currently supported boon or proc, **When** it is equipped versus not equipped in an otherwise identical pinned action sequence, **Then** the named downstream event differs in the expected direction (positive control), and the unequipped or wrong-mode variant shows no difference (negative control).
2. **Given** a proc with a cooldown, an interruptible channel, and a buff activated after a hit, **When** the sequence runs, **Then** the proc fires at most once per cooldown, the interrupted channel loses its later hits, and the late buff does not improve the earlier hit.
3. **Given** a complete mechanism, a variant missing its enabler and a variant missing its payoff, **When** all three are evaluated, **Then** the expected event difference is stated before the score direction is asserted, and capped effects are allowed to show no gain when prerequisites are already met.
4. **Given** one known unsupported trigger, **When** a build using it is evaluated, **Then** the result carries an explicit incomplete-coverage qualification and never a verified zero benefit.
5. **Given** one complete build and scenario, **When** it is evaluated directly, through the deterministic Optimize path, through the display, and through Choya's full-build tool, **Then** the canonical metrics match within a stated tolerance, a selected trait or upgrade mutation visible in direct evaluation is visible through the tool, and the skill-only rotation tool is labelled with its narrower scope.
6. **Given** an experiment that needs diagnostics existing reports cannot supply, **When** the diagnostic mode is on, **Then** output is bounded, truncation is indicated, and nothing is emitted during normal search or into AI prompts.

---

### User Story 4 - The smallest demonstrated break is fixed (Priority: P4)

As a player, when Sprint 1 demonstrates one break in the chain for the first slice, I want the smallest remedy for it, guarded by the failing experiment that demonstrated it, so that the sprint ends with a usable improvement and not only a report.

**Why this priority**: The plan's own instruction is to deliver evidence and a usable incremental result each sprint. This story is conditional: if Sprint 1 refutes every suspected gap, it is skipped and the refutation is the result.

**Independent Test**: The experiment that failed before the remedy passes after it, every calibrated fixture still passes, and the remedy is cross-referenced to its audit finding.

**Acceptance Scenarios**:

1. **Given** a demonstrated break, **When** the remedy lands, **Then** its regression experiment was failing before and passes after, and no calibrated fixture or threshold changed without independent evidence.
2. **Given** no demonstrated break, **When** the sprint closes, **Then** the audit records the refutations and no code changed for this story.
3. **Given** a remedy that changes addon code, **When** the in-flight Choya and free-model work has been accepted in-game (free models answer and a player produces a build with Choya), **Then** the remedy ships in that release with a version bump, built DLL, push and release; **otherwise** it stays as local commits and the sprint still closes on its passing experiments.

---

### Edge Cases

- A finding that depends on a scheduler choosing differently between runs: adaptive scheduling is tested separately from pinned sequences so the two cannot confound each other.
- Saturated effects: a mechanic already at cap may legitimately show zero incremental gain; the experiment must not read that as a defect.
- Timing and rounding: float tolerances are justified from timing granularity and rounding, not chosen after seeing a result.
- The working tree is shared with in-progress provider and latency work: baseline recording must list dirty paths, and this feature must not edit those files. The full-build tool (FR-018) touches the tool list and prompt text that work is measuring; it lands on this feature's branch and is merged only after the latency experiment has closed, so neither invalidates the other's measurements.
- The external comparison simulator disagrees with ours: that is a question to record, not proof either way; it is not a dependency.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The audit document MUST record the baseline (commit, dirty paths, versions, active data manifest, mode and source snapshots) with no credentials or personal account data.
- **FR-002**: The audit document MUST hold a findings table with, per finding: CONN identifier, exact source evidence, observed behavior, affected modes and callers, reproduction, expected behavior, proposed remedy and status, with hypotheses marked as such.
- **FR-003**: New findings MUST use local CONN identifiers and link to existing audit findings rather than renumbering them or counting against the original audit total.
- **FR-004**: Existing fixtures, calibration examples, consistency checks and published-build fixtures MUST be inventoried and reused before any new fixture is added.
- **FR-005**: The first slice MUST be one WvW scenario already supported by fixtures, one build family with a meaningful mechanic, and one small PvE comparison; profession coverage MUST NOT widen before the slice is complete. The first slice is Necromancer Reaper in WvW: shroud, chill uptime, on-crit sigil procs and strike damage modifiers, with a small Reaper PvE comparison to expose path differences.
- **FR-006**: The trace matrix MUST cover the eight stages and the seven mechanic rows named in Story 2, with every cell classified as exact, approximate, missing, intentionally inapplicable or not yet checked.
- **FR-007**: The trace MUST name actual callers per path and MUST NOT present an equipment-source count as a percentage of mechanics implemented.
- **FR-008**: Each experiment in Story 3 MUST be a behavioral test that fails when the mechanic's observable event is absent; tests that only check vocabulary membership or restate implementation arithmetic do not count.
- **FR-009**: Event-level experiments MUST pin an explicit action sequence; adaptive scheduling MUST be tested separately.
- **FR-010**: The unsupported-trigger control MUST surface as an explicit incomplete-coverage qualification at every projection and MUST NOT be scored as zero benefit. In this sprint the projections are: the evaluation result, Choya's evidence (FR-018), and the overlay, where it appears as one plain-language line on the build's existing quality marker (for example "on-crit sigil procs are not modeled in this comparison"). Reason categories, stable effect identity and their layout are CONN-03 B and out of scope.
- **FR-011**: Cross-path parity MUST be checked for one complete build and scenario across direct evaluation, the deterministic Optimize path, the display, and Choya's full-build evaluation tool (FR-018), with tolerance stated and the skill-only rotation tool labelled with its narrower scope.
- **FR-018**: Choya MUST be able to obtain the same full-build verdict the app computes: one tool call that takes a complete build (per-slot gear, both weapon sets, trait choices, rune, all sigils, relic, pets or legends, pins) and a scenario, and returns compact numbers, gate failures, coverage reasons and the most important causal changes, computed by the same validated-build, scenario and referee path the app uses. Existing scoring and simulation tools MUST be inspected first and one of them extended if it fits; a new tool is added only if none does. Omitted fields MUST have explicit semantics and MUST never silently substitute a different equipped build. The call MUST be bounded and cancellable, including for malformed or excessively long rotations. The skill-only rotation tool remains, labelled so its output cannot pass as a full-build verdict.
- **FR-012**: Any added diagnostic output MUST be optional, off during normal search, bounded, truncation-indicated, and never appended to AI prompts.
- **FR-013**: Every experiment's verdict MUST classify the gap as data, runtime, search or reporting, or record that it was refuted.
- **FR-014**: A remedy under Story 4 MUST be preceded by its failing regression experiment and cross-referenced to its finding; calibrated formulas, gates and thresholds MUST NOT change without independent evidence. A remedy in this sprint MUST NOT add a new trigger or operation to the simulator; if the demonstrated break is an unmodeled trigger (for example on-crit procs), the remedy is the explicit coverage qualification of FR-010 and the implementation is deferred to the CONN-03 C specification.
- **FR-015**: This feature MUST NOT introduce a replacement simulator, a port of the external comparison simulator, a second AI pipeline, an ID-heavy output contract or a generic graph framework.
- **FR-016**: This feature MUST NOT edit files owned by the in-progress provider and latency work and MUST record those paths as dirty in the baseline.
- **FR-017**: Scope: this specification covers CONN-00 to CONN-02, the full-build evaluation tool of CONN-03 A (FR-018), and one smallest demonstrated remedy. Coverage reasons in the UI (CONN-03 B), mechanic completion (CONN-03 C) and later phases get their own specification shaped by this sprint's findings.

### Key Entities

- **Finding**: one observed difference between described and executed behavior; carries a CONN identifier, evidence, reproduction, classification (data, runtime, search, reporting, refuted, unconfirmed) and status.
- **Trace matrix**: rows are the selected mechanics of the first slice, columns are the eight stages; each cell holds a classification and a pointer to the observable event or the reason for its absence.
- **Experiment**: a pinned scenario with an expected event, a control variant and a verdict; belongs to one of the seven experiment kinds (positive, negative, timing, ablation, interaction pair, unsupported control, cross-path parity).
- **Coverage qualification**: the statement attached to a result that names what was not modeled and why; must survive projection to the player.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of findings in the audit document have a reproduction that a second person can follow on the recorded baseline; none are marked fixed without a passing experiment.
- **SC-002**: The trace matrix has no cell left as "not yet checked" for the first slice at sprint close, and every "missing" cell has a stated reason.
- **SC-003**: All seven experiment kinds exist for the first slice, run in the normal test suite, and each fails when its mechanic is disabled (demonstrated at least once per kind during development). Together they add at most 10 seconds to the default optimizer test run; any experiment heavier than that is marked ignored and documented as on-demand.
- **SC-004**: The unsupported-trigger control is visible as a plain-language qualification in every place a player or the AI sees that build's result: the evaluation result, Choya's full-build evidence, and one line on the overlay's existing quality marker.
- **SC-005**: Direct evaluation, the deterministic Optimize path, the display and Choya's full-build tool agree on the canonical metrics for the parity build within the stated tolerance.
- **SC-006**: Zero calibrated fixtures regress, and zero thresholds change without a linked evidence note.
- **SC-007**: Each of the plan's open trace questions has a recorded answer or an explicit "left open" at sprint close.
- **SC-008**: If a break is demonstrated, the player-visible result for the first slice changes in the direction the experiment predicted; if none is demonstrated, the audit says so and no engine code changed.

## Assumptions

- The sprint's evidence, not this document, decides whether CONN-03 A, B or C is next; the plan's later phases are out of scope here.
- The external comparison simulator is an offline reference only; no production dependency on it.
- Existing "Provisional" quality behavior for unmodeled effects is preserved and extended, not replaced.
- "Verified" in the current UI means a software and data classification, not proven meta quality; wording is only clarified if the trace shows a player would confuse the two.
- Live provider calls are not needed for this sprint; every experiment runs locally against the game database and fixtures with no model involved. The model's only role is to receive the locally computed full-build verdict (FR-018) and choose or explain from it.
- Small local commits per verified step are allowed. A push or release of this sprint's code happens only together with the in-game acceptance of the in-flight Choya and free-model work; this document itself authorizes neither.
- The audit document lives at `docs/simulator-connection-audit.md`; findings use the `CONN-*` prefix from the plan.
- Reaper was chosen for the first slice because its on-crit sigils exercise the trigger the plan flags as unmodeled, and because Necromancer has existing fixtures. The exact WvW scenario is whichever fixture-supported scenario the CONN-00 inventory shows already covers a Reaper build; if none does, the closest existing scenario is used and the gap is recorded as a finding.
