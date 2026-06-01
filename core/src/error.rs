use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentoolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("OpenAPI parse error: {0}")]
    OpenApiParse(String),

    #[error("HTTP error: {0}")]
    Http(String),
}
