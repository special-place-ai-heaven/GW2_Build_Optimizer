use crate::app::AppState;
use crate::config::Config;
use crate::error::ApiError;
use crate::reports::{client_ip, StatusRow};
use crate::taxonomy::Taxonomy;
use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, Extension, Path, Query, Request, State};
use axum::http::header::{HeaderValue, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SESSION_COOKIE: &str = "choya_session";
const SESSION_TTL_SECS: u64 = 7 * 24 * 3600;
type HmacSha256 = Hmac<Sha256>;

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hex_of(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sign(secret: &str, payload: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(payload.as_bytes());
    hex_of(&mac.finalize().into_bytes())
}

fn mint_session(cfg: &Config) -> String {
    let exp = now_secs().saturating_add(SESSION_TTL_SECS);
    let payload = format!("v1.{exp}");
    format!("{payload}.{}", sign(&cfg.session_secret, &payload))
}

fn session_ok(headers: &HeaderMap, cfg: &Config) -> bool {
    let Some(raw) = headers.get("cookie").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(value) = raw.split(';').find_map(|p| {
        let p = p.trim();
        p.strip_prefix(SESSION_COOKIE)
            .and_then(|rest| rest.strip_prefix('='))
            .map(str::trim)
    }) else {
        return false;
    };
    let Some((payload, sig)) = value.rsplit_once('.') else {
        return false;
    };
    let Some((_, exp_s)) = payload.split_once('.') else {
        return false;
    };
    let Ok(exp) = exp_s.parse::<u64>() else {
        return false;
    };
    if exp <= now_secs() {
        return false;
    }
    constant_time_eq(
        sign(&cfg.session_secret, payload).as_bytes(),
        sig.as_bytes(),
    )
}

fn bearer_ok(headers: &HeaderMap, cfg: &Config) -> bool {
    // RFC 7235 auth schemes are case-insensitive, so match on the scheme rather
    // than on the literal "Bearer " prefix.
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| constant_time_eq(t.trim().as_bytes(), cfg.admin_token.as_bytes()))
        .unwrap_or(false)
}

fn authorized(headers: &HeaderMap, cfg: &Config) -> bool {
    bearer_ok(headers, cfg) || session_ok(headers, cfg)
}

fn set_cookie(value: &str, max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={max_age}"
    ))
    .expect("cookie header")
}

async fn require_admin(
    State(s): State<AppState>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if authorized(&headers, &s.config) {
        Ok(next.run(req).await)
    } else {
        Err(ApiError::Unauthorized)
    }
}

#[derive(Deserialize)]
struct LoginBody {
    user: String,
    password: String,
}

fn login_rejected(r: JsonRejection) -> ApiError {
    if r.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::PayloadTooLarge
    } else {
        ApiError::Unauthorized
    }
}

async fn login(
    State(s): State<AppState>,
    headers: HeaderMap,
    addr: Option<Extension<ConnectInfo<SocketAddr>>>,
    body: Result<Json<LoginBody>, JsonRejection>,
) -> Result<Response, ApiError> {
    let ip = client_ip(
        &headers,
        addr.map(|Extension(ConnectInfo(a))| a),
        s.config.trust_xff,
    );
    s.limiter
        .check(&format!("login:{ip}"), 8, Duration::from_secs(15 * 60))
        .map_err(|retry_after_secs| ApiError::RateLimited { retry_after_secs })?;
    let Json(body) = body.map_err(login_rejected)?;
    let user_ok = constant_time_eq(body.user.trim().as_bytes(), s.config.admin_user.as_bytes());
    let pass_ok = constant_time_eq(body.password.as_bytes(), s.config.admin_password.as_bytes());
    if !(user_ok && pass_ok) {
        return Err(ApiError::Unauthorized);
    }
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut().insert(
        SET_COOKIE,
        set_cookie(&mint_session(&s.config), SESSION_TTL_SECS),
    );
    Ok(res)
}

async fn logout(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let mut res = StatusCode::NO_CONTENT.into_response();
    if session_ok(&headers, &s.config) {
        res.headers_mut().insert(SET_COOKIE, set_cookie("", 0));
    }
    res
}

async fn me(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !authorized(&headers, &s.config) {
        return Err(ApiError::Unauthorized);
    }
    Ok(Json(serde_json::json!({ "user": s.config.admin_user })))
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
    pub category: Option<String>,
    pub q: Option<String>,
    pub limit: Option<i64>,
}

const ADMIN_COLS: &str = "short_id, received_at, category, path, title, body, contact, account, addon_version, game_build, status, reply, unvalidated, payload";

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<AdminRow>>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let status = q.status.filter(|s| !s.is_empty());
    let category = q.category.filter(|s| !s.is_empty());
    let like =
        q.q.filter(|s| !s.trim().is_empty())
            .map(|s| format!("%{}%", s.trim()));
    let rows = sqlx::query_as::<_, AdminRow>(&format!(
        "select {ADMIN_COLS} from reports
         where ($1::text is null or status = $1)
           and ($2::text is null or category = $2)
           and ($3::text is null or title ilike $3 or body ilike $3
                or short_id ilike $3 or coalesce(account, '') ilike $3)
         order by received_at desc
         limit $4"
    ))
    .bind(status)
    .bind(category)
    .bind(like)
    .bind(limit)
    .fetch_all(&s.pool)
    .await?;
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
pub struct ReadBody {
    pub ids: Vec<String>,
}

async fn mark_read_ids(
    State(s): State<AppState>,
    Json(b): Json<ReadBody>,
) -> Result<StatusCode, ApiError> {
    let ids: Vec<String> = b.ids.into_iter().map(|id| id.to_uppercase()).collect();
    mark_read(&s, ids).await?;
    Ok(StatusCode::NO_CONTENT)
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

async fn inbox() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("admin.html"))
}

pub fn page_routes() -> Router<AppState> {
    Router::new()
        .route("/admin", get(inbox))
        .route("/admin/login", post(login))
        .route("/admin/logout", post(logout))
        .route("/admin/me", get(me))
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/reports", get(list))
        .route("/reports/read", post(mark_read_ids))
        .route("/reports/{id}", get(get_one))
        .route("/reports/{id}/reply", post(reply))
        .route("/reports/{id}/status", post(set_status))
        .route("/taxonomy", put(put_taxonomy))
        .layer(middleware::from_fn_with_state(state, require_admin))
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use axum::http::HeaderValue;

    fn cfg() -> Config {
        Config {
            database_url: String::new(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            admin_token: "tok".into(),
            admin_user: "admin".into(),
            admin_password: "pw".into(),
            session_secret: "secret".into(),
            ip_salt: "salt".into(),
            min_addon_version: "1.6.0".into(),
            trust_xff: false,
        }
    }

    #[test]
    fn minted_cookie_authorizes() {
        let cfg = cfg();
        let value = mint_session(&cfg);
        let mut h = HeaderMap::new();
        h.insert(
            "cookie",
            format!("{SESSION_COOKIE}={value}").parse().unwrap(),
        );
        assert!(session_ok(&h, &cfg));
        assert!(authorized(&h, &cfg));
    }

    #[test]
    fn garbage_cookie_is_rejected() {
        let cfg = cfg();
        let mut h = HeaderMap::new();
        h.insert(
            "cookie",
            HeaderValue::from_static("choya_session=v1.1.dead"),
        );
        assert!(!session_ok(&h, &cfg));
        assert!(!authorized(&h, &cfg));
    }

    #[test]
    fn bearer_still_authorizes() {
        let cfg = cfg();
        let mut h = HeaderMap::new();
        h.insert("authorization", HeaderValue::from_static("Bearer tok"));
        assert!(authorized(&h, &cfg));
    }

    fn test_state() -> AppState {
        AppState {
            pool: sqlx::PgPool::connect_lazy("postgres://u:p@127.0.0.1:1/db").expect("lazy pool"),
            config: std::sync::Arc::new(cfg()),
            taxonomy: std::sync::Arc::new(tokio::sync::RwLock::new(
                crate::taxonomy::Taxonomy::embedded(),
            )),
            limiter: std::sync::Arc::new(crate::ratelimit::RateLimiter::new()),
        }
    }

    async fn post(app: axum::Router, uri: &str, headers: &[(&str, &str)], body: &str) -> Response {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;
        let mut b = Request::builder().method("POST").uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        app.oneshot(b.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn logout_without_session_is_204_without_set_cookie() {
        let res = post(
            page_routes().with_state(test_state()),
            "/admin/logout",
            &[],
            "",
        )
        .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(res.headers().get(SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn logout_with_garbage_cookie_does_not_set_cookie() {
        let res = post(
            page_routes().with_state(test_state()),
            "/admin/logout",
            &[("cookie", "choya_session=v1.1.dead")],
            "",
        )
        .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(res.headers().get(SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn logout_with_bearer_only_does_not_set_cookie() {
        let res = post(
            page_routes().with_state(test_state()),
            "/admin/logout",
            &[("authorization", "Bearer tok")],
            "",
        )
        .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(res.headers().get(SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn logout_with_session_clears_cookie() {
        let value = mint_session(&cfg());
        let cookie = format!("{SESSION_COOKIE}={value}");
        let res = post(
            page_routes().with_state(test_state()),
            "/admin/logout",
            &[("cookie", &cookie)],
            "",
        )
        .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let set = res.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
        assert!(set.contains("Max-Age=0"), "{set}");
        assert!(set.contains(SESSION_COOKIE), "{set}");
    }

    #[tokio::test]
    async fn mark_read_requires_admin() {
        let res = post(
            crate::app::router(test_state()),
            "/v1/admin/reports/read",
            &[("content-type", "application/json")],
            r#"{"ids":[]}"#,
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn mark_read_empty_ids_is_204() {
        let res = post(
            crate::app::router(test_state()),
            "/v1/admin/reports/read",
            &[
                ("authorization", "Bearer tok"),
                ("content-type", "application/json"),
            ],
            r#"{"ids":[]}"#,
        )
        .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }
}
