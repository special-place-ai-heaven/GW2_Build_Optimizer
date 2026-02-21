//! Local JSON file cache with staleness detection.
//! Each cache file stores metadata (build number, timestamp) alongside the data.
//! Cache is invalidated when the GW2 game build number changes.

use std::io::BufWriter;
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
    pub fn save<T: Serialize>(
        &self,
        key: &str,
        data: &T,
        build: u32,
    ) -> Result<(), CacheError> {
        let entry = CacheEntry {
            build,
            fetched_at: Utc::now(),
            data,
        };
        let path = self.path_for(key);
        let tmp_path = self.base_path.join(format!("{}.tmp", key));
        let file = std::fs::File::create(&tmp_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &entry)?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// Load data from cache. Returns None if file doesn't exist.
    pub fn load<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, CacheError> {
        let path = self.path_for(key);
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(&path)?;
        let entry: CacheEntry<T> = serde_json::from_str(&json)?;
        Ok(Some(entry.data))
    }

    /// Check if cache is stale (build number doesn't match current).
    pub fn is_stale(&self, key: &str, current_build: u32) -> bool {
        let path = self.path_for(key);
        if !path.exists() {
            return true;
        }
        let Ok(json) = std::fs::read_to_string(&path) else {
            return true;
        };
        // Parse just the metadata, not the full data
        #[derive(Deserialize)]
        struct Meta {
            build: u32,
        }
        let Ok(meta) = serde_json::from_str::<Meta>(&json) else {
            return true;
        };
        meta.build != current_build
    }

    /// Get the cached build number for a key, if it exists.
    pub fn cached_build(&self, key: &str) -> Option<u32> {
        let path = self.path_for(key);
        let json = std::fs::read_to_string(&path).ok()?;
        #[derive(Deserialize)]
        struct Meta {
            build: u32,
        }
        serde_json::from_str::<Meta>(&json).ok().map(|m| m.build)
    }

    /// Delete a cache entry.
    pub fn delete(&self, key: &str) {
        let path = self.path_for(key);
        std::fs::remove_file(&path).ok();
    }

    /// Clear all cached data.
    pub fn clear_all(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.base_path) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "json") {
                    std::fs::remove_file(entry.path()).ok();
                }
            }
        }
    }

    /// Check if a cache key exists.
    pub fn exists(&self, key: &str) -> bool {
        self.path_for(key).exists()
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.base_path.join(format!("{}.json", key))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_cache() -> DataCache {
        let dir = env::temp_dir().join(format!("gw2_cache_test_{}", std::process::id()));
        DataCache::new(dir)
    }

    #[test]
    fn test_save_and_load() {
        let cache = temp_cache();
        let data = vec![1u32, 2, 3, 4, 5];
        cache.save("test_data", &data, 12345).unwrap();

        let loaded: Option<Vec<u32>> = cache.load("test_data").unwrap();
        assert_eq!(loaded, Some(vec![1, 2, 3, 4, 5]));

        cache.clear_all();
    }

    #[test]
    fn test_staleness() {
        let cache = temp_cache();
        cache.save("stale_test", &"hello", 100).unwrap();

        assert!(!cache.is_stale("stale_test", 100)); // same build
        assert!(cache.is_stale("stale_test", 101));  // different build
        assert!(cache.is_stale("nonexistent", 100));  // missing file

        cache.clear_all();
    }

    #[test]
    fn test_load_nonexistent() {
        let cache = temp_cache();
        let result: Option<Vec<u32>> = cache.load("does_not_exist").unwrap();
        assert!(result.is_none());
    }
}
