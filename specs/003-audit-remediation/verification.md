# Sprint verification

## Sprint 1 — ledger and plan

Commit: 0496b0b. Verified 273 unique tasks, 268 unique finding entries; git diff --cached --check passed. Specs are locally ignored, so only the eight named feature artifacts were explicitly force-added. Original docs/audit remains uncommitted.

## Sprint 2 — W011/W012 feedback history

- W011: fallible history load distinguishes NotFound from read/parse failure. Addon feedback state retains the load error across fresh FeedbackStore instances; dirty flushing refuses publication after failed load, including when the file later becomes readable. Error is visible through state and Nexus log.
- W012: feedback and taxonomy writes use the existing Windows-safe storage::replace_file routine.
- Red proof: Terminal Commander job job_01a0731133ea79c18ccf68fce1b268d4 ran the new addon regression: 1 failed, exit 101. Initial exact-name invocation matched zero tests and is not verification evidence.
- Green: core feedback store suite 7 passed (job_01a07312ef5e7d519105dd7df963d05c); addon feedback suite 60 passed (job_01a0731289f67fb3a9f596331e7e012b). Tests cover parse/read failure, first run, interrupted sends, repeated overwrite, taxonomy overwrite and session flush refusal.
- Scoped formatting and git diff --check pass. Strict core Clippy result is recorded in the sprint commit after completion.
- Limitation: a failed history load intentionally disables history writes until addon reload. Repair/recover the file before reloading; in-session new feedback state is not persisted while refusal is active.
- Workspace Clippy remains blocked by W001's pre-existing diagnostics; in-game acceptance and release build remain pending. These are scoped verified remedies, not campaign completion.
- Other agent owns concurrent Cargo version/lockfile and LLM/chat/scraper changes; they are excluded from this sprint commit.
