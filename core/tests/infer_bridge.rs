use agentool_core::infer_bridge::{infer_from_html, InferredFragment};

fn pubmed_html() -> &'static str {
    include_str!("fixtures/pubmed.docs.html")
}

// ── Pause Gate A2 requirement: ≥ 5 endpoints with sensible verbs ─────────────

#[test]
fn pubmed_extracts_at_least_five_endpoints() {
    let frags = infer_from_html(
        "https://www.ncbi.nlm.nih.gov/books/NBK25499/",
        pubmed_html(),
    );

    assert!(
        frags.len() >= 5,
        "expected >= 5 inferred endpoints, got {}:\n{:#?}",
        frags.len(),
        frags
    );
}

#[test]
fn pubmed_all_verbs_are_valid() {
    const VALID: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
    let frags = infer_from_html(
        "https://www.ncbi.nlm.nih.gov/books/NBK25499/",
        pubmed_html(),
    );
    for f in &frags {
        assert!(
            VALID.contains(&f.method.as_str()),
            "invalid HTTP verb {:?} in fragment {:?}",
            f.method, f
        );
    }
}

#[test]
fn pubmed_all_paths_start_with_slash() {
    let frags = infer_from_html(
        "https://www.ncbi.nlm.nih.gov/books/NBK25499/",
        pubmed_html(),
    );
    for f in &frags {
        assert!(
            f.path.starts_with('/'),
            "path {:?} does not start with '/'",
            f.path
        );
    }
}

#[test]
fn pubmed_contains_einfo_endpoint() {
    let frags = infer_from_html("https://eutils.ncbi.nlm.nih.gov", pubmed_html());
    assert!(
        frags.iter().any(|f| f.method == "GET" && f.path == "/einfo.fcgi"),
        "GET /einfo.fcgi not found; got: {:#?}",
        frags
    );
}

#[test]
fn pubmed_contains_post_endpoint() {
    let frags = infer_from_html("https://eutils.ncbi.nlm.nih.gov", pubmed_html());
    assert!(
        frags.iter().any(|f| f.method == "POST"),
        "expected at least one POST endpoint; got: {:#?}",
        frags
    );
}

#[test]
fn pubmed_no_duplicate_endpoints() {
    let frags = infer_from_html("https://eutils.ncbi.nlm.nih.gov", pubmed_html());
    let mut seen = std::collections::HashSet::new();
    for f in &frags {
        let key = format!("{} {}", f.method, f.path);
        assert!(seen.insert(key.clone()), "duplicate endpoint: {key}");
    }
}

// ── Generic extractor edge cases ─────────────────────────────────────────────

#[test]
fn empty_html_returns_nothing() {
    let frags = infer_from_html("https://example.com", "");
    assert!(frags.is_empty());
}

#[test]
fn junk_html_does_not_panic() {
    let _ = infer_from_html("https://example.com", "<not valid html >>> {{}}");
}

#[test]
fn pre_block_endpoints_extracted() {
    let html = r#"<html><body>
        <pre>
GET /v1/users
POST /v1/users
DELETE /v1/users/{id}
        </pre>
    </body></html>"#;
    let frags = infer_from_html("https://docs.example.com", html);
    assert!(frags.len() >= 3, "expected 3 from pre block, got: {:#?}", frags);
}

#[test]
fn data_method_data_path_attributes_extracted() {
    let html = r#"<html><body>
        <div data-method="GET"    data-path="/api/widgets"></div>
        <div data-method="POST"   data-path="/api/widgets"></div>
        <div data-method="DELETE" data-path="/api/widgets/{id}"></div>
    </body></html>"#;
    let frags = infer_from_html("https://docs.example.com", html);
    assert!(
        frags.len() >= 3,
        "expected 3 from data attributes, got: {:#?}",
        frags
    );
}
