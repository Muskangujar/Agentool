"""Tests for optional AgentID and AgentMem integrations in the Tool class."""

import json
import os
import pytest
from unittest.mock import MagicMock, patch

from agentool import Tool


# ── AgentID Integration ──────────────────────────────────────────────────────


class TestAgentIDIntegration:
    def test_call_without_identity_skips_auth(self):
        """Tool with no identity should call without injecting credentials."""
        tool = Tool("https://api.github.com")
        result = tool.call("search_repositories", query="test")
        assert result["auth"] is None

    def test_call_with_identity_env_credential(self):
        """Tool with identity should inject credentials from env var."""
        identity = MagicMock()
        identity.fingerprint = "ag:sha256:abc123"

        with patch.dict(os.environ, {"AGENTID_CREDENTIALS_GITHUB": "ghp_faketoken123"}):
            tool = Tool("https://api.github.com", identity=identity)
            result = tool.call("search_repositories", query="test")
            assert result["auth"] is not None
            assert result["auth"]["type"] == "bearer"
            assert result["auth"]["token"] == "ghp_faketoken123"

    def test_call_with_identity_token_env(self):
        """Tool with identity should fall back to SERVICE_TOKEN env var."""
        identity = MagicMock()
        identity.fingerprint = "ag:sha256:abc123"

        with patch.dict(os.environ, {"GITHUB_TOKEN": "ghp_alt_token"}, clear=False):
            # Make sure the primary key is NOT set
            env_copy = os.environ.copy()
            env_copy.pop("AGENTID_CREDENTIALS_GITHUB", None)
            with patch.dict(os.environ, env_copy, clear=True):
                with patch.dict(os.environ, {"GITHUB_TOKEN": "ghp_alt_token"}):
                    tool = Tool("https://api.github.com", identity=identity)
                    result = tool.call("search_repositories", query="test")
                    assert result["auth"]["token"] == "ghp_alt_token"

    def test_call_with_identity_no_credentials_raises(self):
        """Tool with identity but no credentials anywhere should raise ValueError."""
        identity = MagicMock()
        identity.fingerprint = "ag:sha256:abc123"

        # Clear all possible credential sources
        env = {k: v for k, v in os.environ.items()
               if "AGENTID" not in k and "GITHUB" not in k}
        with patch.dict(os.environ, env, clear=True):
            with patch("pathlib.Path.exists", return_value=False):
                tool = Tool("https://api.github.com", identity=identity)
                with pytest.raises(ValueError, match="No credentials for github"):
                    tool.call("search_repositories", query="test")

    def test_call_with_identity_credentials_file(self, tmp_path):
        """Tool with identity should read credentials from ~/.agentid/credentials.json."""
        identity = MagicMock()
        identity.fingerprint = "ag:sha256:abc123"

        creds_file = tmp_path / "credentials.json"
        creds_file.write_text(json.dumps({
            "github": "ghp_from_file_token"
        }))

        env = {k: v for k, v in os.environ.items()
               if "AGENTID" not in k and "GITHUB" not in k}
        with patch.dict(os.environ, env, clear=True):
            with patch("pathlib.Path.home", return_value=tmp_path.parent):
                # We need the .agentid dir structure
                agentid_dir = tmp_path.parent / ".agentid"
                agentid_dir.mkdir(exist_ok=True)
                creds = agentid_dir / "credentials.json"
                creds.write_text(json.dumps({"github": "ghp_from_file_token"}))

                tool = Tool("https://api.github.com", identity=identity)
                result = tool.call("search_repositories", query="test")
                assert result["auth"]["token"] == "ghp_from_file_token"


# ── AgentMem Integration ─────────────────────────────────────────────────────


class TestAgentMemIntegration:
    def test_schema_cached_in_memory(self):
        """Tool with memory should cache the schema on first load."""
        mem = MagicMock()
        mem.get.return_value = None  # No cache yet

        tool = Tool("https://api.github.com", memory=mem)

        # Verify set() was called with the schema key
        mem.set.assert_called_once()
        call_args = mem.set.call_args
        assert call_args[0][0] == "schema:https://api.github.com"
        assert isinstance(call_args[0][1], bytes)  # Schema JSON as bytes

    def test_schema_loaded_from_cache(self):
        """Tool with memory should load schema from cache if available."""
        cached_schema = json.dumps({
            "tool_id": "cached_github",
            "version": "0.1.0",
            "base_url": "https://api.github.com",
            "methods": [{
                "name": "cached_method",
                "description": "From cache",
                "http": {"method": "GET", "path": "/cached"},
                "params": []
            }]
        })
        mem = MagicMock()
        mem.get.return_value = cached_schema.encode("utf-8")

        tool = Tool("https://api.github.com", memory=mem)

        # Should have loaded from cache, NOT called set()
        mem.set.assert_not_called()
        assert tool.methods[0].name == "cached_method"

    def test_call_logs_episode(self):
        """Tool with memory should log an episode after each call."""
        mem = MagicMock()
        mem.get.return_value = None  # No cache

        tool = Tool("https://api.github.com", memory=mem)
        tool.call("search_repositories", query="test")

        mem.log_episode.assert_called_once()
        call_kwargs = mem.log_episode.call_args
        assert "github" in call_kwargs.kwargs["action"]
        assert "search_repositories" in call_kwargs.kwargs["action"]
