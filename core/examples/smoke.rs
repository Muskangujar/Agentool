// examples/smoke.rs — end-to-end smoke test for the agentool core stack.
// Run with: cargo run --example smoke
//
// What it does:
//   1. Builds a minimal ToolSchema in memory (no file I/O needed).
//   2. Starts an McpServer on an ephemeral port.
//   3. Sends `initialize`, `tools/list`, and `tools/call` (to a minimal stub
//      upstream HTTP server on a second ephemeral port).
//   4. Asserts the JSON-RPC envelopes are correct.
//   5. Exits 0 on success, non-zero on failure.

use std::collections::HashMap;

use agentool_core::http_client::UpstreamClient;
use agentool_core::mcp_server::McpServer;
use agentool_core::schema::{
    Auth, AuthType, HttpMethod, HttpSpec, Method, Provenance, ProvenanceSource, ToolSchema,
};

const MINIMAL_OPENAPI: &str = r#"
openapi: "3.0.3"
info:
  title: Smoke API
  version: "0.1.0"
servers:
  - url: https://example.com
paths:
  /health:
    get:
      operationId: getHealth
      summary: Health check
      responses:
        "200":
          description: OK
"#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 0. Verify the OpenAPI parser round-trips a minimal spec.
    let parsed = agentool_core::openapi::parse_from_str(MINIMAL_OPENAPI, None)?;
    assert_eq!(parsed.methods.len(), 1, "expected 1 method from spec");
    assert_eq!(parsed.methods[0].name, "getHealth");
    println!("openapi parse: OK ({} method)", parsed.methods.len());

    // 1. Spawn a minimal upstream stub that always returns {"result":"ok"}.
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let upstream_port = upstream_listener.local_addr()?.port();
    tokio::spawn(async move {
        serve_stub(upstream_listener).await;
    });

    // 2. Build a schema pointing at the stub.
    let schema = build_schema(upstream_port);

    // 3. Start the MCP server on an ephemeral port.
    let mcp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mcp_port = mcp_listener.local_addr()?.port();
    let upstream_client = UpstreamClient::new()?;
    let server = McpServer::new(schema, upstream_client).into_service();
    tokio::spawn(async move {
        let _ = server.serve_tokio(mcp_listener).await;
    });

    // Small delay to let server start accepting.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // 4. Run the three smoke requests.
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{mcp_port}/mcp");

    // ── initialize ────────────────────────────────────────────────────────────
    let resp: serde_json::Value = client
        .post(&base)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .send()
        .await?
        .json()
        .await?;

    assert_eq!(
        resp["result"]["protocolVersion"],
        "2024-11-05",
        "initialize: unexpected protocolVersion"
    );
    println!("initialize: OK");

    // ── tools/list ────────────────────────────────────────────────────────────
    let resp: serde_json::Value = client
        .post(&base)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await?
        .json()
        .await?;

    let tools = resp["result"]["tools"]
        .as_array()
        .expect("tools/list: result.tools is not an array");
    assert!(!tools.is_empty(), "tools/list returned no tools");
    println!("tools/list: OK ({} tools)", tools.len());

    // ── tools/call ────────────────────────────────────────────────────────────
    let resp: serde_json::Value = client
        .post(&base)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "ping_health",
                "arguments": {}
            }
        }))
        .send()
        .await?
        .json()
        .await?;

    assert_eq!(
        resp["result"]["isError"],
        false,
        "tools/call returned isError=true: {resp:#}"
    );
    println!("tools/call: OK");

    println!("smoke test PASSED");
    Ok(())
}

// ── Minimal upstream HTTP/1.1 stub ────────────────────────────────────────────

async fn serve_stub(listener: tokio::net::TcpListener) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    loop {
        if let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let body = br#"{"result":"ok"}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(body).await;
            });
        }
    }
}

// ── Schema builder ────────────────────────────────────────────────────────────

fn build_schema(upstream_port: u16) -> ToolSchema {
    ToolSchema {
        tool_id: "smoke-api".into(),
        version: "0.1.0".into(),
        base_url: format!("http://127.0.0.1:{upstream_port}"),
        auth: Auth {
            auth_type: AuthType::None,
            location: None,
            name: None,
            scheme: None,
        },
        rate_limit: None,
        methods: vec![Method {
            name: "ping_health".into(),
            description: "Health check endpoint.".into(),
            http: HttpSpec {
                method: HttpMethod::GET,
                path: "/health".into(),
            },
            params: vec![],
            returns: None,
        }],
        components: HashMap::new(),
        provenance: Provenance {
            source: ProvenanceSource::Openapi,
            fetched_at: "2026-06-02T00:00:00Z".into(),
            source_url: None,
        },
    }
}
