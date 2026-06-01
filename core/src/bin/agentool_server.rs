use std::fs::File;
use std::io::BufReader;

use agentool_core::openapi;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let url = flag_value(&args, "--openapi-url").unwrap_or_else(|| {
        eprintln!("error: --openapi-url <url> is required");
        eprintln!("usage: agentool-server --openapi-url <file://path/to/spec.json>");
        std::process::exit(1);
    });

    let result = if let Some(path) = url.strip_prefix("file://") {
        let f = File::open(path).unwrap_or_else(|e| {
            eprintln!("error: cannot open {path}: {e}");
            std::process::exit(1);
        });
        openapi::parse_from_reader(BufReader::new(f), Some(url.clone()))
    } else {
        // HTTP fetch will be wired in Phase A3 once http_client.rs is complete.
        eprintln!("error: HTTP URLs are not yet supported. Use file:// for Phase A1 testing.");
        eprintln!("       example: --openapi-url file://core/tests/fixtures/github.openapi.json");
        std::process::exit(1);
    };

    match result {
        Ok(schema) => {
            let json = serde_json::to_string_pretty(&schema).unwrap_or_else(|e| {
                eprintln!("error: serialization failed: {e}");
                std::process::exit(1);
            });
            println!("{json}");
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}
