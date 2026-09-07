# Contract: `score_build` full-build evaluation (FR-018)

Provider-neutral tool exposed to Choya through `gemini_tools::tool_declarations()`; the same declaration is mapped for Gemini, OpenAI, Anthropic and OpenRouter by `llm/tools.rs`.

## Request

```json
{
  "gear_prefix": "Marauder",                 // optional, legacy prefix-only mode
  "build": {                                 // optional; when present, full-build mode
    "specializations": [{"name": "Spite", "traits": ["Spiteful Talisman", "Chill of Death", "Signets of Suffering"]}, ...],
    "weapons": {"set1": {"main_hand": "Greatsword"}, "set2": {"main_hand": "Axe", "off_hand": "Focus"}},
    "skills": ["Signet of Vampirism", ...],
    "rune": "Superior Rune of the Scholar",
    "sigils": ["Superior Sigil of Fire", "Superior Sigil of Force", "...", "..."],
    "relic": "Relic of the Thief",
    "stat_prefix": "Marauder",
    "gear_slots": {"Helm": "Marauder", ...},   // optional per-slot override
    "pets": [], "legends": []
  }
}
```
Field names and semantics are exactly those of the plate the model already emits (`prompts::GeminiBuildResponse`). Scenario and game mode are NOT arguments; they come from the addon's current `ScenarioSpec` and `BalanceContext`.

## Response

Full-build mode:
```json
{
  "mode": "full_build",
  "viable": true,
  "gates": [{"gate": "WvwStability", "passed": false, "note": "..."}],
  "user_intent_score": 0.73,
  "realized": {"power": 0.8, "condition": 0.1, "boon_support": 0.2, "healing": 0.1, "sustain": 0.4, "control": 0.3},
  "quality": "Provisional",
  "quality_reasons": ["Necromancer.wvw_timeline.effects [WvW]: Not simulated: Superior Sigil of Fire (on-crit)"],
  "coverage_note": "Not simulated: Superior Sigil of Fire (on-crit)",
  "warnings": ["Spite column 2 filled with Chill of Death"],
  "errors": []
}
```
Errors: `{"error": "no build supplied"}` when neither `gear_prefix` nor `build` is present; `{"mode":"full_build","errors":[...]}` with no scores when validation has errors; `{"error": "evaluation budget spent"}` after the third full-build evaluation in one chat request.

Legacy mode (`gear_prefix` only, `build` absent) keeps today's shape and adds `"mode": "prefix_only"` and `"scope": "prefix only; traits, upgrades and rotation not evaluated"`.

## Guarantees

- Same path as the app: `validate_gemini_build` → `evaluate_validated_build_with` (opener empty). Values equal the app's own referee output for the same plate within 1e-9.
- An omitted `build` never evaluates the equipped loadout or any other build.
- Validation fills are reported in `warnings`, never silent.
- The skill-only `simulate_rotation` tool description gains: "estimates a skill list on an open dummy; not a full-build verdict; use score_build with a build for that".
