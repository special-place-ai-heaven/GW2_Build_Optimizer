use crate::config::Config;
use crate::taxonomy::{self, Taxonomy};
use axum::extract::State;
use axum::routing::get;
use axum::Json;
use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub taxonomy: Arc<RwLock<Taxonomy>>,
}

impl AppState {
    pub async fn new(pool: PgPool, config: Arc<Config>) -> Self {
        taxonomy::seed_if_empty(&pool).await.expect("seed taxonomy");
        let current = taxonomy::load_current(&pool).await.expect("load taxonomy");
        Self { pool, config, taxonomy: Arc::new(RwLock::new(current)) }
    }
}

async fn get_taxonomy(State(s): State<AppState>) -> Json<serde_json::Value> {
    Json(s.taxonomy.read().await.body.clone())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/taxonomy", get(get_taxonomy))
        .with_state(state)
}
