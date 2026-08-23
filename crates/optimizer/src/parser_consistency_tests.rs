//! Cross-parser consistency guard.
//!
//! The optimizer has TWO parallel `Fact` parsers that must agree on what a
//! modifier *means*:
//!
//! - `combat::extract_modifier_from_fact` — populates a `DamageModifiers`
//!   struct used by the combat math model.
//! - `synergy::extract_effects_from_fact` — emits `NormalizedEffect`s used by
//!   the synergy pipeline.
//!
//! They are independent code paths over the same input and have silently
//! diverged before (condition-damage dropped in one, "Poison" vs "Poisoned"
//! key mismatch, first-vs-closest percent branch). Each individual bug got a
//! narrow regression test in its own module, but nothing asserted the two
//! parsers stay CONSISTENT with each other. This module is that guard.
//!
//! Each parser is run through a `#[cfg(test)] pub(crate)` shim
//! (`combat::tests_consistency_shim::classify_fact` /
//! `synergy::tests_consistency_shim::classify_fact`) that collapses its output
//! into the comparable [`FactClass`] set below. For every `Fact` in the corpus
//! we assert the two shims produce the same classification set.
//!
//! If a case is found where the parsers STILL disagree, that is a real bug:
//! it must be fixed in the parser, not papered over in the test. Any
//! intentional asymmetry must be documented inline (see [`Expectation`]).

/// A parser-agnostic classification of what a modifier `Fact` means.
///
/// Both parser shims map their (very different) internal representations down
/// to this single enum so the two can be compared directly with `assert_eq!`.
/// String payloads carry the *canonical* condition name (e.g. "Poisoned"),
/// which is itself part of what the parsers must agree on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FactClass {
    /// Strike (direct/power) damage increase.
    Strike,
    /// Global condition damage increase.
    ConditionDamage,
    /// Per-condition damage increase, canonical key.
    SpecificConditionDamage(String),
    /// Additive critical (ferocity) damage increase.
    Crit,
    /// Outgoing healing increase.
    Healing,
    /// Global condition duration increase.
    AllConditionDuration,
    /// Global boon duration increase.
    AllBoonDuration,
    /// Per-condition duration increase, canonical key.
    SpecificConditionDuration(String),
}

#[cfg(test)]
mod tests {
    use super::FactClass;
    use crate::combat::tests_consistency_shim::classify_fact as classify_combat;
    use crate::synergy::tests_consistency_shim::classify_fact as classify_synergy;
    use gw2_api::models::Fact;

    fn percent_fact(text: &str, percent: f64) -> Fact {
        Fact::Percent {
            text: Some(text.to_string()),
            icon: None,
            percent: Some(percent),
        }
    }

    /// Normalize a classification set for order-independent comparison: both
    /// parsers may emit the same classes in different orders (the combat shim
    /// iterates struct fields; the synergy shim iterates an effect Vec).
    fn normalized(mut v: Vec<FactClass>) -> Vec<FactClass> {
        v.sort();
        v.dedup();
        v
    }

    /// One corpus row: an input fact and the classification both parsers MUST
    /// agree on. `expected` documents (and double-checks) the intended class,
    /// catching the case where the two parsers agree with each other but on the
    /// WRONG answer.
    struct Case {
        label: &'static str,
        text: &'static str,
        percent: f64,
        expected: Vec<FactClass>,
    }

    fn corpus() -> Vec<Case> {
        use FactClass::*;
        vec![
            // ── Strike damage ──
            Case {
                label: "explicit strike damage",
                text: "+10% Strike Damage",
                percent: 10.0,
                expected: vec![Strike],
            },
            Case {
                label: "generic damage increase routes to strike",
                text: "Damage increased by 7%",
                percent: 7.0,
                expected: vec![Strike],
            },
            // ── Condition damage (the dropped-without-"increase" bug) ──
            Case {
                label: "condition damage, no 'increase' keyword",
                text: "Condition Damage: +8%",
                percent: 8.0,
                expected: vec![ConditionDamage],
            },
            Case {
                label: "condition damage, explicit plus",
                text: "+5% Condition Damage",
                percent: 5.0,
                expected: vec![ConditionDamage],
            },
            // ── All-condition duration ──
            Case {
                label: "all condition duration",
                text: "+10% Condition Duration",
                percent: 10.0,
                expected: vec![AllConditionDuration],
            },
            // ── All-boon duration ──
            Case {
                label: "all boon duration",
                text: "+10% Boon Duration",
                percent: 10.0,
                expected: vec![AllBoonDuration],
            },
            // ── Specific-condition duration (canonical-key bugs) ──
            Case {
                label: "burning duration keeps 'Burning' key",
                text: "+7% Burning Duration",
                percent: 7.0,
                expected: vec![SpecificConditionDuration("Burning".to_string())],
            },
            Case {
                // THE bug we just fixed: verb-form "Poison" must canonicalize to
                // "Poisoned" in BOTH parsers, or a duration bonus never matches.
                label: "poison duration canonicalizes to 'Poisoned'",
                text: "+10% Poison Duration",
                percent: 10.0,
                expected: vec![SpecificConditionDuration("Poisoned".to_string())],
            },
            // ── Outgoing healing ──
            Case {
                label: "outgoing healing",
                text: "+15% Outgoing Healing",
                percent: 15.0,
                expected: vec![Healing],
            },
            // ── Critical damage ──
            Case {
                label: "critical damage",
                text: "+10% Critical Damage",
                percent: 10.0,
                expected: vec![Crit],
            },
            // ── Screenshot / standing-percent rejects ──
            Case {
                label: "Precise Strike 100% is not standing crit",
                text: "Critical Chance Increase",
                percent: 100.0,
                expected: vec![],
            },
            Case {
                label: "standing crit chance has no FactClass",
                text: "Critical Chance Increase",
                percent: 10.0,
                expected: vec![],
            },
            Case {
                label: "Damage Reduced is not outgoing strike",
                text: "Damage Reduced",
                percent: 25.0,
                expected: vec![],
            },
            Case {
                label: "Recharge Reduced is not strike",
                text: "Recharge Reduced",
                percent: 20.0,
                expected: vec![],
            },
            Case {
                label: "bare Percent tooltip is not strike",
                text: "Percent",
                percent: 5.0,
                expected: vec![],
            },
            Case {
                label: "API Damage Increase stays strike",
                text: "Damage Increase",
                percent: 10.0,
                expected: vec![Strike],
            },
        ]
    }

    /// The core invariant: for every corpus fact, the combat parser and the
    /// synergy parser must produce the SAME classification set, and it must be
    /// the intended one. A divergence here is a real bug in one of the parsers.
    #[test]
    fn parsers_agree_on_modifier_classification() {
        let mut disagreements = Vec::new();
        let mut wrong = Vec::new();

        for case in corpus() {
            let fact = percent_fact(case.text, case.percent);
            let combat = normalized(classify_combat(&fact));
            let synergy = normalized(classify_synergy(&fact));
            let expected = normalized(case.expected.clone());

            if combat != synergy {
                disagreements.push(format!(
                    "  [{}] input {:?}\n      combat  = {:?}\n      synergy = {:?}",
                    case.label, case.text, combat, synergy
                ));
            }
            // Also assert the agreed-upon answer is the correct one, so the two
            // parsers can't silently agree on a wrong classification.
            if combat == synergy && combat != expected {
                wrong.push(format!(
                    "  [{}] input {:?}\n      both     = {:?}\n      expected = {:?}",
                    case.label, case.text, combat, expected
                ));
            }
        }

        assert!(
            disagreements.is_empty(),
            "Cross-parser DISAGREEMENT (a real bug — fix the parser, not this test):\n{}",
            disagreements.join("\n")
        );
        assert!(
            wrong.is_empty(),
            "Parsers agree but on the WRONG classification:\n{}",
            wrong.join("\n")
        );
    }
}
