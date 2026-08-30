use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAP_CAP: usize = 10_000;

struct Bucket {
    hits: Vec<Instant>,
    last_seq: u64,
}

#[derive(Default)]
struct Inner {
    map: HashMap<String, Bucket>,
    seq: u64,
}

/// In-memory sliding window. State is per process and lost on restart, which
/// is acceptable: the limits exist to stop floods, not to meter billing.
/// ponytail: single Mutex<HashMap>; swap for a sharded map if p99 ever matters.
#[derive(Default)]
pub struct RateLimiter {
    inner: Mutex<Inner>,
}

fn evict_oldest(map: &mut HashMap<String, Bucket>, cap: usize, keep: &str) {
    let overflow = map.len().saturating_sub(cap);
    if overflow == 0 {
        return;
    }
    let mut victims: Vec<(u64, String)> = map
        .iter()
        .filter(|(k, _)| k.as_str() != keep)
        .map(|(k, b)| (b.last_seq, k.clone()))
        .collect();
    victims.sort_unstable_by_key(|(seq, _)| *seq);
    for (_, k) in victims.into_iter().take(overflow) {
        map.remove(&k);
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ok(()) if under `limit` within `window`; Err(seconds until the oldest hit expires) otherwise.
    pub fn check(&self, key: &str, limit: usize, window: Duration) -> Result<(), u64> {
        self.check_capped(key, limit, window, MAP_CAP)
    }

    fn check_capped(
        &self,
        key: &str,
        limit: usize,
        window: Duration,
        cap: usize,
    ) -> Result<(), u64> {
        // A zero limit admits nothing. Return before touching the window: an empty
        // vec satisfies `len >= 0`, and indexing it would panic while holding the
        // global Mutex, poisoning it for every later request.
        if limit == 0 {
            return Err(window.as_secs().max(1));
        }
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        inner.seq = inner.seq.saturating_add(1);
        let seq = inner.seq;
        let v = inner.map.entry(key.to_string()).or_insert(Bucket {
            hits: Vec::new(),
            last_seq: 0,
        });
        v.hits.retain(|t| now.duration_since(*t) < window);
        if v.hits.len() >= limit {
            let oldest = v.hits[0];
            let retry = window
                .saturating_sub(now.duration_since(oldest))
                .as_secs()
                .max(1);
            return Err(retry);
        }
        v.hits.push(now);
        v.last_seq = seq;
        if inner.map.len() > cap {
            inner
                .map
                .retain(|_, b| b.hits.iter().any(|t| now.duration_since(*t) < window));
            evict_oldest(&mut inner.map, cap, key);
        }
        Ok(())
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().unwrap().map.len()
    }

    #[cfg(test)]
    fn contains(&self, key: &str) -> bool {
        self.inner.lock().unwrap().map.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_sliding_window() {
        let l = RateLimiter::new();
        for _ in 0..3 {
            assert!(l.check("k", 3, Duration::from_secs(60)).is_ok());
        }
        let err = l.check("k", 3, Duration::from_secs(60)).unwrap_err();
        assert!(err >= 1);
        assert!(l.check("other", 3, Duration::from_secs(60)).is_ok());
    }

    #[test]
    fn limiter_with_a_zero_limit_rejects_instead_of_panicking() {
        let l = RateLimiter::new();
        assert_eq!(l.check("k", 0, Duration::from_secs(60)), Err(60));
        assert_eq!(l.check("k", 0, Duration::from_secs(0)), Err(1));
        assert!(l.check("k", 1, Duration::from_secs(60)).is_ok());
    }

    #[test]
    fn limiter_evicts_oldest_keys_when_over_cap() {
        let l = RateLimiter::new();
        let window = Duration::from_secs(24 * 3600);
        assert!(l.check_capped("old", 50, window, 2).is_ok());
        assert!(l.check_capped("mid", 50, window, 2).is_ok());
        assert!(l.check_capped("new", 50, window, 2).is_ok());
        assert_eq!(l.len(), 2);
        assert!(!l.contains("old"));
        assert!(l.contains("mid"));
        assert!(l.contains("new"));
        assert!(l.check_capped("fresh", 50, window, 2).is_ok());
        assert_eq!(l.len(), 2);
        assert!(!l.contains("mid"));
        assert!(l.contains("new"));
        assert!(l.contains("fresh"));
    }

    #[test]
    fn limiter_caps_at_ten_thousand() {
        let l = RateLimiter::new();
        let window = Duration::from_secs(24 * 3600);
        for i in 0..=MAP_CAP {
            assert!(
                l.check(&format!("client:{i}"), 50, window).is_ok(),
                "insert {i}"
            );
        }
        assert_eq!(l.len(), MAP_CAP);
        assert!(!l.contains("client:0"));
        assert!(l.contains(&format!("client:{MAP_CAP}")));
    }
}
