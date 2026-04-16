# Architecture Decision Records (ADRs)

This directory records significant architecture and design decisions for GW2 Build Optimizer using the [Michael Nygard ADR template](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions).

## Why ADRs

ADRs capture **why** a decision was made, not just what was decided. When future-you (or a future contributor) wonders "why is `WEIGHT_BUDGET` exactly 2.0?" or "why is Gemini's gear prefix overwritten by cosine-sim?", an ADR answers it instead of forcing code archaeology.

## When to Write One

Write an ADR when you make a decision that:

- Changes the optimization pipeline contract (tier order, tier added/removed)
- Adds, removes, or restructures an LLM provider in the `LlmClient` trait
- Changes empirically tuned constants (`STRIKE_DPS_NORM`, `WEIGHT_BUDGET`, etc.)
- Alters the GW2 API rate-limiter, caching, or download strategy
- Changes the global state model (`Mutex<Option<AddonState>>`, `with_state` pattern)
- Touches config schema (`AppConfig`, persisted JSON shape)

If you can answer the change with a one-line code comment, you don't need an ADR.

## Filename Convention

`NNNN-short-kebab-title.md` — zero-padded sequence, e.g. `0001-llm-client-trait.md`.

## Template

```markdown
# NNNN. Title

- **Status:** Proposed | Accepted | Deprecated | Superseded by ADR-XXXX
- **Date:** YYYY-MM-DD

## Context

What is the issue we're seeing that motivates this decision? Cite specific files, constants, gotchas, or bug reports.

## Decision

What we decided to do, stated clearly.

## Consequences

Positive, negative, and neutral outcomes. Include trade-offs accepted.
```

## Index

_(empty — add entries below as ADRs are written)_

| # | Title | Status | Date |
|---|-------|--------|------|
| | | | |
