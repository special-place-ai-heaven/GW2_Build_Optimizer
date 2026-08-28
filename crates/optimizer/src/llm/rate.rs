//! Shared request-rate tracker for the `LlmError`-based providers
//! (OpenAI, Anthropic, OpenRouter). Tracks requests-per-minute against a
//! per-provider RPM limit plus a rolling daily counter persisted to disk.
//!
//! Gemini has its own tracker in `crate::gemini` because it enforces a hard
//! daily cap and reports failures as `GeminiError`, not `LlmError`.
//!
//! **Both windows are persisted.** `llm::create_client` builds a fresh client
//! for every user action, so an in-memory-only minute window always started
//! empty and the RPM limit was never actually enforced (Claude F37). The
//! minute window rides to disk as a wall-clock anchor because `Instant` is
//! not serializable and does not survive a process restart; the anchor is
//! converted straight back to an `Instant` on load, so `check_and_reserve`
//! keeps using one monotonic clock and cannot be pushed around by an NTP
//! step.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::LlmError;

/// Daily counter used only for display; these providers have no hard daily
/// cap, so this is the addon's own budget and not a provider quota. The
/// number `remaining_today` reports is "requests left in that budget", which
/// is why it must not be read as a vendor allowance (GLM F22 — the UI label
/// lives in the addon crate and still says "quota").
const DISPLAY_DAILY_BUDGET: u32 = 10000;

/// Length of the per-minute window.
const MINUTE: Duration = Duration::from_secs(60);

#[derive(Serialize, Deserialize)]
pub(crate) struct PersistedUsage {
    pub day: u64,
    pub requests_today: u32,
    /// Wall-clock epoch second the current minute window opened.
    ///
    /// `#[serde(default)]` on both minute fields: a usage file written by an
    /// older build has neither, and losing the daily counter to a strict
    /// parse would be a worse bug than starting the minute window fresh.
    #[serde(default)]
    pub minute_start_epoch: u64,
    #[serde(default)]
    pub requests_this_minute: u32,
}

pub(crate) fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn current_epoch_day() -> u64 {
    current_epoch_secs() / 86400
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

        let now = Instant::now();
        // Age of the persisted minute window. `checked_sub` is the clock-went-
        // backwards guard: a saturating subtraction would read as "zero
        // seconds old" and pin a user at the limit until wall clock caught up.
        let age = current_epoch_secs().checked_sub(persisted.minute_start_epoch);
        let (requests_this_minute, minute_start) = match age {
            Some(age) if age < MINUTE.as_secs() && persisted.minute_start_epoch > 0 => (
                persisted.requests_this_minute,
                // Re-anchor the monotonic clock so the window expires when it
                // would have, not 60 s from now.
                now.checked_sub(Duration::from_secs(age)).unwrap_or(now),
            ),
            _ => (0, now),
        };

        Self {
            requests_this_minute,
            minute_start,
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
        if now.duration_since(self.minute_start) >= MINUTE {
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

    /// Requests charged to the current minute window. Test-only: the
    /// transport tests use it to prove the reserve/undo handshake balances
    /// and that the window survives a reload.
    #[cfg(test)]
    pub fn requests_this_minute(&self) -> u32 {
        self.requests_this_minute
    }

    pub fn remaining_today(&self) -> u32 {
        DISPLAY_DAILY_BUDGET.saturating_sub(self.requests_today)
    }

    pub fn to_persisted(&self) -> PersistedUsage {
        // Convert the monotonic anchor back to wall clock only here, at the
        // serialization boundary.
        let age = self.minute_start.elapsed().as_secs();
        PersistedUsage {
            day: self.current_day,
            requests_today: self.requests_today,
            minute_start_epoch: current_epoch_secs().saturating_sub(age),
            requests_this_minute: self.requests_this_minute,
        }
    }
}

/// Write the tracker's counters beside the addon config, atomically.
///
/// Was copied verbatim into all four providers (GLM F20). Same `.tmp` +
/// rename shape as `gw2_core::config`: a torn write must not leave a usage
/// file that fails to parse and silently resets the user's counters.
pub(crate) fn persist_usage(path: Option<&std::path::Path>, rate: &RateTracker) {
    let Some(path) = path else { return };
    let Ok(json) = serde_json::to_string(&rate.to_persisted()) else {
        return;
    };
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, &json).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    } else {
        let _ = std::fs::remove_file(&tmp);
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
            minute_start_epoch: current_epoch_secs(),
            requests_this_minute: 3,
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
        tracker.minute_start = Instant::now() - Duration::from_secs(61);
        tracker.check_and_reserve().unwrap();
        assert_eq!(tracker.requests_this_minute, 1);
        assert_eq!(tracker.requests_today, 6);
    }

    /// Claude F37: the minute window has to cross a reload, because
    /// `create_client` reloads on every user action.
    #[test]
    fn persisted_minute_window_survives_a_reload() {
        let mut tracker = RateTracker::new(4);
        for _ in 0..4 {
            tracker.check_and_reserve().unwrap();
        }
        let mut reloaded = RateTracker::from_persisted(tracker.to_persisted(), 4);
        assert_eq!(reloaded.requests_this_minute, 4);
        assert!(
            matches!(reloaded.check_and_reserve(), Err(LlmError::RateLimited)),
            "a reload must not hand back a fresh minute"
        );
    }

    #[test]
    fn persisted_minute_window_expires_with_the_wall_clock() {
        let persisted = PersistedUsage {
            day: current_epoch_day(),
            requests_today: 4,
            minute_start_epoch: current_epoch_secs().saturating_sub(61),
            requests_this_minute: 4,
        };
        let mut reloaded = RateTracker::from_persisted(persisted, 4);
        assert_eq!(reloaded.requests_this_minute, 0, "stale window must reset");
        assert!(reloaded.check_and_reserve().is_ok());
    }

    #[test]
    fn a_backwards_clock_does_not_pin_the_user_at_the_limit() {
        // Anchor one hour in the future: an NTP step or a manual clock edit.
        let persisted = PersistedUsage {
            day: current_epoch_day(),
            requests_today: 4,
            minute_start_epoch: current_epoch_secs() + 3600,
            requests_this_minute: 4,
        };
        let mut reloaded = RateTracker::from_persisted(persisted, 4);
        assert_eq!(reloaded.requests_this_minute, 0);
        assert!(reloaded.check_and_reserve().is_ok());
    }

    /// A usage file written before the minute window was persisted must still
    /// load, and must keep the daily counter it does carry.
    #[test]
    fn legacy_usage_file_without_minute_fields_still_parses() {
        let legacy = format!("{{\"day\":{},\"requests_today\":7}}", current_epoch_day());
        let persisted: PersistedUsage = serde_json::from_str(&legacy).expect("legacy parses");
        let reloaded = RateTracker::from_persisted(persisted, 60);
        assert_eq!(reloaded.requests_today, 7);
        assert_eq!(reloaded.requests_this_minute, 0);
    }
}
