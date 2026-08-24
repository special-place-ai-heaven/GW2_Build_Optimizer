use gw2bo_feedback::app::{router, AppState};
use gw2bo_feedback::config::Config;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = Arc::new(Config::from_env().expect("config"));
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&config.database_url)
        .await
        .expect("database");
    sqlx::migrate!("./migrations").run(&pool).await.expect("migrations");
    let state = AppState::new(pool, config.clone()).await;
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await.expect("bind");
    tracing::info!("feedback listening on {}", config.bind_addr);
    axum::serve(listener, router(state).into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); })
        .await
        .expect("serve");
}
