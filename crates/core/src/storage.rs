//! Build persistence — save/load optimizer results as JSON files.
//! Each saved build is one JSON file in `{addon_dir}/saves/`.
//!
//! Every write puts the complete build in a temp file and flushes it before
//! publishing it under the real name, so an interrupted save can lose the new
//! build but never the old one and never leaves a half-written `.json` behind.
//! `save_new` publishes by hard link because that is also the claim on the
//! name; `save_overwrite` publishes by rename because it already owns it.

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
    /// Two-phase write, in the order that matters: (1) write the complete
    /// build to a private staging file and flush it, then (2) claim the
    /// destination by hard-linking that file into place. Content first means
    /// `<name>.json` only ever exists whole — a crash anywhere in phase 1
    /// leaves nothing but a staging file that `list` ignores.
    ///
    /// Publishing by link (not by `create_new` on the final path, and not by
    /// `rename`) is what keeps both halves of the promise: the link refuses an
    /// existing destination, so two concurrent `save_new` calls with the same
    /// name still produce exactly one winner and one `AlreadyExists`, and it
    /// carries the bytes with it, so the claim is never an empty file.
    pub fn save_new(&self, build: &SavedBuild) -> Result<(), String> {
        std::fs::create_dir_all(&self.saves_dir)
            .map_err(|e| format!("Failed to create saves dir: {}", e))?;

        let filename = sanitize_filename(&build.name);
        if filename.is_empty() {
            return Err("Build name is empty".into());
        }

        let path = self.saves_dir.join(format!("{}.json", filename));
        let json = serde_json::to_string_pretty(build)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        // Phase 1: the complete build lands in a staging file nobody else can
        // pick, and is flushed to the device, before anything claims the real
        // name.
        let staged = self.staging_path(&filename);
        if let Err(e) = write_durably(&staged, &json) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("Failed to write {}: {}", staged.display(), e));
        }

        // Phase 2: publish content that is already on disk, so `<name>.json` is
        // never observable empty or half-written.
        let published = publish_new(&staged, &path).map_err(|e| match e {
            PublishError::Taken => format!(
                "A build with filename '{}' already exists \
                 (names that differ only in special characters collide)",
                filename
            ),
            PublishError::Io(e) => format!("Failed to create {}: {}", path.display(), e),
        });

        // The staged copy has done its job either way. On the hard-link path the
        // published file is a second name for the same content, not a move, so
        // this drops a link rather than the build.
        let _ = std::fs::remove_file(&staged);
        published
    }

    /// A private, unique destination for the in-progress copy of a build.
    ///
    /// Unique because `save_new` writes the content *before* it claims the final
    /// name, so two concurrent calls for the same build name are both staging at
    /// the same moment and must not share a file. The extension is `.tmp`, never
    /// `.json`, so `list` skips it.
    fn staging_path(&self, filename: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);

        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        self.saves_dir
            .join(format!("{}.{}-{}.tmp", filename, std::process::id(), seq))
    }

    /// Overwrite an existing saved build on disk. Fails if the file does NOT exist.
    ///
    /// The replacement is written to `<name>.tmp` and flushed before the rename
    /// publishes it, so an interrupted overwrite leaves the previous build
    /// intact rather than a truncated one.
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

        // Complete content first, then one atomic rename over the old build.
        let tmp_path = path.with_extension("tmp");
        if let Err(e) = write_durably(&tmp_path, &json) {
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
    /// Thin wrapper over [`Self::list_with_skipped`] for callers that don't
    /// need the names of skipped (corrupt) files. Existing callers and their
    /// return type stay untouched.
    pub fn list(&self) -> Vec<SavedBuild> {
        self.list_with_skipped().0
    }

    /// List all saved builds, sorted by timestamp descending (newest first),
    /// plus the basenames of any `.json` files in the saves directory that
    /// could not be parsed. One directory scan produces both — corrupt files
    /// used to be dropped with only an `eprintln!` that an injected DLL's
    /// caller never sees; the names are now returned so the UI can show the
    /// player which saves were skipped.
    ///
    /// Streams each save file via `BufReader` rather than `read_to_string` so
    /// large saved-build collections don't pay double allocation (raw text +
    /// parsed values) for every file.
    pub fn list_with_skipped(&self) -> (Vec<SavedBuild>, Vec<String>) {
        let Ok(entries) = std::fs::read_dir(&self.saves_dir) else {
            return (Vec::new(), Vec::new());
        };

        let mut builds: Vec<SavedBuild> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.extension().is_some_and(|ext| ext == "json") {
                continue;
            }
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            let reader = std::io::BufReader::new(file);
            match serde_json::from_reader(reader) {
                Ok(build) => builds.push(build),
                Err(_) => {
                    // Corrupt save file — skip but don't crash. Record the
                    // basename so callers can surface it, not just this log line.
                    eprintln!("Warning: corrupt save file skipped: {}", path.display());
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        skipped.push(name.to_string());
                    }
                }
            }
        }

        builds.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
        (builds, skipped)
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

/// Why a completed staging file could not be published under its final name.
enum PublishError {
    /// The name is held by something that is not ours to replace.
    Taken,
    Io(std::io::Error),
}

/// Publish a complete staging file at `path` without ever leaving an empty or
/// partial file there, and without replacing an existing build.
///
/// `hard_link` is the one std primitive that both refuses an existing
/// destination and publishes content that is already on disk, so the name is
/// claimed and filled in a single step. `rename` cannot take that role: on
/// Windows it silently replaces the destination, which would turn a name
/// collision into a lost build.
fn publish_new(staged: &Path, path: &Path) -> Result<(), PublishError> {
    match std::fs::hard_link(staged, path) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return publish_by_rename(staged, path),
    }

    // A zero-length `<name>.json` is not a build. Versions up to 1.7.0 claimed
    // the name with an empty file before writing any content, so a crash in
    // that window wedged the name for good: the save never appears in `list`,
    // so the UI cannot offer it for deletion, while every retry reports
    // "already exists". Reclaim exactly that shape — never a file with bytes
    // in it, never a directory — and only once, so a live competitor that
    // publishes between the two attempts still wins the race.
    if !is_empty_file(path) || std::fs::remove_file(path).is_err() {
        return Err(PublishError::Taken);
    }
    match std::fs::hard_link(staged, path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(PublishError::Taken),
        Err(_) => publish_by_rename(staged, path),
    }
}

/// Fallback for volumes that cannot hard-link (exFAT, some network shares).
///
/// ponytail: `rename` replaces the destination on Windows instead of failing,
/// so the existence check ahead of it is a check and not a claim — two
/// processes racing the same build name on such a volume can end with the
/// later save winning instead of erroring. Crash safety is unaffected: the
/// file being renamed is already complete. Upgrade path is a real lock file if
/// anyone ever reports it.
fn publish_by_rename(staged: &Path, path: &Path) -> Result<(), PublishError> {
    if path.exists() {
        return Err(PublishError::Taken);
    }
    std::fs::rename(staged, path).map_err(PublishError::Io)
}

/// True only for a regular file of exactly zero bytes.
fn is_empty_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() == 0)
}

/// Write `contents` to `path` and flush it to the device before returning, so
/// the rename or link that publishes this file cannot become durable ahead of
/// the bytes it publishes.
fn write_durably(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
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
            slot_prefixes: None,
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

    /// Names of everything in the saves dir that is not a published `.json`.
    fn non_json_entries(saves_dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(saves_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_none_or(|ext| ext != "json"))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn save_new_no_empty_claim() {
        // save_new used to claim `<name>.json` with a zero-byte file before it had
        // any content to put there. That file is what this test refuses to allow:
        // an interrupted save left a 0-byte `.json` that `list` skips — so the UI
        // has no row to offer for deletion — while every retry answered "already
        // exists". The build name was wedged with no in-app way out, and the
        // in-flight content, which only ever reached `.tmp`, was gone.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = temp_dir("no_empty_claim");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let saves_dir = dir.join("saves");

        // 1. A published build is complete the moment it exists, and staging
        //    leaves nothing behind.
        storage.save_new(&test_build("Complete")).unwrap();
        let published = saves_dir.join("Complete.json");
        assert!(
            std::fs::metadata(&published).unwrap().len() > 0,
            "a published save must never be a zero-byte file",
        );
        let stored: SavedBuild =
            serde_json::from_str(&std::fs::read_to_string(&published).unwrap()).unwrap();
        assert_eq!(stored.name, "Complete");
        assert!(
            non_json_entries(&saves_dir).is_empty(),
            "staging files must not survive a save: {:?}",
            non_json_entries(&saves_dir),
        );

        // 2. A real build still owns its name — the collision contract survives
        //    the new publish path, and a rejected save leaves the stored build
        //    untouched rather than replacing it (what a bare rename would do).
        let mut second = test_build("Complete");
        second.stat_prefix = "Viper's".into();
        let err = storage.save_new(&second).unwrap_err();
        assert!(
            err.contains("already exists"),
            "expected a collision, got: {}",
            err
        );
        let after: SavedBuild =
            serde_json::from_str(&std::fs::read_to_string(&published).unwrap()).unwrap();
        assert_eq!(
            after.stat_prefix, "Berserker's",
            "a rejected save must not touch the stored build"
        );
        assert!(non_json_entries(&saves_dir).is_empty());

        // 3. The wedge repair: a 0-byte `.json` — all an interrupted pre-1.7.1
        //    save could leave — is not a build, so the name is reclaimable.
        let wedged = saves_dir.join("Wedged.json");
        std::fs::write(&wedged, "").unwrap();
        assert!(
            storage.list().iter().all(|b| b.name != "Wedged"),
            "a 0-byte save is invisible to list(), which is why it wedged the name",
        );
        storage
            .save_new(&test_build("Wedged"))
            .expect("a 0-byte leftover must not wedge the build name");
        let recovered: SavedBuild =
            serde_json::from_str(&std::fs::read_to_string(&wedged).unwrap()).unwrap();
        assert_eq!(recovered.name, "Wedged");

        // 4. Reclaiming stops at exactly that shape: anything else on the name is
        //    reported, never deleted.
        let occupied = saves_dir.join("Occupied.json");
        std::fs::create_dir_all(&occupied).unwrap();
        let err = storage.save_new(&test_build("Occupied")).unwrap_err();
        assert!(
            err.contains("already exists"),
            "an occupied name must be reported as a collision, got: {}",
            err,
        );
        assert!(
            occupied.is_dir(),
            "save_new must not remove what it did not create"
        );

        // 5. The invariant itself: while saves are in flight, the final path is
        //    never observable as an empty file. Content is written and flushed
        //    before the name is claimed, so there is no window for the watcher to
        //    catch — the old claim-then-write order had one on every save.
        let watched = saves_dir.join("Watched.json");
        let done = Arc::new(AtomicBool::new(false));
        let saw_empty = Arc::new(AtomicBool::new(false));
        let watcher = {
            let done = Arc::clone(&done);
            let saw_empty = Arc::clone(&saw_empty);
            let watched = watched.clone();
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    if let Ok(meta) = std::fs::metadata(&watched) {
                        if meta.is_file() && meta.len() == 0 {
                            saw_empty.store(true, Ordering::Relaxed);
                        }
                    }
                }
            })
        };
        for _ in 0..20 {
            storage.save_new(&test_build("Watched")).unwrap();
            std::fs::remove_file(&watched).unwrap();
        }
        done.store(true, Ordering::Relaxed);
        watcher.join().unwrap();
        assert!(
            !saw_empty.load(Ordering::Relaxed),
            "`<name>.json` was observable as a 0-byte file during a save",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn publish_by_rename_refuses_existing() {
        // The no-hard-link fallback (exFAT, some network shares) never runs on a
        // developer or CI filesystem, so exercise it directly. It must still
        // refuse an occupied name instead of replacing the build sitting there,
        // which is exactly what a bare `rename` does on Windows.
        let dir = temp_dir("publish_rename");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let staged = dir.join("staged.tmp");
        let path = dir.join("Existing.json");
        std::fs::write(&staged, "replacement").unwrap();
        std::fs::write(&path, "original").unwrap();

        assert!(matches!(
            publish_by_rename(&staged, &path),
            Err(PublishError::Taken)
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        assert!(
            staged.exists(),
            "a refused publish must leave the staging file to clean up"
        );

        std::fs::remove_file(&path).unwrap();
        assert!(publish_by_rename(&staged, &path).is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement");
        assert!(
            !staged.exists(),
            "a rename publish consumes the staging file"
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
    fn list_with_skipped_reports_corrupt_names() {
        // C29: `list_with_skipped` must return the basenames of corrupt files
        // it drops, in the same directory scan that produces the good builds,
        // so a caller can surface them instead of relying on `eprintln!`.
        let dir = temp_dir("list_with_skipped");
        let _ = std::fs::remove_dir_all(&dir);

        let storage = BuildStorage::new(&dir);
        let good = test_build("GoodBuild");
        storage.save_new(&good).unwrap();

        let saves_dir = dir.join("saves");
        std::fs::write(saves_dir.join("corrupt.json"), "{ not json }").unwrap();
        std::fs::write(saves_dir.join("readme.txt"), "hello").unwrap();

        let (builds, skipped) = storage.list_with_skipped();
        assert_eq!(builds.len(), 1, "the one good build should survive");
        assert_eq!(builds[0].name, "GoodBuild");
        assert_eq!(
            skipped,
            vec!["corrupt.json".to_string()],
            "the corrupt basename should be reported, and the non-json file ignored",
        );

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
