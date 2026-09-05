# Research and triage decisions

User override (2026-09-05): work in small sprints and commit often. Verified local sprint commits may precede in-game acceptance, superseding wait-before-commit wording below. Stage only owned changes; another agent is actively modifying this shared tree. No push/release is implied.
Baseline: main at 2d664900259e4b02b9e25f93ec7238e295641af8, workspace 1.11.27. Report describes 1.11.26, so its line numbers are historical.

## Current evidence
- SymForge index refreshed explicitly: 268 files / 22,585 symbols. Audit report exceeds the indexing threshold; exact document parsing is the declared fallback.
- FeedbackStore::load returns an empty MessagesFile after read/parse failure. ensure_loaded copies it into state; flush_dirty creates a fresh FeedbackStore and writes it. A guard only stored on the original store object would be insufficient.
- render_lock_panel's Lock All loop indexes locks.specs from an unbounded current_specs iterator. Its trait-column loop is already bounded by a three-element array and guarded nine-trait slices.
- stream_host_reserved parses raw Url::host_str; logos::normalized_host strips IPv6 brackets. Share normalization/address policy while preserving worker-only DNS.
- attach_chat_stats estimates a full set from suggestion.stat_prefix and does not consume suggestion.slot_prefixes. Independent research traces the validated ranking/publication path before B001 changes.
- cargo clippy --workspace --all-targets --message-format=short -- -D warnings exited 1 with ten diagnostics matching W001 (scraper line moved to 1680).

## Decisions
Decision: prioritize W011 (data destruction), W006 (host panic), W002 (address guard), then restore CI and correct B001.
Rationale: audit S2 conflates harmful runtime faults with large refactors. Keep original severity immutable and add execution priority.
Alternative rejected: numeric ID order or treating every S2 as equal.

Decision: preserve history on a failed read and carry refusal through the addon load/flush lifecycle; use storage::replace_file for feedback atomic publication (W012).
Rationale: fresh store instances are created per flush; a transient unreadable file can later become readable, so checking current readability alone does not protect against a previously empty snapshot.
Alternative rejected: merely logging and continuing, or a guard held only by the discarded loader.

Decision: keep research, source findings, and implementation status separate.
Rationale: six contested items and eleven explicit no-action/conditional proposals cannot honestly be marked fixed from the report alone.
Alternative rejected: deleting documented compatibility and calibrated mechanisms to make a checklist green.

Decision: choose minimum behavior-preserving consolidation for duplicate parsers/renderers; for data fields that promise real behavior, wire their intended consumer with regression coverage or explicitly remove the unsupported promise after current-call analysis.
Rationale: duplicate removal must not rebalance builds or remove user capabilities.
Alternative rejected: automatic dead-code deletion based solely on public reference counts.

## Existing user modifications
Preserve chat_flow.rs, llm/anthropic.rs, llm/openai.rs, llm/openai_compat.rs, llm/openrouter.rs, and scraper.rs. Read overlapping diffs before editing. docs/audit is untracked and must remain uncommitted.

## Governance
.specify/memory/constitution.md is an unfilled template, not ratified policy. This plan follows the explicit user instructions, AGENTS.md and concrete project constraints; it does not invent constitution rules. Optional speckit.git.commit hooks are not executed because the project requires in-game acceptance before commits.
