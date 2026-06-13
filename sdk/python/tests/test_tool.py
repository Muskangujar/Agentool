import pytest
from agentool import Tool, Method, Param

def test_tool_initialization_openapi():
    # Test standard openapi url loading
    tool = Tool("https://api.github.com")
    assert tool.url == "https://api.github.com"
    assert len(tool.methods) == 1
    
    method = tool.methods[0]
    assert method.name == "search_repositories"
    assert method.description == "Search repos"
    assert method.http_method == "GET"
    assert method.http_path == "/search/repositories"
    assert len(method.params) == 1
    assert method.params[0].name == "query"

def test_tool_initialization_fallback_html(fake_native):
    # Test falling back to HTML scraping
    # We will trigger the except block in parse_openapi_url which then calls infer_from_html_py
    import urllib.request
    from unittest.mock import patch, MagicMock
    from tests.conftest import mock_parse_openapi_url

    # Setup parse_openapi_url to raise error
    fake_native.parse_openapi_url.side_effect = ValueError("Not OpenAPI")

    # Mock urllib urlopen
    mock_response = MagicMock()
    mock_response.read.return_value = b"<html><form action='/form' method='POST'><input name='username'></form></html>"
    
    try:
        with patch("urllib.request.urlopen", return_value=mock_response):
            tool = Tool("https://example.com/login")
            assert tool.url == "https://example.com/login"
            assert len(tool.methods) == 1
            assert tool.methods[0].name == "get_form"
    finally:
        fake_native.parse_openapi_url.side_effect = mock_parse_openapi_url

def test_tool_call():
    tool = Tool("https://api.github.com")
    result = tool.call("search_repositories", query="agentbase")
    
    assert result["status"] == "success"
    assert result["method"] == "search_repositories"
    assert result["params"]["query"] == "agentbase"
