# Research: Trait triggers are the build (007)

Every decision below was checked against the code on branch `007-trait-triggers` (tip `255371f`) and the cached game data under `dev.cfg`'s addons directory on 2026-09-08. Wiki pages are named where a number comes from; no number is copied from the fixture.

## R1. Where trait effects stand today

**Finding.** The cache holds 999 traits: 111 per profession, 9 lines each (5 core, 4 elite; the fourth elite lines are Ritualist, Evoker, Amalgam, Luminary, Troubadour, Galeshot, Conduit, Antiquary, Paragon). Of the Necromancer's 111, the description census finds 14 traits keyed on entering shroud, 4 on leaving, 6 while in shroud, 3 on a chilled foe, 5 on a critical hit, 2 on shouts, 18 naming life force, 9 on minions, 5 on shades, 6 on blight.

What executes today: `TriggerRule::Passive` records are folded into `SimParams` by the combat parser; `OnCrit`, `OnHit` records fire in `apply_skill_effect`; `OnSkillUse` fires only for `SourceType::Skill` (line 967 of `wvw_timeline.rs`); `OnHealthThreshold` and stacking `OnHit` become `ConditionalSpec`s. Nothing fires on `enter_shroud` or `exit_shroud`; nothing reads `in_shroud` for a bonus; `has_condition` looks at conditions on the *player*, never on the foe; `apply_operation` applies boons to the player only and conditions to one foe; there is no ally at all (`supports_allies` is a capability tag).

**Decision.** Keep the Sprint 2 record → `ProcSpec` / `ConditionalSpec` path and add firing sites and prerequisites to it, rather than a second trait engine. The three `T042` holes (foe prerequisite, trait-owned skill-use, life force fact variants) each become one schema field and one runtime check.

## R2. Trigger kinds to add

**Decision.** `TriggerRule` gains five variants, each with exactly one firing site:

| Variant | Fires from | Site |
|---|---|---|
| `OnShroudEnter` | `enter_shroud` after the state is set | `trigger_procs(OnShroudEnter, Some(entry_skill), false, 1.0)` |
| `OnShroudExit` | `exit_shroud` before the state is dropped | same shape; `why` is carried in the trace |
| `OnConditionApplied` | every push to `outgoing_conditions` (skill facts, records, corrupts) | one helper `apply_outgoing_condition` replaces the four inline pushes |
| `OnBoonApplied` | `apply_buff` on the player | one call after the buff lands |
| `OnBoonStripped` | `remove_enemy_boons` | one call per boon removed |
| `Periodic` | each tick of `run` | `trigger_procs(Periodic, None, false, 1.0)`; the record's `internal_cooldown` is the period |

"While in shroud" is **not** a trigger. Death Perception's +15 % critical damage and Reaper's Onslaught's quickness are a standing bonus with a prerequisite, so they are `TriggerRule::Conditional` records with `prerequisite.in_shroud = true`, executed as a new `ConditionalKind::InShroud` in `update_conditionals` (active iff `in_shroud.is_some()`). Vampiric Presence (life steal on hit while in shroud) is `OnHit` with the same prerequisite. One prerequisite field covers all three spec wordings.

**Rationale.** Each variant maps to an existing state transition, so the diff is a call per site and a match arm in `same_trigger` / `trigger_label`. Sprint 2's `unsupported_normalized_trigger_degrades_coverage` keeps unknown variants on the coverage line.

**Alternatives.** A generic event bus keyed by string: rejected, nothing else would consume it and the enum already gives serde validation for free.

## R3. Prerequisites (foe, self, health)

**Decision.** One optional `prerequisite` object on `NormalizedEffect`:

```json
"prerequisite": { "foe_condition": "Chilled", "in_shroud": true, "foe_health": { "above": false, "percent": 50.0 } }
```

All three members optional; an empty object is rejected by validation. Runtime: `foe_has_condition(name)` reads `outgoing_conditions` (the foe's conditions are the ones the player put there; the dummy arrives with none); `in_shroud` reads `self.in_shroud.is_some()`; `foe_health` reads `enemy_health / target_health` and is never met on the open dummy (`target_health` is `None`), traced as `prerequisite never met`. The existing `health_threshold` field stays as the player's own health gate; it is not moved.

**Rationale.** Chilling Nova (crit vs chilled foe), Cold Shoulder, Chilling Victory, Spiteful Fortitude (strike a foe below 50 %), Death Perception and Vampiric Presence are all covered by these three members; the census found no other prerequisite wording in the Necromancer lines.

## R4. Trait-owned skill-use and skill kinds

**Decision.** Allow `OnSkillUse` for `SourceType::Trait` when `trigger_scope` names a skill kind. `TriggerScope` gains `Category(String)` (matches `Skill.categories`: `Shout`, `Well`, `Minion`, `Signet`, `Mark`, `Corruption`, `Elixir`, `Punishment`…) and `Slot(String)` (`Heal`, `Utility`, `Elite`, `Profession`). `RotationSkill` gains `categories: Vec<String>` and `skill_slot_name: Option<String>`, filled by the builder from the API skill. `scope_admits` matches on them. Validation: a trait record with `OnSkillUse` and scope `Any` is an error (it would fire on every skill).

Dread is not skill-use: the wiki (`Dread`, read 2026-09-08) says "Inflicting fear on a foe"; it is `OnConditionApplied` with `trigger_scope: Status("Fear")`. `TriggerScope::Status(String)` therefore also exists, and doubles for `OnBoonApplied` / `OnBoonStripped` filters (Blighter's Boon takes any boon: scope `Any`).

## R5. Effect kinds the records need

**Decision.** `EffectCategory` gains `GainsLifeForce` (value = percent of the pool) and `Heal` (value = flat amount; new optional `healing_power_coefficient` field, e.g. Blighter's Boon 133 + 0.1 × healing power per wiki). `trigger_procs` routes `GainsLifeForce` to the resource ledger through the existing cap and `LifeForceGained` trace, `Heal` to `heal()`. The `OutgoingHealingPct`-with-duration hack that `trigger_procs` uses as a heal today is left alone (no record uses it) and noted as a cleanup in the audit.

Unholy Martyr's exit half is `OnShroudExit`, `RemovesCondition` amount 3 plus a second record `GainsLifeForce` 7 with `scale_by: "ConditionsRemoved"` (a one-variant enum read at the firing site from the cleanse count). Both numbers are on the page; nothing is multiplied in the data.

Traits that summon a strike (Spiteful Spirit, Chilling Nova's explosion) are `TriggeredEffect → StrikeDamagePct` in coefficient form (value ≤ 2.0, Sprint 2 semantics) with the skill's `dmg_multiplier` from the API skill the trait lists (`Trait.skills[].facts`), which the record cites.

## R6. Fight population (FR-003a)

**Finding.** The timeline has one foe (health, protection, stability, outgoing conditions) and the player. Ally boons are applied to the player only; AoE conditions land once.

**Decision.** A `FightPopulation { foes: u32, allies: u32 }` read from `data/formulas/fight_population.json` by `CombatTier`:

| Tier (WvW label) | foes | allies | Why |
|---|---|---|---|
| Solo (Roam) | 1 | 0 | the 1v1 the roam objective already assumes |
| Party (Havoc) | 5 | 4 | one party of five on each side (`referee.rs` Havoc: 5–15 players) |
| Squad (Cloud/Zerg) | 10 | 9 | enough that the WvW 10-target cap and the 5-ally boon cap both bind |

This file is a scale definition, not a scoring constant: no `*_NORM`, gate or `WEIGHT_BUDGET` changes. PvE tiers get the same numbers under their labels but PvE/PvP do not run the timeline, so FR-009 holds.

Runtime shape, deliberately small: the primary foe keeps its full state; extra foes are counted, not simulated. An effect with `target_side: Enemy` and `target_count: n` applies its damage or condition to `min(n, foes)` foes: the primary gets the full state change, the rest add `(k-1) × amount` to two new report totals `cleave_damage` and `cleave_condition_stack_seconds`. An effect with `target_side: Ally` and `target_count: n` applies to the player and adds `min(n-1, allies) × stacks × duration` to `ally_boon_stack_seconds`. Skill facts with `Number of Targets` feed the same path through `SkillEffect` (the builder already reads the fact for some effects; the plan extends it). Ally-facing cleanses and heals count into `ally_cleanses` and `ally_healing` the same way.

Ranking: `cleave_damage` joins `total_damage` and `protected_damage` (it is damage the build did); for `CombatKind::Support | Commander`, `search_rank`'s output slot becomes `sustain_margin + ally_boon_stack_seconds / 1000`, so two support builds differing in an ally-facing trait rank apart (SC-003). This is the one rank-key change and it lands behind a seen-failing ranking test.

**Alternatives.** N full ally state machines: rejected, no record needs an ally's *state* (nothing this sprint reads an ally's conditions), and the cost would triple the timeline's tick work against SC-006.

## R7. Coverage line and reason classes (FR-005, FR-007)

**Finding.** `active_normalized_effects` names every equipped source without a record "(no record)", including weapon skills whose strikes ran from facts. That is CONN-01-06 and why the fixture line has 40 names.

**Decision.** A source is *executed from facts* when the builder produced at least one `SkillEffect` for it (skills) or the combat parser consumed at least one stat, percent or traited fact from it (traits: `combat.rs` line 740 region already walks them; it will return the set of trait ids it used). Those sources are removed from the "(no record)" list. What remains is classified with a `ReasonClass`:

| Reason class | Suffix on the line | When |
|---|---|---|
| `NoRecord` | `(no record)` | nothing executed and no record |
| `PassiveNoEffect` | `(passive, no simulated effect)` | record or facts say the trait changes nothing the timeline measures (e.g. movement speed, revive speed) |
| `NeedsMechanic(name)` | `(needs: minions)` | the record names a mechanic outside this sprint: minions, shades, blight, kill, downed, transform |
| `UnresolvedValue` | `(unresolved value)` | Sprint 2 behaviour, kept |
| `PrerequisiteNeverMet` | `(prerequisite never met)` | trace only, not a coverage entry: the trait *was* simulated and did not fire |
| `NoFiringSite` | `(no firing site)` | trigger the runtime does not have; kept so a future variant stays honest |

`PassiveNoEffect` and `NeedsMechanic` are written into the record itself as `coverage: { "class": "NeedsMechanic", "mechanic": "minions" }` so the audit can list every trait's state from data alone (SC-002). A record with a `coverage` block has no executable payload and is validated as such.

## R8. The audit table and the automated record check (FR-005, FR-010a, SC-002, SC-007)

**Decision.** Two `#[ignore]` tests behind `dev.cfg`, run by the quickstart and CI's nightly job if one exists (it does not; they stay manual gates in the release checklist):

- `trait_coverage_audit_lists_every_trait` writes `docs/audit/trait-coverage.md`: one row per trait of all nine professions with profession, line, name, state (`facts` / `record` / class) and the record's wiki page. It fails if any trait has no state (SC-002) and prints the count per profession.
- `records_match_their_wiki_pages` reads every record's `source` URL, fetches the page through the crawl4ai instance named in `dev.cfg` (`crawl_url`, optional; the test skips with a message when absent), and checks each factual number appears in the page's mode column. This is FR-010a's check; the read date in `source` is refreshed by hand when the page changes.

Every new record also gets a unit test in the seen-failing style (`reaper_*` / `necro_*` experiments) that asserts the firing event and its direction.

## R9. Test builds per profession (FR-012)

**Finding.** The benchmark cache holds published WvW builds with full `published` payloads for every profession from GuildJen (11–19 each) and for eight from Hardstuck (Thief has none). A published `ProviderBuild` carries specs, trait ids, skill ids, gear, rune, sigils, relic and the page prose; the addon turns one into a plated suggestion, and `referee::evaluate_validated_build_with(.., opener)` scores a `ValidatedBuild` with the page's rotation.

**Decision.** Each profession increment adds a `published_<profession>_wvw` fixture built from one GuildJen WvW page's ids (recorded in the test with its URL and scrape date) and a hand-authored `<profession>_opener` fixture in `rotation/<profession>_fixture.rs` in the Reaper fixture's shape, reaching every trigger kind that profession's records use. The published build is scored as the site wrote it; the opener is where trigger controls live. Fixture values never enter `data/`.

Profession order after Necromancer: read the professions of the cached characters (`characters.json` carries the API `profession` field) at the start of the second increment and take them in that order, then the rest alphabetically. The order is a task-list detail, not a design decision.

## R10. Constraints carried from Sprints 1 and 2

- No change to any `*_NORM`, `WEIGHT_BUDGET`, gate threshold or objective profile; the one rank-key change (R6) is ordering, not a constant, and is pinned by a test.
- PvE and PvP byte-identical: the timeline is WvW-only; `pve_output_unchanged_by_conditional_tagging` and the scoring regression tests stay green and a new pin covers the trait-fact consumption set.
- Owned files untouched: `prompts.rs`, `llm/`, `examples/choya_live.rs`.
- `specs/` is gitignored: `git add -f`.
- Every remedy behind a failing test seen under `docs/audit/disable_and_run.py` with a new control entry per mechanism.
- Optimizer lib harness within 10 percent of the Sprint 2 baseline (16.6–19.0 s): population counting is arithmetic, no extra state machines; the fetch-backed record check is `#[ignore]`.
