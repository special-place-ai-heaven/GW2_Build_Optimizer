# Implementation Plan: Audit remediation

User override (2026-09-05): work in small sprints and commit often. Verified local sprint commits may precede in-game acceptance, superseding wait-before-commit wording below. Stage only owned changes; another agent is actively modifying this shared tree. No push/release is implied.
Date: 2026-09-05 | Feature context: 003-audit-remediation | Git branch: main (existing dirty tree preserved)
Spec: [spec.md](spec.md) | Ledger: [ledger.md](ledger.md) | Tasks: [tasks.md](tasks.md)

## Summary
Resolve all 267 actionable report entries plus B001 through five risk-ordered stories. Start with data-loss, host-panic and reserved-address defects, restoring strict CI as their verification prerequisite. Then fix incorrect build/stat/data behavior before S2 structural debt, S3 and S4. The audit's highest label is S2; several S2 runtime failures deserve higher execution priority than its duplicated-renderer findings.

## Technical Context
Rust 2021, cargo/rustc 1.97.1; Windows MSVC Nexus cdylib and separate Axum/SQLx feedback service. Main workspace crates: addon, core, gw2api, optimizer. Dependencies: serde/serde_json, reqwest 0.12, image with restricted features, rodio, pinned stream-download 0.22.9, tokio for radio. Storage: local atomic JSON caches/config/builds/history; feedback service SQLite.
Testing: Cargo unit/integration tests and strict Clippy; manual in-game ImGui and injected-DLL acceptance. Performance: keep disk/network/DNS off render thread, avoid extra candidate evaluations, preserve cancellation. Scope: 268 tracked actionable/observed entries across addon, optimizer, API, core, data, docs, locales and server.

## Constitution Check
The constitution file is an unfilled template, so it supplies no ratified gates. Pre/post-design gates: preserve secrets and existing changes; use SymForge; use Terminal Commander for long work; preserve report exclusions/calibrated math; no local Docker; no audit commits; no commit/push/release before user in-game acceptance. All design choices meet these gates. Optional git commit hooks are left unexecuted under the existing release rule.

## Project Structure
- specs/003-audit-remediation/: spec.md, plan.md, research.md, data-model.md, contracts/remediation.md, quickstart.md, ledger.md, tasks.md, verification.md
- crates/core/src/feedback/store.rs and crates/addon/src/feedback/: safe history load/flush lifecycle
- crates/addon/src/ui/main_view/lock_panel.rs: bounded Lock All mutation
- crates/addon/src/radio/ and news_art.rs: shared reserved-address guard
- crates/addon/src/ui/main_view/{chat_flow,optimization}.rs: validated Choya projection
- crates/optimizer/src/: data provenance, tool/referee correctness, policy consolidation
- server/feedback/: independent manifest and test gate

## Execution and dependency policy
1. Setup: inventory complete report, preserve worktree baseline, establish ledger/acceptance contracts.
2. US1: W011 → W006 → W002, then W012 (same persistence path) and W001 (strict verification prerequisite). First release-sized batch.
3. US2: B001; invalid-prefix simulation W025→W035; active data W044→W013→W015/W016→W019; other listed runtime correctness fixes. Dependencies override numeric IDs.
4. US3: remaining S2 refactors/dead-code cleanup, preserving semantics. Shared parsing core W032 before rune projections W026/W043; OpenAI tool loop W030 before shared retry W027.
5. US4: remaining S3 ordered by explicit ledger dependencies and file ownership.
6. US5: S4, contested adjudication, deliberate exclusions, final documentation/coverage reconciliation.

Every later-story prerequisite of an earlier item is pulled forward explicitly with its dependent rather than silently executing tasks out of order. Duplicate groups retain all report IDs and share evidence. Do not run all lower-risk edits before verifying a runtime batch.

## Verification and release
Use quickstart.md. Regression tests precede behavioral fixes; update ledger/task status immediately after checks. Keep an accurate count of implemented, verified, refuted, retained and pending. Patch bump once per coherent batch; build release DLL and stage local in-game test. Project's user-test gate is a release checkpoint, not campaign completion. No external release or server deployment is authorized by this plan.

## Risks and implementation decisions
- W011 needs state across fresh per-flush FeedbackStore instances; object-local loader flags alone fail.
- W002 fix addresses literal normalization and duplicated policy; do not claim it solves all DNS rebinding without connection pinning analysis.
- B001 accepted gate already uses validated stats; preserve that ranking path and correct its display projection and surfaced warnings.
- Data patch gaps cannot be fixed by copying historical data under a newer date without source evidence.
- Legacy saved-build compatibility and explicit no-action findings must be adjudicated, not blindly deleted.
- Existing LLM/chat/scraper edits overlap later remedies. Use current diff/source before each edit and preserve unrelated changes.

## Parallel opportunities
Spec Kit Phase 0 independent research used one agent for B001 while primary performs persistence/address triage. Read-only diagnostics may run concurrently; Cargo commands sharing target output serialize. US1 storage and address regression design are independent; US2 patch-data and parser research are independent; US3 renderer and engine reference analysis are independent; US4 server/doc and UI analysis are independent; US5 disputed finding checks are independent. Mutations sharing files run sequentially. This describes scheduling opportunities, not blanket authorization to spawn further agents.

## Complexity Tracking
No added production framework or data format. Use existing storage replace primitive, state error channels and validated stat calculator.
