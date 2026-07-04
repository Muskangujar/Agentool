import pytest
import os
import json
from unittest.mock import patch, MagicMock
from agentool import Tool, Method, Param

DUMMY_SPEC_URL = f"file://{os.path.abspath(os.path.join(os.path.dirname(__file__), 'dummy_openapi.json'))}"

def test_tool_initialization_openapi():
    tool = Tool(DUMMY_SPEC_URL)
    # The URL should be stored as the original URL string provided
    assert tool.url == DUMMY_SPEC_URL
    assert len(tool.methods) == 1
    
    method = tool.methods[0]
    assert method.name == "search_repositories"
    assert method.description == "Search repos"
    assert method.http_method == "GET"
    assert method.http_path == "/search/repositories"
    assert len(method.params) == 1
    assert method.params[0].name == "query"

def test_tool_initialization_fallback_html():
    # Test falling back to HTML scraping
    import urllib.request
    from agentool import _native

    mock_response = MagicMock()
    mock_response.read.return_value = b"<html><code>GET /form</code></html>"
    mock_response.__enter__.return_value = mock_response

    with patch("urllib.request.urlopen", return_value=mock_response):
        tool = Tool("https://example.com/login")
        assert tool.url == "https://example.com/login"
        assert len(tool.methods) == 1
        assert tool.methods[0].name == "form"

def test_tool_call():
    tool = Tool(DUMMY_SPEC_URL)
    
    mock_schema = MagicMock()
    mock_schema.call_blocking.return_value = json.dumps({
        "status": "success",
        "method": "search_repositories",
        "params": {"query": "agentbase"}
    })
    tool._native_schema = mock_schema
    
    result = tool.call("search_repositories", query="agentbase")
    
    assert result["status"] == "success"
    assert result["method"] == "search_repositories"
    assert result["params"]["query"] == "agentbase"
    mock_schema.call_blocking.assert_called_once_with("search_repositories", {"query": "agentbase"}, None)
