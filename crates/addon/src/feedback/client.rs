//! HTTP client for the feedback server (`https://feedback.robagentic.tech`).
//!
//! Split into a thin blocking transport (`post_report`, `fetch_status`,
//! `fetch_taxonomy`) and a pure classifier (`classify`) that maps a
//! `TransportOutcome` to what the message row should become. Only the
//! classifier is unit-tested; the transport is a few reqwest calls.

use std::time::Duration;

use gw2_core::feedback::message::{FailReason, MessageStatus};
use reqwest::header::{HeaderMap, HeaderValue};

/// Feedback server base URL. The addon never changes this contract.
pub const BASE: &str = "https://feedback.robagentic.tech";

/// Fail-fast budget for every request; a send that takes longer is `Timeout`.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Longest raw 400 body kept as a `Rejected` reason.
const MAX_REASON_CHARS: usize = 200;

/// Default `RateLimited` wait when the server sends 429 without `retry-after`.
const DEFAULT_RETRY_AFTER_SECS: u64 = 60;

/// What the wire did, before any interpretation.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportOutcome {
    /// DNS / TCP / TLS / builder failure — nothing reached the server.
    ConnectError,
    /// The request did not complete within `TIMEOUT`.
    Timeout,
    /// The server answered.
    Status {
        code: u16,
        /// `retry-after` header as integer seconds, when present and parseable.
        retry_after: Option<u64>,
        /// Response body as text (empty when unreadable).
        body: String,
    },
}

/// What a `POST /v1/reports` attempt means for the message row.
#[derive(Debug, Clone, PartialEq)]
pub enum SendResult {
    /// 201 — the server holds the report under `short_id`. A replay of an
    /// already-known `report_id` returns the original id and its current status.
    Created {
        short_id: String,
        status: MessageStatus,
    },
    Failed(FailReason),
}

/// One row of `GET /v1/reports/status`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct StatusRow {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub reply: Option<String>,
    #[serde(default)]
    pub replied_at: Option<String>,
    #[serde(default)]
    pub closing_note: Option<String>,
}

/// Body of a 201.
#[derive(serde::Deserialize)]
struct CreatedBody {
    id: String,
    status: String,
}

/// Server error envelope (`error.rs` on the server side).
#[derive(serde::Deserialize)]
struct ErrorEnvelope {
    #[serde(default)]
    error: String,
    #[serde(default)]
    reason: String,
}

/// Map a transport outcome to a row outcome. Pure; see the contract table in
/// `specs/002-about-tab/contracts/feedback-server-v1.md`.
pub fn classify(o: TransportOutcome) -> SendResult {
    let (code, retry_after, body) = match o {
        TransportOutcome::ConnectError => return SendResult::Failed(FailReason::Network),
        TransportOutcome::Timeout => return SendResult::Failed(FailReason::Timeout),
        TransportOutcome::Status {
            code,
            retry_after,
            body,
        } => (code, retry_after, body),
    };

    match code {
        201 => match serde_json::from_str::<CreatedBody>(&body) {
            Ok(created) => SendResult::Created {
                short_id: created.id,
                status: parse_message_status(&created.status),
            },
            Err(_) => SendResult::Failed(FailReason::Server),
        },
        400 => SendResult::Failed(FailReason::Rejected {
            reason: rejection_reason(&body),
        }),
        // Body never parsed: the body-limit layer may answer with plain text.
        413 => SendResult::Failed(FailReason::TooLarge),
        426 => SendResult::Failed(FailReason::TooOld),
        429 => SendResult::Failed(FailReason::RateLimited {
            retry_after_secs: retry_after.unwrap_or(DEFAULT_RETRY_AFTER_SECS),
        }),
        _ => SendResult::Failed(FailReason::Server),
    }
}

/// Server status string → enum. Unknown strings degrade to `Received` so a
/// newer server cannot strand a freshly created row.
fn parse_message_status(s: &str) -> MessageStatus {
    match s {
        "read" => MessageStatus::Read,
        "answered" => MessageStatus::Answered,
        "closed" => MessageStatus::Closed,
        _ => MessageStatus::Received,
    }
}

/// `reason` from the envelope, else `error`, else the trimmed raw body (capped).
fn rejection_reason(body: &str) -> String {
    if let Ok(env) = serde_json::from_str::<ErrorEnvelope>(body) {
        if !env.reason.is_empty() {
            return env.reason;
        }
        if !env.error.is_empty() {
            return env.error;
        }
    }
    body.trim().chars().take(MAX_REASON_CHARS).collect()
}

/// A fresh 5 s client. `None` when reqwest cannot build one (TLS backend
/// missing) — callers treat that as a connect error.
fn client() -> Option<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .ok()
}

/// Headers sent on every request: addon version gate + identifying user agent.
fn headers(addon_version: &str) -> HeaderMap {
    let mut map = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(addon_version) {
        map.insert("X-Addon-Version", v);
    }
    if let Ok(v) = HeaderValue::from_str(&format!("GW2BuildOptimizer/{addon_version}")) {
        map.insert(reqwest::header::USER_AGENT, v);
    }
    map
}

/// `retry-after` as integer seconds (RFC 7231 delta-seconds form only).
fn parse_retry_after(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// `reqwest::Error` → outcome. A timeout is the only thing we distinguish.
fn transport_error(e: reqwest::Error) -> TransportOutcome {
    if e.is_timeout() {
        TransportOutcome::Timeout
    } else {
        TransportOutcome::ConnectError
    }
}

/// `POST /v1/reports` with `json` sent verbatim. Never panics; every failure
/// is a `TransportOutcome` for `classify`.
pub fn post_report(json: &str, addon_version: &str) -> TransportOutcome {
    let Some(client) = client() else {
        return TransportOutcome::ConnectError;
    };
    let resp = client
        .post(format!("{BASE}/v1/reports"))
        .headers(headers(addon_version))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(json.to_owned())
        .send();
    match resp {
        Ok(resp) => {
            let code = resp.status().as_u16();
            let retry_after = parse_retry_after(resp.headers());
            let body = resp.text().unwrap_or_default();
            TransportOutcome::Status {
                code,
                retry_after,
                body,
            }
        }
        Err(e) => transport_error(e),
    }
}

/// `GET /v1/reports/status?ids=A,B&client_id=<uuid>` for up to 50 ids.
/// `Err(())` on any transport or non-200 outcome; the caller only needs to
/// know the refresh did not happen.
pub fn fetch_status(
    ids: &[String],
    client_id: &str,
    addon_version: &str,
) -> Result<Vec<StatusRow>, ()> {
    let client = client().ok_or(())?;
    // ids are base32 short ids and client_id a UUID: no URL encoding needed.
    let ids = ids
        .iter()
        .take(50)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",");
    let resp = client
        .get(format!(
            "{BASE}/v1/reports/status?ids={ids}&client_id={client_id}"
        ))
        .headers(headers(addon_version))
        .send()
        .map_err(|_| ())?;
    if resp.status().as_u16() != 200 {
        return Err(());
    }
    resp.json::<Vec<StatusRow>>().map_err(|_| ())
}

/// `GET /v1/taxonomy` → raw JSON on 200, else `None`.
pub fn fetch_taxonomy() -> Option<String> {
    let resp = client()?
        .get(format!("{BASE}/v1/taxonomy"))
        .headers(headers(crate::VERSION))
        .send()
        .ok()?;
    if resp.status().as_u16() != 200 {
        return None;
    }
    resp.text().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gw2_core::feedback::message::{FailReason, MessageStatus};

    fn status(code: u16, body: &str) -> TransportOutcome {
        TransportOutcome::Status {
            code,
            retry_after: None,
            body: body.to_owned(),
        }
    }

    #[test]
    fn created_received() {
        let out = classify(status(201, r#"{"id":"A3F9K2QD","status":"received"}"#));
        assert_eq!(
            out,
            SendResult::Created {
                short_id: "A3F9K2QD".to_owned(),
                status: MessageStatus::Received,
            }
        );
    }

    #[test]
    fn created_replay_answered() {
        let out = classify(status(201, r#"{"id":"A3F9K2QD","status":"answered"}"#));
        assert_eq!(
            out,
            SendResult::Created {
                short_id: "A3F9K2QD".to_owned(),
                status: MessageStatus::Answered,
            }
        );
        // Every documented status maps; an unknown one degrades to Received.
        for (s, want) in [
            ("read", MessageStatus::Read),
            ("closed", MessageStatus::Closed),
            ("banana", MessageStatus::Received),
        ] {
            let body = format!(r#"{{"id":"X","status":"{s}"}}"#);
            assert_eq!(
                classify(status(201, &body)),
                SendResult::Created {
                    short_id: "X".to_owned(),
                    status: want,
                }
            );
        }
    }

    #[test]
    fn created_bad_body_is_server() {
        assert_eq!(
            classify(status(201, "not json")),
            SendResult::Failed(FailReason::Server)
        );
        assert_eq!(
            classify(status(201, r#"{"status":"received"}"#)),
            SendResult::Failed(FailReason::Server)
        );
    }

    #[test]
    fn connect_is_network() {
        assert_eq!(
            classify(TransportOutcome::ConnectError),
            SendResult::Failed(FailReason::Network)
        );
    }

    #[test]
    fn timeout_is_timeout() {
        assert_eq!(
            classify(TransportOutcome::Timeout),
            SendResult::Failed(FailReason::Timeout)
        );
    }

    #[test]
    fn server_5xx_is_server() {
        for code in [500u16, 502, 503, 504] {
            assert_eq!(
                classify(status(code, r#"{"error":"db_unavailable","reason":""}"#)),
                SendResult::Failed(FailReason::Server),
                "code {code}"
            );
        }
    }

    #[test]
    fn rate_limited_with_and_without_header() {
        let with = TransportOutcome::Status {
            code: 429,
            retry_after: Some(90),
            body: r#"{"error":"rate_limited","reason":""}"#.to_owned(),
        };
        assert_eq!(
            classify(with),
            SendResult::Failed(FailReason::RateLimited {
                retry_after_secs: 90
            })
        );
        assert_eq!(
            classify(status(429, "")),
            SendResult::Failed(FailReason::RateLimited {
                retry_after_secs: 60
            })
        );
    }

    #[test]
    fn too_large_ignores_body() {
        assert_eq!(
            classify(status(413, "Payload too large")),
            SendResult::Failed(FailReason::TooLarge)
        );
    }

    #[test]
    fn too_old() {
        assert_eq!(
            classify(status(
                426,
                r#"{"error":"addon_too_old","reason":"update the addon"}"#
            )),
            SendResult::Failed(FailReason::TooOld)
        );
    }

    #[test]
    fn rejected_uses_reason() {
        assert_eq!(
            classify(status(
                400,
                r#"{"error":"bad_request","reason":"title must be 1..120 characters"}"#
            )),
            SendResult::Failed(FailReason::Rejected {
                reason: "title must be 1..120 characters".to_owned()
            })
        );
    }

    #[test]
    fn rejected_falls_back_to_error_then_raw() {
        assert_eq!(
            classify(status(400, r#"{"error":"bad_request","reason":""}"#)),
            SendResult::Failed(FailReason::Rejected {
                reason: "bad_request".to_owned()
            })
        );
        assert_eq!(
            classify(status(400, "  Bad Request\n")),
            SendResult::Failed(FailReason::Rejected {
                reason: "Bad Request".to_owned()
            })
        );
        // Raw bodies are capped at 200 chars so a huge HTML page cannot bloat the row.
        let long = "x".repeat(500);
        match classify(status(400, &long)) {
            SendResult::Failed(FailReason::Rejected { reason }) => {
                assert_eq!(reason.chars().count(), 200)
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn unknown_status_is_server() {
        assert_eq!(
            classify(status(418, "I'm a teapot")),
            SendResult::Failed(FailReason::Server)
        );
    }

    #[test]
    fn status_row_deserializes_nulls() {
        let row: StatusRow = serde_json::from_str(
            r#"{"id":"A3F9K2QD","status":"answered","reply":null,"replied_at":null,"closing_note":null}"#,
        )
        .expect("row parses");
        assert_eq!(
            row,
            StatusRow {
                id: "A3F9K2QD".to_owned(),
                status: "answered".to_owned(),
                reply: None,
                replied_at: None,
                closing_note: None,
            }
        );
        // Missing optional keys are equivalent to null.
        let bare: StatusRow =
            serde_json::from_str(r#"{"id":"B","status":"received"}"#).expect("bare row parses");
        assert_eq!(bare.reply, None);
    }

    #[test]
    fn retry_after_header_parses_delta_seconds_only() {
        let mut h = HeaderMap::new();
        h.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_static(" 42 "),
        );
        assert_eq!(parse_retry_after(&h), Some(42));
        h.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
        );
        assert_eq!(parse_retry_after(&h), None);
        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }

    #[test]
    fn headers_carry_version_and_user_agent() {
        let h = headers("1.6.0");
        assert_eq!(
            h.get("X-Addon-Version").and_then(|v| v.to_str().ok()),
            Some("1.6.0")
        );
        assert_eq!(
            h.get(reqwest::header::USER_AGENT)
                .and_then(|v| v.to_str().ok()),
            Some("GW2BuildOptimizer/1.6.0")
        );
    }
}
