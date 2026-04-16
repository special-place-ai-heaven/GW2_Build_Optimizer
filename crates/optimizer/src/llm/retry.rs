pub(crate) const MAX_RETRIES: u32 = 3;

pub(crate) fn backoff_delay(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_secs(5 * (1u64 << attempt.saturating_sub(1)))
}

pub(crate) enum RetryOutcome<T, E> {
    Done(T),
    Retry(E),
    Terminal(E),
}

pub(crate) fn with_retries<F, T, E>(mut f: F) -> Result<T, E>
where
    F: FnMut(u32) -> RetryOutcome<T, E>,
{
    let mut last_error: Option<E> = None;
    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            std::thread::sleep(backoff_delay(attempt));
        }
        match f(attempt) {
            RetryOutcome::Done(v) => return Ok(v),
            RetryOutcome::Terminal(e) => return Err(e),
            RetryOutcome::Retry(e) => last_error = Some(e),
        }
    }
    // MAX_RETRIES >= 1 and the only path out of the loop without an early
    // return is a `Retry` branch, which populates `last_error`.
    Err(last_error.expect("with_retries: no attempts recorded"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug, PartialEq)]
    enum TestError {
        Transient(u32),
        Fatal,
    }

    #[test]
    fn done_on_first_attempt_returns_ok() {
        let out: Result<u32, TestError> = with_retries(|_| RetryOutcome::Done(42));
        assert_eq!(out.unwrap(), 42);
    }

    #[test]
    fn terminal_returns_immediately() {
        let attempts = Cell::new(0u32);
        let out: Result<(), TestError> = with_retries(|_| {
            attempts.set(attempts.get() + 1);
            RetryOutcome::Terminal(TestError::Fatal)
        });
        assert_eq!(out, Err(TestError::Fatal));
        assert_eq!(attempts.get(), 1);
    }

    #[test]
    fn retry_exhausts_and_returns_last_error() {
        let attempts = Cell::new(0u32);
        let out: Result<(), TestError> = with_retries(|attempt| {
            attempts.set(attempts.get() + 1);
            RetryOutcome::Retry(TestError::Transient(attempt))
        });
        assert_eq!(out, Err(TestError::Transient(MAX_RETRIES - 1)));
        assert_eq!(attempts.get(), MAX_RETRIES);
    }

    #[test]
    fn retry_then_done_returns_ok() {
        let attempts = Cell::new(0u32);
        let out: Result<u32, TestError> = with_retries(|_| {
            let n = attempts.get();
            attempts.set(n + 1);
            if n == 0 {
                RetryOutcome::Retry(TestError::Transient(0))
            } else {
                RetryOutcome::Done(7)
            }
        });
        assert_eq!(out.unwrap(), 7);
        assert_eq!(attempts.get(), 2);
    }

    #[test]
    fn backoff_delay_grows_exponentially() {
        assert_eq!(backoff_delay(1), std::time::Duration::from_secs(5));
        assert_eq!(backoff_delay(2), std::time::Duration::from_secs(10));
        assert_eq!(backoff_delay(3), std::time::Duration::from_secs(20));
    }
}
