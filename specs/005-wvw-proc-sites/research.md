# Research: WvW proc firing sites

Every decision below was checked against the tree on 2026-09-08 (branch `005-wvw-proc-sites`, off the Sprint 1 tip) with SymForge, and against the wiki pages the records will cite. Wiki reads are dated; the implementation re-reads them and puts the date in each record.

## R1. On-crit probability model (Q1 = C)

**Decision.** Search and ranking use an expected-value process the plan calls "cooldown applied to the expected arrival rate". Per landed hit, if the proc is ready: apply `p × effect` where `p = crit chance × record proc chance`, and add `p` to a per-proc probability mass. When the mass reaches 1.0, subtract 1.0 and start the internal cooldown. The diagnostic trace additionally runs seeded Bernoulli trials (8 fixed seeds) and reports per-source mean and min/max proc counts.

**Rationale.** The naive scheme (scale by `p`, start the cooldown on every scaled fire) under-counts: with `p = 0.5`, two hits per second and a 5 s cooldown it yields 0.091 procs/s against the true 0.167 procs/s. The mass scheme gives exactly `1 / (ICD + 1 / (rate × p))` in expectation, is deterministic, and needs no RNG in search. Trials answer the plan's warning that fractional events plus a cooldown are not a real proc process, and they stay out of search and prompts because `trace` is only ever true in tests (`trace_is_empty_unless_requested`).

**Facts.** `apply_skill_effect` (wvw_timeline.rs:1454) already computes the expected crit factor through `strike_crit_factor_with_bonus` (simulator.rs:1315) from `params.precision`, `params.ferocity`, `crit_chance_bonus + fury bonus`; the crit chance itself is `formulas().crit_chance(precision) + bonus`, clamped to [0, 1]. The site calls `trigger_procs(OnHit, …)` on the same line; OnCrit is added there. `load_normalized_effects` (:681) lists OnCrit as unsupported and pushes the "(on-crit)" name; that push is the negative control's seen-failing anchor.

**RNG.** `rand` exists only in `server/feedback/Cargo.toml`. The optimizer gets a 10-line xorshift64* in the timeline module, seeded per trial. No dependency.

**Alternatives.** Seeded trials in ranking (Q1 B): several timeline runs per evaluation, would slow the beam search and needs a variance field in `RealizedAxes`; rejected by the user. Naive scaling: rejected for the under-count above.

## R2. Sigil of Fire record and proc damage

**Facts (wiki `Superior_Sigil_of_Fire`, read 2026-09-08).** "Trigger a flame blast with a 240 radius upon critically hitting a foe. Cooldown 5 s." Notes: the blast cannot crit, has a 0.85 damage coefficient, and uses the unequipped weapon strength 690.5. No proc chance is listed any more (the 50% was removed). Existing record `sigil:24548:0` in `data/normalized_effects/2026-01-13/wvw.json` has `category: ProcEffect`, `inner_category: StrikeDamagePct`, `value: 512.0`, `internal_cooldown: 5.0`, `evidence_level: Derived`. Through `as_ratio` (:2310) 512 becomes a 5.12 multiplier of the weapon strike, which is wrong in shape.

**Decision.** Rewrite the record: `value: 0.85` (coefficient), `evidence_level: Factual`, source URL with date. The `ProcEffect → StrikeDamagePct` arm in `trigger_procs` computes `690.5 × power / reference_armor × coefficient × strike_mult`, no crit factor, with a `// wiki Superior Sigil of Fire (2026-09-08)` comment on the constant. Records with a percent-shaped value (> 2.0) keep the old path so no other record changes meaning.

## R3. Weapon swap and set-2 sigils (CONN-00-07)

**Facts.** `active_normalized_effects` (engine.rs:1606) selects sigil records by `validated.active_sigil_ids()` (set 1 only, validation.rs:253). `Timeline::new` (:606) loads `proc_specs` once. `try_weapon_swap` (:1105) changes `active_weapon_set` and traces `WeaponSwap`. `ProcSpec` (:491) has no set. `ScheduledHit` (:420) has no set either. `SigilSlots` seat order is `[set 1 main, set 1 off, set 2 main, set 2 off]`; the dense fallback puts seat order in list order.

**Decision.** Select records for all four seats; `ProcSpec` gains `weapon_set: u8` (0 = not a sigil, 1, 2) from the seat; `trigger_procs` skips specs whose set is not held. Cooldowns persist across swaps because the spec never leaves the vector (US2 scenario 3). `ScheduledHit` captures `weapon_set` at schedule time so a swap mid-channel keeps the hits' sigils (edge case). `active_normalized_effects` counts a set-2 sigil as modeled when its record loads, so it leaves the coverage line (US2 scenario 4). Wiki `Death_Shroud`: sigils on the equipped weapon keep working in shroud, so shroud does not change the held set.

**Alternative.** Reload `proc_specs` on swap: loses cooldown state, more code. Rejected.

## R4. Conditional and stacking bonuses (CONN-01-01)

**Facts.** Scholar (wiki, read 2026-09-08): "+125 Ferocity; +5% damage while your health is above 90%". Existing record `rune:24836:1`: `OnHealthThreshold`, `TriggeredEffect → StrikeDamagePct`, `value 5.0`, no threshold field. Relic of the Thief (wiki, read 2026-09-08): "Upon striking an enemy with a weapon skill that has a recharge or resource cost, gain +1% strike damage for 6 s, max 5 stacks, refresh all stacks on trigger". Existing record `relic:100916:0` is `Passive StrikeDamagePct 10.0`, which is neither the value nor the shape. `NormalizedEffect` (normalized_effects.rs:308) has `max_stacks`, `effect_duration`, `internal_cooldown`, `status_operation`, `inner_category`; it has no threshold, no proc chance, no trigger scope. `parse_rune_modifier` → `parse_percent_clauses` flattens "+5% … above 90%" into `strike_mult` for every mode (CONN-01-01). The timeline tracks `player_health` and `params.max_health`; incoming damage lands at :1376.

**Decision (schema).** Three optional, `serde(default)` fields on `NormalizedEffect`, all `FactualValue`-bearing so unknowns stay unresolved:
- `health_threshold: Option<HealthThreshold { above: bool, percent: FactualValue<f64> }>` for `OnHealthThreshold` and `Conditional`;
- `proc_chance: Option<FactualValue<f64>>` (fraction; absent = certain) for `OnCrit`/`OnHit`;
- `trigger_scope: Option<TriggerScope>` with `Any` (default) and `WeaponSkillWithRecharge` (Thief).
Validation: `OnHealthThreshold` requires `health_threshold`; a record with `max_stacks` requires `effect_duration`.

**Decision (runtime).** A `ConditionalSpec` list on the timeline beside `proc_specs`: threshold specs evaluate `player_health / max_health` against the threshold at every strike (US3 scenarios 1, 2, edge cases: true at t=0, re-activates on the way back up) and emit `ConditionalActivated` / `ConditionalExpired` trace events on state change; stacking specs hold `stacks` and `expires_at_ms`, gain a stack on a qualifying hit (weapon skill with `cooldown_ms > 0` or a resource rule), refresh the expiry, cap at `max_stacks`, expire in `expire_timed_state`, and multiply strike damage by `1 + stacks × value`.

**Decision (no double count).** `parse_percent_clauses` tags clauses that carry "above N%" / "below N%" health wording as conditional in `DamageModifiers` (new `conditional_strike: Vec<(source_id, value)>`); the flattened value still enters `strike_mult` exactly as today so PvE and PvP scores do not move (FR-010). In the WvW branch of `simulate_prepared_with`, for each source whose conditional record actually loaded, the engine divides that source's flattened factor out of the timeline's `SimParams.strike_mult`. An unrecorded conditional keeps today's flattened approximation and stays on the coverage line.

**Alternative.** Drop conditionals from the parser for every mode: changes PvE/PvP ranking in a WvW-first sprint, and the gate-sim window in PvE is 2 s (CONN-01-05) so the loss would be invisible there anyway. Rejected.

## R5. Dark field combos (CONN-01-02)

**Facts.** `resolve_combo` (:1589) has arms for smoke, water, light and fire; the dark arm only notes "dark field". Wiki `Combo` (to re-read at implementation): dark field + blast = area Blindness, + leap = Dark Aura, + projectile = life siphon, + whirl = leeching bolts (life steal). The fixture's Soul Spiral is a whirl inside Well of Suffering / Nightfall (dark fields).

**Decision.** Add the dark arm: whirl → life steal (damage to enemy plus heal, both from the wiki's per-bolt numbers, healing-power scaled the way the water arm is), leap → `apply_buff("Dark Aura", …)` for the wiki duration, blast → `apply_defense(CoverKind::Blind, …)` as the smoke arm does, projectile → life siphon. The degraded note for "dark field" goes away; unresolved numbers keep the note (FR-008).

## R6. Life force and shroud (Q3 = C, one shared shape)

**Facts (wiki, read 2026-09-08).** `Life_force`: pool capacity = 69% of the health pool; nearby deaths refill 10% (out of scope: no deaths in the dummy fight). `Death_Shroud`: minimum 10% life force to enter; ends at zero or on manual exit; 10 s recharge on deactivation; while in shroud damage goes to life force, reduced 50% in WvW and PvP (33% PvE), overflow to health; no healing in shroud; loses 3%/s (5%/s PvP); sigils keep working; shroud counts as a weapon swap for on-swap sigils. `Reaper's_Shroud`: same shape, 4%/s in PvE, history says the PvE-only reduction from 5% left WvW at 5%/s. Traits and runes that trigger at health thresholds read the regular health pool, not life force (US3 keeps reading `player_health`).

**Facts (cached API, `dev.cfg` addons dir).** Life force gain is a `Percent` fact whose `text` is "Life Force", "Life Force Per Hit", "Life Force per 3 Seconds" or "Life Force When Ending" (40 Necromancer skills carry one; Ghastly Claws 12, Dark Pact 5, Life Reap 1.5 per hit). Shroud entry skills are `Profession_1` with `specialization: None` (Death Shroud 10574, Reaper's Shroud 30792, Harbinger Shroud 62567, Ritualist's Shroud 77238) and a `flip_skill` exit (10585, 30961, 62540, 76933). The shroud bar skills are listed in Death Shroud's `transform_skills` (57 ids) and carry misleading slots: Life Rend 29442 / Life Slash 29458 / Life Reap 30278 are `Downed_1`, Death's Charge 30825 `Downed_2`, Infusing Terror 29958 `Downed_3`, Soul Spiral 30504 `Downed_4`, Executioner's Scythe 30557 `Weapon_5`; each carries `specialization` 34 for Reaper, 64 Harbinger, 76 Ritualist, `None` for core. `builder.rs:291` only admits `Profession_` slots, so the shroud bar is never prepared today. Scourge (60) has no entry skill; its F-skills are ordinary `Profession_` slots with recharges.

**Decision (one shape, per-spec numbers).** `ResourceKind::LifeForce` with cap `0.69 × max_health` (absolute units; facts in percent of pool convert at load). New `SkillResourceRule` fields: `gain_on_use` (Percent fact "Life Force"), `entry_floor` (10% for entry skills), `drain_per_second` (per shroud entry skill: Death 3%, Reaper 5% WvW, Harbinger and Ritualist from their wiki pages or unresolved), `enters_shroud`, `exits_shroud`. Timeline: `in_shroud: Option<ShroudState>`; entry refused below the floor with reason text and a `ShroudRefused` trace event; drain each tick; skill costs of shroud skills paid on use; exit at zero, on the exit skill, or when the opener says so; incoming damage while in shroud hits life force at 50% and overflows to health; `heal()` is a no-op in shroud; a forced exit mid-channel cancels the channel the way `receive_control` does; entry counts as a weapon swap for OnSkillUse-on-swap records (not modeled; named). Builder: prepare the shroud bar from `transform_skills` filtered by the equipped specialization (or `None` for core), mapping `Downed_1..4 → shroud 1..4`, `Weapon_5 → shroud 5`, with a new `RotationSkill.bar: Bar { Weapon, Shroud, Slot }`; `pick_skill` admits `Shroud` skills only in shroud and `Weapon` skills only out of it, `Slot` (heal/utility/elite) both.

**Decision (completeness rule, FR-015).** `resource_model_complete` becomes: at least one rule exists, and every rotation skill whose facts or fields name a resource (initiative, `cost`, a Life Force fact, a Profession_ slot with a cost) has a rule. Checked against today's allowlist: Thief (Steal has no resource fact), Revenant, Warrior and Mesmer stay complete; Guardian, Elementalist, Engineer and Ranger stay incomplete as today; Necromancer flips to complete once the life force rules exist. A test pins these nine outcomes so the rule cannot drift from the list it replaces.

**Alternatives.** Q3 A/B rejected by the user. Separate models per shroud: rejected by the user (shroud is maintained the same way, only numbers change).

## R7. Records this sprint (Q2 = A)

**Decision.** Edit `data/normalized_effects/2026-01-13/wvw.json` in place (the active manifest `2026-07-15` resolves there through `inherits_from`; a new patch directory would add a manifest change with no data reason). Records: Sigil of Fire (R2), Scholar with `health_threshold {above, 90}` (R4), Relic of the Thief rewritten as stacking (R4), plus any trait on the cached Reaper WvW build whose trigger is OnCrit, OnHit or health-gated, each with wiki URL and read date, evidence `Factual` where the wiki states the number and `Heuristic` otherwise. Shroud drain and entry floor live in `SkillResourceRule` derivation from a small `data/formulas`-style table keyed by entry skill id with the same source/date fields, never in the fixture. Nothing from `reaper_fixture.rs` is copied: the fixture keeps its synthetic ids and gains its own life force facts.

**Existing-test impact.** `test_validation_*` in normalized_effects.rs gain two rules (threshold required, stacks need duration). The consistency corpus (`parser_consistency_tests.rs`) is unaffected: the parser still emits the flattened value.

## R8. Test budget and seen-failing discipline

**Facts.** Sprint 1 baseline: optimizer lib harness 16.80 s (16.46–16.83 s range), addon 5.4 s. Trials run only under `trace: true`, in tests, 8 seeds × one opener ≈ 8 × 3 ms.

**Decision.** Each new mechanic gets positive, negative and timing controls in `reaper_experiments` and `referee.rs`, each disabled once by a scripted edit (the Sprint 1 `sec6.py` pattern, restore from copies, never `git checkout`) and its failure quoted in the audit section 8. Expected budget growth: under 3 s. Multi-seed spread beyond 8 seeds is `#[ignore]`.

## R9. Trace and report additions

**Decision.** `TraceKind` gains `ConditionalActivated`, `ConditionalExpired`, `StackGained`, `ComboResolved`, `ShroudEntered`, `ShroudExited`, `ShroudRefused`, `LifeForceGained`. `WvwCombatReport` gains `proc_trials: Vec<ProcTrial { source, mean, min, max }>` (empty unless trace) and `shroud_refusals: Vec<String>` (readable reasons, feed `quality_reasons`). `TRACE_CAP` stays 512; the fixture opener must stay under it (edge case, asserted).

## R10. Ownership boundaries (FR-013)

Untouched: `crates/optimizer/src/prompts.rs`, `crates/optimizer/src/llm/`, `crates/optimizer/examples/choya_live.rs`, every scoring constant and gate threshold, the `score_build` declaration text. Commits stay local; no version bump until the Sprint 1 release condition holds.
