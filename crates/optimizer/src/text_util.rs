//! Shared string-parsing helpers used across the optimizer crate.
//!
//! These were previously duplicated across `combat.rs`, `synergy.rs`,
//! `search_v2.rs`, and `synergy_pipeline.rs`. They are consolidated here so
//! every caller shares one correct implementation. In particular,
//! [`extract_percent_before`] uses the closest-percent algorithm (the buggy
//! first-percent variant that used to live in `synergy.rs` has been removed).

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
}
