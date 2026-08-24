use crate::app::AppState;
use crate::error::ApiError;
use crate::reports::StatusRow;
use crate::taxonomy::Taxonomy;
use axum::extract::{Path, Query, Request, State};
use axum::http::HeaderMap;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

async fn require_token(
    State(s): State<AppState>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let ok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| constant_time_eq(t.as_bytes(), s.config.admin_token.as_bytes()))
        .unwrap_or(false);
    if ok {
        Ok(next.run(req).await)
    } else {
        Err(ApiError::Unauthorized)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AdminRow {
    #[sqlx(rename = "short_id")]
    pub id: String,
    pub received_at: DateTime<Utc>,
    pub category: String,
    pub path: Vec<String>,
    pub title: String,
    pub body: String,
    pub contact: Option<String>,
    pub account: Option<String>,
    pub addon_version: String,
    pub game_build: Option<i64>,
    pub status: String,
    pub reply: Option<String>,
    pub unvalidated: bool,
    pub payload: Value,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

const ADMIN_COLS: &str = "short_id, received_at, category, path, title, body, contact, account, addon_version, game_build, status, reply, unvalidated, payload";

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<AdminRow>>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let rows = match q.status {
        Some(st) => {
            sqlx::query_as::<_, AdminRow>(&format!(
            "select {ADMIN_COLS} from reports where status = $1 order by received_at desc limit $2"
        ))
            .bind(st)
            .bind(limit)
            .fetch_all(&s.pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, AdminRow>(&format!(
                "select {ADMIN_COLS} from reports order by received_at desc limit $1"
            ))
            .bind(limit)
            .fetch_all(&s.pool)
            .await?
        }
    };
    mark_read(&s, rows.iter().map(|r| r.id.clone()).collect()).await?;
    Ok(Json(rows))
}

async fn get_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AdminRow>, ApiError> {
    let row = sqlx::query_as::<_, AdminRow>(&format!(
        "select {ADMIN_COLS} from reports where short_id = $1"
    ))
    .bind(id.to_uppercase())
    .fetch_optional(&s.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    mark_read(&s, vec![row.id.clone()]).await?;
    Ok(Json(row))
}

async fn mark_read(s: &AppState, ids: Vec<String>) -> Result<(), ApiError> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "update reports set status = 'read' where short_id = any($1) and status = 'received'",
    )
    .bind(&ids)
    .execute(&s.pool)
    .await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ReplyBody {
    pub reply: String,
    pub status: String,
}

async fn reply(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<ReplyBody>,
) -> Result<Json<StatusRow>, ApiError> {
    if !matches!(b.status.as_str(), "answered" | "closed") {
        return Err(ApiError::BadRequest(
            "status must be answered or closed".into(),
        ));
    }
    let row = sqlx::query_as::<_, StatusRow>(
        "update reports set reply = $2, replied_at = now(), status = $3 where short_id = $1
         returning short_id, status, reply, replied_at, closing_note",
    )
    .bind(id.to_uppercase())
    .bind(&b.reply)
    .bind(&b.status)
    .fetch_optional(&s.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(row))
}

#[derive(Deserialize)]
pub struct StatusBody {
    pub status: String,
    #[serde(default)]
    pub closing_note: Option<String>,
}

async fn set_status(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<StatusBody>,
) -> Result<Json<StatusRow>, ApiError> {
    if !matches!(
        b.status.as_str(),
        "received" | "read" | "answered" | "closed"
    ) {
        return Err(ApiError::BadRequest("unknown status".into()));
    }
    let row = sqlx::query_as::<_, StatusRow>(
        "update reports set status = $2, closing_note = coalesce($3, closing_note) where short_id = $1
         returning short_id, status, reply, replied_at, closing_note",
    )
    .bind(id.to_uppercase()).bind(&b.status).bind(&b.closing_note)
    .fetch_optional(&s.pool).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(row))
}

async fn put_taxonomy(
    State(s): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let version = body["taxonomy_version"]
        .as_i64()
        .ok_or_else(|| ApiError::BadRequest("taxonomy_version required".into()))?
        as i32;
    let current = s.taxonomy.read().await.version;
    if version <= current {
        return Err(ApiError::BadRequest(format!(
            "taxonomy_version must be > {current}"
        )));
    }
    if !body["categories"].is_array() || !body["steps"].is_object() {
        return Err(ApiError::BadRequest(
            "categories[] and steps{} required".into(),
        ));
    }
    sqlx::query("insert into taxonomy (version, body) values ($1, $2)")
        .bind(version)
        .bind(&body)
        .execute(&s.pool)
        .await?;
    *s.taxonomy.write().await = Taxonomy { version, body };
    Ok(Json(serde_json::json!({ "version": version })))
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/reports", get(list))
        .route("/reports/{id}", get(get_one))
        .route("/reports/{id}/reply", post(reply))
        .route("/reports/{id}/status", post(set_status))
        .route("/taxonomy", put(put_taxonomy))
        .layer(middleware::from_fn_with_state(state, require_token))
}
