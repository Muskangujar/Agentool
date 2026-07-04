from __future__ import annotations
import json
import urllib.parse
from typing import Any, Dict, List, Optional

class Param:
    """Represents a method parameter schema."""
    def __init__(self, name: str, type_str: str = "str", required: bool = False, description: str = ""):
        self.name = name
        self.type = type_str
        self.required = required
        self.description = description

    def __repr__(self) -> str:
        return f"Param(name={self.name!r}, type={self.type!r}, required={self.required})"

class Method:
    """Represents an API/tool method."""
    def __init__(self, native_method: Any):
        self._native = native_method
        self.name = native_method.name
        self.description = native_method.description
        self.http_method = native_method.http_method
        self.http_path = native_method.http_path
        self.params = [Param(p) for p in native_method.param_names]
        self.returns = "str"

    def __repr__(self) -> str:
        return f"Method(name={self.name!r}, path={self.http_path!r}, params={self.params})"

class Tool:
    """Wrapper for external APIs turned into Agentic Tools."""
    def __init__(
        self,
        url: str,
        identity: Optional[Any] = None,
        memory: Optional[Any] = None,
    ) -> None:
        self.url = url
        self.identity = identity
        self.memory = memory
        self._native_schema = self._load_schema()

    def _load_schema(self) -> Any:
        # Check memory cache if memory is provided
        if self.memory is not None:
            try:
                cached = self.memory.get(f"schema:{self.url}")
                if cached:
                    if isinstance(cached, bytes):
                        cached_str = cached.decode("utf-8")
                    elif isinstance(cached, str):
                        cached_str = cached
                    else:
                        cached_str = json.dumps(cached)
                    
                    from agentool._native import schema_from_json
                    return schema_from_json(cached_str)
            except Exception:
                # If memory retrieval fails, fall back silently to network
                pass

        # Parse scheme or URL
        fetch_url = self.url
        if not (fetch_url.startswith("http://") or fetch_url.startswith("https://") or fetch_url.startswith("file://")):
            fetch_url = "https://" + fetch_url

        from agentool._native import parse_openapi_url, infer_from_html, schema_to_json

        try:
            native_schema = parse_openapi_url(fetch_url)
        except Exception:
            # Fallback: Download HTML and infer schema
            import urllib.request
            try:
                req = urllib.request.Request(
                    fetch_url,
                    headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'}
                )
                with urllib.request.urlopen(req, timeout=10) as response:
                    html_content = response.read().decode('utf-8', errors='ignore')
                native_schema = infer_from_html(fetch_url, html_content)
            except Exception as e:
                raise ValueError(f"Failed to fetch or infer schema from '{self.url}': {e}")

        # Cache schema in memory if memory is provided
        if self.memory is not None:
            try:
                schema_json = schema_to_json(native_schema)
                self.memory.set(f"schema:{self.url}", schema_json.encode("utf-8"))
            except Exception:
                pass

        return native_schema

    @property
    def methods(self) -> List[Method]:
        """Expose list of callable methods."""
        return [Method(m) for m in self._native_schema.methods]

    def call(self, method_name: str, **kwargs) -> Any:
        """Invoke a method on the tool."""
        service_name = self._extract_service_name()
        
        # ── AgentID Credential Injection ─────────────────────────────
        auth_dict = None
        if self.identity is not None:
            creds = self._get_identity_credentials(service_name)
            if creds is None:
                raise ValueError(f"No credentials for {service_name}. Run: agentid grant {service_name}")
            auth_dict = creds

        # ── Call Native ──────────────────────────────────────────────
        raw_result = self._native_schema.call_blocking(method_name, kwargs, auth_dict)

        # Parse JSON if possible, otherwise return string
        try:
            result = json.loads(raw_result)
        except (json.JSONDecodeError, TypeError):
            result = raw_result

        # ── AgentMem Episodic Logging ─────────────────────────────────
        if self.memory is not None:
            try:
                # Truncate summary if too long
                summary = str(result)[:200]
                self.memory.log_episode(
                    action=f"called {service_name}.{method_name}",
                    result_summary=f"Success: {summary}",
                )
            except Exception:
                pass

        return result

    def serve_mcp(self, port: int = 3000) -> Any:
        """Serve this tool as an MCP server. Returns a server handle."""
        from agentool._native import start_mcp_server
        return start_mcp_server(self._native_schema, port)

    def _extract_service_name(self) -> str:
        """Helper to extract domain/service name from the URL."""
        try:
            parsed = urllib.parse.urlparse(self.url)
            host = parsed.netloc or parsed.path
            parts = host.split(".")
            if len(parts) >= 2:
                # Return the main domain label
                return parts[-2]
            return host
        except Exception:
            return "unknown"

    def _get_identity_credentials(self, service: str) -> Optional[Dict[str, str]]:
        """Look up credentials for the service name in environment variables or config files."""
        import os
        
        # 1. Check Env
        env_key = f"AGENTID_CREDENTIALS_{service.upper()}"
        if env_key in os.environ:
            return {"type": "bearer", "token": os.environ[env_key]}
        
        tok_key = f"{service.upper()}_TOKEN"
        if tok_key in os.environ:
            return {"type": "bearer", "token": os.environ[tok_key]}

        # 2. Check ~/.agentid/credentials.json
        import pathlib
        path = pathlib.Path.home() / ".agentid" / "credentials.json"
        if path.exists():
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
                
                # Check using agent's fingerprint if available
                fp = getattr(self.identity, "fingerprint", None)
                if fp and fp in data and service in data[fp]:
                    val = data[fp][service]
                    # If it's a string, wrap it in a bearer dict
                    if isinstance(val, str):
                        return {"type": "bearer", "token": val}
                    return val
                
                # Fallback to general service entry
                if service in data:
                    val = data[service]
                    if isinstance(val, str):
                        return {"type": "bearer", "token": val}
                    return val
            except Exception:
                pass

        return None
