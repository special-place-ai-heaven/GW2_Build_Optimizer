//! Search v2 — complete build state beam/evolutionary search.
//!
//! This module provides the foundational types and mutation operators used by
//! `optimize_v2()`.  The search loop (T02) builds on top of the primitives
//! defined here.

use std::time::Instant;

use crate::balance::BalanceContext;
use crate::engine::OptimizeProgress;
use crate::gamedb::GameDb;
use crate::referee::{self, RefereeReport};
use crate::scenario::ScenarioSpec;
use crate::scoring::{self, OptimizationWeights};
use crate::synergy_pipeline;
use crate::validation::{ValidatedBuild, ValidatedGearPrefix, ValidatedItem};

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
            eval_budget: 200,
            time_limit_secs: 28,
        }
    }
}

// ─── Mutation operators ───────────────────────────────────────────────────────

/// Generate all immediate neighbours of `candidate` by applying each of the
/// five atomic mutation operators in turn and collecting the results.
///
/// Each operator clones the current `ValidatedBuild`, changes exactly one
/// slot, and appends the mutated build to the output.  Operators that find no
/// alternatives (e.g. because the DB is empty) simply contribute nothing to
/// the output — the function never panics on an empty `GameDb`.
///
/// Output is interleaved round-robin across operators rather than concatenated.
/// `optimize_v2_search` caps evaluation per beam member at ~30 neighbors;
/// concatenated order would burn the entire budget on `swap_gear_prefix`
/// (which produces ~50 neighbors and appears first), and the search would
/// never explore rune/sigil/relic/utility mutations at all.
pub fn generate_neighbors(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
) -> Vec<ValidatedBuild> {
    let groups: [Vec<ValidatedBuild>; 5] = [
        swap_gear_prefix(candidate, db),
        swap_rune(candidate, db),
        swap_sigil_slots(candidate, db),
        swap_relic(candidate, db),
        swap_utility_skills(candidate, db, profession_name),
    ];

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
pub fn optimize_v2_search(
    db: &GameDb,
    profession_name: &str,
    weights: &OptimizationWeights,
    ctx: &BalanceContext,
    scenario: &ScenarioSpec,
    locks: &gw2_core::types::BuildLocks,
    config: &SearchConfig,
    on_progress: &mut dyn FnMut(OptimizeProgress),
) -> Result<ValidatedBuild, String> {
    // Step 1: select gear prefix (cosine sim).
    let gear_match = scoring::select_gear_prefix(weights);
    let prefix_name = gear_match.primary;

    // Step 2: seed from synergy pipeline.
    on_progress(OptimizeProgress {
        stage: "Seeding from synergy pipeline...".into(),
        done: false,
    });
    let seed_result = synergy_pipeline::optimize_synergy(
        db,
        profession_name,
        weights,
        ctx,
        prefix_name,
        locks,
        &mut |_| {},
    )?;

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
    let mut eval_count = 0usize;

    // Step 5: beam loop.
    while eval_count < config.eval_budget && start.elapsed().as_secs() < config.time_limit_secs {
        let mut next: Vec<BeamCandidate> = Vec::new();

        // Elitism: keep current beam members in the candidate pool.
        next.extend(beam.iter().cloned());

        // Budget per candidate to avoid spending all evals on a single member.
        let budget_per = config.eval_budget.saturating_sub(eval_count) / beam.len().max(1);
        let neighbor_cap = budget_per.min(30).max(1);

        for candidate in &beam {
            let neighbors = generate_neighbors(candidate, db, profession_name);
            for neighbor in neighbors.into_iter().take(neighbor_cap) {
                if eval_count >= config.eval_budget {
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

        // Sort: higher user_intent_score first.
        next.sort_by(|a, b| {
            b.report
                .user_intent_score
                .partial_cmp(&a.report.user_intent_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // De-duplicate by (score, gear_prefix, rune) to keep diversity.
        next.dedup_by(|a, b| {
            a.report.user_intent_score == b.report.user_intent_score
                && a.validated.gear_prefix == b.validated.gear_prefix
                && a.validated.rune == b.validated.rune
        });

        next.truncate(config.beam_width);

        if next.is_empty() {
            break;
        }
        beam = next;
    }

    // Step 6: return best.
    beam.into_iter()
        .next()
        .map(|c| c.validated)
        .ok_or_else(|| "No candidates survived beam search".to_string())
}

// ─── Individual mutation operators (private helpers) ─────────────────────────

/// Operator 1 — swap gear prefix.
///
/// For every `ItemStat` in the DB, produce a clone of the current build with
/// `gear_prefix` set to that stat.  This covers all available gear prefixes
/// (Berserker's, Viper's, …).
///
/// Iterates by id so beam-search neighbor ordering — and therefore the
/// tie-break behavior in the downstream `sort_by + dedup_by + truncate`
/// pipeline — is stable across runs.
fn swap_gear_prefix(candidate: &BeamCandidate, db: &GameDb) -> Vec<ValidatedBuild> {
    let mut itemstats: Vec<&gw2_api::models::ItemStat> = db.itemstats.values().collect();
    itemstats.sort_by_key(|is| is.id);
    itemstats
        .into_iter()
        .map(|is| {
            let mut b = candidate.validated.clone();
            b.gear_prefix = Some(ValidatedGearPrefix {
                itemstat_id: is.id,
                name: is.name.clone(),
            });
            b
        })
        .collect()
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

fn normalize_sigil_family(name: &str) -> String {
    let mut base = name.replace(" (PvP)", "");
    base.make_ascii_lowercase();
    base.trim().to_string()
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
                let slot_ok = match skill.slot.as_deref() {
                    Some("Utility") => true,
                    None => true, // slot absent but in profession list → eligible
                    _ => false,
                };
                if !slot_ok {
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use gw2_api::models::{Item, ItemDetails};

    use super::*;
    use crate::combat::CombatPerformance;
    use crate::data::DataQuality;
    use crate::referee::{RefereeReport, ViabilityReport};
    use crate::scenario::{CombatTier, ScenarioSpec};
    use crate::stats::StatBlock;
    use crate::validation::ValidatedBuild;
    use gw2_core::types::GameMode;

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
        }
    }

    fn dummy_report() -> RefereeReport {
        use crate::combat::DamageModifiers;
        use crate::genome::BuildGenome;
        RefereeReport {
            genome: BuildGenome::from_validated("", &ValidatedBuild::default()),
            scenario: ScenarioSpec {
                game_mode: GameMode::PvE,
                combat_tier: CombatTier::Solo,
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

    /// generate_neighbors on an empty DB must not panic and must return an
    /// empty Vec (no neighbors exist when the DB has no items/skills).
    #[test]
    fn test_generate_neighbors_empty_db_no_panic() {
        let db = empty_db();
        let candidate = make_candidate(ValidatedBuild::default());
        let neighbors = generate_neighbors(&candidate, &db, "Guardian");
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
        let neighbors = generate_neighbors(&candidate, &db, "Warrior");

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
        let neighbors = generate_neighbors(&candidate, &db, "Warrior");
        // First two emitted positions: gear-prefix (group 0 round 0), then rune
        // (group 1 round 0). Confirms rune is reachable inside a small cap.
        assert!(
            neighbors[0].gear_prefix.is_some(),
            "first neighbor should be a gear-prefix mutation"
        );
        assert!(
            neighbors[1].rune.is_some(),
            "second neighbor should be a rune mutation (round-robin interleave)"
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
        assert_eq!(config.eval_budget, 200, "default eval_budget should be 200");
    }
}
