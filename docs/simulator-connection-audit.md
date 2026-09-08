# Simulator connection audit (Sprint 1: CONN-00 to CONN-02, CONN-03 A)

Companion to `docs/simulator-trust-plan.md` and `specs/004-simulator-trust/`. Findings use `CONN-0P-NN` ids (P = phase). Related findings from the 003 remediation ledger are linked by their `W###` / `B001` ids and are never renumbered or reopened here. `Verified`, `Provisional` and `Blocked` are used only with their `DataQuality` meaning (`crates/optimizer/src/data/quality.rs`).

## 1. Baseline

| Item | Value |
|---|---|
| Branch | `004-simulator-trust` |
| Commit | `09740cecef73335307dbf09fed06267754c96384` (fast-forwarded from `d1d4b9d`, the branch point on `main`, onto the tip of `fix/wiki-timing-facts`, because every symbol the research cites — `evaluate_validated_build_with`, `WvwTimelineInput.opener`, scheduled hits — lives there) |
| Workspace version | `1.14.1` (`Cargo.toml`) |
| Active data manifest | `2026-01-13` (`data/normalized_effects/2026-01-13/{pve,pvp,wvw}.json`; the tasks file cites `crates/optimizer/data/…`, which does not exist — the data root is the repository `data/` directory) |
| Scenario | WvW, `CombatTier::Solo`, `CombatKind::StrikeSpike`, `TargetProfile::Single` (`ScenarioSpec::from_balance_context`, `crates/optimizer/src/scenario.rs:80`) |
| PvE comparison | PvE, `Solo` / `StrikeSpike`, same constructor |
| Dirty / owned paths of the latency work | `crates/optimizer/src/prompts.rs` (modified, uncommitted in the `fix/wiki-timing-facts` worktree), `crates/optimizer/src/llm/`, `crates/optimizer/examples/choya_live.rs`, plus untracked `autoresearch/` and `docs/choya-latency-review.md` in that worktree. None of these are edited by this sprint. |
| Credentials, account or character names | none used; every experiment runs on `GameDb::empty_for_tests()` with the hand-authored fixture in `crates/optimizer/src/rotation/reaper_fixture.rs` |

### Test budget baseline

`cargo test -p gw2-optimizer --lib`, this machine, at the commit above (harness time from `test result`; wall time includes the incremental build):

| Run | Result | Harness time | Wall time |
|---|---|---|---|
| 1 | 1112 passed, 0 failed, 4 ignored | 16.83 s | 32 s (incl. rebuild) |
| 2 | 1112 passed, 0 failed, 4 ignored | 16.46 s | 16 s |

For reference the branch point `d1d4b9d` (before the fast-forward) measured 1075 passed / 4 ignored at 16.40 s and 16.48 s. Budget for this sprint: at most +10 s on the harness time (SC-003). Final delta: see section 7.

## 2. Inventory

Existing fixtures, calibration examples and consistency checks, reused before anything new was added (FR-004).

| Asset | Path | Covers |
|---|---|---|
| Scoring regression | `crates/optimizer/tests/scoring_regression.rs` | closed-form `score_with_weights` invariants across presets |
| Math permutations | `crates/optimizer/tests/math_permutations.rs` | stat / modifier arithmetic over permuted inputs |
| Objective profiles integration | `crates/optimizer/tests/objective_profiles_integration.rs` | objective-profile gates and axes end to end |
| Viability calibration | `crates/optimizer/examples/calibrate_viability.rs` | gate pass rates on published builds (needs `dev.cfg` cache) |
| Flow calibration | `crates/optimizer/examples/flow_calibration.rs` | realized-axis norms on published builds |
| Necro holes check | `crates/optimizer/examples/necro_holes_check.rs` | Necromancer loadout holes through `evaluate_validated_build` |
| Scourge support check | `crates/optimizer/examples/scourge_support_check.rs` | Scourge support gates |
| Data consistency tests | `crates/optimizer/src/data/consistency_tests.rs` | manifest ids, effect categories, modes, status payloads, inner categories, sources |
| Published WvW timeline fixtures | `crates/optimizer/src/rotation/wvw_timeline.rs` test module: Spellbreaker (`:2919`), Mirage (`:3015`), Virtuoso (`:3063`) | secured-sequence, strip and resource behaviour on three published kits |
| WvW rotation profiles | `data/rotation_profiles/wvw.json` — one Necromancer row `wvw_necromancer_core`, `elite_spec: null`, `evidence_level: "Heuristic"` (`:270`) | boon uptime and condition application assumptions for the closed-form combat model |
| PvE rotation profiles | `data/rotation_profiles/pve.json` — one Necromancer row `pve_necromancer_core`, `elite_spec: null` (`:548`) | same, PvE |
| Normalized effects | `data/normalized_effects/2026-01-13/{pve,pvp,wvw}.json` | timed and passive effect records by source id; WvW holds `Path of Corruption` (trait 1693, OnHit, ICD 10 s, `wvw.json:175`) and `Superior Sigil of Fire` (sigil 24548, OnCrit, ICD 5 s, `wvw.json:135`) |
| Hit timing | `data/formulas/hit_timing.json` | per-hit offsets for multi-hit skills, Guardian and Ranger only |

**What does not exist for Reaper.** No Reaper trait record in any mode file (Chilling Nova, Cold Shoulder, Soul Eater, Reaper's Onslaught, Decimate Defenses are absent). No Necromancer rune or Necromancer-specific relic record. No Reaper (or any Necromancer) row with an `elite_spec` in either rotation-profile file. No published Reaper timeline fixture. No Necromancer skill in `hit_timing.json`. No life-force resource kind. Everything the experiments press is therefore hand-authored in `reaper_fixture.rs`, and no value from it may be copied into `data/`.

## 3. Findings

Columns follow `specs/004-simulator-trust/data-model.md`. Line numbers are from the baseline commit. Classification `hypothesis` means no experiment has run yet; status moves to `demonstrated`, `refuted` or `remedied` in section 6 / 7.

| Id | Evidence | Observed | Affected | Reproduction | Expected | Remedy | Classification | Links | Status |
|---|---|---|---|---|---|---|---|---|---|
| CONN-00-01 | `data/normalized_effects/2026-01-13/wvw.json` (no Reaper trait, no Necro rune/relic); `data/rotation_profiles/wvw.json:270` (`elite_spec: null`, Heuristic); `crates/optimizer/src/rotation/wvw_timeline.rs:35` `ResourceKind` (Initiative, Energy, Adrenaline, Illusions, Blades — no life force); `crates/optimizer/src/engine.rs:1749` `resource_model_complete` allowlist | Reaper has no normalized-effect records, no rotation profile, no published fixture and no life-force resource model | WvW, PvE, PvP; every caller | section 2 inventory; `grep -c Reaper data/normalized_effects/2026-01-13/wvw.json` returns 0 | a Reaper build's timed effects and shroud economy are represented | none this sprint (data work is CONN-03 C) | data gap | — | demonstrated |
| CONN-00-02 | `crates/addon/src/ui/main_view/resolution.rs:215` → `stats::calculate_full_stats` + `combat::extract_damage_modifiers` + `compute_3tier_combat`; `crates/optimizer/src/combat.rs:571` `buff_profiles_for_profession` | the equipped-character display computes stats and the three closed-form combat tiers and never calls the referee; no gates, no realized axes, no rotation | equipped display, all modes | `find_references(evaluate_validated_build)` lists no caller under `resolution.rs` | the same verdict path as Optimize, or a visible statement that this panel shows stats only | none this sprint | reporting gap | — | demonstrated (`find_references(evaluate_validated_build)`: no caller in `resolution.rs`) |
| CONN-00-03 | `crates/addon/src/ui/main_view/provider_picks.rs:277` `adopt_provider_pick` | an imported published build is adopted as a `BuildSuggestion` without a referee run: `viability` and `rotation` stay empty | published import, all modes | `find_references(evaluate_validated_build)` lists no caller under `provider_picks.rs` | imported builds carry the same verdict fields as Optimize results | none this sprint | reporting gap | — | demonstrated (`find_references(evaluate_validated_build)`: no caller in `provider_picks.rs`) |
| CONN-00-04 | `crates/optimizer/src/engine.rs:1647` `equipped.difference(&modeled).count()`; `crates/optimizer/src/rotation/wvw_timeline.rs:638,642` `unmodeled_effect_sources += 1`; `referee.rs:1227` and `engine.rs:1885` render "{n} equipped or triggered effect sources are not yet represented by timed rules" | unmodeled effect sources are counted; their names are discarded before the report | WvW; referee, Optimize, Choya | `reaper_unsupported_oncrit_is_named_not_zeroed` (fails at baseline: `unmodeled_sources` does not exist) | the report names what it did not simulate | keep names through `WvwCombatReport.unmodeled_sources` and a shared `coverage_reason` (section 7) | reporting gap | W011 | remedied (`2dc3f04`, section 7) |
| CONN-00-05 | `crates/addon/src/ui/main_view/chat_flow.rs:924-927` `BuildSuggestion { label, ..Default::default() }`; `attach_chat_stats` (`optimization.rs:1020`) never sets `data_quality` | a Choya plate's suggestion is built with default quality (`Verified`) regardless of what `plate_shortfall`'s referee report said | Choya, all modes | read the two sites; `choya_suggestion_carries_referee_quality` once the flow is unit-testable | the suggestion carries the referee's `quality` and reasons | set quality fields from the `plate_shortfall` report (section 7) | reporting gap | B001, W011 | remedied (`2dc3f04`, section 7) |
| CONN-00-06 | `crates/optimizer/src/rotation/wvw_timeline.rs:1407` (`trigger_procs(OnHit)` in `apply_skill_effect`), `:855` (`trigger_procs(OnSkillUse)` in `resolve_pending_cast`); no `trigger_procs(TriggerRule::OnCrit, …)` anywhere; `load_normalized_effects:636` treats OnCrit as unsupported | `TriggerRule::OnCrit` has no firing site; an on-crit sigil record is counted as unmodeled and contributes zero damage | WvW; referee, Optimize, Choya | `reaper_unsupported_oncrit_is_named_not_zeroed` | either the proc fires on critical hits or the result says it was not simulated | reporting only this sprint (FR-014); firing is CONN-03 C | runtime gap | — | demonstrated (section 6, unsupported control) |
| CONN-00-07 | `crates/optimizer/src/rotation/wvw_timeline.rs:1039` `try_weapon_swap` changes `active_weapon_set` only; `Timeline::new:549` loads `proc_specs` once from `active_effects`; `engine.rs:1602` selects sigils with `validated.active_sigil_ids()` (set 1 only) | weapon swap does not refresh the active sigil set: set-2 sigils never load, set-1 sigils keep firing after the swap | WvW; every caller | `reaper_negative_control_wrong_mode_and_stowed_set` (b) | the sigil set follows the worn weapon set | none this sprint | runtime gap | — | demonstrated (section 6, negative control b) |
| CONN-00-08 | `crates/optimizer/src/rotation/wvw_timeline.rs:108` `CHILLED_RECHARGE_PERCENT`, `:727` `tick_recharge_rate` applies it to the player's own cooldowns only; `apply_skill_effect` ApplyCondition pushes `outgoing_conditions` with no enemy recharge model | chill applied to the enemy has no effect on the enemy; only incoming chill on the player is modelled | WvW; every caller | no enemy cooldown ledger exists to observe; recorded, not an experiment | outgoing chill slows the enemy's recharge or the report says the enemy has no cooldown model | none this sprint | runtime gap | — | demonstrated (by absence of any enemy-cooldown state) |
| CONN-00-09 | `crates/optimizer/src/engine.rs:1749` `matches!(profession_name, "Thief" \| "Revenant" \| "Warrior" \| "Mesmer")` | `resource_model_complete` is a profession allowlist; Necromancer is not in it, so the flag reads `false` and the referee adds the "outside the bounded resource ledger" reason — correct outcome, but it is a list, not a model, and a new Necro rule would not flip it | WvW; referee, Optimize, Choya | `reaper_fixture_evaluates_without_errors` shows `resource_model_complete == false` for the fixture | the flag derives from which rules the build's skills actually needed | none this sprint | runtime gap (reporting is correct today) | — | demonstrated |
| CONN-00-10 | `crates/addon/src/ui/main_view/optimization.rs:192-201` `evaluate_viability_gates` + `apply_offbar_stability` only; `referee.rs:1170-1182` uses `evaluate_viability_gates_for(…, profile)` + off-bar stability + off-bar cleanse | the Optimize suggestion recomputes gates with a narrower gate set than the referee (no objective-profile floors, no off-bar cleanse), so the displayed gate list can differ from the verdict that ranked the build | Optimize display, all modes | `reaper_parity_referee_matches_optimize_suggestion` asserts only referee-sourced fields; the gate-list difference is documented, not asserted | one gate evaluation, projected | none this sprint (CONN-03 B) | reporting gap | — | demonstrated (section 6, parity: documented, not asserted) |
| CONN-00-11 | `crates/optimizer/src/referee.rs:1108` `evaluate_validated_build_with` has no cancellation parameter; `engine.rs:1348` `simulate_prepared` likewise; `crates/optimizer/src/gemini_tools.rs` has no `is_cancelled`; the chat worker checks only `chat_epoch` staleness (`chat_flow.rs:480`) | a referee run (gate sim + 60 s flow sim) cannot be cancelled once started; a tool call that reaches it runs to completion | Choya tool path, Optimize advisor | read the signatures; `find_references(is_cancelled)` under `referee.rs` and `gemini_tools.rs` is empty | a bounded, cancellable full-build evaluation | per-request evaluation cap of three in the chat tool callback (T036); threading `is_cancelled` into `simulate_prepared` is a recorded follow-up | runtime gap | — | demonstrated; bounded by the per-request cap (section 7), cancellation left open |

## 4. Trace matrix

Reaper slice, WvW Solo/StrikeSpike, fixture build from `reaper_fixture.rs` (greatsword + Fire/Force on set 1, axe/focus on set 2, Scholar rune, Thief relic, Marauder). Cell form: `class — pointer; note`. Classes: `exact`, `approximate`, `missing` (with reason), `inapplicable`. No cell is left `not-yet-checked`. Line numbers are from the baseline commit.

| Mechanic | Intent | Parsing / validation | Data selection | Derived parameters | Rotation preparation | Runtime | Evaluation | Exposure |
|---|---|---|---|---|---|---|---|---|
| **Gravedigger** (greatsword 2) | exact — weapon `Greatsword` set 1 is the player's/plate's choice; `ScenarioSpec` fixes mode/tier/kind | exact — `validation.rs::validate_weapons` resolves the weapon name; skills come from `Profession.weapons["Greatsword"].skills` in `engine.rs::add_weapon_skill_ids:1783` (hand filter, `TwoHand`) | inapplicable — no normalized record for a weapon skill; `sourced_damage_coefficient_profile` (`builder.rs:336`) returns none for the fixture id | exact — `SimParams` from `prepare_validated_rotation:1300-1325` (power, precision, ferocity, `strike_mult` from `DamageModifiers`) | exact — `builder.rs::skill_to_rotation_for_context:139`: `Fact::Damage` → `StrikeDamage{hit_count, dmg_multiplier}`, `Fact::Recharge` → cooldown, `timing_for` default 500 ms for `Weapon_2`; tagged `weapon_set = 1` | exact — `wvw_timeline.rs::start_cast:872` schedules the hits across the activation (`hit_timing::hit_schedule`, even spread when unlisted), `land_scheduled_hits:781` lands each as `StrikeDamage`, `apply_skill_effect:1378` prices it with live Might/Fury and fires `OnHit` procs; `receive_control:1175` clears unlanded hits | exact — `total_damage`, `peak_protected_damage_2s` feed `evaluate_viability_gates_for` and `search_rank:227`; flow sim (`simulate_flow:1411`) feeds `realized.power` | see exposure table |
| **Path of Corruption** (Curses trait, OnHit) | approximate — the trait is a player choice, but the fixture build runs Spite/Soul Reaping/Reaper, so it reaches the timeline only by injection (`records()`) | exact — `validate_specializations` resolves trait names to ids; `all_trait_ids` carries minors + majors | exact — `engine.rs::active_normalized_effects:1579` selects `SourceType::Trait` records whose id is in `all_trait_ids`, from `effects_for_mode("WvW")` (mode split honoured) | inapplicable — an OnHit record is not folded into `SimParams`; `extract_damage_modifiers` reads trait `Fact`s, and the fixture trait has none | inapplicable — records are not rotation skills | exact — `load_normalized_effects:620` builds a `ProcSpec` (OnHit supported), `trigger_procs:1679` fires it on every landed strike with `next_ready_ms` = now + ICD, `apply_operation:1604` CorruptsBoon → clears enemy stability/protection | approximate — the corrupt only flips two enemy flags; no condition is applied (`apply_operation` CorruptsBoon arm), so `condition_dps` and `has_corrupt` do not move; `combo_activations`/`protected_damage` are unaffected | see exposure table |
| **Superior Sigil of Fire** (on-crit) | exact — the plate/player names the sigil | exact — `validate_sigils` resolves the name; `sigil_seats` records set 1 main | exact — record selected by real id 24548 via `active_sigil_ids()` (worn set only) | approximate — `combat.rs::parse_sigil_modifier:1401` has no arm for Fire; the "50% chance on crit" text lands in `DamageModifiers.unparsed`, nothing in `SimParams` | inapplicable | missing — `load_normalized_effects:636` marks `OnCrit` unsupported and counts it; there is no `trigger_procs(OnCrit)` site (CONN-00-06); zero proc damage | approximate — `unmodeled_effect_sources > 0` degrades `quality` to Provisional (`referee.rs:1227`) with a bare count | see exposure table; the name is lost (CONN-00-04) |
| **Superior Sigil of Force** (passive +5 %) | exact | exact — as above; set 1 off-hand seat | exact — record selected by real id 24615; `load_normalized_effects:622` skips `Passive` records on purpose | exact — `parse_sigil_modifier:1424` "sigil of force" arm → `strike_add_pct` → `SimParams.strike_mult` once | inapplicable | exact — the multiplier rides in `params.strike_mult` for every strike (`apply_skill_effect:1399`); not re-applied at runtime (no double count, see section 5) | exact — folded into every strike total | see exposure table |
| **Superior Rune of the Scholar** | exact | exact — `validate_rune` | missing — no rune record in any mode file, so `active_normalized_effects` lists it as equipped-but-unmodeled (counted, CONN-00-04); reason: data gap, CONN-00-01 | approximate — `stats::calculate_rune_stats:238` sums the `+N Stat` tier strings; `combat.rs::parse_rune_modifier` reads "+5% damage; +10% … above 90%" as a flat modifier — the health condition is flattened into a permanent bonus (see section 5) | inapplicable | missing — no timed rule; the conditional never switches at runtime; reason: no record and no health-threshold trigger site (`OnHealthThreshold` unsupported at `:636`) | approximate — counted once toward the Provisional reason | see exposure table |
| **Relic of the Thief** | exact | exact — `validate_relic` | missing — no relic record (data gap, CONN-00-01) | missing — `combat.rs::parse_relic_modifier:1435` knows a fixed list; the Thief text falls to `unparsed`, so the stacking bonus contributes nothing; reason: no parser arm and no record | inapplicable | missing — nothing to execute; reason as left | approximate — counted once toward the Provisional reason; no score effect | see exposure table |
| **Life force** (resource rule) | inapplicable — not a player choice | inapplicable | missing — `engine.rs::wvw_resource_rules:1662` has arms for Initiative/Energy/Adrenaline/Illusions/Blades only; Necromancer emits no rule; reason: CONN-00-01 | inapplicable | missing — `PreparedRotation` carries no resource; shroud skills are prepared as ordinary `Profession_*` skills with cooldowns | missing — `Timeline::can_pay_resource` sees no rule so every shroud skill is free; `simulator.rs` has no resource ledger at all | approximate — `resource_model_complete == false` (allowlist, CONN-00-09) degrades quality with the "outside the bounded resource ledger" reason — the honest outcome, by list rather than by model | see exposure table |
| **Reaper Shroud 4 whirl finisher + dark field** (Soul Spiral in Well of Suffering / Nightfall) | exact — skills are player choices; opener order pins them | exact — `validate_skills` (utilities), `skills.profession` for shroud skills | inapplicable — combo comes from skill facts, not records | inapplicable | exact — `builder.rs:395-409` `Fact::ComboField{Dark}` → `ComboField{duration}`, `Fact::ComboFinisher{Whirl,100}` → `ComboFinisher` | missing — `resolve_combo:1509` counts a dark-field combo as unmodeled (`:1549`, "auras or life-steal, not Blind"); `simulator.rs:751` ignores both fact kinds; reason: no dark-combo outcome model (CONN-01-02) | approximate — `combo_activations` increments, quality degrades by count | see exposure table |

### Exposure by path (column 8, per caller)

| Path | Actual caller | What survives of `RefereeReport` |
|---|---|---|
| Equipped display | `crates/addon/src/ui/main_view/resolution.rs:215` → `stats::calculate_full_stats` → `combat::extract_damage_modifiers` → `compute_3tier_combat` | nothing — the referee is never called (CONN-00-02); stats and three closed-form tiers only |
| Optimize | `crates/addon/src/ui/main_view/optimize_flow.rs` → `engine::optimize_v2` → `search_v2` (`refine_piece_swaps_within:233,270`, `repair_seed:580`, `optimize_v2_search:712,834` call `evaluate_validated_build`, rank by `search_rank`) → `engine.rs:2242 synergy_result_from_validated` → `optimization.rs:10 synergy_result_to_suggestion` → `crates/addon/src/ui/comparison.rs` | stats, three combat tiers, `rotation` (gate sim incl. `wvw`), `data_quality` + `quality_reasons` (count text), gates recomputed with the narrower set (CONN-00-10); `user_intent_score`/`realized` are NOT on `SynergyResult` and never reach the UI |
| Choya | `crates/addon/src/ui/main_view/chat_flow.rs:421 plate_shortfall` (calls `evaluate_validated_build:1283`, keeps gate remedies as concerns and the rank comparison) → suggestion at `:924` built with `..Default::default()` → `attach_chat_stats` (`optimization.rs:1020`) | gate remedies as free text; validation warnings and gear reasons appended to `quality_reasons`; `data_quality` stays `Verified` (CONN-00-05); no rotation, no viability report |
| Published import | `crates/addon/src/ui/main_view/provider_picks.rs:277 adopt_provider_pick` | nothing — the referee is never called (CONN-00-03) |
| Search | `crates/optimizer/src/search_v2.rs` as above; `referee::search_rank:227` | the nine rank keys (viable, gates passed, sequence, outcome, execution, …); the report itself is dropped after ranking |
| Choya tool (`score_build`, after T033–T038) | `crates/optimizer/src/gemini_tools.rs::exec_score_build` full-build mode → `validate_gemini_build` → `evaluate_validated_build_with(…, &[])` | `viable`, gates, `user_intent_score`, six realized axes, `quality`, reasons, `coverage_note`, warnings |

### CONN-01 findings from the matrix

| Id | Evidence | Observed | Affected | Reproduction | Expected | Remedy | Classification | Links | Status |
|---|---|---|---|---|---|---|---|---|---|
| CONN-01-01 | `crates/optimizer/src/combat.rs:1435 parse_relic_modifier` (fixed name list), `parse_rune_modifier` flattening "+10% while above 90%"; no rune/relic record in `data/normalized_effects` | rune and relic effects exist only as string-parsed flat modifiers; conditional and stacking bonuses are flattened or dropped | all modes; every caller | matrix rows Scholar / Thief; `report.modifiers.unparsed` on the fixture names the Thief text | timed or conditional records for runes and relics | none this sprint (CONN-03 C) | data gap | — | demonstrated |
| CONN-01-02 | `crates/optimizer/src/rotation/wvw_timeline.rs:1549 resolve_combo` dark arm; `simulator.rs:751` ignores combo facts | dark-field combos are counted unmodeled in WvW and ignored in PvE/PvP | all modes | `reaper_fixture_evaluates_without_errors` (`combo_activations` ≥ 1 with the count degraded) | a dark combo produces its aura / life-steal outcome or is named in the coverage line | names carried by the coverage line (section 7); outcome model is CONN-03 C | runtime gap | — | demonstrated |
| CONN-01-03 | `crates/optimizer/src/engine.rs:1383 simulate_prepared` builds the WvW timeline only when `scenario.game_mode == GameMode::WvW`; `simulator.rs` has no proc engine | normalized effect records (OnHit, OnSkillUse, …) are executed in WvW only; in PvE and PvP the mode-split files are selected for nothing at runtime | PvE, PvP; every caller | `reaper_pve_comparison_uses_adaptive_scheduler_not_opener` (`report.rotation.wvw.is_none()`) | procs execute in every mode, or the PvE/PvP result says timed effects were not simulated | none this sprint | runtime gap | — | demonstrated |

## 5. Open questions answered

One line per question from `docs/simulator-trust-plan.md` Phase 1.

- **Weapon swap and stowed sigils** — answered: `engine.rs::active_normalized_effects:1602` selects records for `validated.active_sigil_ids()` (set 1 seats only) and `Timeline::new:549` loads `proc_specs` once; `try_weapon_swap:1039` changes `active_weapon_set` and nothing else. A stowed sigil cannot affect a result (negative control b), and a swap cannot bring set 2's sigils in — CONN-00-07 is a real gap in the other direction.
- **Passive double counting** — answered: no. `load_normalized_effects:622` skips `TriggerRule::Passive` records with the comment that they are already in `SimParams`; `parse_sigil_modifier` folds Force once into `strike_mult`. Conditional modifiers ARE flattened: Scholar's "+10% above 90%" becomes a permanent bonus in `parse_rune_modifier` (CONN-01-01).
- **On-hit semantics** — answered: strikes only. `trigger_procs(TriggerRule::OnHit, …)` is called from the `StrikeDamage` arm of `apply_skill_effect:1407` and nowhere else; `tick_conditions:1024` never fires procs, so condition ticks are not "hits" at this caller. The `TriggerRule::OnHit` doc comment says "strike or condition tick"; the runtime disagrees with the vocabulary.
- **Support boons vs self uptime** — answered: assumed external. `combat::buff_profiles_for_profession:571` supplies Solo/Party/Squad boon profiles from `rotation_profiles` regardless of what the build generates; the timeline's `apply_buff` tracks only what the build's own skills apply. The closed-form tiers can therefore credit boons a solo build never has; the timeline cannot credit allies it never sees. Nothing mixes the two.
- **Starved non-damage skills** — answered: yes in the gate sim, no in the flow sim. `simulator.rs::pick_skill:463` ranks by DPCT (`params.intent == None`), so a heal or buff with no damage is only chosen inside the setup window or as a tie-break; `simulate_flow:1411` sets `intent = Some(weights)` so the scheduler prices heals/boons/control. In the WvW timeline the opener is pressed in order and the scorer takes over (`pick_skill:926`) with heal/cover/strip/control priorities.
- **Gate / flow / tooltip / tool parity** — answered: the gate sim (`simulate_prepared`, scenario window, downstate dummy) and the flow sim (`simulate_flow`, 60 s open dummy, radar intent) run different scenarios by design and both are recorded here; the equipped tooltip (CONN-00-02) and the Optimize gate list (CONN-00-10) do not use the referee's assumptions; the Choya tool did not reach the referee at all before T033–T038.
- **Warning survival** — answered: validation warnings survive into the Optimize explanation ("Warnings: …") and into `quality_reasons` via `attach_chat_stats`; coverage limitations survive only as a count; stale-data status survives through `stale_trait_lock_reasons` on Optimize only; the Choya suggestion's `data_quality` is always `Verified` (CONN-00-05).
- **Chill uptime on the enemy** — left open: no enemy-cooldown state exists (`tick_recharge_rate:727` reads the player's `incoming_conditions`), so it cannot be measured; recorded as CONN-00-08.

## 6. Experiments

All seven kinds exist on the Reaper slice and run in `cargo test -p gw2-optimizer --lib` (filter `reaper_`). Every one was seen failing once under the disabling change listed; the failing assertion text is quoted from that run. Findings move from `hypothesis` on that evidence.

| Kind | Test | File | Disabling change | Failing assertion observed | Verdict | Finding served |
|---|---|---|---|---|---|---|
| positive control | `reaper_positive_control_onhit_proc_changes_events` | `crates/optimizer/src/rotation/wvw_timeline.rs` (`reaper_experiments`) | runtime: removed the `trigger_procs(OnHit, …)` call in `apply_skill_effect` | `Path of Corruption fires on the opener's landed hits when its record is active; trace: [HitLanded Well of Suffering 149.4 … Soul Spiral 70.0]` | OnHit records fire on landed strikes and only when equipped; runtime exact | CONN-00-06 (contrast), section 5 on-hit semantics |
| negative control (a) | `reaper_negative_control_wrong_mode_and_stowed_set` | same | data selection: `active_normalized_effects` reads the PvE file for every mode | `under WvW the skill is an equipped source with no record, not a loaded proc: ["Superior Sigil of Fire (on-crit)", … 40 "(no record)" names, none of them Signet of Undeath]` (the record was loaded and consumed as an unsupported proc instead) | a record present only in the PvE file is never loaded as a proc under WvW; the skill shows as `(no record)` | data selection exact; CONN-01-03 |
| negative control (b) | same test | same | data selection: every socketed sigil counted, not the worn set | `stowed: the sigil is absent from the fight` | a stowed sigil neither fires nor is named; CONN-00-07 is the reverse gap (a swap never brings set 2 in) | CONN-00-07 demonstrated |
| timing (ICD) | `reaper_timing_icd_interrupt_and_late_buff` | same | fixture: Path of Corruption ICD set to 0 | `procs 100 ms and 200 ms are inside the 10 s ICD` | consecutive procs ≥ 10 000 ms apart; skipped hits are traced as `ProcSkippedIcd` | runtime exact |
| timing (interrupt) | same test | same | runtime: an interrupt no longer clears `scheduled_hits` | `interrupted 2 hits vs calm 2 hits` | a control at 300 ms into a 500 ms two-hit cast loses the second hit and traces `CastInterrupted` | runtime exact |
| timing (late buff) | same test | same | runtime: strikes ignore live Might | `the recast under Might hits harder: [1120.5, 1120.5, 1120.5, 1120.5] vs [1120.5, 1120.5, 1120.5, 1120.5]` | Might pressed after a hit does not raise that hit; the recast under Might hits harder | runtime exact |
| ablation | `reaper_ablation_enabler_and_payoff` | same | runtime: no proc source ever matches its trigger | `complete mechanism fires: [HitLanded Gravedigger 1120.5, HitLanded Gravedigger 1120.5]` | complete > missing-enabler (0) and complete > missing-payoff (0); the expected event difference is stated in the test before the assertion | runtime exact |
| interaction pair | `reaper_interaction_might_times_strike_modifier` | `crates/optimizer/src/rotation/simulator.rs` | runtime: live Might stacks read as 0 | `each factor alone raises damage: 36426.6 35579.5 40069.3` | `f(A+B) − f(A) − f(B) + f(base) > 1.0`: Might and `strike_mult` multiply, as the model says | runtime exact (PvE simulator) |
| unsupported control | `reaper_unsupported_oncrit_is_named_not_zeroed` | `wvw_timeline.rs` (`reaper_experiments`) | (1) at baseline the report had no `unmodeled_sources` field: `error[E0609]: no field 'unmodeled_sources' on type 'WvwCombatReport'`; (2) after the remedy, with the name push removed: | `the sigil is reported as unmodeled, not silently zeroed` | zero `ProcFired`, `ProcUnmodeled` present, `total_damage` equal with and without the sigil, and `unmodeled_sources` names `Superior Sigil of Fire (on-crit)` | CONN-00-04 remedied, CONN-00-06 demonstrated |
| parity (referee vs Optimize) | `reaper_parity_referee_matches_optimize_suggestion` | `crates/optimizer/src/referee.rs` | Optimize packaging priced stats in PvE while the referee priced WvW | `effective_power: referee 4521.709235172872 vs optimize 4609.509414496616` | stats, three tiers, quality, WvW totals, unmodeled names and the coverage reason agree within 1e-9; `user_intent_score`, `realized` and `viability` do not exist on the Optimize exposure (CONN-01-04) | CONN-00-10 demonstrated (documented, not asserted), CONN-01-04 |
| parity (referee vs Choya tool) | `score_build_full_mode_matches_referee` | `crates/optimizer/src/gemini_tools.rs` | not run against a disabling change: the tool calls the referee directly; the test asserts `user_intent_score`, six axes, `viable`, `quality`, gate count within 1e-9 and that `coverage_note` names the on-crit sigil | — | tool and referee agree on the same plate | FR-018, SC-005 |
| PvE comparison | `reaper_pve_comparison_uses_adaptive_scheduler_not_opener` | `referee.rs` | runtime: the WvW timeline built for every mode | `PvE never runs the timeline; the opener is not pressed and no record executes (CONN-01-03)` | PvE runs the adaptive scheduler only; gate-sim DPS is 0 in the 2 s Solo window (CONN-01-05) while the flow axis is positive | CONN-01-03, CONN-01-05 |

Diagnostics: `trace_is_empty_unless_requested` (production `simulate_prepared` never traces; `find_references(WvwTimelineInput)` lists only `engine.rs::simulate_prepared_with`, which passes the flag through, and `simulate_prepared` passes `false`) and `trace_caps_at_512_and_flags_truncation`.

Budget (SC-003): `cargo test -p gw2-optimizer --lib` after all experiments: 1129 passed, 4 ignored, 16.80 s harness time against 16.46–16.83 s at baseline — within noise, nothing marked `#[ignore]`. Workspace gates at sprint close: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo build --release` all pass.

### Findings added by the experiments

| Id | Evidence | Observed | Affected | Reproduction | Expected | Remedy | Classification | Links | Status |
|---|---|---|---|---|---|---|---|---|---|
| CONN-01-04 | `crates/optimizer/src/engine.rs::synergy_result_from_validated` returns `SynergyResult { stats, combat_*, rotation, data_quality, quality_reasons }`; no `user_intent_score`, `realized`, `viability` | the Optimize exposure never carries the score that ranked the build; `synergy_result_to_suggestion` recomputes gates (CONN-00-10) and shows closed-form indices only | Optimize display, all modes | `reaper_parity_referee_matches_optimize_suggestion` (comment at the end) | the suggestion carries the referee's score and axes | none this sprint (CONN-03 B) | reporting gap | — | demonstrated |
| CONN-01-05 | `crates/optimizer/src/rotation/combat_model.rs::simulation_window_ms_for_mode` (PvE Solo/StrikeSpike = 2 000 ms); `simulator.rs::pick_skill` setup window | in the 2 s PvE Solo gate window the setup priority casts the elite and the stability skill and no strike lands: gate-sim `total_dps == 0` for the fixture while the 60 s flow axis is positive | PvE Solo; gate simulation only (PvE gates read EHP, not DPS) | `reaper_pve_comparison_uses_adaptive_scheduler_not_opener` | gate-sim DPS is either a signal or not read at all in PvE Solo | none this sprint | runtime gap (no score effect today) | — | demonstrated |
| CONN-01-06 | `crates/optimizer/src/engine.rs::active_normalized_effects` (`equipped − modeled` over traits, skills, rune, sigils, relic) | every equipped source without a proc record is reported "not simulated (no record)", including weapon skills whose strikes ARE executed from facts; for the fixture that is 40 names beside the one on-crit sigil | WvW; referee, Optimize, Choya | run any `reaper_*` experiment with the trace and read the `ProcUnmodeled … (no record)` events | "no record" is reported for sources whose mechanic is a proc or modifier, not for skills executed from facts | ordering only this sprint: timeline-detected sources (on-crit, dark field, unsupported proc) are named first, "(no record)" last, so the line leads with what matters; the semantics are CONN-03 B | reporting gap | CONN-00-04 | demonstrated |

## 7. Remedy

**Applied**: the coverage qualification carries names end to end (CONN-00-04) and the Choya suggestion carries the referee's quality (CONN-00-05).

- Failing experiment: `reaper_unsupported_oncrit_is_named_not_zeroed`, first failing at compile time (`no field 'unmodeled_sources' on type 'WvwCombatReport'`), then at runtime with the name push disabled (`the sigil is reported as unmodeled, not silently zeroed`); passing after the remedy unchanged.
- Change: `WvwCombatReport.unmodeled_sources: Vec<String>` replaces the `u32` count (`"{name} ({why})"`, deduplicated, timeline-detected first); `engine::active_normalized_effects` returns the equipped-without-record names resolved through the db; `data::quality::coverage_reason` is the one place that renders `Not simulated: a, b, c and N others` for both `referee.rs` and `engine.rs`; `BuildSuggestion.coverage_note` is drawn in muted text beside the quality marker through `quality.coverage_line` (12 locales); the Choya suggestion takes `data_quality`, `quality_reasons` and `coverage_note` from the `plate_shortfall` referee report and the line is appended to the plate's concerns.
- Commit: `2dc3f04` (optimizer + addon + locales); the `score_build` full-build mode and its parity test follow in the next commit.
- No calibrated constant, gate, threshold or formula changed. The `Provisional` classification fires in exactly the cases it did before (any non-empty list), because the list is the old count with names.
- Bound added for CONN-00-11: three full-build `score_build` evaluations per chat request (`chat_flow.rs::full_build_budget`); threading `is_cancelled` into `simulate_prepared` stays a follow-up.
- In-game check (quickstart): not run this sprint — the release condition (free models answer and a player produced a build with Choya on the latency DLL) has not been confirmed, so the commits stay local.

## 8. Sprint 2 — WvW proc firing sites (`specs/005-wvw-proc-sites`)

Baseline for this section: branch `005-wvw-proc-sites` off `6e4820c`; commits `ce7b32c` (foundation + US1), `e531a75` (US2), `ee4b62f` (US3), `ebae1c1` (US4), `8fd944b` (US5), `7f85e08` (US6). Every control below was seen failing before its mechanism existed and again under `python docs/audit/disable_and_run.py <entry>`; the quoted blocks are in `docs/audit/sprint2-failures.md`.

### Findings closed or moved

| ID | Status | What changed |
|---|---|---|
| CONN-00-06 | closed (WvW) | `TriggerRule::OnCrit` has a firing site in `apply_skill_effect` at the crit chance the damage line prices in. Ranking uses expected value with a probability mass that starts the cooldown (R1); the trace runs eight seeded Bernoulli trials and reports mean/min/max per source (`WvwCombatReport.proc_trials`). |
| CONN-00-07 | closed | `active_normalized_effects` selects records for both weapon sets and returns each sigil's set; `ProcSpec.weapon_set` and `held_set_for` fire a sigil only for hits cast on its set; cooldowns persist across swaps; a set-2 sigil with a record leaves the coverage line. |
| CONN-01-01 | closed (WvW) | `OnHealthThreshold` and stacking `OnHit` strike records become `ConditionalSpec`s evaluated per strike and per tick; the parser keeps flattening health-gated rune clauses everywhere and tags them (`DamageModifiers.conditional_strike`), and only the WvW branch divides an executed clause out of `strike_mult` (`wvw_params_without_executed_conditionals`). PvE/PvP output is byte-identical (`pve_output_unchanged_by_conditional_tagging`). |
| CONN-01-02 | closed (WvW, dark arm) | `resolve_combo` dark arm: whirl = leeching bolt (198 + 0.03 power, 170 + 0.05 healing power; one bolt per activation, the page states no count), leap = Dark Aura 5 s, blast = area Dark Aura 3 s (incoming condition damage −20 %), projectile = life stealing left named unread. |
| CONN-00-01 | closed for life force | `ResourceKind::LifeForce` (69 % of health), rules from `Life Force` / `Life Force Per Hit` facts and `data/formulas/shroud.json` (10 % entry floor, per-mode drain and reduction), the shroud bar prepared from the core entry skill's `transform_skills`, `SHROUD_SET` as the held set in shroud. |
| CONN-00-09 | closed | `resource_model_complete` is derived: rules non-empty and every skill that names a resource (initiative, cost, a shroud entry, a life force fact) has one. Pinned for nine professions (`resource_model_completeness_matches_previous_list`): Thief, Revenant, Warrior, Mesmer, Necromancer complete; Guardian, Elementalist, Engineer, Ranger not. |
| CONN-01-05 | re-recorded | With the fixture's shroud bar moved to the shroud set (a fixture change), the PvE simulator no longer holds Infusing Terror in the 2 s window and a strike lands: gate-sim DPS is no longer zero. The PvE guard was re-pinned once for the same change. |
| CONN-01-03 | open | PvE and PvP still execute no records; the adaptive scheduler ignores `SHROUD_SET` skills (as it never prepared them before). |
| CONN-01-06 | open | "(no record)" still lists executed weapon skills; the fixture's production coverage line is 40 names, all "(no record)", none of them a sigil, rune or relic any more. |

### Experiments added

| kind | test | disabled by (harness entry) | seen failing | passes now |
|---|---|---|---|---|
| positive control (on-crit) | `reaper_oncrit_positive_control_fires_from_crits` | `oncrit`: the OnCrit call gated off | `the on-crit sigil fires from the opener's critical hits; trace: [ProcUnmodeled "Superior Sigil of Fire (on-crit)" "no firing site" …]` | `ProcFired` for the sigil, absent from the coverage line, more total damage than without it |
| negative control (zero crit) | `reaper_oncrit_zero_precision_never_fires` | guard | passes before and after | no `ProcFired`, total damage equal to the bare build within 1e-9 |
| timing (cooldown) | `reaper_oncrit_icd_bounds_rate` | `oncrit` | `one fire inside one 5 s cooldown window … left: 0 right: 1` | exactly one fire, later hits `ProcSkippedIcd` |
| trials bracket expected value | `reaper_oncrit_trials_bracket_expected_value` | `oncrit` | `trials exist for the sigil: []` | mean within one proc of the expected count; production fixture: `ProcTrial { mean: 1.0, min: 1, max: 1 }` |
| positive/negative (swap) | `reaper_swap_loads_set_two_sigils` | `swap`: set 2 mapped to set 1 | `set-2 sigil fires only after the swap at 1350 ms: []` | fires only after the swap; moved to set 1, only before |
| timing (swap) | `reaper_swap_keeps_icd_across_sets` | guard | passes before and after | one cooldown across the swap |
| coverage (swap) | `reaper_set_two_sigil_leaves_coverage_line` | `swap` | `the stowed sigil's record is loaded (it has a trial entry): []` | trial entry present, not named |
| positive/negative (threshold) | `reaper_scholar_applies_only_above_threshold` | `threshold`: threshold forced true | `the threshold is true at the start of the fight: []` | `ConditionalActivated` at 0 ms, ×1.05 above, `ConditionalExpired` at the crossing, no bonus below |
| timing (stacks) | `reaper_thief_stacks_cap_and_expire` | `stack`: cap removed | `six qualifying weapon-skill hits gain or refresh: []` | 5/5 reached and refreshed, expiry 6 s after the last gain |
| unresolved stays named | `reaper_unresolved_conditional_stays_named` | — | `named as unresolved: ["Superior Rune of the Scholar (on-health-threshold)"]` | "(unresolved value)", no activation |
| PvE guard | `pve_output_unchanged_by_conditional_tagging` | — | pinned before the parser change | identical to 1e-9 |
| positive (dark combo) | `reaper_dark_whirl_life_steals` | `dark`: arm returns to the degraded note | `the dark whirl combo resolves: []` | `ComboResolved "Dark field + Whirl finisher → leeching bolt"`, heals and damages, degraded note gone |
| negative (expired field) | `reaper_expired_field_makes_no_combo` | guard | passes before and after | no combo after 5 s |
| real records | `reaper_cached_build_has_recorded_sources` (`#[ignore]`, dev.cfg) | — | — | cached Reaper build (Inquisitor of Pain tab 1: Dolyak, Celerity, Leeching, Energy, Nullification — no records) evaluates; with Fire, Scholar, Thief swapped in all three execute and leave the coverage line |
| refusal (shroud) | `reaper_shroud_refused_without_life_force` | `shroud_floor`: entry floor ignored | `entry refused: [… no ShroudRefused …]` | `Reaper's Shroud needs 10% life force, had 6%` before any entry |
| gain and cap (life force) | `reaper_life_force_gain_capped` | `shroud_floor` run | `Gravedigger's 8 % fact is credited: []` | `LifeForceGained "8% → 8%"`, a 1e9 gain caps at the pool |
| timing (drain and exit) | `reaper_shroud_drains_and_exits` | `drain`: drain zeroed | `entered after the generators: [… no ShroudEntered …]` | entered at 14 %, `ShroudExited "life force 0"` within 2–3.5 s, no weapon skill lands inside |
| builder (shroud bar) | `shroud_bar_is_prepared_from_transform_skills` | — | could not compile before `shroud_bar_for_build` | Reaper ids in slot order, core five for a core build, nothing for a Warrior |
| completeness rule | `resource_model_completeness_matches_previous_list` | — | Necromancer `false` under the allowlist | nine professions pinned |
| determinism | `reaper_results_repeat_identically` | — | — | ten runs identical, trials included |
| trace cap | `reaper_trace_fits_under_cap` | — | — | under 512 on both profiles |

### Re-recorded anchors

The fixture opener is now Gravedigger, Death Spiral, Well of Suffering, Reaper's Shroud, Soul Spiral, Life Rend (generators first: life force starts at zero and shroud needs 10 %). The open-profile trace, 2026-09-08:

```
400 HitLanded Gravedigger 1120.5 · 750 LifeForceGained Gravedigger 8% → 8% · 1800 LifeForceGained Death Spiral 6% → 14%
2100–2500 HitLanded Well of Suffering 149.4 ×5 · 2500 LifeForceGained Well of Suffering 5% → 19%
2700 ShroudEntered Reaper's Shroud 19% life force · 3750–4150 HitLanded Soul Spiral 70.0 ×8 · 4150 ComboResolved Soul Spiral Dark field + Whirl finisher → leeching bolt
4850 HitLanded Life Rend 1307.2 · 5950–6250 HitLanded Death's Charge 311.2 ×3 · 6550 CastInterrupted Life Rend 1 hits lost · 6550 ShroudExited Reaper's Shroud life force 0
totals: damage 19448.8, combos 1, refusals none
```

Section 6's anchor `[HitLanded Well of Suffering 149.4 … Soul Spiral 70.0]` for the Sprint 1 positive control is superseded by the order above; the experiment itself is unchanged and passes.

### Test budget

Sprint 1 baseline: optimizer lib harness 16.80 s (16.46–16.83 s). Sprint 2 runs this session: 16.57, 16.61, 16.79, 16.86, 16.95, 17.20, 17.48, 17.84, 18.00, 19.01 s, with 1 154 tests against 1 129. Growth is under 2.5 s, inside the 10 s cap (FR-011). The eight-seed trial pass runs only under `trace`, in tests. The cached-build test is `#[ignore]`.

### T042 finding: no trait records for the cached Reaper build

The cached build's nine traits (Bitter Chill, Spiteful Fortitude, Dread, Shrouded Removal, Dark Defense, Corrupter's Fervor, Chilling Nova, Decimate Defenses, Blighter's Boon) got no record: Chilling Nova is on-crit against a *chilled* foe, an enemy-condition prerequisite the record schema and the runtime have no field for; Dread is a trait-owned on-skill-use (only skill-owned ones fire); Spiteful Fortitude and Blighter's Boon are life force gains that US6 reads from facts; the rest are passives or carapace mechanics. Follow-ups: an enemy-condition prerequisite on `NormalizedEffect`, trait-owned `OnSkillUse`, and the `Life Force per 3 Seconds` / `When Ending` fact variants, which the rules skip.

### Known approximations added

- Expected-value scaling of stack and count operations rounds to the nearest whole stack (`apply_operation`).
- One leeching bolt per whirl activation (the wiki states no count).
- Dark Aura's torment-on-strike retaliation is not modeled.
- Overflow damage past the life force pool reaches health at the reduced value.
- A shroud entry the pool cannot afford is skipped in the opener with a reason, not queued.

## 9. Sprint 3 — trait triggers are the build (`specs/007-trait-triggers`)

Baseline for this section: branch `007-trait-triggers` off `255371f`; plan `0e50a51`, tasks `91a64aa`. Every control below is seen failing before its mechanism exists and again under `python docs/audit/disable_and_run.py <entry>`; the quoted blocks are in `docs/audit/sprint3-failures.md`.

### 9.1 Coverage truth

CONN-01-06 closed. `active_normalized_effects` now returns a `Vec<CoverageEntry>` (`data/quality.rs`: `ReasonClass::{NoRecord, PassiveNoEffect, NeedsMechanic, UnresolvedValue, NoFiringSite}`): a trait the parser consumed a fact from (`DamageModifiers.consumed_trait_ids`, carried on `PreparedRotation`) or a skill the builder produced a `SkillEffect` for is executed and never named; a record with a `coverage` block puts its class on the list; the timeline's own load-time notes (`(on-crit)`, `(unresolved value)`, `(partial combo)`) are classified back into entries in `report()` and keep their wording. `WvwCombatReport.coverage` is the typed list and `unmodeled_sources` its rendering; an empty list yields no coverage reason (`coverage_reason` unchanged).

| kind | test | disabled by (harness entry) | seen failing | passes now |
|---|---|---|---|---|
| coverage (executed skills) | `coverage_line_never_names_executed_weapon_skills` | `coverage`: the `executed_from_facts` skip gated off | `executed weapon skills leave the coverage line: ["\"Chilled to the Bone!\" (no record)", …, "Gravedigger (no record)", …]` (40 names) | no weapon skill on the line |
| coverage (class) | `coverage_entry_carries_its_class` | — (could not compile before `coverage`) | — | `Flesh of the Master (needs: minions)`, class `NeedsMechanic("minions")` |
| coverage (empty) | `nothing_skipped_is_verified` | — | — | empty `coverage`, empty line, no reason |
| PvE pin | `pve_trait_fact_consumption_set_unchanged` | — | — | fixture consumed set empty in PvE and WvW; a percent-fact trait consumed, a bare one not |

Quoted blocks: `docs/audit/sprint3-failures.md` § coverage. Full lib run after the step: 1 165 passed, 17.95 s.

### 9.2 Shroud

US1 closed for the mechanism. `enter_shroud` ends with `trigger_procs(OnShroudEnter, entry skill)` and `exit_shroud(why)` starts with `trigger_procs(OnShroudExit)` while the state still stands (so an in-shroud prerequisite on an exit record holds); `why` reaches the `TraitFired` detail. `ConditionalKind::InShroud` is built from a `Conditional` record with `prerequisite.in_shroud: true` (strike or crit-damage payload), switched in `update_conditionals` (called at both transitions) and traced as `ShroudBonusActive`/`Ended`; a crit-damage bonus enters the strike through `strike_crit_factor_with_crit_damage` (percentage points on top of the ferocity multiplier). `ProcSpec.prerequisite` is carried; this step evaluates the shroud member only (`prerequisite_holds`), a foe member refuses with `foe prerequisite not evaluated` until 9.3. Every proc keeps its `ProcFired`; a trait record adds `TraitFired` with `at entry` / `at exit ({why})` and counts into `trait_fire_counts`. A shroud record that never fired because no shroud was entered goes on the coverage line as `(shroud never entered)` at the end of the run.

| kind | test | disabled by (harness entry) | seen failing | passes now |
|---|---|---|---|---|
| positive control (entry) | `necro_shroud_enter_fires_once_at_entry` | `shroud_enter`: the entry call a no-op | `the entry record fires once at the entry; trace: [… no TraitFired …] left: 0 right: 1` (before the site: `ProcUnmodeled "Speed of Shadows (on-shroud-enter)" "no firing site"`) | one `TraitFired` at the `ShroudEntered` tick, Swiftness on |
| timing (exit, three ways) | `necro_shroud_exit_fires_for_every_why` | `shroud_enter` run (exit needs an entry) | — | one fire each: `at exit (exit skill)`, `at exit (life force 0)` by drain, by a 6 000 strike at 3 000 ms |
| conditional (in shroud) | `necro_in_shroud_bonus_active_only_inside` | — | — | Gravedigger equal with and without, Life Rend + Soul Spiral higher, `×1.15 crit damage` on at entry, off at exit |
| Scourge rule | `necro_desert_shroud_is_the_scourge_entry` | — | — | Desert Shroud enters and fires; Manifest Sand Shade never |
| ablation | `necro_removed_trait_changes_results` | — | — | an entry burst record raises `total_damage`; without it no fire |
| determinism | `necro_results_repeat_identically` | — | — | ten runs identical (trace, coverage, counts) |
| refusal (no shroud) | `necro_shroud_trigger_without_shroud_floor_never_fires` | — | — | refused entry, no fire, `Speed of Shadows (shroud never entered)` with class `NoFiringSite` |

Deferred to 9.5: a timed strike bonus after a proc (Soul Barbs' +10 % for 10 s) needs a `ConditionalKind` with an expiry; the entry-burst record stands in for it here. Full lib run after the step: 1 172 passed, 16.33 s.

### 9.3 Prerequisites and scopes

US2 closed for the mechanism. `prerequisite_holds` evaluates `foe_condition` against the unexpired outgoing conditions, `in_shroud` against the shroud state and `foe_health` against `enemy_health / target_health` (never met on an open dummy, reason `foe health unknown`); every refusal is traced as `ProcSkippedPrerequisite` and a record that never fired for that reason gets one `prerequisite never met` summary at the end of the run. `RotationSkill.categories` / `slot_name` come from the API skill; `scope_admits` matches `Category`, `Slot` (slot head before `_`) and `Status` (the name of the status trigger in progress, `Timeline.trigger_status`), and the loader admits a trait `OnSkillUse` that carries a scope. New sites: `apply_outgoing_condition` (the one push for skill facts, corrupts and record operations) fires `OnConditionApplied`; `apply_buff` fires `OnBoonApplied`; `remove_enemy_boons` fires `OnBoonStripped` per boon; the tick fires `Periodic` when a periodic record is loaded (period = `internal_cooldown`, first fire at 0 ms). Status triggers never nest (`status_trigger_depth`), so a boon-on-boon record cannot feed itself. `GainsLifeForce` credits the pool through `gain_life_force_percent` (capped, `LifeForceGained` trace); `Heal` heals `value + coefficient × healing power`; `scale_by: ConditionsRemoved` multiplies by the conditions the same trigger's earlier record removed.

Two WvW-only routings found on the way (PvE/PvP builder output untouched, FR-009): a non-damaging condition on a skill fact (Chilled, Crippled, Weakness, Vulnerability, ...) reaches the timeline as `ApplyBuff` and used to land on the *player* as a self-buff; it is now an outgoing condition on the foe when `data/formulas/conditions.json` knows the name. Fear and Taunt are crowd control *and* conditions, so the control arm also applies them as conditions. No `reaper_*` pin moved.

| kind | test | disabled by (harness entry) | seen failing | passes now |
|---|---|---|---|---|
| positive/negative (foe prerequisite) | `necro_chilled_prerequisite_gates_chilling_nova` | `prereq`: the `foe not Chilled` refusal gated off | fires from the first crit at 400 ms: `never before the chill` | refused at 400/750/2 000 ms, fires at 2 500/2 800 ms, refused again after 7 100 ms; unchilled opener never fires |
| scope (category) | `necro_shout_scope_fires_on_shouts_only` | `scope`: the `Category` arm forced false | `one fire inside the 30 s cooldown: left: 0 right: 1` | one fire, `ProcSkippedIcd` on the second shout, none without the category |
| scope (slot) | `necro_slot_scope_fires_on_elite_only` | `scope` run | — | Elite fires once, Heal never |
| status site (condition) | `necro_fear_applied_fires_dread` | — | Fear was control only: `left: 0 right: 1` | `Status("Fear")` fires on the fear skill, `Status("Chilled")` on Grasping Darkness, in that order |
| status sites (boon) | `necro_boon_applied_and_stripped_fire` | — | — | one fire each on a Fury cast and on a corrupted Stability, both into the life force ledger |
| timing (periodic) and scaling | `necro_periodic_and_exit_life_force` | — | — | ticks at 0/3 000/6 000/9 000 ms; exit cleanse removes 2, the scaled record credits `14% →` |
| heal route | `necro_heal_route_uses_healing_power` | — | — | healing delta = 133 + 0.1 × healing power |
| refusal summary | `necro_prerequisite_never_met_is_traced_not_listed` | — | — | `prerequisite never met` at the end, absent from `coverage` |
| timing (cooldown) | `necro_long_cooldown_fires_once_and_traces_refusal` | — | — | one fire, `ProcSkippedIcd` after |

Quoted blocks: `docs/audit/sprint3-failures.md` §§ prereq, scope. Full lib run after the step: 1 181 passed, 15.64 s (baseline median 17.01 s, SC-006 holds).

### 9.4 Population

### 9.5 Necromancer catalogue

### 9.6 Timing

Sprint 2 baseline re-measured 2026-09-08 at `79e731d` before any Sprint 3 code: `cargo test -p gw2-optimizer --lib` reports `finished in` 19.08 s, 17.01 s, 16.65 s (1 157 tests, 5 ignored). SC-006 cap for Sprint 3: within 10 % of the median 17.01 s, i.e. under 18.7 s harness time on a warm run.
