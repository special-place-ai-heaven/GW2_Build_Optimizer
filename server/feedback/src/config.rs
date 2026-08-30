use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub admin_token: String,
    pub admin_user: String,
    pub admin_password: String,
    pub session_secret: String,
    pub ip_salt: String,
    pub min_addon_version: String,
    pub trust_xff: bool,
}

fn need(k: &str) -> Result<String, String> {
    let v = std::env::var(k).map_err(|_| format!("missing env {k}"))?;
    let t = v.trim();
    if t.is_empty() {
        Err(format!("empty env {k}"))
    } else {
        Ok(t.to_string())
    }
}

fn env_flag(k: &str) -> bool {
    std::env::var(k)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            database_url: need("DATABASE_URL")?,
            bind_addr: std::env::var("BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".into())
                .parse()
                .map_err(|e| format!("BIND_ADDR: {e}"))?,
            admin_token: need("FEEDBACK_ADMIN_TOKEN")?,
            admin_user: std::env::var("FEEDBACK_ADMIN_USER").unwrap_or_else(|_| "admin".into()),
            admin_password: need("FEEDBACK_ADMIN_PASSWORD")?,
            session_secret: need("FEEDBACK_SESSION_SECRET")?,
            ip_salt: need("FEEDBACK_IP_SALT")?,
            min_addon_version: std::env::var("MIN_ADDON_VERSION")
                .unwrap_or_else(|_| "1.6.0".into()),
            trust_xff: env_flag("FEEDBACK_TRUST_XFF"),
        })
    }
}

#[cfg(test)]
mod from_env_tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENV: Mutex<()> = Mutex::new(());

    const KEYS: &[&str] = &[
        "DATABASE_URL",
        "BIND_ADDR",
        "FEEDBACK_ADMIN_TOKEN",
        "FEEDBACK_ADMIN_USER",
        "FEEDBACK_ADMIN_PASSWORD",
        "FEEDBACK_SESSION_SECRET",
        "FEEDBACK_IP_SALT",
        "MIN_ADDON_VERSION",
        "FEEDBACK_TRUST_XFF",
    ];

    fn lock_env() -> MutexGuard<'static, ()> {
        ENV.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn clear_keys() {
        for k in KEYS {
            // SAFETY: lock_env is held for the whole test; no other thread reads these keys.
            unsafe { std::env::remove_var(k) };
        }
    }

    fn set(k: &str, v: &str) {
        // SAFETY: lock_env is held for the whole test.
        unsafe { std::env::set_var(k, v) };
    }

    fn fill_ok() {
        set("DATABASE_URL", "postgres://u:p@127.0.0.1/db");
        set("FEEDBACK_ADMIN_TOKEN", "tok");
        set("FEEDBACK_ADMIN_PASSWORD", "pw");
        set("FEEDBACK_SESSION_SECRET", "sess");
        set("FEEDBACK_IP_SALT", "salt");
    }

    #[test]
    fn from_env_reads_required_secrets_without_aliasing_the_token() {
        let _g = lock_env();
        clear_keys();
        fill_ok();
        set("FEEDBACK_TRUST_XFF", "1");
        let c = Config::from_env().unwrap();
        assert_eq!(c.admin_token, "tok");
        assert_eq!(c.admin_password, "pw");
        assert_eq!(c.session_secret, "sess");
        assert_eq!(c.ip_salt, "salt");
        assert!(c.trust_xff);
        assert_ne!(c.admin_password, c.admin_token);
        assert_ne!(c.session_secret, c.admin_token);
    }

    #[test]
    fn from_env_trust_xff_defaults_false() {
        let _g = lock_env();
        clear_keys();
        fill_ok();
        let c = Config::from_env().unwrap();
        assert!(!c.trust_xff);
    }

    #[test]
    fn empty_or_whitespace_secrets_fail() {
        let _g = lock_env();
        for key in [
            "FEEDBACK_ADMIN_TOKEN",
            "FEEDBACK_ADMIN_PASSWORD",
            "FEEDBACK_SESSION_SECRET",
            "FEEDBACK_IP_SALT",
        ] {
            clear_keys();
            fill_ok();
            set(key, "   ");
            let err = Config::from_env().unwrap_err();
            assert!(err.contains(key), "{key}: {err}");
        }
    }

    #[test]
    fn missing_password_and_session_secret_fail() {
        let _g = lock_env();
        clear_keys();
        fill_ok();
        unsafe { std::env::remove_var("FEEDBACK_ADMIN_PASSWORD") };
        assert!(Config::from_env()
            .unwrap_err()
            .contains("FEEDBACK_ADMIN_PASSWORD"));
        set("FEEDBACK_ADMIN_PASSWORD", "pw");
        unsafe { std::env::remove_var("FEEDBACK_SESSION_SECRET") };
        assert!(Config::from_env()
            .unwrap_err()
            .contains("FEEDBACK_SESSION_SECRET"));
    }
}
