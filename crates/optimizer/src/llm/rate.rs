//! Shared request-rate tracker for the `LlmError`-based providers
//! (OpenAI, Anthropic, OpenRouter). Tracks requests-per-minute against a
//! per-provider RPM limit plus a rolling daily counter persisted to disk.
//!
//! Gemini has its own tracker in `crate::gemini` because it enforces a hard
//! daily cap and reports failures as `GeminiError`, not `LlmError`.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::LlmError;

/// Daily counter used only for display; these providers have no hard daily cap.
const DISPLAY_DAILY_BUDGET: u32 = 10000;

#[derive(Serialize, Deserialize)]
pub(crate) struct PersistedUsage {
    pub day: u64,
    pub requests_today: u32,
}

pub(crate) fn current_epoch_day() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86400
}

pub(crate) struct RateTracker {
    requests_this_minute: u32,
    minute_start: Instant,
    requests_today: u32,
    current_day: u64,
    rpm_limit: u32,
}

impl RateTracker {
    pub fn new(rpm_limit: u32) -> Self {
        Self {
            requests_this_minute: 0,
            minute_start: Instant::now(),
            requests_today: 0,
            current_day: current_epoch_day(),
            rpm_limit,
        }
    }

    pub fn from_persisted(persisted: PersistedUsage, rpm_limit: u32) -> Self {
        let today = current_epoch_day();
        let requests_today = if persisted.day == today {
            persisted.requests_today
        } else {
            0
        };
        Self {
            requests_this_minute: 0,
            minute_start: Instant::now(),
            requests_today,
            current_day: today,
            rpm_limit,
        }
    }

    pub fn check_and_reserve(&mut self) -> Result<(), LlmError> {
        let today = current_epoch_day();
        if today != self.current_day {
            self.requests_today = 0;
            self.current_day = today;
        }

        let now = Instant::now();
        if now.duration_since(self.minute_start).as_secs() >= 60 {
            self.requests_this_minute = 0;
            self.minute_start = now;
        }

        if self.requests_this_minute >= self.rpm_limit {
            return Err(LlmError::RateLimited);
        }

        self.requests_this_minute += 1;
        self.requests_today += 1;
        Ok(())
    }

    pub fn undo_reserve(&mut self) {
        self.requests_this_minute = self.requests_this_minute.saturating_sub(1);
        self.requests_today = self.requests_today.saturating_sub(1);
    }

    pub fn remaining_today(&self) -> u32 {
        DISPLAY_DAILY_BUDGET.saturating_sub(self.requests_today)
    }

    pub fn to_persisted(&self) -> PersistedUsage {
        PersistedUsage {
            day: self.current_day,
            requests_today: self.requests_today,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpm_limit_blocks_after_quota() {
        let mut tracker = RateTracker::new(60);
        for _ in 0..60 {
            assert!(tracker.check_and_reserve().is_ok());
        }
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn rpm_limit_is_per_provider() {
        let mut tracker = RateTracker::new(50);
        for _ in 0..50 {
            assert!(tracker.check_and_reserve().is_ok());
        }
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn undo_reserve_frees_a_slot() {
        let mut tracker = RateTracker::new(60);
        tracker.check_and_reserve().unwrap();
        assert_eq!(tracker.requests_this_minute, 1);
        tracker.undo_reserve();
        assert_eq!(tracker.requests_this_minute, 0);
    }

    #[test]
    fn persistence_roundtrip_same_day() {
        let mut tracker = RateTracker::new(60);
        for _ in 0..5 {
            tracker.check_and_reserve().unwrap();
        }
        let persisted = tracker.to_persisted();
        let reloaded = RateTracker::from_persisted(persisted, 60);
        assert_eq!(reloaded.requests_today, 5);
    }

    #[test]
    fn persistence_day_rollover_resets_daily() {
        let yesterday = current_epoch_day().saturating_sub(1);
        let persisted = PersistedUsage {
            day: yesterday,
            requests_today: 9999,
        };
        let reloaded = RateTracker::from_persisted(persisted, 60);
        assert_eq!(reloaded.requests_today, 0);
        assert_eq!(reloaded.current_day, current_epoch_day());
    }

    #[test]
    fn minute_rollover_preserves_daily() {
        let mut tracker = RateTracker::new(60);
        for _ in 0..5 {
            tracker.check_and_reserve().unwrap();
        }
        // Force the per-minute window to look stale.
        tracker.minute_start = Instant::now() - std::time::Duration::from_secs(61);
        tracker.check_and_reserve().unwrap();
        assert_eq!(tracker.requests_this_minute, 1);
        assert_eq!(tracker.requests_today, 6);
    }
}
