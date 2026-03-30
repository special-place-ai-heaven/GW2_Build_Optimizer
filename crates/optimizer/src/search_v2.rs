//! Search v2 — complete build state beam/evolutionary search.
//!
//! This module provides the foundational types and mutation operators used by
//! `optimize_v2()`.  The search loop (T02) builds on top of the primitives
//! defined here.

use crate::gamedb::GameDb;
use crate::referee::RefereeReport;
use crate::validation::{ValidatedBuild, ValidatedGearPrefix, ValidatedItem};

// ─── Core types ──────────────────────────────────────────────────────────────

/// A single candidate on the beam: a fully-validated build together with its
/// referee evaluation (score, viability, stats, …).
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
pub fn generate_neighbors(
    candidate: &BeamCandidate,
    db: &GameDb,
    profession_name: &str,
) -> Vec<ValidatedBuild> {
    let mut neighbors: Vec<ValidatedBuild> = Vec::new();

    neighbors.extend(swap_gear_prefix(candidate, db));
    neighbors.extend(swap_rune(candidate, db));
    neighbors.extend(swap_sigil_slots(candidate, db));
    neighbors.extend(swap_relic(candidate, db));
    neighbors.extend(swap_utility_skills(candidate, db, profession_name));

    neighbors
}

// ─── Individual mutation operators (private helpers) ─────────────────────────

/// Operator 1 — swap gear prefix.
///
/// For every `ItemStat` in the DB, produce a clone of the current build with
/// `gear_prefix` set to that stat.  This covers all available gear prefixes
/// (Berserker's, Viper's, …).
fn swap_gear_prefix(candidate: &BeamCandidate, db: &GameDb) -> Vec<ValidatedBuild> {
    db.itemstats
        .values()
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

    let slot_count = candidate.validated.sigils.len().max(2).min(2);
    let mut neighbors: Vec<ValidatedBuild> = Vec::new();

    for slot_idx in 0..slot_count {
        // IDs currently in the *other* slots (to avoid duplicates)
        let other_ids: Vec<u32> = candidate
            .validated
            .sigils
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != slot_idx)
            .filter_map(|(_, s)| Some(s.id))
            .collect();

        for sigil in &superior_sigils {
            if other_ids.contains(&sigil.id) {
                continue; // would duplicate a sigil in another slot
            }
            let mut b = candidate.validated.clone();
            // Ensure the sigils vec has at least `slot_idx + 1` entries.
            while b.sigils.len() <= slot_idx {
                // Pad with a placeholder that will be overwritten.
                b.sigils.push(ValidatedItem { id: 0, name: String::new() });
            }
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
                    None => true,  // slot absent but in profession list → eligible
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
                optimization_target: crate::scenario::OptimizationTarget { label: String::new() },
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
        assert!(
            rune_ids.contains(&101),
            "expected rune ID 101 in neighbors"
        );
        assert!(
            rune_ids.contains(&102),
            "expected rune ID 102 in neighbors"
        );
    }
}
