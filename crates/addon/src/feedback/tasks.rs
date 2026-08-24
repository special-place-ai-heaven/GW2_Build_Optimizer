//! Background tasks for the About tab: build and send a report, resend a failed
//! one, look up the account name. The pure parts (`report_context`,
//! `context_summary`, `build_report_with`, `apply_send_result`) are unit-tested;
//! the thread bodies follow the `stats.rs` pattern (flag, clone what the thread
//! needs, `catch_unwind`, write back through `with_state`).

use crate::feedback::client::{self, SendResult};
use crate::feedback::{AboutView, FeedbackState, WizardStep};
use crate::state::AddonState;
use gw2_core::feedback::message::{now_unix, FailReason, LastPath, LocalMessage, MessageStatus};
use gw2_core::feedback::report::{
    request_bytes, title_for, to_json, Report, ReportContext, SCHEMA_VERSION,
};
use gw2_core::i18n::t;
use gw2_core::types::GameMode;

/// Category whose successful send shows the Thanks plate instead of the Sent plate.
const PRAISE_CATEGORY: &str = "praise";

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
pub fn ensure_client_id(state: &mut AddonState) -> String {
    if let Some(id) = &state.config.client_id {
        return id.clone();
    }
    let id = uuid::Uuid::new_v4().to_string();
    state.config.client_id = Some(id.clone());
    let _ = state.config.save(&state.config_path);
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

/// Send the open draft: push a `Sending` row, lock the wizard, post in the background.
pub fn send_draft(state: &mut AddonState) {
    let Some((report, json)) = build_report(state) else {
        return;
    };
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

    let feedback = &mut state.main.feedback;
    feedback.messages.insert(0, row);
    feedback.sending = Some(report_id.clone());
    if let Some(draft) = feedback.draft.as_mut() {
        draft.step = WizardStep::Sending;
        draft.error = None;
    }
    feedback.dirty = true;

    spawn_send(state, report_id, json, is_praise);
}

/// Replay a failed row's payload byte-for-byte.
pub fn resend(state: &mut AddonState, report_id: &str) {
    let feedback = &mut state.main.feedback;
    let Some(row) = feedback
        .messages
        .iter_mut()
        .find(|m| m.report_id == report_id && m.status == MessageStatus::Failed)
    else {
        return;
    };
    let Some(json) = row.failed_payload.clone() else {
        return;
    };
    row.status = MessageStatus::Sending;
    row.last_error = None;
    feedback.sending = Some(report_id.to_string());
    feedback.dirty = true;

    spawn_send(state, report_id.to_string(), json, false);
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
    let token = state.cancel_token.clone();

    std::thread::spawn(move || {
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
}

/// Post `json` for `report_id` on a background thread and apply the outcome.
/// A panic counts as a server failure so the row is never stuck in `Sending`.
fn spawn_send(state: &AddonState, report_id: String, json: String, is_praise: bool) {
    let token = state.cancel_token.clone();
    std::thread::spawn(move || {
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
    });
}

/// The lookup failed or cannot run: hide the box and untick it on any open draft.
fn account_failed(feedback: &mut FeedbackState) {
    feedback.account = Some(Err(()));
    if let Some(draft) = feedback.draft.as_mut() {
        draft.include_account = false;
    }
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
    /// like `state::tests` do. Serialised here; run the crate with `--test-threads=1`.
    static TASKS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Run `f` against a freshly initialised global state rooted in a per-test temp dir.
    fn with_fresh_state<R>(label: &str, f: impl FnOnce(&mut AddonState) -> R) -> R {
        let _serial = TASKS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
}
