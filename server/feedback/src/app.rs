use crate::admin;
use crate::config::Config;
use crate::ratelimit::RateLimiter;
use crate::reports;
use crate::taxonomy::{self, Taxonomy};
use axum::extract::State;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::limit::RequestBodyLimitLayer;

pub const MAX_REQUEST_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub taxonomy: Arc<RwLock<Taxonomy>>,
    pub limiter: Arc<RateLimiter>,
}

impl AppState {
    pub async fn new(pool: PgPool, config: Arc<Config>) -> Self {
        taxonomy::seed_if_empty(&pool).await.expect("seed taxonomy");
        let current = taxonomy::load_current(&pool).await.expect("load taxonomy");
        Self { pool, config, taxonomy: Arc::new(RwLock::new(current)), limiter: Arc::new(RateLimiter::new()) }
    }
}

async fn get_taxonomy(State(s): State<AppState>) -> Json<serde_json::Value> {
    Json(s.taxonomy.read().await.body.clone())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/taxonomy", get(get_taxonomy))
        .route("/v1/reports", post(reports::create))
        .route("/v1/reports/status", get(reports::status))
        .nest("/v1/admin", admin::routes(state.clone()))
        .layer(RequestBodyLimitLayer::new(MAX_REQUEST_BYTES))
        .with_state(state)
}
