use std::time::Duration;

/// When the outer retry loop replays an attempt. Both backends read this one type,
/// which is what keeps their retry semantics the same.
///
/// [`with_retries`] and [`with_retry`] are two presets of this single policy rather
/// than two loops stacked on each other, so on a `FlUrl` the later of the two calls
/// wins:
///
/// * `with_retries(n)` — transport failures only, back to back. What the crate always
///   did, kept as is for its callers.
/// * `with_retry(delay, n)` — transport failures **and** received responses with a
///   status of 500 and up, `delay` apart. What it takes to ride out a restart of the
///   service behind a reverse proxy.
///
/// Either way `amount` counts the attempts after the first one, and only idempotent
/// methods are ever replayed — the loops check the method themselves.
///
/// [`with_retries`]: crate::FlUrl::with_retries
/// [`with_retry`]: crate::FlUrl::with_retry
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RetryPolicy {
    amount: usize,
    delay: Duration,
    server_errors: bool,
}

impl RetryPolicy {
    /// The `with_retries(amount)` preset.
    pub fn transport_failures(amount: usize) -> Self {
        Self {
            amount,
            delay: Duration::ZERO,
            server_errors: false,
        }
    }

    /// The `with_retry(delay, amount)` preset.
    pub fn transport_failures_and_server_errors(delay: Duration, amount: usize) -> Self {
        Self {
            amount,
            delay,
            server_errors: true,
        }
    }

    /// How many more attempts may follow the first one.
    pub fn amount(&self) -> usize {
        self.amount
    }

    /// The pause before every attempt after the first. Zero for `with_retries`.
    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// Whether a response that did arrive is worth another attempt. Under `with_retry`
    /// a 5xx is — a reverse proxy answers 502/503 while the service behind it is down.
    /// A 4xx never is: that is the service itself answering.
    pub fn retries_status(&self, status_code: u16) -> bool {
        self.server_errors && status_code >= 500
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::RetryPolicy;

    #[test]
    fn the_default_replays_nothing() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.amount(), 0);
        assert!(!policy.retries_status(503));
    }

    #[test]
    fn with_retries_replays_no_status_and_does_not_pause() {
        let policy = RetryPolicy::transport_failures(3);

        assert_eq!(policy.amount(), 3);
        assert_eq!(policy.delay(), Duration::ZERO);
        assert!(!policy.retries_status(500));
        assert!(!policy.retries_status(503));
    }

    #[test]
    fn with_retry_replays_5xx_and_nothing_below() {
        let policy =
            RetryPolicy::transport_failures_and_server_errors(Duration::from_secs(2), 15);

        assert_eq!(policy.amount(), 15);
        assert_eq!(policy.delay(), Duration::from_secs(2));

        for status in [500, 502, 503, 504, 599] {
            assert!(policy.retries_status(status), "{} must be replayed", status);
        }
        for status in [200, 204, 301, 400, 404, 429, 499] {
            assert!(!policy.retries_status(status), "{} must be returned", status);
        }
    }
}
