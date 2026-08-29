//! Background tasks for the About tab: build and send a report, resend a failed
//! one, look up the account name, refresh message statuses, fetch the taxonomy.
//! The pure parts (`report_context`, `context_summary`, `build_report_with`,
//! `apply_send_result`, `apply_status_rows`, `apply_refresh_outcome`,
//! `apply_taxonomy_fetch`) are unit-tested; the thread bodies follow the
//! `stats.rs` pattern (flag, clone what the thread needs, `catch_unwind`, write
//! back through `with_state`).

use std::time::{Duration, Instant};

use crate::feedback::client::{self, SendResult, StatusRow};
use crate::feedback::{AboutView, FeedbackState, WizardStep};
use crate::state::{AddonState, MainTab};
use gw2_core::feedback::message::{now_unix, FailReason, LastPath, LocalMessage, MessageStatus};
use gw2_core::feedback::report::{
    request_bytes, title_for, to_json, Report, ReportContext, SCHEMA_VERSION,
};
use gw2_core::feedback::store::FeedbackStore;
use gw2_core::feedback::taxonomy::FeedbackTaxonomy;
use gw2_core::i18n::t;
use gw2_core::types::GameMode;

/// Category whose successful send shows the Thanks plate instead of the Sent plate.
const PRAISE_CATEGORY: &str = "praise";

/// `messages.json`. Saved from the frame loop whenever a send or a status
/// refresh marks the log dirty, i.e. never on the render thread's own time.
static MESSAGE_WRITES: crate::ui::SerialWriter = crate::ui::SerialWriter::new("messages-save");

/// Stand-in with the exact length of a uuid v4 string, so `draft_request_bytes`
/// measures the real request size before `client_id` has been minted.
const CLIENT_ID_PLACEHOLDER: &str = "00000000-0000-4000-8000-000000000000";

/// Context attached to every report; built only from non-identifying state.
pub fn report_context(state: &AddonState) -> ReportContext {
    let main = &state.main;
    let build = main.current_build.as_ref();
    ReportContext {
        addon_version: crate::VERSION.to_string(),
        game_build: main.live_build_number.or(state.config.cache_build_number),
        locale: gw2_core::i18n::current(),
        mode: main.game_mode.label().to_string(),
        scale: if main.game_mode == GameMode::WvW {
            main.wvw_combat_tier.label().to_string()
        } else {
            String::new()
        },
        role: main
            .selected_role
            .map(|r| format!("{r:?}"))
            .unwrap_or_default(),
        profession: build.map(|b| b.profession.clone()).unwrap_or_default(),
        elite: build
            .and_then(|b| b.specializations.iter().find(|s| s.elite))
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        llm_provider: state.config.active_provider.short_label().to_lowercase(),
    }
}

/// One line for the Messages table, e.g.
/// `v1.6.0 · game 174122 · en · WvW / Roam / Damage · Ranger → Untamed · gemini`.
/// Empty parts are skipped.
pub fn context_summary(ctx: &ReportContext) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !ctx.addon_version.is_empty() {
        parts.push(format!("v{}", ctx.addon_version));
    }
    if let Some(build) = ctx.game_build {
        parts.push(format!("game {build}"));
    }
    if !ctx.locale.is_empty() {
        parts.push(ctx.locale.clone());
    }
    let scene: Vec<&str> = [ctx.mode.as_str(), ctx.scale.as_str(), ctx.role.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if !scene.is_empty() {
        parts.push(scene.join(" / "));
    }
    match (ctx.profession.is_empty(), ctx.elite.is_empty()) {
        (false, false) => parts.push(format!("{} → {}", ctx.profession, ctx.elite)),
        (false, true) => parts.push(ctx.profession.clone()),
        (true, false) => parts.push(ctx.elite.clone()),
        (true, true) => {}
    }
    if !ctx.llm_provider.is_empty() {
        parts.push(ctx.llm_provider.clone());
    }
    parts.join(" · ")
}

/// The per-install client id; minted and saved to config on first use.
///
/// The one config write in this file that stays synchronous. It happens once per
/// install, on the click that sends the first report, and the id it persists is
/// what every later status poll for that report is keyed on — so it is worth the
/// millisecond to know it reached disk before the request goes out.
pub fn ensure_client_id(state: &mut AddonState) -> String {
    if let Some(id) = &state.config.client_id {
        return id.clone();
    }
    let id = uuid::Uuid::new_v4().to_string();
    state.config.client_id = Some(id.clone());
    if let Err(e) = state.config.save(&state.config_path) {
        nexus::log::log(
            nexus::log::LogLevel::Warning,
            "GW2BuildOpt",
            format!("could not save client_id to config: {e}"),
        );
    }
    id
}

/// The would-be report for the open draft with `client_id` filled in. Pure: no
/// minting, no saving. `None` when no draft is open or it has no category.
pub fn build_report_with(state: &AddonState, client_id: &str) -> Option<Report> {
    let feedback = &state.main.feedback;
    let draft = feedback.draft.as_ref()?;
    let category = draft.category()?;

    let labels: Vec<String> = draft.choice_label_keys().iter().map(|k| t(k)).collect();
    let body = draft.body();
    let contact = draft.contact.trim();
    let contact = (!contact.is_empty()).then(|| contact.to_string());
    let account = match (&feedback.account, draft.include_account) {
        (Some(Ok(name)), true) => Some(name.clone()),
        _ => None,
    };
    let build_snapshot =
        if category.attach_build && draft.include_build && feedback.snapshot_attachable() {
            feedback.snapshot.clone()
        } else {
            None
        };

    Some(Report {
        schema_version: SCHEMA_VERSION,
        report_id: draft.report_id.clone(),
        client_id: client_id.to_string(),
        category: category.id.clone(),
        path: draft.path(),
        title: title_for(&body, &labels),
        body,
        contact,
        account,
        context: report_context(state),
        build_snapshot,
    })
}

/// The report for the open draft plus its exact wire bytes. Mints `client_id` if needed.
pub fn build_report(state: &mut AddonState) -> Option<(Report, String)> {
    let client_id = ensure_client_id(state);
    let report = build_report_with(state, &client_id)?;
    let json = to_json(&report);
    Some((report, json))
}

/// Compact-JSON size of the would-be request for the open draft, without side effects.
pub fn draft_request_bytes(state: &AddonState) -> Option<usize> {
    let client_id = state
        .config
        .client_id
        .as_deref()
        .unwrap_or(CLIENT_ID_PLACEHOLDER);
    build_report_with(state, client_id).map(|r| request_bytes(&r))
}

/// Send the open draft: stage the row and lock the wizard, then post in the background.
pub fn send_draft(state: &mut AddonState) {
    if let Some((report_id, json, is_praise)) = stage_send(state) {
        if !spawn_send(state, report_id.clone(), json, is_praise) {
            fail_unstarted_send(state, &report_id, is_praise);
        }
    }
}

/// The post never started (the OS refused a thread). Undo the `Sending` row so the
/// wizard unlocks and the payload is still there to resend, instead of a message that
/// looks like it is on its way forever.
fn fail_unstarted_send(state: &mut AddonState, report_id: &str, is_praise: bool) {
    apply_send_result(
        state,
        report_id,
        SendResult::Failed(FailReason::Interrupted),
        is_praise,
    );
}

/// Everything `send_draft` does except the background post: refuse while a request is
/// in flight; otherwise keep one row per `report_id` — a failed row whose payload is
/// byte-identical is replayed in place, an edited draft discards it and ships under a
/// fresh id, and a new draft inserts a `Sending` row at the front. Returns the
/// `(report_id, json, is_praise)` triple that `spawn_send` posts, or `None` if nothing
/// was staged. Pure over `state` (plus the `client_id` mint) so tests can drive it.
pub fn stage_send(state: &mut AddonState) -> Option<(String, String, bool)> {
    if state.main.feedback.sending.is_some() {
        return None;
    }
    let (report, json) = build_report(state)?;
    let fb = &mut state.main.feedback;
    if let Some(pos) = fb
        .messages
        .iter()
        .position(|m| m.report_id == report.report_id)
    {
        if fb.messages[pos].failed_payload.as_deref() == Some(json.as_str()) {
            // Identical bytes → replay, one row.
            let id = report.report_id.clone();
            return stage_resend(state, &id);
        }
        // Edited → discard the old row and rebuild under a new id (recursion depth 1:
        // the fresh uuid matches no row).
        fb.messages.remove(pos);
        if let Some(d) = fb.draft.as_mut() {
            d.report_id = uuid::Uuid::new_v4().to_string();
        }
        return stage_send(state);
    }

    let is_praise = report.category == PRAISE_CATEGORY;
    let row = LocalMessage {
        report_id: report.report_id.clone(),
        short_id: None,
        sent_at: now_unix(),
        category: report.category,
        path: report.path,
        title: report.title,
        body: report.body,
        status: MessageStatus::Sending,
        reply: None,
        replied_at: None,
        closing_note: None,
        last_error: None,
        failed_at: None,
        failed_payload: Some(json.clone()),
        context_summary: context_summary(&report.context),
    };
    let report_id = row.report_id.clone();

    fb.messages.insert(0, row);
    fb.sending = Some(report_id.clone());
    if let Some(draft) = fb.draft.as_mut() {
        draft.step = WizardStep::Sending;
        draft.error = None;
    }
    fb.dirty = true;
    Some((report_id, json, is_praise))
}

/// Replay a failed row's payload byte-for-byte.
pub fn resend(state: &mut AddonState, report_id: &str) {
    if let Some((report_id, json, is_praise)) = stage_resend(state, report_id) {
        if !spawn_send(state, report_id.clone(), json, is_praise) {
            fail_unstarted_send(state, &report_id, is_praise);
        }
    }
}

/// Everything `resend` does except the background post: refuse while a request is in
/// flight; otherwise flip the failed row back to `Sending`, lock the draft if it is the
/// one being replayed, and return the triple `spawn_send` posts. `is_praise` comes from
/// the row's category so a replayed praise still lands on the Thanks plate.
pub fn stage_resend(state: &mut AddonState, report_id: &str) -> Option<(String, String, bool)> {
    let feedback = &mut state.main.feedback;
    if feedback.sending.is_some() {
        return None;
    }
    let row = feedback
        .messages
        .iter_mut()
        .find(|m| m.report_id == report_id && m.status == MessageStatus::Failed)?;
    let json = row.failed_payload.clone()?;
    let is_praise = row.category == PRAISE_CATEGORY;
    row.status = MessageStatus::Sending;
    row.last_error = None;
    if let Some(draft) = feedback.draft.as_mut().filter(|d| d.report_id == report_id) {
        draft.step = WizardStep::Sending;
        draft.error = None;
    }
    feedback.sending = Some(report_id.to_string());
    feedback.dirty = true;
    Some((report_id.to_string(), json, is_praise))
}

/// Apply a send outcome to the row (and the draft, when it is the one that was sent).
/// Pure over `state`; also the panic arm's fallback with `Failed(Server)`.
pub fn apply_send_result(
    state: &mut AddonState,
    report_id: &str,
    result: SendResult,
    is_praise: bool,
) {
    let feedback = &mut state.main.feedback;
    // The request is over either way; never leave Send locked on a vanished row.
    if feedback.sending.as_deref() == Some(report_id) {
        feedback.sending = None;
    }
    let Some(row) = feedback
        .messages
        .iter_mut()
        .find(|m| m.report_id == report_id)
    else {
        return;
    };
    let draft = feedback.draft.as_mut().filter(|d| d.report_id == report_id);

    match result {
        SendResult::Created { short_id, status } => {
            row.short_id = Some(short_id.clone());
            row.status = status;
            row.failed_payload = None;
            row.last_error = None;
            feedback.last_path = Some(LastPath {
                category: row.category.clone(),
                path: row.path.clone(),
            });
            if let Some(draft) = draft {
                draft.step = if is_praise {
                    WizardStep::Thanks
                } else {
                    WizardStep::Sent { short_id }
                };
            }
            feedback.view = AboutView::Messages;
            feedback.view_chosen = true;
            // Runs inside `with_state` on the send thread: ask the frame loop to poll
            // rather than spawning from here.
            feedback.refresh_requested = true;
        }
        SendResult::Failed(reason) => {
            row.status = MessageStatus::Failed;
            row.last_error = Some(reason.clone());
            row.failed_at = Some(now_unix());
            if let Some(draft) = draft {
                draft.error = Some(reason);
                draft.step = WizardStep::Summary;
            }
        }
    }
    feedback.dirty = true;
}

/// Fetch `/v2/account` name for the "include my account" box. No-op while a lookup is
/// running or once an answer is known. Without a GW2 key the box hides (`Some(Err(()))`).
pub fn lookup_account(state: &mut AddonState) {
    let feedback = &mut state.main.feedback;
    if feedback.account_looking_up || feedback.account.is_some() {
        return;
    }
    let Some(key) = state.config.gw2_api_key.clone() else {
        account_failed(feedback);
        return;
    };
    feedback.account_looking_up = true;

    let spawned = state.spawn_worker("lookup-account", move |token| {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            /// Only the `name` field of `/v2/account`; nothing else is deserialized.
            #[derive(serde::Deserialize)]
            struct AccountName {
                name: String,
            }
            let result = if token.is_cancelled() {
                None
            } else {
                let r = gw2_api::client::Gw2Client::with_key(&key)
                    .and_then(|c| c.get::<AccountName>("account"));
                if token.is_cancelled() {
                    None
                } else {
                    Some(r)
                }
            };
            crate::state::with_state(|s| {
                let feedback = &mut s.main.feedback;
                feedback.account_looking_up = false;
                match result {
                    Some(Ok(account)) => feedback.account = Some(Ok(account.name)),
                    Some(Err(_)) => account_failed(feedback),
                    None => { /* cancelled — only the flag reset matters */ }
                }
            });
        }));
        if panic_result.is_err() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                "bg thread panicked: lookup_account",
            );
            crate::state::with_state(|s| {
                let feedback = &mut s.main.feedback;
                feedback.account_looking_up = false;
                account_failed(feedback);
            });
        }
    });
    if !spawned {
        // The OS refused the thread (`spawn_worker` logged it). Nothing will
        // answer, so settle the box the same way a failed lookup does instead
        // of leaving it in "looking up" forever.
        let feedback = &mut state.main.feedback;
        feedback.account_looking_up = false;
        account_failed(feedback);
    }
}

/// Post `json` for `report_id` on a background thread and apply the outcome.
/// A panic counts as a server failure so the row is never stuck in `Sending`.
///
/// Returns false when the worker never started; the caller owns the `&mut` needed
/// to unwind the staged row, so it does that (see `fail_unstarted_send`).
#[must_use]
fn spawn_send(state: &AddonState, report_id: String, json: String, is_praise: bool) -> bool {
    state.spawn_worker("send-report", move |token| {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if token.is_cancelled() {
                return;
            }
            let result = client::classify(client::post_report(&json, crate::VERSION));
            if token.is_cancelled() {
                return;
            }
            crate::state::with_state(|s| apply_send_result(s, &report_id, result, is_praise));
        }));
        if panic_result.is_err() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                "bg thread panicked: send report",
            );
            crate::state::with_state(|s| {
                apply_send_result(
                    s,
                    &report_id,
                    SendResult::Failed(FailReason::Server),
                    is_praise,
                )
            });
        }
    })
}

/// The lookup failed or cannot run: hide the box and untick it on any open draft.
fn account_failed(feedback: &mut FeedbackState) {
    feedback.account = Some(Err(()));
    if let Some(draft) = feedback.draft.as_mut() {
        draft.include_account = false;
    }
}

/// Wall-clock gap between periodic status polls (design §6a: every five minutes while
/// the overlay is visible).
const POLL_INTERVAL: Duration = Duration::from_secs(300);

/// The server reads at most this many ids per status request.
const MAX_POLL_IDS: usize = 50;

/// Load `messages.json` and the taxonomy into `state.main.feedback` once (idempotent).
/// The cached server taxonomy wins over the embedded one when its version is higher.
/// Runs from any tab so the periodic poll works before the About tab was ever opened.
pub fn ensure_loaded(state: &mut AddonState) {
    if state.main.feedback.loaded {
        return;
    }
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
        feedback.view = feedback.default_view();
    }
}

/// Short ids of the rows the server may still change (`received`/`read`/`answered`),
/// capped at the server's per-request limit.
pub fn pollable_ids(messages: &[LocalMessage]) -> Vec<String> {
    messages
        .iter()
        .filter(|m| m.pollable())
        .filter_map(|m| m.short_id.clone())
        .take(MAX_POLL_IDS)
        .collect()
}

/// Server status word → enum; `None` for a word this build does not know, so a newer
/// server never overwrites a known status with a guess.
fn status_from_server(word: &str) -> Option<MessageStatus> {
    match word {
        "received" => Some(MessageStatus::Received),
        "read" => Some(MessageStatus::Read),
        "answered" => Some(MessageStatus::Answered),
        "closed" => Some(MessageStatus::Closed),
        _ => None,
    }
}

/// Apply a successful status response to the rows that were polled: a returned row
/// updates status, reply, `replied_at` and closing note; a polled id the server no
/// longer has becomes `Unknown`. Rows outside `polled` are untouched. Returns the
/// `report_id` of a row that just became `Answered`.
pub fn apply_status_rows(
    messages: &mut [LocalMessage],
    polled: &[String],
    rows: &[StatusRow],
) -> Option<String> {
    let mut flipped = None;
    for m in messages.iter_mut() {
        let Some(id) = m.short_id.as_deref() else {
            continue;
        };
        if !polled.iter().any(|p| p == id) {
            continue;
        }
        match rows.iter().find(|r| r.id == id) {
            Some(row) => {
                if let Some(status) = status_from_server(&row.status) {
                    if status == MessageStatus::Answered && m.status != MessageStatus::Answered {
                        flipped = Some(m.report_id.clone());
                    }
                    m.status = status;
                }
                m.reply = row.reply.clone();
                m.replied_at = row.replied_at.clone();
                m.closing_note = row.closing_note.clone();
            }
            None => m.status = MessageStatus::Unknown,
        }
    }
    flipped
}

/// Apply one status fetch outcome. Success updates the rows and stamps the refresh; a
/// failure only records it (design §6a: a failed refresh never blanks a known status and
/// never yields `Unknown`). An answer landing while the player is on another tab pulses
/// the About pill. Pure over `state`; also the thread's write-back.
pub fn apply_refresh_outcome(
    state: &mut AddonState,
    polled: &[String],
    result: Result<Vec<StatusRow>, ()>,
) {
    let feedback = &mut state.main.feedback;
    feedback.refreshing = false;
    let Ok(rows) = result else {
        feedback.last_refresh_ok = Some(false);
        return;
    };
    let flipped = apply_status_rows(&mut feedback.messages, polled, &rows);
    feedback.last_refresh_at = Some(Instant::now());
    feedback.last_refresh_ok = Some(true);
    feedback.dirty = true;
    if let Some(id) = flipped {
        feedback.view = AboutView::Messages;
        feedback.view_chosen = true;
        feedback.expanded = Some(id);
        if state.main.active_tab != MainTab::About {
            state.main.tab_alert = Some(MainTab::About);
        }
    }
}

/// Refresh the status of every pollable row on a background thread. No-op when there is
/// nothing to poll, a refresh is in flight, or no `client_id` was minted yet (then
/// nothing could have been sent).
pub fn refresh_status(state: &mut AddonState) {
    let ids = pollable_ids(&state.main.feedback.messages);
    if ids.is_empty() || state.main.feedback.refreshing {
        return;
    }
    let Some(client_id) = state.config.client_id.clone() else {
        return;
    };
    state.main.feedback.refreshing = true;

    let spawned = state.spawn_worker("refresh-status", move |token| {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let result = if token.is_cancelled() {
                None
            } else {
                let r = client::fetch_status(&ids, &client_id, crate::VERSION);
                if token.is_cancelled() {
                    None
                } else {
                    Some(r)
                }
            };
            crate::state::with_state(|s| match result {
                Some(result) => apply_refresh_outcome(s, &ids, result),
                None => s.main.feedback.refreshing = false,
            });
        }));
        if panic_result.is_err() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                "bg thread panicked: refresh_status",
            );
            crate::state::with_state(|s| {
                let feedback = &mut s.main.feedback;
                feedback.refreshing = false;
                feedback.last_refresh_ok = Some(false);
            });
        }
    });
    if !spawned {
        // The OS refused the thread (`spawn_worker` logged it): release the
        // in-flight latch, and record the refresh as failed rather than leaving
        // the last result showing as if it were current.
        let feedback = &mut state.main.feedback;
        feedback.refreshing = false;
        feedback.last_refresh_ok = Some(false);
    }
}

/// Per-frame poll driver, called from `render_main` on every tab: loads the store once,
/// forgets `was_open` while the player is elsewhere, honours a refresh requested by a
/// successful send as soon as no refresh is in flight, and otherwise polls every
/// [`POLL_INTERVAL`] while some row is pollable.
pub fn maybe_poll(state: &mut AddonState) {
    if state.main.active_tab != MainTab::About {
        state.main.feedback.was_open = false;
    }
    ensure_loaded(state);
    flush_dirty(state);
    let feedback = &mut state.main.feedback;
    if feedback.refreshing {
        return;
    }
    if feedback.refresh_requested {
        feedback.refresh_requested = false;
        feedback.last_poll = Some(Instant::now());
        refresh_status(state);
        return;
    }
    let due = feedback
        .last_poll
        .is_none_or(|t| t.elapsed() >= POLL_INTERVAL);
    if due && feedback.messages.iter().any(LocalMessage::pollable) {
        feedback.last_poll = Some(Instant::now());
        refresh_status(state);
    }
}

/// Minimum gap between two tab-open status refreshes. The status GET shares the server's
/// 10/min per-IP bucket with POST, so flipping tabs must not be able to starve a send.
const OPEN_REFRESH_GAP: Duration = Duration::from_secs(30);

/// First frame on the About tab since the last switch away: refresh statuses (throttled
/// by [`OPEN_REFRESH_GAP`] and stamped as a poll) and fetch the taxonomy.
pub fn refresh_on_open(state: &mut AddonState) {
    if state.main.feedback.was_open {
        return;
    }
    let feedback = &mut state.main.feedback;
    feedback.was_open = true;
    let due = feedback
        .last_poll
        .is_none_or(|t| t.elapsed() >= OPEN_REFRESH_GAP);
    if due && feedback.messages.iter().any(LocalMessage::pollable) {
        feedback.last_poll = Some(Instant::now());
        refresh_status(state);
    }
    fetch_taxonomy(state);
}

/// Persist `messages.json` when a send, refresh, or row action marked the state dirty.
/// Runs every frame from [`maybe_poll`] so a status that lands while another tab is
/// active is saved without waiting for the About tab to render.
///
/// Snapshot here, write on a worker: this is the render thread holding `STATE`,
/// and [`FeedbackStore::save`] stages through one `messages.json.tmp`, so the
/// write must neither stall the frame nor overlap another save of the same file.
pub fn flush_dirty(state: &mut AddonState) {
    if !state.main.feedback.dirty {
        return;
    }
    let file = gw2_core::feedback::message::MessagesFile {
        last_path: state.main.feedback.last_path.clone(),
        messages: state.main.feedback.messages.clone(),
    };
    let addon_dir = state.addon_dir.clone();
    // Cleared before the write lands: `submit` never drops content, so the next
    // change is what marks the file dirty again — not this one, twice.
    state.main.feedback.dirty = false;
    MESSAGE_WRITES.submit(state, move || {
        if let Err(e) = FeedbackStore::new(&addon_dir).save(&file) {
            crate::ui::log_disk_error(format!("messages.json save failed: {e}"));
        }
    });
}

/// Fetch `/v1/taxonomy` on a background thread. A version newer than the one in use is
/// offered to the state (applied now, or once the open draft closes) and cached on disk.
/// Anything else — transport failure, unparseable body, same or older version — changes
/// nothing: the embedded/cached copy stays and no message is shown.
pub fn fetch_taxonomy(state: &mut AddonState) {
    if state.main.feedback.taxonomy_fetching {
        return;
    }
    state.main.feedback.taxonomy_fetching = true;
    let addon_dir = state.addon_dir.clone();

    let spawned = state.spawn_worker("fetch-taxonomy", move |token| {
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let fetched = if token.is_cancelled() {
                None
            } else {
                client::fetch_taxonomy().and_then(|raw| {
                    let parsed = FeedbackTaxonomy::parse(&raw).ok()?;
                    Some((raw, parsed))
                })
            };
            let fetched = if token.is_cancelled() { None } else { fetched };
            let to_cache = crate::state::with_state(|s| apply_taxonomy_fetch(s, fetched));
            // Outside `with_state` on purpose: caching the raw text is a disk
            // write, and STATE is the render thread's lock.
            if let Some(raw) = to_cache.flatten() {
                if let Err(e) = FeedbackStore::new(&addon_dir).save_taxonomy(&raw) {
                    crate::ui::log_disk_error(format!("feedback_taxonomy.json save failed: {e}"));
                }
            }
        }));
        if panic_result.is_err() {
            nexus::log::log(
                nexus::log::LogLevel::Warning,
                "GW2BuildOpt",
                "bg thread panicked: fetch_taxonomy",
            );
            crate::state::with_state(|s| s.main.feedback.taxonomy_fetching = false);
        }
    });
    if !spawned {
        // The OS refused the thread (`spawn_worker` logged it): release the
        // latch so the next About-tab open can try again.
        state.main.feedback.taxonomy_fetching = false;
    }
}

/// Take a fetched `(raw json, parsed)` taxonomy: when newer than the one in use, offer
/// it to the state and hand back the raw text to cache (so the next load starts from
/// it). Anything else — nothing fetched, same or older version — returns `None` and
/// changes nothing. Pure over `state`; also the thread's write-back.
///
/// Returns rather than writes because the caller holds `STATE` while this runs:
/// [`fetch_taxonomy`] does the disk write once the lock is released.
#[must_use = "the returned raw taxonomy still has to be cached, outside the STATE lock"]
pub fn apply_taxonomy_fetch(
    state: &mut AddonState,
    fetched: Option<(String, FeedbackTaxonomy)>,
) -> Option<String> {
    state.main.feedback.taxonomy_fetching = false;
    let (raw, taxonomy) = fetched?;
    if taxonomy.taxonomy_version <= state.main.feedback.taxonomy.taxonomy_version {
        return None;
    }
    state.main.feedback.offer_taxonomy(taxonomy);
    Some(raw)
}

#[cfg(test)]
mod tests {
    // Test fixtures are built field-by-field for readability.
    #![allow(clippy::field_reassign_with_default)]
    use super::*;
    use crate::feedback::Draft;
    use crate::state::{clear, init, with_state};
    use gw2_core::config::AppConfig;
    use gw2_core::feedback::report::{
        snapshot_bytes, BuildSnapshot, MAX_REQUEST_BYTES, MAX_SNAPSHOT_BYTES,
    };
    use gw2_core::feedback::taxonomy::FeedbackTaxonomy;
    use gw2_core::types::{ResolvedBuild, ResolvedSpec};
    use std::path::PathBuf;

    const BAIT_KEY: &str = "BAIT-API-KEY-NEVER-SENT";
    const BAIT_CHARACTER: &str = "Bait Name.1234";
    const BAIT_ACCOUNT: &str = "Bait Account.5678";

    /// `AddonState` is only constructible through `init` (the cancellation token's
    /// constructor is private to `state`), so these tests share the global `STATE`
    /// with `state::tests` and serialise on the same `state_test_guard` lock.
    fn with_fresh_state<R>(label: &str, f: impl FnOnce(&mut AddonState) -> R) -> R {
        let _serial = crate::state::state_test_guard();
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "gw2_feedback_tasks_{}_{}",
            std::process::id(),
            label
        ));
        let _ = std::fs::remove_dir_all(&dir);
        clear();
        init(dir.clone());
        let out = with_state(f).expect("state initialised");
        clear();
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    fn ranger_untamed() -> ResolvedBuild {
        let mut skirmishing = ResolvedSpec::default();
        skirmishing.name = "Skirmishing".into();
        let mut untamed = ResolvedSpec::default();
        untamed.name = "Untamed".into();
        untamed.elite = true;
        ResolvedBuild {
            character_name: BAIT_CHARACTER.into(),
            profession: "Ranger".into(),
            game_mode: GameMode::WvW,
            specializations: vec![skirmishing, untamed],
            skills: Default::default(),
            legends: Vec::new(),
            pets: Vec::new(),
            weapons: Vec::new(),
            armor: Vec::new(),
            trinkets: Vec::new(),
            relic: None,
            rune: None,
            pvp_amulet: None,
        }
    }

    /// A draft on the summary step with every step filled. `wrong_build` attaches builds,
    /// `bug` does not, `praise` shows the Thanks plate.
    fn summary_draft(category: &str, body: &str) -> Draft {
        let mut d = Draft::new(FeedbackTaxonomy::embedded());
        d.pick(category);
        match category {
            "praise" => {
                d.set_choice("liked", "optimizer");
                d.set_text("note_optional", body.to_string());
            }
            _ => {
                d.set_choice("area_screen", "optimize");
                d.set_choice("severity", "wrong");
                d.set_text("describe", body.to_string());
            }
        }
        d.step = WizardStep::Summary;
        assert!(d.can_send(), "fixture draft must be sendable");
        d
    }

    fn small_snapshot() -> BuildSnapshot {
        let mut s = BuildSnapshot::default();
        s.stat_prefix = "Marauder".into();
        s.relic = "Thief".into();
        s
    }

    fn msg(report_id: &str, status: MessageStatus, payload: Option<&str>) -> LocalMessage {
        LocalMessage {
            report_id: report_id.to_string(),
            short_id: None,
            sent_at: 1,
            category: "bug".to_string(),
            path: vec!["optimize".to_string(), "wrong".to_string()],
            title: "t".to_string(),
            body: "b".to_string(),
            status,
            reply: None,
            replied_at: None,
            closing_note: None,
            last_error: None,
            failed_at: None,
            failed_payload: payload.map(str::to_string),
            context_summary: String::new(),
        }
    }

    fn full_context() -> ReportContext {
        ReportContext {
            addon_version: "1.6.0".into(),
            game_build: Some(174122),
            locale: "en".into(),
            mode: "WvW".into(),
            scale: "Roam".into(),
            role: "Damage".into(),
            profession: "Ranger".into(),
            elite: "Untamed".into(),
            llm_provider: "gemini".into(),
        }
    }

    // T021

    #[test]
    fn report_context_never_contains_key_or_character() {
        with_fresh_state("ctx_privacy", |state| {
            state.config.gw2_api_key = Some(BAIT_KEY.into());
            state.main.game_mode = GameMode::WvW;
            state.main.wvw_combat_tier = gw2_optimizer::scenario::CombatTier::Solo;
            state.main.selected_role = Some(gw2_optimizer::scenario::RoleObjective::PowerDps);
            state.main.live_build_number = Some(174122);
            state.main.current_build = Some(ranger_untamed());

            let ctx = report_context(state);
            let json = serde_json::to_string(&ctx).unwrap();
            assert!(!json.contains(BAIT_KEY), "{json}");
            assert!(!json.contains(BAIT_CHARACTER), "{json}");
            assert!(!json.contains("Bait"), "{json}");

            assert_eq!(ctx.addon_version, crate::VERSION);
            assert_eq!(ctx.game_build, Some(174122));
            assert_eq!(ctx.mode, "WvW");
            assert_eq!(ctx.scale, "Roam");
            assert_eq!(ctx.role, "PowerDps");
            assert_eq!(ctx.profession, "Ranger");
            assert_eq!(ctx.elite, "Untamed");
            assert_eq!(ctx.llm_provider, "gemini");

            // Not WvW → no scale; no build → no profession/elite; cache build as fallback.
            state.main.game_mode = GameMode::PvE;
            state.main.current_build = None;
            state.main.live_build_number = None;
            state.config.cache_build_number = Some(170000);
            let ctx = report_context(state);
            assert_eq!(ctx.scale, "");
            assert_eq!(ctx.profession, "");
            assert_eq!(ctx.elite, "");
            assert_eq!(ctx.game_build, Some(170000));
        });
    }

    #[test]
    fn context_summary_skips_empty_parts() {
        assert_eq!(
            context_summary(&full_context()),
            "v1.6.0 · game 174122 · en · WvW / Roam / Damage · Ranger → Untamed · gemini"
        );

        let mut ctx = full_context();
        ctx.game_build = None;
        ctx.scale = String::new();
        ctx.role = String::new();
        ctx.elite = String::new();
        assert_eq!(context_summary(&ctx), "v1.6.0 · en · WvW · Ranger · gemini");

        let mut ctx = full_context();
        ctx.profession = String::new();
        ctx.elite = String::new();
        ctx.llm_provider = String::new();
        assert_eq!(
            context_summary(&ctx),
            "v1.6.0 · game 174122 · en · WvW / Roam / Damage"
        );

        assert_eq!(context_summary(&ReportContext::default()), "");
    }

    #[test]
    fn ensure_client_id_mints_once() {
        with_fresh_state("client_id", |state| {
            assert!(state.config.client_id.is_none());

            let first = ensure_client_id(state);
            assert!(uuid::Uuid::parse_str(&first).is_ok(), "{first}");
            assert_eq!(state.config.client_id.as_deref(), Some(first.as_str()));

            let second = ensure_client_id(state);
            assert_eq!(first, second);

            // Persisted: a fresh load sees the same id.
            let (loaded, err) = AppConfig::load(&state.config_path);
            assert!(err.is_none(), "{err:?}");
            assert_eq!(loaded.client_id.as_deref(), Some(first.as_str()));
        });
    }

    #[test]
    fn build_report_omits_snapshot_and_account_unless_opted_in() {
        with_fresh_state("opt_in", |state| {
            state.config.gw2_api_key = Some(BAIT_KEY.into());
            state.main.current_build = Some(ranger_untamed());
            state.main.feedback.snapshot = Some(small_snapshot());
            state.main.feedback.account = Some(Ok(BAIT_ACCOUNT.into()));
            let body = "The optimizer picked a trident on land, twice.";

            // Nothing ticked.
            state.main.feedback.draft = Some(summary_draft("wrong_build", body));
            let (report, json) = build_report(state).expect("report");
            assert_eq!(report.schema_version, SCHEMA_VERSION);
            assert_eq!(report.category, "wrong_build");
            assert_eq!(
                report.path,
                vec!["optimize".to_string(), "wrong".to_string()]
            );
            assert_eq!(report.body, body);
            assert_eq!(report.title, body);
            assert_eq!(report.contact, None);
            assert_eq!(report.account, None);
            assert_eq!(report.build_snapshot, None);
            assert_eq!(
                Some(report.client_id.as_str()),
                state.config.client_id.as_deref()
            );
            assert!(!json.contains(BAIT_KEY), "{json}");
            assert!(!json.contains(BAIT_CHARACTER), "{json}");
            assert!(!json.contains(BAIT_ACCOUNT), "{json}");

            // Both ticked on a category that attaches builds.
            {
                let d = state.main.feedback.draft.as_mut().unwrap();
                d.include_build = true;
                d.include_account = true;
                d.contact = "  me@example.org  ".into();
            }
            let (report, json) = build_report(state).expect("report");
            assert_eq!(report.build_snapshot, Some(small_snapshot()));
            assert_eq!(report.account.as_deref(), Some(BAIT_ACCOUNT));
            assert_eq!(report.contact.as_deref(), Some("me@example.org"));
            assert!(json.contains(BAIT_ACCOUNT));
            assert!(!json.contains(BAIT_KEY), "{json}");
            assert!(!json.contains(BAIT_CHARACTER), "{json}");

            // A category without `attach_build` never ships a snapshot even when ticked.
            let mut bug = summary_draft("bug", body);
            bug.include_build = true;
            state.main.feedback.draft = Some(bug);
            let (report, _) = build_report(state).expect("report");
            assert_eq!(report.build_snapshot, None);

            // Account ticked but the lookup failed → no account.
            state.main.feedback.account = Some(Err(()));
            let mut d = summary_draft("bug", body);
            d.include_account = true;
            state.main.feedback.draft = Some(d);
            let (report, _) = build_report(state).expect("report");
            assert_eq!(report.account, None);

            // No draft → nothing to build.
            state.main.feedback.draft = None;
            assert!(build_report(state).is_none());
        });
    }

    #[test]
    fn post_body_and_title_strip_wizard_markup() {
        with_fresh_state("markup", |state| {
            let encoded = "%NL0P|The optimizer picked a trident on land.";
            state.main.feedback.draft = Some(summary_draft("bug", encoded));
            let (report, json) = build_report(state).expect("report");
            assert_eq!(report.body, "The optimizer picked a trident on land.");
            assert_eq!(report.title, report.body);
            assert!(!report.body.contains("%NL0P|"), "{}", report.body);
            assert!(!report.title.contains("%NL0P|"), "{}", report.title);
            assert!(!json.contains("%NL0P|"), "{json}");
        });
    }

    #[test]
    fn apply_send_result_created_updates_row_draft_and_last_path() {
        with_fresh_state("created", |state| {
            let draft = summary_draft("bug", "The optimizer picked a trident on land.");
            let id = draft.report_id.clone();
            let mut sending_row = msg(&id, MessageStatus::Sending, Some("{}"));
            sending_row.path = draft.path();
            state.main.feedback.messages =
                vec![sending_row, msg("other", MessageStatus::Received, None)];
            state.main.feedback.sending = Some(id.clone());
            state.main.feedback.draft = Some(draft);
            state.main.feedback.dirty = false;

            apply_send_result(
                state,
                &id,
                SendResult::Created {
                    short_id: "A3F9K2QD".into(),
                    status: MessageStatus::Received,
                },
                false,
            );

            let feedback = &state.main.feedback;
            let row = &feedback.messages[0];
            assert_eq!(row.short_id.as_deref(), Some("A3F9K2QD"));
            assert_eq!(row.status, MessageStatus::Received);
            assert_eq!(row.failed_payload, None);
            assert_eq!(row.last_error, None);
            assert_eq!(feedback.messages[1].status, MessageStatus::Received);
            assert_eq!(
                feedback.last_path,
                Some(LastPath {
                    category: "bug".into(),
                    path: vec!["optimize".into(), "wrong".into()],
                })
            );
            assert_eq!(
                feedback.draft.as_ref().map(|d| d.step.clone()),
                Some(WizardStep::Sent {
                    short_id: "A3F9K2QD".into()
                })
            );
            assert_eq!(feedback.view, AboutView::Messages);
            assert!(feedback.view_chosen);
            assert_eq!(feedback.sending, None);
            assert!(feedback.dirty);

            // Praise → Thanks plate; a replayed id may already be past `received`.
            let praise = summary_draft("praise", "Loved the optimizer, thank you Choya!");
            let pid = praise.report_id.clone();
            state
                .main
                .feedback
                .messages
                .insert(0, msg(&pid, MessageStatus::Sending, Some("{}")));
            state.main.feedback.draft = Some(praise);
            apply_send_result(
                state,
                &pid,
                SendResult::Created {
                    short_id: "B1B1B1B1".into(),
                    status: MessageStatus::Answered,
                },
                true,
            );
            assert_eq!(
                state.main.feedback.messages[0].status,
                MessageStatus::Answered
            );
            assert_eq!(
                state.main.feedback.draft.as_ref().map(|d| d.step.clone()),
                Some(WizardStep::Thanks)
            );

            // Unknown row → nothing changes.
            let before = state.main.feedback.messages.clone();
            apply_send_result(
                state,
                "missing",
                SendResult::Created {
                    short_id: "ZZZZZZZZ".into(),
                    status: MessageStatus::Received,
                },
                false,
            );
            assert_eq!(state.main.feedback.messages, before);
        });
    }

    #[test]
    fn apply_send_result_failed_keeps_payload_and_reopens_summary() {
        with_fresh_state("failed", |state| {
            let mut draft = summary_draft("bug", "The optimizer picked a trident on land.");
            draft.step = WizardStep::Sending;
            let id = draft.report_id.clone();
            state.main.feedback.messages =
                vec![msg(&id, MessageStatus::Sending, Some("{\"k\":1}"))];
            state.main.feedback.sending = Some(id.clone());
            state.main.feedback.draft = Some(draft);
            state.main.feedback.last_path = None;

            let reason = FailReason::RateLimited {
                retry_after_secs: 90,
            };
            apply_send_result(state, &id, SendResult::Failed(reason.clone()), false);

            let feedback = &state.main.feedback;
            let row = &feedback.messages[0];
            assert_eq!(row.status, MessageStatus::Failed);
            assert_eq!(row.last_error, Some(reason.clone()));
            assert_eq!(row.failed_payload.as_deref(), Some("{\"k\":1}"));
            assert!(row.failed_at.is_some_and(|t| t > 0));
            assert_eq!(row.short_id, None);
            assert_eq!(feedback.last_path, None);
            let draft = feedback.draft.as_ref().unwrap();
            assert_eq!(draft.error, Some(reason));
            assert_eq!(draft.step, WizardStep::Summary);
            assert_eq!(draft.body(), "The optimizer picked a trident on land.");
            assert_eq!(feedback.sending, None);
            assert!(feedback.dirty);

            // A resend failure with no draft open leaves the draft slot alone.
            state.main.feedback.draft = None;
            state.main.feedback.messages[0].status = MessageStatus::Sending;
            apply_send_result(state, &id, SendResult::Failed(FailReason::Server), false);
            assert_eq!(
                state.main.feedback.messages[0].status,
                MessageStatus::Failed
            );
            assert_eq!(
                state.main.feedback.messages[0].last_error,
                Some(FailReason::Server)
            );
            assert!(state.main.feedback.draft.is_none());
        });
    }

    // T023

    #[test]
    fn lookup_account_without_key_hides_box() {
        with_fresh_state("account_no_key", |state| {
            state.config.gw2_api_key = None;
            let mut draft = summary_draft("bug", "The optimizer picked a trident on land.");
            draft.include_account = true;
            state.main.feedback.draft = Some(draft);

            lookup_account(state);

            let feedback = &state.main.feedback;
            assert_eq!(feedback.account, Some(Err(())));
            assert!(!feedback.account_looking_up);
            assert!(!feedback.draft.as_ref().unwrap().include_account);

            // Known answer → no second lookup starts.
            state.main.feedback.account = Some(Ok("Someone.1234".into()));
            lookup_account(state);
            assert!(!state.main.feedback.account_looking_up);
            assert_eq!(state.main.feedback.account, Some(Ok("Someone.1234".into())));
        });
    }

    // T024

    #[test]
    fn request_over_budget_blocks_send() {
        with_fresh_state("budget", |state| {
            let body: String = "漢".repeat(4000);
            let mut draft = summary_draft("wrong_build", &body);
            draft.include_build = true;
            state.main.feedback.draft = Some(draft);

            let mut snapshot = BuildSnapshot::default();
            snapshot.relic = "x".repeat(5900);
            assert!(snapshot_bytes(&snapshot) <= MAX_SNAPSHOT_BYTES);
            assert!(snapshot_bytes(&snapshot) >= 5900);
            state.main.feedback.snapshot = Some(snapshot);

            let bytes = draft_request_bytes(state).expect("draft open");
            assert!(bytes > MAX_REQUEST_BYTES, "{bytes}");
            // Measuring never mints or saves a client id.
            assert!(state.config.client_id.is_none());
            assert!(!state.config_path.exists());

            state.main.feedback.snapshot = None;
            let bytes = draft_request_bytes(state).expect("draft open");
            assert!(bytes <= MAX_REQUEST_BYTES, "{bytes}");

            // The measurement matches the bytes that would be posted once the id exists.
            state.config.client_id = Some(uuid::Uuid::new_v4().to_string());
            let measured = draft_request_bytes(state).unwrap();
            let (_, json) = build_report(state).unwrap();
            assert_eq!(measured, json.len());
        });
    }

    // Send path: one row per report_id, replay vs. discard, praise resend, in-flight guard.

    const BODY: &str = "The optimizer picked a trident on land.";

    #[test]
    fn send_after_failure_with_same_bytes_replays_one_row() {
        with_fresh_state("replay_same", |state| {
            state.main.feedback.draft = Some(summary_draft("bug", BODY));

            let (id, json, is_praise) = stage_send(state).expect("staged");
            assert!(!is_praise);
            {
                let fb = &state.main.feedback;
                assert_eq!(fb.messages.len(), 1);
                assert_eq!(fb.messages[0].report_id, id);
                assert_eq!(fb.messages[0].status, MessageStatus::Sending);
                assert_eq!(
                    fb.messages[0].failed_payload.as_deref(),
                    Some(json.as_str())
                );
                assert_eq!(fb.sending.as_deref(), Some(id.as_str()));
                assert_eq!(
                    fb.draft.as_ref().map(|d| d.step.clone()),
                    Some(WizardStep::Sending)
                );
            }

            apply_send_result(state, &id, SendResult::Failed(FailReason::Server), false);
            assert_eq!(
                state.main.feedback.messages[0].status,
                MessageStatus::Failed
            );
            assert_eq!(
                state.main.feedback.draft.as_ref().map(|d| d.step.clone()),
                Some(WizardStep::Summary)
            );

            // Same draft, same bytes → replay under the same id; still exactly one row.
            let (id2, json2, _) = stage_send(state).expect("staged again");
            let fb = &state.main.feedback;
            assert_eq!(id2, id);
            assert_eq!(json2, json);
            assert_eq!(fb.messages.len(), 1);
            assert_eq!(fb.messages[0].report_id, id);
            assert_eq!(fb.messages[0].status, MessageStatus::Sending);
            assert_eq!(fb.messages[0].last_error, None);
            assert_eq!(fb.sending.as_deref(), Some(id.as_str()));
            let draft = fb.draft.as_ref().unwrap();
            assert_eq!(draft.report_id, id);
            assert_eq!(draft.step, WizardStep::Sending);
            assert_eq!(draft.error, None);
        });
    }

    #[test]
    fn send_after_failure_with_edited_body_discards_and_mints_new_id() {
        with_fresh_state("replay_edited", |state| {
            state.main.feedback.draft = Some(summary_draft("bug", BODY));
            let (id, _, _) = stage_send(state).expect("staged");
            apply_send_result(state, &id, SendResult::Failed(FailReason::Network), false);

            let edited = "The optimizer picked a trident on land, twice.";
            state
                .main
                .feedback
                .draft
                .as_mut()
                .unwrap()
                .set_text("describe", edited.to_string());

            // Edited → the failed row is discarded and the draft ships under a new id.
            let (id2, json2, _) = stage_send(state).expect("staged again");
            let fb = &state.main.feedback;
            assert_ne!(id2, id);
            assert!(uuid::Uuid::parse_str(&id2).is_ok(), "{id2}");
            assert_eq!(fb.messages.len(), 1);
            assert_eq!(fb.messages[0].report_id, id2);
            assert_eq!(fb.messages[0].body, edited);
            assert_eq!(fb.messages[0].status, MessageStatus::Sending);
            assert!(json2.contains(edited), "{json2}");
            assert!(json2.contains(&id2), "{json2}");
            assert!(!json2.contains(&id), "{json2}");
            assert_eq!(fb.sending.as_deref(), Some(id2.as_str()));
            let draft = fb.draft.as_ref().unwrap();
            assert_eq!(draft.report_id, id2);
            assert_eq!(draft.step, WizardStep::Sending);
        });
    }

    #[test]
    fn resend_of_praise_is_praise() {
        with_fresh_state("resend_praise", |state| {
            let mut praise = msg("p1", MessageStatus::Failed, Some("{\"praise\":1}"));
            praise.category = PRAISE_CATEGORY.to_string();
            state.main.feedback.messages =
                vec![praise, msg("b1", MessageStatus::Failed, Some("{}"))];

            let (id, json, is_praise) = stage_resend(state, "p1").expect("staged");
            assert_eq!(id, "p1");
            assert_eq!(json, "{\"praise\":1}");
            assert!(is_praise);
            assert_eq!(
                state.main.feedback.messages[0].status,
                MessageStatus::Sending
            );
            assert_eq!(state.main.feedback.sending.as_deref(), Some("p1"));

            // A bug row is not praise; the in-flight guard applies to resend too.
            assert!(stage_resend(state, "b1").is_none());
            state.main.feedback.sending = None;
            let (_, _, is_praise) = stage_resend(state, "b1").expect("staged");
            assert!(!is_praise);
        });
    }

    #[test]
    fn send_refused_while_sending() {
        with_fresh_state("refuse_in_flight", |state| {
            state.main.feedback.draft = Some(summary_draft("bug", BODY));
            state.main.feedback.sending = Some("other".to_string());

            assert!(stage_send(state).is_none());
            let fb = &state.main.feedback;
            assert!(fb.messages.is_empty());
            assert_eq!(fb.sending.as_deref(), Some("other"));
            assert_eq!(
                fb.draft.as_ref().map(|d| d.step.clone()),
                Some(WizardStep::Summary)
            );

            state.main.feedback.messages = vec![msg("f1", MessageStatus::Failed, Some("{}"))];
            assert!(stage_resend(state, "f1").is_none());
            assert_eq!(
                state.main.feedback.messages[0].status,
                MessageStatus::Failed
            );
        });
    }

    // T027 — status refresh.

    fn polled(short_id: &str, status: MessageStatus) -> LocalMessage {
        LocalMessage {
            short_id: Some(short_id.to_string()),
            ..msg(short_id, status, None)
        }
    }

    fn status_row(id: &str, status: &str, reply: Option<&str>) -> StatusRow {
        StatusRow {
            id: id.to_string(),
            status: status.to_string(),
            reply: reply.map(str::to_string),
            replied_at: reply.map(|_| "2026-08-25T10:00:00Z".to_string()),
            closing_note: None,
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn refresh_applies_rows_and_marks_unknown_only_on_success() {
        let mut messages = vec![
            polled("A", MessageStatus::Received),
            polled("B", MessageStatus::Received),
            polled("C", MessageStatus::Failed),
        ];
        let polled_ids = ids(&["A", "B"]);
        let rows = vec![status_row("A", "answered", Some("Fixed in 1.6.1"))];

        assert!(apply_status_rows(&mut messages, &polled_ids, &rows).is_some());
        assert_eq!(messages[0].status, MessageStatus::Answered);
        assert_eq!(messages[0].reply.as_deref(), Some("Fixed in 1.6.1"));
        assert_eq!(
            messages[0].replied_at.as_deref(),
            Some("2026-08-25T10:00:00Z")
        );
        assert_eq!(messages[1].status, MessageStatus::Unknown);
        assert_eq!(messages[2].status, MessageStatus::Failed);
        assert_eq!(messages[2].reply, None);

        // Already answered → no second flip.
        assert!(apply_status_rows(&mut messages, &polled_ids, &rows).is_none());
        assert_eq!(messages[0].status, MessageStatus::Answered);
    }

    #[test]
    fn apply_status_rows_ignores_unknown_status_string() {
        let mut messages = vec![polled("A", MessageStatus::Received)];
        let rows = vec![status_row("A", "archived", Some("note"))];
        assert!(apply_status_rows(&mut messages, &ids(&["A"]), &rows).is_none());
        assert_eq!(messages[0].status, MessageStatus::Received);
        assert_eq!(messages[0].reply.as_deref(), Some("note"));
    }

    #[test]
    fn refresh_failure_keeps_statuses() {
        with_fresh_state("refresh_failure", |state| {
            state.main.feedback.messages = vec![
                polled("A", MessageStatus::Received),
                polled("B", MessageStatus::Read),
            ];
            state.main.feedback.refreshing = true;
            state.main.feedback.dirty = false;

            apply_refresh_outcome(state, &ids(&["A", "B"]), Err(()));

            let feedback = &state.main.feedback;
            assert_eq!(feedback.messages[0].status, MessageStatus::Received);
            assert_eq!(feedback.messages[1].status, MessageStatus::Read);
            assert!(!feedback.refreshing);
            assert_eq!(feedback.last_refresh_ok, Some(false));
            assert_eq!(feedback.last_refresh_at, None);
            assert!(!feedback.dirty);
            assert_eq!(state.main.tab_alert, None);
        });
    }

    #[test]
    fn only_pollable_rows_are_polled() {
        let mut messages: Vec<LocalMessage> = [
            MessageStatus::Received,
            MessageStatus::Read,
            MessageStatus::Answered,
            MessageStatus::Closed,
            MessageStatus::Failed,
            MessageStatus::Local,
            MessageStatus::Sending,
            MessageStatus::Unknown,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, status)| polled(&format!("S{i}"), status))
        .collect();
        // Received but never acknowledged: nothing to ask the server about.
        messages.push(msg("no-short-id", MessageStatus::Received, None));

        assert_eq!(pollable_ids(&messages), ids(&["S0", "S1", "S2"]));

        let many: Vec<LocalMessage> = (0..60)
            .map(|i| polled(&format!("M{i}"), MessageStatus::Received))
            .collect();
        assert_eq!(pollable_ids(&many).len(), 50);
    }

    #[test]
    fn answered_flip_sets_tab_alert_when_not_on_about() {
        with_fresh_state("flip_alert", |state| {
            state.main.active_tab = MainTab::Settings;
            state.main.feedback.messages = vec![polled("A", MessageStatus::Read)];
            state.main.feedback.refreshing = true;
            state.main.feedback.dirty = false;

            let answered = || Ok(vec![status_row("A", "answered", Some("hi"))]);
            apply_refresh_outcome(state, &ids(&["A"]), answered());

            assert_eq!(state.main.tab_alert, Some(MainTab::About));
            let feedback = &state.main.feedback;
            assert_eq!(feedback.view, AboutView::Messages);
            assert_eq!(feedback.expanded.as_deref(), Some("A"));
            assert_eq!(feedback.messages[0].status, MessageStatus::Answered);
            assert!(!feedback.refreshing);
            assert_eq!(feedback.last_refresh_ok, Some(true));
            assert!(feedback.last_refresh_at.is_some());
            assert!(feedback.dirty);

            // The same answer again is not news: no fresh pulse.
            state.main.tab_alert = None;
            apply_refresh_outcome(state, &ids(&["A"]), answered());
            assert_eq!(state.main.tab_alert, None);
        });
    }

    #[test]
    fn no_alert_when_on_about() {
        with_fresh_state("no_alert", |state| {
            state.main.active_tab = MainTab::About;
            state.main.feedback.messages = vec![polled("A", MessageStatus::Received)];

            apply_refresh_outcome(
                state,
                &ids(&["A"]),
                Ok(vec![status_row("A", "answered", Some("hi"))]),
            );

            assert_eq!(state.main.tab_alert, None);
            assert_eq!(
                state.main.feedback.messages[0].status,
                MessageStatus::Answered
            );
            assert_eq!(state.main.feedback.view, AboutView::Messages);
            assert_eq!(
                state.main.feedback.expanded.as_deref(),
                Some(state.main.feedback.messages[0].report_id.as_str())
            );
        });
    }

    #[test]
    fn maybe_poll_waits_for_request_and_forgets_was_open() {
        with_fresh_state("maybe_poll", |state| {
            state.main.feedback.loaded = true;
            state.main.active_tab = MainTab::Settings;
            state.main.feedback.was_open = true;
            state.main.feedback.refresh_requested = true;
            state.main.feedback.refreshing = true;

            // A refresh in flight: the request waits, `was_open` is forgotten anyway.
            maybe_poll(state);
            assert!(!state.main.feedback.was_open);
            assert!(state.main.feedback.refresh_requested);
            assert_eq!(state.main.feedback.last_poll, None);

            // Idle: the request is consumed and counts as the poll. A fresh config has no
            // `client_id`, so `refresh_status` is a no-op and nothing is spawned.
            state.main.feedback.refreshing = false;
            maybe_poll(state);
            assert!(!state.main.feedback.refresh_requested);
            assert!(state.main.feedback.last_poll.is_some());
            assert!(!state.main.feedback.refreshing);
        });
    }

    // T029 — taxonomy refresh.

    #[test]
    fn fetched_taxonomy_is_cached_and_applied_only_when_newer() {
        with_fresh_state("taxonomy_fetch", |state| {
            let current = FeedbackTaxonomy::embedded();
            state.main.feedback.taxonomy = current.clone();
            let store = FeedbackStore::new(&state.addon_dir);

            // Same version → ignored, and nothing handed back to cache.
            state.main.feedback.taxonomy_fetching = true;
            let same_raw = serde_json::to_string(&current).unwrap();
            assert_eq!(
                apply_taxonomy_fetch(state, Some((same_raw, current.clone()))),
                None
            );
            assert!(!state.main.feedback.taxonomy_fetching);
            assert_eq!(state.main.feedback.taxonomy, current);
            assert_eq!(store.load_taxonomy(), None);

            // Newer → in use, and the raw text comes back for the caller to
            // cache. It is deliberately *not* written here: this runs with STATE
            // held, and `fetch_taxonomy` does the disk write once it is released.
            let mut newer = current.clone();
            newer.taxonomy_version += 1;
            let raw = serde_json::to_string(&newer).unwrap();
            state.main.feedback.taxonomy_fetching = true;
            let to_cache = apply_taxonomy_fetch(state, Some((raw.clone(), newer.clone())));
            assert_eq!(to_cache.as_deref(), Some(raw.as_str()));
            assert!(!state.main.feedback.taxonomy_fetching);
            assert_eq!(state.main.feedback.taxonomy, newer);
            assert_eq!(
                store.load_taxonomy(),
                None,
                "the cache write belongs to the caller, outside the lock"
            );

            // What `fetch_taxonomy` then does with it, off the lock.
            store.save_taxonomy(&to_cache.unwrap()).unwrap();
            assert_eq!(store.load_taxonomy(), Some(newer.clone()));

            // Nothing fetched → only the flag clears.
            state.main.feedback.taxonomy_fetching = true;
            assert_eq!(apply_taxonomy_fetch(state, None), None);
            assert!(!state.main.feedback.taxonomy_fetching);
            assert_eq!(state.main.feedback.taxonomy, newer);
        });
    }
}
