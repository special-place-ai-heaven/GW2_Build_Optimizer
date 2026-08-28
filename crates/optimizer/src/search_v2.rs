//! Search v2 — complete build state beam/evolutionary search.
//!
//! This module provides the foundational types and mutation operators used by
//! `optimize_v2()`.  The search loop (T02) builds on top of the primitives
//! defined here.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::balance::BalanceContext;
use crate::engine::OptimizeProgress;
use crate::gamedb::GameDb;
use crate::referee::{self, RefereeReport, ViabilityGate};
use crate::scenario::ScenarioSpec;
use crate::scoring::{self, OptimizationWeights};
use crate::synergy_pipeline;
use crate::text_util::normalize_sigil_family;
use crate::validation::{
    ValidatedBuild, ValidatedItem, ValidatedSpec, ValidatedWeaponSet, ARMOR_SLOTS, TRINKET_SLOTS,
    WEAPON_SET1_SLOTS,
};
use gw2_api::models::{Profession, Specialization};
use gw2_core::types::{BuildLocks, GearSlot, PrefixRef};

// ─── Core types ──────────────────────────────────────────────────────────────

/// A single candidate on the beam: a fully-validated build together with its
/// referee evaluation (score, viability, stats, …).
#[derive(Clone)]
pub struct BeamCandidate {
    pub validated: ValidatedBuild,
    pub report: RefereeReport,
}

/// Configuration knobs for the beam/evolutionary search.
pub struct SearchConfig {
    /// Number of candidates kept at each generation.
    pub beam_width: usize,
    /// Hard cap on referee evaluations across the entire run.
    pub eval_budget: usize,
    /// Wall-clock time limit (seconds).  The search aborts cleanly when this
    /// elapses so the caller always gets a result inside `time_limit_secs + ε`.
    pub time_limit_secs: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            beam_width: 10,
            eval_budget: 1500,
            time_limit_secs: 45,
        }
    }
}

/// Referee evaluations the nudge pass may spend.
///
/// Derived from the shape of the pass rather than picked: `MAX_ROUNDS` (4)
/// rounds × the fourteen stat-bearing slots × the canonical pool, which is 66
/// named prefixes on the live cache — about 3,700. Rounded up to 4,000 so a
/// complete pass on real game data never hits the cap, and a pool that grows
/// past what the model was measured on does.
const NUDGE_EVAL_BUDGET: usize = 4_000;

/// Wall-clock ceiling on the nudge pass, in seconds.
///
/// The eval cap alone does not bound the pass, because per-evaluation cost is
/// not a constant: a full referee evaluation runs a combat *and* a rotation
/// simulation, and rotation-heavy professions on slow machines cost multiples
/// of the measured average. Four full rounds measured ~1.2 s on live data in a
/// release build, so ten seconds is roughly eight times the measured cost — a
/// ceiling, not a schedule.
const NUDGE_TIME_LIMIT_SECS: u64 = 10;

/// What the post-beam nudge is allowed to spend.
///
/// The beam runs inside `SearchConfig`'s 1,500 evaluations and 45 seconds. The
/// nudge runs *after* that, and used to have nothing bounding it but the
/// user's Cancel button: four rounds over sixteen slots against the entire
/// 191-row itemstat map, every entry a full referee evaluation. That bound was
/// structural, not enforced. This makes it enforced, so the promise
/// `SearchConfig` documents — the caller always gets a result inside a known
/// time — covers the whole optimize and not just its first half.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NudgeBudget {
    evals_left: usize,
    deadline: Instant,
}

impl NudgeBudget {
    /// The production budget: [`NUDGE_EVAL_BUDGET`] evaluations inside
    /// [`NUDGE_TIME_LIMIT_SECS`].
    pub(crate) fn standard() -> Self {
        Self::new(
            NUDGE_EVAL_BUDGET,
            Duration::from_secs(NUDGE_TIME_LIMIT_SECS),
        )
    }

    /// A budget of `evals` referee evaluations, expiring `limit` from now.
    pub(crate) fn new(evals: usize, limit: Duration) -> Self {
        Self {
            evals_left: evals,
            deadline: Instant::now() + limit,
        }
    }

    /// Claim one referee evaluation, or report that the budget is spent.
    ///
    /// Both limits are checked on every claim: an evaluation is only worth
    /// starting if there is both an allowance and time left for it.
    fn claim(&mut self) -> bool {
        if self.evals_left == 0 || Instant::now() >= self.deadline {
            return false;
        }
        self.evals_left -= 1;
        true
    }
}

/// The nudge's candidate prefixes, in the order it should try them.
///
/// [`prioritized_itemstats`] supplies the pool: canonical (one id per display
/// name, unpriceable rows dropped) and radar-primary first. On top of that the
/// weight-aware tier prefixes float ahead of the rest, so if the budget does
/// run out it runs out on prefixes that never served the user's radar weights.
///
/// What this replaced was `select_prefixes_by_tiers` **plus** the whole
/// `db.itemstats` map, concatenated. The tier names were a subset of the map,
/// so every tier prefix was evaluated twice; and the tier half resolved each
/// name with `db.itemstats.values().find(...)`, so which of Berserker's five
/// ids the nudge tried was decided by `HashMap` iteration order and was not
/// stable between runs on one machine, let alone between machines.
///
/// Tier names are matched through [`normalized_prefix_name`], not string
/// equality: the tier tables spell four prefixes without their possessive
/// ("Marauder", "Valkyrie"), and an exact match silently dropped those from
/// the weight-aware half of the pool.
fn nudge_pool<'a>(
    db: &'a GameDb,
    weights: &OptimizationWeights,
) -> Vec<&'a gw2_api::models::ItemStat> {
    let tier_names: Vec<String> = scoring::select_prefixes_by_tiers(weights)
        .into_iter()
        .map(normalized_prefix_name)
        .collect();
    let mut pool = prioritized_itemstats(db, weights);
    // Stable sort on "is not a tier prefix": tier prefixes keep their priority
    // order and move to the front, everything else keeps its order behind them.
    pool.sort_by_cached_key(|itemstat| {
        !tier_names.contains(&normalized_prefix_name(&itemstat.name))
    });
    pool
}

/// Post-beam nudge pass — the "replace 1–4 pieces" fine-tuner.
///
/// The beam runs ~2 generations on default config, so it reliably finds the
/// best uniform prefix but rarely composes multi-piece stat nudges. This
/// hill-climbs single-piece swaps (every stat-bearing slot × the canonical
/// prefix pool) against the full referee rank until no swap improves. Up to
/// `MAX_ROUNDS` improving swaps compose into "replaced 1–4 pieces" results.
/// Saturating axis scores (`min(1.0)`) are what make mixes win: when a heavily
/// weighted axis is already capped, trading its surplus for an unsaturated
/// axis raises the weighted score.
///
/// Spends at most a [`NudgeBudget::standard`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn refine_piece_swaps(
    best: ValidatedBuild,
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &ScenarioSpec,
    locks: &BuildLocks,
    on_progress: &mut dyn FnMut(OptimizeProgress),
    is_cancelled: &dyn Fn() -> bool,
) -> ValidatedBuild {
    refine_piece_swaps_within(
        best,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
        locks,
        on_progress,
        is_cancelled,
        NudgeBudget::standard(),
    )
}

/// [`refine_piece_swaps`] with the budget supplied by the caller.
///
/// Separate from the production entry point purely so the budget is
/// observable: a bound nothing can exercise is a comment, not a bound.
#[allow(clippy::too_many_arguments)]
pub(crate) fn refine_piece_swaps_within(
    best: ValidatedBuild,
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &ScenarioSpec,
    locks: &BuildLocks,
    on_progress: &mut dyn FnMut(OptimizeProgress),
    is_cancelled: &dyn Fn() -> bool,
    mut budget: NudgeBudget,
) -> ValidatedBuild {
    const MAX_ROUNDS: usize = 4;

    let itemstats = nudge_pool(db, weights);
    let mut current = best;
    if !budget.claim() {
        return current;
    }
    let current_report =
        referee::evaluate_validated_build(&current, db, profession_name, weights, ctx, scenario);
    let mut current_rank = referee::search_rank(&current_report);

    for round in 1..=MAX_ROUNDS {
        if is_cancelled() {
            break;
        }
        on_progress(OptimizeProgress {
            stage: format!("Fine-tuning piece swaps (pass {round}/{MAX_ROUNDS})..."),
            done: false,
        });

        // Recomputed each round: an improving move changes what the slots hold,
        // and a lock or an empty hand is a property of the build, not the pass.
        let movable = movable_slot_prefixes(&current, &locks.gear_locks);
        let mut best_move: Option<(ValidatedBuild, [i64; 9])> = None;
        let mut spent = false;
        'slots: for (slot, current_prefix) in &movable {
            for itemstat in itemstats.iter() {
                if itemstat.id == current_prefix.itemstat_id {
                    continue; // no-op
                }
                if is_cancelled() {
                    return current;
                }
                if !budget.claim() {
                    spent = true;
                    break 'slots;
                }
                let mut build = current.clone();
                build.gear_slots.set(
                    *slot,
                    PrefixRef {
                        itemstat_id: itemstat.id,
                        name: itemstat.name.clone(),
                    },
                );
                let report = referee::evaluate_validated_build(
                    &build,
                    db,
                    profession_name,
                    weights,
                    ctx,
                    scenario,
                );
                let rank = referee::search_rank(&report);
                if rank > current_rank
                    && best_move
                        .as_ref()
                        .is_none_or(|(_, best_rank)| rank > *best_rank)
                {
                    best_move = Some((build, rank));
                }
            }
        }

        match best_move {
            // The winning move's rank was measured when the move was found;
            // re-evaluating the same build against the same inputs returns the
            // same report, so the old re-evaluation here bought nothing and
            // cost one full combat + rotation simulation per round.
            Some((build, rank)) => {
                current = build;
                current_rank = rank;
            }
            None => break, // converged: no single piece swap improves
        }

        if spent {
            // Budget gone. The improving move above was fully paid for and is
            // kept; there is nothing left to look for another one with.
            break;
        }
    }

    current
}

// ─── Mutation operators ───────────────────────────────────────────────────────

/// Generate all immediate neighbours of `candidate` by applying each of the
/// six atomic mutation operators in turn and collecting the results.
///
/// Each operator clones the current `ValidatedBuild`, changes exactly one
/// aspect, and appends the mutated build to the output.  Operators that find no
/// alternatives (e.g. because the DB is empty) simply contribute nothing to
/// the output — the function never panics on an empty `GameDb`.
///
/// Original operators plus the per-slot gear operator, elite-spec, and weapon
/// jumps. All three gear operators respect `BuildLocks.gear_locks`: locked
/// slots keep their locked prefix under every mutation.
///
/// Output is interleaved round-robin across operators rather than concatenated.
/// `optimize_v2_search` caps evaluation per beam member at ~80 neighbours, so
/// each operator here gets roughly a twelfth of that — six or seven. Two
/// consequences, and both matter:
///
/// * Concatenated order would burn the whole cap on `swap_gear_prefix` (one
///   neighbour per canonical prefix, 66 on live data, and it comes first), and
///   the search would never reach rune/sigil/relic/utility mutations at all.
/// * Each operator's *own* order decides what its six or seven evaluations
///   land on. That is why `swap_slot_prefix` emits prefix-major: slot-major
///   spent every one of them on the first two slots in `STAT_SLOTS`.
pub fn generate_neighbors(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
    locks: &BuildLocks,
    weights: &OptimizationWeights,
) -> Vec<ValidatedBuild> {
    let mut groups: Vec<Vec<ValidatedBuild>> = Vec::new();
    if !candidate.report.viability.is_viable {
        groups.push(swap_utilities_for_failed_gates(
            candidate,
            db,
            profession_name,
        ));
        groups.push(swap_relics_for_failed_gates(candidate, db));
    }
    let gear_locks = &locks.gear_locks;
    groups.push(swap_gear_prefix(candidate, db, weights, gear_locks));
    groups.push(swap_gear_groups(candidate, db, weights, gear_locks));
    groups.push(swap_slot_prefix(candidate, db, weights, gear_locks));
    groups.push(swap_rune(candidate, db));
    groups.push(swap_sigil_slots(candidate, db));
    groups.push(swap_relic(candidate, db));
    groups.push(swap_heal_skills(candidate, db, profession_name));
    groups.push(swap_utility_skills(candidate, db, profession_name));
    groups.push(swap_elite_skills(candidate, db, profession_name));
    groups.push(swap_major_traits(candidate, db, locks));
    groups.push(swap_elite_spec(candidate, db, profession_name, locks));
    groups.push(swap_weapons(candidate, db, profession_name));

    let total: usize = groups.iter().map(|g| g.len()).sum();
    let max_len = groups.iter().map(|g| g.len()).max().unwrap_or(0);
    let mut neighbors: Vec<ValidatedBuild> = Vec::with_capacity(total);
    let mut iters: Vec<_> = groups.into_iter().map(|g| g.into_iter()).collect();
    for _ in 0..max_len {
        for it in iters.iter_mut() {
            if let Some(n) = it.next() {
                neighbors.push(n);
            }
        }
    }
    neighbors
}

// ─── Beam search entry point ──────────────────────────────────────────────────

/// Run the beam/evolutionary search over complete build states.
///
/// Seeds from the synergy pipeline, then iteratively expands neighbors,
/// evaluates each with the gated referee, keeps the top `config.beam_width`
/// candidates, and returns the best `ValidatedBuild` found within the
/// time/evaluation budget.
///
/// Returns `Err` if seeding fails (e.g. unknown profession).
// Beam-search core; db, weights, context, scenario, and config are independent
// inputs — a params struct adds indirection without clarifying the search.
#[allow(clippy::too_many_arguments)]
pub fn optimize_v2_search(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &ScenarioSpec,
    locks: &gw2_core::types::BuildLocks,
    config: &SearchConfig,
    on_progress: &mut dyn FnMut(OptimizeProgress),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<ValidatedBuild, String> {
    if is_cancelled() {
        return Err("Cancelled".into());
    }

    // Step 1: select gear prefix (cosine sim).
    let gear_match = scoring::select_gear_prefix(weights);
    let prefix_name = gear_match.primary;

    // Step 2: seed from synergy pipeline.
    on_progress(OptimizeProgress {
        stage: "Seeding from synergy pipeline...".into(),
        done: false,
    });
    // Seeding is not instant — it walks spec/trait combos and evaluates each —
    // so it takes the same cancel token as the beam it feeds. The
    // non-cancellable entry point ignored a Cancel pressed during seeding and
    // only noticed it once the whole seed had finished.
    let mut seed_result = synergy_pipeline::optimize_synergy_cancellable(
        db,
        profession_name,
        weights,
        ctx,
        prefix_name,
        locks,
        Some(scenario),
        &mut |_| {},
        is_cancelled,
    )?;

    if is_cancelled() {
        return Err("Cancelled".into());
    }

    // Gear locks are requirements, not just prohibitions: operators refuse to
    // mutate a locked slot, but a lock targeting a slot the seed left empty
    // (improve flow, lock-only users) must still end up carrying its
    // required prefix. Pin each locked itemstat onto its slot once, before
    // the seed is evaluated. Unknown itemstat ids stay unpinned (the lock
    // still blocks mutation) rather than fabricating a name.
    if !locks.gear_locks.is_empty() {
        let mut validated = seed_result.validated;
        let mut pins: Vec<(&GearSlot, &u32)> = locks.gear_locks.iter().collect();
        // Canonical order → deterministic writes.
        pins.sort_by_key(|(slot, _)| {
            GearSlot::ALL
                .iter()
                .position(|canonical| *canonical == **slot)
        });
        for (slot, id) in pins {
            if validated.gear_slots.prefix_id(*slot) == Some(*id) {
                continue;
            }
            if let Some(itemstat) = db.itemstats.get(id) {
                validated.gear_slots.set(
                    *slot,
                    PrefixRef {
                        itemstat_id: *id,
                        name: itemstat.name.clone(),
                    },
                );
            }
        }
        seed_result.validated = validated;
    }

    // Step 3: evaluate seed.
    let seed_report = referee::evaluate_validated_build(
        &seed_result.validated,
        db,
        profession_name,
        weights,
        ctx,
        scenario,
    );

    // Step 4: initialise beam.
    let mut beam: Vec<BeamCandidate> = vec![BeamCandidate {
        validated: seed_result.validated,
        report: seed_report,
    }];

    let start = Instant::now();
    let deadline = start + Duration::from_secs(config.time_limit_secs);
    let mut eval_count = 0usize;
    let mut generation = 0u32;

    // Step 5: beam loop — keep permuting until the clock or eval budget is gone.
    while eval_count < config.eval_budget && Instant::now() < deadline && !is_cancelled() {
        generation += 1;
        on_progress(OptimizeProgress {
            stage: format!(
                "Permuting kits (gen {generation}, {eval_count} evals, {:.0}s)...",
                start.elapsed().as_secs_f32()
            ),
            done: false,
        });
        let mut next: Vec<BeamCandidate> = Vec::new();

        // Elitism: keep current beam members in the candidate pool.
        next.extend(beam.iter().cloned());

        // Budget per candidate to avoid spending all evals on a single member.
        let budget_per = config.eval_budget.saturating_sub(eval_count) / beam.len().max(1);
        let neighbor_cap = budget_per.clamp(1, 80);

        for candidate in &beam {
            if Instant::now() >= deadline || is_cancelled() {
                break;
            }
            let neighbors = generate_neighbors(candidate, db, profession_name, locks, weights);
            for neighbor in neighbors.into_iter().take(neighbor_cap) {
                if eval_count >= config.eval_budget || Instant::now() >= deadline || is_cancelled()
                {
                    break;
                }
                let report = referee::evaluate_validated_build(
                    &neighbor,
                    db,
                    profession_name,
                    weights,
                    ctx,
                    scenario,
                );
                eval_count += 1;
                next.push(BeamCandidate {
                    validated: neighbor,
                    report,
                });
            }
        }

        // Viable kit first; roam ranks the fight sim, not paper combat indices.
        next.sort_by_key(|candidate| std::cmp::Reverse(referee::search_rank(&candidate.report)));

        // A build identity includes every combat-relevant choice. Omitting
        // weapons, sigils, or traits collapsed genuinely different WvW chains.
        next.dedup_by(|a, b| {
            a.validated.gear_identity() == b.validated.gear_identity()
                && a.validated.rune == b.validated.rune
                && a.validated.sigils == b.validated.sigils
                && a.validated.relic == b.validated.relic
                && a.validated.specializations == b.validated.specializations
                && a.validated.weapons == b.validated.weapons
                && a.validated.skills.heal == b.validated.skills.heal
                && a.validated.skills.elite == b.validated.skills.elite
                && a.validated.skills.utilities == b.validated.skills.utilities
                && a.validated.legends == b.validated.legends
                && a.validated.pets == b.validated.pets
        });

        next.truncate(config.beam_width);

        if next.is_empty() {
            break;
        }
        beam = next;
    }

    if is_cancelled() {
        return Err("Cancelled".into());
    }

    finish_search(beam)
}

/// Take the already-ranked beam head. Empty is a real failure; a lock-respecting
/// non-viable head is a provisional result, not an error.
fn finish_search(beam: Vec<BeamCandidate>) -> Result<ValidatedBuild, String> {
    let mut best = beam
        .into_iter()
        .next()
        .ok_or_else(|| "No candidates survived beam search".to_string())?;
    if !best.report.viability.is_viable {
        best.validated.warnings.push(format!(
            "Best lock-respecting result is provisional: {}",
            referee::viability_failure_summary(&best.report.viability)
        ));
    }
    Ok(best.validated)
}

// ─── Individual mutation operators (private helpers) ─────────────────────────

/// May a gear operator move this slot?
///
/// Three rules, each a domain fact rather than a search-tuning knob:
///
/// * A gear lock pins the slot. The user asked for that prefix; no operator
///   overrides it.
/// * Only the fourteen stat-bearing slots count ([`crate::search::STAT_SLOTS`]).
///   Weapon set 2 is *carried*, not worn: it draws no slot budget and its
///   sigils are inactive, so a set-2 prefix swap is a guaranteed zero-stat
///   neighbour — and because it still changes `gear_identity`, the beam's
///   dedup cannot collapse it either. It costs a full referee evaluation to
///   learn nothing.
/// * The build must actually wear the slot. A two-hander has no off-hand, and
///   a half-built draft has whatever it has.
fn mutable_gear_slot(
    build: &ValidatedBuild,
    slot: GearSlot,
    gear_locks: &HashMap<GearSlot, u32>,
) -> bool {
    !gear_locks.contains_key(&slot)
        && crate::search::STAT_SLOTS.contains(&slot)
        && build.wears(slot)
}

/// Operator 1 — swap the whole build to one gear prefix.
///
/// For every prefix in the canonical pool, produce a clone of the current
/// build with every unlocked worn slot set to that prefix. "Worn" is
/// [`ValidatedBuild::fill_unlocked_gear_slots`]'s definition, which is wider
/// than [`mutable_gear_slot`]'s: a weapon set 2 that holds weapons counts as
/// worn there, so a whole-build fill does restamp those two cells. That costs
/// no extra evaluation — this operator emits one neighbour per prefix either
/// way — but it does leave a set-2 prefix in the saved slot map that nothing
/// scored.
///
/// The mutation is its own no-op test: `fill_unlocked_gear_slots` reports
/// whether it changed anything, so the neighbour is kept only when it does.
/// The hand-written predicate this replaced tried to mirror that rule and got
/// it wrong in one direction — it counted an unlocked *empty* cell as "would
/// change", and a two-hander's off-hand is permanently empty, so the prefix
/// the build already wore on every slot still produced one identical
/// neighbour per call. Deriving the answer from the mutation cannot drift
/// from it.
///
/// Iterates the pool in its priority order so beam-search neighbour ordering —
/// and therefore the tie-break behaviour in the downstream
/// `sort_by + dedup_by + truncate` pipeline — is stable across runs.
fn swap_gear_prefix(
    candidate: &BeamCandidate,
    db: &GameDb,
    weights: &OptimizationWeights,
    gear_locks: &HashMap<GearSlot, u32>,
) -> Vec<ValidatedBuild> {
    prioritized_itemstats(db, weights)
        .into_iter()
        .filter_map(|itemstat| {
            let mut build = candidate.validated.clone();
            let changed = build.fill_unlocked_gear_slots(
                PrefixRef {
                    itemstat_id: itemstat.id,
                    name: itemstat.name.clone(),
                },
                gear_locks,
            );
            changed.then_some(build)
        })
        .collect()
}

/// Operator 2 — mutate armour, trinkets, and weapons independently.
///
/// Three groups capture the common WvW Marauder/Berserker/Demolisher mixes
/// without exploding the search into sixteen independent slot dimensions. The
/// groups already name weapon *set 1* only, so set 2 is out of reach here; the
/// per-slot `wears` test is what keeps a two-hander's empty off-hand empty
/// instead of stamping a prefix into a hand that holds nothing.
fn swap_gear_groups(
    candidate: &BeamCandidate,
    db: &GameDb,
    weights: &OptimizationWeights,
    gear_locks: &HashMap<GearSlot, u32>,
) -> Vec<ValidatedBuild> {
    let itemstats = prioritized_itemstats(db, weights);
    let mut out = Vec::with_capacity(itemstats.len() * 3);
    // Slot groups standing in for the old armor / trinkets / weapons categories.
    const GROUPS: [&[GearSlot]; 3] = [&ARMOR_SLOTS, &TRINKET_SLOTS, &WEAPON_SET1_SLOTS];
    for itemstat in itemstats {
        let prefix = PrefixRef {
            itemstat_id: itemstat.id,
            name: itemstat.name.clone(),
        };
        for slots in GROUPS {
            let mut build = candidate.validated.clone();
            let mut changed = false;
            for &slot in slots {
                // Locked pieces keep their locked prefix and unworn hands stay
                // empty; a group fill never touches either.
                if !mutable_gear_slot(&candidate.validated, slot, gear_locks) {
                    continue;
                }
                if candidate.validated.gear_slots.prefix_id(slot) != Some(prefix.itemstat_id) {
                    build.gear_slots.set(slot, prefix.clone());
                    changed = true;
                }
            }
            if changed {
                out.push(build);
            }
        }
    }
    out
}

/// How many of the canonical pool's leading prefixes the per-slot operator
/// offers each slot. Four is enough to reach a genuine hybrid (radar primary,
/// radar secondary, and two neighbours in id order) without multiplying the
/// slot count by the whole pool.
const SLOT_PREFIX_CANDIDATES: usize = 4;

/// Operator 3 — per-slot prefix swap.
///
/// For each movable slot and each of the leading `SLOT_PREFIX_CANDIDATES`
/// canonical prefixes (skipping the no-op same-prefix swap), emit a clone with
/// exactly that one slot changed. This is the operator that makes true hybrid
/// mixes — Berserker's helm, Cavalier's coat — reachable at full granularity.
///
/// **Output is prefix-major, so consecutive neighbours land on different
/// slots.** That ordering is the whole point. `generate_neighbors` interleaves
/// operators round-robin and `optimize_v2_search` then takes at most
/// ~80 neighbours per beam member, which leaves this operator roughly its
/// twelfth: about six or seven evaluations. Emitting slot-major — every
/// prefix for the helm, then every prefix for the shoulders — spent all of
/// them on the first two slots in `GearSlot::ALL`, so rings, amulet, and
/// weapons never received a single per-slot evaluation in the beam. Walking
/// prefixes on the outside gives every slot its first candidate before any
/// slot gets its second.
///
/// Ordering is `(prefix_priority, itemstat_id, slot_index)`: the pool arrives
/// already sorted by priority and `STAT_SLOTS` is a fixed canonical order, so
/// the emitted sequence is identical on every run — the same determinism
/// discipline as every other neighbour source.
fn swap_slot_prefix(
    candidate: &BeamCandidate,
    db: &GameDb,
    weights: &OptimizationWeights,
    gear_locks: &HashMap<GearSlot, u32>,
) -> Vec<ValidatedBuild> {
    let itemstats = prioritized_itemstats(db, weights);
    let top_prefixes: Vec<&gw2_api::models::ItemStat> = itemstats
        .iter()
        .copied()
        .take(SLOT_PREFIX_CANDIDATES)
        .collect();

    let movable: Vec<(GearSlot, PrefixRef)> =
        movable_slot_prefixes(&candidate.validated, gear_locks);

    let mut out = Vec::with_capacity(movable.len() * top_prefixes.len());
    for itemstat in &top_prefixes {
        for (slot, current) in &movable {
            if current.itemstat_id == itemstat.id {
                continue; // no-op same-prefix swap
            }
            let mut build = candidate.validated.clone();
            build.gear_slots.set(
                *slot,
                PrefixRef {
                    itemstat_id: itemstat.id,
                    name: itemstat.name.clone(),
                },
            );
            out.push(build);
        }
    }
    out
}

/// Every slot a per-slot operator may move, paired with the prefix it carries
/// today, in `STAT_SLOTS` order.
///
/// A slot with no prefix is not listed: an empty cell stays empty (that is the
/// occupancy rule the whole stat fabric rests on), and there is nothing to
/// swap *from*.
fn movable_slot_prefixes(
    build: &ValidatedBuild,
    gear_locks: &HashMap<GearSlot, u32>,
) -> Vec<(GearSlot, PrefixRef)> {
    crate::search::STAT_SLOTS
        .iter()
        .copied()
        .filter(|slot| mutable_gear_slot(build, *slot, gear_locks))
        .filter_map(|slot| {
            build
                .gear_slots
                .get(slot)
                .cloned()
                .map(|prefix| (slot, prefix))
        })
        .collect()
}

/// The canonical prefix pool in search order: the user's radar primary first,
/// its secondary next, then the rest by ascending id.
///
/// The rows come from [`crate::itemstat_pool::canonical_itemstats`], which is
/// the only pool a prefix enumerator may draw from. `db.itemstats` is a table
/// of stat *templates*, not of prefixes: on live data its 191 rows resolve to
/// 66 named prefixes, 43 names carry two to nine ids, and the 1041-1052 band
/// carries no positive multiplier at all. Enumerating it raw is how the "top
/// four prefixes" for a power build became four Berserker's ids and how a
/// search could settle on an id that the name-keyed appliers never resolve
/// back to.
///
/// The ordering is the second half of the job. The beam evaluates at most ~80
/// neighbours per member, so id-only ordering put the condition/sustain mixes
/// the user actually asked for behind sixty prefixes they did not.
fn prioritized_itemstats<'a>(
    db: &'a GameDb,
    weights: &OptimizationWeights,
) -> Vec<&'a gw2_api::models::ItemStat> {
    let preferred = scoring::select_gear_prefix(weights);
    let primary = normalized_prefix_name(preferred.primary);
    let secondary = preferred.secondary.map(normalized_prefix_name);
    let mut itemstats = crate::itemstat_pool::canonical_itemstats(db);
    itemstats.sort_by_key(|itemstat| {
        let name = normalized_prefix_name(&itemstat.name);
        let preference = if name == primary {
            0
        } else if secondary
            .as_ref()
            .is_some_and(|candidate| *candidate == name)
        {
            1
        } else {
            2
        };
        (preference, itemstat.id)
    });
    itemstats
}

fn normalized_prefix_name(name: &str) -> String {
    let mut normalized: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if normalized.ends_with('s') {
        normalized.pop();
    }
    normalized
}

/// Operator 2 — swap rune.
///
/// For every Superior rune item in the DB, produce a clone with `rune` set to
/// that item.
fn swap_rune(candidate: &BeamCandidate, db: &GameDb) -> Vec<ValidatedBuild> {
    db.all_runes()
        .into_iter()
        .filter(|r| r.name.contains("Superior"))
        .map(|r| {
            let mut b = candidate.validated.clone();
            b.rune = Some(ValidatedItem {
                id: r.id,
                name: r.name.clone(),
            });
            b
        })
        .collect()
}

/// Operator 3 — swap sigil slots.
///
/// For each sigil slot (up to 2), try every Superior sigil from the DB.
/// Skip if the proposed sigil is already present in another slot (no
/// duplicate sigils within a single build).
fn swap_sigil_slots(candidate: &BeamCandidate, db: &GameDb) -> Vec<ValidatedBuild> {
    let superior_sigils: Vec<_> = db
        .all_sigils()
        .into_iter()
        .filter(|s| s.name.contains("Superior"))
        .collect();

    // Treat sigils as 4 fixed slots: [set1_main, set1_off, set2_main, set2_off].
    // Enforce "no duplicate sigil family within a weapon set", but allow the
    // same family in both sets independently.
    //
    // Only mutate slots the seed already filled. Previously this function
    // padded missing slots with `ValidatedItem { id: 0, name: "" }`, which is
    // not a valid item — those placeholders rendered as empty slots in the UI
    // and were skipped by stat calculation, producing builds that scored worse
    // than the seed for an unrelated reason. The synergy pipeline always seeds
    // with 4 sigils, so this is normally a no-op guard.
    let slot_count = candidate.validated.sigils.len().min(4);
    let mut neighbors: Vec<ValidatedBuild> = Vec::new();

    for slot_idx in 0..slot_count {
        // Determine the 2-slot weapon set this slot belongs to.
        let set_start = (slot_idx / 2) * 2;
        let set_end = set_start + 2;

        // IDs and families currently in the *other* slot(s) of this set.
        let mut other_ids: Vec<u32> = Vec::new();
        let mut other_families: Vec<String> = Vec::new();
        for i in set_start..set_end {
            if i == slot_idx {
                continue;
            }
            if let Some(s) = candidate.validated.sigils.get(i) {
                other_ids.push(s.id);
                other_families.push(normalize_sigil_family(&s.name));
            }
        }

        for sigil in &superior_sigils {
            let family = normalize_sigil_family(&sigil.name);
            if other_ids.contains(&sigil.id) {
                continue; // duplicate by item id in this set
            }
            if other_families.iter().any(|f| f == &family) {
                continue; // duplicate by family name in this set
            }

            let mut b = candidate.validated.clone();
            b.sigils[slot_idx] = ValidatedItem {
                id: sigil.id,
                name: sigil.name.clone(),
            };
            neighbors.push(b);
        }
    }

    neighbors
}

/// Operator 4 — swap relic.
///
/// For every relic item in the DB, produce a clone with `relic` set to that
/// item.
fn swap_relic(candidate: &BeamCandidate, db: &GameDb) -> Vec<ValidatedBuild> {
    db.all_relics()
        .into_iter()
        .map(|r| {
            let mut b = candidate.validated.clone();
            b.relic = Some(ValidatedItem {
                id: r.id,
                name: r.name.clone(),
            });
            b
        })
        .collect()
}

fn eligible_slot_skills(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
    slot: &str,
) -> Vec<(u32, String)> {
    // Revenant bar skills are a legend package, not independent slot choices.
    if profession_name == "Revenant" {
        return Vec::new();
    }
    let equipped_specs: Vec<u32> = candidate
        .validated
        .specializations
        .iter()
        .map(|spec| spec.spec_id)
        .collect();
    let mut choices: Vec<_> = db
        .skills_by_profession
        .get(profession_name)
        .into_iter()
        .flatten()
        .filter_map(|id| db.skills.get(id))
        .filter(|skill| {
            skill.slot.as_deref() == Some(slot)
                && db.skill_palette_id(skill.id) != 0
                && skill
                    .specialization
                    .is_none_or(|required| equipped_specs.contains(&required))
        })
        .map(|skill| (skill.id, skill.name.clone()))
        .collect();
    choices.sort_by_key(|(id, _)| *id);
    choices
}

/// Heal skills define recovery timing and are a first-class search dimension.
fn swap_heal_skills(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
) -> Vec<ValidatedBuild> {
    eligible_slot_skills(candidate, db, profession_name, "Heal")
        .into_iter()
        .filter(|choice| candidate.validated.skills.heal.as_ref() != Some(choice))
        .map(|choice| {
            let mut build = candidate.validated.clone();
            build.skills.heal = Some(choice);
            build
        })
        .collect()
}

/// Elite skills can be the protected spike, defensive reset, or group-control
/// endpoint; keeping them fixed made those complete chains unreachable.
fn swap_elite_skills(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
) -> Vec<ValidatedBuild> {
    eligible_slot_skills(candidate, db, profession_name, "Elite")
        .into_iter()
        .filter(|choice| candidate.validated.skills.elite.as_ref() != Some(choice))
        .map(|choice| {
            let mut build = candidate.validated.clone();
            build.skills.elite = Some(choice);
            build
        })
        .collect()
}

/// Mutate one unlocked major-trait column at a time while preserving the
/// minor-trait spine and every explicit user lock.
fn swap_major_traits(
    candidate: &BeamCandidate,
    db: &GameDb,
    locks: &BuildLocks,
) -> Vec<ValidatedBuild> {
    let mut out = Vec::new();
    for (spec_idx, validated_spec) in candidate.validated.specializations.iter().enumerate() {
        let Some(spec) = db.specializations.get(&validated_spec.spec_id) else {
            continue;
        };
        for column in 0..3usize {
            if locks.locked_trait(spec.id, column).is_some() {
                continue;
            }
            for trait_id in spec.major_traits.iter().skip(column * 3).take(3).copied() {
                if validated_spec.trait_ids.get(column) == Some(&trait_id) {
                    continue;
                }
                let Some(trait_data) = db.traits.get(&trait_id) else {
                    continue;
                };
                let mut build = candidate.validated.clone();
                let target = &mut build.specializations[spec_idx];
                while target.trait_ids.len() < 3 {
                    target.trait_ids.push(0);
                    target.trait_names.push(String::new());
                }
                target.trait_ids[column] = trait_id;
                target.trait_names[column] = trait_data.name.clone();
                target.all_trait_ids = spec.minor_traits.clone();
                target
                    .all_trait_ids
                    .extend(target.trait_ids.iter().copied().filter(|id| *id != 0));
                out.push(build);
            }
        }
    }
    out
}

/// Operator 5 — swap utility skills.
///
/// For each of the 3 utility slots, iterate all skills available to
/// `profession_name` and propose swapping that slot.  A skill is eligible if:
///
/// - Its `slot` field is `Some("Utility")`, **or** the slot is `None` but
///   the skill appears in the profession's skill list (palette entry).
/// - If the skill has a required `specialization`, that spec must be in the
///   current build's equipped spec IDs.
///
/// Skills that are already in another utility slot are kept as-is (no
/// de-duplication — GW2 does not forbid duplicate utility skills, and keeping
/// them avoids ruling out valid states).
fn swap_utility_skills(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
) -> Vec<ValidatedBuild> {
    // Revenant utilities are bound to the active legend; mixing slots
    // from two stances produces an illegal template.
    if profession_name == "Revenant" {
        return Vec::new();
    }
    let prof_skill_ids: Vec<u32> = db
        .skills_by_profession
        .get(profession_name)
        .cloned()
        .unwrap_or_default();

    if prof_skill_ids.is_empty() {
        return Vec::new();
    }

    // Collect the spec IDs currently equipped in the build.
    let equipped_spec_ids: Vec<u32> = candidate
        .validated
        .specializations
        .iter()
        .map(|s| s.spec_id)
        .collect();

    // Collect eligible utility skills for this profession.
    let utility_skills: Vec<u32> = prof_skill_ids
        .iter()
        .copied()
        .filter(|&id| {
            if let Some(skill) = db.skills.get(&id) {
                // Check slot eligibility.
                let slot_ok = skill.slot.as_deref() == Some("Utility");
                if !slot_ok {
                    return false;
                }
                if db.skill_palette_id(id) == 0 {
                    return false;
                }
                // Check specialization gating.
                if let Some(req_spec) = skill.specialization {
                    return equipped_spec_ids.contains(&req_spec);
                }
                true
            } else {
                false
            }
        })
        .collect();

    let mut neighbors: Vec<ValidatedBuild> = Vec::new();

    for slot_idx in 0..3usize {
        for &skill_id in &utility_skills {
            let skill = match db.skills.get(&skill_id) {
                Some(s) => s,
                None => continue,
            };
            let mut b = candidate.validated.clone();
            // Ensure utilities vec has enough entries.
            while b.skills.utilities.len() <= slot_idx {
                b.skills.utilities.push(None);
            }
            b.skills.utilities[slot_idx] = Some((skill_id, skill.name.clone()));
            neighbors.push(b);
        }
    }

    neighbors
}

fn gate_failed(report: &crate::referee::ViabilityReport, gate: ViabilityGate) -> bool {
    report.gates.iter().any(|g| g.gate == gate && !g.passed)
}

/// When Stability is missing on the bar, try relics that grant it (Cavalier, etc.).
fn swap_relics_for_failed_gates(candidate: &BeamCandidate, db: &GameDb) -> Vec<ValidatedBuild> {
    if !gate_failed(&candidate.report.viability, ViabilityGate::StabilityAccess) {
        return Vec::new();
    }
    db.all_relics()
        .into_iter()
        .filter(|r| {
            let bonuses = r
                .details
                .as_ref()
                .map(|d| d.bonuses.as_slice())
                .unwrap_or(&[]);
            let desc = r.description.as_deref().or_else(|| {
                r.details
                    .as_ref()
                    .and_then(|d| d.infix_upgrade.as_ref())
                    .and_then(|u| u.buff.as_ref())
                    .and_then(|b| b.description.as_deref())
            });
            crate::text_util::gear_text_grants_stability(&r.name, desc, bonuses)
        })
        .map(|r| {
            let mut b = candidate.validated.clone();
            b.relic = Some(ValidatedItem {
                id: r.id,
                name: r.name.clone(),
            });
            b
        })
        .collect()
}

/// When the current kit fails stunbreak/stability/cleanse, try those utilities first
/// instead of burning the eval budget on gear-prefix neighbors.
fn swap_utilities_for_failed_gates(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
) -> Vec<ValidatedBuild> {
    if profession_name == "Revenant" {
        return Vec::new();
    }
    let need_stability = gate_failed(&candidate.report.viability, ViabilityGate::StabilityAccess);
    let need_stunbreak = gate_failed(&candidate.report.viability, ViabilityGate::StunbreakCount);
    let need_cleanse = gate_failed(&candidate.report.viability, ViabilityGate::CleanseRate);
    if !(need_stability || need_stunbreak || need_cleanse) {
        return Vec::new();
    }

    let equipped_spec_ids: Vec<u32> = candidate
        .validated
        .specializations
        .iter()
        .map(|s| s.spec_id)
        .collect();
    let prof_skill_ids: Vec<u32> = db
        .skills_by_profession
        .get(profession_name)
        .cloned()
        .unwrap_or_default();
    if prof_skill_ids.is_empty() {
        return Vec::new();
    }

    let utility_skills: Vec<u32> = prof_skill_ids
        .iter()
        .copied()
        .filter(|&id| {
            let Some(skill) = db.skills.get(&id) else {
                return false;
            };
            if skill.slot.as_deref() != Some("Utility") || db.skill_palette_id(id) == 0 {
                return false;
            }
            if let Some(req_spec) = skill.specialization {
                if !equipped_spec_ids.contains(&req_spec) {
                    return false;
                }
            }
            (need_stability && synergy_pipeline::skill_has_cc_answer(skill))
                || (need_stunbreak && synergy_pipeline::skill_is_stunbreak(skill))
                || (need_cleanse && synergy_pipeline::skill_cleanse_count(skill) > 0)
        })
        .collect();

    let mut neighbors: Vec<ValidatedBuild> = Vec::new();
    for slot_idx in 0..3usize {
        for &skill_id in &utility_skills {
            let skill = match db.skills.get(&skill_id) {
                Some(s) => s,
                None => continue,
            };
            let mut b = candidate.validated.clone();
            while b.skills.utilities.len() <= slot_idx {
                b.skills.utilities.push(None);
            }
            b.skills.utilities[slot_idx] = Some((skill_id, skill.name.clone()));
            neighbors.push(b);
        }
    }
    if need_stability {
        for &skill_id in &prof_skill_ids {
            let Some(skill) = db.skills.get(&skill_id) else {
                continue;
            };
            if skill.slot.as_deref() != Some("Heal") || db.skill_palette_id(skill_id) == 0 {
                continue;
            }
            if let Some(req_spec) = skill.specialization {
                if !equipped_spec_ids.contains(&req_spec) {
                    continue;
                }
            }
            if !synergy_pipeline::skill_has_cc_answer(skill) {
                continue;
            }
            let mut b = candidate.validated.clone();
            b.skills.heal = Some((skill_id, skill.name.clone()));
            neighbors.push(b);
        }
    }
    neighbors
}

fn swap_elite_spec(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
    locks: &BuildLocks,
) -> Vec<ValidatedBuild> {
    if locks.specs[2].is_some() {
        return Vec::new();
    }
    let Some(profession) = db.profession(profession_name) else {
        return Vec::new();
    };
    let current_elite_id = candidate
        .validated
        .specializations
        .iter()
        .find(|s| s.elite)
        .map(|s| s.spec_id);

    let mut elites: Vec<&Specialization> = profession
        .specializations
        .iter()
        .filter_map(|id| db.specializations.get(id))
        .filter(|s| s.elite)
        .collect();
    elites.sort_by_key(|s| s.id);

    let mut out = Vec::new();
    for spec in elites {
        if Some(spec.id) == current_elite_id {
            continue;
        }
        let mut b = candidate.validated.clone();
        let vs = validated_spec_from(spec, db, locks);
        if let Some(idx) = b.specializations.iter().position(|s| s.elite) {
            b.specializations[idx] = vs;
        } else if b.specializations.len() >= 3 {
            b.specializations[2] = vs;
        } else {
            b.specializations.push(vs);
        }
        retarget_after_elite_swap(&mut b, db, profession);
        out.push(b);
    }
    out
}

fn validated_spec_from(spec: &Specialization, db: &GameDb, locks: &BuildLocks) -> ValidatedSpec {
    let mut trait_ids = Vec::new();
    let mut trait_names = Vec::new();
    for col in 0..3 {
        let id = locks
            .locked_trait(spec.id, col)
            .or_else(|| spec.major_traits.get(col * 3).copied());
        if let Some(id) = id {
            trait_ids.push(id);
            trait_names.push(
                db.traits
                    .get(&id)
                    .map(|t| t.name.clone())
                    .unwrap_or_default(),
            );
        }
    }
    let mut all_trait_ids = spec.minor_traits.clone();
    all_trait_ids.extend(trait_ids.iter().copied());
    ValidatedSpec {
        spec_id: spec.id,
        name: spec.name.clone(),
        elite: true,
        trait_ids,
        trait_names,
        all_trait_ids,
    }
}

fn retarget_after_elite_swap(build: &mut ValidatedBuild, db: &GameDb, profession: &Profession) {
    let equipped: Vec<u32> = build.specializations.iter().map(|s| s.spec_id).collect();
    let elite_ids: Vec<u32> = build
        .specializations
        .iter()
        .filter(|s| s.elite)
        .map(|s| s.spec_id)
        .collect();

    if let Some((id, _)) = &build.skills.heal {
        if skill_gated_out(*id, db, &equipped) {
            build.skills.heal = None;
        }
    }
    for slot in &mut build.skills.utilities {
        if let Some((id, _)) = slot {
            if skill_gated_out(*id, db, &equipped) {
                *slot = None;
            }
        }
    }
    if let Some((id, _)) = &build.skills.elite {
        if skill_gated_out(*id, db, &equipped) {
            build.skills.elite = None;
        }
    }
    build.skills.profession =
        crate::rotation::builder::profession_skills_for_build(db, &profession.name, &equipped);

    let combos = land_weapon_combos(profession, &elite_ids);
    if !weapon_set_ok(&build.weapons.set1, profession, &elite_ids) {
        build.weapons.set1 = combos
            .first()
            .map(|(mh, oh)| ValidatedWeaponSet {
                main_hand: mh.clone(),
                off_hand: oh.clone(),
            })
            .unwrap_or_default();
    }
    if !weapon_set_ok(&build.weapons.set2, profession, &elite_ids) {
        build.weapons.set2 = combos
            .get(1)
            .or(combos.first())
            .map(|(mh, oh)| ValidatedWeaponSet {
                main_hand: mh.clone(),
                off_hand: oh.clone(),
            })
            .unwrap_or_default();
    }
}

fn skill_gated_out(id: u32, db: &GameDb, equipped: &[u32]) -> bool {
    match db.skills.get(&id).and_then(|s| s.specialization) {
        Some(req) => !equipped.contains(&req),
        None => false,
    }
}

fn weapon_set_ok(set: &ValidatedWeaponSet, profession: &Profession, elite_ids: &[u32]) -> bool {
    [&set.main_hand, &set.off_hand]
        .into_iter()
        .flatten()
        .all(|name| weapon_ok(name, profession, elite_ids))
}

fn weapon_ok(name: &str, profession: &Profession, elite_ids: &[u32]) -> bool {
    let Some(info) = profession.weapons.get(name) else {
        return false;
    };
    if !info.land_usable(name) {
        return false;
    }
    match info.specialization {
        Some(req) => elite_ids.contains(&req),
        None => true,
    }
}

fn land_weapon_combos(
    profession: &Profession,
    elite_ids: &[u32],
) -> Vec<(Option<String>, Option<String>)> {
    let mut two_hand = Vec::new();
    let mut main = Vec::new();
    let mut off = Vec::new();
    for (name, info) in &profession.weapons {
        if !info.land_usable(name) {
            continue;
        }
        if let Some(req) = info.specialization {
            if !elite_ids.contains(&req) {
                continue;
            }
        }
        if info.flags.iter().any(|f| f == "TwoHand") {
            two_hand.push(name.clone());
        } else if info.flags.iter().any(|f| f == "Mainhand") {
            main.push(name.clone());
        }
        if info.flags.iter().any(|f| f == "Offhand") {
            off.push(name.clone());
        }
    }
    two_hand.sort();
    main.sort();
    off.sort();
    let mut combos = Vec::new();
    for w in two_hand {
        combos.push((Some(w), None));
    }
    for m in &main {
        combos.push((Some(m.clone()), None));
        for o in &off {
            if o != m {
                combos.push((Some(m.clone()), Some(o.clone())));
            }
        }
    }
    combos
}

fn swap_weapons(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
) -> Vec<ValidatedBuild> {
    let Some(profession) = db.profession(profession_name) else {
        return Vec::new();
    };
    let elite_ids: Vec<u32> = candidate
        .validated
        .specializations
        .iter()
        .filter(|s| s.elite)
        .map(|s| s.spec_id)
        .collect();
    let combos = land_weapon_combos(profession, &elite_ids);
    let set1 = (
        candidate.validated.weapons.set1.main_hand.clone(),
        candidate.validated.weapons.set1.off_hand.clone(),
    );
    let set2 = (
        candidate.validated.weapons.set2.main_hand.clone(),
        candidate.validated.weapons.set2.off_hand.clone(),
    );
    let mut out = Vec::new();
    for combo in combos {
        if combo != set1 {
            let mut b = candidate.validated.clone();
            b.weapons.set1 = ValidatedWeaponSet {
                main_hand: combo.0.clone(),
                off_hand: combo.1.clone(),
            };
            out.push(b);
        }
        if combo != set2 {
            let mut b = candidate.validated.clone();
            b.weapons.set2 = ValidatedWeaponSet {
                main_hand: combo.0,
                off_hand: combo.1,
            };
            out.push(b);
        }
        if out.len() >= 16 {
            break;
        }
    }
    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use gw2_api::models::{Item, ItemDetails};

    use super::*;
    use crate::combat::CombatPerformance;
    use crate::data::DataQuality;
    use crate::referee::{GateResult, RefereeReport, ViabilityGate, ViabilityReport};
    use crate::scenario::{CombatTier, ScenarioSpec};
    use crate::stats::StatBlock;
    use crate::validation::{ValidatedBuild, ValidatedItem, ValidatedSpec};
    use gw2_core::types::{BuildLocks, GameMode};

    fn empty_db() -> GameDb {
        GameDb {
            items: HashMap::new(),
            itemstats: HashMap::new(),
            skills: HashMap::new(),
            traits: HashMap::new(),
            specializations: HashMap::new(),
            professions: HashMap::new(),
            legends: HashMap::new(),
            pvp_amulets: HashMap::new(),
            pets: HashMap::new(),
            skills_by_profession: HashMap::new(),
            traits_by_spec: HashMap::new(),
            items_by_type: HashMap::new(),
            runes: Vec::new(),
            sigils: Vec::new(),
            relics: Vec::new(),
            skill_to_palette: HashMap::new(),
            palette_to_skill: HashMap::new(),
            traits_by_condition: HashMap::new(),
            skills_by_condition: HashMap::new(),
            traits_by_buff: HashMap::new(),
            skills_by_buff: HashMap::new(),
            localized: None,
        }
    }

    fn dummy_report() -> RefereeReport {
        use crate::combat::DamageModifiers;
        RefereeReport {
            scenario: ScenarioSpec {
                game_mode: GameMode::PvE,
                combat_tier: CombatTier::Solo,
                combat_kind: crate::scenario::CombatKind::StrikeSpike,
                target_profile: crate::scenario::TargetProfile::Single,
                optimization_target: crate::scenario::OptimizationTarget {
                    label: String::new(),
                },
                patch_id: None,
            },
            stats: StatBlock::default(),
            modifiers: DamageModifiers::default(),
            combat_solo: CombatPerformance::default(),
            combat_party: CombatPerformance::default(),
            combat_squad: CombatPerformance::default(),
            primary_combat: CombatPerformance::default(),
            rotation: None,
            viability: ViabilityReport {
                gates: Vec::new(),
                is_viable: true,
            },
            user_intent_score: 0.0,
            raw_direction_score: -1.0,
            quality: DataQuality::Verified,
            quality_reasons: Vec::new(),
        }
    }

    fn make_candidate(validated: ValidatedBuild) -> BeamCandidate {
        BeamCandidate {
            validated,
            report: dummy_report(),
        }
    }

    #[test]
    fn finish_search_returns_best_nonviable_locked_candidate() {
        let locked = ValidatedBuild {
            specializations: vec![ValidatedSpec {
                spec_id: 5,
                name: "Druid".into(),
                elite: true,
                trait_ids: vec![1, 2, 3],
                trait_names: vec!["A".into(), "B".into(), "C".into()],
                all_trait_ids: vec![1, 2, 3],
            }],
            rune: Some(ValidatedItem {
                id: 9,
                name: "Scholar".into(),
            }),
            ..ValidatedBuild::default()
        };
        let mut report = dummy_report();
        report.viability.is_viable = false;
        report.viability.gates = vec![GateResult {
            gate: ViabilityGate::ProtectedExecution,
            passed: false,
            note: "protected=0ms (minimum 2000ms secured inside the sequence)".into(),
        }];
        let result = finish_search(vec![BeamCandidate {
            validated: locked,
            report: report.clone(),
        }])
        .expect("non-viable locked candidate is a result");
        assert_eq!(result.specializations[0].name, "Druid");
        assert_eq!(result.rune.as_ref().map(|item| item.id), Some(9));
        let summary = crate::referee::viability_failure_summary(&report.viability);
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains(&summary)),
            "warnings: {:?}",
            result.warnings
        );
    }

    #[test]
    fn finish_search_empty_beam_still_errors() {
        let err = finish_search(Vec::new()).unwrap_err();
        assert_eq!(err, "No candidates survived beam search");
    }

    #[test]
    fn roam_nonviable_tie_honors_user_weights() {
        let mut lower = dummy_report();
        lower.scenario.game_mode = GameMode::WvW;
        lower.scenario.combat_tier = CombatTier::Solo;
        lower.viability.is_viable = false;
        lower.viability.gates = vec![GateResult {
            gate: ViabilityGate::ProtectedExecution,
            passed: false,
            note: "protected=0ms".into(),
        }];
        lower.user_intent_score = 0.1;
        let mut higher = lower.clone();
        higher.user_intent_score = 0.8;
        assert!(
            crate::referee::search_rank(&higher) > crate::referee::search_rank(&lower),
            "user_intent_score must break a non-viable roam tie"
        );
    }

    #[test]
    fn roam_rank_prefers_viable_kit_over_paper_stack() {
        let mut playable = dummy_report();
        playable.scenario.game_mode = GameMode::WvW;
        playable.scenario.combat_tier = CombatTier::Solo;
        playable.viability.is_viable = true;
        playable.user_intent_score = 0.2;

        let mut glass = playable.clone();
        glass.viability.is_viable = false;
        glass.user_intent_score = -1.0;

        assert!(
            crate::referee::search_rank(&playable) > crate::referee::search_rank(&glass),
            "viable roam kit must beat a non-viable number stack"
        );
    }

    #[test]
    fn roam_rank_prefers_more_gates_when_both_nonviable() {
        let mut closer = dummy_report();
        closer.scenario.game_mode = GameMode::WvW;
        closer.scenario.combat_tier = CombatTier::Solo;
        closer.viability.is_viable = false;
        closer.viability.gates = vec![
            GateResult {
                gate: ViabilityGate::StabilityAccess,
                passed: true,
                note: String::new(),
            },
            GateResult {
                gate: ViabilityGate::EncounterOutcome,
                passed: false,
                note: String::new(),
            },
        ];
        let mut farther = closer.clone();
        farther.viability.gates[0].passed = false;
        assert!(crate::referee::search_rank(&closer) > crate::referee::search_rank(&farther));
    }

    /// generate_neighbors on an empty DB must not panic and must return an
    /// empty Vec (no neighbors exist when the DB has no items/skills).
    #[test]
    fn test_generate_neighbors_empty_db_no_panic() {
        let db = empty_db();
        let candidate = make_candidate(ValidatedBuild::default());
        let neighbors = generate_neighbors(
            &candidate,
            &db,
            "Guardian",
            &BuildLocks::default(),
            &OptimizationWeights::default(),
        );
        // No items or skills → no neighbors.
        assert!(
            neighbors.is_empty(),
            "expected empty neighbors from empty DB, got {}",
            neighbors.len()
        );
    }

    /// With 2 Superior runes in the DB, generate_neighbors should produce
    /// exactly 2 neighbor builds from the rune-swap operator, each with a
    /// distinct rune ID.
    #[test]
    fn test_swap_rune_two_options() {
        let mut db = empty_db();

        // Build two Superior rune items.
        let rune1 = Item {
            id: 101,
            name: "Superior Rune of the Scholar".to_string(),
            description: None,
            icon: None,
            item_type: "UpgradeComponent".to_string(),
            rarity: "Exotic".to_string(),
            level: 60,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: Vec::new(),
            game_types: Vec::new(),
            restrictions: Vec::new(),
            details: Some(ItemDetails {
                detail_type: Some("Rune".to_string()),
                weight_class: None,
                defense: None,
                damage_type: None,
                min_power: None,
                max_power: None,
                suffix: None,
                bonuses: Vec::new(),
                infusion_upgrade_flags: Vec::new(),
                infusion_slots: Vec::new(),
                attribute_adjustment: None,
                infix_upgrade: None,
                suffix_item_id: None,
                secondary_suffix_item_id: None,
                stat_choices: Vec::new(),
            }),
        };
        let mut rune2 = rune1.clone();
        rune2.id = 102;
        rune2.name = "Superior Rune of the Berserker".to_string();

        db.items.insert(101, rune1);
        db.items.insert(102, rune2);
        db.runes.push(101);
        db.runes.push(102);

        let candidate = make_candidate(ValidatedBuild::default());
        let neighbors = generate_neighbors(
            &candidate,
            &db,
            "Warrior",
            &BuildLocks::default(),
            &OptimizationWeights::default(),
        );

        // Collect the rune IDs that appear in results.
        let rune_ids: Vec<u32> = neighbors
            .iter()
            .filter_map(|b| b.rune.as_ref().map(|r| r.id))
            .collect();

        assert_eq!(
            rune_ids.len(),
            2,
            "expected 2 rune-swap neighbors, got {}",
            rune_ids.len()
        );
        assert!(rune_ids.contains(&101), "expected rune ID 101 in neighbors");
        assert!(rune_ids.contains(&102), "expected rune ID 102 in neighbors");
    }

    /// Regression: generate_neighbors interleaves operator outputs so
    /// `take(neighbor_cap)` exposes diversity. If we set many gear prefixes
    /// AND a rune, the cap'd subset must include the rune mutation rather
    /// than only gear-prefix swaps.
    #[test]
    fn test_generate_neighbors_interleaves_operators() {
        let mut db = empty_db();
        // Add 5 itemstats so swap_gear_prefix produces 5 neighbors first.
        for i in 1..=5u32 {
            db.itemstats.insert(
                i,
                gw2_api::models::ItemStat {
                    id: i,
                    name: format!("Prefix{}", i),
                    attributes: Vec::new(),
                },
            );
        }
        // Add 1 rune.
        let rune = Item {
            id: 200,
            name: "Superior Rune of the Test".to_string(),
            description: None,
            icon: None,
            item_type: "UpgradeComponent".to_string(),
            rarity: "Exotic".to_string(),
            level: 60,
            vendor_value: None,
            chat_link: None,
            default_skin: None,
            flags: Vec::new(),
            game_types: Vec::new(),
            restrictions: Vec::new(),
            details: Some(ItemDetails {
                detail_type: Some("Rune".to_string()),
                weight_class: None,
                defense: None,
                damage_type: None,
                min_power: None,
                max_power: None,
                suffix: None,
                bonuses: Vec::new(),
                infusion_upgrade_flags: Vec::new(),
                infusion_slots: Vec::new(),
                attribute_adjustment: None,
                infix_upgrade: None,
                suffix_item_id: None,
                secondary_suffix_item_id: None,
                stat_choices: Vec::new(),
            }),
        };
        db.items.insert(200, rune);
        db.runes.push(200);

        let candidate = make_candidate(ValidatedBuild::default());
        let neighbors = generate_neighbors(
            &candidate,
            &db,
            "Warrior",
            &BuildLocks::default(),
            &OptimizationWeights::default(),
        );
        // The first round now includes whole-build gear, grouped gear, and rune
        // mutations. The rune must still be reachable inside a small cap.
        assert!(
            neighbors[0].primary_prefix().is_some(),
            "first neighbor should be a gear-prefix mutation"
        );
        assert!(
            neighbors
                .iter()
                .take(3)
                .any(|neighbor| neighbor.rune.is_some()),
            "the first operator round should include a rune mutation"
        );
    }

    #[test]
    fn weighted_prefixes_are_reachable_before_the_neighbor_cap() {
        let mut db = empty_db();
        for (id, name) in [
            (1, "Berserker's"),
            (2, "Marauder's"),
            (3, "Soldier's"),
            (900, "Plaguedoctor's"),
        ] {
            db.itemstats.insert(
                id,
                gw2_api::models::ItemStat {
                    id,
                    name: name.into(),
                    attributes: Vec::new(),
                },
            );
        }
        let weights = OptimizationWeights {
            power: 0.10,
            condition: 0.55,
            boon_support: 0.19,
            healing: 0.32,
            sustain: 0.42,
            control: 0.42,
        };
        let candidate = make_candidate(ValidatedBuild::default());

        let neighbors =
            generate_neighbors(&candidate, &db, "Ranger", &BuildLocks::default(), &weights);

        assert_eq!(
            neighbors[0]
                .primary_prefix()
                .map(|prefix| prefix.name.as_str()),
            Some("Plaguedoctor's")
        );
        assert!(neighbors.iter().take(12).any(|build| {
            build
                .gear_slots
                .get(GearSlot::Helm)
                .is_some_and(|prefix| prefix.name == "Plaguedoctor's")
        }));
    }

    #[test]
    fn swap_slot_prefix_reaches_independent_slot_pair() {
        let mut db = empty_db();
        for (id, name) in [
            (1, "Berserker's"),
            (2, "Cavalier's"),
            (3, "Marauder's"),
            (4, "Soldier's"),
        ] {
            db.itemstats.insert(
                id,
                gw2_api::models::ItemStat {
                    id,
                    name: name.into(),
                    attributes: Vec::new(),
                },
            );
        }
        // Seed uniform Berserker's — every slot carries its own prefix, so the
        // slot operator must be able to flip one piece without touching others.
        let mut validated = ValidatedBuild::default();
        validated.fill_gear_slots(PrefixRef {
            itemstat_id: 1,
            name: "Berserker's".into(),
        });
        let candidate = make_candidate(validated);

        let weights = OptimizationWeights::default();
        let neighbors = swap_slot_prefix(&candidate, &db, &weights, &HashMap::new());

        // The pair (helm=Berserker's AND coat=Cavalier's) is directly reachable.
        assert!(neighbors.iter().any(|build| {
            build
                .gear_slots
                .get(GearSlot::Helm)
                .is_some_and(|p| p.itemstat_id == 1)
                && build
                    .gear_slots
                    .get(GearSlot::Coat)
                    .is_some_and(|p| p.name == "Cavalier's")
        }));
        // No-op same-prefix swaps are never emitted: exactly one slot differs
        // from the seed in each neighbor, and none is identical to the seed.
        for neighbor in &neighbors {
            assert_ne!(
                neighbor.gear_identity(),
                candidate.validated.gear_identity()
            );
            let differing = GearSlot::ALL
                .iter()
                .filter(|slot| {
                    neighbor.gear_slots.prefix_id(**slot)
                        != candidate.validated.gear_slots.prefix_id(**slot)
                })
                .count();
            assert_eq!(differing, 1, "per-slot op changes exactly one slot");
        }
        // The pair survives round-robin interleave + neighbor cap too.
        let interleaved =
            generate_neighbors(&candidate, &db, "Warrior", &BuildLocks::default(), &weights);
        assert!(interleaved.iter().take(80).any(|build| {
            build
                .gear_slots
                .get(GearSlot::Helm)
                .is_some_and(|p| p.itemstat_id == 1)
                && build
                    .gear_slots
                    .get(GearSlot::Coat)
                    .is_some_and(|p| p.name == "Cavalier's")
        }));
    }

    /// Grok F7: the whole-build operator must not offer the prefix the build
    /// already wears.
    ///
    /// Its no-op filter used to be hand-written, and it read an unlocked *empty*
    /// cell as "this prefix would change something". A two-hander's off-hand is
    /// permanently empty, so the filter never rejected anything and the current
    /// prefix came back as a neighbour identical to the candidate — one referee
    /// evaluation per beam member per generation that could not move a number.
    #[test]
    fn swap_gear_prefix_skips_the_prefix_already_worn() {
        let mut db = empty_db();
        for (id, name) in [(1, "Berserker's"), (2, "Cavalier's")] {
            db.itemstats.insert(
                id,
                gw2_api::models::ItemStat {
                    id,
                    name: name.into(),
                    attributes: Vec::new(),
                },
            );
        }

        let mut validated = ValidatedBuild {
            weapons: crate::validation::ValidatedWeapons {
                set1: ValidatedWeaponSet {
                    main_hand: Some("Greatsword".into()),
                    off_hand: None,
                },
                set2: ValidatedWeaponSet::default(),
            },
            ..ValidatedBuild::default()
        };
        let worn = PrefixRef {
            itemstat_id: 1,
            name: "Berserker's".into(),
        };
        validated.fill_worn_gear_slots(worn.clone());
        let candidate = make_candidate(validated);

        let neighbors = swap_gear_prefix(
            &candidate,
            &db,
            &OptimizationWeights::default(),
            &HashMap::new(),
        );

        for neighbor in &neighbors {
            assert_ne!(
                neighbor.gear_identity(),
                candidate.validated.gear_identity(),
                "a neighbour identical to the candidate is a wasted referee evaluation"
            );
        }
        // Two prefixes in the pool, one of them already worn everywhere: exactly
        // one whole-build swap is available.
        assert_eq!(neighbors.len(), 1, "expected only the Cavalier's fill");
        assert_eq!(
            neighbors[0].gear_slots.get(GearSlot::WeaponSet1Off),
            None,
            "a greatsword's off-hand stays empty under a whole-build fill"
        );
    }

    /// C19 / Grok F2: the per-slot operator must not spend its whole share of the
    /// beam's neighbour cap on the first entries of `STAT_SLOTS`.
    ///
    /// `generate_neighbors` interleaves a dozen operators and `optimize_v2_search`
    /// then takes at most ~80 neighbours per beam member, so this operator gets
    /// roughly six or seven evaluations. Slot-major output — every prefix for the
    /// helm, then every prefix for the shoulders — spent all of them on the first
    /// two slots, and rings, amulet, and weapons never received a single per-slot
    /// evaluation in the beam. That is the hybrid-mix search v1.7 added this
    /// operator for.
    #[test]
    fn swap_slot_prefix_round_robins_slots() {
        let mut db = empty_db();
        for (id, name) in [
            (1, "Berserker's"),
            (2, "Cavalier's"),
            (3, "Marauder's"),
            (4, "Soldier's"),
            (5, "Celestial"),
        ] {
            db.itemstats.insert(
                id,
                gw2_api::models::ItemStat {
                    id,
                    name: name.into(),
                    attributes: Vec::new(),
                },
            );
        }

        // A greatsword build: main hand worn, off hand empty, set 2 bare.
        // `fill_gear_slots` stamps a prefix into all sixteen cells anyway —
        // including the off-hand this build does not wear and both carried set-2
        // hands — so the operator has to decide eligibility from the build, not
        // from "the cell is populated".
        let mut validated = ValidatedBuild {
            weapons: crate::validation::ValidatedWeapons {
                set1: ValidatedWeaponSet {
                    main_hand: Some("Greatsword".into()),
                    off_hand: None,
                },
                set2: ValidatedWeaponSet::default(),
            },
            ..ValidatedBuild::default()
        };
        validated.fill_gear_slots(PrefixRef {
            itemstat_id: 1,
            name: "Berserker's".into(),
        });
        let candidate = make_candidate(validated);

        let weights = OptimizationWeights::default();
        let neighbors = swap_slot_prefix(&candidate, &db, &weights, &HashMap::new());

        let changed_slot = |build: &ValidatedBuild| -> GearSlot {
            let mut differing = GearSlot::ALL.iter().copied().filter(|slot| {
                build.gear_slots.prefix_id(*slot) != candidate.validated.gear_slots.prefix_id(*slot)
            });
            let slot = differing
                .next()
                .expect("a per-slot neighbour changes one slot");
            assert!(
                differing.next().is_none(),
                "a per-slot neighbour changes exactly one slot"
            );
            slot
        };

        // Measured from the operator's own output rather than asserted from a
        // literal: however many slots it considers movable, its first that-many
        // neighbours have to land on that many *different* slots.
        let touched: Vec<GearSlot> = neighbors.iter().map(changed_slot).collect();
        let distinct: std::collections::HashSet<GearSlot> = touched.iter().copied().collect();
        assert!(
            distinct.len() > 1,
            "fixture must offer more than one movable slot"
        );
        assert!(
            touched.len() >= distinct.len(),
            "every movable slot gets at least one candidate prefix"
        );
        let first_pass: std::collections::HashSet<GearSlot> =
            touched.iter().copied().take(distinct.len()).collect();
        assert_eq!(
            first_pass.len(),
            distinct.len(),
            "the first {} neighbours must cover every movable slot exactly once, got {:?}",
            distinct.len(),
            &touched[..distinct.len()]
        );

        // Dead slots are not movable. Weapon set 2 is carried, not worn — it draws
        // no slot budget — and a greatsword has no off-hand, even though
        // `fill_gear_slots` left a prefix in all three of those cells.
        for dead in [
            GearSlot::WeaponSet1Off,
            GearSlot::WeaponSet2Main,
            GearSlot::WeaponSet2Off,
        ] {
            assert!(
                !distinct.contains(&dead),
                "{dead:?} carries no stats; swapping it is a referee evaluation that cannot change a number"
            );
        }
        assert!(
            distinct.contains(&GearSlot::WeaponSet1Main),
            "the worn main hand is stat-bearing and must be reachable"
        );

        // Same inputs, same sequence: the pool is id-ordered, not HashMap-ordered.
        let again = swap_slot_prefix(&candidate, &db, &weights, &HashMap::new());
        assert_eq!(
            again
                .iter()
                .map(|build| build.gear_identity())
                .collect::<Vec<_>>(),
            neighbors
                .iter()
                .map(|build| build.gear_identity())
                .collect::<Vec<_>>(),
            "neighbour order must be reproducible"
        );
    }

    /// C19 / Claude F16 + F20, Grok F1, GLM F7: the nudge draws from the canonical
    /// prefix pool, and it stops when its budget is gone.
    #[test]
    fn nudge_uses_canonical_pool_and_budget() {
        // ── the pool ────────────────────────────────────────────────────────────
        let mut db = empty_db();
        let power = |id: u32, name: &str, multiplier: f64| gw2_api::models::ItemStat {
            id,
            name: name.into(),
            attributes: vec![gw2_api::models::StatAttribute {
                attribute: "Power".into(),
                multiplier,
                value: 0,
            }],
        };
        // Two ids for one displayed prefix: 43 names look like this on live data,
        // and "Berserker's" alone carries five.
        db.itemstats.insert(161, power(161, "Berserker's", 0.35));
        db.itemstats.insert(1077, power(1077, "Berserker's", 0.35));
        // A legacy all-zero-multiplier row whose display name nothing else shares.
        // The slot-budget model cannot price it, so it is not a prefix at all —
        // and with a unique name the one-id-per-name rule has nothing to prefer
        // over it.
        db.itemstats
            .insert(1049, power(1049, "Legacy Ossified", 0.0));
        db.itemstats.insert(1015, power(1015, "Marauder's", 0.30));

        let weights = OptimizationWeights::default();
        let pool = nudge_pool(&db, &weights);

        let ids: Vec<u32> = pool.iter().map(|stat| stat.id).collect();
        let unique_ids: std::collections::HashSet<u32> = ids.iter().copied().collect();
        assert_eq!(
            ids.len(),
            unique_ids.len(),
            "no id may appear twice; the pool this replaced concatenated the tier \
             names onto the whole itemstat map, so every tier prefix was evaluated \
             twice: {ids:?}"
        );
        let names: Vec<String> = pool
            .iter()
            .map(|stat| normalized_prefix_name(&stat.name))
            .collect();
        let unique_names: std::collections::HashSet<&String> = names.iter().collect();
        assert_eq!(
            names.len(),
            unique_names.len(),
            "one id per displayed prefix; a 'top four' of four Berserker's ids \
             searches one prefix four times: {names:?}"
        );
        assert!(
            !ids.contains(&1049),
            "an unpriceable row is not a prefix a search may choose: {ids:?}"
        );
        assert!(
            ids.contains(&1015),
            "a healthy uniquely-named prefix must survive: {ids:?}"
        );
        // Every pooled row is the row name resolution hands back for the same
        // name, so the prefix that wins a search is the prefix that gets applied.
        for stat in &pool {
            assert_eq!(
                db.itemstat_by_name(&stat.name).map(|hit| hit.id),
                Some(stat.id),
                "pool entry {} ({}) is not what itemstat_by_name resolves",
                stat.id,
                stat.name
            );
        }

        // ── the budget ──────────────────────────────────────────────────────────
        use crate::synergy_pipeline::runtime_diagnostics_tests::make_diag_db;

        let diag = make_diag_db();
        let ctx = crate::balance::BalanceContext::new(GameMode::PvE);
        let scenario = ScenarioSpec {
            game_mode: GameMode::PvE,
            combat_tier: CombatTier::Solo,
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: crate::scenario::TargetProfile::Single,
            optimization_target: crate::scenario::OptimizationTarget {
                label: String::new(),
            },
            patch_id: None,
        };
        let seed = optimize_v2_search(
            &diag,
            "Warrior",
            &weights,
            &ctx,
            &scenario,
            &BuildLocks::default(),
            &SearchConfig {
                beam_width: 2,
                eval_budget: 40,
                time_limit_secs: 30,
            },
            &mut |_| {},
            &|| false,
        )
        .expect("diag db should seed a searchable build");

        // Start uniform on the id the diag fixture's referee ranks *lower*, so the
        // pass has a real single-piece improvement to find. Which of the two the
        // referee prefers is its business — this test asserts that the nudge finds
        // whatever the referee prefers, and that the budget stops it from looking.
        let mut uniform = seed;
        uniform.fill_worn_gear_slots(PrefixRef {
            itemstat_id: 584,
            name: "Berserker's".into(),
        });

        let run = |budget: NudgeBudget| {
            let mut rounds = 0usize;
            let out = refine_piece_swaps_within(
                uniform.clone(),
                &diag,
                "Warrior",
                &weights,
                &ctx,
                &scenario,
                &BuildLocks::default(),
                &mut |_| rounds += 1,
                &|| false,
                budget,
            );
            (out, rounds)
        };

        // Positive control: with the production budget the pass does real work.
        let (improved, full_rounds) = run(NudgeBudget::standard());
        assert!(full_rounds >= 1, "the pass must open at least one round");
        assert_ne!(
            improved.gear_identity(),
            uniform.gear_identity(),
            "an unstarved pass must find the improving single-piece swap"
        );
        // Whatever it moved, it moved to a prefix from the canonical pool — the
        // nudge cannot introduce an id the appliers can never resolve back.
        let diag_pool: std::collections::HashSet<u32> = nudge_pool(&diag, &weights)
            .iter()
            .map(|stat| stat.id)
            .collect();
        for slot in GearSlot::ALL {
            if let Some(id) = improved.gear_slots.prefix_id(slot) {
                assert!(
                    diag_pool.contains(&id),
                    "{slot:?} ended on itemstat {id}, which is not in the canonical pool"
                );
            }
        }

        // One allowance buys exactly the baseline evaluation: the pass opens a
        // round and then cannot afford a single swap. The difference between this
        // and the zero case below is the evaluation counter, measured rather than
        // asserted from a constant.
        let (starved, starved_rounds) = run(NudgeBudget::new(1, Duration::from_secs(60)));
        assert_eq!(
            starved_rounds, 1,
            "one allowance opens one round and buys nothing more"
        );
        assert_eq!(
            starved.gear_identity(),
            uniform.gear_identity(),
            "a starved pass must return the build it was given, unimproved"
        );

        // No allowance at all: not even the baseline evaluation, so no round opens.
        let (untouched, no_rounds) = run(NudgeBudget::new(0, Duration::from_secs(60)));
        assert_eq!(no_rounds, 0);
        assert_eq!(untouched.gear_identity(), uniform.gear_identity());

        // The wall clock bounds the pass on its own, whatever the eval count says:
        // per-evaluation cost is not a constant, and a rotation-heavy profession on
        // a slow machine is exactly the case the eval cap cannot see.
        let (expired, expired_rounds) = run(NudgeBudget::new(usize::MAX, Duration::ZERO));
        assert_eq!(
            expired_rounds, 0,
            "an expired deadline stops the pass before it evaluates anything"
        );
        assert_eq!(expired.gear_identity(), uniform.gear_identity());
    }

    #[test]
    fn gear_locked_slots_are_never_mutated_by_gear_operators() {
        let mut db = empty_db();
        for (id, name) in [
            (1, "Berserker's"),
            (2, "Cavalier's"),
            (3, "Marauder's"),
            (4, "Soldier's"),
        ] {
            db.itemstats.insert(
                id,
                gw2_api::models::ItemStat {
                    id,
                    name: name.into(),
                    attributes: Vec::new(),
                },
            );
        }
        // Locked slots hold a DIFFERENT prefix than everything else so that a
        // silent lock violation would show up as an identity change.
        let mut validated = ValidatedBuild::default();
        validated.fill_gear_slots(PrefixRef {
            itemstat_id: 2,
            name: "Cavalier's".into(),
        });
        validated.gear_slots.set(
            GearSlot::Helm,
            PrefixRef {
                itemstat_id: 1,
                name: "Berserker's".into(),
            },
        );
        let candidate = make_candidate(validated);

        let mut gear_locks = HashMap::new();
        gear_locks.insert(GearSlot::Helm, 1u32);
        gear_locks.insert(GearSlot::Ring2, 2u32);
        let locks = BuildLocks {
            gear_locks,
            ..Default::default()
        };

        let weights = OptimizationWeights::default();
        for neighbor in generate_neighbors(&candidate, &db, "Warrior", &locks, &weights) {
            assert_eq!(
                neighbor.gear_slots.prefix_id(GearSlot::Helm),
                Some(1),
                "locked helm must keep its prefix under any operator"
            );
            assert_eq!(
                neighbor.gear_slots.prefix_id(GearSlot::Ring2),
                Some(2),
                "locked ring must keep its prefix under any operator"
            );
        }
    }

    #[test]
    fn optimize_v2_search_is_deterministic_for_identical_inputs() {
        use crate::synergy_pipeline::runtime_diagnostics_tests::make_diag_db;

        let db = make_diag_db();
        let weights = OptimizationWeights::default();
        let ctx = crate::balance::BalanceContext::new(GameMode::PvE);
        let scenario = ScenarioSpec {
            game_mode: GameMode::PvE,
            combat_tier: CombatTier::Solo,
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: crate::scenario::TargetProfile::Single,
            optimization_target: crate::scenario::OptimizationTarget {
                label: String::new(),
            },
            patch_id: None,
        };
        let config = SearchConfig {
            beam_width: 4,
            eval_budget: 200,
            time_limit_secs: 30,
        };

        let run = || {
            optimize_v2_search(
                &db,
                "Warrior",
                &weights,
                &ctx,
                &scenario,
                &BuildLocks::default(),
                &config,
                &mut |_| {},
                &|| false,
            )
            .expect("diag db should seed a searchable build")
        };

        let first = run();
        let second = run();
        assert_eq!(
            first.gear_identity(),
            second.gear_identity(),
            "two runs with identical inputs must end on identical gear"
        );
        assert_eq!(
            first.primary_prefix().map(|p| p.name.clone()),
            second.primary_prefix().map(|p| p.name.clone())
        );
    }

    #[test]
    fn locked_helm_survives_optimize_v2_search() {
        use crate::synergy_pipeline::runtime_diagnostics_tests::make_diag_db;

        let db = make_diag_db();
        let weights = OptimizationWeights::default();
        let ctx = crate::balance::BalanceContext::new(GameMode::PvE);
        let scenario = ScenarioSpec {
            game_mode: GameMode::PvE,
            combat_tier: CombatTier::Solo,
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: crate::scenario::TargetProfile::Single,
            optimization_target: crate::scenario::OptimizationTarget {
                label: String::new(),
            },
            patch_id: None,
        };
        let mut locks = BuildLocks::default();
        locks.gear_locks.insert(GearSlot::Helm, 584);
        let config = SearchConfig {
            beam_width: 4,
            eval_budget: 200,
            time_limit_secs: 30,
        };

        let best = optimize_v2_search(
            &db,
            "Warrior",
            &weights,
            &ctx,
            &scenario,
            &locks,
            &config,
            &mut |_| {},
            &|| false,
        )
        .expect("diag db should seed a searchable build");

        assert_eq!(
            best.gear_slots.prefix_id(GearSlot::Helm),
            Some(584),
            "final build helm prefix must equal the locked itemstat id"
        );
    }

    /// optimize_v2_search on an empty DB (no professions) must return Err and
    /// must not panic.
    #[test]
    fn test_optimize_v2_search_empty_db_returns_err() {
        use crate::scenario::{CombatTier, OptimizationTarget, ScenarioSpec, TargetProfile};
        use gw2_core::types::{BuildLocks, GameMode};

        let db = empty_db();
        let weights = OptimizationWeights::default();
        let ctx = crate::balance::BalanceContext::new(GameMode::PvE);
        let scenario = ScenarioSpec {
            game_mode: GameMode::PvE,
            combat_tier: CombatTier::Solo,
            combat_kind: crate::scenario::CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: String::new(),
            },
            patch_id: None,
        };
        let locks = BuildLocks::default();
        let config = SearchConfig::default();

        let result = optimize_v2_search(
            &db,
            "Guardian",
            &weights,
            &ctx,
            &scenario,
            &locks,
            &config,
            &mut |_| {},
            &|| false,
        );

        assert!(
            result.is_err(),
            "expected Err from optimize_v2_search with empty DB, got Ok"
        );
    }

    /// SearchConfig::default() must have the expected sentinel values.
    #[test]
    fn test_search_config_default() {
        let config = SearchConfig::default();
        assert_eq!(config.beam_width, 10, "default beam_width should be 10");
        assert_eq!(
            config.eval_budget, 1500,
            "default eval_budget should be 1500"
        );
        assert_eq!(config.time_limit_secs, 45);
    }

    fn twohand(name: &str, spec: Option<u32>) -> (String, gw2_api::models::WeaponInfo) {
        (
            name.into(),
            gw2_api::models::WeaponInfo {
                specialization: spec,
                flags: vec!["TwoHand".into()],
                skills: Vec::new(),
            },
        )
    }

    #[test]
    fn land_weapon_combos_includes_spear_with_aquatic_flag() {
        let mut weapons = HashMap::new();
        weapons.insert(
            "Spear".into(),
            gw2_api::models::WeaponInfo {
                specialization: None,
                flags: vec!["TwoHand".into(), "Aquatic".into()],
                skills: Vec::new(),
            },
        );
        weapons.insert(
            "Trident".into(),
            gw2_api::models::WeaponInfo {
                specialization: None,
                flags: vec!["TwoHand".into(), "Aquatic".into()],
                skills: Vec::new(),
            },
        );
        weapons.insert(
            "Staff".into(),
            gw2_api::models::WeaponInfo {
                specialization: None,
                flags: vec!["TwoHand".into()],
                skills: Vec::new(),
            },
        );
        let prof = gw2_api::models::Profession {
            id: "Guardian".into(),
            name: "Guardian".into(),
            code: None,
            specializations: vec![],
            weapons,
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        };
        let mains: Vec<_> = land_weapon_combos(&prof, &[])
            .into_iter()
            .filter_map(|(m, _)| m)
            .collect();
        assert!(mains.iter().any(|w| w == "Spear"));
        assert!(mains.iter().any(|w| w == "Staff"));
        assert!(!mains.iter().any(|w| w == "Trident"));
    }

    fn spec_line(id: u32, name: &str, elite: bool) -> gw2_api::models::Specialization {
        gw2_api::models::Specialization {
            id,
            name: name.into(),
            profession: "Guardian".into(),
            elite,
            minor_traits: Vec::new(),
            major_traits: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
            weapon_trait: None,
            icon: None,
            background: None,
            profession_icon: None,
            profession_icon_big: None,
        }
    }

    #[test]
    fn swap_elite_respects_lock_and_jumps_when_free() {
        let mut db = empty_db();
        let dh = spec_line(27, "Dragonhunter", true);
        let fb = spec_line(62, "Firebrand", true);
        db.specializations.insert(27, dh);
        db.specializations.insert(62, fb);
        let mut weapons = HashMap::new();
        let (n, w) = twohand("Greatsword", None);
        weapons.insert(n, w);
        db.professions.insert(
            "Guardian".into(),
            gw2_api::models::Profession {
                id: "Guardian".into(),
                name: "Guardian".into(),
                code: None,
                specializations: vec![27, 62],
                weapons,
                training: Vec::new(),
                skills_by_palette: Vec::new(),
                icon: None,
                icon_big: None,
            },
        );

        let build = ValidatedBuild {
            specializations: vec![crate::validation::ValidatedSpec {
                spec_id: 27,
                name: "Dragonhunter".into(),
                elite: true,
                trait_ids: vec![1, 4, 7],
                trait_names: vec!["a".into(), "b".into(), "c".into()],
                all_trait_ids: vec![1, 4, 7],
            }],
            ..Default::default()
        };
        let candidate = make_candidate(build);

        let locked = BuildLocks {
            specs: [None, None, Some(27)],
            trait_locks: HashMap::new(),
            gear_locks: HashMap::new(),
        };
        let none_locked = generate_neighbors(
            &candidate,
            &db,
            "Guardian",
            &locked,
            &OptimizationWeights::default(),
        );
        assert!(
            none_locked
                .iter()
                .all(|b| b.specializations.iter().all(|s| s.spec_id == 27)),
            "locked elite must not jump"
        );

        let jumped = generate_neighbors(
            &candidate,
            &db,
            "Guardian",
            &BuildLocks::default(),
            &OptimizationWeights::default(),
        );
        assert!(
            jumped
                .iter()
                .any(|b| b.specializations.iter().any(|s| s.elite && s.spec_id == 62)),
            "free elite must jump to Firebrand"
        );
    }

    #[test]
    fn swap_weapons_emits_other_land_set() {
        let mut db = empty_db();
        let mut weapons = HashMap::new();
        let (n, w) = twohand("Greatsword", None);
        weapons.insert(n, w);
        let (n, w) = twohand("Staff", None);
        weapons.insert(n, w);
        db.professions.insert(
            "Guardian".into(),
            gw2_api::models::Profession {
                id: "Guardian".into(),
                name: "Guardian".into(),
                code: None,
                specializations: Vec::new(),
                weapons,
                training: Vec::new(),
                skills_by_palette: Vec::new(),
                icon: None,
                icon_big: None,
            },
        );
        let build = ValidatedBuild {
            weapons: crate::validation::ValidatedWeapons {
                set1: crate::validation::ValidatedWeaponSet {
                    main_hand: Some("Greatsword".into()),
                    off_hand: None,
                },
                set2: Default::default(),
            },
            ..Default::default()
        };
        let candidate = make_candidate(build);
        let neighbors = generate_neighbors(
            &candidate,
            &db,
            "Guardian",
            &BuildLocks::default(),
            &OptimizationWeights::default(),
        );
        assert!(
            neighbors
                .iter()
                .any(|b| b.weapons.set1.main_hand.as_deref() == Some("Staff")),
            "weapon jump should offer Staff"
        );
    }
}
