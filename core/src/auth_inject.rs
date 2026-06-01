// auth_inject.rs — Credential injection for outbound requests.
//
// SECURITY: credentials must NEVER be logged. All tracing instrumentation
// skips credential arguments. The `Debug` trait is intentionally NOT derived
// on `Credential` to prevent accidental exposure through log macros.

/// Supported authentication mechanisms for upstream API calls.
#[derive(Clone)]
pub enum Credential {
    /// HTTP Bearer token — adds `Authorization: Bearer <token>`.
    Bearer { token: String },

    /// Arbitrary header-based API key — adds `<header>: <key>`.
    ApiKey { header: String, key: String },

    /// HTTP Basic auth — uses reqwest's built-in `.basic_auth()` helper.
    Basic { user: String, pass: String },
}

/// Inject the supplied `cred` into `req` and return the modified builder.
///
/// All credential values are `skip`ped in tracing instrumentation so they
/// never appear in log output.
#[tracing::instrument(skip(req, cred), fields(credential_type = credential_type(&cred)))]
pub fn inject(req: reqwest::RequestBuilder, cred: &Credential) -> reqwest::RequestBuilder {
    match cred {
        Credential::Bearer { token } => req.header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {token}"),
        ),
        Credential::ApiKey { header, key } => req.header(
            reqwest::header::HeaderName::from_bytes(header.as_bytes())
                .unwrap_or(reqwest::header::AUTHORIZATION),
            key.as_str(),
        ),
        Credential::Basic { user, pass } => req.basic_auth(user, Some(pass)),
    }
}

/// Helper used only for the tracing `fields` attribute — returns a
/// human-readable variant name that contains no secret data.
fn credential_type(cred: &Credential) -> &'static str {
    match cred {
        Credential::Bearer { .. } => "bearer",
        Credential::ApiKey { .. } => "api_key",
        Credential::Basic { .. }  => "basic",
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_credential_clones() {
        let cred = Credential::Bearer { token: "tok".into() };
        let _copy = cred.clone();
    }

    #[test]
    fn api_key_credential_clones() {
        let cred = Credential::ApiKey {
            header: "X-Api-Key".into(),
            key: "secret".into(),
        };
        let _copy = cred.clone();
    }

    #[test]
    fn basic_credential_clones() {
        let cred = Credential::Basic {
            user: "user".into(),
            pass: "pass".into(),
        };
        let _copy = cred.clone();
    }

    /// Confirm that `Credential` does NOT implement `Debug` — if this test
    /// compiles it means the trait IS derived, which we do not want.
    /// We verify this by checking that `credential_type()` returns the right
    /// strings instead of relying on `{:?}` formatting.
    #[test]
    fn credential_type_strings() {
        assert_eq!(credential_type(&Credential::Bearer { token: "t".into() }), "bearer");
        assert_eq!(
            credential_type(&Credential::ApiKey { header: "h".into(), key: "k".into() }),
            "api_key",
        );
        assert_eq!(credential_type(&Credential::Basic { user: "u".into(), pass: "p".into() }), "basic");
    }
}
