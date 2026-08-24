//! Message developer wizard — the step runner over [`Draft`] shown inside the
//! About tab below the action row: pick tiles, chip steps, text steps, the
//! summary card, and the sent/thanks plates. The label helpers at the top are
//! pure and unit-tested; everything ImGui sits below them.

use std::sync::atomic::{AtomicU32, Ordering};

use nexus::imgui::{StyleVar, Ui};

use super::glyphs::{category_color, draw_glyph};
use crate::feedback::{Draft, FeedbackState, TextError, WizardStep};
use crate::state::AddonState;
use crate::ui::{color_u32, theme};
use gw2_core::feedback::message::{now_unix, FailReason, LocalMessage, MessageStatus};
use gw2_core::feedback::report::{snapshot_bytes, MAX_REQUEST_BYTES, MAX_SNAPSHOT_BYTES};
use gw2_core::feedback::taxonomy::FeedbackTaxonomy;
use gw2_core::i18n::{t, tf};

/// Plate padding on every side.
const PAD: f32 = 12.0;
/// Choya avatar size in the plate's left column.
const AVATAR: f32 = 48.0;
/// Gap between tiles, chips, and footer buttons.
const GAP: f32 = 8.0;
const ROUNDING: f32 = 6.0;
/// `Reach me` input cap (chars).
const CONTACT_MAX: usize = 200;

/// Plate height measured last frame, stored as `f32` bits. The plate has to be
/// painted before its content and the draw list cannot be channel-split here
/// because `select_chip` / `draw_choya_avatar` re-borrow it, so the height lags
/// one frame — invisible except on the very first frame after opening.
static PLATE_H: AtomicU32 = AtomicU32::new(0);

// ── pure label helpers ───────────────────────────────────────────────────────

/// `Report a bug › Optimize › Wrong result`: the category label followed by
/// the `choice.<id>` label of every entry in `path`.
fn path_label(taxonomy: &FeedbackTaxonomy, category: &str, path: &[String]) -> String {
    let mut out = taxonomy
        .category(category)
        .map_or_else(|| category.to_string(), |c| t(&c.label));
    for id in path {
        out.push_str(" › ");
        out.push_str(&t(&format!("choice.{id}")));
    }
    out
}

/// [`path_label`] for the draft's picked category and current `path()`; empty before a pick.
fn path_text(draft: &Draft) -> String {
    match draft.category.as_deref() {
        Some(cat) => path_label(&draft.taxonomy, cat, &draft.path()),
        None => String::new(),
    }
}

/// The step's translated prompt, or the raw id when the frozen taxonomy does not define it.
fn step_prompt(taxonomy: &FeedbackTaxonomy, step_id: &str) -> String {
    taxonomy
        .step(step_id)
        .map_or_else(|| step_id.to_string(), |s| t(&s.prompt))
}

/// `about.missing` with the prompts of every required step that still lacks a value.
fn missing_text(draft: &Draft) -> String {
    let steps: Vec<String> = draft
        .missing_steps()
        .iter()
        .map(|id| step_prompt(&draft.taxonomy, id))
        .collect();
    tf("about.missing", &[("steps", &steps.join(", "))])
}

/// `about.too_big` with the measured request size and the `MAX_REQUEST_BYTES` cap.
fn too_big_text(bytes: usize) -> String {
    tf(
        "about.too_big",
        &[
            ("bytes", &bytes.to_string()),
            ("max", &MAX_REQUEST_BYTES.to_string()),
        ],
    )
}

/// Locale key for Choya's optional aside on a step.
fn quip_key(cat: &str, step: &str) -> String {
    format!("quip.{cat}.{step}")
}

/// Choya's aside for a step, only when the catalog actually has one.
fn quip_for(cat: &str, step: &str) -> Option<String> {
    let key = quip_key(cat, step);
    let text = t(&key);
    (text != key).then_some(text)
}

fn text_error_text(e: &TextError) -> String {
    match e {
        TextError::TooShort(min) => tf("about.too_short", &[("min", &min.to_string())]),
        TextError::TooLong(max) => tf("about.too_long", &[("max", &max.to_string())]),
    }
}

/// Why `Next` is dimmed on a step, or `None` when the step may be left.
fn next_block_text(draft: &Draft, step_id: &str) -> Option<String> {
    if !draft.is_required(step_id) || draft.has_value(step_id) {
        return None;
    }
    Some(match draft.text_error(step_id) {
        Some(e) => text_error_text(&e),
        None => tf(
            "about.missing",
            &[("steps", &step_prompt(&draft.taxonomy, step_id))],
        ),
    })
}

/// Player-facing text for a send failure (`msg.fail.*`). `Rejected` shows the
/// server's reason verbatim; `RateLimited` rounds the wait up to whole minutes.
pub(super) fn fail_text(r: &FailReason) -> String {
    match r {
        FailReason::Network => t("msg.fail.network"),
        FailReason::Server => t("msg.fail.server"),
        FailReason::Timeout => t("msg.fail.timeout"),
        FailReason::RateLimited { retry_after_secs } => tf(
            "msg.fail.rate",
            &[("n", &retry_after_secs.div_ceil(60).to_string())],
        ),
        FailReason::TooLarge => t("msg.fail.too_large"),
        FailReason::Rejected { reason } => reason.clone(),
        FailReason::TooOld => t("msg.fail.too_old"),
        FailReason::Interrupted => t("msg.fail.interrupted"),
    }
}

// ── rendering ────────────────────────────────────────────────────────────────

/// What the player did this frame; applied after every widget has rendered so
/// no `&Draft` is alive while the state is mutated.
enum Action {
    None,
    Pick(String),
    Choice(String, String),
    Text(String, String),
    Next,
    Back,
    Cancel,
    Coffee(String),
    Send,
    ToggleBuild(bool),
    ToggleAccount(bool),
    Contact(String),
    Done,
    SameAsLast,
}

fn fade(c: [f32; 4], alpha: f32) -> [f32; 4] {
    [c[0], c[1], c[2], c[3] * alpha]
}

/// `text_wrapped` pinned to an absolute right edge (the plate's inner edge).
fn wrapped_to(ui: &Ui, color: [f32; 4], text: &str, right_x: f32) {
    let wrap = ui.push_text_wrap_pos_with_pos(right_x);
    ui.text_colored(color, text);
    wrap.pop(ui);
}

/// Gold button drawn inert at 40 % with a tooltip saying why.
fn dimmed_gold(ui: &Ui, label: impl AsRef<str>, tip: &str) {
    let style = ui.push_style_var(StyleVar::Alpha(0.4));
    theme::gold_button_sized(ui, label, [0.0, 0.0]);
    style.pop();
    if ui.is_item_hovered() {
        ui.tooltip_text(tip);
    }
}

fn done_button(ui: &Ui) -> Action {
    ui.dummy([0.0, 6.0]);
    if theme::gold_button_sized(ui, format!("{}##wz_done", t("about.btn.done")), [0.0, 0.0]) {
        Action::Done
    } else {
        Action::None
    }
}

/// Render the open draft inside a plate under the action row. No-op without a draft.
pub(super) fn render_wizard(ui: &Ui, state: &mut AddonState) {
    let feedback = &state.main.feedback;
    let Some(draft) = feedback.draft.as_ref() else {
        return;
    };
    ui.dummy([0.0, 10.0]);
    let origin = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    let min_h = AVATAR + PAD * 2.0;
    let plate_h = f32::from_bits(PLATE_H.load(Ordering::Relaxed)).max(min_h);
    {
        let dl = ui.get_window_draw_list();
        let br = [origin[0] + width, origin[1] + plate_h];
        dl.add_rect(origin, br, theme::PLATE)
            .filled(true)
            .rounding(ROUNDING)
            .build();
        dl.add_rect(origin, br, theme::GOLD_DIM)
            .rounding(ROUNDING)
            .build();
    }
    theme::draw_choya_avatar(
        ui,
        [
            origin[0] + PAD + AVATAR * 0.5,
            origin[1] + PAD + AVATAR * 0.5,
        ],
        AVATAR,
    );

    let indent = PAD + AVATAR + 10.0;
    let inner_w = (width - indent - PAD).max(80.0);
    let right_x = origin[0] + width - PAD;
    ui.indent_by(indent);
    ui.set_cursor_screen_pos([origin[0] + indent, origin[1] + PAD + 4.0]);

    let action = match draft.step.clone() {
        WizardStep::Pick => render_pick(ui, draft, feedback, inner_w),
        WizardStep::Step(i) => render_step(ui, draft, i, inner_w),
        WizardStep::Summary => {
            // Measured every summary frame so the Send gate tracks the boxes and contact.
            let bytes = crate::feedback::tasks::draft_request_bytes(state);
            render_summary(ui, draft, feedback, bytes, inner_w, right_x)
        }
        WizardStep::Sending => {
            ui.text_colored(theme::MUTED, t("msg.status.sending"));
            Action::None
        }
        WizardStep::Sent { short_id } => {
            ui.text_colored(
                theme::OPTIMIZED,
                tf("about.sent_plate", &[("id", &short_id)]),
            );
            done_button(ui)
        }
        WizardStep::Thanks => {
            ui.text_colored(theme::GOLD, t("about.thanks"));
            done_button(ui)
        }
    };

    ui.unindent_by(indent);
    let measured = (ui.cursor_screen_pos()[1] + PAD - origin[1]).max(min_h);
    PLATE_H.store(measured.to_bits(), Ordering::Relaxed);
    ui.set_cursor_screen_pos([origin[0], origin[1] + measured.max(plate_h)]);

    apply(state, action);
}

fn render_pick(ui: &Ui, draft: &Draft, feedback: &FeedbackState, inner_w: f32) -> Action {
    ui.text_colored(theme::CREAM, t("step.pick"));
    ui.dummy([0.0, 4.0]);

    let cats = &draft.taxonomy.categories;
    let labels: Vec<String> = cats.iter().map(|c| t(&c.label)).collect();
    let widest = labels
        .iter()
        .map(|l| ui.calc_text_size(l)[0])
        .fold(0.0, f32::max);
    let tile_w = (widest + 44.0).max(150.0);
    let tile_h = theme::control_height(ui) + 12.0;

    let mut action = Action::None;
    let mut row_x = 0.0;
    for (cat, label) in cats.iter().zip(&labels) {
        theme::wrap_chip(ui, inner_w, &mut row_x, tile_w, GAP);
        let kind = cat.kind.as_str();
        let live = matches!(kind, "report" | "link");
        let alpha = if live { 1.0 } else { 0.4 };
        let style = (!live).then(|| ui.push_style_var(StyleVar::Alpha(0.4)));
        let p = ui.cursor_screen_pos();
        let clicked = ui.invisible_button(format!("##wz_tile_{}", cat.id), [tile_w, tile_h]);
        let hovered = ui.is_item_hovered();
        {
            let dl = ui.get_window_draw_list();
            let br = [p[0] + tile_w, p[1] + tile_h];
            let fill = if hovered && live {
                theme::GOLD_HOVER
            } else {
                [0.12, 0.10, 0.07, 0.9]
            };
            dl.add_rect(p, br, fade(fill, alpha))
                .filled(true)
                .rounding(ROUNDING)
                .build();
            dl.add_rect(p, br, fade(theme::GOLD_DIM, alpha))
                .rounding(ROUNDING)
                .build();
            draw_glyph(
                ui,
                &dl,
                &cat.icon,
                [p[0] + 17.0, p[1] + tile_h * 0.5],
                18.0,
                fade(category_color(&cat.color), alpha),
            );
            let th = ui.calc_text_size(label)[1];
            dl.add_text(
                [p[0] + 36.0, p[1] + ((tile_h - th) * 0.5).round()],
                color_u32(fade(theme::CREAM, alpha)),
                label,
            );
        }
        if let Some(style) = style {
            style.pop();
        }
        match kind {
            "report" => {
                if clicked {
                    action = Action::Pick(cat.id.clone());
                }
            }
            "link" => {
                if clicked {
                    if let Some(url) = &cat.url {
                        action = Action::Coffee(url.clone());
                    }
                }
            }
            _ => {
                if hovered {
                    ui.tooltip_text(t("about.needs_newer"));
                }
            }
        }
    }

    ui.dummy([0.0, 4.0]);
    if let Some(last) = feedback
        .last_path
        .as_ref()
        .filter(|l| draft.taxonomy.category(&l.category).is_some())
    {
        let label = tf(
            "about.same_as_last",
            &[(
                "path",
                &path_label(&draft.taxonomy, &last.category, &last.path),
            )],
        );
        if theme::select_chip(ui, &label, false, "##wz_same_as_last", None) {
            action = Action::SameAsLast;
        }
        ui.same_line_with_spacing(0.0, GAP);
    }
    if ui.button(format!("{}##wz_cancel", t("about.btn.cancel"))) {
        action = Action::Cancel;
    }
    action
}

fn render_step(ui: &Ui, draft: &Draft, i: usize, inner_w: f32) -> Action {
    let step_id = draft.step_id(i).unwrap_or_default().to_string();
    let step = draft.taxonomy.step(&step_id).cloned();
    let cat_id = draft.category.as_deref().unwrap_or_default();

    ui.text_colored(theme::CREAM, step_prompt(&draft.taxonomy, &step_id));
    if let Some(quip) = quip_for(cat_id, &step_id) {
        ui.text_colored(theme::MUTED, quip);
    }
    ui.dummy([0.0, 4.0]);

    let mut action = Action::None;
    if let Some(step) = &step {
        if !step.choices.is_empty() {
            let selected = draft.choices.get(&step_id).map(String::as_str);
            let mut row_x = 0.0;
            for c in &step.choices {
                let label = t(&format!("choice.{c}"));
                let w = theme::select_chip_size(ui, &label, false)[0];
                theme::wrap_chip(ui, inner_w, &mut row_x, w, GAP);
                if theme::select_chip(
                    ui,
                    &label,
                    selected == Some(c.as_str()),
                    &format!("##wz_{step_id}_{c}"),
                    None,
                ) {
                    action = Action::Choice(step_id.clone(), c.clone());
                }
            }
        }
        if let Some(rule) = &step.text {
            let stored = draft.texts.get(&step_id).cloned().unwrap_or_default();
            let mut buf = stored.clone();
            let h = ui.text_line_height() * 5.0 + theme::control_pad(ui)[1] * 2.0;
            ui.input_text_multiline(format!("##wz_text_{step_id}"), &mut buf, [inner_w, h])
                .build();
            if buf != stored {
                action = Action::Text(step_id.clone(), buf.clone());
            }
            ui.text_colored(
                theme::MUTED,
                format!("{}/{}", buf.chars().count(), rule.max),
            );
            if let Some(e) = draft.text_error(&step_id) {
                ui.text_colored(theme::WARN, text_error_text(&e));
            }
        }
    }

    ui.dummy([0.0, 6.0]);
    if ui.button(format!("{}##wz_back", t("about.btn.back"))) {
        action = Action::Back;
    }
    ui.same_line_with_spacing(0.0, GAP);
    ui.align_text_to_frame_padding();
    let n = draft.current_index().unwrap_or(i + 1);
    ui.text_colored(
        theme::MUTED,
        tf(
            "about.step_n",
            &[
                ("n", &n.to_string()),
                ("m", &draft.total_steps().to_string()),
            ],
        ),
    );
    ui.same_line_with_spacing(0.0, GAP);
    let next_label = format!("{}##wz_next", t("about.btn.next"));
    match next_block_text(draft, &step_id) {
        Some(tip) => dimmed_gold(ui, next_label, &tip),
        None => {
            if theme::gold_button_sized(ui, next_label, [0.0, 0.0]) {
                action = Action::Next;
            }
        }
    }
    action
}

fn render_summary(
    ui: &Ui,
    draft: &Draft,
    feedback: &FeedbackState,
    request_bytes: Option<usize>,
    inner_w: f32,
    right_x: f32,
) -> Action {
    let mut action = Action::None;
    let cat = draft.category().cloned();

    let p = ui.cursor_screen_pos();
    if let Some(cat) = &cat {
        let dl = ui.get_window_draw_list();
        draw_glyph(
            ui,
            &dl,
            &cat.icon,
            [p[0] + 8.0, p[1] + ui.text_line_height() * 0.5],
            16.0,
            category_color(&cat.color),
        );
    }
    ui.set_cursor_screen_pos([p[0] + 22.0, p[1]]);
    ui.text_colored(theme::CREAM, path_text(draft));
    let body = draft.body();
    if !body.is_empty() {
        wrapped_to(ui, theme::CREAM, &body, right_x);
    }

    ui.dummy([0.0, 4.0]);
    ui.text_colored(theme::MUTED, t("about.attached"));
    ui.same_line_with_spacing(0.0, GAP);
    ui.text_colored(
        theme::CREAM,
        format!("v{}  ·  {}", crate::VERSION, gw2_core::i18n::current()),
    );

    if cat.as_ref().is_some_and(|c| c.attach_build) {
        let label = format!("{}##wz_build", t("about.include_build"));
        let fits = feedback
            .snapshot
            .as_ref()
            .is_some_and(|s| snapshot_bytes(s) <= MAX_SNAPSHOT_BYTES);
        if fits {
            let mut v = draft.include_build;
            if ui.checkbox(label, &mut v) {
                action = Action::ToggleBuild(v);
            }
        } else {
            let style = ui.push_style_var(StyleVar::Alpha(0.4));
            let mut off = false;
            ui.checkbox(label, &mut off);
            style.pop();
            if ui.is_item_hovered() {
                ui.tooltip_text(t("about.include_build_big"));
            }
        }
    }

    if feedback.account != Some(Err(())) {
        let mut v = draft.include_account;
        if ui.checkbox(
            format!("{}##wz_account", t("about.include_account")),
            &mut v,
        ) {
            action = Action::ToggleAccount(v);
        }
        if feedback.account_looking_up {
            ui.same_line_with_spacing(0.0, GAP);
            ui.text_colored(theme::MUTED, t("about.account_lookup"));
        } else if let Some(Ok(name)) = &feedback.account {
            ui.same_line_with_spacing(0.0, GAP);
            ui.text_colored(theme::CREAM, name);
        }
    }

    ui.align_text_to_frame_padding();
    ui.text_colored(theme::MUTED, t("about.reach_me"));
    ui.same_line_with_spacing(0.0, GAP);
    let mut contact = draft.contact.clone();
    ui.set_next_item_width((inner_w * 0.6).max(120.0));
    ui.input_text("##wz_contact", &mut contact)
        .hint(&t("about.reach_hint"))
        .build();
    if contact.chars().count() > CONTACT_MAX {
        contact = contact.chars().take(CONTACT_MAX).collect();
    }
    if contact != draft.contact {
        action = Action::Contact(contact);
    }

    if let Some(err) = &draft.error {
        ui.dummy([0.0, 4.0]);
        wrapped_to(ui, theme::ERR, &fail_text(err), right_x);
    }

    ui.dummy([0.0, 6.0]);
    if ui.button(format!("{}##wz_back", t("about.btn.back"))) {
        action = Action::Back;
    }
    ui.same_line_with_spacing(0.0, GAP);
    if ui.button(format!("{}##wz_cancel", t("about.btn.cancel"))) {
        action = Action::Cancel;
    }
    ui.same_line_with_spacing(0.0, GAP);
    let send_label = format!("{}##wz_send", t("about.btn.send"));
    if !draft.can_send() {
        dimmed_gold(ui, send_label, &missing_text(draft));
    } else if let Some(bytes) = request_bytes.filter(|b| *b > MAX_REQUEST_BYTES) {
        dimmed_gold(ui, send_label, &too_big_text(bytes));
    } else if theme::gold_button_sized(ui, send_label, [0.0, 0.0]) {
        action = Action::Send;
    }
    action
}

// ── apply phase ──────────────────────────────────────────────────────────────

/// Run `f` on the open draft; `R::default()` when none is open.
fn with_draft<R: Default>(state: &mut AddonState, f: impl FnOnce(&mut Draft) -> R) -> R {
    state
        .main
        .feedback
        .draft
        .as_mut()
        .map_or_else(R::default, f)
}

fn apply(state: &mut AddonState, action: Action) {
    match action {
        Action::None => {}
        Action::Pick(id) => {
            let summary = with_draft(state, |d| {
                d.pick(&id);
                d.step == WizardStep::Summary
            });
            if summary {
                on_enter_summary(state);
            }
        }
        Action::Choice(step_id, choice) => {
            let summary = with_draft(state, |d| {
                d.set_choice(&step_id, &choice);
                d.next();
                d.step == WizardStep::Summary
            });
            if summary {
                on_enter_summary(state);
            }
        }
        Action::Text(step_id, text) => with_draft(state, |d| d.set_text(&step_id, text)),
        Action::Next => {
            let summary = with_draft(state, |d| {
                d.next();
                d.step == WizardStep::Summary
            });
            if summary {
                on_enter_summary(state);
            }
        }
        Action::Back => with_draft(state, |d| d.back()),
        Action::Cancel | Action::Done => state.main.feedback.close_draft(),
        Action::SameAsLast => {
            let feedback = &mut state.main.feedback;
            let Some(last) = feedback.last_path.clone() else {
                return;
            };
            let Some(taxonomy) = feedback.draft.as_ref().map(|d| d.taxonomy.clone()) else {
                return;
            };
            let draft = Draft::from_last_path(taxonomy, &last);
            let summary = draft.step == WizardStep::Summary;
            feedback.draft = Some(draft);
            if summary {
                on_enter_summary(state);
            }
        }
        Action::Coffee(url) => coffee(state, &url),
        Action::Send => on_send(state),
        Action::ToggleBuild(v) => with_draft(state, |d| d.include_build = v),
        Action::ToggleAccount(v) => {
            with_draft(state, |d| d.include_account = v);
            if v {
                on_account_tick(state);
            }
        }
        Action::Contact(contact) => with_draft(state, |d| d.contact = contact),
    }
}

// ── hooks (later tasks replace the bodies) ───────────────────────────────────

/// Send the draft: row, lock, background post (`feedback::tasks::send_draft`).
pub(super) fn on_send(state: &mut AddonState) {
    crate::feedback::tasks::send_draft(state);
}

/// The wizard just landed on the summary step: rebuild the attachable snapshot
/// from the selected suggestion. Split borrow: both are fields of `state.main`.
pub(super) fn on_enter_summary(state: &mut AddonState) {
    let (feedback, comparison) = (&mut state.main.feedback, &state.main.comparison);
    feedback.refresh_snapshot(comparison);
}

/// The player ticked "include my GW2 account name".
pub(super) fn on_account_tick(state: &mut AddonState) {
    crate::feedback::tasks::lookup_account(state);
}

/// Open the Ko-fi page and remember it as a local `about.coffee_row` message.
/// No server call, no dialog on failure (a Warning log only). A draft still on
/// the pick step closes; anything further along is left alone.
pub(super) fn coffee(state: &mut AddonState, url: &str) {
    if !crate::feedback::shell::open_url(url) {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            format!("could not open {url}"),
        );
    }
    let row = LocalMessage {
        report_id: uuid::Uuid::new_v4().to_string(),
        short_id: None,
        sent_at: now_unix(),
        category: "coffee".to_string(),
        path: vec![],
        title: t("about.coffee_row"),
        body: String::new(),
        status: MessageStatus::Local,
        reply: None,
        replied_at: None,
        closing_note: None,
        last_error: None,
        failed_at: None,
        failed_payload: None,
        context_summary: String::new(),
    };
    let feedback = &mut state.main.feedback;
    feedback.messages.insert(0, row);
    feedback.dirty = true;
    if feedback
        .draft
        .as_ref()
        .is_some_and(|d| d.step == WizardStep::Pick)
    {
        feedback.close_draft();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `set_language` is process-global; serialise the tests that depend on `en`.
    static LANG: Mutex<()> = Mutex::new(());

    fn with_en<R>(f: impl FnOnce() -> R) -> R {
        let _g = LANG.lock().unwrap_or_else(|e| e.into_inner());
        gw2_core::i18n::set_language("en");
        f()
    }

    fn bug_draft() -> Draft {
        let mut d = Draft::new(FeedbackTaxonomy::embedded());
        d.pick("bug");
        d
    }

    #[test]
    fn path_text_joins_category_and_choice_labels() {
        with_en(|| {
            let mut d = bug_draft();
            assert_eq!(path_text(&d), "Report a bug");
            d.set_choice("area_screen", "optimize");
            d.set_choice("severity", "wrong");
            assert_eq!(path_text(&d), "Report a bug › Optimize › Wrong result");

            let none = Draft::new(FeedbackTaxonomy::embedded());
            assert_eq!(path_text(&none), "");
        });
    }

    #[test]
    fn path_label_falls_back_to_raw_ids() {
        with_en(|| {
            let tax = FeedbackTaxonomy::embedded();
            assert_eq!(
                path_label(&tax, "vote", &["rocket".to_string()]),
                "vote › choice.rocket"
            );
        });
    }

    #[test]
    fn missing_text_lists_step_prompts_in_order() {
        with_en(|| {
            let mut d = bug_draft();
            assert_eq!(
                missing_text(&d),
                "Missing: Where did it happen?, How bad is it?, Tell Choya what happened"
            );
            d.set_choice("area_screen", "optimize");
            d.set_choice("severity", "wrong");
            d.set_text("describe", "Optimize picks Trident on land.".into());
            assert_eq!(missing_text(&d), "Missing: ");
        });
    }

    #[test]
    fn too_big_text_carries_bytes_and_cap() {
        with_en(|| {
            let text = too_big_text(17_234);
            assert!(text.contains("17234"), "{text}");
            assert!(text.contains(&MAX_REQUEST_BYTES.to_string()), "{text}");
            assert!(!text.contains('{'), "{text}");
        });
    }

    #[test]
    fn quip_key_shape() {
        assert_eq!(quip_key("bug", "area_screen"), "quip.bug.area_screen");
    }

    #[test]
    fn quip_for_is_none_without_a_catalog_entry() {
        with_en(|| {
            // The English catalog ships no quips yet: `t` echoes the key back.
            let key = quip_key("bug", "area_screen");
            assert_eq!(t(&key), key);
            assert_eq!(quip_for("bug", "area_screen"), None);
            // Sanity: the same `t(key) != key` rule does resolve a key the catalog has.
            assert_ne!(t("step.pick"), "step.pick");
        });
    }

    #[test]
    fn next_block_text_only_when_required_and_empty() {
        with_en(|| {
            let mut d = bug_draft();
            assert_eq!(
                next_block_text(&d, "severity").as_deref(),
                Some("Missing: How bad is it?")
            );
            assert_eq!(
                next_block_text(&d, "describe").as_deref(),
                Some("At least 10 characters")
            );
            d.set_choice("severity", "wrong");
            assert_eq!(next_block_text(&d, "severity"), None);
            d.set_text("describe", "x".repeat(4001));
            assert_eq!(
                next_block_text(&d, "describe").as_deref(),
                Some("At most 4000 characters")
            );
            let mut praise = Draft::new(FeedbackTaxonomy::embedded());
            praise.pick("praise");
            assert_eq!(next_block_text(&praise, "note_optional"), None);
        });
    }

    #[test]
    fn fail_text_maps_reasons() {
        with_en(|| {
            assert_eq!(
                fail_text(&FailReason::Network),
                "Couldn't reach Choya. Check your connection."
            );
            assert_eq!(
                fail_text(&FailReason::RateLimited {
                    retry_after_secs: 61
                }),
                "Slow down — try again in 2 min."
            );
            assert_eq!(
                fail_text(&FailReason::RateLimited {
                    retry_after_secs: 120
                }),
                "Slow down — try again in 2 min."
            );
            assert_eq!(
                fail_text(&FailReason::Rejected {
                    reason: "bad schema".into()
                }),
                "bad schema"
            );
            assert_eq!(fail_text(&FailReason::Interrupted), "Interrupted");
        });
    }
}
