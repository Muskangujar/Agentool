// py.rs — PyO3 surface (S2 contract).
//
// Exposes the agentool core stack to Python via a `_native` cdylib.
// Every public item is part of the S2 contract; do not remove or rename them.

use std::sync::{Arc, Mutex};

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::auth_inject::Credential;
use crate::http_client::{InvokeRequest, UpstreamClient};
use crate::mcp_server::McpServer;
use crate::rate_limit::default_quota;
use crate::schema::{
    Auth, AuthType, HttpMethod, HttpSpec, Method, ParamLocation, Provenance,
    ProvenanceSource, ToolSchema,
};

// ── MethodPy ──────────────────────────────────────────────────────────────────

/// Python-visible wrapper around `crate::schema::Method`.
#[pyclass]
pub struct MethodPy {
    inner: crate::schema::Method,
}

#[pymethods]
impl MethodPy {
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn description(&self) -> &str {
        &self.inner.description
    }

    #[getter]
    fn http_method(&self) -> String {
        format!("{:?}", self.inner.http.method)
    }

    #[getter]
    fn http_path(&self) -> &str {
        &self.inner.http.path
    }

    #[getter]
    fn param_names(&self) -> Vec<String> {
        self.inner.params.iter().map(|p| p.name.clone()).collect()
    }
}

// ── ToolSchemaPy ──────────────────────────────────────────────────────────────

/// Python-visible wrapper around `Arc<ToolSchema>`.
#[pyclass]
pub struct ToolSchemaPy {
    pub(crate) inner: Arc<ToolSchema>,
}

#[pymethods]
impl ToolSchemaPy {
    #[getter]
    fn tool_id(&self) -> &str {
        &self.inner.tool_id
    }

    #[getter]
    fn version(&self) -> &str {
        &self.inner.version
    }

    #[getter]
    fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    #[getter]
    fn methods(&self) -> Vec<MethodPy> {
        self.inner
            .methods
            .iter()
            .map(|m| MethodPy { inner: m.clone() })
            .collect()
    }

    /// Synchronously invoke one method against the upstream API.
    ///
    /// `params` — Python dict of argument name → value (strings / ints / bools).
    /// `auth`   — optional dict with keys "type" ("bearer"|"api_key"|"basic"),
    ///            and "token" for bearer, "header"+"key" for api_key,
    ///            "user"+"pass" for basic.
    ///
    /// Returns the upstream response body as a Python `str`.
    fn call_blocking(
        &self,
        py: Python<'_>,
        method_name: &str,
        params: &Bound<'_, pyo3::types::PyDict>,
        auth: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<PyObject> {
        // ── 1. Locate method ──────────────────────────────────────────────
        let method = self
            .inner
            .methods
            .iter()
            .find(|m| m.name == method_name)
            .ok_or_else(|| PyValueError::new_err(format!("no method '{method_name}' in schema")))?;

        // ── 2. Build URL — substitute path params, collect query/body ─────
        let mut path = method.http.path.clone();
        let mut query_pairs: Vec<(String, String)> = Vec::new();
        let mut body_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

        for param in &method.params {
            let val_obj = params.get_item(&param.name)?;
            let val_str = match val_obj {
                Some(v) => v.str()?.extract::<String>()?,
                None => continue,
            };

            match param.location {
                ParamLocation::Path => {
                    path = path.replace(&format!("{{{}}}", param.name), &val_str);
                }
                ParamLocation::Query => {
                    query_pairs.push((param.name.clone(), val_str));
                }
                ParamLocation::Body => {
                    body_map
                        .insert(param.name.clone(), serde_json::Value::String(val_str));
                }
                ParamLocation::Header => {
                    // Header params not forwarded in this layer.
                }
            }
        }

        let base = self.inner.base_url.trim_end_matches('/');
        let url_no_qs = format!("{base}{path}");

        let final_url = if query_pairs.is_empty() {
            url_no_qs
        } else {
            let qs: String = query_pairs
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&");
            format!("{url_no_qs}?{qs}")
        };

        let body_bytes: Option<bytes::Bytes> = if body_map.is_empty() {
            None
        } else {
            let b = serde_json::to_vec(&serde_json::Value::Object(body_map))
                .map_err(|e| PyRuntimeError::new_err(format!("body serialization: {e}")))?;
            Some(bytes::Bytes::from(b))
        };

        // ── 3. Convert HTTP method ────────────────────────────────────────
        let http_method = match method.http.method {
            HttpMethod::GET => reqwest::Method::GET,
            HttpMethod::POST => reqwest::Method::POST,
            HttpMethod::PUT => reqwest::Method::PUT,
            HttpMethod::DELETE => reqwest::Method::DELETE,
            HttpMethod::PATCH => reqwest::Method::PATCH,
            HttpMethod::HEAD => reqwest::Method::HEAD,
            HttpMethod::OPTIONS => reqwest::Method::OPTIONS,
        };

        // ── 4. Build Credential ───────────────────────────────────────────
        let credential: Option<Credential> = if let Some(auth_dict) = auth {
            let auth_type = auth_dict
                .get_item("type")?
                .map(|v| -> PyResult<String> { Ok(v.str()?.extract::<String>()?) })
                .transpose()?
                .unwrap_or_default();

            let cred = match auth_type.as_str() {
                "bearer" => {
                    let token = auth_dict
                        .get_item("token")?
                        .map(|v| -> PyResult<String> { Ok(v.str()?.extract::<String>()?) })
                        .transpose()?
                        .unwrap_or_default();
                    Credential::Bearer { token }
                }
                "api_key" => {
                    let header = auth_dict
                        .get_item("header")?
                        .map(|v| -> PyResult<String> { Ok(v.str()?.extract::<String>()?) })
                        .transpose()?
                        .unwrap_or_else(|| "X-API-Key".into());
                    let key = auth_dict
                        .get_item("key")?
                        .map(|v| -> PyResult<String> { Ok(v.str()?.extract::<String>()?) })
                        .transpose()?
                        .unwrap_or_default();
                    Credential::ApiKey { header, key }
                }
                "basic" => {
                    let user = auth_dict
                        .get_item("user")?
                        .map(|v| -> PyResult<String> { Ok(v.str()?.extract::<String>()?) })
                        .transpose()?
                        .unwrap_or_default();
                    let pass = auth_dict
                        .get_item("pass")?
                        .map(|v| -> PyResult<String> { Ok(v.str()?.extract::<String>()?) })
                        .transpose()?
                        .unwrap_or_default();
                    Credential::Basic { user, pass }
                }
                other => {
                    return Err(PyValueError::new_err(format!(
                        "unknown auth type '{other}'"
                    )));
                }
            };
            Some(cred)
        } else {
            None
        };

        // ── 5. Create a fresh runtime and invoke (GIL released) ───────────
        let tool_id = self.inner.tool_id.clone();
        let quota = default_quota();

        let text = py.allow_threads(move || -> Result<String, String> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("failed to build tokio runtime: {e}"))?;

            let upstream = UpstreamClient::new()
                .map_err(|e| format!("failed to build upstream client: {e}"))?;

            let invoke_req = InvokeRequest {
                tool_id,
                url: final_url,
                method: http_method,
                headers: reqwest::header::HeaderMap::new(),
                body: body_bytes,
                credential,
                quota,
            };

            let resp = rt
                .block_on(upstream.invoke(invoke_req))
                .map_err(|e| format!("invoke error: {e}"))?;

            Ok(String::from_utf8_lossy(&resp.body).into_owned())
        });

        let text = text.map_err(PyRuntimeError::new_err)?;
        Ok(PyString::new_bound(py, &text).into())
    }
}

// ── McpServerHandle ───────────────────────────────────────────────────────────

/// Handle to a background MCP server thread. Call `.stop()` to shut it down.
#[pyclass]
pub struct McpServerHandle {
    shutdown_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    port: u16,
}

#[pymethods]
impl McpServerHandle {
    /// Send the shutdown signal to the background server thread.
    fn stop(&self) -> PyResult<()> {
        let mut guard = self
            .shutdown_tx
            .lock()
            .map_err(|e| PyRuntimeError::new_err(format!("mutex poisoned: {e}")))?;
        if let Some(tx) = guard.take() {
            // Ignore send errors — server may have already stopped.
            let _ = tx.send(());
        }
        Ok(())
    }

    /// The port on which the MCP server is listening.
    fn port(&self) -> u16 {
        self.port
    }
}

// ── Pyfunctions ───────────────────────────────────────────────────────────────

/// Parse an OpenAPI 3.x spec from a URL.
///
/// Supports:
///   - `"file:///path/to/spec.json"` — reads from the filesystem.
///   - `"http(s)://"` URLs — spawns a fresh Tokio runtime and does a reqwest GET.
#[pyfunction]
pub fn parse_openapi_url(url: &str) -> PyResult<ToolSchemaPy> {
    if url.starts_with("file://") {
        let path = url.strip_prefix("file://").unwrap_or_default();
        let file = std::fs::File::open(path)
            .map_err(|e| PyRuntimeError::new_err(format!("cannot open file '{path}': {e}")))?;
        let schema = crate::openapi::parse_from_reader(file, Some(url.to_owned()))
            .map_err(|e| PyRuntimeError::new_err(format!("OpenAPI parse error: {e}")))?;
        return Ok(ToolSchemaPy {
            inner: Arc::new(schema),
        });
    }

    // HTTP(S): fetch with reqwest.
    let url_owned = url.to_owned();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| PyRuntimeError::new_err(format!("tokio runtime: {e}")))?;

    let bytes = rt
        .block_on(async move {
            reqwest::get(&url_owned)
                .await
                .map_err(|e| format!("GET {url_owned}: {e}"))?
                .bytes()
                .await
                .map_err(|e| format!("reading body: {e}"))
        })
        .map_err(PyRuntimeError::new_err)?;

    let schema = crate::openapi::parse_from_reader(bytes.as_ref(), Some(url.to_owned()))
        .map_err(|e| PyRuntimeError::new_err(format!("OpenAPI parse error: {e}")))?;

    Ok(ToolSchemaPy {
        inner: Arc::new(schema),
    })
}

/// Parse an OpenAPI 3.x spec from a JSON/YAML string.
#[pyfunction]
pub fn parse_openapi_str(spec: &str) -> PyResult<ToolSchemaPy> {
    let schema = crate::openapi::parse_from_str(spec, None)
        .map_err(|e| PyRuntimeError::new_err(format!("OpenAPI parse error: {e}")))?;
    Ok(ToolSchemaPy {
        inner: Arc::new(schema),
    })
}

/// Run the C++ HTML inference engine on `html` fetched from `url`.
///
/// Returns a `ToolSchemaPy` whose methods come from the inferred fragments.
/// The schema's `base_url` is derived by stripping the path component of `url`.
#[pyfunction]
#[pyo3(name = "infer_from_html")]
pub fn infer_from_html_py(url: &str, html: &str) -> PyResult<ToolSchemaPy> {
    // Derive base URL (scheme + host only).
    let base_url = match url::Url::parse(url) {
        Ok(parsed) => {
            let scheme = parsed.scheme();
            let host = parsed.host_str().unwrap_or("localhost");
            if let Some(port) = parsed.port() {
                format!("{scheme}://{host}:{port}")
            } else {
                format!("{scheme}://{host}")
            }
        }
        Err(_) => url.to_owned(),
    };

    let fragments = crate::infer_bridge::infer_from_html(url, html);

    let methods: Vec<Method> = fragments
        .into_iter()
        .map(|frag| {
            let http_method = match frag.method.to_uppercase().as_str() {
                "GET" => HttpMethod::GET,
                "POST" => HttpMethod::POST,
                "PUT" => HttpMethod::PUT,
                "DELETE" => HttpMethod::DELETE,
                "PATCH" => HttpMethod::PATCH,
                "HEAD" => HttpMethod::HEAD,
                "OPTIONS" => HttpMethod::OPTIONS,
                _ => HttpMethod::GET,
            };

            // Derive a snake_case name from the path.
            let name = path_to_method_name(&frag.path);

            Method {
                name,
                description: format!("{} {}", frag.method.to_uppercase(), frag.path),
                http: HttpSpec {
                    method: http_method,
                    path: frag.path,
                },
                params: vec![],
                returns: None,
            }
        })
        .collect();

    let schema = ToolSchema {
        tool_id: sanitize_id(&base_url),
        version: "0.1.0".into(),
        base_url,
        auth: Auth {
            auth_type: AuthType::None,
            location: None,
            name: None,
            scheme: None,
        },
        rate_limit: None,
        methods,
        components: std::collections::HashMap::new(),
        provenance: Provenance {
            source: ProvenanceSource::HtmlInfer,
            fetched_at: chrono::Utc::now().to_rfc3339(),
            source_url: Some(url.to_owned()),
        },
    };

    Ok(ToolSchemaPy {
        inner: Arc::new(schema),
    })
}

/// Deserialize a `ToolSchema` from a JSON string.
#[pyfunction]
pub fn schema_from_json(json: &str) -> PyResult<ToolSchemaPy> {
    let schema: ToolSchema = serde_json::from_str(json)
        .map_err(|e| PyRuntimeError::new_err(format!("JSON deserialize error: {e}")))?;
    Ok(ToolSchemaPy {
        inner: Arc::new(schema),
    })
}

/// Serialize a `ToolSchemaPy` to a JSON string.
#[pyfunction]
pub fn schema_to_json(schema: &ToolSchemaPy) -> PyResult<String> {
    serde_json::to_string(schema.inner.as_ref())
        .map_err(|e| PyRuntimeError::new_err(format!("JSON serialize error: {e}")))
}

/// Start the MCP server in a background thread.
///
/// Binds to `0.0.0.0:<port>`. If `port` is `0`, an ephemeral port is chosen.
/// Returns a `McpServerHandle` immediately (the server is live when this returns).
#[pyfunction]
pub fn start_mcp_server(schema: &ToolSchemaPy, port: u16) -> PyResult<McpServerHandle> {
    // 1. Bind synchronously so the port is live before we return.
    let std_listener = std::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .map_err(|e| PyRuntimeError::new_err(format!("bind error: {e}")))?;

    // 2. Record the actual (possibly ephemeral) port.
    let actual_port = std_listener
        .local_addr()
        .map_err(|e| PyRuntimeError::new_err(format!("local_addr: {e}")))?
        .port();

    // 3. Shutdown channel.
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    // 4. Clone the schema.
    let schema_clone: ToolSchema = (*schema.inner).clone();

    // 5. Background thread with its own Tokio runtime.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let upstream = UpstreamClient::new().expect("upstream client");
            let handle = McpServer::new(schema_clone, upstream).into_service();
            let tokio_listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            tokio::select! {
                _ = handle.serve_tokio(tokio_listener) => {},
                _ = rx => {},
            }
        });
    });

    Ok(McpServerHandle {
        shutdown_tx: Mutex::new(Some(tx)),
        port: actual_port,
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Derive a snake_case method name from an HTTP path.
/// E.g. `/api/v1/users` → `api_v1_users`
fn path_to_method_name(path: &str) -> String {
    path.trim_start_matches('/')
        .replace(['-', '/'], "_")
        .to_lowercase()
}

/// Derive a tool_id from any string (URL host, etc.).
fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_to_method_name_basic() {
        assert_eq!(path_to_method_name("/api/v1/users"), "api_v1_users");
        assert_eq!(path_to_method_name("/health"), "health");
        assert_eq!(path_to_method_name("/some-resource"), "some_resource");
    }

    #[test]
    fn schema_roundtrip_via_json() {
        let schema = ToolSchema {
            tool_id: "test".into(),
            version: "0.1.0".into(),
            base_url: "https://example.com".into(),
            auth: Auth {
                auth_type: AuthType::None,
                location: None,
                name: None,
                scheme: None,
            },
            rate_limit: None,
            methods: vec![],
            components: std::collections::HashMap::new(),
            provenance: Provenance {
                source: ProvenanceSource::Openapi,
                fetched_at: "2026-06-02T00:00:00Z".into(),
                source_url: None,
            },
        };
        let wrapper = ToolSchemaPy {
            inner: Arc::new(schema),
        };
        let json = schema_to_json(&wrapper).unwrap();
        let restored = schema_from_json(&json).unwrap();
        assert_eq!(restored.inner.tool_id, "test");
    }
}
