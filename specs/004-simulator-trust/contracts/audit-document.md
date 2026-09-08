# Contract: `docs/simulator-connection-audit.md` (FR-001 to FR-003, FR-006, FR-007)

## Sections, in order

1. **Baseline**: commit, branch, dirty paths (the latency work's files listed by name), workspace version, active data manifest id (`2026-01-13`), scenario used, no credentials, no character or account names.
2. **Inventory**: existing fixtures, calibration examples, consistency tests and published-build fixtures, each with path and what it covers; explicitly notes what does not exist for Reaper.
3. **Findings**: table with the columns of data-model.md "Finding". Ids `CONN-0P-NN`. Hypotheses marked `hypothesis` in the classification column until an experiment moves them.
4. **Trace matrix**: the 8 × 8 matrix for the Reaper slice (rows and columns as in data-model.md), each cell `class` plus pointer.
5. **Open questions answered**: one line per question from `docs/simulator-trust-plan.md` Phase 1 (weapon swap and stowed sigils, passive double counting, on-hit semantics, support boons, starved non-damage skills, gate/flow/tooltip/tool parity, warning survival), each `answered: …` with evidence or `left open: …` with reason.
6. **Experiments**: kind, test name, file, verdict, finding served.
7. **Remedy**: the one remedy applied, its failing-then-passing experiment, commit id, cross-reference; or "none demonstrated" with the refutations.

## Rules

- Never renumber or reopen a `W###` finding; link to it from the `links` column.
- No count of equipped sources is presented as a percentage of mechanics implemented.
- Every `missing` cell and every `unconfirmed` finding states a reason.
- `Verified`, `Provisional`, `Blocked` are used only with their `DataQuality` meaning.
