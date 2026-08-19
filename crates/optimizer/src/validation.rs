//! Validates Gemini build output against the GameDb.
//! Resolves names to IDs, checks GW2 build rules (spec slots, trait columns,
//! weapon availability, skill slots), and reports errors and warnings.
//! A ValidatedBuild is always returned — even with errors — so the caller
//! can decide whether to proceed with a partial result.

use std::collections::HashMap;

use gw2_api::models::{Item, Skill, Specialization, Trait as GW2Trait};

use crate::gamedb::GameDb;
use crate::prompts::GeminiBuildResponse;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A fully validated and resolved build from Gemini output.
#[derive(Debug, Clone, Default)]
pub struct ValidatedBuild {
    pub specializations: Vec<ValidatedSpec>,
    pub weapons: ValidatedWeapons,
    pub skills: ValidatedSkills,
    /// Revenant terrestrial legends (`Legend1`…), active first.
    pub legends: Vec<String>,
    /// Revenant aquatic legends. Empty → encoder copies terrestrial.
    pub aquatic_legends: Vec<String>,
    /// Ranger pet IDs: terrestrial[2] then aquatic[2].
    pub pets: Option<(Option<u32>, Option<u32>, Option<u32>, Option<u32>)>,
    pub rune: Option<ValidatedItem>,
    pub sigils: Vec<ValidatedItem>,
    pub relic: Option<ValidatedItem>,
    pub gear_prefix: Option<ValidatedGearPrefix>,
    pub explanation: String,
    pub synergy_explanation: String,
    pub changes: Vec<ChangeEntry>,
    pub warnings: Vec<String>,
    pub errors: Vec<ValidationReject>,
}

#[derive(Debug, Clone)]
pub struct ValidatedSpec {
    pub spec_id: u32,
    pub name: String,
    pub elite: bool,
    /// Selected major trait IDs (3 per spec: Adept, Master, Grandmaster).
    pub trait_ids: Vec<u32>,
    pub trait_names: Vec<String>,
    /// All equipped trait IDs: minor traits + selected major traits.
    pub all_trait_ids: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct ValidatedWeapons {
    pub set1: ValidatedWeaponSet,
    pub set2: ValidatedWeaponSet,
}

#[derive(Debug, Clone, Default)]
pub struct ValidatedWeaponSet {
    pub main_hand: Option<String>,
    pub off_hand: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ValidatedSkills {
    pub heal: Option<(u32, String)>,
    pub utilities: Vec<Option<(u32, String)>>,
    pub elite: Option<(u32, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedItem {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedGearPrefix {
    pub itemstat_id: u32,
    pub name: String,
}

/// A structured change entry from Gemini's output.
#[derive(Debug, Clone)]
pub struct ChangeEntry {
    pub slot: String,
    pub from: String,
    pub to: String,
    pub reason: String,
}

/// Machine-readable rejection code. Pair with `ValidationReject.detail`
/// (which is human-readable) so a retry loop can key off the typed code
/// and feed the LLM a precise correction instruction rather than parse
/// prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectCode {
    /// Build spec requires exactly 3 specializations; got a different count.
    WrongSpecCount { expected: usize, actual: usize },
    /// Specialization name not found for the given profession.
    SpecNotFound { spec: String, profession: String },
    /// Specialization exists but belongs to another profession.
    SpecWrongProfession {
        spec: String,
        owner: String,
        expected: String,
    },
    /// More than one elite spec in the 3-slot list.
    MultipleEliteSpecs { spec: String },
    /// Weapon not available for the profession.
    WeaponNotAvailable {
        slot: String,
        weapon: String,
        profession: String,
    },
    /// Weapon requires an elite spec that is not equipped.
    WeaponGatedBySpec {
        slot: String,
        weapon: String,
        required_spec: String,
    },
    /// Rune/sigil/relic name not in game data (all fuzzy passes failed).
    ItemNotFound { item_type: String, name: String },
    /// Gear prefix (stat name) not in itemstats.
    GearPrefixNotFound { name: String },
    /// A specialization resolved with fewer than 3 major traits.
    IncompleteSpecTraits { spec: String, actual: usize },
    /// Heal / 3 utilities / elite bar is missing slots.
    IncompleteSkillBar {
        heal: bool,
        utilities: usize,
        elite: bool,
    },
}

/// Structured validator rejection. `detail` mirrors the prior flat string
/// format and is safe to show in UI (`impl Display`); `code` is the
/// stable discriminator for retry logic.
#[derive(Debug, Clone)]
pub struct ValidationReject {
    pub code: RejectCode,
    pub detail: String,
}

impl std::fmt::Display for ValidationReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Validate a parsed Gemini build response against the GameDb.
/// Always returns a ValidatedBuild, even if there are errors.
// The result is populated incrementally as each validation stage runs; a single
// struct literal would not match the staged, side-effecting validation flow.
#[allow(clippy::field_reassign_with_default)]
/// Resolve a profession from specialization names (Tempest → Elementalist).
pub fn infer_profession_from_spec_names<'a>(
    db: &GameDb,
    spec_names: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let mut spec_ids: Vec<u32> = db.specializations.keys().copied().collect();
    spec_ids.sort_unstable();
    for name in spec_names {
        let clean = name.trim_end_matches(" [E]").trim();
        for sid in &spec_ids {
            if let Some(spec) = db.specializations.get(sid) {
                if spec.name.eq_ignore_ascii_case(clean) {
                    return Some(spec.profession.clone());
                }
            }
        }
    }
    None
}

fn alnum_spaces(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect()
}

fn contains_as_words(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let hay = format!(" {} ", alnum_spaces(haystack));
    let n = format!(" {} ", alnum_spaces(needle).trim());
    hay.contains(&n)
}

/// Scan free text for a spec or profession name ("tempest celestial" → Elementalist).
pub fn infer_profession_from_text(db: &GameDb, text: &str) -> Option<String> {
    let mut specs: Vec<(&str, &str)> = db
        .specializations
        .values()
        .map(|s| (s.name.as_str(), s.profession.as_str()))
        .collect();
    specs.sort_by_key(|(n, _)| std::cmp::Reverse(n.len()));
    for (name, profession) in specs {
        if contains_as_words(text, name) {
            return Some(profession.to_string());
        }
    }
    let mut professions: Vec<&str> = db.professions.values().map(|p| p.name.as_str()).collect();
    professions.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for name in professions {
        if contains_as_words(text, name) {
            return Some(name.to_string());
        }
    }
    None
}

pub fn validate_gemini_build(
    response: &GeminiBuildResponse,
    db: &GameDb,
    profession_name: &str,
) -> ValidatedBuild {
    let inferred_profession = infer_profession_from_spec_names(
        db,
        response.specializations.iter().map(|(n, _)| n.as_str()),
    );
    let profession_name = match db.profession(profession_name) {
        Some(_) => profession_name,
        None => inferred_profession.as_deref().unwrap_or(profession_name),
    };

    let mut result = ValidatedBuild::default();

    // Copy explanation fields
    result.explanation = response.explanation.clone();
    result.synergy_explanation = response
        .synergy_explanation
        .clone()
        .unwrap_or_else(|| response.explanation.clone());

    // Parse structured changes
    result.changes = parse_changes(response);

    // Validate each component
    validate_specializations(response, db, profession_name, &mut result);
    if result.specializations.len() != 3 {
        let actual = result.specializations.len();
        result.errors.push(ValidationReject {
            code: RejectCode::WrongSpecCount {
                expected: 3,
                actual,
            },
            detail: format!("Expected 3 specializations, got {}", actual),
        });
    }
    validate_weapons(response, db, profession_name, &mut result);
    validate_skills(response, db, profession_name, &mut result);
    if !response.specializations.is_empty() {
        for spec in &result.specializations {
            if spec.trait_ids.len() != 3 {
                result.errors.push(ValidationReject {
                    code: RejectCode::IncompleteSpecTraits {
                        spec: spec.name.clone(),
                        actual: spec.trait_ids.len(),
                    },
                    detail: format!(
                        "{}: expected 3 traits, got {}",
                        spec.name,
                        spec.trait_ids.len()
                    ),
                });
            }
        }
        let utils = result
            .skills
            .utilities
            .iter()
            .filter(|u| u.is_some())
            .count();
        if result.skills.heal.is_none() || result.skills.elite.is_none() || utils != 3 {
            result.errors.push(ValidationReject {
                code: RejectCode::IncompleteSkillBar {
                    heal: result.skills.heal.is_some(),
                    utilities: utils,
                    elite: result.skills.elite.is_some(),
                },
                detail: format!(
                    "Need heal, 3 utilities, and elite (heal={}, utils={}, elite={})",
                    result.skills.heal.is_some(),
                    utils,
                    result.skills.elite.is_some()
                ),
            });
        }
    }
    if profession_name == "Revenant" {
        fill_revenant_legends(&mut result, db);
    }
    validate_rune(response, db, &mut result);
    validate_sigils(response, db, &mut result);
    validate_relic(response, db, &mut result);
    validate_gear_prefix(response, db, &mut result);

    result
}

// ---------------------------------------------------------------------------
// Sub-validators
// ---------------------------------------------------------------------------

fn validate_specializations(
    response: &GeminiBuildResponse,
    db: &GameDb,
    profession_name: &str,
    result: &mut ValidatedBuild,
) {
    let prof = db.profession(profession_name);
    let prof_spec_ids: Vec<u32> = prof.map(|p| p.specializations.clone()).unwrap_or_default();

    let mut elite_count = 0;

    for (spec_name, trait_names) in &response.specializations {
        // Strip display-only " [E]" suffix that the LLM or UI may include for elite specs
        let spec_name_clean = spec_name.trim_end_matches(" [E]");
        let spec = find_spec_by_name(db, spec_name_clean, &prof_spec_ids);

        let Some(spec) = spec else {
            result.errors.push(ValidationReject {
                code: RejectCode::SpecNotFound {
                    spec: spec_name.clone(),
                    profession: profession_name.to_string(),
                },
                detail: format!(
                    "Specialization '{}' not found for {}",
                    spec_name, profession_name
                ),
            });
            continue;
        };

        // Check profession ownership
        if spec.profession != profession_name {
            result.errors.push(ValidationReject {
                code: RejectCode::SpecWrongProfession {
                    spec: spec.name.clone(),
                    owner: spec.profession.clone(),
                    expected: profession_name.to_string(),
                },
                detail: format!(
                    "Specialization '{}' belongs to {}, not {}",
                    spec.name, spec.profession, profession_name
                ),
            });
            continue;
        }

        if spec.elite {
            elite_count += 1;
            if elite_count > 1 {
                result.errors.push(ValidationReject {
                    code: RejectCode::MultipleEliteSpecs {
                        spec: spec.name.clone(),
                    },
                    detail: format!("Multiple elite specs selected ({})", spec.name),
                });
            }
        }

        // Resolve traits
        let spec_traits = db.spec_traits(spec.id);
        let major_traits: Vec<&GW2Trait> = spec_traits
            .iter()
            .filter(|t| t.slot == "Major")
            .copied()
            .collect();

        let mut resolved_trait_ids = Vec::new();
        let mut resolved_trait_names = Vec::new();
        let mut used_tiers: HashMap<u32, String> = HashMap::new();

        for trait_name in trait_names {
            if let Some(t) = find_trait_by_name(trait_name, &major_traits) {
                // Check column uniqueness
                if let Some(existing) = used_tiers.get(&t.tier) {
                    result.warnings.push(format!(
                        "Spec '{}': tier {} has '{}' and '{}' — keeping first",
                        spec.name,
                        tier_label(t.tier),
                        existing,
                        t.name
                    ));
                    continue;
                }
                used_tiers.insert(t.tier, t.name.clone());
                resolved_trait_ids.push(t.id);
                resolved_trait_names.push(t.name.clone());
            } else {
                result.warnings.push(format!(
                    "Trait '{}' not found in spec '{}'",
                    trait_name, spec.name
                ));
            }
        }

        complete_major_trait_columns(
            &major_traits,
            &mut resolved_trait_ids,
            &mut resolved_trait_names,
            &mut used_tiers,
            &spec.name,
            &mut result.warnings,
        );

        // Collect minor traits (always active)
        let minor_ids: Vec<u32> = spec.minor_traits.clone();
        let mut all_trait_ids = minor_ids;
        all_trait_ids.extend(&resolved_trait_ids);

        result.specializations.push(ValidatedSpec {
            spec_id: spec.id,
            name: spec.name.clone(),
            elite: spec.elite,
            trait_ids: resolved_trait_ids,
            trait_names: resolved_trait_names,
            all_trait_ids,
        });
    }
}

fn validate_weapons(
    response: &GeminiBuildResponse,
    db: &GameDb,
    profession_name: &str,
    result: &mut ValidatedBuild,
) {
    let prof = db.profession(profession_name);

    // Parse weapon sets from the response
    // The response.weapons is Vec<String> like ["Set 1: Axe / Axe", "Set 2: Greatsword"]
    // or may be empty if using the new format (parsed differently)
    let (set1, set2) = parse_weapon_sets_from_response(response);

    result.weapons.set1 = validate_weapon_set(&set1, prof, db, result, "Set 1");
    result.weapons.set2 = validate_weapon_set(&set2, prof, db, result, "Set 2");
}

fn validate_weapon_set(
    weapons: &(Option<String>, Option<String>),
    prof: Option<&gw2_api::models::Profession>,
    db: &GameDb,
    result: &mut ValidatedBuild,
    label: &str,
) -> ValidatedWeaponSet {
    let mut set = ValidatedWeaponSet::default();

    let Some(prof) = prof else {
        return set;
    };

    // Validate main hand
    if let Some(ref mh) = weapons.0 {
        if let Some((canonical, info)) = find_weapon(mh, prof) {
            if !info.land_usable(&canonical) {
                result.errors.push(ValidationReject {
                    code: RejectCode::WeaponNotAvailable {
                        slot: label.to_string(),
                        weapon: canonical.clone(),
                        profession: prof.name.clone(),
                    },
                    detail: format!(
                        "{}: '{}' is underwater and cannot be a land weapon set",
                        label, canonical
                    ),
                });
            } else {
                // Store canonical name so later `prof.weapons.get(...)` (case-sensitive)
                // hits — preserving the LLM's casing would bypass the elite spec gate.
                set.main_hand = Some(canonical.clone());
            }
        } else {
            result.errors.push(ValidationReject {
                code: RejectCode::WeaponNotAvailable {
                    slot: label.to_string(),
                    weapon: mh.clone(),
                    profession: prof.name.clone(),
                },
                detail: format!("{}: weapon '{}' not available for {}", label, mh, prof.name),
            });
        }
    }

    // Validate off hand
    if let Some(ref oh) = weapons.1 {
        if let Some((canonical, info)) = find_weapon(oh, prof) {
            if !info.land_usable(&canonical) {
                result.errors.push(ValidationReject {
                    code: RejectCode::WeaponNotAvailable {
                        slot: label.to_string(),
                        weapon: canonical.clone(),
                        profession: prof.name.clone(),
                    },
                    detail: format!(
                        "{}: '{}' is underwater and cannot be a land weapon set",
                        label, canonical
                    ),
                });
            } else {
                set.off_hand = Some(canonical.clone());
            }
        } else {
            result.errors.push(ValidationReject {
                code: RejectCode::WeaponNotAvailable {
                    slot: label.to_string(),
                    weapon: oh.clone(),
                    profession: prof.name.clone(),
                },
                detail: format!("{}: weapon '{}' not available for {}", label, oh, prof.name),
            });
        }
    }

    // Check elite spec weapon gates
    let elite_spec_ids: Vec<u32> = result
        .specializations
        .iter()
        .filter(|s| s.elite)
        .map(|s| s.spec_id)
        .collect();

    // Elite spec weapon gate: collect weapons that need a spec not in the build.
    // These are HARD errors — the player cannot equip them without the required spec.
    let mut gated_weapons: Vec<String> = Vec::new();
    for weapon_name in [&set.main_hand, &set.off_hand].into_iter().flatten() {
        if let Some(info) = prof.weapons.get(weapon_name.as_str()) {
            if let Some(required_spec) = info.specialization {
                if !elite_spec_ids.contains(&required_spec) {
                    let spec_name = db
                        .spec(required_spec)
                        .map(|s| s.name.as_str())
                        .unwrap_or("unknown");
                    result.errors.push(ValidationReject {
                        code: RejectCode::WeaponGatedBySpec {
                            slot: label.to_string(),
                            weapon: weapon_name.clone(),
                            required_spec: spec_name.to_string(),
                        },
                        detail: format!(
                            "{}: '{}' requires {} (not equipped) — weapon cannot be used",
                            label, weapon_name, spec_name
                        ),
                    });
                    gated_weapons.push(weapon_name.clone());
                }
            }
        }
    }
    // Remove gated weapons from the validated set so downstream code can't apply them.
    for w in &gated_weapons {
        if set.main_hand.as_deref() == Some(w.as_str()) {
            set.main_hand = None;
        }
        if set.off_hand.as_deref() == Some(w.as_str()) {
            set.off_hand = None;
        }
    }

    set
}

fn validate_skills(
    response: &GeminiBuildResponse,
    db: &GameDb,
    profession_name: &str,
    result: &mut ValidatedBuild,
) {
    // Determine which elite spec (if any) is equipped — used to gate elite spec skills.
    let equipped_elite_spec_id: Option<u32> = result
        .specializations
        .iter()
        .find(|s| s.elite)
        .map(|s| s.spec_id);

    // Filter to only core skills (specialization == None) or skills from the equipped elite spec.
    // This prevents cross-spec skill suggestions from slipping through (e.g. Berserker using
    // a Spellbreaker utility when Spellbreaker is not equipped).
    let all_prof_skills = db.profession_skills(profession_name);
    let prof_skills: Vec<&Skill> = all_prof_skills
        .into_iter()
        .filter(|s| match s.specialization {
            None => true,
            Some(spec_id) => Some(spec_id) == equipped_elite_spec_id,
        })
        .filter(|s| db.skill_palette_id(s.id) != 0)
        .collect();

    // Parse skill names from the response
    let (heal_name, utility_names, elite_name) = parse_skill_names_from_response(response);

    // Validate heal
    if let Some(name) = &heal_name {
        result.skills.heal = find_skill_by_name(name, &prof_skills, Some("Heal"), result);
    }

    // Validate utilities. GW2 has exactly 3 utility slots — cap so an LLM that
    // hallucinates 4+ utilities cannot inflate the build, and dedupe by skill id
    // so a single utility is never equipped in two slots simultaneously.
    let mut seen_utility_ids: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for name in &utility_names {
        if result.skills.utilities.len() >= 3 {
            break;
        }
        let resolved = find_skill_by_name(name, &prof_skills, Some("Utility"), result);
        if let Some((id, ref skill_name)) = resolved {
            if !seen_utility_ids.insert(id) {
                result.warnings.push(format!(
                    "Skill '{}' already equipped — duplicate utility ignored",
                    skill_name
                ));
                continue;
            }
        }
        result.skills.utilities.push(resolved);
    }

    // Validate elite
    if let Some(name) = &elite_name {
        result.skills.elite = find_skill_by_name(name, &prof_skills, Some("Elite"), result);
    }
}

/// Revenant heal/utilities/elite are a legend bundle, not a free mix.
/// `/v2/legends` plus the swap skill's `specialization` gate which stances
/// are legal; the template byte is `Legend.code`.
fn fill_revenant_legends(result: &mut ValidatedBuild, db: &GameDb) {
    if db.legends.is_empty() {
        return;
    }
    let spec_ids: Vec<u32> = result.specializations.iter().map(|s| s.spec_id).collect();
    let mut ids = Vec::new();
    if let Some((heal_id, _)) = &result.skills.heal {
        if let Some(id) = db.legends.iter().find_map(|(id, l)| {
            (l.heal == *heal_id && db.legend_available(id, &spec_ids)).then(|| id.clone())
        }) {
            ids.push(id);
        }
    }
    let mut rest: Vec<(u8, String)> = db
        .legends
        .keys()
        .filter(|id| !ids.contains(id) && db.legend_available(id, &spec_ids))
        .map(|id| (db.legend_template_code(id), id.clone()))
        .collect();
    rest.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, id) in rest {
        if ids.len() >= 2 {
            break;
        }
        ids.push(id);
    }
    if ids.is_empty() {
        return;
    }
    if let Some(legend) = db.legends.get(&ids[0]) {
        let name_of = |id: u32| {
            db.skills
                .get(&id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| format!("Skill {id}"))
        };
        result.skills.heal = Some((legend.heal, name_of(legend.heal)));
        result.skills.utilities = legend
            .utilities
            .iter()
            .take(3)
            .map(|&id| Some((id, name_of(id))))
            .collect();
        result.skills.elite = Some((legend.elite, name_of(legend.elite)));
    }
    result.legends = ids.clone();
    result.aquatic_legends = ids;
}

fn validate_rune(response: &GeminiBuildResponse, db: &GameDb, result: &mut ValidatedBuild) {
    if response.rune.is_empty() {
        result
            .warnings
            .push("Rune field is empty — no rune selected".into());
        return;
    }

    let runes = db.all_runes();
    result.rune = find_item_by_name(&response.rune, &runes, "Rune", result);
}

fn validate_sigils(response: &GeminiBuildResponse, db: &GameDb, result: &mut ValidatedBuild) {
    let sigils_list = db.all_sigils();

    // Handle both old format (flat array) and new format (per-slot map).
    // When `sigils_map` is provided, positions are [set1_main, set1_off, set2_main, set2_off].
    let (sigil_names, positional) = if let Some(ref map) = response.sigils_map {
        (
            vec![
                map.get("set1_main").cloned().unwrap_or_default(),
                map.get("set1_off").cloned().unwrap_or_default(),
                map.get("set2_main").cloned().unwrap_or_default(),
                map.get("set2_off").cloned().unwrap_or_default(),
            ],
            true,
        )
    } else {
        (response.sigils.clone(), false)
    };

    // GW2 forbids duplicate sigils within a single weapon set, but the same
    // sigil is allowed across sets. When positions are known (sigils_map path)
    // we can enforce this per-set. Falling back to "no dedup" for the legacy
    // flat list — that path lacks slot identity so we can't reliably split.
    let mut resolved: Vec<Option<ValidatedItem>> = Vec::with_capacity(sigil_names.len());
    for name in &sigil_names {
        if name.is_empty() {
            resolved.push(None);
            continue;
        }
        resolved.push(find_item_by_name(name, &sigils_list, "Sigil", result));
    }

    if positional && resolved.len() == 4 {
        // Set 1 = indices 0,1 | Set 2 = indices 2,3
        for (set_label, a, b) in [("Set 1", 0usize, 1usize), ("Set 2", 2usize, 3usize)] {
            if let (Some(left), Some(right)) = (&resolved[a], &resolved[b]) {
                if left.id == right.id {
                    result.warnings.push(format!(
                        "{}: duplicate sigil '{}' — GW2 forbids two of the same sigil in one weapon set; \
                         dropping the off-hand slot",
                        set_label, left.name
                    ));
                    resolved[b] = None;
                }
            }
        }
    }

    for item in resolved.into_iter().flatten() {
        result.sigils.push(item);
    }
}

fn validate_relic(response: &GeminiBuildResponse, db: &GameDb, result: &mut ValidatedBuild) {
    if response.relic.is_empty() {
        result
            .warnings
            .push("Relic field is empty — no relic selected".into());
        return;
    }

    let relics = db.all_relics();
    result.relic = find_item_by_name(&response.relic, &relics, "Relic", result);
}

fn validate_gear_prefix(response: &GeminiBuildResponse, db: &GameDb, result: &mut ValidatedBuild) {
    if response.stat_prefix.is_empty() {
        return;
    }

    // Case-insensitive search in itemstats. HashMap::values() iteration order is
    // unspecified, so we collect all candidates and pick deterministically:
    //   1. Exact case-insensitive name match (id tiebreak if multiple).
    //   2. Substring match — prefer the *shortest* name, then lowest id. This
    //      makes "Berserker" pick "Berserker's" over "Marauder's Berserker..."
    //      and survives HashMap reorders across runs.
    let needle = response.stat_prefix.to_lowercase();

    let mut exact: Vec<(u32, &str)> = Vec::new();
    let mut fuzzy: Vec<(usize, u32, &str)> = Vec::new();
    for is in db.itemstats.values() {
        let lower = is.name.to_lowercase();
        if lower == needle {
            exact.push((is.id, is.name.as_str()));
        } else if lower.contains(&needle) {
            fuzzy.push((is.name.len(), is.id, is.name.as_str()));
        }
    }

    let picked = if !exact.is_empty() {
        exact.sort_by_key(|(id, _)| *id);
        let (id, name) = exact[0];
        Some((id, name.to_string(), true))
    } else if !fuzzy.is_empty() {
        fuzzy.sort_by_key(|(len, id, _)| (*len, *id));
        let (_, id, name) = fuzzy[0];
        Some((id, name.to_string(), false))
    } else {
        None
    };

    match picked {
        Some((id, name, is_exact)) => {
            if !is_exact {
                result.warnings.push(format!(
                    "Gear prefix '{}' fuzzy-matched to '{}'",
                    response.stat_prefix, name
                ));
            }
            result.gear_prefix = Some(ValidatedGearPrefix {
                itemstat_id: id,
                name,
            });
        }
        None => {
            result.errors.push(ValidationReject {
                code: RejectCode::GearPrefixNotFound {
                    name: response.stat_prefix.clone(),
                },
                detail: format!("Gear prefix '{}' not found", response.stat_prefix),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/// Find a specialization by name (case-insensitive) within a profession's spec list.
fn find_spec_by_name<'a>(
    db: &'a GameDb,
    name: &str,
    prof_spec_ids: &[u32],
) -> Option<&'a Specialization> {
    let needle = name.to_lowercase();

    // Prefer exact match within profession
    for id in prof_spec_ids {
        if let Some(spec) = db.spec(*id) {
            if spec.name.to_lowercase() == needle {
                return Some(spec);
            }
        }
    }

    // Fallback: case-insensitive contains (spec name contains search string only).
    // Do NOT check the reverse direction — prevents "Ber" matching "Berserker".
    for id in prof_spec_ids {
        if let Some(spec) = db.spec(*id) {
            if spec.name.to_lowercase().contains(&needle) {
                return Some(spec);
            }
        }
    }

    None
}

/// Find a trait by name (case-insensitive) within a spec's major traits.
fn find_trait_by_name<'a>(name: &str, major_traits: &[&'a GW2Trait]) -> Option<&'a GW2Trait> {
    let needle = name.to_lowercase();

    // Exact match
    if let Some(t) = major_traits.iter().find(|t| names_eq(&t.name, name)) {
        return Some(t);
    }

    // Contains match: only check if trait name contains the search needle.
    // Do NOT check the reverse (needle contains trait name) — that causes
    // "Empowered" to match input "Power" or "Swift" to match "Swift Empowerment".
    // Minimum needle length guard: short needles (< 5 chars) over-match on long
    // trait names. A 4-char LLM hallucination like "swif" would otherwise silently
    // match "Swift Retribution". Exact matches (above) are exempt from this guard.
    if needle.len() < 5 {
        return None;
    }
    major_traits
        .iter()
        .find(|t| t.name.to_lowercase().contains(&needle))
        .copied()
}

/// Fill empty Adept/Master/Grandmaster columns so a nearly-complete LLM plate is still legal.
/// Picks the lowest-order major in the missing tier (top row). Orders Adept → Master → Grandmaster.
fn complete_major_trait_columns(
    major_traits: &[&GW2Trait],
    resolved_ids: &mut Vec<u32>,
    resolved_names: &mut Vec<String>,
    used_tiers: &mut HashMap<u32, String>,
    spec_name: &str,
    warnings: &mut Vec<String>,
) {
    for tier in [1u32, 2, 3] {
        if used_tiers.contains_key(&tier) {
            continue;
        }
        let Some(t) = major_traits
            .iter()
            .filter(|t| t.tier == tier)
            .min_by_key(|t| (t.order, t.id))
        else {
            continue;
        };
        used_tiers.insert(tier, t.name.clone());
        resolved_ids.push(t.id);
        resolved_names.push(t.name.clone());
        warnings.push(format!(
            "Spec '{}': filled {} with '{}'",
            spec_name,
            tier_label(tier),
            t.name
        ));
    }
    let mut ordered_ids = Vec::with_capacity(resolved_ids.len());
    let mut ordered_names = Vec::with_capacity(resolved_names.len());
    for tier in [1u32, 2, 3] {
        if let Some(t) = major_traits
            .iter()
            .find(|t| t.tier == tier && resolved_ids.contains(&t.id))
        {
            ordered_ids.push(t.id);
            ordered_names.push(t.name.clone());
        }
    }
    *resolved_ids = ordered_ids;
    *resolved_names = ordered_names;
}

/// Find a skill by name (case-insensitive) and validate its slot type.
fn find_skill_by_name(
    name: &str,
    prof_skills: &[&Skill],
    expected_slot: Option<&str>,
    result: &mut ValidatedBuild,
) -> Option<(u32, String)> {
    let needle = name.to_lowercase();

    // Exact match first. Fall back to substring contains, but only when the
    // needle is at least 5 chars long — otherwise short LLM hallucinations like
    // "heal" or "fire" would over-match skills they never named (e.g. "heal"
    // matching the first heal-tagged skill alphabetically). Matches the same
    // guard used by `find_trait_by_name`.
    let exact_match = prof_skills.iter().find(|s| names_eq(&s.name, name));
    let found = if exact_match.is_some() {
        exact_match
    } else if needle.len() >= 5 {
        prof_skills
            .iter()
            .find(|s| s.name.to_lowercase().contains(&needle))
    } else {
        None
    };

    if let Some(skill) = found {
        // Validate slot type
        if let Some(expected) = expected_slot {
            if let Some(ref slot) = skill.slot {
                if !slot.eq_ignore_ascii_case(expected) {
                    result.warnings.push(format!(
                        "Skill '{}' has slot '{}', expected '{}'",
                        skill.name, slot, expected
                    ));
                }
            }
        }

        let exact = names_eq(&skill.name, name);
        if !exact {
            result.warnings.push(format!(
                "Skill '{}' fuzzy-matched to '{}'",
                name, skill.name
            ));
        }

        Some((skill.id, skill.name.clone()))
    } else {
        result
            .warnings
            .push(format!("Skill '{}' not found for this profession", name));
        None
    }
}

fn names_eq(a: &str, b: &str) -> bool {
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    let ka = gw2_core::i18n::alnum_key(a);
    !ka.is_empty() && ka == gw2_core::i18n::alnum_key(b)
}

/// Find an item (rune/sigil/relic) by name (case-insensitive).
fn find_item_by_name(
    name: &str,
    items: &[&Item],
    item_type: &str,
    result: &mut ValidatedBuild,
) -> Option<ValidatedItem> {
    let needle = name.to_lowercase();

    // Exact match
    let found = items.iter().find(|i| names_eq(&i.name, name));

    if let Some(item) = found {
        return Some(ValidatedItem {
            id: item.id,
            name: item.name.clone(),
        });
    }

    // Item name contains search string
    let found = items.iter().find(|i| {
        let item_lower = i.name.to_lowercase();
        item_lower.contains(&needle)
    });

    if let Some(item) = found {
        result.warnings.push(format!(
            "{} '{}' fuzzy-matched to '{}'",
            item_type, name, item.name
        ));
        return Some(ValidatedItem {
            id: item.id,
            name: item.name.clone(),
        });
    }

    // Last resort: search string contains item name.
    // Require minimum 8 chars AND item name must be >= 50% of search string length
    // to prevent spurious matches on short common words (e.g. "Fire" matching inside
    // a hallucinated long name).
    let found = items.iter().find(|i| {
        let item_lower = i.name.to_lowercase();
        item_lower.len() >= 8
            && needle.contains(&item_lower)
            && item_lower.len() * 2 >= needle.len()
    });

    if let Some(item) = found {
        result.warnings.push(format!(
            "{} '{}' fuzzy-matched to '{}'",
            item_type, name, item.name
        ));
        return Some(ValidatedItem {
            id: item.id,
            name: item.name.clone(),
        });
    }

    // Try stripping "Superior Rune/Sigil of (the) " prefix from the search name
    let stripped = name
        .strip_prefix("Superior Rune of the ")
        .or_else(|| name.strip_prefix("Superior Rune of "))
        .or_else(|| name.strip_prefix("Superior Sigil of the "))
        .or_else(|| name.strip_prefix("Superior Sigil of "))
        .or_else(|| name.strip_prefix("Relic of the "))
        .or_else(|| name.strip_prefix("Relic of "));

    if let Some(short) = stripped {
        let short_lower = short.to_lowercase();
        let found = items
            .iter()
            .find(|i| i.name.to_lowercase().contains(&short_lower));
        if let Some(item) = found {
            result.warnings.push(format!(
                "{} '{}' fuzzy-matched to '{}'",
                item_type, name, item.name
            ));
            return Some(ValidatedItem {
                id: item.id,
                name: item.name.clone(),
            });
        }
    }

    result.errors.push(ValidationReject {
        code: RejectCode::ItemNotFound {
            item_type: item_type.to_string(),
            name: name.to_string(),
        },
        detail: format!("{} '{}' not found in game data", item_type, name),
    });
    None
}

/// Find a weapon type in the profession's weapon list (case-insensitive).
/// Returns the canonical key + WeaponInfo so callers can store the canonical
/// name instead of the LLM-supplied casing. Otherwise a downstream
/// `prof.weapons.get(canonical_key)` (case-sensitive) misses and skips the
/// elite-spec weapon gate.
fn find_weapon<'a>(
    name: &str,
    prof: &'a gw2_api::models::Profession,
) -> Option<(&'a String, &'a gw2_api::models::WeaponInfo)> {
    // Profession/skill: Shortbow. Items: ShortBow. Models: "Short Bow".
    // Items also use Harpoon for profession Spear.
    let needle = gw2_core::i18n::weapon_type_key(name);
    if needle.is_empty() {
        return None;
    }
    prof.weapons
        .iter()
        .find(|(k, _)| gw2_core::i18n::weapon_type_key(k) == needle)
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// A single weapon set as (main-hand, off-hand) names.
type WeaponSlots = (Option<String>, Option<String>);

/// Parse weapon sets from GeminiBuildResponse.
/// Handles both old format ("Set 1: Axe / Axe") and the raw fields.
fn parse_weapon_sets_from_response(response: &GeminiBuildResponse) -> (WeaponSlots, WeaponSlots) {
    let mut set1 = (None, None);
    let mut set2 = (None, None);

    for w in &response.weapons {
        let (label, rest) = if let Some(idx) = w.find(':') {
            (w[..idx].trim(), w[idx + 1..].trim())
        } else {
            ("", w.as_str())
        };

        let parts: Vec<&str> = rest.split('/').map(|s| s.trim()).collect();
        let main = parts
            .first()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let off = parts
            .get(1)
            .filter(|s| !s.is_empty() && *s != &"null" && *s != &"None")
            .map(|s| s.to_string());

        if label.contains('1') || set1.0.is_none() && label.is_empty() {
            set1 = (main, off);
        } else {
            set2 = (main, off);
        }
    }

    (set1, set2)
}

/// Parse skill names from GeminiBuildResponse.
/// Handles both old format ("Heal: Mending", "Utils: Foo, Bar, Baz") and direct fields.
///
/// Label prefix matching is case-insensitive — the LLM occasionally lowercases
/// labels ("heal:") and a case-sensitive strip silently dropped those skills.
fn parse_skill_names_from_response(
    response: &GeminiBuildResponse,
) -> (Option<String>, Vec<String>, Option<String>) {
    fn strip_label_ci<'a>(s: &'a str, label: &str) -> Option<&'a str> {
        // UTF-8 safe via `str::get` — returns None on non-char-boundary indices.
        let head = s.get(..label.len())?;
        if head.eq_ignore_ascii_case(label) {
            Some(&s[label.len()..])
        } else {
            None
        }
    }

    let mut heal = None;
    let mut utilities = Vec::new();
    let mut elite = None;

    for skill_line in &response.skills {
        if let Some(rest) = strip_label_ci(skill_line, "Heal: ") {
            heal = Some(rest.trim().to_string());
        } else if let Some(rest) = strip_label_ci(skill_line, "Utils: ") {
            utilities.extend(rest.split(',').map(|s| s.trim().to_string()));
        } else if let Some(rest) = strip_label_ci(skill_line, "Utility: ") {
            let name = rest.trim();
            if !name.is_empty() {
                utilities.push(name.to_string());
            }
        } else if let Some(rest) = strip_label_ci(skill_line, "Elite: ") {
            elite = Some(rest.trim().to_string());
        }
    }

    (heal, utilities, elite)
}

/// Parse structured changes from the response.
fn parse_changes(response: &GeminiBuildResponse) -> Vec<ChangeEntry> {
    // First try structured changes from the new format
    if let Some(ref changes) = response.changes_structured {
        return changes
            .iter()
            .filter_map(|c| {
                Some(ChangeEntry {
                    slot: c.get("slot")?.as_str()?.to_string(),
                    from: c
                        .get("from")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    to: c
                        .get("to")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    reason: c
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            })
            .collect();
    }

    // Fallback: convert flat string changes to ChangeEntry
    response
        .changes_made
        .iter()
        .map(|s| ChangeEntry {
            slot: String::new(),
            from: String::new(),
            to: String::new(),
            reason: s.clone(),
        })
        .collect()
}

/// Human-readable tier label.
fn tier_label(tier: u32) -> &'static str {
    match tier {
        1 => "Adept",
        2 => "Master",
        3 => "Grandmaster",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    // Test fixtures are built field-by-field for readability.
    #![allow(clippy::field_reassign_with_default)]
    use super::*;

    #[test]
    fn test_parse_weapon_sets_from_response() {
        let mut response = GeminiBuildResponse::default();
        response.weapons = vec!["Set 1: Axe / Axe".into(), "Set 2: Greatsword".into()];
        let (set1, set2) = parse_weapon_sets_from_response(&response);
        assert_eq!(set1.0.as_deref(), Some("Axe"));
        assert_eq!(set1.1.as_deref(), Some("Axe"));
        assert_eq!(set2.0.as_deref(), Some("Greatsword"));
        assert_eq!(set2.1, None);
    }

    #[test]
    fn test_parse_skill_names_from_response() {
        let mut response = GeminiBuildResponse::default();
        response.skills = vec![
            "Heal: Mending".into(),
            "Utils: Signet of Fury, Banner of Strength, Bull's Charge".into(),
            "Elite: Signet of Rage".into(),
        ];
        let (heal, utils, elite) = parse_skill_names_from_response(&response);
        assert_eq!(heal.as_deref(), Some("Mending"));
        assert_eq!(utils.len(), 3);
        assert_eq!(utils[0], "Signet of Fury");
        assert_eq!(elite.as_deref(), Some("Signet of Rage"));
    }

    #[test]
    fn test_parse_skill_names_case_insensitive() {
        // Regression: case-sensitive strip_prefix dropped lowercase labels.
        let mut response = GeminiBuildResponse::default();
        response.skills = vec![
            "heal: Mending".into(),
            "UTILS: Signet of Fury, Banner of Strength".into(),
            "Elite: Signet of Rage".into(),
        ];
        let (heal, utils, elite) = parse_skill_names_from_response(&response);
        assert_eq!(heal.as_deref(), Some("Mending"));
        assert_eq!(utils.len(), 2);
        assert_eq!(elite.as_deref(), Some("Signet of Rage"));
    }

    #[test]
    fn test_parse_changes_structured() {
        let mut response = GeminiBuildResponse::default();
        response.changes_structured = Some(vec![serde_json::json!({
            "slot": "Adept", "from": "Trait A", "to": "Trait B", "reason": "Better synergy"
        })]);
        let changes = parse_changes(&response);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].slot, "Adept");
        assert_eq!(changes[0].to, "Trait B");
    }

    #[test]
    fn test_parse_changes_flat_fallback() {
        let mut response = GeminiBuildResponse::default();
        response.changes_made = vec!["Switched to Axe/Axe for burst".into()];
        let changes = parse_changes(&response);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].reason, "Switched to Axe/Axe for burst");
    }

    #[test]
    fn test_tier_label() {
        assert_eq!(tier_label(1), "Adept");
        assert_eq!(tier_label(2), "Master");
        assert_eq!(tier_label(3), "Grandmaster");
    }

    // ── find_trait_by_name() length guard ────────────────────────────────────

    fn make_trait(id: u32, name: &str) -> GW2Trait {
        GW2Trait {
            id,
            name: name.into(),
            icon: None,
            description: None,
            specialization: 0,
            tier: 1,
            order: 0,
            slot: "Major".into(),
            facts: vec![],
            traited_facts: vec![],
            skills: vec![],
        }
    }

    #[test]
    fn test_find_trait_short_needle_no_contains_match() {
        // needle "swif" (4 chars) is a substring of "Swift Retribution".
        // Exact match fails ("swift retribution" != "swif").
        // Length guard (< 5) must block the contains fallback → None.
        let t = make_trait(1, "Swift Retribution");
        let traits = vec![&t];
        let result = find_trait_by_name("swif", &traits);
        assert!(
            result.is_none(),
            "4-char needle must not match via contains fallback"
        );
    }

    #[test]
    fn test_find_trait_needle_ge5_contains_match() {
        // needle "valor" (5 chars) is a substring of "Valorous Recovery".
        // Exact match fails ("valorous recovery" != "valor").
        // Length guard passes (5 >= 5) → contains fires → Some.
        let t = make_trait(2, "Valorous Recovery");
        let traits = vec![&t];
        let result = find_trait_by_name("valor", &traits);
        assert!(
            result.is_some(),
            "5-char needle must match via contains fallback"
        );
        assert_eq!(result.unwrap().id, 2);
    }

    fn arcane_ele_db() -> GameDb {
        let mut db = empty_db_with_itemstats(vec![]);
        db.professions.insert(
            "Elementalist".into(),
            gw2_api::models::Profession {
                id: "Elementalist".into(),
                name: "Elementalist".into(),
                code: None,
                specializations: vec![41],
                weapons: std::collections::HashMap::new(),
                training: vec![],
                skills_by_palette: vec![],
                icon: None,
                icon_big: None,
            },
        );
        db.specializations.insert(
            41,
            Specialization {
                id: 41,
                name: "Arcane".into(),
                profession: "Elementalist".into(),
                elite: false,
                minor_traits: vec![],
                major_traits: vec![1, 2, 3],
                weapon_trait: None,
                icon: None,
                background: None,
                profession_icon: None,
                profession_icon_big: None,
            },
        );
        let mut t1 = make_trait(1, "Arcane Precision");
        t1.tier = 1;
        t1.specialization = 41;
        let mut t2 = make_trait(2, "Arcane Resurrection");
        t2.tier = 2;
        t2.specialization = 41;
        let mut t3 = make_trait(3, "Evasive Arcana");
        t3.tier = 3;
        t3.specialization = 41;
        db.traits.insert(1, t1);
        db.traits.insert(2, t2);
        db.traits.insert(3, t3);
        db.traits_by_spec.insert(41, vec![1, 2, 3]);
        db
    }

    #[test]
    fn validate_gemini_build_fills_missing_arcane_trait_column() {
        // Choya named Arcane but only two traits (the live "got 2" reject).
        // Fill Adept from game data so the plate is legal.
        let db = arcane_ele_db();
        let response = GeminiBuildResponse {
            specializations: vec![(
                "Arcane".into(),
                vec!["Arcane Resurrection".into(), "Evasive Arcana".into()],
            )],
            ..Default::default()
        };
        let result = validate_gemini_build(&response, &db, "Elementalist");
        assert_eq!(result.specializations[0].trait_ids, vec![1, 2, 3]);
        assert_eq!(
            result.specializations[0].trait_names,
            vec![
                "Arcane Precision".to_string(),
                "Arcane Resurrection".to_string(),
                "Evasive Arcana".to_string()
            ]
        );
        assert!(
            !result
                .errors
                .iter()
                .any(|e| matches!(e.code, RejectCode::IncompleteSpecTraits { .. })),
            "filled plate must not reject traits: {:?}",
            result.errors
        );
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("Arcane Precision")));
    }

    // ── validate_gear_prefix() determinism + tie-break ───────────────────────

    fn empty_db_with_itemstats(stats: Vec<(u32, &str)>) -> GameDb {
        let mut itemstats = std::collections::HashMap::new();
        for (id, name) in stats {
            itemstats.insert(
                id,
                gw2_api::models::itemstats::ItemStat {
                    id,
                    name: name.into(),
                    attributes: vec![],
                },
            );
        }
        GameDb {
            items: std::collections::HashMap::new(),
            itemstats,
            skills: std::collections::HashMap::new(),
            traits: std::collections::HashMap::new(),
            specializations: std::collections::HashMap::new(),
            professions: std::collections::HashMap::new(),
            legends: std::collections::HashMap::new(),
            pvp_amulets: std::collections::HashMap::new(),
            skills_by_profession: std::collections::HashMap::new(),
            traits_by_spec: std::collections::HashMap::new(),
            items_by_type: std::collections::HashMap::new(),
            runes: vec![],
            sigils: vec![],
            relics: vec![],
            skill_to_palette: std::collections::HashMap::new(),
            palette_to_skill: std::collections::HashMap::new(),
            traits_by_condition: std::collections::HashMap::new(),
            skills_by_condition: std::collections::HashMap::new(),
            traits_by_buff: std::collections::HashMap::new(),
            skills_by_buff: std::collections::HashMap::new(),
            localized: None,
        }
    }

    fn run_validate_gear_prefix(prefix: &str, db: &GameDb) -> ValidatedBuild {
        let mut response = GeminiBuildResponse::default();
        response.stat_prefix = prefix.into();
        let mut result = ValidatedBuild::default();
        validate_gear_prefix(&response, db, &mut result);
        result
    }

    #[test]
    fn test_validate_gear_prefix_exact_match_beats_substring() {
        // Two candidates contain "berserker"; exact match must win regardless of insertion order.
        let db = empty_db_with_itemstats(vec![
            (100, "Marauder's Berserker Combo"),
            (101, "Berserker's"),
        ]);
        let result = run_validate_gear_prefix("Berserker's", &db);
        let p = result.gear_prefix.expect("should match");
        assert_eq!(p.itemstat_id, 101);
        assert_eq!(p.name, "Berserker's");
        assert!(
            result.warnings.is_empty(),
            "exact match must not emit fuzzy warning"
        );
    }

    #[test]
    fn test_validate_gear_prefix_fuzzy_prefers_shortest_name() {
        // "Viper" substring matches multiple. Tie-break: shortest name wins.
        // This is the determinism fix: HashMap iteration order would otherwise
        // make this test flaky depending on hasher seed.
        let db = empty_db_with_itemstats(vec![
            (200, "Carrion-Viper Hybrid Marauder Combo"),
            (201, "Viper's"),
            (202, "Trailblazer's Viper Combo"),
        ]);
        for _ in 0..10 {
            let result = run_validate_gear_prefix("Viper", &db);
            let p = result.gear_prefix.expect("should fuzzy match");
            assert_eq!(p.itemstat_id, 201, "shortest name must always win");
            assert_eq!(p.name, "Viper's");
        }
        let result = run_validate_gear_prefix("Viper", &db);
        assert_eq!(result.warnings.len(), 1, "fuzzy match must warn once");
    }

    #[test]
    fn test_validate_gear_prefix_fuzzy_id_tiebreak_when_lengths_equal() {
        // Two equal-length names both contain needle. Lower id wins deterministically.
        let db = empty_db_with_itemstats(vec![(350, "Zerk Sample A"), (300, "Zerk Sample B")]);
        let result = run_validate_gear_prefix("Sample", &db);
        let p = result.gear_prefix.expect("should match");
        assert_eq!(p.itemstat_id, 300, "lower id must win equal-length tie");
    }

    #[test]
    fn test_validate_gear_prefix_not_found_emits_error() {
        let db = empty_db_with_itemstats(vec![(400, "Berserker's")]);
        let result = run_validate_gear_prefix("Nonexistent", &db);
        assert!(result.gear_prefix.is_none());
        assert_eq!(result.errors.len(), 1);
        assert!(matches!(
            result.errors[0].code,
            RejectCode::GearPrefixNotFound { .. }
        ));
    }

    // ── find_skill_by_name() needle-length guard ─────────────────────────────

    fn make_skill(id: u32, name: &str) -> gw2_api::models::Skill {
        gw2_api::models::Skill {
            id,
            name: name.into(),
            description: None,
            icon: None,
            chat_link: None,
            skill_type: None,
            weapon_type: None,
            professions: vec![],
            slot: Some("Utility".into()),
            facts: vec![],
            traited_facts: vec![],
            categories: vec![],
            attunement: None,
            cost: None,
            dual_wield: None,
            flip_skill: None,
            initiative: None,
            next_chain: None,
            prev_chain: None,
            transform_skills: vec![],
            bundle_skills: vec![],
            toolbelt_skill: None,
            flags: vec![],
            specialization: None,
        }
    }

    #[test]
    fn test_find_skill_short_needle_no_contains_match() {
        // needle "heal" (4 chars) is a substring of "Healing Spring".
        // Length guard (< 5) must block the contains fallback → None.
        let s = make_skill(1, "Healing Spring");
        let skills = vec![&s];
        let mut result = ValidatedBuild::default();
        let found = find_skill_by_name("heal", &skills, None, &mut result);
        assert!(
            found.is_none(),
            "4-char needle must not match via contains fallback"
        );
    }

    #[test]
    fn test_find_skill_ge5_needle_contains_match() {
        // needle "heali" (5 chars) is a substring of "Healing Spring".
        let s = make_skill(2, "Healing Spring");
        let skills = vec![&s];
        let mut result = ValidatedBuild::default();
        let found = find_skill_by_name("heali", &skills, None, &mut result);
        assert_eq!(found.map(|(id, _)| id), Some(2));
    }

    fn make_prof_with_elite_axe() -> gw2_api::models::Profession {
        let mut weapons = std::collections::HashMap::new();
        weapons.insert(
            "Axe".to_string(),
            gw2_api::models::WeaponInfo {
                specialization: Some(99), // requires elite spec 99
                flags: vec!["Mainhand".into()],
                skills: vec![],
            },
        );
        gw2_api::models::Profession {
            id: "Guardian".into(),
            name: "Guardian".into(),
            code: None,
            specializations: vec![],
            weapons,
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        }
    }

    #[test]
    fn test_validate_weapon_set_lowercase_input_still_triggers_elite_gate() {
        // Regression: find_weapon used to be case-insensitive but stored the
        // LLM's input casing. Then prof.weapons.get(weapon_name) was case-
        // sensitive and missed, silently bypassing the elite-spec weapon gate.
        let prof = make_prof_with_elite_axe();
        let weapons = (Some("axe".to_string()), None);
        let db = empty_db_with_itemstats(vec![]);
        let mut result = ValidatedBuild::default();
        let set = validate_weapon_set(&weapons, Some(&prof), &db, &mut result, "Set 1");
        // Weapon must be removed because no elite spec is equipped
        assert!(
            set.main_hand.is_none(),
            "gated axe should be removed; got {:?}",
            set.main_hand
        );
        assert!(
            result.errors.iter().any(|e| matches!(
                &e.code,
                RejectCode::WeaponGatedBySpec { weapon, .. } if weapon.eq_ignore_ascii_case("Axe")
            )),
            "expected WeaponGatedBySpec error; got {:?}",
            result.errors
        );
    }

    #[test]
    fn test_find_weapon_ignores_spaces() {
        let mut weapons = std::collections::HashMap::new();
        weapons.insert(
            "Shortbow".to_string(),
            gw2_api::models::WeaponInfo {
                specialization: None,
                flags: vec!["TwoHand".into()],
                skills: vec![],
            },
        );
        let prof = gw2_api::models::Profession {
            id: "Thief".into(),
            name: "Thief".into(),
            code: None,
            specializations: vec![],
            weapons,
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        };
        let (key, _) = find_weapon("Short Bow", &prof).expect("Short Bow should match Shortbow");
        assert_eq!(key, "Shortbow");
        assert!(find_weapon("shortbow", &prof).is_some());
        assert_eq!(
            find_weapon("ShortBow", &prof).map(|(k, _)| k.as_str()),
            Some("Shortbow")
        );

        let db = empty_db_with_itemstats(vec![]);
        let mut result = ValidatedBuild::default();
        let set = validate_weapon_set(
            &(Some("Short Bow".into()), None),
            Some(&prof),
            &db,
            &mut result,
            "Set 2",
        );
        assert_eq!(set.main_hand.as_deref(), Some("Shortbow"));
        assert!(
            result.errors.is_empty(),
            "spaced Short Bow should not reject; got {:?}",
            result.errors
        );
    }

    #[test]
    fn test_find_weapon_item_api_aliases() {
        let mut weapons = std::collections::HashMap::new();
        weapons.insert(
            "Spear".to_string(),
            gw2_api::models::WeaponInfo {
                specialization: None,
                flags: vec!["TwoHand".into()],
                skills: vec![],
            },
        );
        let prof = gw2_api::models::Profession {
            id: "Thief".into(),
            name: "Thief".into(),
            code: None,
            specializations: vec![],
            weapons,
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        };
        assert_eq!(
            find_weapon("Harpoon", &prof).map(|(k, _)| k.as_str()),
            Some("Spear")
        );
    }

    #[test]
    fn test_validate_weapon_set_rejects_aquatic_trident_on_land() {
        let mut weapons = std::collections::HashMap::new();
        weapons.insert(
            "Trident".to_string(),
            gw2_api::models::WeaponInfo {
                specialization: None,
                flags: vec!["TwoHand".into(), "Aquatic".into()],
                skills: vec![],
            },
        );
        weapons.insert(
            "Staff".to_string(),
            gw2_api::models::WeaponInfo {
                specialization: None,
                flags: vec!["TwoHand".into()],
                skills: vec![],
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
        let db = empty_db_with_itemstats(vec![]);
        let mut result = ValidatedBuild::default();
        let set = validate_weapon_set(
            &(Some("Trident".into()), None),
            Some(&prof),
            &db,
            &mut result,
            "Set 2",
        );
        assert!(
            set.main_hand.is_none(),
            "trident must not survive as a land set; got {:?}",
            set.main_hand
        );
        assert!(
            result.errors.iter().any(|e| matches!(
                &e.code,
                RejectCode::WeaponNotAvailable { weapon, .. } if weapon == "Trident"
            )),
            "expected WeaponNotAvailable for Trident; got {:?}",
            result.errors
        );

        let mut ok = ValidatedBuild::default();
        let staff = validate_weapon_set(
            &(Some("Staff".into()), None),
            Some(&prof),
            &db,
            &mut ok,
            "Set 2",
        );
        assert_eq!(staff.main_hand.as_deref(), Some("Staff"));
        assert!(ok.errors.is_empty());
    }

    #[test]
    fn test_validate_weapon_set_accepts_land_spear_with_aquatic_flag() {
        let mut weapons = std::collections::HashMap::new();
        weapons.insert(
            "Spear".to_string(),
            gw2_api::models::WeaponInfo {
                specialization: None,
                flags: vec!["TwoHand".into(), "Aquatic".into()],
                skills: vec![],
            },
        );
        let prof = gw2_api::models::Profession {
            id: "Thief".into(),
            name: "Thief".into(),
            code: None,
            specializations: vec![],
            weapons,
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        };
        let db = empty_db_with_itemstats(vec![]);
        let mut result = ValidatedBuild::default();
        let set = validate_weapon_set(
            &(Some("Spear".into()), None),
            Some(&prof),
            &db,
            &mut result,
            "Set 1",
        );
        assert_eq!(set.main_hand.as_deref(), Some("Spear"));
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }

    #[test]
    fn test_find_skill_exact_match_bypasses_guard() {
        // Exact match for a 4-char skill name must still succeed.
        let s = make_skill(3, "Bolt");
        let skills = vec![&s];
        let mut result = ValidatedBuild::default();
        let found = find_skill_by_name("Bolt", &skills, None, &mut result);
        assert_eq!(found.map(|(id, _)| id), Some(3));
    }

    #[test]
    fn test_validate_gear_prefix_empty_input_is_noop() {
        let db = empty_db_with_itemstats(vec![(500, "Berserker's")]);
        let result = run_validate_gear_prefix("", &db);
        assert!(result.gear_prefix.is_none());
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_parse_skill_names_utility_prefix() {
        let mut response = GeminiBuildResponse::default();
        response.skills = vec![
            "Heal: Mending".into(),
            "Utility: Signet of Fury".into(),
            "utility: Banner of Strength".into(),
            "Elite: Signet of Rage".into(),
        ];
        let (heal, utils, elite) = parse_skill_names_from_response(&response);
        assert_eq!(heal.as_deref(), Some("Mending"));
        assert_eq!(utils, vec!["Signet of Fury", "Banner of Strength"]);
        assert_eq!(elite.as_deref(), Some("Signet of Rage"));
    }

    fn ele_db_with_tempest() -> GameDb {
        let mut db = GameDb::empty_for_tests();
        db.professions.insert(
            "Elementalist".into(),
            gw2_api::models::Profession {
                id: "Elementalist".into(),
                name: "Elementalist".into(),
                code: Some(6),
                specializations: vec![48, 17, 41],
                weapons: HashMap::new(),
                training: vec![],
                skills_by_palette: vec![],
                icon: None,
                icon_big: None,
            },
        );
        for (id, name, elite) in [
            (48u32, "Tempest", true),
            (17, "Water", false),
            (41, "Arcane", false),
        ] {
            db.specializations.insert(
                id,
                Specialization {
                    id,
                    name: name.into(),
                    profession: "Elementalist".into(),
                    elite,
                    minor_traits: vec![],
                    major_traits: vec![],
                    weapon_trait: None,
                    icon: None,
                    background: None,
                    profession_icon: None,
                    profession_icon_big: None,
                },
            );
        }
        db
    }

    #[test]
    fn test_unknown_profession_infers_elementalist_from_tempest() {
        let db = ele_db_with_tempest();
        assert_eq!(
            infer_profession_from_text(&db, "tempest celestial support").as_deref(),
            Some("Elementalist")
        );
        assert_eq!(
            infer_profession_from_spec_names(&db, ["Tempest", "Water", "Arcane"]).as_deref(),
            Some("Elementalist")
        );

        let mut response = GeminiBuildResponse::default();
        response.specializations = vec![
            ("Tempest".into(), vec![]),
            ("Water".into(), vec![]),
            ("Arcane".into(), vec![]),
        ];
        let result = validate_gemini_build(&response, &db, "unknown");
        let names: Vec<&str> = result
            .specializations
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["Tempest", "Water", "Arcane"]);
        assert!(
            !result.errors.iter().any(|e| e.detail.contains("unknown")),
            "{result:?}"
        );
    }
}
