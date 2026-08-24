use crate::app::AppState;
use crate::error::ApiError;
use crate::ids::{ip_hash, short_id};
use axum::extract::{ConnectInfo, Extension, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
use std::time::Duration;
use uuid::Uuid;

pub const MAX_BODY_CHARS: usize = 4000;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_SNAPSHOT_BYTES: usize = 6 * 1024;

#[derive(Debug, Deserialize)]
pub struct NewReport {
    pub schema_version: i16,
    pub report_id: Uuid,
    pub client_id: Uuid,
    pub category: String,
    pub path: Vec<String>,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub contact: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    pub context: Value,
    #[serde(default)]
    pub build_snapshot: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Created {
    pub id: String,
    pub status: String,
}

pub fn client_ip(headers: &HeaderMap, addr: Option<SocketAddr>) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| addr.map(|a| a.ip().to_string()))
        .unwrap_or_else(|| "0.0.0.0".into())
}

fn version_at_least(have: &str, min: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(have) >= parse(min)
}

pub fn check_addon_version(headers: &HeaderMap, min: &str) -> Result<(), ApiError> {
    let have = headers
        .get("x-addon-version")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::BadRequest("missing X-Addon-Version".into()))?;
    if version_at_least(have, min) { Ok(()) } else { Err(ApiError::UpgradeRequired) }
}

pub async fn create(
    State(s): State<AppState>,
    headers: HeaderMap,
    // axum 0.8's `ConnectInfo` does not implement `OptionalFromRequestParts`, so
    // `Option<ConnectInfo<_>>` doesn't compile; `Option<Extension<ConnectInfo<_>>>`
    // reads the same request extension and IS optional-aware.
    addr: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(req): Json<NewReport>,
) -> Result<(StatusCode, Json<Created>), ApiError> {
    check_addon_version(&headers, &s.config.min_addon_version)?;
    if req.body.chars().count() > MAX_BODY_CHARS {
        return Err(ApiError::BadRequest(format!("body over {MAX_BODY_CHARS} characters")));
    }
    if req.title.chars().count() > MAX_TITLE_CHARS || req.title.trim().is_empty() {
        return Err(ApiError::BadRequest(format!("title must be 1..{MAX_TITLE_CHARS} characters")));
    }
    if let Some(snap) = &req.build_snapshot {
        if snap.to_string().len() > MAX_SNAPSHOT_BYTES {
            return Err(ApiError::BadRequest(format!("build_snapshot over {MAX_SNAPSHOT_BYTES} bytes")));
        }
    }
    let unvalidated = !s.taxonomy.read().await.validate(&req.category, &req.path);
    let ip = client_ip(&headers, addr.map(|Extension(ConnectInfo(a))| a));
    let hash = ip_hash(&ip, &s.config.ip_salt, chrono::Utc::now().date_naive());
    s.limiter.check(&format!("ip:{hash}"), 10, Duration::from_secs(60))
        .map_err(|retry_after_secs| ApiError::RateLimited { retry_after_secs })?;
    s.limiter.check(&format!("client:{}", req.client_id), 50, Duration::from_secs(24 * 3600))
        .map_err(|retry_after_secs| ApiError::RateLimited { retry_after_secs })?;
    let addon_version = req.context["addon_version"].as_str().unwrap_or("unknown").to_string();
    let game_build = req.context["game_build"].as_i64();
    let payload = serde_json::to_value(&RawEcho::from(&req)).map_err(|e| ApiError::Internal(e.to_string()))?;

    // Idempotent: a resend with the same report_id returns the original row.
    let row: (String, String) = sqlx::query_as(
        r#"
        insert into reports
          (short_id, report_id, client_id, schema_version, category, path, title, body,
           contact, account, addon_version, game_build, unvalidated, payload, ip_hash)
        values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
        on conflict (report_id) do update set report_id = excluded.report_id
        returning short_id, status
        "#,
    )
    .bind(short_id())
    .bind(req.report_id)
    .bind(req.client_id)
    .bind(req.schema_version)
    .bind(&req.category)
    .bind(&req.path)
    .bind(&req.title)
    .bind(&req.body)
    .bind(&req.contact)
    .bind(&req.account)
    .bind(&addon_version)
    .bind(game_build)
    .bind(unvalidated)
    .bind(&payload)
    .bind(&hash)
    .fetch_one(&s.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(Created { id: row.0, status: row.1 })))
}

#[derive(Deserialize)]
pub struct StatusQuery {
    pub ids: String,
    pub client_id: Uuid,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct StatusRow {
    #[sqlx(rename = "short_id")]
    pub id: String,
    pub status: String,
    pub reply: Option<String>,
    pub replied_at: Option<DateTime<Utc>>,
    pub closing_note: Option<String>,
}

pub async fn status(
    State(s): State<AppState>,
    headers: HeaderMap,
    // See the comment on `create`'s `addr` param: axum 0.8's `ConnectInfo` does
    // not implement `OptionalFromRequestParts`, so `Option<Extension<ConnectInfo<_>>>`
    // is the optional-aware form that reads the same request extension.
    addr: Option<Extension<ConnectInfo<SocketAddr>>>,
    Query(q): Query<StatusQuery>,
) -> Result<Json<Vec<StatusRow>>, ApiError> {
    let ip = client_ip(&headers, addr.map(|Extension(ConnectInfo(a))| a));
    let hash = ip_hash(&ip, &s.config.ip_salt, chrono::Utc::now().date_naive());
    s.limiter.check(&format!("ip:{hash}"), 10, Duration::from_secs(60))
        .map_err(|retry_after_secs| ApiError::RateLimited { retry_after_secs })?;
    let ids: Vec<String> = q.ids.split(',').map(|x| x.trim().to_uppercase()).filter(|x| !x.is_empty()).take(50).collect();
    if ids.is_empty() {
        return Err(ApiError::BadRequest("ids required".into()));
    }
    let rows = sqlx::query_as::<_, StatusRow>(
        "select short_id, status, reply, replied_at, closing_note from reports where short_id = any($1) and client_id = $2",
    )
    .bind(&ids)
    .bind(q.client_id)
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(rows))
}

/// The request as received, re-serialised for the JSONB `payload` column.
#[derive(Serialize)]
struct RawEcho<'a> {
    schema_version: i16,
    report_id: Uuid,
    client_id: Uuid,
    category: &'a str,
    path: &'a [String],
    title: &'a str,
    body: &'a str,
    contact: &'a Option<String>,
    account: &'a Option<String>,
    context: &'a Value,
    build_snapshot: &'a Option<Value>,
}

impl<'a> From<&'a NewReport> for RawEcho<'a> {
    fn from(r: &'a NewReport) -> Self {
        Self {
            schema_version: r.schema_version, report_id: r.report_id, client_id: r.client_id,
            category: &r.category, path: &r.path, title: &r.title, body: &r.body,
            contact: &r.contact, account: &r.account, context: &r.context, build_snapshot: &r.build_snapshot,
        }
    }
}
