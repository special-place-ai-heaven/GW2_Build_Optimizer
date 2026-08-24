use axum::body::Body;
use axum::http::{Request, StatusCode};
use gw2bo_feedback::app::{router, AppState};
use gw2bo_feedback::config::Config;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

fn test_config() -> Arc<Config> {
    Arc::new(Config {
        database_url: String::new(), // pool is injected by sqlx::test
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        admin_token: "test-admin-token".into(),
        ip_salt: "test-salt".into(),
        min_addon_version: "1.6.0".into(),
    })
}

async fn state(pool: PgPool) -> AppState {
    AppState::new(pool, test_config()).await
}

#[sqlx::test(migrations = "./migrations")]
async fn healthz_returns_ok(pool: PgPool) {
    let app = router(state(pool).await);
    let res = app
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn schema_has_reports_and_taxonomy(pool: PgPool) {
    let cols: Vec<(String,)> = sqlx::query_as(
        "select column_name::text from information_schema.columns where table_name = 'reports' order by ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<&str> = cols.iter().map(|c| c.0.as_str()).collect();
    for required in ["short_id", "report_id", "client_id", "category", "path", "body", "status", "payload", "ip_hash", "unvalidated", "closing_note"] {
        assert!(names.contains(&required), "missing column {required}");
    }
    let (n,): (i64,) = sqlx::query_as("select count(*) from taxonomy").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0);
}

use http_body_util::BodyExt;

async fn json_body(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn taxonomy_is_seeded_and_served(pool: PgPool) {
    let app = router(state(pool.clone()).await);
    let res = app
        .oneshot(Request::builder().uri("/v1/taxonomy").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = json_body(res).await;
    assert_eq!(v["taxonomy_version"], 1);
    assert!(v["categories"].as_array().unwrap().iter().any(|c| c["id"] == "praise"));
    let (n,): (i64,) = sqlx::query_as("select count(*) from taxonomy").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1, "seeded exactly once");
}

#[test]
fn taxonomy_validate_knows_its_ids() {
    use gw2bo_feedback::taxonomy::Taxonomy;
    let t = Taxonomy::embedded();
    assert!(t.validate("bug", &["optimize".into(), "wrong".into()]));
    assert!(t.validate("praise", &["everything".into()]));
    assert!(!t.validate("bug", &["optimize".into(), "nope".into()]));
    assert!(!t.validate("teleport", &[]));
}

fn report_json(report_id: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "report_id": report_id,
        "client_id": "11111111-1111-4111-8111-111111111111",
        "category": "bug",
        "path": ["optimize", "wrong"],
        "title": "Optimize picks Trident on land",
        "body": "Expected a land weapon, got Trident on a Ranger in WvW roam.",
        "contact": null,
        "account": null,
        "context": { "addon_version": "1.6.0", "game_build": 174122, "locale": "en",
                     "mode": "WvW", "scale": "Roam", "role": "Damage",
                     "profession": "Ranger", "elite": "Untamed", "llm_provider": "gemini" },
        "build_snapshot": null
    })
}

fn post_report(body: serde_json::Value, ip: &str, addon_version: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/reports")
        .header("content-type", "application/json")
        .header("x-forwarded-for", ip)
        .header("x-addon-version", addon_version)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn post_report_creates_row_and_returns_short_id(pool: PgPool) {
    let app = router(state(pool.clone()).await);
    let res = app.oneshot(post_report(report_json("22222222-2222-4222-8222-222222222222"), "203.0.113.5", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let v = json_body(res).await;
    assert_eq!(v["status"], "received");
    assert_eq!(v["id"].as_str().unwrap().len(), 8);
    let (cat, unval, ip_hash): (String, bool, String) =
        sqlx::query_as("select category, unvalidated, ip_hash from reports").fetch_one(&pool).await.unwrap();
    assert_eq!(cat, "bug");
    assert!(!unval);
    assert_ne!(ip_hash, "203.0.113.5", "raw ip must never be stored");
}

#[sqlx::test(migrations = "./migrations")]
async fn post_same_report_id_twice_is_one_row_same_short_id(pool: PgPool) {
    let st = state(pool.clone()).await;
    let a = json_body(router(st.clone()).oneshot(post_report(report_json("33333333-3333-4333-8333-333333333333"), "203.0.113.5", "1.6.0")).await.unwrap()).await;
    let b = json_body(router(st).oneshot(post_report(report_json("33333333-3333-4333-8333-333333333333"), "203.0.113.5", "1.6.0")).await.unwrap()).await;
    assert_eq!(a["id"], b["id"]);
    let (n,): (i64,) = sqlx::query_as("select count(*) from reports").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn post_unknown_category_is_stored_unvalidated(pool: PgPool) {
    let mut body = report_json("44444444-4444-4444-8444-444444444444");
    body["category"] = "teleport".into();
    let res = router(state(pool.clone()).await).oneshot(post_report(body, "203.0.113.5", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let (unval,): (bool,) = sqlx::query_as("select unvalidated from reports").fetch_one(&pool).await.unwrap();
    assert!(unval);
}

#[sqlx::test(migrations = "./migrations")]
async fn post_rejects_too_long_body_and_bad_shape(pool: PgPool) {
    let st = state(pool).await;
    let mut long = report_json("55555555-5555-4555-8555-555555555555");
    long["body"] = "x".repeat(4001).into();
    let res = router(st.clone()).oneshot(post_report(long, "203.0.113.5", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let huge = serde_json::json!({ "pad": "y".repeat(20_000) });
    let res = router(st.clone()).oneshot(post_report(huge, "203.0.113.5", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let res = router(st).oneshot(post_report(report_json("66666666-6666-4666-8666-666666666666"), "203.0.113.5", "1.5.3")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UPGRADE_REQUIRED);
}

#[sqlx::test(migrations = "./migrations")]
async fn eleventh_post_in_a_minute_from_one_ip_is_429(pool: PgPool) {
    let st = state(pool).await;
    for i in 0..10 {
        let id = format!("77777777-7777-4777-8777-7777777777{:02}", i);
        let res = router(st.clone()).oneshot(post_report(report_json(&id), "198.51.100.9", "1.6.0")).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED, "request {i}");
    }
    let res = router(st).oneshot(post_report(report_json("77777777-7777-4777-8777-777777777799"), "198.51.100.9", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = res.headers()["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry >= 1 && retry <= 60);
}

#[test]
fn limiter_sliding_window() {
    use gw2bo_feedback::ratelimit::RateLimiter;
    use std::time::Duration;
    let l = RateLimiter::new();
    for _ in 0..3 { assert!(l.check("k", 3, Duration::from_secs(60)).is_ok()); }
    let err = l.check("k", 3, Duration::from_secs(60)).unwrap_err();
    assert!(err >= 1);
    assert!(l.check("other", 3, Duration::from_secs(60)).is_ok());
}

#[test]
fn ids_are_shaped_right() {
    use gw2bo_feedback::ids::{ip_hash, short_id};
    let s = short_id();
    assert_eq!(s.len(), 8);
    assert!(s.chars().all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)));
    let d = chrono::NaiveDate::from_ymd_opt(2026, 8, 24).unwrap();
    assert_eq!(ip_hash("1.2.3.4", "salt", d), ip_hash("1.2.3.4", "salt", d));
    assert_ne!(ip_hash("1.2.3.4", "salt", d), ip_hash("1.2.3.4", "salt", d.succ_opt().unwrap()));
}
