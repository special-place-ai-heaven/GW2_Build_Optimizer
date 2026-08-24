use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub admin_token: String,
    pub ip_salt: String,
    pub min_addon_version: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        fn need(k: &str) -> Result<String, String> {
            std::env::var(k).map_err(|_| format!("missing env {k}"))
        }
        Ok(Self {
            database_url: need("DATABASE_URL")?,
            bind_addr: std::env::var("BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".into())
                .parse()
                .map_err(|e| format!("BIND_ADDR: {e}"))?,
            admin_token: need("FEEDBACK_ADMIN_TOKEN")?,
            ip_salt: need("FEEDBACK_IP_SALT")?,
            min_addon_version: std::env::var("MIN_ADDON_VERSION")
                .unwrap_or_else(|_| "1.6.0".into()),
        })
    }
}
