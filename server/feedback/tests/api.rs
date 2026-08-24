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
