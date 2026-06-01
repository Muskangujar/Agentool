// tests/mcp_protocol.rs — Integration tests for the MCP JSON-RPC 2.0 server.
//
// Each test spins up a real McpServer on an ephemeral port and sends HTTP
// requests using reqwest. A wiremock MockServer stands in for the upstream API
// during `tools/call` tests.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agentool_core::http_client::UpstreamClient;
use agentool_core::mcp_server::McpServer;
use agentool_core::rate_limit::RateLimiter;
use agentool_core::retry::RetryPolicy;
use agentool_core::schema::{
    Auth, AuthType, HttpMethod, HttpSpec, Method, Param, ParamLocation, ParamType, Provenance,
    ProvenanceSource, ToolSchema,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── Fixture helpers ───────────────────────────────────────────────────────────

fn fixture_schema(base_url: &str) -> ToolSchema {
    ToolSchema {
        tool_id: "test-api".into(),
        version: "0.1.0".into(),
        base_url: base_url.to_string(),
        auth: Auth {
            auth_type: AuthType::None,
            location: None,
            name: None,
            scheme: None,
        },
        rate_limit: None,
        methods: vec![Method {
            name: "search".into(),
            description: "Search things.".into(),
            http: HttpSpec {
                method: HttpMethod::GET,
                path: "/search".into(),
            },
            params: vec![Param {
                name: "q".into(),
                location: ParamLocation::Query,
                param_type: ParamType::String,
                required: true,
                description: None,
                enum_values: None,
            }],
            returns: None,
        }],
        components: HashMap::new(),
        provenance: Provenance {
            source: ProvenanceSource::Openapi,
            fetched_at: "2026-06-01T00:00:00Z".into(),
            source_url: None,
        },
    }
}

/// Build a fast-failing `UpstreamClient` suitable for tests (2 s max elapsed).
fn test_upstream() -> UpstreamClient {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let rate_limiter = Arc::new(RateLimiter::new());
    let retry_policy = Arc::new(RetryPolicy::with_max_elapsed(Duration::from_secs(2)));
    UpstreamClient::with_components(client, rate_limiter, retry_policy)
}

/// Bind the McpServer on an ephemeral port, spawn it in the background, and
/// return the `http://127.0.0.1:<port>` base URL.
async fn spawn_server(schema: ToolSchema) -> String {
    let upstream = test_upstream();
    let server = McpServer::new(schema, upstream);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = server.into_service();
    tokio::spawn(async move {
        handle.serve_tokio(listener).await.unwrap();
    });
    format!("http://127.0.0.1:{port}")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_initialize() {
    let base = spawn_server(fixture_schema("http://unused")).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    assert_eq!(body["result"]["protocolVersion"], "2024-11-05");
    assert!(body.get("error").is_none(), "should not have error key");
}

#[tokio::test]
async fn test_tools_list() {
    let base = spawn_server(fixture_schema("http://unused")).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 2);

    let tools = body["result"]["tools"].as_array().expect("tools must be array");
    assert!(!tools.is_empty(), "should have at least one tool");

    // All tools should carry the fixture schema tool names.
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"search"), "should contain 'search'");

    // Every tool must have inputSchema.type == "object".
    for tool in tools {
        assert_eq!(
            tool["inputSchema"]["type"],
            "object",
            "inputSchema.type must be 'object'"
        );
    }
}

#[tokio::test]
async fn test_tools_call_proxies_to_upstream() {
    // Start a wiremock upstream that responds to GET /search?q=rust.
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "rust"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&mock_server)
        .await;

    let base = spawn_server(fixture_schema(&mock_server.uri())).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "search",
                "arguments": { "q": "rust" }
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 3);
    assert!(body.get("error").is_none(), "should not have error key");
    assert_eq!(body["result"]["isError"], false);

    let content = body["result"]["content"]
        .as_array()
        .expect("content must be array");
    assert!(!content.is_empty());
    assert_eq!(content[0]["type"], "text");
    let text = content[0]["text"].as_str().expect("text must be string");
    assert!(
        text.contains("ok"),
        "response text should contain 'ok'; got: {text}"
    );
}

#[tokio::test]
async fn test_ping() {
    let base = spawn_server(fixture_schema("http://unused")).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "ping",
            "params": {}
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 4);
    assert_eq!(body["result"], json!({}));
    assert!(body.get("error").is_none());
}

#[tokio::test]
async fn test_unknown_method_returns_error() {
    let base = spawn_server(fixture_schema("http://unused")).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "no_such_method",
            "params": {}
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 5);
    assert!(body.get("result").is_none(), "should not have result key");

    let error = &body["error"];
    assert_eq!(error["code"], -32601);
}
