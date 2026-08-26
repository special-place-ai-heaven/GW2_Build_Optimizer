//! Shared LLM response cache with TTL and size cap.
//!
//! Prompts embed the full game context, so an insert-only map grows for the
//! life of the process — every provider previously inlined this eviction
//! logic. One struct, one policy: entries expire after `ttl_secs`, and the
//! map is cleared when it reaches `cap` (responses are always re-fetchable).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

struct Entry {
    text: String,
    cached_at: Instant,
}

pub(crate) struct ResponseCache {
    ttl_secs: u64,
    cap: usize,
    entries: Mutex<HashMap<String, Entry>>,
}

impl ResponseCache {
    pub(crate) fn new(ttl_secs: u64, cap: usize) -> Self {
        Self {
            ttl_secs,
            cap,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Cached response for `prompt`, if present and unexpired.
    pub(crate) fn get(&self, prompt: &str) -> Option<String> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries
            .get(prompt)
            .filter(|e| e.cached_at.elapsed().as_secs() < self.ttl_secs)
            .map(|e| e.text.clone())
    }

    /// Insert, first evicting expired entries and enforcing the size cap.
    pub(crate) fn insert(&self, prompt: &str, text: String) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, e| e.cached_at.elapsed().as_secs() < self.ttl_secs);
        if entries.len() >= self.cap {
            entries.clear();
        }
        entries.insert(
            prompt.to_string(),
            Entry {
                text,
                cached_at: Instant::now(),
            },
        );
    }

    /// Drop every cached response.
    pub(crate) fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_returns_inserted_text() {
        let cache = ResponseCache::new(1800, 8);
        cache.insert("prompt", "answer".into());
        assert_eq!(cache.get("prompt").as_deref(), Some("answer"));
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn zero_ttl_entries_expire_immediately() {
        let cache = ResponseCache::new(0, 8);
        cache.insert("prompt", "answer".into());
        assert_eq!(cache.get("prompt"), None);
    }

    #[test]
    fn insert_at_cap_clears_before_insert() {
        let cache = ResponseCache::new(1800, 2);
        cache.insert("a", "1".into());
        cache.insert("b", "2".into());
        // Reaching the cap clears the map before the new insert lands.
        cache.insert("c", "3".into());
        assert_eq!(cache.get("c").as_deref(), Some("3"));
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), None);
    }
}
