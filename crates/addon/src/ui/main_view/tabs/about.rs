//! About tab — in-game changelog, Message developer wizard, message list.

pub(super) mod glyphs;
mod wizard;

use nexus::imgui::{ChildWindow, Ui};

use crate::feedback::AboutView;
use crate::state::AddonState;
use crate::ui::theme;
use gw2_core::feedback::changelog::{self, ChangelogEntry};
use gw2_core::feedback::message::{LocalMessage, MessageStatus, MessagesFile};
use gw2_core::feedback::store::FeedbackStore;
use gw2_core::feedback::taxonomy::FeedbackTaxonomy;
use gw2_core::i18n::{t, tf};

/// How many changelog releases the What's new view shows.
const CHANGELOG_SHOWN: usize = 5;

/// Messages once a row was actually sent; otherwise the release notes.
fn default_view(messages: &[LocalMessage]) -> AboutView {
    if messages.iter().any(|m| !m.is_local()) {
        AboutView::Messages
    } else {
        AboutView::WhatsNew
    }
}

/// Rows that went to the server (everything that is not `Local`).
fn sent_count(messages: &[LocalMessage]) -> usize {
    messages.iter().filter(|m| !m.is_local()).count()
}

fn answered_count(messages: &[LocalMessage]) -> usize {
    messages
        .iter()
        .filter(|m| m.status == MessageStatus::Answered)
        .count()
}

/// The newest [`CHANGELOG_SHOWN`] releases bundled into the DLL.
fn changelog_entries() -> Vec<ChangelogEntry> {
    let mut entries = changelog::parse(changelog::EMBEDDED);
    entries.truncate(CHANGELOG_SHOWN);
    entries
}

fn load_feedback(state: &mut AddonState) {
    let store = FeedbackStore::new(&state.addon_dir);
    let file = store.load();
    let mut taxonomy = FeedbackTaxonomy::embedded();
    if let Some(cached) = store.load_taxonomy() {
        if cached.taxonomy_version > taxonomy.taxonomy_version {
            taxonomy = cached;
        }
    }
    let feedback = &mut state.main.feedback;
    feedback.messages = file.messages;
    feedback.last_path = file.last_path;
    feedback.taxonomy = taxonomy;
    feedback.loaded = true;
    if !feedback.view_chosen {
        feedback.view = default_view(&feedback.messages);
    }
}

fn render_about_hero(ui: &Ui, state: &AddonState) {
    const MASCOT: f32 = 96.0;
    const PAD_L: f32 = 20.0;
    const PAD_T: f32 = 38.0;
    const PAD_R: f32 = 24.0;
    const PAD_B: f32 = 8.0;
    let box_w = PAD_L + MASCOT + PAD_R;
    let box_h = PAD_T + MASCOT + PAD_B;
    let top = ui.cursor_screen_pos();
    ui.invisible_button("##about_mascot", [box_w, box_h]);
    let below = ui.cursor_screen_pos();
    let center = [top[0] + PAD_L + MASCOT * 0.5, top[1] + PAD_T + MASCOT * 0.5];
    theme::draw_choya_hero(ui, center, MASCOT);

    let text_x = top[0] + box_w + 8.0;
    let ty0 = top[1] + PAD_T + 10.0;
    let lh = ui.text_line_height();
    ui.set_cursor_screen_pos([text_x, ty0]);
    ui.text_colored(theme::GOLD, t("about.title"));

    ui.set_cursor_screen_pos([text_x, ty0 + lh + 6.0]);
    let sub = format!(
        "{}  ·  {}  ·  {}",
        t("info.product"),
        tf("fmt.version", &[("ver", crate::VERSION)]),
        tf(
            "fmt.ai",
            &[("provider", state.config.active_provider.label())]
        ),
    );
    ui.text_colored(theme::CREAM, sub);

    ui.set_cursor_screen_pos([text_x, ty0 + lh * 2.0 + 12.0]);
    ui.text_colored(theme::MUTED, t("about.tagline"));

    let messages = &state.main.feedback.messages;
    ui.set_cursor_screen_pos([text_x, ty0 + lh * 3.0 + 16.0]);
    ui.text_colored(
        theme::MUTED,
        tf(
            "about.counts",
            &[
                ("sent", &sent_count(messages).to_string()),
                ("answered", &answered_count(messages).to_string()),
            ],
        ),
    );

    let after = ui.cursor_screen_pos();
    ui.set_cursor_screen_pos([top[0], below[1].max(after[1] + 8.0)]);
}

fn render_action_row(ui: &Ui, state: &mut AddonState) {
    ui.dummy([0.0, 10.0]);
    let msg_label = t("about.btn.message");
    let coffee_label = t("about.btn.coffee");
    let btn_w = theme::gold_button_width(ui, &msg_label)
        .max(theme::gold_button_width(ui, &coffee_label))
        .max(160.0);
    if theme::gold_button_sized(ui, format!("{msg_label}##about_msg"), [btn_w, 0.0])
        && state.main.feedback.draft.is_none()
    {
        state.main.feedback.open_draft();
    }
    ui.same_line_with_spacing(0.0, 10.0);
    if theme::gold_button_sized(ui, format!("{coffee_label}##about_coffee"), [btn_w, 0.0]) {
        let url = state
            .main
            .feedback
            .taxonomy
            .categories
            .iter()
            .find(|c| c.kind == "link")
            .and_then(|c| c.url.clone());
        if let Some(url) = url {
            wizard::coffee(state, &url);
        }
    }
}

fn render_view_toggle(ui: &Ui, state: &mut AddonState) {
    ui.dummy([0.0, 12.0]);
    let labels = [t("about.view.messages"), t("about.view.whats_new")];
    let refs = [labels[0].as_str(), labels[1].as_str()];
    let selected = match state.main.feedback.view {
        AboutView::Messages => 0,
        AboutView::WhatsNew => 1,
    };
    // `segment_row` fills the available width; a two-way toggle across the
    // whole content pane looks like a title bar, so box it like the Saves row.
    let row_w = theme::segment_row_min_width(ui, &refs) * 1.6;
    let row_h = ui.text_line_height() + 8.0;
    let mut picked = None;
    ChildWindow::new("##about_view_wrap")
        .size([row_w, row_h])
        .build(ui, || {
            picked = theme::segment_row(ui, &refs, selected, "##about_view");
        });
    if let Some(i) = picked {
        let feedback = &mut state.main.feedback;
        feedback.view = if i == 0 {
            AboutView::Messages
        } else {
            AboutView::WhatsNew
        };
        feedback.view_chosen = true;
    }
}

fn render_whats_new(ui: &Ui, state: &AddonState) {
    let base = state.config.font_scale;
    let entries = changelog_entries();
    let scroll_h = (ui.content_region_avail()[1] - 4.0).max(64.0);
    ChildWindow::new("##about_changelog")
        .size([0.0, scroll_h])
        .build(ui, || {
            if entries.is_empty() {
                ui.text_colored(theme::MUTED, t("about.no_changelog"));
                return;
            }
            for e in &entries {
                ui.text_colored(theme::CREAM, format!("{}  ·  {}", e.version, e.date));
                ui.set_window_font_scale(base * 0.85);
                theme::wrapped(ui, theme::MUTED, &e.body);
                ui.set_window_font_scale(base);
                ui.dummy([0.0, 10.0]);
            }
        });
}

fn render_messages(ui: &Ui, state: &AddonState) {
    let messages = &state.main.feedback.messages;
    if messages.is_empty() {
        ui.dummy([0.0, 8.0]);
        ui.text_colored(theme::MUTED, t("about.empty"));
        ui.text_colored(theme::MUTED, t("about.empty_hint"));
        return;
    }
    // T025: messages table
    for m in messages {
        ui.text_colored(theme::MUTED, format!("{}  ·  {:?}", m.title, m.status));
    }
}

/// Render the About tab.
pub(in crate::ui::main_view) fn render_about_tab(ui: &Ui, state: &mut AddonState) {
    if !state.main.feedback.loaded {
        load_feedback(state);
    }

    render_about_hero(ui, state);
    render_action_row(ui, state);
    if state.main.feedback.draft.is_some() {
        wizard::render_wizard(ui, state);
    }
    render_view_toggle(ui, state);

    ui.dummy([0.0, 10.0]);
    match state.main.feedback.view {
        AboutView::WhatsNew => render_whats_new(ui, state),
        AboutView::Messages => render_messages(ui, state),
    }
    if state.main.feedback.dirty {
        let file = MessagesFile {
            last_path: state.main.feedback.last_path.clone(),
            messages: state.main.feedback.messages.clone(),
        };
        if let Err(e) = FeedbackStore::new(&state.addon_dir).save(&file) {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                format!("messages.json save failed: {e}"),
            );
        }
        state.main.feedback.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::AboutView;
    use gw2_core::feedback::message::{LocalMessage, MessageStatus};

    fn msg(status: MessageStatus) -> LocalMessage {
        LocalMessage {
            report_id: "r".into(),
            short_id: None,
            sent_at: 0,
            category: "bug".into(),
            path: vec![],
            title: "t".into(),
            body: "b".into(),
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

    #[test]
    fn default_view_is_whats_new_without_sent_rows() {
        assert_eq!(default_view(&[]), AboutView::WhatsNew);
        assert_eq!(
            default_view(&[msg(MessageStatus::Local)]),
            AboutView::WhatsNew
        );
    }

    #[test]
    fn default_view_is_messages_once_a_row_was_sent() {
        assert_eq!(
            default_view(&[msg(MessageStatus::Local), msg(MessageStatus::Failed)]),
            AboutView::Messages
        );
    }

    #[test]
    fn sent_count_excludes_local_rows() {
        let rows = [
            msg(MessageStatus::Local),
            msg(MessageStatus::Received),
            msg(MessageStatus::Answered),
            msg(MessageStatus::Failed),
        ];
        assert_eq!(sent_count(&[]), 0);
        assert_eq!(sent_count(&rows), 3);
    }

    #[test]
    fn answered_count_counts_only_answered() {
        let rows = [
            msg(MessageStatus::Answered),
            msg(MessageStatus::Closed),
            msg(MessageStatus::Answered),
            msg(MessageStatus::Local),
        ];
        assert_eq!(answered_count(&[]), 0);
        assert_eq!(answered_count(&rows), 2);
    }

    #[test]
    fn changelog_entries_are_the_first_five() {
        let entries = changelog_entries();
        assert_eq!(entries.len(), 5);
        let all = gw2_core::feedback::changelog::parse(gw2_core::feedback::changelog::EMBEDDED);
        assert_eq!(entries[0], all[0]);
        assert!(entries.iter().all(|e| !e.version.is_empty()));
    }
}
