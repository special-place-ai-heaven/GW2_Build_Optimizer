//! Application configuration — API keys and user preferences.
//! Stored as JSON in the Nexus addon directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub gw2_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub cache_build_number: Option<u32>,
}

impl AppConfig {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    pub fn has_gw2_key(&self) -> bool {
        self.gw2_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn has_gemini_key(&self) -> bool {
        self.gemini_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    pub fn is_setup_complete(&self) -> bool {
        self.has_gw2_key() && self.has_gemini_key() && self.cache_build_number.is_some()
    }

    pub fn config_path(addon_dir: &Path) -> PathBuf {
        addon_dir.join("config.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert!(!config.is_setup_complete());
        assert!(!config.has_gw2_key());
        assert!(!config.has_gemini_key());
    }

    #[test]
    fn test_save_and_load() {
        let dir = env::temp_dir().join(format!("gw2_config_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let config = AppConfig {
            gw2_api_key: Some("test-key-123".into()),
            gemini_api_key: Some("gemini-key-456".into()),
            cache_build_number: Some(12345),
        };
        config.save(&path).unwrap();

        let loaded = AppConfig::load(&path);
        assert_eq!(loaded.gw2_api_key.as_deref(), Some("test-key-123"));
        assert!(loaded.is_setup_complete());

        std::fs::remove_dir_all(&dir).ok();
    }
}
