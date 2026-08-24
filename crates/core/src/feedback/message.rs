//! Local feedback messages: what the player sent, what the server answered, and why a send failed.
//! Persisted in `{addon_dir}/messages.json` via `FeedbackStore`.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lifecycle of one feedback message as seen by the addon.
///
/// Transitions: `Sending → Received | Failed`; `Received → Read → Answered → Closed`
/// (server-driven); `Received | Read | Answered → Unknown` when the server no longer
/// lists the id; `Failed → Sending` on Resend; `Sending → Failed(Interrupted)` on load.
/// `Local` is terminal (never sent, e.g. a coffee link click).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    Sending,
    Received,
    Read,
    Answered,
    Closed,
    Failed,
    Local,
    Unknown,
}

/// Why the last send attempt failed. Drives the Resend / Edit affordance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FailReason {
    /// No connection / DNS / TLS failure.
    Network,
    /// The server answered 5xx.
    Server,
    /// The request did not complete in time.
    Timeout,
    /// The server answered 429; wait `retry_after_secs` from `failed_at` before resending.
    RateLimited { retry_after_secs: u64 },
    /// The payload exceeded the server's size limit (413). Edit only.
    TooLarge,
    /// The server rejected the payload (4xx with a reason). Edit only.
    Rejected { reason: String },
    /// The client version is no longer accepted by the server. Edit only.
    TooOld,
    /// The addon was unloaded while the message was still `Sending`.
    Interrupted,
}

/// One feedback message persisted locally, with the server's reply once known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalMessage {
    /// UUID v4 string, minted when the draft is created.
    pub report_id: String,
    /// Short server-assigned id, once the server has acknowledged the report.
    #[serde(default)]
    pub short_id: Option<String>,
    /// Unix seconds when the message was sent.
    pub sent_at: u64,
    pub category: String,
    pub path: Vec<String>,
    pub title: String,
    pub body: String,
    pub status: MessageStatus,
    #[serde(default)]
    pub reply: Option<String>,
    /// RFC 3339 timestamp exactly as received from the server.
    #[serde(default)]
    pub replied_at: Option<String>,
    #[serde(default)]
    pub closing_note: Option<String>,
    #[serde(default)]
    pub last_error: Option<FailReason>,
    /// Unix seconds of the last failure (drives the rate-limit countdown).
    #[serde(default)]
    pub failed_at: Option<u64>,
    /// The exact JSON that was sent; replayed verbatim by Resend.
    #[serde(default)]
    pub failed_payload: Option<String>,
    /// Human-readable context line, e.g. `v1.6.0 · game 174122 · en · WvW / Roam · Druid → Untamed · Gemini`.
    #[serde(default)]
    pub context_summary: String,
}

/// The last category and wizard path the player used, to pre-select next time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastPath {
    pub category: String,
    pub path: Vec<String>,
}

/// On-disk shape of `messages.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessagesFile {
    #[serde(default)]
    pub last_path: Option<LastPath>,
    #[serde(default)]
    pub messages: Vec<LocalMessage>,
}

impl LocalMessage {
    /// True when the server may still change this message's state (worth polling).
    pub fn pollable(&self) -> bool {
        matches!(
            self.status,
            MessageStatus::Received | MessageStatus::Read | MessageStatus::Answered
        )
    }

    /// True when the failed payload may be replayed as-is at time `now` (unix seconds).
    ///
    /// Transient failures (network, server, timeout, interrupted, or no recorded reason)
    /// may always be resent. `TooOld`, `TooLarge`, and `Rejected` require editing.
    /// `RateLimited` opens once `failed_at + retry_after_secs` has passed.
    pub fn resend_allowed(&self, now: u64) -> bool {
        if self.status != MessageStatus::Failed {
            return false;
        }
        match &self.last_error {
            None
            | Some(FailReason::Network)
            | Some(FailReason::Server)
            | Some(FailReason::Timeout)
            | Some(FailReason::Interrupted) => true,
            Some(FailReason::TooOld)
            | Some(FailReason::TooLarge)
            | Some(FailReason::Rejected { .. }) => false,
            Some(FailReason::RateLimited { retry_after_secs }) => {
                now >= self
                    .failed_at
                    .unwrap_or(0)
                    .saturating_add(*retry_after_secs)
            }
        }
    }

    /// True for messages that were never sent to the server.
    pub fn is_local(&self) -> bool {
        self.status == MessageStatus::Local
    }
}

/// Current unix time in whole seconds; 0 if the clock is before the epoch.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(status: MessageStatus) -> LocalMessage {
        LocalMessage {
            report_id: "r-1".into(),
            short_id: None,
            sent_at: 1_000,
            category: "bug".into(),
            path: vec!["area_screen".into(), "severity".into()],
            title: "Title".into(),
            body: "Body".into(),
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

    fn failed(reason: Option<FailReason>, failed_at: Option<u64>) -> LocalMessage {
        LocalMessage {
            last_error: reason,
            failed_at,
            ..msg(MessageStatus::Failed)
        }
    }

    #[test]
    fn status_serde_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&MessageStatus::Answered).unwrap(),
            "\"answered\""
        );
        assert_eq!(
            serde_json::from_str::<MessageStatus>("\"answered\"").unwrap(),
            MessageStatus::Answered
        );

        let all = [
            (MessageStatus::Sending, "sending"),
            (MessageStatus::Received, "received"),
            (MessageStatus::Read, "read"),
            (MessageStatus::Answered, "answered"),
            (MessageStatus::Closed, "closed"),
            (MessageStatus::Failed, "failed"),
            (MessageStatus::Local, "local"),
            (MessageStatus::Unknown, "unknown"),
        ];
        for (status, text) in all {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            let back: MessageStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn fail_reason_serde_round_trips() {
        let all = [
            FailReason::Network,
            FailReason::Server,
            FailReason::Timeout,
            FailReason::RateLimited {
                retry_after_secs: 90,
            },
            FailReason::TooLarge,
            FailReason::Rejected { reason: "x".into() },
            FailReason::TooOld,
            FailReason::Interrupted,
        ];
        for reason in all {
            let json = serde_json::to_string(&reason).unwrap();
            let back: FailReason = serde_json::from_str(&json).unwrap();
            assert_eq!(back, reason, "round-trip of {json}");
        }

        // Internally tagged with snake_case variant names.
        let rl = serde_json::to_value(FailReason::RateLimited {
            retry_after_secs: 90,
        })
        .unwrap();
        assert_eq!(rl["kind"], "rate_limited");
        assert_eq!(rl["retry_after_secs"], 90);
        let rj = serde_json::to_value(FailReason::Rejected { reason: "x".into() }).unwrap();
        assert_eq!(rj["kind"], "rejected");
        assert_eq!(rj["reason"], "x");
        assert_eq!(
            serde_json::to_value(FailReason::TooOld).unwrap()["kind"],
            "too_old"
        );
    }

    #[test]
    fn pollable_only_for_received_read_answered() {
        assert!(msg(MessageStatus::Received).pollable());
        assert!(msg(MessageStatus::Read).pollable());
        assert!(msg(MessageStatus::Answered).pollable());

        assert!(!msg(MessageStatus::Sending).pollable());
        assert!(!msg(MessageStatus::Closed).pollable());
        assert!(!msg(MessageStatus::Failed).pollable());
        assert!(!msg(MessageStatus::Local).pollable());
        assert!(!msg(MessageStatus::Unknown).pollable());
    }

    #[test]
    fn resend_rules() {
        let now = 10_000;

        // Not failed: never resendable, whatever last_error says.
        assert!(!msg(MessageStatus::Received).resend_allowed(now));
        assert!(!LocalMessage {
            last_error: Some(FailReason::Network),
            ..msg(MessageStatus::Sending)
        }
        .resend_allowed(now));

        // Failed with a transient (or unrecorded) reason: allowed.
        assert!(failed(None, None).resend_allowed(now));
        assert!(failed(Some(FailReason::Network), None).resend_allowed(now));
        assert!(failed(Some(FailReason::Server), None).resend_allowed(now));
        assert!(failed(Some(FailReason::Timeout), None).resend_allowed(now));
        assert!(failed(Some(FailReason::Interrupted), None).resend_allowed(now));

        // Permanent reasons: edit only.
        assert!(!failed(Some(FailReason::TooOld), None).resend_allowed(now));
        assert!(!failed(Some(FailReason::TooLarge), None).resend_allowed(now));
        assert!(!failed(
            Some(FailReason::Rejected {
                reason: "nope".into()
            }),
            None
        )
        .resend_allowed(now));

        // Rate limited: blocked until failed_at + retry_after_secs.
        let rl = failed(
            Some(FailReason::RateLimited {
                retry_after_secs: 90,
            }),
            Some(now),
        );
        assert!(!rl.resend_allowed(now));
        assert!(!rl.resend_allowed(now + 89));
        assert!(rl.resend_allowed(now + 90));
        assert!(rl.resend_allowed(now + 500));

        // Rate limited with no failed_at recorded: the window counts from 0.
        let rl_no_ts = failed(
            Some(FailReason::RateLimited {
                retry_after_secs: 90,
            }),
            None,
        );
        assert!(!rl_no_ts.resend_allowed(89));
        assert!(rl_no_ts.resend_allowed(90));
    }

    #[test]
    fn is_local_only_for_local() {
        assert!(msg(MessageStatus::Local).is_local());
        assert!(!msg(MessageStatus::Received).is_local());
        assert!(!msg(MessageStatus::Failed).is_local());
    }

    #[test]
    fn messages_file_backward_compat() {
        let a: MessagesFile = serde_json::from_str(r#"{"messages":[]}"#).unwrap();
        assert!(a.messages.is_empty());
        assert!(a.last_path.is_none());

        let b: MessagesFile = serde_json::from_str("{}").unwrap();
        assert!(b.messages.is_empty());
        assert!(b.last_path.is_none());
        assert_eq!(b, MessagesFile::default());

        // A message written before any of the optional fields existed still parses.
        let minimal = r#"{
            "messages": [{
                "report_id": "abc",
                "sent_at": 42,
                "category": "bug",
                "path": ["area_screen"],
                "title": "t",
                "body": "b",
                "status": "received"
            }]
        }"#;
        let c: MessagesFile = serde_json::from_str(minimal).unwrap();
        assert_eq!(c.messages.len(), 1);
        let m = &c.messages[0];
        assert_eq!(m.report_id, "abc");
        assert_eq!(m.status, MessageStatus::Received);
        assert!(m.short_id.is_none());
        assert!(m.reply.is_none());
        assert!(m.replied_at.is_none());
        assert!(m.closing_note.is_none());
        assert!(m.last_error.is_none());
        assert!(m.failed_at.is_none());
        assert!(m.failed_payload.is_none());
        assert_eq!(m.context_summary, "");

        // Full round-trip including last_path and a failure record.
        let full = MessagesFile {
            last_path: Some(LastPath {
                category: "bug".into(),
                path: vec!["area_screen".into()],
            }),
            messages: vec![failed(
                Some(FailReason::RateLimited {
                    retry_after_secs: 5,
                }),
                Some(7),
            )],
        };
        let json = serde_json::to_string_pretty(&full).unwrap();
        let back: MessagesFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, full);
    }

    #[test]
    fn now_unix_is_plausible() {
        // 2024-01-01T00:00:00Z; anything earlier means the clock or the helper is broken.
        assert!(now_unix() > 1_704_067_200);
    }
}
