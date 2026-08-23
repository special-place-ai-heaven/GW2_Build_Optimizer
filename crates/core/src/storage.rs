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
    ///
    /// Two-phase write: (1) atomically claim the destination path with
    /// `create_new(true)` on a zero-byte placeholder, then (2) write the
    /// content to `<path>.tmp` and rename over our own placeholder. This is
    /// both race-safe (two concurrent `save_new` calls with the same name
    /// can never both succeed — one sees `AlreadyExists` on the claim) and
    /// crash-safe (partial writes land in `.tmp`, never published).
    pub fn save_new(&self, build: &SavedBuild) -> Result<(), String> {
        use std::fs::OpenOptions;

        std::fs::create_dir_all(&self.saves_dir)
            .map_err(|e| format!("Failed to create saves dir: {}", e))?;

        let filename = sanitize_filename(&build.name);
        if filename.is_empty() {
            return Err("Build name is empty".into());
        }

        let path = self.saves_dir.join(format!("{}.json", filename));

        // Phase 1: claim the destination atomically. `create_new(true)` returns
        // AlreadyExists if any other thread/process has already reserved it,
        // which is the collision signal callers rely on.
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_handle) => { /* claim acquired; handle dropped immediately */ }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(format!(
                    "A build with filename '{}' already exists \
                     (names that differ only in special characters collide)",
                    filename
                ));
            }
            Err(e) => {
                return Err(format!("Failed to create {}: {}", path.display(), e));
            }
        }

        let json = match serde_json::to_string_pretty(build) {
            Ok(s) => s,
            Err(e) => {
                // Remove the placeholder so the slot doesn't leak for the user.
                let _ = std::fs::remove_file(&path);
                return Err(format!("Failed to serialize: {}", e));
            }
        };

        // Phase 2: crash-safe content write.
        let tmp_path = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp_path, &json) {
            let _ = std::fs::remove_file(&tmp_path);
            let _ = std::fs::remove_file(&path);
            return Err(format!("Failed to write {}: {}", tmp_path.display(), e));
        }
        std::fs::rename(&tmp_path, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            let _ = std::fs::remove_file(&path);
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
    ///
    /// Streams each save file via `BufReader` rather than `read_to_string` so
    /// large saved-build collections don't pay double allocation (raw text +
    /// parsed values) for every file.
    pub fn list(&self) -> Vec<SavedBuild> {
        let Ok(entries) = std::fs::read_dir(&self.saves_dir) else {
            return Vec::new();
        };

        let mut builds: Vec<SavedBuild> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|e| {
                let path = e.path();
                let file = std::fs::File::open(&path).ok()?;
                let reader = std::io::BufReader::new(file);
                match serde_json::from_reader(reader) {
                    Ok(build) => Some(build),
                    Err(_) => {
                        // Corrupt save file — skip but don't crash
                        eprintln!("Warning: corrupt save file skipped: {}", path.display());
                        None
                    }
                }
            })
            .collect();

        builds.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
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
            gear_prefixes: crate::types::GearPrefixGroups::default(),
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
            notes: String::new(),
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
        assert_eq!(saved.notes, "");
        assert_eq!(saved.name, "Old Build");
    }

    #[test]
    fn test_forward_compat_ignores_unknown_fields() {
        // If a future DLL adds a new field to `SavedBuild` and the user then
        // downgrades, the older binary must still load the saved file. Serde's
        // default behavior is to ignore unknown fields — this test locks that
        // in (i.e. guards against a future `#[serde(deny_unknown_fields)]`
        // being accidentally added).
        let json = r#"{
            "name": "Future Build",
            "timestamp": 9000,
            "character_name": "FutureChar",
            "game_mode": "PvE",
            "profession": "Necromancer",
            "engine_version": "9.9.9",
            "balance_manifest_version": "2099-01-01",
            "label": "Future",
            "stat_prefix": "Berserker's",
            "specializations": [],
            "weapons": [],
            "skills": [],
            "rune": "",
            "sigils": [],
            "relic": "",
            "explanation": "",
            "synergy_explanation": "",
            "changes_made": [],
            "estimated_stats": null,
            "unknown_scalar_from_the_future": 42,
            "unknown_string_from_the_future": "hello",
            "unknown_nested_from_the_future": {"foo": ["a", "b"], "bar": true}
        }"#;
        let saved: SavedBuild = serde_json::from_str(json)
            .expect("SavedBuild must tolerate unknown fields for DLL-downgrade safety");
        assert_eq!(saved.name, "Future Build");
        assert_eq!(saved.profession, "Necromancer");
        assert_eq!(
            saved.balance_manifest_version.as_deref(),
            Some("2099-01-01"),
        );
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
    fn test_save_new_concurrent_race() {
        // Two threads call save_new on the same filename at the same time.
        // The contract promises that save_new rejects collisions (see
        // `test_save_new_collision`); under contention exactly one thread
        // must win and the other must error with "already exists". No .tmp
        // leftover should remain. Repeated across many iterations to widen
        // the scheduler race window.
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;

        let dir = temp_dir("save_new_race");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = Arc::new(BuildStorage::new(&dir));
        let saves_dir = dir.join("saves");

        const ITERATIONS: usize = 50;
        let successes = Arc::new(AtomicUsize::new(0));
        let collisions = Arc::new(AtomicUsize::new(0));
        let other_errors = Arc::new(AtomicUsize::new(0));

        for i in 0..ITERATIONS {
            let name = format!("Race_{}", i);

            let s1 = Arc::clone(&storage);
            let s2 = Arc::clone(&storage);
            let n1 = name.clone();
            let n2 = name.clone();
            let suc1 = Arc::clone(&successes);
            let suc2 = Arc::clone(&successes);
            let col1 = Arc::clone(&collisions);
            let col2 = Arc::clone(&collisions);
            let err1 = Arc::clone(&other_errors);
            let err2 = Arc::clone(&other_errors);

            let classify = move |result: Result<(), String>,
                                 suc: &AtomicUsize,
                                 col: &AtomicUsize,
                                 err: &AtomicUsize| {
                match result {
                    Ok(()) => {
                        suc.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(msg) if msg.contains("already exists") => {
                        col.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => {
                        err.fetch_add(1, Ordering::SeqCst);
                    }
                }
            };

            let c1 = classify;
            let c2 = classify;

            let t1 = thread::spawn(move || {
                let build = test_build(&n1);
                c1(s1.save_new(&build), &suc1, &col1, &err1);
            });
            let t2 = thread::spawn(move || {
                let build = test_build(&n2);
                c2(s2.save_new(&build), &suc2, &col2, &err2);
            });
            t1.join().unwrap();
            t2.join().unwrap();

            let json_path = saves_dir.join(format!("{}.json", name));
            let tmp_path = saves_dir.join(format!("{}.tmp", name));
            assert!(
                json_path.exists(),
                "final .json must exist after race for {}",
                name
            );
            assert!(
                !tmp_path.exists(),
                ".tmp must not leak after race for {}",
                name
            );
        }

        let suc = successes.load(Ordering::SeqCst);
        let col = collisions.load(Ordering::SeqCst);
        let err = other_errors.load(Ordering::SeqCst);

        assert_eq!(
            err, 0,
            "unexpected non-collision errors during race ({} total)",
            err,
        );
        // Contract: exactly one winner per race, exactly one collision rejection.
        assert_eq!(
            suc, ITERATIONS,
            "expected one success per race ({} iterations), got {} successes + {} collisions",
            ITERATIONS, suc, col,
        );
        assert_eq!(
            col, ITERATIONS,
            "expected one collision rejection per race ({} iterations), got {} collisions + {} successes",
            ITERATIONS, col, suc,
        );

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

    #[test]
    fn test_list_skips_corrupt_json_files() {
        // list() must silently skip unparseable .json files (logging a
        // warning) and return only the valid ones. A single corrupt file
        // dropped in by a crashed editor or partial sync must not take the
        // whole saved-build list down with it.
        let dir = temp_dir("list_corrupt");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let good = test_build("GoodBuild");
        storage.save_new(&good).unwrap();

        // Drop a malformed .json alongside and a non-json file that must
        // also be ignored by the extension filter.
        let saves_dir = dir.join("saves");
        std::fs::write(saves_dir.join("corrupt.json"), "{ not json }").unwrap();
        std::fs::write(saves_dir.join("readme.txt"), "hello").unwrap();

        let builds = storage.list();
        assert_eq!(
            builds.len(),
            1,
            "exactly one good build should survive the corrupt file",
        );
        assert_eq!(builds[0].name, "GoodBuild");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_delete_missing_returns_err() {
        // delete() of a name that was never saved must error with the
        // user-visible "not found on disk" message. Covers the early-return
        // branch in storage.rs so callers can surface the failure to the UI.
        let dir = temp_dir("delete_missing");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        std::fs::create_dir_all(dir.join("saves")).unwrap();

        let result = storage.delete("NeverSaved");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("not found on disk"),
            "expected 'not found on disk' in error, got: {}",
            msg,
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
