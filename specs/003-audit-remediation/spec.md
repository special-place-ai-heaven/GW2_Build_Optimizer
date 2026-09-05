# Feature Specification: Audit remediation

User override (2026-09-05): work in small sprints and commit often. Verified local sprint commits may precede in-game acceptance, superseding wait-before-commit wording below. Stage only owned changes; another agent is actively modifying this shared tree. No push/release is implied.
Date: 2026-09-05 | Feature: 003-audit-remediation | Status: implementation authorized

## Objective and scope
Remedy every substantiated finding in docs/audit/AUDIT-REPORT-2026-09-05.md in descending risk order. Preserve W001–W267, R001–R003, and the additional section 4b Choya bug (B001). Report verdicts are leads, not proof against this working tree. See ledger.md for each claim, proposed remedy, decision, dependencies and closure evidence.

## User stories
### US1 — Preserve player data and keep the host stable (P1)
As a player, malformed cache input must not crash my game, an unreadable feedback file must not be replaced by empty history, and community radio URLs must pass the same address restrictions.
Acceptance: malformed and temporarily unreadable history stays byte-identical across load/flush; missing history supports a first save; excess specialization input is bounded; reserved IPv4/IPv6 literals are rejected consistently. CI's strict Clippy gate passes.
### US2 — Trust builds, stats and calculations (P2)
As a player, displayed stats and ranking must describe the validated per-slot kit, tool failures must be explicit, and patch/data coverage gaps must be visible.
Acceptance: a mixed-prefix kit yields the same plated stats as the canonical validated calculation; invalid prefixes never fabricate DPS; mode-specific simulation uses actual inputs; data-quality failures cannot silently become verified results.
### US3 — Have one implementation of each critical policy (P3)
As a maintainer, S2 duplication and dead production APIs must have one verified resolution without changing calibrated gameplay behavior.
Acceptance: callers use the chosen implementation; removed APIs have no production consumers; existing behavior tests pass; preserved compatibility remains tested.
### US4 — Clear the remaining S3 debt (P4)
As a maintainer/player, diagnostics, persistence, localization and UI behavior remain consistent.
Acceptance: every assigned ledger entry has implementation or a current-code refutation with evidence; affected crate and server checks pass.
### US5 — Resolve S4 and contested findings (P5)
As a maintainer, stale prose, cosmetic issues and deliberate deferrals have explicit dispositions.
Acceptance: each contested claim is independently adjudicated; code fixes are verified; deliberate project exclusions are retained with rationale and reconsideration trigger, never claimed as code fixes.

## Requirements
- Preserve all original IDs and distinguish audit verdict, current verification, implementation and in-game acceptance.
- Re-check exact symbols and callers with SymForge before each edit; refresh file impact afterward.
- Use Terminal Commander for long/noisy commands.
- Write regression tests for observable corruption, panic, security and calculation failures; avoid source-string tests as sole behavioral evidence.
- Process risk before cosmetics; permit lower-severity prerequisites in the same batch only when dependencies are explicit.
- Preserve existing user edits and audit artifacts. No secrets in chat, generated artifacts or commits.
- Preserve calibrated scoring constants, lenient GW2 fact serde, HP/armor distinctions, manual comma queries, image feature limits, dependency pin and cancellation/panic containment.
- Do not remove saved-build compatibility or delete historical data solely because it lacks current runtime callers.
- No local feedback Docker build/load. Server is a separate Cargo workspace.
- Each executable batch ends with verification, patch bump, release build and local DLL handoff where applicable. In-game acceptance precedes commit/push/release under CLAUDE.md.

## Success
All 268 actionable/observed entries have a final evidence-backed disposition, all code remedies pass relevant checks, and the user accepts the in-game changes. Pending investigations, retained risks and untested changes do not count as fixed.
