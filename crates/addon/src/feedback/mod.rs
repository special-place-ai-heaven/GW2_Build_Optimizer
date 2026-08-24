//! Feedback feature (addon side): state, HTTP client, shell helper, background tasks.

pub mod client;
pub mod shell;
pub mod tasks;

use std::collections::HashMap;
use std::time::Instant;

use gw2_core::feedback::message::{FailReason, LastPath, LocalMessage, MessageStatus};
use gw2_core::feedback::report::BuildSnapshot;
use gw2_core::feedback::taxonomy::{Category, FeedbackTaxonomy};

/// Which list the About tab shows under the hero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AboutView {
    #[default]
    WhatsNew,
    Messages,
}

/// Where the Message developer wizard currently is.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum WizardStep {
    #[default]
    Pick,
    Step(usize),
    Summary,
    Sending,
    Sent { short_id: String },
    Thanks,
}

/// An open wizard draft. The taxonomy is frozen at open time so a background
/// refresh cannot change the steps under the player.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub report_id: String,
    pub taxonomy: FeedbackTaxonomy,
    pub category: Option<String>,
    /// step id → choice id
    pub choices: HashMap<String, String>,
    /// step id → text
    pub texts: HashMap<String, String>,
    pub contact: String,
    pub include_build: bool,
    pub include_account: bool,
    pub step: WizardStep,
    /// Last send failure shown under Send.
    pub error: Option<FailReason>,
}

/// Why a free-text step does not satisfy its `TextRule`; carries the bound that was violated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    /// Fewer characters than the step's `min`.
    TooShort(usize),
    /// More characters than the step's `max`.
    TooLong(usize),
}

impl Draft {
    /// New draft with a frozen copy of the taxonomy and a fresh report id; step = Pick, no category.
    pub fn new(taxonomy: FeedbackTaxonomy) -> Self {
        Self {
            report_id: uuid::Uuid::new_v4().to_string(),
            taxonomy,
            category: None,
            choices: HashMap::new(),
            texts: HashMap::new(),
            contact: String::new(),
            include_build: false,
            include_account: false,
            step: WizardStep::Pick,
            error: None,
        }
    }

    /// "Same as last time": pick the category, fill the choices from `last.path` in order over
    /// the category's choice steps, then jump to the first text step (or Summary if none).
    /// A choice id the frozen taxonomy no longer offers is skipped, so it shows up in `missing_steps`.
    pub fn from_last_path(taxonomy: FeedbackTaxonomy, last: &LastPath) -> Self {
        let mut draft = Self::new(taxonomy);
        draft.pick(&last.category);
        let choice_steps: Vec<String> = draft
            .step_ids()
            .iter()
            .filter(|id| draft.taxonomy.step(id).is_some_and(|s| s.text.is_none()))
            .cloned()
            .collect();
        for (step_id, choice_id) in choice_steps.iter().zip(&last.path) {
            let offered = draft
                .taxonomy
                .step(step_id)
                .is_some_and(|s| s.choices.iter().any(|c| c == choice_id));
            if offered {
                draft.set_choice(step_id, choice_id);
            }
        }
        if draft.category.is_some() {
            draft.step = match draft.first_text_step() {
                Some(index) => WizardStep::Step(index),
                None => WizardStep::Summary,
            };
        }
        draft
    }

    /// Pick a category: sets `category`, resets choices/texts and the last send error, and moves
    /// to `Step(0)` (or `Summary` when the category has no steps). Unknown ids are ignored.
    pub fn pick(&mut self, category_id: &str) {
        if self.taxonomy.category(category_id).is_none() {
            return;
        }
        self.category = Some(category_id.to_string());
        self.choices.clear();
        self.texts.clear();
        self.error = None;
        self.step = if self.step_ids().is_empty() {
            WizardStep::Summary
        } else {
            WizardStep::Step(0)
        };
    }

    /// The picked category, if it exists in the frozen taxonomy.
    pub fn category(&self) -> Option<&Category> {
        self.category
            .as_deref()
            .and_then(|id| self.taxonomy.category(id))
    }

    /// The picked category's step ids in order; empty before a pick or for links.
    pub fn step_ids(&self) -> &[String] {
        self.category().map_or(&[], |c| c.steps.as_slice())
    }

    /// Step id at `index` in the category's step order.
    pub fn step_id(&self, index: usize) -> Option<&str> {
        self.step_ids().get(index).map(String::as_str)
    }

    /// Number of wizard screens: the category's steps plus the summary.
    pub fn total_steps(&self) -> usize {
        self.step_ids().len() + 1
    }

    /// 1-based position for "Step n of m": `Step(i)` → `i + 1`, `Summary` → `total_steps()`, else None.
    pub fn current_index(&self) -> Option<usize> {
        match self.step {
            WizardStep::Step(i) => Some(i + 1),
            WizardStep::Summary => Some(self.total_steps()),
            _ => None,
        }
    }

    /// Choice steps are always required; text steps only when their `min` is above zero.
    /// A step id the frozen taxonomy does not define is never required.
    pub fn is_required(&self, step_id: &str) -> bool {
        match self.taxonomy.step(step_id) {
            Some(step) => match &step.text {
                Some(rule) => rule.min > 0,
                None => true,
            },
            None => false,
        }
    }

    /// True when the step is satisfied: a choice was made, or the text is within its rule and
    /// either non-empty or optional (`min == 0`).
    pub fn has_value(&self, step_id: &str) -> bool {
        match self.taxonomy.step(step_id) {
            Some(step) => match &step.text {
                Some(rule) => {
                    let text = self.texts.get(step_id).map_or("", String::as_str);
                    self.text_error(step_id).is_none() && (!text.is_empty() || rule.min == 0)
                }
                None => self.choices.contains_key(step_id),
            },
            None => false,
        }
    }

    /// Length check for a text step against its `TextRule`, counting chars (not bytes).
    /// Missing text counts as empty. Choice steps and unknown ids never error.
    pub fn text_error(&self, step_id: &str) -> Option<TextError> {
        let rule = self.taxonomy.step(step_id)?.text.as_ref()?;
        let count = self
            .texts
            .get(step_id)
            .map_or(0, |text| text.chars().count());
        if count < rule.min {
            Some(TextError::TooShort(rule.min))
        } else if count > rule.max {
            Some(TextError::TooLong(rule.max))
        } else {
            None
        }
    }

    /// Required step ids that still lack a value, in taxonomy order.
    pub fn missing_steps(&self) -> Vec<String> {
        self.step_ids()
            .iter()
            .filter(|id| self.is_required(id) && !self.has_value(id))
            .cloned()
            .collect()
    }

    /// True when nothing is missing and the category is a `report` kind (links never post).
    pub fn can_send(&self) -> bool {
        self.category().is_some_and(|c| c.kind == "report") && self.missing_steps().is_empty()
    }

    /// Advance one screen: `Step(i)` → `Step(i + 1)`, or `Summary` after the last step.
    /// Every other position stays where it is.
    pub fn next(&mut self) {
        if let WizardStep::Step(i) = self.step {
            self.step = if i + 1 >= self.step_ids().len() {
                WizardStep::Summary
            } else {
                WizardStep::Step(i + 1)
            };
        }
    }

    /// Go back one screen: `Summary` → last step, `Step(i > 0)` → `Step(i - 1)`, `Step(0)` → `Pick`.
    /// The category and everything typed are kept.
    pub fn back(&mut self) {
        self.step = match self.step {
            WizardStep::Summary => match self.step_ids().len().checked_sub(1) {
                Some(last) => WizardStep::Step(last),
                None => WizardStep::Pick,
            },
            WizardStep::Step(0) => WizardStep::Pick,
            WizardStep::Step(i) => WizardStep::Step(i - 1),
            ref other => other.clone(),
        };
    }

    /// Record the choice for a choice step.
    pub fn set_choice(&mut self, step_id: &str, choice_id: &str) {
        self.choices
            .insert(step_id.to_string(), choice_id.to_string());
    }

    /// Record the text for a text step.
    pub fn set_text(&mut self, step_id: &str, text: String) {
        self.texts.insert(step_id.to_string(), text);
    }

    /// Choice ids in step order, for choice steps that have a value.
    pub fn path(&self) -> Vec<String> {
        self.step_ids()
            .iter()
            .filter(|id| self.taxonomy.step(id).is_some_and(|s| s.text.is_none()))
            .filter_map(|id| self.choices.get(id).cloned())
            .collect()
    }

    /// `choice.<id>` locale keys for each entry of `path()`, for the report title.
    pub fn choice_label_keys(&self) -> Vec<String> {
        self.path()
            .into_iter()
            .map(|id| format!("choice.{id}"))
            .collect()
    }

    /// The text of the first text step that has something typed, else empty.
    pub fn body(&self) -> String {
        self.step_ids()
            .iter()
            .filter(|id| self.taxonomy.step(id).is_some_and(|s| s.text.is_some()))
            .filter_map(|id| self.texts.get(id))
            .find(|text| !text.is_empty())
            .cloned()
            .unwrap_or_default()
    }

    /// Index (in step order) of the first step that carries a text rule.
    pub fn first_text_step(&self) -> Option<usize> {
        self.step_ids()
            .iter()
            .position(|id| self.taxonomy.step(id).is_some_and(|s| s.text.is_some()))
    }
}

/// About tab state, held as `MainState.feedback`.
#[derive(Debug, Clone, Default)]
pub struct FeedbackState {
    pub loaded: bool,
    pub messages: Vec<LocalMessage>,
    pub last_path: Option<LastPath>,
    /// Empty until the tab first renders (T014 loads embedded/cached), replaced by cache/server when no draft is open.
    pub taxonomy: FeedbackTaxonomy,
    pub taxonomy_fetching: bool,
    /// Arrived while a draft was open; applied once the draft closes.
    pub pending_taxonomy: Option<FeedbackTaxonomy>,
    pub draft: Option<Draft>,
    pub view: AboutView,
    pub view_chosen: bool,
    /// report_id of the expanded row.
    pub expanded: Option<String>,
    /// report_id in flight.
    pub sending: Option<String>,
    pub refreshing: bool,
    pub last_refresh_at: Option<Instant>,
    pub last_refresh_ok: Option<bool>,
    pub last_poll: Option<Instant>,
    pub account: Option<Result<String, ()>>,
    pub account_looking_up: bool,
    /// Built when the summary step opens.
    pub snapshot: Option<BuildSnapshot>,
    /// Messages need saving.
    pub dirty: bool,
    pub was_open: bool,
}

impl FeedbackState {
    /// Open a fresh draft over a frozen copy of the current taxonomy.
    pub fn open_draft(&mut self) {
        self.draft = Some(Draft::new(self.taxonomy.clone()));
    }

    /// Drop the draft, then let a taxonomy that arrived meanwhile take over.
    pub fn close_draft(&mut self) {
        self.draft = None;
        self.apply_pending_taxonomy();
    }

    /// A newer taxonomy replaces the current one only when no draft is open; otherwise it waits
    /// in `pending_taxonomy`. Same or older versions are ignored.
    pub fn offer_taxonomy(&mut self, t: FeedbackTaxonomy) {
        if t.taxonomy_version <= self.taxonomy.taxonomy_version {
            return;
        }
        if self.draft.is_none() {
            self.taxonomy = t;
        } else if self
            .pending_taxonomy
            .as_ref()
            .is_none_or(|p| t.taxonomy_version > p.taxonomy_version)
        {
            self.pending_taxonomy = Some(t);
        }
    }

    /// Move `pending_taxonomy` into `taxonomy` if no draft is open and it is still newer.
    pub fn apply_pending_taxonomy(&mut self) {
        if self.draft.is_some() {
            return;
        }
        if let Some(pending) = self.pending_taxonomy.take() {
            if pending.taxonomy_version > self.taxonomy.taxonomy_version {
                self.taxonomy = pending;
            }
        }
    }

    /// Messages that went to the server (everything except `Local` rows).
    pub fn sent_count(&self) -> usize {
        self.messages.iter().filter(|m| !m.is_local()).count()
    }

    /// Messages the developer has answered.
    pub fn answered_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.status == MessageStatus::Answered)
            .count()
    }

    /// `Messages` once anything was sent, else `WhatsNew`.
    pub fn default_view(&self) -> AboutView {
        if self.sent_count() > 0 {
            AboutView::Messages
        } else {
            AboutView::WhatsNew
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taxonomy() -> FeedbackTaxonomy {
        FeedbackTaxonomy::embedded()
    }

    fn message(status: MessageStatus) -> LocalMessage {
        LocalMessage {
            report_id: uuid::Uuid::new_v4().to_string(),
            short_id: None,
            sent_at: 0,
            category: "bug".to_string(),
            path: Vec::new(),
            title: "t".to_string(),
            body: String::new(),
            status,
            reply: None,
            replied_at: None,
            closing_note: None,
            last_error: None,
            failed_at: None,
            failed_payload: None,
            context_summary: String::new(),
        }
    }

    fn bug_draft() -> Draft {
        let mut draft = Draft::new(taxonomy());
        draft.pick("bug");
        draft
    }

    #[test]
    fn new_draft_mints_uuid_v4_and_freezes_taxonomy() {
        let tax = taxonomy();
        let draft = Draft::new(tax.clone());
        let id = uuid::Uuid::parse_str(&draft.report_id).expect("report_id is a uuid");
        assert_eq!(id.get_version_num(), 4);
        assert_eq!(draft.taxonomy, tax);
        assert_eq!(draft.step, WizardStep::Pick);
        assert_eq!(draft.category, None);
    }

    #[test]
    fn missing_steps_lists_required_steps_in_order() {
        let mut draft = bug_draft();
        assert_eq!(
            draft.missing_steps(),
            vec!["area_screen", "severity", "describe"]
        );

        draft.set_choice("area_screen", "optimize");
        assert_eq!(draft.missing_steps(), vec!["severity", "describe"]);

        let mut praise = Draft::new(taxonomy());
        praise.pick("praise");
        praise.set_choice("liked", "choya");
        assert!(praise.missing_steps().is_empty());
    }

    #[test]
    fn can_send_only_when_nothing_missing() {
        let mut draft = bug_draft();
        assert!(!draft.can_send());
        draft.set_choice("area_screen", "optimize");
        draft.set_choice("severity", "wrong");
        assert!(!draft.can_send());
        draft.set_text("describe", "123456789".to_string());
        assert!(!draft.can_send());
        draft.set_text("describe", "1234567890".to_string());
        assert!(draft.can_send());

        let mut coffee = Draft::new(taxonomy());
        coffee.pick("coffee");
        assert!(coffee.missing_steps().is_empty());
        assert!(!coffee.can_send());
    }

    #[test]
    fn text_validation_min_max() {
        let mut draft = bug_draft();
        draft.set_text("describe", "x".repeat(9));
        assert_eq!(draft.text_error("describe"), Some(TextError::TooShort(10)));
        draft.set_text("describe", "x".repeat(4001));
        assert_eq!(draft.text_error("describe"), Some(TextError::TooLong(4000)));
        draft.set_text("describe", "x".repeat(10));
        assert_eq!(draft.text_error("describe"), None);
        draft.set_text("describe", "é".repeat(10));
        assert_eq!(draft.text_error("describe"), None);

        let mut praise = Draft::new(taxonomy());
        praise.pick("praise");
        praise.set_text("note_optional", String::new());
        assert_eq!(praise.text_error("note_optional"), None);
    }

    #[test]
    fn step_count_and_index() {
        let mut draft = bug_draft();
        assert_eq!(draft.total_steps(), 4);
        assert_eq!(draft.step, WizardStep::Step(0));
        assert_eq!(draft.current_index(), Some(1));
        draft.step = WizardStep::Summary;
        assert_eq!(draft.current_index(), Some(4));
        draft.step = WizardStep::Pick;
        assert_eq!(draft.current_index(), None);

        let fresh = Draft::new(taxonomy());
        assert_eq!(fresh.total_steps(), 1);
        assert!(fresh.step_ids().is_empty());
    }

    #[test]
    fn next_and_back_walk_the_steps() {
        let mut draft = Draft::new(taxonomy());
        draft.next();
        assert_eq!(draft.step, WizardStep::Pick);

        draft.pick("bug");
        assert_eq!(draft.step, WizardStep::Step(0));
        draft.next();
        assert_eq!(draft.step, WizardStep::Step(1));
        draft.next();
        assert_eq!(draft.step, WizardStep::Step(2));
        draft.next();
        assert_eq!(draft.step, WizardStep::Summary);
        draft.next();
        assert_eq!(draft.step, WizardStep::Summary);

        draft.back();
        assert_eq!(draft.step, WizardStep::Step(2));
        draft.back();
        assert_eq!(draft.step, WizardStep::Step(1));
        draft.back();
        assert_eq!(draft.step, WizardStep::Step(0));
        draft.back();
        assert_eq!(draft.step, WizardStep::Pick);
        assert_eq!(draft.category.as_deref(), Some("bug"));
        draft.back();
        assert_eq!(draft.step, WizardStep::Pick);
    }

    #[test]
    fn pick_resets_choices_and_texts() {
        let mut draft = bug_draft();
        draft.set_choice("area_screen", "optimize");
        draft.set_text("describe", "something went wrong".to_string());
        draft.pick("wish");
        assert!(draft.choices.is_empty());
        assert!(draft.texts.is_empty());
        assert_eq!(draft.step, WizardStep::Step(0));
        assert_eq!(draft.step_ids(), ["area_feature", "describe"]);
        assert_eq!(draft.first_text_step(), Some(1));

        draft.pick("coffee");
        assert_eq!(draft.step, WizardStep::Summary);
        assert_eq!(draft.first_text_step(), None);
    }

    #[test]
    fn path_and_body_follow_step_order() {
        let mut draft = bug_draft();
        draft.set_choice("severity", "wrong");
        draft.set_choice("area_screen", "optimize");
        assert_eq!(draft.path(), vec!["optimize", "wrong"]);
        assert_eq!(draft.body(), "");
        draft.set_text("describe", "Optimize picks Trident on land".to_string());
        assert_eq!(draft.body(), "Optimize picks Trident on land");

        let mut praise = Draft::new(taxonomy());
        praise.pick("praise");
        assert!(praise.path().is_empty());
        praise.set_text("note_optional", "nice".to_string());
        assert_eq!(praise.body(), "nice");
    }

    #[test]
    fn same_as_last_prefills_choices_and_jumps_to_first_text_step() {
        let last = LastPath {
            category: "bug".to_string(),
            path: vec!["optimize".to_string(), "wrong".to_string()],
        };
        let draft = Draft::from_last_path(taxonomy(), &last);
        assert_eq!(draft.category.as_deref(), Some("bug"));
        assert_eq!(
            draft.choices.get("area_screen").map(String::as_str),
            Some("optimize")
        );
        assert_eq!(
            draft.choices.get("severity").map(String::as_str),
            Some("wrong")
        );
        assert_eq!(draft.step, WizardStep::Step(2));
        assert_eq!(draft.path(), vec!["optimize", "wrong"]);
        assert_eq!(
            draft.choice_label_keys(),
            vec!["choice.optimize", "choice.wrong"]
        );
        assert_eq!(draft.missing_steps(), vec!["describe"]);

        let short = LastPath {
            category: "bug".to_string(),
            path: vec!["optimize".to_string()],
        };
        let draft = Draft::from_last_path(taxonomy(), &short);
        assert_eq!(draft.step, WizardStep::Step(2));
        assert_eq!(draft.missing_steps(), vec!["severity", "describe"]);
    }

    #[test]
    fn pending_taxonomy_applies_only_when_no_draft() {
        let v1 = taxonomy();
        let mut v2 = taxonomy();
        v2.taxonomy_version = 2;

        let mut state = FeedbackState {
            taxonomy: v1.clone(),
            ..Default::default()
        };
        state.open_draft();
        assert!(state.draft.is_some());
        state.offer_taxonomy(v2.clone());
        assert_eq!(state.taxonomy.taxonomy_version, 1);
        assert_eq!(state.pending_taxonomy, Some(v2.clone()));

        state.close_draft();
        assert!(state.draft.is_none());
        assert_eq!(state.taxonomy, v2);
        assert_eq!(state.pending_taxonomy, None);

        state.offer_taxonomy(v1);
        assert_eq!(state.taxonomy.taxonomy_version, 2);
        assert_eq!(state.pending_taxonomy, None);

        let mut v3 = taxonomy();
        v3.taxonomy_version = 3;
        state.offer_taxonomy(v3.clone());
        assert_eq!(state.taxonomy, v3);
        assert_eq!(state.pending_taxonomy, None);
    }

    #[test]
    fn default_view_and_counts() {
        let mut state = FeedbackState::default();
        assert_eq!(state.default_view(), AboutView::WhatsNew);
        assert_eq!(state.sent_count(), 0);
        assert_eq!(state.answered_count(), 0);

        state.messages.push(message(MessageStatus::Local));
        assert_eq!(state.default_view(), AboutView::WhatsNew);
        assert_eq!(state.sent_count(), 0);

        state.messages.push(message(MessageStatus::Received));
        assert_eq!(state.default_view(), AboutView::Messages);
        assert_eq!(state.sent_count(), 1);
        assert_eq!(state.answered_count(), 0);

        state.messages.push(message(MessageStatus::Answered));
        assert_eq!(state.sent_count(), 2);
        assert_eq!(state.answered_count(), 1);
    }
}
