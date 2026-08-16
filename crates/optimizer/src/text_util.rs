//! Shared string-parsing helpers used across the optimizer crate.

/// Extract a percentage number associated with `keyword` from text.
/// Picks the `N%` occurrence closest (by char distance) to the keyword,
/// in either direction.
///
/// Examples:
/// - `"10% burning duration"` + `"burning duration"` → `Some(10.0)`
/// - `"increases outgoing healing by 15%"` + `"healing"` → `Some(15.0)`
/// - `"+10% condition duration. +5% boon duration."` + `"boon duration"`
///   → `Some(5.0)` (the closer percent, not the first one).
///
/// Uses char-level iteration to avoid UTF-8 boundary panics.
#[cfg(test)]
pub(crate) fn extract_percent_before(text: &str, keyword: &str) -> Option<f64> {
    let chars: Vec<char> = text.chars().collect();
    let keyword_chars: Vec<char> = keyword.chars().collect();
    if keyword_chars.is_empty() || keyword_chars.len() > chars.len() {
        return None;
    }
    // Find the first occurrence of keyword as a char-window in chars.
    let kw_start = (0..=chars.len() - keyword_chars.len())
        .find(|&i| chars[i..i + keyword_chars.len()] == keyword_chars[..])?;
    let kw_end = kw_start + keyword_chars.len();
    // Find the `%` whose distance to the keyword span is minimal.
    let pct_pos = chars
        .iter()
        .enumerate()
        .filter(|(_, &c)| c == '%')
        .map(|(i, _)| {
            let dist = if i < kw_start {
                kw_start - i
            } else if i >= kw_end {
                i - (kw_end - 1)
            } else {
                0
            };
            (dist, i)
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, i)| i)?;
    // Walk backwards from `%` to find the start of the number.
    let start = chars[..pct_pos]
        .iter()
        .rposition(|c| !c.is_ascii_digit() && *c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= pct_pos {
        return None;
    }
    let num: String = chars[start..pct_pos].iter().collect();
    num.parse::<f64>().ok()
}

/// Uppercase the first character of `s`, leaving the rest unchanged.
pub(crate) fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// Normalize a sigil name into its canonical "family" key.
///
/// Strips the `" (PvP)"` suffix (the PvP variant shares the same family),
/// lowercases, and trims. Used to detect duplicate sigil families across
/// weapon sets.
pub(crate) fn normalize_sigil_family(name: &str) -> String {
    let mut base = name.replace(" (PvP)", "");
    base.make_ascii_lowercase();
    base.trim().to_string()
}

/// Heuristic: does `text` describe a skill that removes/cleanses conditions?
///
/// Matches GW2 fact/description wording — a "condition" stem plus a
/// remove/cleanse/cure verb. Used by rotation analysis and the synergy
/// pipeline to detect cleanse coverage.
pub(crate) fn text_describes_condition_cleanse(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("condit")
        && (lower.contains("remov") || lower.contains("cleanse") || lower.contains("cure"))
}

/// Strip GW2 tooltip markup (`<br>`, `<c=@reminder>`, `@abilitytype`).
pub(crate) fn strip_gw2_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace('\u{00a0}', " ")
}

/// Stack cap from API text (`max 25 stacks`, `maximum of 5 stacks`).
/// Returns 1 when the word "stack" is a verb (Fireworks "refreshes duration on stack").
pub(crate) fn stack_multiplier(text: &str) -> f64 {
    let t = text.to_lowercase();
    let mut search_from = 0;
    while let Some(rel) = t[search_from..].find("stack") {
        let idx = search_from + rel;
        let before = t[..idx].trim_end();
        let digits: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(n) = digits.parse::<f64>() {
            if (2.0..=25.0).contains(&n) {
                let prefix = before[..before.len() - digits.len()].trim_end();
                if prefix.ends_with("maximum of")
                    || prefix.ends_with("max")
                    || prefix.ends_with("up to")
                {
                    return n;
                }
            }
        }
        search_from = idx + 1;
    }
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_percent_before_basic() {
        assert_eq!(
            extract_percent_before("10% burning duration", "burning duration"),
            Some(10.0)
        );
        assert_eq!(
            extract_percent_before("+10% condition duration", "condition duration"),
            Some(10.0)
        );
        assert_eq!(
            extract_percent_before("grants 5% damage bonus", "damage"),
            Some(5.0)
        );
        // No match
        assert_eq!(extract_percent_before("no number here", "damage"), None);
        // Keyword not found
        assert_eq!(
            extract_percent_before("10% burning duration", "poison duration"),
            None
        );
    }

    #[test]
    fn test_extract_percent_before_unicode_safe() {
        // Should not panic on non-ASCII characters
        assert_eq!(extract_percent_before("—5% damage", "damage"), Some(5.0));
        assert_eq!(
            extract_percent_before("résumé 10% condition duration", "condition duration"),
            Some(10.0)
        );
    }

    #[test]
    fn extract_percent_before_simple() {
        assert_eq!(
            extract_percent_before("10% burning duration", "burning duration"),
            Some(10.0)
        );
        assert_eq!(extract_percent_before("+7% damage", "damage"), Some(7.0));
    }

    #[test]
    fn extract_percent_before_picks_closest_when_multiple_percents() {
        // Bug regression: previously the synergy.rs copy returned the FIRST `%`
        // in the text, so this case would return 10.0 for `"boon duration"`
        // instead of 5.0. The shared closest-percent implementation now protects
        // BOTH the combat path and the synergy path against this regression.
        let text = "+10% condition duration. +5% boon duration.";
        assert_eq!(extract_percent_before(text, "boon duration"), Some(5.0));
        assert_eq!(
            extract_percent_before(text, "condition duration"),
            Some(10.0)
        );
    }

    #[test]
    fn extract_percent_before_missing_keyword() {
        assert_eq!(extract_percent_before("10% damage", "boon duration"), None);
    }

    #[test]
    fn extract_percent_before_percent_after_keyword() {
        // Real GW2 description form: "increases outgoing healing by 15%" —
        // percent appears AFTER the keyword. The picker is direction-agnostic
        // and returns the closest percent in either direction.
        assert_eq!(
            extract_percent_before("increases outgoing healing by 15%", "healing"),
            Some(15.0),
        );
    }

    #[test]
    fn extract_percent_before_handles_decimals() {
        assert_eq!(
            extract_percent_before("+0.5% burning damage", "burning damage"),
            Some(0.5)
        );
    }

    #[test]
    fn capitalize_basic() {
        assert_eq!(capitalize("burning"), "Burning");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("a"), "A");
    }

    #[test]
    fn normalize_sigil_family_strips_pvp_and_lowercases() {
        assert_eq!(
            normalize_sigil_family("Superior Sigil of Force (PvP)"),
            "superior sigil of force"
        );
        assert_eq!(
            normalize_sigil_family("  Superior Sigil of Force  "),
            "superior sigil of force"
        );
    }

    #[test]
    fn condition_cleanse_matches_stem_plus_verb() {
        assert!(text_describes_condition_cleanse("Remove a condition"));
        assert!(text_describes_condition_cleanse("Cleanse 2 conditions"));
        assert!(text_describes_condition_cleanse(
            "Cure conditions from allies"
        ));
        // Needs both a condition stem AND a cleanse verb.
        assert!(!text_describes_condition_cleanse("Gain 3 stacks of might"));
        assert!(!text_describes_condition_cleanse(
            "Conditions you apply last longer"
        ));
    }

    #[test]
    fn strip_gw2_markup_drops_tags() {
        assert_eq!(
            strip_gw2_markup("<c=@reminder>Deal increased strike damage</c>"),
            "Deal increased strike damage"
        );
        assert_eq!(strip_gw2_markup("A<br>B"), "AB");
    }

    #[test]
    fn stack_multiplier_reads_cap_not_verb() {
        assert_eq!(
            stack_multiplier("Gain 3% condition duration. Maximum of 5 stacks."),
            5.0
        );
        assert_eq!(
            stack_multiplier("Deal increased strike damage. Refreshes duration on stack."),
            1.0
        );
        assert_eq!(stack_multiplier("+10 power. Max 25 stacks."), 25.0);
    }
}
