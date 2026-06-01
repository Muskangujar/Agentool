use std::collections::HashMap;
use std::io::Read;

use chrono::Utc;
use struson::reader::{JsonReader, JsonStreamReader, ValueType};

use crate::error::AgentoolError;
use crate::schema::{
    Auth, AuthLocation, AuthType, HttpMethod, HttpSpec, Method, Param, ParamLocation, ParamType,
    Provenance, ProvenanceSource, ToolSchema,
};

// ── Intermediate types (never leave this module) ──────────────────────────────

#[derive(Default)]
struct ParseState {
    title: Option<String>,
    version: Option<String>,
    base_url: Option<String>,
    security_schemes: HashMap<String, SchemeInfo>,
    global_security: Vec<String>,
    operations: Vec<RawOp>,
}

struct SchemeInfo {
    kind: SchemeKind,
}

enum SchemeKind {
    Bearer,
    Basic,
    ApiKey { in_: String, name: String },
}

struct RawOp {
    path: String,
    verb: String,
    name: String,
    description: String,
    params: Vec<RawParam>,
    has_body: bool,
    body_required: bool,
}

struct RawParam {
    name: String,
    in_: String,
    type_: String,
    required: bool,
    description: Option<String>,
    enum_values: Option<Vec<String>>,
}

// ── Error adapter ─────────────────────────────────────────────────────────────

#[inline]
fn ae<E: std::fmt::Display>(e: E) -> AgentoolError {
    AgentoolError::OpenApiParse(e.to_string())
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Parse an OpenAPI 3.x spec from any `Read` source using a streaming JSON
/// reader. Never allocates the entire document into a `serde_json::Value`.
pub fn parse_from_reader<R: Read>(
    source: R,
    source_url: Option<String>,
) -> Result<ToolSchema, AgentoolError> {
    let mut r = JsonStreamReader::new(source);
    let state = parse_root(&mut r)?;
    assemble(state, source_url)
}

/// Convenience wrapper for in-memory strings (used in tests and the CLI).
pub fn parse_from_str(
    input: &str,
    source_url: Option<String>,
) -> Result<ToolSchema, AgentoolError> {
    parse_from_reader(input.as_bytes(), source_url)
}

// ── Streaming parse ───────────────────────────────────────────────────────────

fn parse_root<R: Read>(r: &mut JsonStreamReader<R>) -> Result<ParseState, AgentoolError> {
    let mut s = ParseState::default();
    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let key = r.next_name().map_err(ae)?.to_owned();
        match key.as_str() {
            "info"       => parse_info(r, &mut s)?,
            "servers"    => parse_servers(r, &mut s)?,
            "security"   => parse_global_security(r, &mut s)?,
            "paths"      => parse_paths(r, &mut s)?,
            "components" => parse_components(r, &mut s)?,
            _            => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;
    Ok(s)
}

fn parse_info<R: Read>(r: &mut JsonStreamReader<R>, s: &mut ParseState) -> Result<(), AgentoolError> {
    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let key = r.next_name().map_err(ae)?.to_owned();
        match key.as_str() {
            "title"   => s.title   = Some(r.next_string().map_err(ae)?),
            "version" => s.version = Some(r.next_string().map_err(ae)?),
            _         => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;
    Ok(())
}

fn parse_servers<R: Read>(r: &mut JsonStreamReader<R>, s: &mut ParseState) -> Result<(), AgentoolError> {
    r.begin_array().map_err(ae)?;
    let mut captured = false;
    while r.has_next().map_err(ae)? {
        r.begin_object().map_err(ae)?;
        while r.has_next().map_err(ae)? {
            let key = r.next_name().map_err(ae)?.to_owned();
            match (key.as_str(), captured) {
                ("url", false) => {
                    s.base_url = Some(r.next_string().map_err(ae)?);
                    captured = true;
                }
                _ => r.skip_value().map_err(ae)?,
            }
        }
        r.end_object().map_err(ae)?;
    }
    r.end_array().map_err(ae)?;
    Ok(())
}

fn parse_global_security<R: Read>(r: &mut JsonStreamReader<R>, s: &mut ParseState) -> Result<(), AgentoolError> {
    // Each element: { "SchemeName": [] }
    r.begin_array().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        r.begin_object().map_err(ae)?;
        while r.has_next().map_err(ae)? {
            let name = r.next_name().map_err(ae)?.to_owned();
            s.global_security.push(name);
            r.skip_value().map_err(ae)?;
        }
        r.end_object().map_err(ae)?;
    }
    r.end_array().map_err(ae)?;
    Ok(())
}

fn parse_paths<R: Read>(r: &mut JsonStreamReader<R>, s: &mut ParseState) -> Result<(), AgentoolError> {
    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let path = r.next_name().map_err(ae)?.to_owned();
        parse_path_item(r, s, &path)?;
    }
    r.end_object().map_err(ae)?;
    Ok(())
}

fn parse_path_item<R: Read>(
    r: &mut JsonStreamReader<R>,
    s: &mut ParseState,
    path: &str,
) -> Result<(), AgentoolError> {
    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let verb = r.next_name().map_err(ae)?.to_owned();
        match verb.as_str() {
            "get" | "post" | "put" | "delete" | "patch" | "head" | "options" => {
                parse_operation(r, s, path, &verb)?;
            }
            _ => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;
    Ok(())
}

fn parse_operation<R: Read>(
    r: &mut JsonStreamReader<R>,
    s: &mut ParseState,
    path: &str,
    verb: &str,
) -> Result<(), AgentoolError> {
    let mut op_id: Option<String> = None;
    let mut summary: Option<String> = None;
    let mut description: Option<String> = None;
    let mut params: Vec<RawParam> = Vec::new();
    let mut has_body = false;
    let mut body_required = false;

    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let key = r.next_name().map_err(ae)?.to_owned();
        match key.as_str() {
            "operationId" => op_id       = Some(r.next_string().map_err(ae)?),
            "summary"     => summary     = Some(r.next_string().map_err(ae)?),
            "description" => description = Some(r.next_string().map_err(ae)?),
            "parameters"  => parse_params_array(r, &mut params)?,
            "requestBody" => {
                body_required = parse_request_body(r)?;
                has_body = true;
            }
            _ => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;

    if has_body {
        params.push(RawParam {
            name: "body".into(),
            in_: "body".into(),
            type_: "object".into(),
            required: body_required,
            description: Some("Request body.".into()),
            enum_values: None,
        });
    }

    let name = op_id.unwrap_or_else(|| derive_op_name(verb, path));
    let desc = description
        .or(summary)
        .unwrap_or_else(|| format!("{} {}", verb.to_uppercase(), path));

    s.operations.push(RawOp {
        path: path.to_owned(),
        verb: verb.to_uppercase(),
        name,
        description: desc,
        params,
        has_body,
        body_required,
    });
    Ok(())
}

fn parse_params_array<R: Read>(
    r: &mut JsonStreamReader<R>,
    out: &mut Vec<RawParam>,
) -> Result<(), AgentoolError> {
    r.begin_array().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        if let Some(p) = parse_single_param(r)? {
            out.push(p);
        }
    }
    r.end_array().map_err(ae)?;
    Ok(())
}

fn parse_single_param<R: Read>(
    r: &mut JsonStreamReader<R>,
) -> Result<Option<RawParam>, AgentoolError> {
    let mut name: Option<String> = None;
    let mut in_: Option<String> = None;
    let mut type_ = "string".to_owned();
    let mut required = false;
    let mut description: Option<String> = None;
    let mut enum_values: Option<Vec<String>> = None;

    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let key = r.next_name().map_err(ae)?.to_owned();
        match key.as_str() {
            "name"        => name = Some(r.next_string().map_err(ae)?),
            "in"          => in_  = Some(r.next_string().map_err(ae)?),
            "required"    => required = r.next_bool().map_err(ae)?,
            "description" => description = Some(r.next_string().map_err(ae)?),
            "schema"      => {
                let (t, ev) = parse_schema_object(r)?;
                type_ = t;
                enum_values = ev;
            }
            _ => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;

    // "cookie" and any unrecognised location are not in our ParamLocation — skip.
    let in_str = match in_.as_deref() {
        Some(l @ ("query" | "header" | "path" | "body")) => l.to_owned(),
        _ => return Ok(None),
    };

    Ok(Some(RawParam {
        name: name.unwrap_or_default(),
        in_: in_str,
        type_,
        required,
        description,
        enum_values,
    }))
}

/// Reads a JSON Schema `schema` object and returns `(type_str, enum_values)`.
/// Treats `$ref` and any non-object value as `"object"`.
fn parse_schema_object<R: Read>(
    r: &mut JsonStreamReader<R>,
) -> Result<(String, Option<Vec<String>>), AgentoolError> {
    match r.peek().map_err(ae)? {
        ValueType::Object => {}
        _ => {
            r.skip_value().map_err(ae)?;
            return Ok(("object".to_owned(), None));
        }
    }

    let mut type_ = "string".to_owned();
    let mut enum_values: Option<Vec<String>> = None;

    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let key = r.next_name().map_err(ae)?.to_owned();
        match key.as_str() {
            "type" => type_ = r.next_string().map_err(ae)?,
            "enum" => {
                let mut vals: Vec<String> = Vec::new();
                r.begin_array().map_err(ae)?;
                while r.has_next().map_err(ae)? {
                    match r.peek().map_err(ae)? {
                        ValueType::String => vals.push(r.next_string().map_err(ae)?),
                        _ => r.skip_value().map_err(ae)?,
                    }
                }
                r.end_array().map_err(ae)?;
                if !vals.is_empty() {
                    enum_values = Some(vals);
                }
            }
            _ => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;
    Ok((type_, enum_values))
}

/// Returns whether the body is marked required. Streams past the content tree.
fn parse_request_body<R: Read>(r: &mut JsonStreamReader<R>) -> Result<bool, AgentoolError> {
    let mut required = false;
    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let key = r.next_name().map_err(ae)?.to_owned();
        match key.as_str() {
            "required" => required = r.next_bool().map_err(ae)?,
            _          => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;
    Ok(required)
}

fn parse_components<R: Read>(r: &mut JsonStreamReader<R>, s: &mut ParseState) -> Result<(), AgentoolError> {
    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let key = r.next_name().map_err(ae)?.to_owned();
        match key.as_str() {
            "securitySchemes" => parse_security_schemes(r, s)?,
            _                 => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;
    Ok(())
}

fn parse_security_schemes<R: Read>(
    r: &mut JsonStreamReader<R>,
    s: &mut ParseState,
) -> Result<(), AgentoolError> {
    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let name = r.next_name().map_err(ae)?.to_owned();
        let info = parse_one_scheme(r)?;
        s.security_schemes.insert(name, info);
    }
    r.end_object().map_err(ae)?;
    Ok(())
}

fn parse_one_scheme<R: Read>(r: &mut JsonStreamReader<R>) -> Result<SchemeInfo, AgentoolError> {
    let mut oas_type: Option<String> = None;
    let mut scheme: Option<String> = None;
    let mut in_: Option<String> = None;
    let mut param_name: Option<String> = None;

    r.begin_object().map_err(ae)?;
    while r.has_next().map_err(ae)? {
        let key = r.next_name().map_err(ae)?.to_owned();
        match key.as_str() {
            "type"   => oas_type   = Some(r.next_string().map_err(ae)?),
            "scheme" => scheme     = Some(r.next_string().map_err(ae)?),
            "in"     => in_        = Some(r.next_string().map_err(ae)?),
            "name"   => param_name = Some(r.next_string().map_err(ae)?),
            _        => r.skip_value().map_err(ae)?,
        }
    }
    r.end_object().map_err(ae)?;

    let kind = match oas_type.as_deref() {
        Some("http") => match scheme.as_deref().map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("bearer") => SchemeKind::Bearer,
            _              => SchemeKind::Basic,
        },
        Some("apiKey") => SchemeKind::ApiKey {
            in_:  in_.unwrap_or_else(|| "header".into()),
            name: param_name.unwrap_or_else(|| "X-API-Key".into()),
        },
        _ => SchemeKind::Bearer,
    };

    Ok(SchemeInfo { kind })
}

// ── Assembly ──────────────────────────────────────────────────────────────────

fn assemble(s: ParseState, source_url: Option<String>) -> Result<ToolSchema, AgentoolError> {
    let title = s
        .title
        .ok_or_else(|| AgentoolError::OpenApiParse("missing info.title".into()))?;
    let base_url = s
        .base_url
        .ok_or_else(|| AgentoolError::OpenApiParse("missing servers[0].url".into()))?;

    let tool_id = sanitize_id(&title);
    let version = s.version.unwrap_or_else(|| "0.0.0".into());
    let auth    = resolve_auth(&s.global_security, &s.security_schemes);

    let methods = s
        .operations
        .into_iter()
        .map(convert_op)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ToolSchema {
        tool_id,
        version,
        base_url,
        auth,
        rate_limit: None,
        methods,
        components: HashMap::new(),
        provenance: Provenance {
            source:     ProvenanceSource::Openapi,
            fetched_at: Utc::now().to_rfc3339(),
            source_url,
        },
    })
}

fn resolve_auth(global: &[String], schemes: &HashMap<String, SchemeInfo>) -> Auth {
    let chosen = global.first().or_else(|| schemes.keys().next());
    match chosen.and_then(|n| schemes.get(n)).map(|i| &i.kind) {
        Some(SchemeKind::Bearer) => Auth {
            auth_type: AuthType::Bearer,
            location:  Some(AuthLocation::Header),
            name:      Some("Authorization".into()),
            scheme:    Some("Bearer".into()),
        },
        Some(SchemeKind::Basic) => Auth {
            auth_type: AuthType::Basic,
            location:  Some(AuthLocation::Header),
            name:      Some("Authorization".into()),
            scheme:    None,
        },
        Some(SchemeKind::ApiKey { in_, name }) => Auth {
            auth_type: AuthType::ApiKey,
            location:  Some(if in_ == "query" { AuthLocation::Query } else { AuthLocation::Header }),
            name:      Some(name.clone()),
            scheme:    None,
        },
        None => Auth {
            auth_type: AuthType::None,
            location:  None,
            name:      None,
            scheme:    None,
        },
    }
}

fn convert_op(op: RawOp) -> Result<Method, AgentoolError> {
    let http_method = match op.verb.as_str() {
        "GET"     => HttpMethod::GET,
        "POST"    => HttpMethod::POST,
        "PUT"     => HttpMethod::PUT,
        "DELETE"  => HttpMethod::DELETE,
        "PATCH"   => HttpMethod::PATCH,
        "HEAD"    => HttpMethod::HEAD,
        "OPTIONS" => HttpMethod::OPTIONS,
        v => return Err(AgentoolError::OpenApiParse(format!("unknown HTTP verb: {v}"))),
    };

    let params = op
        .params
        .into_iter()
        .map(convert_param)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Method {
        name: op.name,
        description: op.description,
        http: HttpSpec { method: http_method, path: op.path },
        params,
        returns: None,
    })
}

fn convert_param(p: RawParam) -> Result<Param, AgentoolError> {
    let location = match p.in_.as_str() {
        "query"  => ParamLocation::Query,
        "header" => ParamLocation::Header,
        "path"   => ParamLocation::Path,
        "body"   => ParamLocation::Body,
        l => return Err(AgentoolError::OpenApiParse(format!("unknown param location: {l}"))),
    };

    let param_type = match p.type_.as_str() {
        "integer" => ParamType::Integer,
        "number"  => ParamType::Number,
        "boolean" => ParamType::Boolean,
        "array"   => ParamType::Array,
        "object"  => ParamType::Object,
        _         => ParamType::String,
    };

    Ok(Param {
        name: p.name,
        location,
        param_type,
        required: p.required,
        description: p.description,
        enum_values: p.enum_values,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn sanitize_id(title: &str) -> String {
    let s: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    s.trim_matches('_').to_owned()
}

/// Fallback method name when `operationId` is absent.
/// Produces e.g. `get_search_repositories` from `GET /search/repositories`.
/// Path parameter placeholders like `{owner}` are omitted.
fn derive_op_name(verb: &str, path: &str) -> String {
    let segs: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .collect();
    if segs.is_empty() {
        verb.to_lowercase()
    } else {
        format!("{}_{}", verb.to_lowercase(), segs.join("_"))
    }
}
