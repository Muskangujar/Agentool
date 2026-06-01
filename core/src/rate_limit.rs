// rate_limit.rs — Per-tool token-bucket rate limiting backed by `governor`.
//
// Each distinct `tool_id` gets its own `DefaultDirectRateLimiter` created
// lazily on first use. The `DashMap` provides lock-free concurrent access so
// multiple in-flight requests for the same tool share a single limiter without
// a global mutex.

use std::sync::Arc;
use std::num::NonZeroU32;

use dashmap::DashMap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter as GovernorRateLimiter};

/// A collection of per-tool rate limiters.
///
/// Create one shared instance and `Arc::clone` it into every component that
/// needs to respect upstream API quotas.
pub struct RateLimiter {
    inner: DashMap<String, Arc<DefaultDirectRateLimiter>>,
}

impl RateLimiter {
    /// Construct an empty limiter registry. Limiters are created lazily when
    /// `check_or_wait` is called with a previously unseen `tool_id`.
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Block (asynchronously) until the token bucket for `tool_id` allows the
    /// request to proceed.
    ///
    /// If no limiter exists for `tool_id` yet, one is created from `quota`.
    /// If one already exists, `quota` is **ignored** — the existing limiter's
    /// configuration is reused. This is intentional: quota changes take effect
    /// only after a process restart (prevents mid-flight reconfiguration races).
    pub async fn check_or_wait(&self, tool_id: &str, quota: Quota) {
        let limiter = self
            .inner
            .entry(tool_id.to_owned())
            .or_insert_with(|| Arc::new(GovernorRateLimiter::direct(quota)))
            .clone();

        limiter.until_ready().await;
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a `Quota` from a sustained requests-per-second rate and a separate
/// burst capacity.
///
/// # Panics
///
/// Panics if `rps` or `burst` is zero.
pub fn quota_from_rps(rps: u32, burst: u32) -> Quota {
    let rps_nz = NonZeroU32::new(rps).expect("rps must be non-zero");
    let burst_nz = NonZeroU32::new(burst).expect("burst must be non-zero");
    Quota::per_second(rps_nz).allow_burst(burst_nz)
}

/// A conservative default quota used when a `ToolSchema` carries no
/// `rate_limit` configuration: 10 rps with a burst of 10.
pub fn default_quota() -> Quota {
    quota_from_rps(10, 10)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    #[tokio::test]
    async fn first_call_does_not_block() {
        // A very permissive quota — the first token should be available immediately.
        let quota = Quota::per_second(NonZeroU32::new(1000).unwrap());
        let rl = RateLimiter::new();
        // Should complete without sleeping.
        rl.check_or_wait("test_tool", quota).await;
    }

    #[tokio::test]
    async fn separate_tools_get_independent_limiters() {
        let quota = Quota::per_second(NonZeroU32::new(1000).unwrap());
        let rl = RateLimiter::new();
        rl.check_or_wait("tool_a", quota).await;
        rl.check_or_wait("tool_b", quota).await;
        assert_eq!(rl.inner.len(), 2);
    }

    #[test]
    fn quota_from_rps_builds_valid_quota() {
        let q = quota_from_rps(10, 30);
        // Verify it round-trips — just check it doesn't panic.
        let _limiter: DefaultDirectRateLimiter = GovernorRateLimiter::direct(q);
    }

    #[test]
    #[should_panic(expected = "rps must be non-zero")]
    fn quota_from_rps_panics_on_zero_rps() {
        quota_from_rps(0, 10);
    }
}
