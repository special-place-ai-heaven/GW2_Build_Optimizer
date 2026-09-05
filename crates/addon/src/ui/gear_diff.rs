//! Parsing helpers for LLM-generated suggestion strings: route "Set N:",
//! "Heal:", "Utils:" / "Utility:" etc. prefixed entries into structured fields.

#[derive(Debug, Clone)]
pub(crate) struct ParsedSkills {
    pub heal: String,
    pub utilities: Vec<String>,
    pub elite: String,
    pub stances: String,
    pub pets: String,
}

fn strip_label_ci<'a>(s: &'a str, label: &str) -> Option<&'a str> {
    let head = s.get(..label.len())?;
    if head.eq_ignore_ascii_case(label) {
        Some(&s[label.len()..])
    } else {
        None
    }
}

fn push_csv_names(dest: &mut Vec<String>, rest: &str) {
    for name in rest.split(',') {
        let name = name.trim();
        if !name.is_empty() {
            dest.push(name.to_string());
        }
    }
}

/// Parse suggestion skill strings: "Heal: X", "Utils: a, b, c" / "Utility: X",
/// "Elite: X", plus optional "Stances:" / "Pets:" rows.
///
/// Prefix matching is case-insensitive — the LLM occasionally lowercases
/// labels, and a case-sensitive `strip_prefix` would silently misroute
/// "heal: X" into the utility bucket.
pub(crate) fn parse_suggestion_skills(skills: &[String]) -> ParsedSkills {
    let mut parsed = ParsedSkills {
        heal: String::new(),
        utilities: Vec::new(),
        elite: String::new(),
        stances: String::new(),
        pets: String::new(),
    };
    for s in skills {
        if let Some(name) = strip_label_ci(s, "Heal: ") {
            parsed.heal = name.trim().to_string();
        } else if let Some(rest) = strip_label_ci(s, "Utils: ") {
            push_csv_names(&mut parsed.utilities, rest);
        } else if let Some(rest) = strip_label_ci(s, "Utility: ") {
            push_csv_names(&mut parsed.utilities, rest);
        } else if let Some(name) = strip_label_ci(s, "Elite: ") {
            parsed.elite = name.trim().to_string();
        } else if let Some(name) = strip_label_ci(s, "Stances: ") {
            parsed.stances = name.trim().to_string();
        } else if let Some(name) = strip_label_ci(s, "Pets: ") {
            parsed.pets = name.trim().to_string();
        } else {
            parsed.utilities.push(s.trim().to_string());
        }
    }
    parsed
}

/// Parse suggestion weapon strings: "Set 1: Sword / Shield", "Set 2: Rifle".
///
/// Prefix matching is case-insensitive so "set 1:" or "SET 1:" route the
/// same as the canonical "Set 1:".
pub(crate) fn parse_suggestion_weapons(weapons: &[String]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for w in weapons {
        if let Some(rest) = strip_label_ci(w, "Set 1: ") {
            result.push(("Set 1".to_string(), rest.trim().to_string()));
        } else if let Some(rest) = strip_label_ci(w, "Set 2: ") {
            result.push(("Set 2".to_string(), rest.trim().to_string()));
        } else {
            result.push(("Weapons".to_string(), w.trim().to_string()));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skills() {
        let skills = vec![
            "Heal: Mending".to_string(),
            "Utility: Signet of Resolve".to_string(),
            "Utility: Stand Your Ground".to_string(),
            "Utility: Advance!".to_string(),
            "Elite: Feel My Wrath".to_string(),
        ];
        let parsed = parse_suggestion_skills(&skills);
        assert_eq!(parsed.heal, "Mending");
        assert_eq!(parsed.utilities.len(), 3);
        assert_eq!(parsed.elite, "Feel My Wrath");
        assert!(parsed.stances.is_empty());
        assert!(parsed.pets.is_empty());
    }

    #[test]
    fn test_parse_skills_stances_and_pets() {
        let skills = vec![
            "Stances: Assassin / Dwarf".to_string(),
            "Pets: #1 / #2".to_string(),
            "Heal: Mending".to_string(),
            "Elite: Feel My Wrath".to_string(),
        ];
        let parsed = parse_suggestion_skills(&skills);
        assert_eq!(parsed.stances, "Assassin / Dwarf");
        assert_eq!(parsed.pets, "#1 / #2");
        assert_eq!(parsed.heal, "Mending");
        assert!(parsed.utilities.is_empty());
        assert_eq!(parsed.elite, "Feel My Wrath");
    }

    #[test]
    fn test_parse_skills_case_insensitive_labels() {
        // Regression: lowercase / mixed-case labels from the LLM used to fall
        // through to the unknown-format branch (treated as utilities).
        let skills = vec![
            "heal: Mending".to_string(),
            "UTILITY: Signet of Resolve".to_string(),
            "Elite: Feel My Wrath".to_string(),
        ];
        let parsed = parse_suggestion_skills(&skills);
        assert_eq!(parsed.heal, "Mending");
        assert_eq!(parsed.utilities, vec!["Signet of Resolve".to_string()]);
        assert_eq!(parsed.elite, "Feel My Wrath");
    }

    #[test]
    fn test_parse_skills_utils_comma_list_and_unlabeled() {
        // summarize_resolved_build writes "Utils:"; gemini_from_validated writes
        // "Utility:". Unlabeled rows stay utilities so rotation name lookup
        // still sees them.
        let skills = vec![
            "utils: Blood Reckoning, Bull's Charge, Signet of Fury".to_string(),
            "orphaned".to_string(),
            "Pets: #7 / #8".to_string(),
        ];
        let parsed = parse_suggestion_skills(&skills);
        assert_eq!(
            parsed.utilities,
            vec![
                "Blood Reckoning".to_string(),
                "Bull's Charge".to_string(),
                "Signet of Fury".to_string(),
                "orphaned".to_string(),
            ]
        );
        assert_eq!(parsed.pets, "#7 / #8");
        assert!(parsed.heal.is_empty());
    }

    #[test]
    fn test_parse_weapons_case_insensitive_labels() {
        let weapons = vec![
            "set 1: Axe / Axe".to_string(),
            "SET 2: Greatsword".to_string(),
        ];
        let parsed = parse_suggestion_weapons(&weapons);
        assert_eq!(
            parsed,
            vec![
                ("Set 1".to_string(), "Axe / Axe".to_string()),
                ("Set 2".to_string(), "Greatsword".to_string()),
            ]
        );
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
