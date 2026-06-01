// mcp_server.rs — JSON-RPC 2.0 MCP server implemented over Hyper.
//
// Handles four methods:
//   initialize   — returns protocol version and capabilities
//   tools/list   — returns all methods as MCP tool descriptors
//   tools/call   — proxies the call to the upstream API
//   ping         — returns an empty result object

use std::convert::Infallible;
use std::net::TcpListener;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder as AutoBuilder;
use hyper_util::rt::TokioIo;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, error};

use crate::auth_inject::Credential;
use crate::http_client::{InvokeRequest, UpstreamClient};
use crate::rate_limit::default_quota;
use crate::schema::{HttpMethod, ParamLocation, ParamType, ToolSchema};

// ── JSON-RPC envelope ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JrpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

// ── McpServer ─────────────────────────────────────────────────────────────────

/// An MCP-over-HTTP server that wraps a `ToolSchema` and proxies `tools/call`
/// requests to the upstream API via `UpstreamClient`.
pub struct McpServer {
    schema:     Arc<ToolSchema>,
    upstream:   Arc<UpstreamClient>,
    credential: Option<Arc<Credential>>,
}

impl McpServer {
    /// Create a new `McpServer`. Pass `credential = None` if no upstream auth
    /// is required (e.g. public APIs or tests).
    pub fn new(schema: ToolSchema, upstream: UpstreamClient) -> Self {
        Self {
            schema:     Arc::new(schema),
            upstream:   Arc::new(upstream),
            credential: None,
        }
    }

    /// Same as `new` but injects a static credential for every upstream call.
    pub fn with_credential(
        schema:     ToolSchema,
        upstream:   UpstreamClient,
        credential: Credential,
    ) -> Self {
        Self {
            schema:     Arc::new(schema),
            upstream:   Arc::new(upstream),
            credential: Some(Arc::new(credential)),
        }
    }

    // ── Dispatch ──────────────────────────────────────────────────────────────

    /// Handle one JSON-RPC request body and return a JSON-RPC response value.
    pub async fn handle(&self, body: Bytes) -> Value {
        // Parse the JSON-RPC envelope.
        let req: JrpcRequest = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                return jrpc_error(None, -32700, "Parse error", Some(e.to_string()));
            }
        };

        let id = req.id.clone();
        debug!(method = %req.method, "mcp dispatch");

        match req.method.as_str() {
            "initialize"  => self.handle_initialize(id),
            "tools/list"  => self.handle_tools_list(id),
            "tools/call"  => self.handle_tools_call(id, req.params).await,
            "ping"        => jrpc_ok(id, json!({})),
            _             => jrpc_error(id, -32601, "Method not found", None),
        }
    }

    // ── initialize ────────────────────────────────────────────────────────────

    fn handle_initialize(&self, id: Option<Value>) -> Value {
        jrpc_ok(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "serverInfo": {
                "name":    "agentool",
                "version": "0.1.0"
            }
        }))
    }

    // ── tools/list ────────────────────────────────────────────────────────────

    fn handle_tools_list(&self, id: Option<Value>) -> Value {
        let tools: Vec<Value> = self
            .schema
            .methods
            .iter()
            .map(|m| {
                let mut properties = serde_json::Map::new();
                let mut required: Vec<Value> = Vec::new();

                for p in &m.params {
                    let mut prop = serde_json::Map::new();
                    prop.insert(
                        "type".to_owned(),
                        Value::String(param_type_str(&p.param_type).to_owned()),
                    );
                    if let Some(desc) = &p.description {
                        prop.insert("description".to_owned(), Value::String(desc.clone()));
                    }
                    if let Some(enums) = &p.enum_values {
                        prop.insert(
                            "enum".to_owned(),
                            Value::Array(
                                enums.iter().map(|v| Value::String(v.clone())).collect(),
                            ),
                        );
                    }
                    properties.insert(p.name.clone(), Value::Object(prop));
                    if p.required {
                        required.push(Value::String(p.name.clone()));
                    }
                }

                json!({
                    "name":        m.name,
                    "description": m.description,
                    "inputSchema": {
                        "type":       "object",
                        "properties": properties,
                        "required":   required,
                    }
                })
            })
            .collect();

        jrpc_ok(id, json!({ "tools": tools }))
    }

    // ── tools/call ────────────────────────────────────────────────────────────

    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> Value {
        // Validate params shape.
        let params = match params {
            Some(Value::Object(m)) => m,
            _ => {
                return jrpc_error(id, -32602, "Invalid params", Some("expected object".into()));
            }
        };

        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_owned(),
            None => {
                return jrpc_error(id, -32602, "Invalid params", Some("missing 'name'".into()));
            }
        };

        let arguments = match params.get("arguments") {
            Some(Value::Object(a)) => a.clone(),
            Some(Value::Null) | None => serde_json::Map::new(),
            _ => {
                return jrpc_error(
                    id,
                    -32602,
                    "Invalid params",
                    Some("'arguments' must be an object".into()),
                );
            }
        };

        // Locate the method in the schema.
        let method = match self.schema.methods.iter().find(|m| m.name == tool_name) {
            Some(m) => m,
            None => {
                return jrpc_error(
                    id,
                    -32601,
                    "Method not found",
                    Some(format!("no tool named '{tool_name}'")),
                );
            }
        };

        // ── Build the upstream URL ─────────────────────────────────────────
        let mut path = method.http.path.clone();

        // Substitute path parameters.
        for p in method.params.iter().filter(|p| p.location == ParamLocation::Path) {
            if let Some(val) = arguments.get(&p.name).and_then(|v| v.as_str()) {
                path = path.replace(&format!("{{{}}}", p.name), val);
            }
        }

        let url = format!("{}{}", self.schema.base_url.trim_end_matches('/'), path);

        // ── Collect query and body params ──────────────────────────────────
        let mut query_pairs: Vec<(String, String)> = Vec::new();
        let mut body_map: serde_json::Map<String, Value> = serde_json::Map::new();

        for p in &method.params {
            let val = match arguments.get(&p.name) {
                Some(v) => v,
                None => continue,
            };
            match p.location {
                ParamLocation::Query => {
                    query_pairs.push((p.name.clone(), value_to_string(val)));
                }
                ParamLocation::Body => {
                    body_map.insert(p.name.clone(), val.clone());
                }
                ParamLocation::Header | ParamLocation::Path => {
                    // Headers not forwarded in A4; path already substituted above.
                }
            }
        }

        // Append query string.
        let final_url = if query_pairs.is_empty() {
            url
        } else {
            let qs: String = query_pairs
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding_simple(k), urlencoding_simple(v)))
                .collect::<Vec<_>>()
                .join("&");
            format!("{url}?{qs}")
        };

        // Body bytes (only for POST/PUT/PATCH).
        let body_bytes: Option<Bytes> = if body_map.is_empty() {
            None
        } else {
            match serde_json::to_vec(&Value::Object(body_map)) {
                Ok(b) => Some(Bytes::from(b)),
                Err(e) => {
                    return jrpc_error(
                        id,
                        -32603,
                        "Internal error",
                        Some(format!("body serialization: {e}")),
                    );
                }
            }
        };

        // Convert HttpMethod → reqwest::Method.
        let http_method = match method.http.method {
            HttpMethod::GET     => reqwest::Method::GET,
            HttpMethod::POST    => reqwest::Method::POST,
            HttpMethod::PUT     => reqwest::Method::PUT,
            HttpMethod::DELETE  => reqwest::Method::DELETE,
            HttpMethod::PATCH   => reqwest::Method::PATCH,
            HttpMethod::HEAD    => reqwest::Method::HEAD,
            HttpMethod::OPTIONS => reqwest::Method::OPTIONS,
        };

        // Build the quota from the schema rate_limit, or use the default.
        let quota = match &self.schema.rate_limit {
            Some(rl) => crate::rate_limit::quota_from_rps(rl.rps, rl.burst),
            None     => default_quota(),
        };

        let invoke_req = InvokeRequest {
            tool_id:    self.schema.tool_id.clone(),
            url:        final_url,
            method:     http_method,
            headers:    HeaderMap::new(),
            body:       body_bytes,
            credential: self.credential.as_ref().map(|c| (**c).clone()),
            quota,
        };

        // ── Call the upstream ──────────────────────────────────────────────
        match self.upstream.invoke(invoke_req).await {
            Ok(resp) => {
                let text = String::from_utf8_lossy(&resp.body).into_owned();
                jrpc_ok(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": false,
                    }),
                )
            }
            Err(e) => {
                error!(error = %e, tool = %tool_name, "upstream call failed");
                jrpc_ok(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": e.to_string() }],
                        "isError": true,
                    }),
                )
            }
        }
    }

    // ── HTTP service layer ────────────────────────────────────────────────────

    /// Handle one raw Hyper request. Only POST /mcp is accepted.
    async fn serve_request(
        self: Arc<Self>,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        // Method guard.
        if req.method() != Method::POST {
            return Ok(status_response(StatusCode::METHOD_NOT_ALLOWED));
        }
        // Path guard.
        if req.uri().path() != "/mcp" {
            return Ok(status_response(StatusCode::NOT_FOUND));
        }

        // Read body.
        let body_bytes = match req.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                error!(error = %e, "failed to read request body");
                let resp = jrpc_error(None, -32700, "Parse error", Some(e.to_string()));
                return Ok(json_response(resp));
            }
        };

        let result = self.handle(body_bytes).await;
        Ok(json_response(result))
    }

    /// Wrap `self` in an `Arc` and return a Hyper service suitable for use
    /// with `hyper_util::server::conn::auto::Builder`.
    ///
    /// # Usage
    ///
    /// ```ignore
    /// let service = McpServer::new(schema, upstream).into_service();
    /// // bind TcpListener, then:
    /// builder.serve_connection(io, service).await?;
    /// ```
    pub fn into_service(self) -> McpServiceHandle {
        McpServiceHandle {
            inner: Arc::new(self),
        }
    }
}

// ── McpServiceHandle ──────────────────────────────────────────────────────────

/// A clonable handle to a shared `McpServer` that can be handed to the Hyper
/// accept loop.
#[derive(Clone)]
pub struct McpServiceHandle {
    inner: Arc<McpServer>,
}

impl McpServiceHandle {
    /// Serve all incoming connections on `listener` using `hyper_util`'s
    /// `auto::Builder`. This is a convenience helper used by `agentool-server`.
    pub async fn serve(self, listener: TcpListener) -> anyhow::Result<()> {
        let builder = AutoBuilder::new(TokioExecutor::new());
        loop {
            let (stream, _addr) = tokio::net::TcpListener::from_std(listener.try_clone()?)
                .unwrap()
                .accept()
                .await?;
            let io       = TokioIo::new(stream);
            let server   = self.inner.clone();
            let builder2 = builder.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req| {
                    let s = server.clone();
                    async move { s.serve_request(req).await }
                });
                if let Err(e) = builder2.serve_connection(io, svc).await {
                    error!(error = %e, "connection error");
                }
            });
        }
    }

    /// Accept connections on a pre-created `tokio::net::TcpListener`.
    pub async fn serve_tokio(self, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
        let builder = AutoBuilder::new(TokioExecutor::new());
        loop {
            let (stream, _addr) = listener.accept().await?;
            let io       = TokioIo::new(stream);
            let server   = self.inner.clone();
            let builder2 = builder.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req| {
                    let s = server.clone();
                    async move { s.serve_request(req).await }
                });
                if let Err(e) = builder2.serve_connection(io, svc).await {
                    error!(error = %e, "connection error");
                }
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn jrpc_ok(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

fn jrpc_error(id: Option<Value>, code: i32, message: &str, data: Option<String>) -> Value {
    let mut err = json!({
        "code":    code,
        "message": message,
    });
    if let Some(d) = data {
        err["data"] = Value::String(d);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": err,
    })
}

fn json_response(body: Value) -> Response<Full<Bytes>> {
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(bytes)))
        .unwrap()
}

fn status_response(status: StatusCode) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::new()))
        .unwrap()
}

fn param_type_str(t: &ParamType) -> &'static str {
    match t {
        ParamType::String  => "string",
        ParamType::Integer => "integer",
        ParamType::Number  => "number",
        ParamType::Boolean => "boolean",
        ParamType::Array   => "array",
        ParamType::Object  => "object",
    }
}

/// Minimal percent-encoding: replace space with %20, & with %26, = with %3D.
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' '  => out.push_str("%20"),
            '&'  => out.push_str("%26"),
            '='  => out.push_str("%3D"),
            '+'  => out.push_str("%2B"),
            '#'  => out.push_str("%23"),
            _    => out.push(c),
        }
    }
    out
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b)   => b.to_string(),
        other            => other.to_string(),
    }
}
