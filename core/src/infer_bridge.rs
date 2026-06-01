use serde::Deserialize;

use crate::error::AgentoolError;

// ── CXX bridge ────────────────────────────────────────────────────────────────

#[cxx::bridge(namespace = "agentool")]
mod ffi {
    unsafe extern "C++" {
        include!("schema_infer.h");

        /// Returns a JSON array of extracted endpoint fragments:
        /// `[{"method":"GET","path":"/foo","auth_hint":"bearer"},...]`
        ///
        /// Never panics; returns `"[]"` on parse failure.
        fn infer_endpoints_json(url: &str, html: &str) -> String;
    }
}

// ── Public Rust API ───────────────────────────────────────────────────────────

/// A single endpoint extracted by the C++ HTML inference engine.
#[derive(Debug, Clone, Deserialize)]
pub struct InferredFragment {
    pub method:    String,
    pub path:      String,
    pub auth_hint: String,
}

/// Run HTML inference on the given `html` document (fetched from `url`) and
/// return all detected API endpoints. Returns an empty `Vec` on any error
/// rather than propagating — callers should treat an empty result as "no
/// structured docs found, fall back to browser recorder".
pub fn infer_from_html(url: &str, html: &str) -> Vec<InferredFragment> {
    let json = ffi::infer_endpoints_json(url, html);
    serde_json::from_str(&json).unwrap_or_default()
}

/// Same as `infer_from_html` but surfaces a `AgentoolError` on JSON
/// deserialisation failure (useful in contexts where you want to distinguish
/// "no endpoints found" from "something went wrong").
pub fn infer_from_html_strict(
    url: &str,
    html: &str,
) -> Result<Vec<InferredFragment>, AgentoolError> {
    let json = ffi::infer_endpoints_json(url, html);
    serde_json::from_str(&json).map_err(AgentoolError::Json)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_html_returns_empty_vec() {
        let frags = infer_from_html("https://example.com", "");
        assert!(frags.is_empty());
    }

    #[test]
    fn malformed_html_does_not_panic() {
        let frags = infer_from_html("https://x.com", "<<<not html at all>>>");
        // May return 0 or more results; must not crash.
        let _ = frags;
    }

    #[test]
    fn single_code_block_endpoint() {
        let html = r#"
            <html><body>
              <p>Use this endpoint:</p>
              <code>GET /api/v1/status</code>
            </body></html>
        "#;
        let frags = infer_from_html("https://docs.example.com", html);
        assert_eq!(frags.len(), 1);
        assert_eq!(frags[0].method, "GET");
        assert_eq!(frags[0].path,   "/api/v1/status");
    }

    #[test]
    fn deduplicates_repeated_endpoints() {
        let html = r#"
            <html><body>
              <code>GET /ping</code>
              <code>GET /ping</code>
              <code>GET /ping</code>
            </body></html>
        "#;
        let frags = infer_from_html("https://docs.example.com", html);
        assert_eq!(frags.len(), 1, "duplicate endpoints should be collapsed");
    }

    #[test]
    fn multiple_verbs_extracted() {
        let html = r#"
            <html><body>
              <code>GET /users</code>
              <code>POST /users</code>
              <code>DELETE /users/{id}</code>
            </body></html>
        "#;
        let frags = infer_from_html("https://docs.example.com", html);
        assert_eq!(frags.len(), 3);
        assert!(frags.iter().any(|f| f.method == "GET"    && f.path == "/users"));
        assert!(frags.iter().any(|f| f.method == "POST"   && f.path == "/users"));
        assert!(frags.iter().any(|f| f.method == "DELETE" && f.path == "/users/{id}"));
    }
}
