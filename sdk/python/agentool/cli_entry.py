import argparse
import sys
import json


def main():
    parser = argparse.ArgumentParser(description="agentool — Universal API Translator CLI")
    subparsers = parser.add_subparsers(dest="command", help="Command to run")

    # ── wrap subcommand (hero command from blueprint) ────────────────────
    wrap_parser = subparsers.add_parser(
        "wrap",
        help="Wrap a URL into a ToolSchema — auto-detects OpenAPI or infers from HTML",
    )
    wrap_parser.add_argument("url", help="URL of the API or website to wrap")
    wrap_parser.add_argument(
        "-o", "--output", default=None,
        help="Output schema file path (default: <service>.schema.json)",
    )
    wrap_parser.add_argument(
        "--json", dest="print_json", action="store_true",
        help="Print schema JSON to stdout instead of writing to a file",
    )

    # ── record subcommand ────────────────────────────────────────────────
    record_parser = subparsers.add_parser(
        "record",
        help="Record and generate schema from forms of an API-free site",
    )
    record_parser.add_argument("url", help="URL of the website to crawl")
    record_parser.add_argument(
        "-o", "--output", default="tool.schema.json",
        help="Output schema file path",
    )

    # ── serve subcommand ─────────────────────────────────────────────────
    serve_parser = subparsers.add_parser(
        "serve",
        help="Serve a schema as an MCP server",
    )
    serve_parser.add_argument("schema_path", help="Path to schema.json file")
    serve_parser.add_argument(
        "-p", "--port", type=int, default=3000, help="Server port",
    )

    args = parser.parse_args()

    if args.command == "wrap":
        _cmd_wrap(args)
    elif args.command == "record":
        _cmd_record(args)
    elif args.command == "serve":
        _cmd_serve(args)
    else:
        parser.print_help()


# ── wrap ─────────────────────────────────────────────────────────────────────

def _cmd_wrap(args):
    """Wrap a URL: parse OpenAPI or infer from HTML, output ToolSchema JSON."""
    import urllib.parse

    url = args.url
    if not (url.startswith("http://") or url.startswith("https://")):
        url = "https://" + url

    try:
        from agentool._native import (
            parse_openapi_url,
            infer_from_html,
            schema_to_json,
        )
    except ImportError:
        print(
            "Error: agentool native module not found.\n"
            "Build with: maturin develop --release",
            file=sys.stderr,
        )
        sys.exit(1)

    # Try OpenAPI first, fall back to HTML inference
    native_schema = None
    source = "openapi"
    try:
        native_schema = parse_openapi_url(url)
    except Exception:
        source = "html_infer"
        try:
            import urllib.request

            req = urllib.request.Request(
                url,
                headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"},
            )
            with urllib.request.urlopen(req, timeout=15) as response:
                html_content = response.read().decode("utf-8", errors="ignore")
            native_schema = infer_from_html(url, html_content)
        except Exception as e:
            print(f"Error: Failed to wrap '{args.url}': {e}", file=sys.stderr)
            sys.exit(1)

    schema_json = schema_to_json(native_schema)

    # Pretty-print the JSON
    try:
        schema_dict = json.loads(schema_json)
        schema_pretty = json.dumps(schema_dict, indent=2)
    except (json.JSONDecodeError, TypeError):
        schema_pretty = schema_json

    # Output
    if args.print_json:
        print(schema_pretty)
        return

    # Determine output filename
    output_path = args.output
    if output_path is None:
        parsed = urllib.parse.urlparse(url)
        host = parsed.netloc or parsed.path
        parts = host.split(".")
        service = parts[-2] if len(parts) >= 2 else host
        output_path = f"{service}.schema.json"

    with open(output_path, "w", encoding="utf-8") as f:
        f.write(schema_pretty)

    method_count = len(native_schema.methods)
    print(f"✓ Wrapped {args.url}")
    print(f"  Source:  {source}")
    print(f"  Tool ID: {native_schema.tool_id}")
    print(f"  Methods: {method_count}")
    print(f"  Output:  {output_path}")


# ── record ───────────────────────────────────────────────────────────────────

def _cmd_record(args):
    """Crawl a website's HTML forms and generate a ToolSchema."""
    from agentool.record import record_and_save

    try:
        record_and_save(args.url, args.output)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


# ── serve ────────────────────────────────────────────────────────────────────

def _cmd_serve(args):
    """Serve a schema JSON file as an MCP server."""
    try:
        from agentool._native import schema_from_json, start_mcp_server
        import time

        with open(args.schema_path, "r", encoding="utf-8") as f:
            schema_json = f.read()

        native_schema = schema_from_json(schema_json)
        handle = start_mcp_server(native_schema, args.port)
        print(f"Serving MCP server on port {handle.port()}... Press Ctrl+C to stop.")
        try:
            while True:
                time.sleep(1)
        except KeyboardInterrupt:
            handle.stop()
            print("Server stopped.")
    except ImportError:
        print(
            "Error: agentool native module not found.\n"
            "Build with: maturin develop --release",
            file=sys.stderr,
        )
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
