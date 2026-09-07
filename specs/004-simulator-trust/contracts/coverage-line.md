# Contract: coverage qualification line (FR-010, SC-004)

One plain-language line that says what the comparison did not simulate. It exists once, in the referee, and is projected unchanged.

## Text

`Not simulated: {detail}` where `detail` lists up to three source names with their trigger in parentheses, then `and N others`.
Example: `Not simulated: Superior Sigil of Fire (on-crit), Reaper Shroud 4 (dark field) and 1 other`.
Locale key `quality.coverage_line` in all 12 locale files; `{detail}` is not translated (game item names follow the game language already).

## Projections

| Projection | Carrier | Where it appears |
|---|---|---|
| Evaluation result | `RefereeReport.quality_reasons` entry with `field == "wvw_timeline.effects"` | explanation text |
| Optimize suggestion | `BuildSuggestion.coverage_note` | same line, muted, right of the quality marker in the comparison view |
| Choya suggestion | `BuildSuggestion.coverage_note` + `data_quality` + `quality_reasons` set from the referee report | same place; and the line appended to the plate's concerns text |
| Choya evidence | `score_build` response `coverage_note` | verbatim |

## Rules

- The line is present whenever `unmodeled_sources` is non-empty for the evaluated mode; absent otherwise. Absence means every equipped source with a record was executed, not that every mechanic is modeled.
- The line never changes scores, gates or viability. It qualifies; it does not penalise.
- The one-word quality marker keeps its current behaviour (Provisional when the line is present).
- `Verified` continues to mean the `DataQuality::Verified` classification (all inputs factual), not proven meta quality.
