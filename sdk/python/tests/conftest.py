import sys
from unittest.mock import MagicMock
import pytest

# Define mock classes that mimic the Rust PyO3 classes
class MockMethodPy:
    def __init__(self, name, description, http_method="GET", http_path="/", param_names=None):
        self._name = name
        self._description = description
        self._http_method = http_method
        self._http_path = http_path
        self._param_names = param_names or []

    @property
    def name(self) -> str:
        return self._name

    @property
    def description(self) -> str:
        return self._description

    @property
    def http_method(self) -> str:
        return self._http_method

    @property
    def http_path(self) -> str:
        return self._http_path

    @property
    def param_names(self) -> list[str]:
        return self._param_names

class MockToolSchemaPy:
    def __init__(self, tool_id, version, base_url, methods):
        self._tool_id = tool_id
        self._version = version
        self._base_url = base_url
        self._methods = methods

    @property
    def tool_id(self) -> str:
        return self._tool_id

    @property
    def version(self) -> str:
        return self._version

    @property
    def base_url(self) -> str:
        return self._base_url

    @property
    def methods(self) -> list[MockMethodPy]:
        return self._methods

    def call_blocking(self, method_name, params, auth=None):
        # Reconstruct standard JSON string output for tests
        import json
        return json.dumps({
            "status": "success",
            "method": method_name,
            "params": dict(params),
            "auth": dict(auth) if auth else None
        })

class MockMcpServerHandle:
    def __init__(self, port):
        self._port = port
    def stop(self) -> None:
        pass
    def port(self) -> int:
        return self._port

# Define mock function behaviors
def mock_parse_openapi_url(url):
    # Mock behavior for testing
    if "error" in url:
        raise ValueError("Simulated network/parse error")
    return MockToolSchemaPy(
        tool_id="github",
        version="0.1.0",
        base_url="https://api.github.com",
        methods=[
            MockMethodPy("search_repositories", "Search repos", "GET", "/search/repositories", ["query"])
        ]
    )

def mock_infer_from_html_py(url, html):
    return MockToolSchemaPy(
        tool_id="inferred",
        version="0.1.0",
        base_url="https://example.com",
        methods=[
            MockMethodPy("get_form", "GET /form", "GET", "/form", [])
        ]
    )

def mock_schema_from_json(json_str):
    import json
    data = json.loads(json_str)
    methods = [
        MockMethodPy(
            m["name"], 
            m["description"], 
            m.get("http", {}).get("method", "GET"),
            m.get("http", {}).get("path", "/"),
            [p["name"] for p in m.get("params", [])]
        )
        for m in data.get("methods", [])
    ]
    return MockToolSchemaPy(
        tool_id=data.get("tool_id", "inferred"),
        version=data.get("version", "0.1.0"),
        base_url=data.get("base_url", "https://example.com"),
        methods=methods
    )

def mock_schema_to_json(schema):
    import json
    methods_json = []
    for m in schema.methods:
        methods_json.append({
            "name": m.name,
            "description": m.description,
            "http": {"method": m.http_method, "path": m.http_path},
            "params": [{"name": p, "type": "str", "required": True} for p in m.param_names]
        })
    return json.dumps({
        "tool_id": schema.tool_id,
        "version": schema.version,
        "base_url": schema.base_url,
        "methods": methods_json
    })

def mock_start_mcp_server(schema, port):
    return MockMcpServerHandle(port if port > 0 else 3000)

# Create the module mock and populate functions
if "agentool._native" not in sys.modules:
    mock_native = MagicMock()
    mock_native.parse_openapi_url = MagicMock(side_effect=mock_parse_openapi_url)
    mock_native.parse_openapi_str = MagicMock(side_effect=mock_parse_openapi_url)  # same signature
    mock_native.infer_from_html_py = MagicMock(side_effect=mock_infer_from_html_py)
    mock_native.schema_from_json = MagicMock(side_effect=mock_schema_from_json)
    mock_native.schema_to_json = MagicMock(side_effect=mock_schema_to_json)
    mock_native.start_mcp_server = MagicMock(side_effect=mock_start_mcp_server)
    mock_native.ToolSchemaPy = MockToolSchemaPy
    mock_native.MethodPy = MockMethodPy
    mock_native.McpServerHandle = MockMcpServerHandle
    # Inject into sys.modules immediately on conftest load
    sys.modules["agentool._native"] = mock_native
else:
    mock_native = sys.modules["agentool._native"]

@pytest.fixture(autouse=True)
def fake_native():
    """Returns the mocked native Rust module."""
    return mock_native
