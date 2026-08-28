//! Cooperative cancellation for the blocking LLM transports.
//!
//! `reqwest::blocking` has no cancel handle: a worker parked in a socket read
//! stays parked until the body ends or the request timeout fires. `on_unload`
//! cannot wait out a 420-second budget, so every LLM read loop polls a
//! predicate between chunks and every retry backoff sleeps in slices.
//!
//! The predicate is a **thread-local**, not a process-global flag. One worker
//! thread owns one request; installing the token on that thread means
//! cancelling an optimize cannot abort an unrelated chat running on another
//! thread. `LlmClient` takes `&self` and is shared across threads, so it
//! cannot carry a per-request token in its signature — but the thread making
//! the call can.
//!
//! The addon installs one at the top of a worker closure:
//!
//! ```ignore
//! let token = state.cancel.clone();
//! let _cancel = llm::cancel::CancelScope::new(move || token.is_cancelled());
//! ```
//!
//! With no scope installed `is_cancelled()` is `false` and every transport
//! behaves exactly as before, so an un-migrated caller is never broken by
//! this module — it just does not get a bounded unload.

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Marker used in `LlmError::Unavailable` when a request was cancelled.
/// Same wording as `crate::scraper::CANCELLED_ERROR` so the UI can treat
/// "the user stopped it" uniformly across subsystems.
pub const CANCELLED: &str = "cancelled";

type Hook = Box<dyn Fn() -> bool>;

thread_local! {
    static CANCEL_HOOK: RefCell<Option<Hook>> = RefCell::new(None);
}

/// Installs a cancellation predicate for the current thread and restores the
/// previous one on drop, so nested scopes compose.
pub struct CancelScope {
    previous: Option<Hook>,
}

impl CancelScope {
    pub fn new(hook: impl Fn() -> bool + 'static) -> Self {
        let previous = CANCEL_HOOK.with(|slot| slot.borrow_mut().replace(Box::new(hook)));
        Self { previous }
    }
}

impl Drop for CancelScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CANCEL_HOOK.with(|slot| *slot.borrow_mut() = previous);
    }
}

/// Whether the work on this thread has been cancelled. `false` when no
/// [`CancelScope`] is installed.
pub fn is_cancelled() -> bool {
    CANCEL_HOOK.with(|slot| slot.borrow().as_ref().is_some_and(|hook| hook()))
}

/// Sleep up to `total`, waking every 100 ms to observe cancellation.
/// Returns `false` when the sleep was cut short.
///
/// A retry backoff is the other place a cancelled worker used to sit for
/// seconds after the flag flipped.
pub(crate) fn sleep_observing(total: Duration, is_cancelled: &dyn Fn() -> bool) -> bool {
    const SLICE: Duration = Duration::from_millis(100);
    let start = Instant::now();
    loop {
        if is_cancelled() {
            return false;
        }
        let elapsed = start.elapsed();
        if elapsed >= total {
            return true;
        }
        std::thread::sleep((total - elapsed).min(SLICE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_scope_means_not_cancelled() {
        assert!(!is_cancelled());
    }

    #[test]
    fn scope_is_observed_and_restored() {
        assert!(!is_cancelled());
        {
            let _scope = CancelScope::new(|| true);
            assert!(is_cancelled());
            {
                let _inner = CancelScope::new(|| false);
                assert!(!is_cancelled());
            }
            assert!(is_cancelled(), "outer scope must be restored");
        }
        assert!(!is_cancelled());
    }

    #[test]
    fn scope_does_not_leak_to_another_thread() {
        let _scope = CancelScope::new(|| true);
        assert!(is_cancelled());
        let other = std::thread::spawn(is_cancelled).join().expect("joined");
        assert!(!other, "cancellation must stay on the thread that owns it");
    }

    #[test]
    fn sleep_observing_returns_early_when_cancelled() {
        let start = Instant::now();
        let completed = sleep_observing(Duration::from_secs(30), &|| true);
        assert!(!completed);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "cancelled sleep must not run the full backoff"
        );
    }

    #[test]
    fn sleep_observing_runs_to_completion_when_live() {
        let start = Instant::now();
        assert!(sleep_observing(Duration::from_millis(250), &|| false));
        assert!(start.elapsed() >= Duration::from_millis(250));
    }
}
