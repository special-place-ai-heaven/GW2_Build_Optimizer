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
