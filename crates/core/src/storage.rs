//! Build persistence — save/load optimizer results as JSON files.
//! Each saved build is one JSON file in `{addon_dir}/saves/`.
//! All writes use the crash-safe temp-write + atomic rename pattern
//! (matching `AppConfig::save()` in config.rs).

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

    /// Save a new build to disk. Fails if a file with this name already exists.
    /// Uses crash-safe temp-write + atomic rename pattern.
    pub fn save_new(&self, build: &SavedBuild) -> Result<(), String> {
        std::fs::create_dir_all(&self.saves_dir)
            .map_err(|e| format!("Failed to create saves dir: {}", e))?;

        let filename = sanitize_filename(&build.name);
        if filename.is_empty() {
            return Err("Build name is empty".into());
        }

        let path = self.saves_dir.join(format!("{}.json", filename));
        if path.exists() {
            return Err(format!(
                "A build with filename '{}' already exists \
                 (names that differ only in special characters collide)",
                filename
            ));
        }

        let json = serde_json::to_string_pretty(build)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        // Crash-safe: write to .tmp then atomic rename to .json
        let tmp_path = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp_path, &json) {
            let _ = std::fs::remove_file(&tmp_path); // best-effort cleanup
            return Err(format!("Failed to write {}: {}", tmp_path.display(), e));
        }
        std::fs::rename(&tmp_path, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path); // best-effort cleanup
            format!(
                "Failed to rename {} → {}: {}",
                tmp_path.display(),
                path.display(),
                e
            )
        })
    }

    /// Overwrite an existing saved build on disk. Fails if the file does NOT exist.
    /// Uses crash-safe temp-write + atomic rename pattern.
    pub fn save_overwrite(&self, build: &SavedBuild) -> Result<(), String> {
        let filename = sanitize_filename(&build.name);
        if filename.is_empty() {
            return Err("Build name is empty".into());
        }

        let path = self.saves_dir.join(format!("{}.json", filename));
        if !path.exists() {
            return Err(format!(
                "Build '{}' not found on disk — cannot overwrite",
                build.name
            ));
        }

        let json = serde_json::to_string_pretty(build)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        // Crash-safe: write to .tmp then atomic rename to .json
        let tmp_path = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp_path, &json) {
            let _ = std::fs::remove_file(&tmp_path); // best-effort cleanup
            return Err(format!("Failed to write {}: {}", tmp_path.display(), e));
        }
        std::fs::rename(&tmp_path, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path); // best-effort cleanup
            format!(
                "Failed to rename {} → {}: {}",
                tmp_path.display(),
                path.display(),
                e
            )
        })
    }

    /// Save a build to disk with explicit overwrite control.
    /// `overwrite = false` → fails if file exists (like `save_new`).
    /// `overwrite = true` → fails if file does NOT exist (like `save_overwrite`).
    pub fn save(&self, build: &SavedBuild, overwrite: bool) -> Result<(), String> {
        if overwrite {
            self.save_overwrite(build)
        } else {
            self.save_new(build)
        }
    }

    /// List all saved builds, sorted by timestamp descending (newest first).
    pub fn list(&self) -> Vec<SavedBuild> {
        let Ok(entries) = std::fs::read_dir(&self.saves_dir) else {
            return Vec::new();
        };

        let mut builds: Vec<SavedBuild> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|e| {
                let path = e.path();
                let json = std::fs::read_to_string(&path).ok()?;
                match serde_json::from_str(&json) {
                    Ok(build) => Some(build),
                    Err(_) => {
                        // Corrupt save file — skip but don't crash
                        eprintln!("Warning: corrupt save file skipped: {}", path.display());
                        None
                    }
                }
            })
            .collect();

        builds.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        builds
    }

    /// Delete a saved build by name.
    pub fn delete(&self, name: &str) -> Result<(), String> {
        let filename = sanitize_filename(name);
        let path = self.saves_dir.join(format!("{}.json", filename));
        if !path.exists() {
            return Err(format!("Build '{}' not found on disk", name));
        }
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete: {}", e))?;
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

    /// Create a minimal SavedBuild for testing.
    fn test_build(name: &str) -> SavedBuild {
        SavedBuild {
            name: name.into(),
            timestamp: 1000,
            character_name: "TestChar".into(),
            game_mode: crate::types::GameMode::PvE,
            profession: "Necromancer".into(),
            engine_version: "1.0.0".into(),
            balance_manifest_version: None,
            label: "Build 1".into(),
            stat_prefix: "Berserker's".into(),
            specializations: vec![],
            weapons: vec![],
            skills: vec![],
            rune: String::new(),
            sigils: vec![],
            relic: String::new(),
            explanation: String::new(),
            synergy_explanation: String::new(),
            changes_made: vec![],
            estimated_stats: None,
        }
    }

    /// Create a unique temp dir for a test.
    fn temp_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("gw2_test_storage_{}_{}", std::process::id(), label))
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("My Build (v2)"), "My Build _v2_");
        assert_eq!(sanitize_filename("power/dps"), "power_dps");
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn test_save_and_list() {
        let dir = temp_dir("save_and_list");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let build = test_build("Test Build");

        storage.save_new(&build).unwrap();
        let list = storage.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test Build");
        assert_eq!(list[0].stat_prefix, "Berserker's");
        assert_eq!(list[0].profession, "Necromancer");
        assert_eq!(list[0].engine_version, "1.0.0");
        assert!(list[0].balance_manifest_version.is_none());

        storage.delete("Test Build").unwrap();
        let list = storage.list();
        assert!(list.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_backward_compat_no_new_fields() {
        // JSON without profession, engine_version, balance_manifest_version
        // must deserialize with defaults (empty string, empty string, None).
        let json = r#"{
            "name": "Old Build",
            "timestamp": 500,
            "character_name": "OldChar",
            "game_mode": "PvE",
            "label": "Legacy",
            "stat_prefix": "Viper's",
            "specializations": [],
            "weapons": [],
            "skills": [],
            "rune": "",
            "sigils": [],
            "relic": "",
            "explanation": "",
            "changes_made": [],
            "estimated_stats": null
        }"#;
        let saved: SavedBuild = serde_json::from_str(json).unwrap();
        assert_eq!(saved.profession, "");
        assert_eq!(saved.engine_version, "");
        assert!(saved.balance_manifest_version.is_none());
        assert_eq!(saved.name, "Old Build");
    }

    #[test]
    fn test_round_trip_with_new_fields() {
        let build = SavedBuild {
            profession: "Necromancer".into(),
            engine_version: "1.0.0".into(),
            balance_manifest_version: Some("2026-03-06".into()),
            ..test_build("Round Trip Build")
        };
        let json = serde_json::to_string_pretty(&build).unwrap();
        let deserialized: SavedBuild = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.profession, "Necromancer");
        assert_eq!(deserialized.engine_version, "1.0.0");
        assert_eq!(
            deserialized.balance_manifest_version.as_deref(),
            Some("2026-03-06")
        );
        assert_eq!(deserialized.name, "Round Trip Build");
    }

    #[test]
    fn test_crash_safe_save_new() {
        // Verify .tmp is written then renamed to .json (no .tmp left behind)
        let dir = temp_dir("crash_new");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let build = test_build("CrashSafe");

        storage.save_new(&build).unwrap();

        let json_path = dir.join("saves").join("CrashSafe.json");
        let tmp_path = dir.join("saves").join("CrashSafe.tmp");
        assert!(json_path.exists(), ".json file must exist after save");
        assert!(!tmp_path.exists(), ".tmp file must not remain after save");

        // Verify the file contains valid JSON
        let contents = std::fs::read_to_string(&json_path).unwrap();
        let loaded: SavedBuild = serde_json::from_str(&contents).unwrap();
        assert_eq!(loaded.name, "CrashSafe");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_new_collision() {
        // save_new must fail if file already exists
        let dir = temp_dir("collision");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let build = test_build("Duplicate");

        storage.save_new(&build).unwrap();
        let result = storage.save_new(&build);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_overwrite_replaces_existing() {
        let dir = temp_dir("overwrite");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let mut build = test_build("Overwrite Me");
        storage.save_new(&build).unwrap();

        // Change a field and overwrite
        build.stat_prefix = "Viper's".into();
        storage.save_overwrite(&build).unwrap();

        let list = storage.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].stat_prefix, "Viper's");

        // .tmp must not remain
        let tmp_path = dir.join("saves").join("Overwrite Me.tmp");
        assert!(!tmp_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_overwrite_missing_file() {
        // save_overwrite must fail if file doesn't exist
        let dir = temp_dir("overwrite_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("saves")).unwrap();

        let storage = BuildStorage::new(&dir);
        let build = test_build("Nonexistent");

        let result = storage.save_overwrite(&build);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_convenience_method() {
        // Test the save(build, overwrite) convenience method
        let dir = temp_dir("save_convenience");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let build = test_build("Convenience");

        // save(false) = save_new
        storage.save(&build, false).unwrap();
        assert_eq!(storage.list().len(), 1);

        // save(false) again = collision
        assert!(storage.save(&build, false).is_err());

        // save(true) = overwrite existing
        storage.save(&build, true).unwrap();
        assert_eq!(storage.list().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
