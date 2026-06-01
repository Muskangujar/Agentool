// agentool_server.rs — Tokio + Hyper MCP server entry point.
//
// CLI flags:
//   --schema      <path>   Load a pre-built ToolSchema JSON file
//   --openapi-url <url>    Parse an OpenAPI spec (file:// or http://)
//   --port        <n>      Bind port (default 3000)
//   --token       <tok>    Optional Bearer token for upstream auth
//   --help                 Print usage

use std::fs::File;
use std::io::BufReader;

use anyhow::{bail, Context, Result};
use tokio::net::TcpListener;
use tracing::info;

use agentool_core::auth_inject::Credential;
use agentool_core::http_client::UpstreamClient;
use agentool_core::mcp_server::McpServer;
use agentool_core::openapi;
use agentool_core::schema::ToolSchema;

#[tokio::main]
async fn main() -> Result<()> {
    // ── Tracing ───────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // ── CLI parsing ───────────────────────────────────────────────────────────
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage(&args[0]);
        return Ok(());
    }

    let port: u16 = flag_value(&args, "--port")
        .map(|s| s.parse::<u16>().context("--port must be a u16"))
        .transpose()?
        .unwrap_or(3000);

    let token = flag_value(&args, "--token");

    // ── Schema loading ────────────────────────────────────────────────────────
    let schema: ToolSchema = if let Some(schema_path) = flag_value(&args, "--schema") {
        load_schema_from_file(&schema_path)?
    } else if let Some(openapi_url) = flag_value(&args, "--openapi-url") {
        load_schema_from_openapi_url(&openapi_url).await?
    } else {
        bail!("one of --schema or --openapi-url is required\n\nRun with --help for usage.");
    };

    // ── Build the server ──────────────────────────────────────────────────────
    let upstream = UpstreamClient::new()
        .context("failed to build upstream HTTP client")?;

    let server = if let Some(tok) = token {
        McpServer::with_credential(schema.clone(), upstream, Credential::Bearer { token: tok })
    } else {
        McpServer::new(schema.clone(), upstream)
    };

    // ── Bind and serve ────────────────────────────────────────────────────────
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    info!(
        port,
        tool_id = %schema.tool_id,
        methods = schema.methods.len(),
        "agentool-server started"
    );

    server.into_service().serve_tokio(listener).await?;

    Ok(())
}

// ── Schema loaders ────────────────────────────────────────────────────────────

fn load_schema_from_file(path: &str) -> Result<ToolSchema> {
    let f = File::open(path).with_context(|| format!("cannot open schema file: {path}"))?;
    let reader = BufReader::new(f);
    let schema: ToolSchema =
        serde_json::from_reader(reader).context("failed to deserialise ToolSchema JSON")?;
    Ok(schema)
}

async fn load_schema_from_openapi_url(url: &str) -> Result<ToolSchema> {
    if let Some(path) = url.strip_prefix("file://") {
        let f = File::open(path).with_context(|| format!("cannot open file: {path}"))?;
        let reader = BufReader::new(f);
        openapi::parse_from_reader(reader, Some(url.to_owned()))
            .context("failed to parse OpenAPI spec")
    } else {
        // HTTP fetch.
        let bytes = reqwest::get(url)
            .await
            .with_context(|| format!("HTTP GET {url} failed"))?
            .bytes()
            .await
            .context("failed to read HTTP response body")?;
        openapi::parse_from_reader(bytes.as_ref(), Some(url.to_owned()))
            .context("failed to parse OpenAPI spec from HTTP response")
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}

fn print_usage(prog: &str) {
    eprintln!(
        "Usage: {prog} [OPTIONS]\n\
         \n\
         Options:\n\
           --schema      <path>   Load a pre-built ToolSchema JSON file\n\
           --openapi-url <url>    Parse an OpenAPI spec (file:// or http://)\n\
           --port        <n>      Bind port (default 3000)\n\
           --token       <tok>    Optional Bearer token for upstream auth\n\
           --help                 Print this message\n"
    );
}
