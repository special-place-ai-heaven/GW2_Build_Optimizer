//! Feedback taxonomy: categories the player can pick and the wizard steps each one walks through.
//! The embedded copy ships in the DLL; a newer one may arrive from the server and must still parse.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The taxonomy JSON compiled into the DLL (`data/feedback_taxonomy.json` at the repo root).
pub const EMBEDDED: &str = include_str!("../../../../data/feedback_taxonomy.json");

/// Categories plus the step definitions they reference by id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FeedbackTaxonomy {
    pub taxonomy_version: u32,
    pub categories: Vec<Category>,
    pub steps: HashMap<String, Step>,
}

/// One tile on the "Message Developer" screen.
///
/// `kind`, `icon` and `color` are deliberately plain strings: the UI maps the values it knows
/// and treats anything else as inert/dot/muted, so a newer taxonomy never fails to parse.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    /// `"report"` opens the wizard, `"link"` opens `url`; unknown kinds are inert.
    #[serde(rename = "type")]
    pub kind: String,
    /// Locale key for the tile label.
    pub label: String,
    /// Glyph name; unknown glyphs render as a dot.
    pub icon: String,
    /// Named accent color; unknown colors render muted.
    pub color: String,
    /// Wizard step ids, in order. Empty for links.
    #[serde(default)]
    pub steps: Vec<String>,
    /// Whether the report should carry a build snapshot.
    #[serde(default)]
    pub attach_build: bool,
    /// Target for `kind == "link"` categories.
    #[serde(default)]
    pub url: Option<String>,
}

/// One wizard step: either a choice list or a free-text field.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Locale key for the step prompt.
    pub prompt: String,
    /// Choice ids for pick-one steps. Empty for text steps.
    #[serde(default)]
    pub choices: Vec<String>,
    /// Length rule for free-text steps. `None` for choice steps.
    #[serde(default)]
    pub text: Option<TextRule>,
}

/// Inclusive character-count bounds for a free-text step.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextRule {
    pub min: usize,
    pub max: usize,
}

impl FeedbackTaxonomy {
    /// The taxonomy compiled into the DLL. Cannot fail at runtime: the JSON is checked by
    /// `embedded_taxonomy_parses` and a broken file would fail `cargo test`, not the player.
    pub fn embedded() -> Self {
        Self::parse(EMBEDDED).expect("embedded feedback taxonomy is valid JSON")
    }

    /// Parse a taxonomy document (embedded, cached, or fresh from the server).
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Look up a category by id.
    pub fn category(&self, id: &str) -> Option<&Category> {
        self.categories.iter().find(|c| c.id == id)
    }

    /// Look up a step by id.
    pub fn step(&self, id: &str) -> Option<&Step> {
        self.steps.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_taxonomy_parses() {
        let tax = FeedbackTaxonomy::embedded();
        assert_eq!(tax.taxonomy_version, 1);
        assert_eq!(tax.categories.len(), 6);
        assert_eq!(tax.steps.len(), 6);

        let coffee = tax.category("coffee").expect("coffee category");
        assert_eq!(coffee.kind, "link");
        assert!(coffee.url.is_some());
        assert!(coffee.steps.is_empty());
        assert!(!coffee.attach_build);

        let wrong_build = tax.category("wrong_build").expect("wrong_build category");
        assert!(wrong_build.attach_build);

        let bug = tax.category("bug").expect("bug category");
        assert_eq!(bug.steps, ["area_screen", "severity", "describe"]);
        assert!(!bug.attach_build);
        assert!(bug.url.is_none());
    }

    #[test]
    fn unknown_kind_icon_color_parse() {
        let json = r#"{
            "taxonomy_version": 2,
            "categories": [
                {"id":"vote","type":"vote","label":"cat.vote","icon":"rocket","color":"pink","steps":[]}
            ],
            "steps": {}
        }"#;
        let tax = FeedbackTaxonomy::parse(json).expect("unknown kind/icon/color must still parse");
        assert_eq!(tax.taxonomy_version, 2);
        let vote = tax.category("vote").expect("vote category");
        assert_eq!(vote.kind, "vote");
        assert_eq!(vote.icon, "rocket");
        assert_eq!(vote.color, "pink");
        assert!(vote.steps.is_empty());
        assert!(!vote.attach_build);
        assert!(vote.url.is_none());

        // Round-trip: the strings survive serialize/deserialize unchanged and `kind` is wired as "type".
        let out = serde_json::to_string(&tax).unwrap();
        assert!(out.contains("\"type\":\"vote\""));
        let back = FeedbackTaxonomy::parse(&out).unwrap();
        assert_eq!(back, tax);
    }

    #[test]
    fn text_rule_parses() {
        let tax = FeedbackTaxonomy::embedded();

        let describe = tax.step("describe").expect("describe step");
        assert_eq!(describe.text, Some(TextRule { min: 10, max: 4000 }));
        assert!(describe.choices.is_empty());

        let area_screen = tax.step("area_screen").expect("area_screen step");
        assert_eq!(area_screen.choices.len(), 9);
        assert_eq!(area_screen.text, None);
    }

    #[test]
    fn lookup_helpers() {
        let tax = FeedbackTaxonomy::embedded();
        assert!(tax.category("bug").is_some());
        assert!(tax.category("nope").is_none());
        assert!(tax.step("describe").is_some());
        assert!(tax.step("nope").is_none());

        let empty = FeedbackTaxonomy::default();
        assert_eq!(empty.taxonomy_version, 0);
        assert!(empty.category("bug").is_none());
        assert!(empty.step("describe").is_none());
    }
}
