// http_client.rs — Industrial-grade HTTP client with rate limiting and retry.
//
// Architecture
// ────────────
// `UpstreamClient` owns:
//   * a pooled `reqwest::Client` (HTTP/2, rustls, connection pooling enabled by default)
//   * a shared `RateLimiter` (per-tool token bucket)
//   * a shared `RetryPolicy` (exponential backoff + per-tool circuit breaker)
//
// On each `invoke` call the flow is:
//   1. Rate-limit: wait for a token from the per-tool bucket.
//   2. Retry loop: build request → inject auth → send → inspect status.
//      * 429 / 5xx → transient error (retry)
//      * Other 4xx / network error → permanent error (no retry)
//      * 2xx/3xx → success

use std::sync::Arc;

use bytes::Bytes;
use reqwest::{header::HeaderMap, Method, StatusCode};
use thiserror::Error;
use tracing::{debug, warn};

use crate::auth_inject::Credential;
use crate::rate_limit::RateLimiter;
use crate::retry::{RetryError, RetryPolicy};

pub use governor::Quota;

// ── Public request / response types ──────────────────────────────────────────

/// All parameters needed to invoke one upstream HTTP endpoint.
pub struct InvokeRequest {
    pub tool_id:    String,
    pub url:        String,
    pub method:     Method,
    pub headers:    HeaderMap,
    pub body:       Option<Bytes>,
    pub credential: Option<Credential>,
    pub quota:      Quota,
}

/// The normalised response from a successful upstream call.
#[derive(Debug)]
pub struct InvokeResponse {
    pub status:  u16,
    pub headers: HeaderMap,
    pub body:    Bytes,
}

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that `UpstreamClient::invoke` may return.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("circuit breaker open")]
    CircuitOpen,

    #[error("request failed after retries: {0}")]
    Exhausted(String),

    #[error("http error {status}: {body}")]
    Http { status: u16, body: String },
}

// ── UpstreamClient ────────────────────────────────────────────────────────────

/// A connection-pooled HTTP client with integrated rate limiting and retry.
#[derive(Clone)]
pub struct UpstreamClient {
    inner:        reqwest::Client,
    rate_limiter: Arc<RateLimiter>,
    retry_policy: Arc<RetryPolicy>,
}

impl UpstreamClient {
    /// Build an `UpstreamClient` with the default `reqwest` TLS configuration
    /// (rustls) and fresh rate-limiter / retry-policy registries.
    pub fn new() -> Result<Self, ClientError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| ClientError::Exhausted(format!("failed to build reqwest client: {e}")))?;

        Ok(Self {
            inner:        client,
            rate_limiter: Arc::new(RateLimiter::new()),
            retry_policy: Arc::new(RetryPolicy::new()),
        })
    }

    /// Build from externally supplied components (useful for testing).
    pub fn with_components(
        client:       reqwest::Client,
        rate_limiter: Arc<RateLimiter>,
        retry_policy: Arc<RetryPolicy>,
    ) -> Self {
        Self { inner: client, rate_limiter, retry_policy }
    }

    /// Invoke an upstream HTTP endpoint, honouring rate limits and retrying on
    /// transient failures.
    pub async fn invoke(&self, req: InvokeRequest) -> Result<InvokeResponse, ClientError> {
        // ── 1. Rate-limit ──────────────────────────────────────────────────
        self.rate_limiter.check_or_wait(&req.tool_id, req.quota).await;

        // Snapshot the fields we need inside the closure.
        let tool_id    = req.tool_id.clone();
        let url        = req.url.clone();
        let method     = req.method.clone();
        let headers    = req.headers.clone();
        let body       = req.body.clone();
        let credential = req.credential.clone();
        let inner      = self.inner.clone();

        // ── 2. Retry loop ──────────────────────────────────────────────────
        let result = self
            .retry_policy
            .execute(&tool_id, || {
                let inner      = inner.clone();
                let url        = url.clone();
                let method     = method.clone();
                let headers    = headers.clone();
                let body       = body.clone();
                let credential = credential.clone();

                async move {
                    // ── a. Build the request ───────────────────────────────
                    let mut builder = inner.request(method.clone(), &url);

                    // ── b. Inject credentials ──────────────────────────────
                    if let Some(ref cred) = credential {
                        builder = crate::auth_inject::inject(builder, cred);
                    }

                    // ── c. Add caller-supplied headers ─────────────────────
                    builder = builder.headers(headers.clone());

                    // ── d. Set body if present ─────────────────────────────
                    if let Some(ref b) = body {
                        builder = builder.body(b.clone());
                    }

                    // ── e. Send ────────────────────────────────────────────
                    let response = builder
                        .send()
                        .await
                        .map_err(|e| {
                            warn!(error = %e, "reqwest send error");
                            format!("network error: {e}")
                        })?;

                    let status = response.status();

                    // ── f. Treat 429 and 5xx as transient ─────────────────
                    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                        let body_text = response
                            .text()
                            .await
                            .unwrap_or_else(|_| String::new());
                        warn!(
                            status = status.as_u16(),
                            body = %body_text,
                            "transient upstream error — will retry",
                        );
                        return Err(format!("upstream returned {status}: {body_text}"));
                    }

                    // ── g. Build the response ──────────────────────────────
                    let status_u16  = status.as_u16();
                    let resp_headers = response.headers().clone();
                    let resp_bytes  = response
                        .bytes()
                        .await
                        .map_err(|e| format!("failed to read response body: {e}"))?;

                    debug!(status = status_u16, bytes = resp_bytes.len(), "upstream response received");

                    Ok(InvokeResponse {
                        status:  status_u16,
                        headers: resp_headers,
                        body:    resp_bytes,
                    })
                }
            })
            .await;

        // ── 3. Map RetryError → ClientError ───────────────────────────────
        result.map_err(|e| match e {
            RetryError::CircuitOpen(_) => ClientError::CircuitOpen,
            RetryError::Exhausted(msg) => ClientError::Exhausted(msg),
        })
    }
}

impl Default for UpstreamClient {
    fn default() -> Self {
        Self::new().expect("failed to construct default UpstreamClient")
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_client_constructs_without_panic() {
        let _client = UpstreamClient::new().expect("should build");
    }

    #[test]
    fn client_error_formats_correctly() {
        let e = ClientError::Http { status: 404, body: "not found".into() };
        assert_eq!(e.to_string(), "http error 404: not found");

        let e2 = ClientError::CircuitOpen;
        assert_eq!(e2.to_string(), "circuit breaker open");
    }
}
