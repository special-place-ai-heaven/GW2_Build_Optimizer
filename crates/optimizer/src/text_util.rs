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
    // ASCII-only lowercase copy so byte windows never split a multibyte char.
    let lower: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii() { c } else { ' ' })
        .collect();
    if !lower.contains("condit") {
        return false;
    }
    // Every verb the game uses for getting a condition OFF the caster or an
    // ally, each within a short window of "condit" so "condition damage ...
    // transference" (Chaotic Transference) does not read as a cleanse:
    // remove / cleanse / cure / purge it, transfer or send it to a foe,
    // consume it (Consume Conditions, Spectral Walk), convert it INTO a boon
    // (Well of Power), but not a boon into a condition (Corrupt Boon).
    const WINDOW: usize = 48;
    let bytes = lower.as_bytes();
    let at_word_start = |i: usize| i == 0 || !bytes[i - 1].is_ascii_alphabetic();
    let near_condit = |i: usize, len: usize| {
        let lo = i.saturating_sub(WINDOW);
        let hi = (i + len + WINDOW).min(lower.len());
        lower[lo..hi].contains("condit")
    };
    for verb in ["remov", "cleanse", "cure", "purg", "transfer", "consum", "send", "sent"] {
        for (i, _) in lower.match_indices(verb) {
            if !at_word_start(i) || !near_condit(i, verb.len()) {
                continue;
            }
            // "transference" is a stat trait, not a transfer.
            if verb == "transfer" && bytes.get(i + verb.len()) == Some(&b'e') {
                continue;
            }
            return true;
        }
    }
    for (i, _) in lower.match_indices("convert") {
        if !at_word_start(i) {
            continue;
        }
        let before = &lower[i.saturating_sub(WINDOW)..i];
        let after = &lower[i..(i + WINDOW).min(lower.len())];
        if before.contains("boon") {
            continue; // "Boons Converted to Conditions"
        }
        let condit_after = after.find("condit");
        let boon_after = after.find("boon");
        let cleanse = match (condit_after, boon_after) {
            (Some(c), Some(b)) => c < b, // "convert conditions into boons", not the reverse
            (Some(_), None) => true,     // "convert conditions into life force"
            // "Conditions Converted to Boons": the condition sits before the verb.
            (None, Some(_)) => before.contains("condit"),
            (None, None) => false,
        };
        if cleanse {
            return true;
        }
    }
    false
}

/// Heuristic: skill text grants Stability (not "instability").
pub(crate) fn text_describes_stability(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("stability") && !lower.contains("instability")
}

/// Relic/rune tooltip or name grants self-Stability (Cavalier relic has no skill fact).
pub(crate) fn gear_text_grants_stability(
    name: &str,
    description: Option<&str>,
    bonuses: &[String],
) -> bool {
    if text_describes_stability(name)
        || description.is_some_and(text_describes_stability)
        || bonuses.iter().any(|b| text_describes_stability(b))
    {
        return true;
    }
    let n = name.to_lowercase();
    n.contains("relic") && n.contains("cavalier")
}

/// Heuristic: skill text grants a block (not "unblockable").
pub(crate) fn text_describes_block(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("block") && !lower.contains("unblockable")
}

/// Heuristic: evade / dodge frames (Daredevil Bound, Roll for Initiative).
pub(crate) fn text_describes_evade(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("evade") || lower.contains("dodge")
}

/// Heuristic: stealth / invisibility (not just the word "hidden").
pub(crate) fn text_describes_stealth(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("stealth") || lower.contains("invisib")
}

/// Heuristic: true invuln (Distortion, Mist Form) — not a strippable boon.
pub(crate) fn text_describes_invulnerability(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("invulnerab") || lower.contains("distortion") || lower.contains("mist form")
}

/// Personal cover vs incoming CC: stab, evade, block, invuln, or stealth.
pub(crate) fn text_describes_cc_answer(text: &str) -> bool {
    text_describes_stability(text)
        || text_describes_evade(text)
        || text_describes_block(text)
        || text_describes_stealth(text)
        || text_describes_invulnerability(text)
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

    /// Necromancer, Revenant and Engineer cleanse by transferring, sending,
    /// consuming and converting; a WvW Reaper with "Suffer!" was judged to
    /// have no cleanse at all (2026-09-05). Boon corruption is the inverse.
    #[test]
    fn condition_cleanse_knows_every_verb_the_game_uses() {
        assert!(text_describes_condition_cleanse("Conditions Transferred"));
        assert!(text_describes_condition_cleanse(
            "Transfer conditions to each foe you strike."
        ));
        assert!(text_describes_condition_cleanse("Conditions Sent"));
        assert!(text_describes_condition_cleanse(
            "Feast on your conditions, gaining health for each one consumed."
        ));
        assert!(text_describes_condition_cleanse(
            "become spectral, consuming conditions for life force"
        ));
        assert!(text_describes_condition_cleanse("Conditions Converted to Boons"));
        assert!(text_describes_condition_cleanse("Convert conditions into boons."));
        assert!(text_describes_condition_cleanse("Purge conditions from allies."));
        // Not cleanses.
        assert!(!text_describes_condition_cleanse(
            "Corrupt boons on your foe, converting their boons into conditions."
        ));
        assert!(!text_describes_condition_cleanse("Boons Converted to Conditions"));
        assert!(!text_describes_condition_cleanse(
            "Gain condition damage based on a percentage of your toughness. (Chaotic Transference)"
        ));
        assert!(!text_describes_condition_cleanse(
            "Conditions present on the target deal more damage."
        ));
        assert!(!text_describes_condition_cleanse(
            "Secure the area; conditions last longer."
        ));
        assert!(
            !text_describes_condition_cleanse("Übermächtige Zustände"),
            "non-ASCII is safe"
        );
    }

    #[test]
    fn stability_text_ignores_instability() {
        assert!(text_describes_stability("Grant 3 stacks of stability."));
        assert!(!text_describes_stability("Cause instability on hit."));
        assert!(!text_describes_stability("Gain might and fury."));
    }

    #[test]
    fn cc_answer_accepts_evade_stealth_invuln_not_unblockable() {
        assert!(text_describes_cc_answer("Evade backward and break stun."));
        assert!(text_describes_cc_answer("Grant stealth to yourself."));
        assert!(text_describes_cc_answer("Distortion: immune to damage."));
        assert!(text_describes_block("Block the next attack."));
        assert!(!text_describes_block("This attack is unblockable."));
        assert!(!text_describes_cc_answer("Deal more damage."));
    }

    #[test]
    fn cavalier_relic_counts_as_stability_without_the_word() {
        assert!(gear_text_grants_stability(
            "Relic of the Cavalier",
            None,
            &[],
        ));
        assert!(gear_text_grants_stability(
            "Some Relic",
            Some("Gain stability when you use a healing skill."),
            &[],
        ));
        assert!(!gear_text_grants_stability("Cavalier's armor", None, &[]));
    }

    #[test]
    fn block_text_ignores_unblockable() {
        assert!(text_describes_block("Block the next attack."));
        assert!(!text_describes_block("This attack is unblockable."));
        assert!(!text_describes_block("Gain might and fury."));
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
