//! Local JSON file cache with staleness detection.
//! Each cache file stores metadata (build number, timestamp) alongside the data.
//! Cache is invalidated when the GW2 game build number changes.

use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Wrapper around cached data with metadata for staleness checks.
#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry<T> {
    build: u32,
    fetched_at: DateTime<Utc>,
    data: T,
}

/// Local file-based cache for GW2 API data.
pub struct DataCache {
    base_path: PathBuf,
}

impl DataCache {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        let path = base_path.into();
        std::fs::create_dir_all(&path).ok();
        Self { base_path: path }
    }

    /// Save data to cache with current build number.
    ///
    /// Uses crash-safe temp-write + atomic rename. If serialization or write
    /// fails partway, the orphan `.tmp` file is best-effort removed so it does
    /// not accumulate after repeated failed downloads.
    pub fn save<T: Serialize>(&self, key: &str, data: &T, build: u32) -> Result<(), CacheError> {
        let entry = CacheEntry {
            build,
            fetched_at: Utc::now(),
            data,
        };
        let path = self.path_for(key);
        let tmp_path = self.base_path.join(format!("{}.tmp", key));
        let result = (|| -> Result<(), CacheError> {
            let file = std::fs::File::create(&tmp_path)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer(&mut writer, &entry)?;
            writer.flush()?;
            std::fs::rename(&tmp_path, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
        result
    }

    /// Load data from cache. Returns None if file doesn't exist.
    ///
    /// Streams via `BufReader` + `serde_json::from_reader` rather than reading
    /// the whole file into a `String`. The items cache is ~50 MB of JSON; the
    /// old `read_to_string` approach paid for both the raw text and the parsed
    /// values simultaneously.
    pub fn load<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, CacheError> {
        let path = self.path_for(key);
        if !path.exists() {
            return Ok(None);
        }
        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let entry: CacheEntry<T> = serde_json::from_reader(reader)?;
        Ok(Some(entry.data))
    }

    /// Check if the cached entry's build number differs from `current_build`.
    ///
    /// Any mismatch — including rollback (cached > current) — is treated as
    /// stale. Callers are expected to refetch on `true`.
    ///
    /// Streams via `BufReader` + `serde(deny_unknown_fields = false)` so the
    /// 50 MB items cache is not pulled fully into a `String` just to extract
    /// the 4-byte `build` field.
    pub fn is_stale(&self, key: &str, current_build: u32) -> bool {
        let path = self.path_for(key);
        if !path.exists() {
            return true;
        }
        let Ok(file) = std::fs::File::open(&path) else {
            return true;
        };
        let reader = BufReader::new(file);
        // Parse just the metadata, not the full data
        #[derive(Deserialize)]
        struct Meta {
            build: u32,
        }
        let Ok(meta) = serde_json::from_reader::<_, Meta>(reader) else {
            return true;
        };
        meta.build != current_build
    }

    /// Get the cached build number for a key, if it exists.
    pub fn cached_build(&self, key: &str) -> Option<u32> {
        let path = self.path_for(key);
        let file = std::fs::File::open(&path).ok()?;
        let reader = BufReader::new(file);
        #[derive(Deserialize)]
        struct Meta {
            build: u32,
        }
        serde_json::from_reader::<_, Meta>(reader)
            .ok()
            .map(|m| m.build)
    }

    /// Delete a cache entry.
    pub fn delete(&self, key: &str) {
        let path = self.path_for(key);
        std::fs::remove_file(&path).ok();
    }

    /// Clear all cached data (every `*.json` / `*.tmp` entry under `base_path`).
    ///
    /// Contract: **best-effort**. If any individual entry cannot be removed,
    /// the remaining entries are still attempted; the per-entry failures are
    /// returned as `CacheError::PartialClear`. A missing cache directory is
    /// treated as success (nothing to clear). Non-matching extensions are
    /// left untouched.
    pub fn clear_all(&self) -> Result<(), CacheError> {
        let entries = match std::fs::read_dir(&self.base_path) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(CacheError::Io(e)),
        };

        let mut failures: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let is_clearable = path
                .extension()
                .is_some_and(|ext| ext == "json" || ext == "tmp");
            if !is_clearable {
                continue;
            }
            if let Err(e) = std::fs::remove_file(&path) {
                failures.push(format!("{}: {}", path.display(), e));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(CacheError::PartialClear { failures })
        }
    }

    /// Check if a cache key exists.
    pub fn exists(&self, key: &str) -> bool {
        self.path_for(key).exists()
    }

    // --- Character-specific cache methods ---
    // Character data changes independently of game patches, so these use
    // simple JSON files without build-number invalidation.

    /// Save character-specific data (build tabs, equipment tabs).
    /// Key format: `char_{sanitized_name}_{data_type}.json`
    ///
    /// Crash-safe temp+rename with orphan `.tmp` cleanup on failure.
    pub fn save_character<T: Serialize>(
        &self,
        character: &str,
        data_type: &str,
        data: &T,
    ) -> Result<(), CacheError> {
        let key = format!("char_{}_{}", sanitize_name(character), data_type);
        let path = self.path_for(&key);
        let tmp_path = self.base_path.join(format!("{}.tmp", key));
        let result = (|| -> Result<(), CacheError> {
            let file = std::fs::File::create(&tmp_path)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer(&mut writer, data)?;
            writer.flush()?;
            std::fs::rename(&tmp_path, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
        result
    }

    /// Load character-specific cached data. Returns None if not cached.
    pub fn load_character<T: DeserializeOwned>(
        &self,
        character: &str,
        data_type: &str,
    ) -> Result<Option<T>, CacheError> {
        let key = format!("char_{}_{}", sanitize_name(character), data_type);
        let path = self.path_for(&key);
        if !path.exists() {
            return Ok(None);
        }
        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(Some(data))
    }

    /// Save the character name list. Crash-safe temp+rename with orphan
    /// `.tmp` cleanup on failure.
    pub fn save_characters(&self, characters: &[String]) -> Result<(), CacheError> {
        let path = self.path_for("characters");
        let tmp_path = self.base_path.join("characters.tmp");
        let result = (|| -> Result<(), CacheError> {
            let file = std::fs::File::create(&tmp_path)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer(&mut writer, characters)?;
            writer.flush()?;
            std::fs::rename(&tmp_path, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
        result
    }

    /// Load the cached character name list. Returns None if not cached.
    pub fn load_characters(&self) -> Result<Option<Vec<String>>, CacheError> {
        let path = self.path_for("characters");
        if !path.exists() {
            return Ok(None);
        }
        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let data: Vec<String> = serde_json::from_reader(reader)?;
        Ok(Some(data))
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.base_path.join(format!("{}.json", key))
    }
}

/// Sanitize a character name for use in cache filenames.
/// Uses full Unicode lowercase for consistent normalization of non-ASCII names.
fn sanitize_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push('_');
        }
    }
    result
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// `clear_all` attempted every matching entry; this is the list of entries
    /// it could not remove. Best-effort contract — cleared files stay cleared.
    #[error("Cache clear partially failed: {failures:?}")]
    PartialClear { failures: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_CACHE_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_cache() -> DataCache {
        let id = NEXT_CACHE_ID.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("gw2_cache_test_{}_{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        DataCache::new(dir)
    }

    #[test]
    fn test_save_and_load() {
        let cache = temp_cache();
        let data = vec![1u32, 2, 3, 4, 5];
        cache.save("test_data", &data, 12345).unwrap();

        let loaded: Option<Vec<u32>> = cache.load("test_data").unwrap();
        assert_eq!(loaded, Some(vec![1, 2, 3, 4, 5]));

        let _ = cache.clear_all();
    }

    #[test]
    fn test_staleness() {
        let cache = temp_cache();
        cache.save("stale_test", &"hello", 100).unwrap();

        assert!(!cache.is_stale("stale_test", 100)); // same build
        assert!(cache.is_stale("stale_test", 101)); // different build
        assert!(cache.is_stale("nonexistent", 100)); // missing file

        let _ = cache.clear_all();
    }

    #[test]
    fn test_staleness_rollback_is_stale() {
        // Server build number going DOWN (rollback / rollout revert) must
        // invalidate the cache, not be silently trusted as "still fresh".
        let cache = temp_cache();
        cache.save("rollback_test", &"hello", 200).unwrap();

        assert!(cache.is_stale("rollback_test", 150));
        assert!(cache.is_stale("rollback_test", 0));

        let _ = cache.clear_all();
    }

    #[test]
    fn test_load_nonexistent() {
        let cache = temp_cache();
        let result: Option<Vec<u32>> = cache.load("does_not_exist").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_character_cache_roundtrip() {
        let cache = temp_cache();
        let tabs = vec!["tab1".to_string(), "tab2".to_string()];
        cache
            .save_character("Fun Detected", "buildtabs", &tabs)
            .unwrap();

        let loaded: Option<Vec<String>> =
            cache.load_character("Fun Detected", "buildtabs").unwrap();
        assert_eq!(loaded, Some(tabs));

        // Different character returns None
        let other: Option<Vec<String>> = cache.load_character("Other Char", "buildtabs").unwrap();
        assert!(other.is_none());

        let _ = cache.clear_all();
    }

    #[test]
    fn test_characters_list_cache() {
        let cache = temp_cache();
        let chars = vec!["Alpha".into(), "Beta".into(), "Gamma".into()];
        cache.save_characters(&chars).unwrap();

        let loaded = cache.load_characters().unwrap();
        assert_eq!(loaded, Some(chars));

        let _ = cache.clear_all();
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(super::sanitize_name("Fun Detected"), "fun_detected");
        assert_eq!(super::sanitize_name("Ælfred's Toon"), "ælfred_s_toon");
        assert_eq!(super::sanitize_name("simple"), "simple");
    }

    /// Per-test isolated cache to avoid racing with the shared `temp_cache()`
    /// tests — these clear_all assertions would otherwise see files written by
    /// parallel tests and vice versa.
    fn isolated_cache(tag: &str) -> (DataCache, PathBuf) {
        let path = env::temp_dir().join(format!("gw2_cache_test_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&path);
        let cache = DataCache::new(&path);
        (cache, path)
    }

    #[test]
    fn clear_all_on_missing_directory_returns_ok() {
        // If the cache dir never existed (first run, user wiped it), clearing
        // should be a no-op, not an error.
        let (cache, path) = isolated_cache("clear_missing");
        // DataCache::new called create_dir_all — remove it again to simulate missing
        let _ = std::fs::remove_dir_all(&path);
        cache.clear_all().expect("clearing a missing dir is OK");
    }

    #[test]
    fn clear_all_leaves_non_cache_files_alone() {
        let (cache, path) = isolated_cache("clear_non_cache");
        cache.save("keep_json", &"x", 1).unwrap();
        let unrelated = path.join("notes.txt");
        std::fs::write(&unrelated, b"user data").unwrap();

        cache.clear_all().expect("clear should succeed");

        assert!(unrelated.exists(), ".txt file must not be touched");
        assert!(
            !cache.exists("keep_json"),
            ".json cache file should be cleared"
        );
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn clear_all_surfaces_partial_failure_for_undeletable_entry() {
        // Seed the cache dir with a *directory* whose name ends in `.json`.
        // `remove_file` refuses to delete directories on both Windows (ACCESS_DENIED)
        // and Unix (EISDIR), which gives us a portable simulation of a per-entry
        // removal failure — exercising the `PartialClear` path.
        let (cache, path) = isolated_cache("clear_partial");
        cache.save("deletable", &"x", 1).unwrap();
        let blocker = path.join("stuck.json");
        std::fs::create_dir(&blocker).unwrap();

        let err = cache
            .clear_all()
            .expect_err("expected PartialClear for undeletable entry");

        match err {
            CacheError::PartialClear { failures } => {
                assert_eq!(failures.len(), 1, "only the directory should fail");
                assert!(
                    failures[0].contains("stuck.json"),
                    "failure list must name the offending entry: {:?}",
                    failures
                );
            }
            other => panic!("expected PartialClear, got {:?}", other),
        }

        // Best-effort contract: the other .json entry must still be cleared.
        assert!(
            !cache.exists("deletable"),
            "removable entry must still be cleared"
        );

        let _ = std::fs::remove_dir(&blocker);
        let _ = std::fs::remove_dir_all(&path);
    }
}
