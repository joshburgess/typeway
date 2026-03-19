//! Retry policy with exponential backoff and jitter.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

use http::StatusCode;

/// Configures retry behavior for failed requests.
///
/// Supports exponential backoff with jitter to avoid thundering herd problems.
///
/// # Example
///
/// ```
/// use typeway_client::RetryPolicy;
/// use std::time::Duration;
///
/// let policy = RetryPolicy::default()
///     .max_retries(5)
///     .initial_backoff(Duration::from_millis(200));
/// ```
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries).
    pub max_retries: u32,
    /// Initial backoff duration before the first retry.
    pub initial_backoff: Duration,
    /// Maximum backoff duration (caps exponential growth).
    pub max_backoff: Duration,
    /// Multiplier applied to the backoff after each attempt.
    pub backoff_multiplier: f64,
    /// Status codes that trigger a retry.
    pub retry_on_status: Vec<StatusCode>,
    /// Whether to retry on request timeouts.
    pub retry_on_timeout: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            retry_on_status: vec![
                StatusCode::TOO_MANY_REQUESTS,
                StatusCode::BAD_GATEWAY,
                StatusCode::SERVICE_UNAVAILABLE,
                StatusCode::GATEWAY_TIMEOUT,
            ],
            retry_on_timeout: true,
        }
    }
}

impl RetryPolicy {
    /// A policy that disables all retries.
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
            backoff_multiplier: 1.0,
            retry_on_status: Vec::new(),
            retry_on_timeout: false,
        }
    }

    /// Set the maximum number of retry attempts.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    /// Set the initial backoff duration.
    pub fn initial_backoff(mut self, d: Duration) -> Self {
        self.initial_backoff = d;
        self
    }

    /// Set the maximum backoff duration.
    pub fn max_backoff(mut self, d: Duration) -> Self {
        self.max_backoff = d;
        self
    }

    /// Set the backoff multiplier.
    pub fn backoff_multiplier(mut self, f: f64) -> Self {
        self.backoff_multiplier = f;
        self
    }

    /// Set which status codes should trigger retries.
    pub fn retry_on_status(mut self, codes: Vec<StatusCode>) -> Self {
        self.retry_on_status = codes;
        self
    }

    /// Set whether timeouts should trigger retries.
    pub fn retry_on_timeout(mut self, enabled: bool) -> Self {
        self.retry_on_timeout = enabled;
        self
    }

    /// Returns `true` if the given status code should be retried.
    pub(crate) fn should_retry_status(&self, status: StatusCode) -> bool {
        self.retry_on_status.contains(&status)
    }

    /// Compute the backoff duration for the given attempt (0-indexed).
    ///
    /// Applies exponential backoff capped at `max_backoff`, plus random
    /// jitter of 0-25% to avoid thundering herd.
    pub(crate) fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let base = self.initial_backoff.as_secs_f64() * self.backoff_multiplier.powi(attempt as i32);
        let capped = base.min(self.max_backoff.as_secs_f64());

        // Add 0-25% jitter using RandomState (no external deps needed).
        let jitter_frac = random_fraction() * 0.25;
        let with_jitter = capped * (1.0 + jitter_frac);

        Duration::from_secs_f64(with_jitter.min(self.max_backoff.as_secs_f64()))
    }
}

/// Returns a pseudo-random f64 in [0.0, 1.0) using `RandomState` for entropy.
///
/// This is not cryptographically secure, but is sufficient for jitter.
fn random_fraction() -> f64 {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(0);
    let bits = hasher.finish();
    (bits >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_values() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.initial_backoff, Duration::from_millis(100));
        assert_eq!(p.max_backoff, Duration::from_secs(10));
        assert!((p.backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert!(p.retry_on_timeout);
        assert!(p.retry_on_status.contains(&StatusCode::TOO_MANY_REQUESTS));
        assert!(p.retry_on_status.contains(&StatusCode::SERVICE_UNAVAILABLE));
    }

    #[test]
    fn none_policy_disables_everything() {
        let p = RetryPolicy::none();
        assert_eq!(p.max_retries, 0);
        assert!(p.retry_on_status.is_empty());
        assert!(!p.retry_on_timeout);
    }

    #[test]
    fn backoff_grows_exponentially() {
        let p = RetryPolicy::default();
        let b0 = p.backoff_for_attempt(0);
        let b1 = p.backoff_for_attempt(1);
        let b2 = p.backoff_for_attempt(2);
        // Each should be roughly double the previous (within jitter tolerance).
        assert!(b1 > b0, "b1 ({b1:?}) should be > b0 ({b0:?})");
        assert!(b2 > b1, "b2 ({b2:?}) should be > b1 ({b1:?})");
    }

    #[test]
    fn backoff_capped_at_max() {
        let p = RetryPolicy::default().max_backoff(Duration::from_millis(500));
        let b10 = p.backoff_for_attempt(10);
        assert!(b10 <= Duration::from_millis(500));
    }

    #[test]
    fn builder_methods_chain() {
        let p = RetryPolicy::none()
            .max_retries(5)
            .initial_backoff(Duration::from_millis(50))
            .max_backoff(Duration::from_secs(5))
            .backoff_multiplier(3.0)
            .retry_on_status(vec![StatusCode::INTERNAL_SERVER_ERROR])
            .retry_on_timeout(true);

        assert_eq!(p.max_retries, 5);
        assert_eq!(p.initial_backoff, Duration::from_millis(50));
        assert_eq!(p.max_backoff, Duration::from_secs(5));
        assert!((p.backoff_multiplier - 3.0).abs() < f64::EPSILON);
        assert_eq!(p.retry_on_status, vec![StatusCode::INTERNAL_SERVER_ERROR]);
        assert!(p.retry_on_timeout);
    }
}
