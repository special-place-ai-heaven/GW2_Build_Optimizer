//! Build diff computation: compares ResolvedBuild (current) vs BuildSuggestion (optimized).
//! Produces a structured `BuildDiff` with per-slot change status for rendering.

use gw2_core::types::ResolvedBuild;

use super::comparison::BuildSuggestion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Unchanged,
    Changed,
}

#[derive(Debug, Clone)]
pub struct SlotDiff {
    pub slot_label: String,
    pub current_value: String,
    pub proposed_value: String,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone)]
pub struct BuildDiff {
    pub gear_prefix: SlotDiff,
    /// (spec_name_diff, traits_diff) per slot
    pub specializations: Vec<(SlotDiff, SlotDiff)>,
    /// heal, util1, util2, util3, elite
    pub skills: Vec<SlotDiff>,
    /// (weapons_diff, sigils_diff) per set
    pub weapon_sets: Vec<(SlotDiff, SlotDiff)>,
    pub rune: SlotDiff,
    pub relic: SlotDiff,
}

/// Normalize a name for comparison: lowercase, strip common prefixes.
fn normalize(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("Superior Rune of the ").unwrap_or(s);
    let s = s.strip_prefix("Superior Rune of ").unwrap_or(s);
    let s = s.strip_prefix("Superior Sigil of the ").unwrap_or(s);
    let s = s.strip_prefix("Superior Sigil of ").unwrap_or(s);
    let s = s.strip_prefix("Relic of the ").unwrap_or(s);
    let s = s.strip_prefix("Relic of ").unwrap_or(s);
    s.to_lowercase()
}

fn diff_slot(label: &str, current: &str, proposed: &str) -> SlotDiff {
    let status = if normalize(current) == normalize(proposed) {
        ChangeStatus::Unchanged
    } else {
        ChangeStatus::Changed
    };
    SlotDiff {
        slot_label: label.to_string(),
        current_value: current.to_string(),
        proposed_value: proposed.to_string(),
        status,
    }
}

/// Parse suggestion skill strings: "Heal: X", "Utility: X", "Elite: X"
fn parse_suggestion_skills(skills: &[String]) -> (String, Vec<String>, String) {
    let mut heal = String::new();
    let mut utils = Vec::new();
    let mut elite = String::new();
    for s in skills {
        if let Some(name) = s.strip_prefix("Heal: ") {
            heal = name.trim().to_string();
        } else if let Some(name) = s.strip_prefix("Utility: ") {
            utils.push(name.trim().to_string());
        } else if let Some(name) = s.strip_prefix("Elite: ") {
            elite = name.trim().to_string();
        } else {
            // Unknown format — treat as utility
            utils.push(s.trim().to_string());
        }
    }
    (heal, utils, elite)
}

/// Parse suggestion weapon strings: "Set 1: Sword / Shield", "Set 2: Rifle"
/// Returns Vec of (weapon_summary_string, label).
fn parse_suggestion_weapons(weapons: &[String]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for w in weapons {
        if let Some(rest) = w.strip_prefix("Set 1: ") {
            result.push(("Set 1".to_string(), rest.trim().to_string()));
        } else if let Some(rest) = w.strip_prefix("Set 2: ") {
            result.push(("Set 2".to_string(), rest.trim().to_string()));
        } else {
            result.push(("Weapons".to_string(), w.trim().to_string()));
        }
    }
    result
}

pub fn compute_build_diff(current: &ResolvedBuild, suggestion: &BuildSuggestion) -> BuildDiff {
    // --- Gear Prefix ---
    let current_prefixes: Vec<&str> = current
        .armor
        .iter()
        .map(|a| a.stat_prefix.as_str())
        .filter(|p| !p.is_empty())
        .collect();
    let unique_prefixes: Vec<&str> = {
        let mut v = current_prefixes.clone();
        v.sort();
        v.dedup();
        v
    };
    let current_prefix_str = if unique_prefixes.len() <= 1 {
        unique_prefixes
            .first()
            .copied()
            .unwrap_or("(none)")
            .to_string()
    } else {
        format!("Mixed ({})", unique_prefixes.join(", "))
    };
    let gear_prefix = diff_slot("Gear Prefix", &current_prefix_str, &suggestion.stat_prefix);

    // --- Specializations ---
    let mut specializations = Vec::new();
    let max_specs = current
        .specializations
        .len()
        .max(suggestion.specializations.len());
    for i in 0..max_specs {
        let (cur_name, cur_traits_str) = if let Some(spec) = current.specializations.get(i) {
            let elite_tag = if spec.elite { " [E]" } else { "" };
            let name = format!("{}{}", spec.name, elite_tag);
            let traits: Vec<&str> = spec
                .traits_selected
                .iter()
                .map(|t| t.name.as_str())
                .collect();
            (name, traits.join(" | "))
        } else {
            ("(empty)".to_string(), String::new())
        };

        let (sug_name, sug_traits_str) =
            if let Some((name, traits)) = suggestion.specializations.get(i) {
                (name.clone(), traits.join(" | "))
            } else {
                ("(empty)".to_string(), String::new())
            };

        let slot_label = format!("Spec {}", i + 1);
        let spec_diff = diff_slot(&slot_label, &cur_name, &sug_name);
        let trait_diff = diff_slot("  Traits", &cur_traits_str, &sug_traits_str);
        specializations.push((spec_diff, trait_diff));
    }

    // --- Skills ---
    let (sug_heal, sug_utils, sug_elite) = parse_suggestion_skills(&suggestion.skills);
    let cur_heal = current
        .skills
        .heal
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let cur_elite = current
        .skills
        .elite
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let cur_utils: Vec<String> = current
        .skills
        .utilities
        .iter()
        .map(|u| u.as_ref().map(|s| s.name.clone()).unwrap_or_default())
        .collect();

    let mut skills = Vec::new();
    skills.push(diff_slot("Heal", &cur_heal, &sug_heal));
    let max_utils = cur_utils.len().max(sug_utils.len()).max(3);
    for i in 0..max_utils {
        let cur_u = cur_utils.get(i).map(|s| s.as_str()).unwrap_or("");
        let sug_u = sug_utils.get(i).map(|s| s.as_str()).unwrap_or("");
        skills.push(diff_slot(&format!("Utility {}", i + 1), cur_u, sug_u));
    }
    skills.push(diff_slot("Elite", &cur_elite, &sug_elite));

    // --- Weapons & Sigils ---
    let sug_weapons = parse_suggestion_weapons(&suggestion.weapons);
    let mut weapon_sets = Vec::new();
    let max_sets = current.weapons.len().max(sug_weapons.len());
    for i in 0..max_sets {
        let (cur_weapon_str, cur_sigil_str) = if let Some(ws) = current.weapons.get(i) {
            let mut parts = Vec::new();
            if let Some(ref mh) = ws.main_hand {
                parts.push(mh.weapon_type.clone());
            }
            if let Some(ref oh) = ws.off_hand {
                parts.push(oh.weapon_type.clone());
            }
            let weapon_str = if parts.is_empty() {
                "(empty)".to_string()
            } else {
                parts.join(" / ")
            };
            let sigil_names: Vec<&str> = ws.sigils.iter().map(|s| s.name.as_str()).collect();
            let sigil_str = if sigil_names.is_empty() {
                "(none)".to_string()
            } else {
                sigil_names.join(", ")
            };
            (weapon_str, sigil_str)
        } else {
            ("(empty)".to_string(), "(none)".to_string())
        };

        let (sug_label, sug_weapon_str) = if let Some((label, weapons)) = sug_weapons.get(i) {
            (label.clone(), weapons.clone())
        } else {
            (format!("Set {}", i + 1), "(empty)".to_string())
        };

        // Sigils from suggestion — flat list, assign 2 per set
        let sug_sigil_str = {
            let start = i * 2;
            let sigils: Vec<&str> = suggestion
                .sigils
                .iter()
                .skip(start)
                .take(2)
                .map(|s| s.as_str())
                .collect();
            if sigils.is_empty() {
                "(none)".to_string()
            } else {
                sigils.join(", ")
            }
        };

        let weapon_diff = diff_slot(&sug_label, &cur_weapon_str, &sug_weapon_str);
        let sigil_diff = diff_slot("  Sigils", &cur_sigil_str, &sug_sigil_str);
        weapon_sets.push((weapon_diff, sigil_diff));
    }

    // --- Rune ---
    let cur_rune = current
        .rune
        .as_ref()
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let rune = diff_slot("Rune", &cur_rune, &suggestion.rune);

    // --- Relic ---
    let cur_relic = current
        .relic
        .as_ref()
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let relic = diff_slot("Relic", &cur_relic, &suggestion.relic);

    BuildDiff {
        gear_prefix,
        specializations,
        skills,
        weapon_sets,
        rune,
        relic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize() {
        assert_eq!(normalize("Superior Rune of the Scholar"), "scholar");
        assert_eq!(normalize("Superior Sigil of Force"), "force");
        assert_eq!(normalize("Relic of the Thief"), "thief");
        assert_eq!(normalize("Berserker's"), "berserker's");
    }

    #[test]
    fn test_diff_unchanged() {
        let d = diff_slot("Test", "Scholar", "Superior Rune of the Scholar");
        assert_eq!(d.status, ChangeStatus::Unchanged);
    }

    #[test]
    fn test_diff_changed() {
        let d = diff_slot("Test", "Scholar", "Firebrand");
        assert_eq!(d.status, ChangeStatus::Changed);
    }

    #[test]
    fn test_parse_skills() {
        let skills = vec![
            "Heal: Mending".to_string(),
            "Utility: Signet of Resolve".to_string(),
            "Utility: Stand Your Ground".to_string(),
            "Utility: Advance!".to_string(),
            "Elite: Feel My Wrath".to_string(),
        ];
        let (heal, utils, elite) = parse_suggestion_skills(&skills);
        assert_eq!(heal, "Mending");
        assert_eq!(utils.len(), 3);
        assert_eq!(elite, "Feel My Wrath");
    }

    #[test]
    fn test_parse_weapons() {
        let weapons = vec![
            "Set 1: Sword / Shield".to_string(),
            "Set 2: Rifle".to_string(),
        ];
        let parsed = parse_suggestion_weapons(&weapons);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].1, "Sword / Shield");
        assert_eq!(parsed[1].1, "Rifle");
    }
}
