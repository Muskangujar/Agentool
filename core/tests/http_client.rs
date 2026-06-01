// Integration tests for `UpstreamClient` using `wiremock` as the fake upstream.
//
// Tests:
//   1. `test_rate_limit_429_triggers_backoff`  — 429 twice then 200; result is Ok.
//   2. `test_circuit_breaker_opens_after_five_500s` — 500 always; circuit opens.
//   3. `test_bearer_auth_header_forwarded`     — mock verifies the header value.

use std::num::NonZeroU32;
use std::sync::Arc;

use bytes::Bytes;
use reqwest::header::HeaderMap;
use reqwest::Method;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use agentool_core::auth_inject::Credential;
use agentool_core::http_client::{ClientError, InvokeRequest, UpstreamClient};
use agentool_core::rate_limit::RateLimiter;
use agentool_core::retry::RetryPolicy;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_client() -> UpstreamClient {
    // Use a permissive rate limiter so tests are not slowed down by token-bucket waits.
    UpstreamClient::with_components(
        reqwest::Client::new(),
        Arc::new(RateLimiter::new()),
        Arc::new(RetryPolicy::new()),
    )
}

/// Build a `Quota` that effectively never limits in tests.
fn unlimited_quota() -> agentool_core::http_client::Quota {
    governor::Quota::per_second(NonZeroU32::new(10_000).unwrap())
}

// ── Test 1: 429 backoff ───────────────────────────────────────────────────────

/// The mock server returns 429 for the first two requests, then 200 with a
/// JSON body. The client must retry and ultimately succeed.
#[tokio::test]
async fn test_rate_limit_429_triggers_backoff() {
    let server = MockServer::start().await;

    // First two responses: 429 Too Many Requests
    Mock::given(method("GET"))
        .and(path("/data"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&server)
        .await;

    // Third response (and beyond): 200 OK
    Mock::given(method("GET"))
        .and(path("/data"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("all good"),
        )
        .mount(&server)
        .await;

    let client = make_client();
    let result = client
        .invoke(InvokeRequest {
            tool_id:    "test_tool".into(),
            url:        format!("{}/data", server.uri()),
            method:     Method::GET,
            headers:    HeaderMap::new(),
            body:       None,
            credential: None,
            quota:      unlimited_quota(),
        })
        .await;

    assert!(result.is_ok(), "expected Ok after backoff, got: {:?}", result);
    let resp = result.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, Bytes::from("all good"));
}

// ── Test 2: circuit breaker ───────────────────────────────────────────────────

/// The mock server always returns 500. After enough consecutive failures the
/// circuit breaker must open, causing an early-exit `CircuitOpen` error.
#[tokio::test]
async fn test_circuit_breaker_opens_after_five_500s() {
    let server = MockServer::start().await;

    // Always 500.
    Mock::given(method("GET"))
        .and(path("/fail"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    // Use a RetryPolicy with a very short max_elapsed so each failed call
    // returns quickly, allowing the circuit to trip within a reasonable time.
    use agentool_core::retry::RetryPolicy;
    use std::time::Duration;

    let policy = Arc::new(RetryPolicy::with_max_elapsed(Duration::from_millis(200)));
    let rl = Arc::new(RateLimiter::new());
    let client = UpstreamClient::with_components(
        reqwest::Client::new(),
        rl,
        policy,
    );

    let url = format!("{}/fail", server.uri());
    let mut last_error: Option<ClientError> = None;

    // The default circuit-breaker threshold is 5. We call 6 times; by the 6th
    // call (or possibly earlier, depending on timing) the circuit must be open.
    for i in 0..6u32 {
        let res = client
            .invoke(InvokeRequest {
                tool_id:    "failing_tool".into(),
                url:        url.clone(),
                method:     Method::GET,
                headers:    HeaderMap::new(),
                body:       None,
                credential: None,
                quota:      unlimited_quota(),
            })
            .await;

        match res {
            Err(e) => {
                last_error = Some(e);
                // Once the circuit opens, stop early.
                if matches!(last_error, Some(ClientError::CircuitOpen)) {
                    break;
                }
            }
            Ok(_) => panic!("expected error on call {i}"),
        }
    }

    assert!(
        matches!(last_error, Some(ClientError::CircuitOpen)),
        "expected CircuitOpen error, got: {:?}",
        last_error,
    );
}

// ── Test 3: Bearer auth header forwarded ─────────────────────────────────────

/// The mock expects `Authorization: Bearer secret`.  The test verifies that
/// the client forwards the header exactly.
#[tokio::test]
async fn test_bearer_auth_header_forwarded() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/protected"))
        .and(header("Authorization", "Bearer secret"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("authenticated"),
        )
        .mount(&server)
        .await;

    let client = make_client();
    let result = client
        .invoke(InvokeRequest {
            tool_id:    "auth_tool".into(),
            url:        format!("{}/protected", server.uri()),
            method:     Method::GET,
            headers:    HeaderMap::new(),
            body:       None,
            credential: Some(Credential::Bearer { token: "secret".into() }),
            quota:      unlimited_quota(),
        })
        .await;

    assert!(result.is_ok(), "expected Ok with valid bearer token, got: {:?}", result);
    let resp = result.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, Bytes::from("authenticated"));
}
