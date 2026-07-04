"""Tests for optional AgentID and AgentMem integrations in the Tool class."""

import json
import os
import pytest
from unittest.mock import MagicMock, patch

from agentool import Tool

DUMMY_SPEC_URL = f"file://{os.path.abspath(os.path.join(os.path.dirname(__file__), 'dummy_openapi.json'))}"


# ── AgentID Integration ──────────────────────────────────────────────────────


class TestAgentIDIntegration:
    def test_call_without_identity_skips_auth(self):
        """Tool with no identity should call without injecting credentials."""
        tool = Tool(DUMMY_SPEC_URL)
        mock_schema = MagicMock()
        mock_schema.call_blocking.return_value = '{"status": "ok"}'
        tool._native_schema = mock_schema
        tool._extract_service_name = MagicMock(return_value="dummy_openapi")
        tool.call("search_repositories", query="test")
        mock_schema.call_blocking.assert_called_once_with("search_repositories", {"query": "test"}, None)

    def test_call_with_identity_env_credential(self):
        """Tool with identity should inject credentials from env var."""
        identity = MagicMock()
        identity.fingerprint = "ag:sha256:abc123"

        with patch.dict(os.environ, {"AGENTID_CREDENTIALS_DUMMY_OPENAPI": "ghp_faketoken123"}):
            tool = Tool(DUMMY_SPEC_URL, identity=identity)
            mock_schema = MagicMock()
            mock_schema.call_blocking.return_value = '{"status": "ok"}'
            tool._native_schema = mock_schema
            tool._extract_service_name = MagicMock(return_value="dummy_openapi")
            tool.call("search_repositories", query="test")
            mock_schema.call_blocking.assert_called_once_with(
                "search_repositories", 
                {"query": "test"}, 
                {"type": "bearer", "token": "ghp_faketoken123"}
            )

    def test_call_with_identity_token_env(self):
        """Tool with identity should fall back to SERVICE_TOKEN env var."""
        identity = MagicMock()
        identity.fingerprint = "ag:sha256:abc123"

        env_copy = os.environ.copy()
        env_copy.pop("AGENTID_CREDENTIALS_DUMMY_OPENAPI", None)
        with patch.dict(os.environ, env_copy, clear=True):
            with patch.dict(os.environ, {"DUMMY_OPENAPI_TOKEN": "ghp_alt_token"}):
                tool = Tool(DUMMY_SPEC_URL, identity=identity)
                mock_schema = MagicMock()
                mock_schema.call_blocking.return_value = '{"status": "ok"}'
                tool._native_schema = mock_schema
                tool._extract_service_name = MagicMock(return_value="dummy_openapi")
                tool.call("search_repositories", query="test")
                mock_schema.call_blocking.assert_called_once_with(
                    "search_repositories", 
                    {"query": "test"}, 
                    {"type": "bearer", "token": "ghp_alt_token"}
                )

    def test_call_with_identity_no_credentials_raises(self):
        """Tool with identity but no credentials anywhere should raise ValueError."""
        identity = MagicMock()
        identity.fingerprint = "ag:sha256:abc123"

        env = {k: v for k, v in os.environ.items()
               if "AGENTID" not in k and "DUMMY_OPENAPI" not in k}
        with patch.dict(os.environ, env, clear=True):
            with patch("pathlib.Path.exists", return_value=False):
                tool = Tool(DUMMY_SPEC_URL, identity=identity)
                tool._extract_service_name = MagicMock(return_value="dummy_openapi")
                with pytest.raises(ValueError, match="No credentials for dummy_openapi"):
                    tool.call("search_repositories", query="test")

    def test_call_with_identity_credentials_file(self, tmp_path):
        """Tool with identity should read credentials from ~/.agentid/credentials.json."""
        identity = MagicMock()
        identity.fingerprint = "ag:sha256:abc123"

        env = {k: v for k, v in os.environ.items()
               if "AGENTID" not in k and "DUMMY_OPENAPI" not in k}
        with patch.dict(os.environ, env, clear=True):
            with patch("pathlib.Path.home", return_value=tmp_path.parent):
                agentid_dir = tmp_path.parent / ".agentid"
                agentid_dir.mkdir(exist_ok=True)
                creds = agentid_dir / "credentials.json"
                creds.write_text(json.dumps({"dummy_openapi": "ghp_from_file_token"}))

                tool = Tool(DUMMY_SPEC_URL, identity=identity)
                mock_schema = MagicMock()
                mock_schema.call_blocking.return_value = '{"status": "ok"}'
                tool._native_schema = mock_schema
                tool._extract_service_name = MagicMock(return_value="dummy_openapi")
                tool.call("search_repositories", query="test")
                mock_schema.call_blocking.assert_called_once_with(
                    "search_repositories", 
                    {"query": "test"}, 
                    {"type": "bearer", "token": "ghp_from_file_token"}
                )


# ── AgentMem Integration ─────────────────────────────────────────────────────


class TestAgentMemIntegration:
    def test_schema_cached_in_memory(self):
        """Tool with memory should cache the schema on first load."""
        mem = MagicMock()
        mem.get.return_value = None  # No cache yet

        tool = Tool(DUMMY_SPEC_URL, memory=mem)

        # Verify set() was called with the schema key
        mem.set.assert_called_once()
        call_args = mem.set.call_args
        assert call_args[0][0] == f"schema:{DUMMY_SPEC_URL}"
        assert isinstance(call_args[0][1], bytes)  # Schema JSON as bytes

    def test_schema_loaded_from_cache(self):
        """Tool with memory should load schema from cache if available."""
        cached_schema = json.dumps({
            "tool_id": "cached_github",
            "version": "0.1.0",
            "base_url": "https://api.github.com",
            "auth": {"type": "none"},
            "provenance": {"source": "openapi", "fetched_at": "2026-07-04T00:00:00Z"},
            "methods": [{
                "name": "cached_method",
                "description": "From cache",
                "http": {"method": "GET", "path": "/cached"},
                "params": []
            }]
        })
        mem = MagicMock()
        mem.get.return_value = cached_schema.encode("utf-8")

        tool = Tool(DUMMY_SPEC_URL, memory=mem)

        # Should have loaded from cache, NOT called set()
        mem.set.assert_not_called()
        assert tool.methods[0].name == "cached_method"

    def test_call_logs_episode(self):
        """Tool with memory should log an episode after each call."""
        mem = MagicMock()
        mem.get.return_value = None  # No cache

        tool = Tool(DUMMY_SPEC_URL, memory=mem)
        mock_schema = MagicMock()
        mock_schema.call_blocking.return_value = '{"status": "ok"}'
        tool._native_schema = mock_schema
        tool._extract_service_name = MagicMock(return_value="dummy_openapi")
        tool.call("search_repositories", query="test")

        mem.log_episode.assert_called_once()
        call_kwargs = mem.log_episode.call_args
        assert "dummy_openapi" in call_kwargs.kwargs["action"]
        assert "search_repositories" in call_kwargs.kwargs["action"]
