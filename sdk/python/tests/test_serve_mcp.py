import pytest
import os
from unittest.mock import patch, MagicMock
from agentool import Tool

DUMMY_SPEC_URL = f"file://{os.path.abspath(os.path.join(os.path.dirname(__file__), 'dummy_openapi.json'))}"

class TestServeMcp:
    def test_serve_mcp_returns_handle(self):
        tool = Tool(DUMMY_SPEC_URL)
        # Port 0 means ephemeral, we don't mock it now, let it spin up a real socket!
        handle = tool.serve_mcp(port=0)
        assert handle is not None
        assert handle.port() > 0
        handle.stop()

    def test_serve_mcp_custom_port(self):
        tool = Tool(DUMMY_SPEC_URL)
        handle = tool.serve_mcp(port=3002)
        assert handle is not None
        assert handle.port() == 3002
        handle.stop()

    def test_serve_mcp_stop(self):
        tool = Tool(DUMMY_SPEC_URL)
        handle = tool.serve_mcp(port=0)
        # Should not raise
        handle.stop()
