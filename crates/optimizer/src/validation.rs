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
    pub rune: Option<ValidatedItem>,
    pub sigils: Vec<ValidatedItem>,
    pub relic: Option<ValidatedItem>,
    pub gear_prefix: Option<ValidatedGearPrefix>,
    pub explanation: String,
    pub synergy_explanation: String,
    pub changes: Vec<ChangeEntry>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
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

#[derive(Debug, Clone)]
pub struct ValidatedItem {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
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

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Validate a parsed Gemini build response against the GameDb.
/// Always returns a ValidatedBuild, even if there are errors.
pub fn validate_gemini_build(
    response: &GeminiBuildResponse,
    db: &GameDb,
    profession_name: &str,
) -> ValidatedBuild {
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
    validate_weapons(response, db, profession_name, &mut result);
    validate_skills(response, db, profession_name, &mut result);
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
    let prof_spec_ids: Vec<u32> = prof
        .map(|p| p.specializations.clone())
        .unwrap_or_default();

    let mut elite_count = 0;

    for (spec_name, trait_names) in &response.specializations {
        // Case-insensitive spec lookup
        let spec = find_spec_by_name(db, spec_name, &prof_spec_ids);

        let Some(spec) = spec else {
            result
                .errors
                .push(format!("Specialization '{}' not found for {}", spec_name, profession_name));
            continue;
        };

        // Check profession ownership
        if spec.profession != profession_name {
            result.errors.push(format!(
                "Specialization '{}' belongs to {}, not {}",
                spec.name, spec.profession, profession_name
            ));
            continue;
        }

        if spec.elite {
            elite_count += 1;
            if elite_count > 1 {
                result
                    .errors
                    .push(format!("Multiple elite specs selected ({})", spec.name));
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
                        spec.name, tier_label(t.tier), existing, t.name
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
        if let Some(_info) = find_weapon(mh, prof) {
            set.main_hand = Some(mh.clone());
        } else {
            result
                .warnings
                .push(format!("{}: weapon '{}' not available for {}", label, mh, prof.name));
        }
    }

    // Validate off hand
    if let Some(ref oh) = weapons.1 {
        if let Some(_info) = find_weapon(oh, prof) {
            set.off_hand = Some(oh.clone());
        } else {
            result
                .warnings
                .push(format!("{}: weapon '{}' not available for {}", label, oh, prof.name));
        }
    }

    // Check elite spec weapon gates
    let elite_spec_ids: Vec<u32> = result
        .specializations
        .iter()
        .filter(|s| s.elite)
        .map(|s| s.spec_id)
        .collect();

    for weapon_name in [&set.main_hand, &set.off_hand].into_iter().flatten() {
        if let Some(info) = prof.weapons.get(weapon_name.as_str()) {
            if let Some(required_spec) = info.specialization {
                if !elite_spec_ids.contains(&required_spec) {
                    let spec_name = db
                        .spec(required_spec)
                        .map(|s| s.name.as_str())
                        .unwrap_or("unknown");
                    result.warnings.push(format!(
                        "{}: '{}' requires {} but it's not in the build",
                        label, weapon_name, spec_name
                    ));
                }
            }
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
    let prof_skills = db.profession_skills(profession_name);

    // Parse skill names from the response
    let (heal_name, utility_names, elite_name) = parse_skill_names_from_response(response);

    // Validate heal
    if let Some(name) = &heal_name {
        result.skills.heal = find_skill_by_name(name, &prof_skills, Some("Heal"), result);
    }

    // Validate utilities
    for name in &utility_names {
        let resolved = find_skill_by_name(name, &prof_skills, Some("Utility"), result);
        result.skills.utilities.push(resolved);
    }

    // Validate elite
    if let Some(name) = &elite_name {
        result.skills.elite = find_skill_by_name(name, &prof_skills, Some("Elite"), result);
    }
}

fn validate_rune(response: &GeminiBuildResponse, db: &GameDb, result: &mut ValidatedBuild) {
    if response.rune.is_empty() {
        return;
    }

    let runes = db.all_runes();
    result.rune = find_item_by_name(&response.rune, &runes, "Rune", result);
}

fn validate_sigils(response: &GeminiBuildResponse, db: &GameDb, result: &mut ValidatedBuild) {
    let sigils_list = db.all_sigils();

    // Handle both old format (flat array) and new format (per-slot map)
    let sigil_names = if let Some(ref map) = response.sigils_map {
        vec![
            map.get("set1_main").cloned().unwrap_or_default(),
            map.get("set1_off").cloned().unwrap_or_default(),
            map.get("set2_main").cloned().unwrap_or_default(),
            map.get("set2_off").cloned().unwrap_or_default(),
        ]
    } else {
        response.sigils.clone()
    };

    for name in &sigil_names {
        if name.is_empty() {
            continue;
        }
        if let Some(item) = find_item_by_name(name, &sigils_list, "Sigil", result) {
            result.sigils.push(item);
        }
    }
}

fn validate_relic(response: &GeminiBuildResponse, db: &GameDb, result: &mut ValidatedBuild) {
    if response.relic.is_empty() {
        return;
    }

    let relics = db.all_relics();
    result.relic = find_item_by_name(&response.relic, &relics, "Relic", result);
}

fn validate_gear_prefix(
    response: &GeminiBuildResponse,
    db: &GameDb,
    result: &mut ValidatedBuild,
) {
    if response.stat_prefix.is_empty() {
        return;
    }

    // Case-insensitive search in itemstats
    let needle = response.stat_prefix.to_lowercase();
    let found = db.itemstats.values().find(|is| {
        is.name.to_lowercase() == needle || is.name.to_lowercase().contains(&needle)
    });

    if let Some(is) = found {
        let exact = is.name.to_lowercase() == needle;
        if !exact {
            result.warnings.push(format!(
                "Gear prefix '{}' fuzzy-matched to '{}'",
                response.stat_prefix, is.name
            ));
        }
        result.gear_prefix = Some(ValidatedGearPrefix {
            itemstat_id: is.id,
            name: is.name.clone(),
        });
    } else {
        result
            .errors
            .push(format!("Gear prefix '{}' not found", response.stat_prefix));
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

    // Fallback: case-insensitive contains
    for id in prof_spec_ids {
        if let Some(spec) = db.spec(*id) {
            if spec.name.to_lowercase().contains(&needle)
                || needle.contains(&spec.name.to_lowercase())
            {
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
    if let Some(t) = major_traits
        .iter()
        .find(|t| t.name.to_lowercase() == needle)
    {
        return Some(t);
    }

    // Contains match
    major_traits
        .iter()
        .find(|t| {
            t.name.to_lowercase().contains(&needle) || needle.contains(&t.name.to_lowercase())
        })
        .copied()
}

/// Find a skill by name (case-insensitive) and validate its slot type.
fn find_skill_by_name(
    name: &str,
    prof_skills: &[&Skill],
    expected_slot: Option<&str>,
    result: &mut ValidatedBuild,
) -> Option<(u32, String)> {
    let needle = name.to_lowercase();

    // Exact match
    let found = prof_skills
        .iter()
        .find(|s| s.name.to_lowercase() == needle)
        .or_else(|| {
            prof_skills
                .iter()
                .find(|s| s.name.to_lowercase().contains(&needle))
        });

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

        let exact = skill.name.to_lowercase() == needle;
        if !exact {
            result.warnings.push(format!(
                "Skill '{}' fuzzy-matched to '{}'",
                name, skill.name
            ));
        }

        Some((skill.id, skill.name.clone()))
    } else {
        result.warnings.push(format!(
            "Skill '{}' not found for this profession",
            name
        ));
        None
    }
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
    let found = items.iter().find(|i| i.name.to_lowercase() == needle);

    if let Some(item) = found {
        return Some(ValidatedItem {
            id: item.id,
            name: item.name.clone(),
        });
    }

    // Contains match
    let found = items.iter().find(|i| {
        let item_lower = i.name.to_lowercase();
        item_lower.contains(&needle) || needle.contains(&item_lower)
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

    result
        .errors
        .push(format!("{} '{}' not found in game data", item_type, name));
    None
}

/// Find a weapon type in the profession's weapon list (case-insensitive).
fn find_weapon<'a>(
    name: &str,
    prof: &'a gw2_api::models::Profession,
) -> Option<&'a gw2_api::models::WeaponInfo> {
    let needle = name.to_lowercase();
    prof.weapons
        .iter()
        .find(|(k, _)| k.to_lowercase() == needle)
        .map(|(_, v)| v)
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse weapon sets from GeminiBuildResponse.
/// Handles both old format ("Set 1: Axe / Axe") and the raw fields.
fn parse_weapon_sets_from_response(
    response: &GeminiBuildResponse,
) -> ((Option<String>, Option<String>), (Option<String>, Option<String>)) {
    let mut set1 = (None, None);
    let mut set2 = (None, None);

    for w in &response.weapons {
        let (label, rest) = if let Some(idx) = w.find(':') {
            (w[..idx].trim(), w[idx + 1..].trim())
        } else {
            ("", w.as_str())
        };

        let parts: Vec<&str> = rest.split('/').map(|s| s.trim()).collect();
        let main = parts.first().filter(|s| !s.is_empty()).map(|s| s.to_string());
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
fn parse_skill_names_from_response(
    response: &GeminiBuildResponse,
) -> (Option<String>, Vec<String>, Option<String>) {
    let mut heal = None;
    let mut utilities = Vec::new();
    let mut elite = None;

    for skill_line in &response.skills {
        if let Some(rest) = skill_line.strip_prefix("Heal: ") {
            heal = Some(rest.trim().to_string());
        } else if let Some(rest) = skill_line.strip_prefix("Utils: ") {
            utilities.extend(rest.split(',').map(|s| s.trim().to_string()));
        } else if let Some(rest) = skill_line.strip_prefix("Elite: ") {
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
    use super::*;

    #[test]
    fn test_parse_weapon_sets_from_response() {
        let mut response = GeminiBuildResponse::default();
        response.weapons = vec![
            "Set 1: Axe / Axe".into(),
            "Set 2: Greatsword".into(),
        ];
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
    fn test_parse_changes_structured() {
        let mut response = GeminiBuildResponse::default();
        response.changes_structured = Some(vec![
            serde_json::json!({
                "slot": "Adept", "from": "Trait A", "to": "Trait B", "reason": "Better synergy"
            }),
        ]);
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
}
