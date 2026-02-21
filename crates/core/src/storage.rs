//! Build persistence — save/load optimizer results as JSON files.
//! Each saved build is one JSON file in `{addon_dir}/saves/`.

use std::path::{Path, PathBuf};

use crate::types::SavedBuild;

pub struct BuildStorage {
    saves_dir: PathBuf,
}

impl BuildStorage {
    pub fn new(addon_dir: &Path) -> Self {
        let saves_dir = addon_dir.join("saves");
        Self { saves_dir }
    }

    /// Save a build to disk. Filename is derived from the build name.
    pub fn save(&self, build: &SavedBuild) -> Result<(), String> {
        std::fs::create_dir_all(&self.saves_dir)
            .map_err(|e| format!("Failed to create saves dir: {}", e))?;

        let filename = sanitize_filename(&build.name);
        if filename.is_empty() {
            return Err("Build name is empty".into());
        }

        let path = self.saves_dir.join(format!("{}.json", filename));
        let json = serde_json::to_string_pretty(build)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

        Ok(())
    }

    /// List all saved builds, sorted by timestamp descending (newest first).
    pub fn list(&self) -> Vec<SavedBuild> {
        let Ok(entries) = std::fs::read_dir(&self.saves_dir) else {
            return Vec::new();
        };

        let mut builds: Vec<SavedBuild> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "json")
            })
            .filter_map(|e| {
                let json = std::fs::read_to_string(e.path()).ok()?;
                serde_json::from_str(&json).ok()
            })
            .collect();

        builds.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        builds
    }

    /// Delete a saved build by name.
    pub fn delete(&self, name: &str) -> Result<(), String> {
        let filename = sanitize_filename(name);
        let path = self.saves_dir.join(format!("{}.json", filename));
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete: {}", e))?;
        }
        Ok(())
    }
}

/// Sanitize a build name for use as a filename.
/// Replaces unsafe characters with underscores.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("My Build (v2)"), "My Build _v2_");
        assert_eq!(sanitize_filename("power/dps"), "power_dps");
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn test_save_and_list() {
        let dir = std::env::temp_dir().join("gw2_test_storage");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let build = SavedBuild {
            name: "Test Build".into(),
            timestamp: 1000,
            character_name: "TestChar".into(),
            game_mode: crate::types::GameMode::PvE,
            label: "Build 1".into(),
            stat_prefix: "Berserker's".into(),
            specializations: vec![],
            weapons: vec![],
            skills: vec![],
            rune: String::new(),
            sigils: vec![],
            relic: String::new(),
            explanation: String::new(),
            changes_made: vec![],
            estimated_stats: None,
        };

        storage.save(&build).unwrap();
        let list = storage.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test Build");
        assert_eq!(list[0].stat_prefix, "Berserker's");

        storage.delete("Test Build").unwrap();
        let list = storage.list();
        assert!(list.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
