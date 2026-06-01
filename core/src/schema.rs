use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level data model for a wrapped API. Every parser path (OpenAPI 3.x,
/// HTML inference, browser recording) produces exactly one `ToolSchema`. The
/// MCP server and the PyO3 surface both consume this type — it is the S1
/// contract shared between Track A and Track B.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub tool_id: String,
    pub version: String,
    pub base_url: String,
    pub auth: Auth,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimit>,
    pub methods: Vec<Method>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub components: HashMap<String, serde_json::Value>,
    pub provenance: Provenance,
}

// ── Authentication ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Auth {
    #[serde(rename = "type")]
    pub auth_type: AuthType,
    /// Where the credential is sent. Required for all types except `None`.
    #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
    pub location: Option<AuthLocation>,
    /// Header or query-parameter name (e.g. `"Authorization"`, `"X-API-Key"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Value prefix used only for `Bearer` (e.g. `"Bearer"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    Bearer,
    ApiKey,
    Basic,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthLocation {
    Header,
    Query,
}

// ── Rate limit ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    /// Sustained requests per second.
    pub rps: u32,
    /// Token-bucket burst capacity.
    pub burst: u32,
}

// ── Method ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Method {
    /// Snake-case identifier exposed to the agent (e.g. `"search_repositories"`).
    pub name: String,
    /// Human-readable docstring shown to the LLM.
    pub description: String,
    pub http: HttpSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<ReturnType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpSpec {
    pub method: HttpMethod,
    /// Path relative to `base_url`. Path parameters use `{param_name}` syntax.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

// ── Parameter ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(rename = "in")]
    pub location: ParamLocation,
    #[serde(rename = "type")]
    pub param_type: ParamType,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When present, restricts the value to one of these literal strings.
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamLocation {
    Query,
    Header,
    Path,
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
}

// ── Return types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnType {
    pub kind: ReturnKind,
    /// Named fields present when `kind` is `Object`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<ReturnField>>,
    /// Element type present when `kind` is `Array`. Either an inline
    /// `ReturnType` or a `$ref` pointer into `ToolSchema.components`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<ItemsRef>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnKind {
    Object,
    Array,
    String,
    Integer,
    Number,
    Boolean,
    Null,
}

/// Either an inline `ReturnType` or a `$ref` into `ToolSchema.components`.
/// Uses `#[serde(untagged)]` so the JSON representation has no wrapper key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ItemsRef {
    /// `{ "$ref": "#/components/Repository" }`
    Ref {
        #[serde(rename = "$ref")]
        ref_path: String,
    },
    /// Inline nested `ReturnType`.
    Inline(ReturnType),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnField {
    pub name: String,
    /// JSON-compatible type string (`"string"`, `"integer"`, `"array"`, …).
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Element type present when `field_type` is `"array"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<ItemsRef>>,
}

// ── Provenance ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub source: ProvenanceSource,
    /// ISO 8601 UTC timestamp of when the source was fetched or the recording
    /// completed. Stored as a string so the struct has no chrono dependency —
    /// callers use `chrono::Utc::now().to_rfc3339()` before constructing this.
    pub fetched_at: String,
    /// URL of the OpenAPI spec or docs page. Omitted for `browser_record`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSource {
    Openapi,
    HtmlInfer,
    BrowserRecord,
}

// ── Round-trip tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn github_fixture() -> ToolSchema {
        ToolSchema {
            tool_id: "github".into(),
            version: "1.0.0".into(),
            base_url: "https://api.github.com".into(),
            auth: Auth {
                auth_type: AuthType::Bearer,
                location: Some(AuthLocation::Header),
                name: Some("Authorization".into()),
                scheme: Some("Bearer".into()),
            },
            rate_limit: Some(RateLimit { rps: 10, burst: 30 }),
            methods: vec![Method {
                name: "search_repositories".into(),
                description: "Search repos by query.".into(),
                http: HttpSpec {
                    method: HttpMethod::GET,
                    path: "/search/repositories".into(),
                },
                params: vec![
                    Param {
                        name: "q".into(),
                        location: ParamLocation::Query,
                        param_type: ParamType::String,
                        required: true,
                        description: None,
                        enum_values: None,
                    },
                    Param {
                        name: "sort".into(),
                        location: ParamLocation::Query,
                        param_type: ParamType::String,
                        required: false,
                        description: None,
                        enum_values: Some(vec![
                            "stars".into(),
                            "forks".into(),
                            "updated".into(),
                        ]),
                    },
                ],
                returns: Some(ReturnType {
                    kind: ReturnKind::Object,
                    fields: Some(vec![
                        ReturnField {
                            name: "total_count".into(),
                            field_type: "integer".into(),
                            description: None,
                            items: None,
                        },
                        ReturnField {
                            name: "items".into(),
                            field_type: "array".into(),
                            description: None,
                            items: Some(Box::new(ItemsRef::Ref {
                                ref_path: "#/components/Repository".into(),
                            })),
                        },
                    ]),
                    items: None,
                }),
            }],
            components: HashMap::new(),
            provenance: Provenance {
                source: ProvenanceSource::Openapi,
                fetched_at: "2026-05-31T00:00:00Z".into(),
                source_url: Some("https://api.github.com/openapi.json".into()),
            },
        }
    }

    #[test]
    fn round_trips_to_json_and_back() {
        let original = github_fixture();
        let json = serde_json::to_string_pretty(&original).expect("serialize");
        let restored: ToolSchema = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(original.tool_id, restored.tool_id);
        assert_eq!(original.version, restored.version);
        assert_eq!(original.base_url, restored.base_url);
        assert_eq!(original.auth.auth_type, restored.auth.auth_type);
        assert_eq!(original.methods.len(), restored.methods.len());
        assert_eq!(original.methods[0].name, restored.methods[0].name);
        assert_eq!(original.methods[0].params.len(), restored.methods[0].params.len());
        assert_eq!(original.provenance.source, restored.provenance.source);
    }

    #[test]
    fn auth_none_omits_optional_fields() {
        let schema = ToolSchema {
            tool_id: "public-api".into(),
            version: "0.1.0".into(),
            base_url: "https://api.example.com".into(),
            auth: Auth {
                auth_type: AuthType::None,
                location: None,
                name: None,
                scheme: None,
            },
            rate_limit: None,
            methods: vec![],
            components: HashMap::new(),
            provenance: Provenance {
                source: ProvenanceSource::HtmlInfer,
                fetched_at: "2026-05-31T00:00:00Z".into(),
                source_url: None,
            },
        };

        let json = serde_json::to_value(&schema).expect("serialize");
        let auth_obj = json.get("auth").unwrap();

        assert!(auth_obj.get("in").is_none(), "\"in\" should be absent for auth_type=none");
        assert!(auth_obj.get("name").is_none());
        assert!(auth_obj.get("scheme").is_none());
        assert!(json.get("rate_limit").is_none());
    }

    #[test]
    fn method_without_params_omits_params_key() {
        let schema = ToolSchema {
            tool_id: "t".into(),
            version: "0.1.0".into(),
            base_url: "https://x.com".into(),
            auth: Auth {
                auth_type: AuthType::None,
                location: None,
                name: None,
                scheme: None,
            },
            rate_limit: None,
            methods: vec![Method {
                name: "ping".into(),
                description: "Health check.".into(),
                http: HttpSpec {
                    method: HttpMethod::GET,
                    path: "/ping".into(),
                },
                params: vec![],
                returns: None,
            }],
            components: HashMap::new(),
            provenance: Provenance {
                source: ProvenanceSource::BrowserRecord,
                fetched_at: "2026-05-31T00:00:00Z".into(),
                source_url: None,
            },
        };

        let json = serde_json::to_value(&schema).expect("serialize");
        let method = &json["methods"][0];

        assert!(method.get("params").is_none(), "empty params should be omitted");
        assert!(method.get("returns").is_none());
    }
}
