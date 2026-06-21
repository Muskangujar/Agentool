"""Tests for MCP server lifecycle (start/stop) via the Tool facade."""

import pytest
from agentool import Tool


class TestServeMcp:
    def test_serve_mcp_returns_handle(self):
        tool = Tool("https://api.github.com")
        handle = tool.serve_mcp(port=0)
        assert handle is not None
        assert handle.port() == 3000  # mock returns 3000 for port=0

    def test_serve_mcp_custom_port(self):
        tool = Tool("https://api.github.com")
        handle = tool.serve_mcp(port=8080)
        assert handle.port() == 8080

    def test_serve_mcp_stop(self):
        tool = Tool("https://api.github.com")
        handle = tool.serve_mcp(port=4000)
        # stop() should not raise
        handle.stop()
