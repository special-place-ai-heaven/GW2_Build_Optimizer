use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// In-memory sliding window. State is per process and lost on restart, which
/// is acceptable: the limits exist to stop floods, not to meter billing.
/// ponytail: single Mutex<HashMap>; swap for a sharded map if p99 ever matters.
#[derive(Default)]
pub struct RateLimiter {
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self { Self::default() }

    /// Ok(()) if under `limit` within `window`; Err(seconds until the oldest hit expires) otherwise.
    pub fn check(&self, key: &str, limit: usize, window: Duration) -> Result<(), u64> {
        let now = Instant::now();
        let mut map = self.hits.lock().unwrap();
        let v = map.entry(key.to_string()).or_default();
        v.retain(|t| now.duration_since(*t) < window);
        if v.len() >= limit {
            let oldest = v[0];
            let retry = window.saturating_sub(now.duration_since(oldest)).as_secs().max(1);
            return Err(retry);
        }
        v.push(now);
        // Opportunistic cleanup so the map does not grow without bound.
        if map.len() > 10_000 {
            map.retain(|_, hits| hits.iter().any(|t| now.duration_since(*t) < window));
        }
        Ok(())
    }
}
