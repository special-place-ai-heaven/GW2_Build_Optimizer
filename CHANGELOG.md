# Changelog

All notable changes to GW2 Build Optimizer are documented here.

## 1.5.1 - 2026-08-23

Empty `gear_groups` now inherit the build prefix for armor, trinkets, and weapons, and every sheet path counts only the active land weapon set. Legacy saves with blank groups fill from `stat_prefix` on load. Empty inherited Strong and explicit all-Strong both lock at Power 2556 / Precision 2186 for the Ranger axe/axe fixture.

## 1.5.0 - 2026-08-23

This release moves WvW optimization away from isolated attribute totals and toward a mode-aware, two-sided exchange timeline. It also carries forward the v1.4.18 character-sheet corrections: passive attributes remain values a player can reproduce in Guild Wars 2, while temporary effects, modeled output, and incomplete mechanics are shown separately and labeled according to data quality.

### Reproducible character-sheet values

- The primary Stats comparison remains grounded in the level-80 Hero-panel attributes, profession health tier, armor-weight defense, equipped gear, active land weapon set, runes, and permanent sourced adjustments.
- Tooltip coefficients such as barrier amounts, direct heals, and life-siphon values are rejected by the shared permanent-stat classifier instead of inflating Power or Healing Power.
- Character-sheet calculation, optimizer scoring, synergy extraction, and model-facing build data use the same classification rules so a rejected tooltip amount cannot continue influencing search through another parser.
- PvE and competitive variants of duplicated attribute and percentage facts are resolved by the selected game mode instead of being stacked together.
- Synthetic evaluation values remain separate from visible attributes. The optimizer does not present internal scoring aids as numbers a player should expect to see in the Hero panel.

### Mode-aware balance and timing data

- Added an active 2026-07-15 balance manifest and patch ledger with explicit PvE, PvP, and WvW override files. The earlier 2026-01-13 manifest is retained as the inherited, superseded baseline.
- Rotation construction now receives the selected game mode. Activation time, recharge, resource cost, status duration, and combo-field duration can therefore resolve to different sourced values in PvE, PvP, and WvW.
- The first exact timing slice covers Black Powder, Heartseeker, and Steal, including competitive initiative and coefficient splits where the published values differ.
- Unsupported or ambiguous facts are not silently promoted to exact mechanics. They remain omitted from exact calculations or are surfaced as `Provisional` while source coverage is expanded.

### WvW exchange timeline

- WvW candidates are evaluated on a two-sided timeline with committed casts, incoming actions, interrupts, control, boon removal, defensive layers, recovery, and an exit instead of being ranked primarily by average dummy output.
- Secured sequences may be created by one applicable layer: control ownership, timed Stability, evade, block, invulnerability, or stealth. The evaluator does not require control and defensive cover simultaneously.
- Aegis and Blind are charge-based safeguards. Their mere presence does not create a multi-second protected interval; they preserve only the action or event they actually answer.
- Sequence completion now measures whether meaningful actions execute without interruption during secured slices. A generic mobility label no longer invents timed evade or stealth coverage.
- Enemy actions respect control state, and pending player actions can be interrupted. This makes ordering, short overlaps, and recovery materially affect the report.
- The WvW report exposes sequence completion, protected pressure, target threshold progress, tempo, sustain margin, exit availability, resource legality, and unmodeled-effect counts for ranking and diagnostics.

### Profession mechanics and resource legality

- Profession/F-slot skills can participate in the candidate and timeline instead of limiting evaluation to heal, utility, elite, and weapon actions.
- Added a bounded resource ledger for Initiative, Adrenaline, Energy, Illusions, and Blades. Higher-priority actions are skipped when their known cost cannot be paid, costs are spent at cast start, landed-hit gains respect caps, and over-cap gains are discarded.
- Resource accounting is intentionally a legality guard rather than a complete profession simulator. Attunements, legends, shroud, heat, life force, pets, kits, and other full state machines are outside this release.
- Weapon swaps use the candidate's actual weapon sets and sigils in search identity, while the passive character sheet continues to count only the active land set.

### Search, weighting, and grouped equipment

- WvW search ranking now reads the exchange report instead of leading with legacy dummy DPS.
- Role selection and fine-tune weights are propagated into candidate ranking so Condition, Control, Sustain, support, and direct-pressure preferences can influence which legal build is retained.
- Candidate identity includes weapon sets, sigils, elite specialization, and profession actions, preventing materially different loadouts from being discarded as duplicates.
- Mixed equipment is represented in grouped armor, trinket, and weapon prefixes. Search reachability was corrected so user-weighted alternatives can enter and survive the beam instead of repeatedly collapsing toward one direct-pressure prefix.
- Gate-repair neighbors remain available for candidates missing required defenses or utility, but passing viability does not override the user's selected role and weights.

### Corrected interaction rules

- Stability on the target prevents applicable control until it is removed; ordered removal can therefore change whether the following control action lands.
- Blind and Aegis no longer answer condition application, and their treatment is separated from evade and invulnerability when evaluating incoming actions.
- Resistance and condition application use their own timeline rules rather than borrowing strike-avoidance behavior.
- Incoming condition ticks use the shared condition-damage calculation instead of a max-health percentage stub.
- Might uses the mode-aware boon table rather than a fixed invented attribute increase.
- Combo fields carry explicit durations. Smoke, water, light, and fire finishers use the supported result for their finisher type; unsupported combinations are marked unmodeled instead of receiving a convenient fallback effect.
- Stealth is timed cover for relevant execution windows, not blanket immunity, and a mobility classification alone no longer grants a hardcoded duration.

### UI, reproducibility, and data quality

- Optimized gear display, comparison rows, save/load presentation, and generated build details preserve grouped-prefix and active-set choices so the shown result can be reconstructed instead of appearing as an unexplained aggregate.
- Mode-aware rotation previews use the visible candidate stats and selected mode rather than silently falling back to a generic PvE simulation.
- Build reports distinguish verified source-backed facts from provisional modeled behavior and count unmodeled sources for diagnosis.
- Exact passive attributes, simulated rotation output, boons, conditions, control, cleanses, and defensive utility remain separate concepts in the UI.

### Runtime and cancellation

- Long optimization work now observes cancellation and runtime limits throughout candidate generation and evaluation instead of only between coarse phases.
- Mode-aware evaluation is reused by search and addon previews, reducing disagreement between the candidate that was ranked and the result shown to the player.

### Validation and current boundaries

- Regression coverage includes the v1.4.18 Ranger character-sheet values, tooltip-stat rejection, competitive percentage classification, mode-isolated timing overrides, charge-versus-duration cover, interruption, ordered removal and control, resource paywalls, grouped-gear search identity, and user-weight reachability.
- The active data slice is not a claim that every Guild Wars 2 trait, skill, rune, sigil, relic, or profession mechanic is now exact. Exact competitive coverage is being expanded incrementally from authoritative sources.
- Timings and effects without a supported mode-specific fact remain `Provisional` or unmodeled. The optimizer will prefer an explicit gap over inventing a duration, coefficient, or interaction.
- Rotation results are deterministic model output, not a live combat record. Player positioning, facing, movement, opponent decisions, and profession state machines not listed above still require further modeling.
- Conditional damage thresholds are represented in the sourced balance layer, but complete conditional execution coverage continues to be expanded. A stored source value is not treated as active unless the timeline can justify its condition.

## 1.4.18 - 2026-08-23

This release replaces several optimistic stat assumptions with sourced, mode-aware character-sheet rules. Its first priority is trust: values shown as attributes should be values a player can reproduce in Guild Wars 2, while modeled rotation output is identified separately.

### Player-visible stat presentation

- The Stats pane now leads with the nine level-80 Hero-panel attributes: Power, Precision, Toughness, Vitality, Condition Damage, Expertise, Concentration, Ferocity, and Healing Power.
- Health and Armor remain visible derived Hero-panel values.
- Removed synthetic `Effective Power`, `Effective HP`, and `Healing Index` from the primary comparison. Those internal scoring aids are not character-sheet attributes and should not look like in-game values.
- Removed the synthetic three-scenario damage table from the primary attribute comparison. Rotation output remains labeled as a simulation rather than a live combat record.
- Boons, conditions, cleanses, stability, skill use, and viability remain separated from passive attributes so temporary effects are not presented as permanent gear stats.

### Permanent attribute corrections

- Tooltip coefficients no longer become permanent attributes. This fixes values such as barrier amounts, direct healing amounts, life-siphon damage, and life-siphon healing being added to Power or Healing Power.
- The same permanent-stat rule is shared by character-sheet calculation, synergy extraction, and the optimizer's LLM tool data. A tooltip amount can no longer disappear from the UI while still biasing candidate selection through another parser.
- Conditional named adjustments such as `Additional Power` are excluded from the unbuffed panel. They belong to timed activation modeling.
- Trait `BuffConversion` facts remain supported. For example, Wellspring's Power-to-Healing-Power conversion still changes the visible passive result.

### Game-mode corrections

- Known duplicated `AttributeAdjust` rows are resolved by trait, target attribute, and selected game mode rather than being summed.
- Lingering Magic now contributes 240 Concentration in PvE and 120 in PvP/WvW, instead of adding both API rows.
- Known conditional or pet-only duplicated rows are excluded from the standing player panel.
- Unknown duplicated rows are omitted instead of guessed. Missing data is safer than a confidently inflated attribute.
- Percentage tooltip facts now share one classifier between combat and synergy parsing.
- Tooltip-only 100% critical-chance facts, recharge reductions, incoming-damage reductions, and other defensive percentages no longer become outgoing damage multipliers.
- Two-value percentage pairs within one trait collapse to one mode value instead of stacking simultaneously. Conditional timing remains provisional and is handled separately from the passive panel.

### Equipment corrections

- Only the active land weapon set's two sigils contribute to the standing stat/modifier calculation. The second set is reserved for weapon-swap timeline handling.
- Rune tier bonuses continue to be summed from all six equipped tiers.
- Level-80 base attributes remain 1,000 for Power, Precision, Toughness, and Vitality, and 0 for the five secondary attributes.

### Profession and armor baselines

The optimizer uses separate sourced profession health and ascended-armor defense values:

| Baseline | Value | Professions |
| --- | ---: | --- |
| High base health | 9,212 | Warrior, Necromancer |
| Medium base health | 5,922 | Revenant, Engineer, Ranger, Mesmer |
| Low base health | 1,645 | Guardian, Thief, Elementalist |
| Heavy armor defense | 1,271 | Warrior, Guardian, Revenant |
| Medium armor defense | 1,118 | Engineer, Ranger, Thief |
| Light armor defense | 967 | Elementalist, Mesmer, Necromancer |

Visible Health is `profession base health + 10 × Vitality`. Visible Armor is `armor-set defense + Toughness`. Armor weight does not change the attribute budget of an otherwise identical armor prefix.

Sources: [Attribute](https://wiki.guildwars2.com/wiki/Attribute), [Health](https://wiki.guildwars2.com/wiki/Health), and [Armor](https://wiki.guildwars2.com/wiki/Armor).

### Reproduced failure and corrected result

The original Ranger case reached 7,156 Power and 13,696 Healing Power because effect coefficients were treated as passive stats. The exact offending tooltip amounts were removed from every permanent-stat consumer.

The cached WvW reproduction now resolves the tested optimized candidate to:

| Attribute | Value | Exact source |
| --- | ---: | --- |
| Power | 3,058 | level-80 base + Strong ascended gear + six Rune of Infiltration tiers |
| Precision | 2,544 | level-80 base + Strong ascended gear + six Rune of Infiltration tiers |
| Toughness | 1,000 | level-80 base |
| Vitality | 1,240 | level-80 base + Natural Fortitude |
| Concentration | 120 | Lingering Magic, WvW value |
| Healing Power | 214 | Wellspring: 7% of 3,058 Power, rounded for display |

The Rune of Infiltration contribution is exactly +175 Power and +225 Precision from its six API bonus lines.

### Validation

- All optimizer tests pass: 735 passed, 1 network test ignored.
- All 39 math permutation tests pass.
- All 29 objective-profile integration tests pass.
- Addon compilation passes.
- Clippy completes successfully; existing repository warnings remain non-blocking.

### Known boundaries

- Rotation output is still a model, not a live combat log. It remains labeled as simulated.
- Conditional trait and upgrade effects require timeline activation rules before their contribution can be called exact.
- A shield's separate defense rating is not yet added to the optimized Hero-panel Armor value. The armor-weight baseline itself is exact.
- Food, utility consumables, temporary boons, and triggered upgrade effects are intentionally not folded into the unbuffed attribute table.
