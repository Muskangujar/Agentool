from __future__ import annotations
import json
import urllib.request
import urllib.parse
import re
from html.parser import HTMLParser
from typing import Any, Dict, List

class FormParser(HTMLParser):
    """HTML Parser that extracts form actions, methods, and input elements."""
    def __init__(self):
        super().__init__()
        self.forms = []
        self.current_form = None

    def handle_starttag(self, tag, attrs):
        attrs_dict = dict(attrs)
        if tag == "form":
            self.current_form = {
                "action": attrs_dict.get("action", ""),
                "method": attrs_dict.get("method", "GET").upper(),
                "id": attrs_dict.get("id", ""),
                "name": attrs_dict.get("name", ""),
                "inputs": []
            }
        elif self.current_form is not None:
            if tag in ("input", "textarea", "select"):
                name = attrs_dict.get("name")
                if name:
                    self.current_form["inputs"].append({
                        "name": name,
                        "type": attrs_dict.get("type", "text") if tag == "input" else tag,
                        "required": "required" in attrs_dict
                    })

    def handle_endtag(self, tag):
        if tag == "form" and self.current_form is not None:
            self.forms.append(self.current_form)
            self.current_form = None

def record_site_forms(url: str) -> dict:
    """Crawl a website and extract its HTML forms to build a ToolSchema JSON dict."""
    if not (url.startswith("http://") or url.startswith("https://")):
        url = "https://" + url

    try:
        req = urllib.request.Request(
            url, 
            headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'}
        )
        with urllib.request.urlopen(req, timeout=10) as response:
            html = response.read().decode('utf-8', errors='ignore')
    except Exception as e:
        raise ValueError(f"Failed to crawl '{url}': {e}")

    parser = FormParser()
    parser.feed(html)

    methods = []
    for i, form in enumerate(parser.forms):
        # Derive method name
        name = form["id"] or form["name"]
        if not name:
            action_path = urllib.parse.urlparse(form["action"]).path
            action_clean = action_path.strip("/").replace("/", "_").replace("-", "_")
            method_prefix = form["method"].lower()
            name = f"{method_prefix}_{action_clean}" if action_clean else f"{method_prefix}_form_{i+1}"
        
        name = re.sub(r'[^a-zA-Z0-9_]', '', name).strip("_")

        # Extract parameters
        params = []
        for inp in form["inputs"]:
            # Default all param types to string for HTML form submittals
            params.append({
                "name": inp["name"],
                "type": "string",
                "required": inp["required"],
                "description": f"Field '{inp['name']}' ({inp['type']})"
            })

        action_url = form["action"]
        if not (action_url.startswith("http://") or action_url.startswith("https://")):
            action_url = urllib.parse.urljoin(url, action_url)
        
        parsed_action = urllib.parse.urlparse(action_url)
        path = parsed_action.path or "/"
        
        methods.append({
            "name": name,
            "description": f"Submit form {name} to {path}",
            "http": {
                "method": form["method"],
                "path": path
            },
            "params": params,
            "returns": "HTML response page"
        })

    # Build schema
    parsed_base = urllib.parse.urlparse(url)
    base_url = f"{parsed_base.scheme}://{parsed_base.netloc}"
    tool_id = base_url.replace("https://", "").replace("http://", "").replace(".", "_").replace(":", "_")

    import datetime
    schema = {
        "tool_id": tool_id,
        "version": "0.1.0",
        "base_url": base_url,
        "auth": {
            "auth_type": "none",
            "location": None,
            "name": None,
            "scheme": None
        },
        "rate_limit": None,
        "methods": methods,
        "components": {},
        "provenance": {
            "source": "html_form_crawler",
            "fetched_at": datetime.datetime.utcnow().isoformat() + "Z",
            "source_url": url
        }
    }

    return schema

def record_and_save(url: str, output_path: str = "tool.schema.json") -> None:
    """Convenience function to crawl a URL and write the schema to a file."""
    schema = record_site_forms(url)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(schema, f, indent=2)
    print(f"Generated ToolSchema for {url} -> {output_path}")
