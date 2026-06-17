# Improvement Plans — GW2 Build Optimizer

Advisor: `/improve` session. Plans written against commit `5406273`.

These plans are self-contained specs for an executor with **no context from the
advising session**. Read the plan top to bottom before touching code. Each plan
states its own scope, verification gates, and stop conditions.

## Origin

User report (with screenshot): "benchmark data synchronization does not work.
Never worked really." The Settings → Benchmark Data panel shows
`Synced: 2026-06-17 SC:45 HS:0 GJ:30` and the orange error
`[!] hardstuck: Hardstuck: no build links found on any profession page`.

Snowcrows (45 builds) and GuildJen (30 builds) sync fine; **Hardstuck returns
zero**. Root cause was investigated live (see plan 001).

## Root cause (confirmed by live HTTP probe)

`hardstuck.gg` is **blocked at the user's UniFi UDMPro gateway**. A GET to
`https://hardstuck.gg/gw2/builds/guardian/` returns **HTTP 200** with a 563 KB
**Ubiquiti/UniFi block page** ("Blocked ... blocked because the domain is
restricted. Contact your administrator"), not the real builds page. The scraper
sees a 200-OK body with no build links and reports "no build links found."

This splits into two issues:

1. **Environmental (NOT a code fix).** To actually get Hardstuck data, the user
   must allowlist `hardstuck.gg` on the UDMPro content filter. There is nothing
   to change in the repo for this. It is documented in plan 001's "Environmental
   note" so the symptom is understood, not silently worked around.

2. **Real code defect (plan 001).** `fetch_html` ignores the HTTP status code
   and never checks whether the returned page is actually from the expected
   site. A 200-OK block page (or any captive-portal / redirect / wrong-content
   response) is silently treated as valid HTML, producing the misleading
   "no build links found" error. The scraper cannot distinguish
   "blocked / site changed" from "genuinely no builds." That is fixable and
   worth fixing: it turns a confusing dead-end into an actionable diagnostic and
   hardens all three scrapers (Snowcrows/GuildJen would fail the same way if
   ever blocked or moved).

## Plans

| #   | Title                                                        | Category            | Effort | Risk | Status |
|-----|-------------------------------------------------------------|---------------------|--------|------|--------|
| 001 | Detect non-OK / wrong-content scrape responses; clear error | correctness / DX    | S–M    | Low  | EXECUTED — verified in worktree, awaiting merge approval |

### Plan 001 execution result (advisor-reviewed)

Executed by a dispatched executor in an isolated worktree; diff reviewed
hunk-by-hunk and done-criteria re-run independently. **Verdict: APPROVED.**

- Scope: only `crates/optimizer/src/scraper.rs` (97 insertions, 3 deletions).
- Wired the block-page detector into Hardstuck (reported case) + Snowcrows.
- GuildJen correctly skipped per the plan's clean-mapping clause — its control
  flow has no empty-links terminal error (a 200 block page yields
  `any_success=true` and 0 builds with no inspectable error), so wiring it would
  require contorting control flow, which the plan forbids. Documented limitation.
- Gates (re-run independently): `cargo check` clean · 3 new helper tests pass ·
  full optimizer suite 624 passed / 0 failed · clippy 0 · fmt clean.
- Worktree: `.claude/worktrees/agent-a23e755c0b4e5ee17`. NOT yet merged to `main`
  (advisor does not merge without approval).

## Execution order

Single plan. No dependencies. Plan 001 is purely additive (new error detection +
tests) and changes no scoring/optimizer behavior.

## Considered and rejected

- **"Render Hardstuck's JS to get the links" (headless browser / JS engine).**
  Rejected. The page that comes back is a network block page, not a JS-rendered
  app — rendering JS would render the block page. Even absent the block,
  pulling in a headless-browser dependency for one of three scrapers is a large,
  high-risk change disproportionate to the value. If Hardstuck is unblocked and
  *still* returns no static links, that is a separate future investigation, not
  this plan.
- **"Just remove Hardstuck from the scraper."** Rejected unless the user asks.
  The block is environmental; once `hardstuck.gg` is allowlisted the source may
  work. Removing it would discard a working integration to mask a network issue.
