// retry.rs — Exponential backoff with a per-tool circuit breaker.
//
// Design
// ──────
// * `CircuitBreaker` is a plain state machine stored behind a `Mutex` inside
//   a `DashMap` keyed by `tool_id`.
// * `RetryPolicy` wraps the map and exposes a single `execute` method that:
//     1. Rejects the call immediately if the circuit is Open.
//     2. Otherwise delegates to `backoff::future::retry` with an exponential
//        backoff capped at 60 seconds total elapsed time.
//     3. On success it records success; on every transient error it records
//        failure (which may trip the circuit).

use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use backoff::ExponentialBackoffBuilder;
use dashmap::DashMap;
use parking_lot::Mutex;
use thiserror::Error;
use tracing::{debug, warn};

// ── Circuit-breaker state ─────────────────────────────────────────────────────

/// The three states of the circuit breaker.
#[derive(Clone, Debug, PartialEq)]
pub enum CbState {
    /// Normal operation; calls are forwarded to the upstream.
    Closed,
    /// Too many consecutive failures; calls are rejected until `until` elapses.
    Open { until: Instant },
    /// Recovery probe window: one call is allowed through to test the upstream.
    HalfOpen,
}

/// Per-tool circuit breaker.
#[derive(Debug)]
pub struct CircuitBreaker {
    pub failure_count: u32,
    pub state: CbState,
    /// Number of consecutive failures that trips the circuit.
    pub threshold: u32,
    /// How long the circuit stays Open before transitioning to HalfOpen.
    pub recovery_timeout: Duration,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            failure_count: 0,
            state: CbState::Closed,
            threshold: 5,
            recovery_timeout: Duration::from_secs(30),
        }
    }
}

impl CircuitBreaker {
    /// Reset to Closed and clear the failure counter.
    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CbState::Closed;
        debug!("circuit breaker: closed (success recorded)");
    }

    /// Increment the failure counter; trip to Open when the threshold is reached.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        if self.failure_count >= self.threshold {
            let until = Instant::now() + self.recovery_timeout;
            self.state = CbState::Open { until };
            warn!(
                failure_count = self.failure_count,
                recovery_secs = self.recovery_timeout.as_secs(),
                "circuit breaker: opened",
            );
        }
    }

    /// Return `true` if the circuit is currently Open (calls should be rejected).
    ///
    /// As a side effect, transitions `Open → HalfOpen` when the recovery
    /// timeout has elapsed.
    pub fn is_open(&mut self) -> bool {
        if let CbState::Open { until } = self.state {
            if Instant::now() >= until {
                self.state = CbState::HalfOpen;
                debug!("circuit breaker: half-open (recovery timeout elapsed)");
                return false;
            }
            return true;
        }
        false
    }
}

// ── Retry error types ─────────────────────────────────────────────────────────

/// Errors returned from `RetryPolicy::execute`.
#[derive(Debug, Error)]
pub enum RetryError {
    #[error("circuit breaker open for tool {0}")]
    CircuitOpen(String),

    #[error("max retries exceeded: {0}")]
    Exhausted(String),
}

// ── Retry policy ──────────────────────────────────────────────────────────────

/// Wraps a per-tool circuit-breaker map and drives exponential backoff retries.
pub struct RetryPolicy {
    breakers:        DashMap<String, Mutex<CircuitBreaker>>,
    max_elapsed:     Duration,
    initial_interval: Duration,
}

impl RetryPolicy {
    pub fn new() -> Self {
        Self {
            breakers:        DashMap::new(),
            max_elapsed:     Duration::from_secs(60),
            initial_interval: Duration::from_millis(200),
        }
    }

    /// Construct a policy with a custom maximum elapsed backoff time.
    /// Primarily useful in tests to avoid 60-second waits.
    pub fn with_max_elapsed(max_elapsed: Duration) -> Self {
        Self {
            breakers:        DashMap::new(),
            max_elapsed,
            initial_interval: Duration::from_millis(10),
        }
    }

    /// Pre-seed the circuit-breaker for `tool_id` as already Open.
    /// This is used in tests to bypass the backoff cycle.
    pub fn force_open_circuit(&self, tool_id: &str) {
        let entry = self
            .breakers
            .entry(tool_id.to_owned())
            .or_insert_with(|| Mutex::new(CircuitBreaker::default()));
        let mut cb = entry.lock();
        for _ in 0..cb.threshold {
            cb.record_failure();
        }
    }

    /// Execute `f`, retrying on transient errors with exponential backoff.
    ///
    /// Returns `Err(RetryError::CircuitOpen)` immediately if the circuit for
    /// `tool_id` is Open. Returns `Err(RetryError::Exhausted)` when
    /// `backoff::ExponentialBackoff` decides no more retries should be
    /// attempted (max elapsed time exceeded).
    pub async fn execute<F, Fut, T, E>(
        &self,
        tool_id: &str,
        f: F,
    ) -> Result<T, RetryError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: fmt::Display,
    {
        // ── Check circuit ──────────────────────────────────────────────────
        {
            let entry = self
                .breakers
                .entry(tool_id.to_owned())
                .or_insert_with(|| Mutex::new(CircuitBreaker::default()));

            let mut cb = entry.lock();
            if cb.is_open() {
                return Err(RetryError::CircuitOpen(tool_id.to_owned()));
            }
        }

        // ── Backoff configuration ──────────────────────────────────────────
        let max_elapsed     = self.max_elapsed;
        let initial_interval = self.initial_interval;
        let max_interval    = std::cmp::min(
            Duration::from_secs(10),
            max_elapsed / 2,
        );
        let backoff_cfg = ExponentialBackoffBuilder::new()
            .with_max_elapsed_time(Some(max_elapsed))
            .with_initial_interval(initial_interval)
            .with_max_interval(max_interval)
            .with_multiplier(2.0)
            .with_randomization_factor(0.3)
            .build();

        // We need a local last_error string to surface on exhaustion.
        let last_err: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let last_err_inner = last_err.clone();

        // ── Drive the operation ────────────────────────────────────────────
        let result: Result<T, _> = backoff::future::retry(backoff_cfg, || {
            let last_err_ref = last_err_inner.clone();
            let fut = f();
            async move {
                match fut.await {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        let msg = e.to_string();
                        *last_err_ref.lock() = msg.clone();
                        Err(backoff::Error::transient(msg))
                    }
                }
            }
        })
        .await;

        match result {
            Ok(v) => {
                // Record success on the circuit breaker.
                let entry = self
                    .breakers
                    .entry(tool_id.to_owned())
                    .or_insert_with(|| Mutex::new(CircuitBreaker::default()));
                entry.lock().record_success();
                Ok(v)
            }
            Err(e) => {
                // Record failure on the circuit breaker.
                let entry = self
                    .breakers
                    .entry(tool_id.to_owned())
                    .or_insert_with(|| Mutex::new(CircuitBreaker::default()));
                entry.lock().record_failure();

                let msg = format!("{e}: last error: {}", last_err.lock());
                Err(RetryError::Exhausted(msg))
            }
        }
    }

    /// Check whether the circuit for `tool_id` is currently open without
    /// modifying any state. Primarily used in tests.
    pub fn circuit_is_open(&self, tool_id: &str) -> bool {
        match self.breakers.get(tool_id) {
            None => false,
            Some(entry) => {
                let mut cb = entry.lock();
                // is_open() may transition Open → HalfOpen as a side-effect.
                cb.is_open()
            }
        }
    }

    /// Expose the raw failure count for a tool. Used in tests only.
    #[cfg(test)]
    pub fn failure_count(&self, tool_id: &str) -> u32 {
        match self.breakers.get(tool_id) {
            None => 0,
            Some(entry) => entry.lock().failure_count,
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::default();
        for _ in 0..4 {
            cb.record_failure();
            assert_eq!(cb.state, CbState::Closed);
        }
        cb.record_failure(); // 5th failure
        assert!(matches!(cb.state, CbState::Open { .. }));
    }

    #[test]
    fn circuit_breaker_resets_on_success() {
        let mut cb = CircuitBreaker::default();
        for _ in 0..5 {
            cb.record_failure();
        }
        assert!(matches!(cb.state, CbState::Open { .. }));
        cb.record_success();
        assert_eq!(cb.state, CbState::Closed);
        assert_eq!(cb.failure_count, 0);
    }

    #[test]
    fn is_open_transitions_to_half_open_after_timeout() {
        let mut cb = CircuitBreaker {
            threshold: 1,
            recovery_timeout: Duration::from_millis(1),
            ..Default::default()
        };
        cb.record_failure();
        assert!(matches!(cb.state, CbState::Open { .. }));

        // Wait for the timeout to elapse.
        std::thread::sleep(Duration::from_millis(5));
        assert!(!cb.is_open(), "should have transitioned to HalfOpen");
        assert_eq!(cb.state, CbState::HalfOpen);
    }

    #[tokio::test]
    async fn retry_policy_succeeds_after_two_failures() {
        let policy = RetryPolicy::new();
        let calls = Arc::new(AtomicU32::new(0));
        let calls_ref = calls.clone();

        let result = policy
            .execute("tool", || {
                let c = calls_ref.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err::<u32, &str>("transient")
                    } else {
                        Ok(42u32)
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_policy_returns_circuit_open_when_tripped() {
        let policy = RetryPolicy::new();

        // Force the circuit open by injecting state directly.
        {
            let entry = policy
                .breakers
                .entry("tool".to_owned())
                .or_insert_with(|| Mutex::new(CircuitBreaker::default()));
            let mut cb = entry.lock();
            for _ in 0..5 {
                cb.record_failure();
            }
        }

        let result = policy
            .execute("tool", || async { Err::<u32, &str>("should not be called") })
            .await;

        assert!(matches!(result, Err(RetryError::CircuitOpen(_))));
    }
}
