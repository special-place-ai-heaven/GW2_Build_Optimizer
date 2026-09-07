# Data model: Simulator trust (Sprint 1)

Types are named as they exist in the tree; new fields are marked (new). No new file formats.

## Finding (audit document, `docs/simulator-connection-audit.md`)

| Field | Values |
|---|---|
| id | `CONN-0P-NN`, P = phase 0/1/2/3 |
| evidence | `path:line` in the recorded baseline commit |
| observed | one sentence of behaviour seen |
| affected | modes (PvE/WvW/PvP) and callers (equipped display, Optimize, Choya, import, search, tool) |
| reproduction | test name or command |
| expected | one sentence |
| remedy | proposed, or "none this sprint" |
| classification | data gap / runtime gap / search gap / reporting gap / refuted / unconfirmed / hypothesis |
| links | related `W###` ids from specs/003 ledger, never renumbered |
| status | open / demonstrated / remedied / refuted |

State transitions: hypothesis → demonstrated (a failing experiment exists) → remedied (the experiment passes and a commit is named) ; hypothesis → refuted (an experiment shows the expected behaviour) ; hypothesis → unconfirmed (could not be reproduced within the sprint, reason stated).

## Trace matrix (section of the audit document)

Rows (Reaper slice): Gravedigger (skill), Path of Corruption (trait), Superior Sigil of Fire (on-crit sigil), a passive sigil, the equipped rune, the equipped relic, life force (resource rule), Reaper Shroud 4 dark field + finisher (combo).
Columns: intent, parsing/validation, data selection, derived parameters, rotation preparation, runtime, evaluation, exposure.
Cell: `{ class: exact | approximate | missing | intentionally-inapplicable | not-yet-checked, pointer: path:line or event, note }`. Every `missing` cell carries a reason.

## Experiment (Rust test)

`kind ∈ {positive, negative, timing, ablation, interaction, unsupported, parity}`; each test names its kind in the function name (`reaper_positive_control_…`). Inputs: a hand-built `ValidatedBuild`, a `ScenarioSpec` (WvW, Solo/StrikeSpike unless stated), an `opener: &[u32]`, and zero or more hand-authored `NormalizedEffect` records. Output assertions are on `WvwCombatReport` fields, `RefereeReport` fields, or `TraceEvent`s. Verdict is written into the audit finding it serves.

## Coverage qualification

- `DataQualityReason` (existing, data/quality.rs): `field = "wvw_timeline.effects"`, `entity = profession`, `modes = [mode]`, `explanation` = plain-language line.
- `WvwCombatReport.unmodeled_sources: Vec<String>` (new; replaces the `u32` count, which becomes `.len()`). Names are `"{Effect name} ({trigger})"`, e.g. `Superior Sigil of Fire (on-crit)`.
- `engine::active_normalized_effects` return (changed): `(Vec<&NormalizedEffect>, Vec<String>)`, the second being equipped sources with no record, resolved to names.
- `BuildSuggestion.coverage_note: Option<String>` (new; addon comparison.rs). Populated from the reason above by `synergy_result_to_suggestion` and by chat_flow when the Choya suggestion is built.
- Locale key `quality.coverage_line` (new, all 12 locales): `"Not simulated: {detail}"`.
- Choya evidence: the same line appended to `concerns` before `fmt.plate_concern` formatting.

## Trace event (new, diagnostics)

```
WvwTimelineInput.trace: bool            // default false; never set by search or tools
WvwCombatReport.trace: Vec<TraceEvent>  // empty unless requested
WvwCombatReport.trace_truncated: bool
TraceEvent { t_ms: u32, kind: TraceKind, source: String, detail: String }
TraceKind = HitLanded | ProcFired | ProcSkippedIcd | ProcUnmodeled | CastInterrupted | WeaponSwap
```
Cap: 512 events; the 513th sets `trace_truncated` and is dropped.

## `score_build` tool (extended; see contracts/score-build-tool.md)

Argument gains `build: Option<GeminiBuildResponse-shaped object>`. Result gains `viable`, `gates[]`, `user_intent_score`, `realized{six axes}`, `quality`, `quality_reasons[]`, `coverage_note`, `warnings[]`, `errors[]`. `ToolContext.scenario: &ScenarioSpec` (new). Per-turn evaluation counter (new, in the chat tool callback): max 3.
