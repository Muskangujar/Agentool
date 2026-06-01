use std::fs;
use std::path::Path;

use agentool_core::openapi;
use agentool_core::schema::{AuthType, ParamType};

fn fixture(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap_or_else(|_| panic!("fixture not found: {name}"))
}

fn stable(input: &str) -> serde_json::Value {
    let mut schema = openapi::parse_from_str(input, None).expect("parse failed");
    // Normalize the timestamp so snapshots are deterministic.
    schema.provenance.fetched_at = "2026-01-01T00:00:00+00:00".into();
    serde_json::to_value(&schema).expect("serialize failed")
}

// ── Snapshot tests ────────────────────────────────────────────────────────────

#[test]
fn github_snapshot() {
    let json = stable(&fixture("github.openapi.json"));
    insta::assert_json_snapshot!(json);
}

#[test]
fn stripe_snapshot() {
    let json = stable(&fixture("stripe.openapi.json"));
    insta::assert_json_snapshot!(json);
}

// ── Structural / assertion tests ──────────────────────────────────────────────

#[test]
fn github_tool_id() {
    let s = openapi::parse_from_str(&fixture("github.openapi.json"), None).unwrap();
    assert_eq!(s.tool_id, "github");
}

#[test]
fn github_base_url() {
    let s = openapi::parse_from_str(&fixture("github.openapi.json"), None).unwrap();
    assert_eq!(s.base_url, "https://api.github.com");
}

#[test]
fn github_bearer_auth() {
    let s = openapi::parse_from_str(&fixture("github.openapi.json"), None).unwrap();
    assert_eq!(s.auth.auth_type, AuthType::Bearer);
    assert_eq!(s.auth.scheme.as_deref(), Some("Bearer"));
}

#[test]
fn stripe_basic_auth() {
    let s = openapi::parse_from_str(&fixture("stripe.openapi.json"), None).unwrap();
    assert_eq!(s.auth.auth_type, AuthType::Basic);
}

#[test]
fn github_method_count() {
    let s = openapi::parse_from_str(&fixture("github.openapi.json"), None).unwrap();
    // fixture has 5 operations: search_repositories, get_repository,
    // list_repository_issues, create_issue, get_authenticated_user
    assert_eq!(s.methods.len(), 5, "expected 5 methods, got: {:?}", s.methods.iter().map(|m| &m.name).collect::<Vec<_>>());
}

#[test]
fn stripe_method_count() {
    let s = openapi::parse_from_str(&fixture("stripe.openapi.json"), None).unwrap();
    // fixture has 6 operations
    assert_eq!(s.methods.len(), 6, "expected 6 methods, got: {:?}", s.methods.iter().map(|m| &m.name).collect::<Vec<_>>());
}

#[test]
fn search_repositories_params() {
    let s = openapi::parse_from_str(&fixture("github.openapi.json"), None).unwrap();
    let m = s.methods.iter().find(|m| m.name == "search_repositories").expect("method not found");

    let q = m.params.iter().find(|p| p.name == "q").expect("q param missing");
    assert!(q.required, "q should be required");
    assert_eq!(q.param_type, ParamType::String);

    let sort = m.params.iter().find(|p| p.name == "sort").expect("sort param missing");
    assert!(!sort.required, "sort should be optional");
    let enum_vals = sort.enum_values.as_ref().expect("sort should have enum values");
    assert!(enum_vals.iter().any(|v| v == "stars"));
    assert!(enum_vals.iter().any(|v| v == "forks"));
}

#[test]
fn create_issue_has_body_param() {
    let s = openapi::parse_from_str(&fixture("github.openapi.json"), None).unwrap();
    let m = s.methods.iter().find(|m| m.name == "create_issue").expect("method not found");
    let body = m.params.iter().find(|p| p.location == agentool_core::schema::ParamLocation::Body);
    assert!(body.is_some(), "create_issue should have a body param");
    assert!(body.unwrap().required, "body param should be required");
}

#[test]
fn create_customer_body_optional() {
    let s = openapi::parse_from_str(&fixture("stripe.openapi.json"), None).unwrap();
    let m = s.methods.iter().find(|m| m.name == "create_customer").expect("method not found");
    let body = m.params.iter().find(|p| p.location == agentool_core::schema::ParamLocation::Body);
    assert!(body.is_some(), "create_customer should have a body param");
    assert!(!body.unwrap().required, "body param should be optional");
}

#[test]
fn path_params_have_path_location() {
    let s = openapi::parse_from_str(&fixture("github.openapi.json"), None).unwrap();
    let m = s.methods.iter().find(|m| m.name == "get_repository").expect("method not found");
    for p in &m.params {
        assert_eq!(p.location, agentool_core::schema::ParamLocation::Path, "get_repository param {:?} should be Path", p.name);
    }
}

#[test]
fn provenance_source_is_openapi() {
    let s = openapi::parse_from_str(&fixture("github.openapi.json"), None).unwrap();
    assert_eq!(s.provenance.source, agentool_core::schema::ProvenanceSource::Openapi);
}

#[test]
fn source_url_is_recorded() {
    let s = openapi::parse_from_str(
        &fixture("github.openapi.json"),
        Some("file://core/tests/fixtures/github.openapi.json".into()),
    ).unwrap();
    assert_eq!(
        s.provenance.source_url.as_deref(),
        Some("file://core/tests/fixtures/github.openapi.json")
    );
}

#[test]
fn no_methods_from_unknown_verbs() {
    let spec = r#"{
        "openapi": "3.0.0",
        "info": { "title": "test", "version": "0.1.0" },
        "servers": [{ "url": "https://example.com" }],
        "paths": {
            "/ping": {
                "x-custom-extension": {},
                "parameters": [],
                "get": {
                    "operationId": "ping",
                    "summary": "Ping",
                    "responses": { "200": { "description": "OK" } }
                }
            }
        }
    }"#;
    let s = openapi::parse_from_str(spec, None).unwrap();
    assert_eq!(s.methods.len(), 1);
    assert_eq!(s.methods[0].name, "ping");
}

#[test]
fn missing_operation_id_derives_name() {
    let spec = r#"{
        "openapi": "3.0.0",
        "info": { "title": "test", "version": "0.1.0" },
        "servers": [{ "url": "https://example.com" }],
        "paths": {
            "/users/{id}/profile": {
                "get": {
                    "summary": "Get user profile",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": { "200": { "description": "OK" } }
                }
            }
        }
    }"#;
    let s = openapi::parse_from_str(spec, None).unwrap();
    // Path params excluded → get_users_profile
    assert_eq!(s.methods[0].name, "get_users_profile");
}
