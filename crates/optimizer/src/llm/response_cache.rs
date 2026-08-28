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

    /// Insert, first dropping expired entries and then enforcing the size cap.
    ///
    /// At the cap this evicts exactly **one** entry, the oldest. Clearing the
    /// whole map made the 65th prompt of a session throw away 64 cached
    /// answers, so every one of them had to be paid for again (Grok F12).
    pub(crate) fn insert(&self, prompt: &str, text: String) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, e| e.cached_at.elapsed().as_secs() < self.ttl_secs);
        // Replacing an existing prompt does not grow the map, so it owes no
        // eviction.
        if entries.len() >= self.cap && !entries.contains_key(prompt) {
            // ponytail: linear scan over `cap` (64) entries on insert. A
            // BTreeMap keyed by insertion order if the cap ever grows hot.
            let oldest = entries
                .iter()
                .min_by_key(|(_, e)| e.cached_at)
                .map(|(key, _)| key.clone());
            if let Some(key) = oldest {
                entries.remove(&key);
            }
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

    /// Grok F12: reaching the cap used to `clear()` the whole map, so one
    /// insert past the ceiling cost every other cached answer.
    #[test]
    fn response_cache_evicts_one() {
        let cache = ResponseCache::new(1800, 3);
        cache.insert("a", "1".into());
        // Instant has coarse granularity on some platforms; separate the
        // entries so "oldest" is unambiguous.
        std::thread::sleep(std::time::Duration::from_millis(5));
        cache.insert("b", "2".into());
        std::thread::sleep(std::time::Duration::from_millis(5));
        cache.insert("c", "3".into());

        // At the cap: exactly the oldest entry goes.
        cache.insert("d", "4".into());
        assert_eq!(cache.get("a"), None, "the oldest entry is evicted");
        assert_eq!(cache.get("b").as_deref(), Some("2"), "b must survive");
        assert_eq!(cache.get("c").as_deref(), Some("3"), "c must survive");
        assert_eq!(cache.get("d").as_deref(), Some("4"));

        // And again: one more insert costs one more entry, not the map.
        std::thread::sleep(std::time::Duration::from_millis(5));
        cache.insert("e", "5".into());
        assert_eq!(cache.get("b"), None);
        assert_eq!(cache.get("c").as_deref(), Some("3"));
        assert_eq!(cache.get("d").as_deref(), Some("4"));
        assert_eq!(cache.get("e").as_deref(), Some("5"));
    }

    #[test]
    fn overwriting_an_existing_prompt_evicts_nothing() {
        let cache = ResponseCache::new(1800, 2);
        cache.insert("a", "1".into());
        std::thread::sleep(std::time::Duration::from_millis(5));
        cache.insert("b", "2".into());
        // At the cap, but this replaces a key rather than growing the map.
        cache.insert("a", "1b".into());
        assert_eq!(cache.get("a").as_deref(), Some("1b"));
        assert_eq!(cache.get("b").as_deref(), Some("2"));
    }
}
