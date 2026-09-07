//! Which outgoing-damage percent modifiers share the game's one additive
//! bucket. Everything else multiplies. Same `include_str!` + `OnceLock` shape
//! as the boon and condition formulas.
//!
//! Wiki `Damage_calculation`: `(1 + 0.05 + 0.03 + 0.10) * 1.20 * 1.05` — some
//! sigils, traits and utility effects add together first, the rest are
//! separate factors. Nothing in the API says which is which; the list in
//! `data/formulas/modifier_buckets.json` is per-effect tested fact.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;

const MODIFIER_BUCKETS_JSON: &str = include_str!("../../../../data/formulas/modifier_buckets.json");

static ADDITIVE: OnceLock<HashSet<String>> = OnceLock::new();

#[derive(Deserialize)]
struct RawBuckets {
    additive: Vec<String>,
}

/// True when `name` (a trait, sigil or relic display name) is one of the
/// modifiers that add with each other instead of multiplying.
pub fn is_additive_modifier(name: &str) -> bool {
    ADDITIVE
        .get_or_init(|| {
            let raw: RawBuckets = serde_json::from_str(MODIFIER_BUCKETS_JSON)
                .expect("embedded modifier_buckets.json is invalid");
            raw.additive.into_iter().collect()
        })
        .contains(name)
}

#[cfg(test)]
mod tests {
    use super::is_additive_modifier;

    #[test]
    fn force_sigil_adds_and_an_unknown_name_multiplies() {
        assert!(is_additive_modifier("Superior Sigil of Force"));
        assert!(is_additive_modifier("Berserker's Power"));
        assert!(!is_additive_modifier("Big Game Hunter"));
        assert!(!is_additive_modifier("not a trait"));
    }
}
