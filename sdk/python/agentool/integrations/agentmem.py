"""
agentool.integrations.agentmem — Optional AgentMem schema caching + episodic logging.

When an ``agentmem.Memory`` object is passed to the ``Tool`` constructor,
Agentool automatically:

1. **Caches schemas** — on first load the parsed ToolSchema is serialised to
   JSON and stored in structured memory under key ``schema:<url>``.  On
   subsequent instantiations the schema is loaded from cache (instant, no
   network).

2. **Logs tool calls** — every ``Tool.call()`` invocation writes an episodic
   memory entry: ``"called <service>.<method>"`` with a result summary.

Usage::

    from agentmem import Memory
    from agentool import Tool

    mem = Memory(namespace="tool-schemas")
    github = Tool("https://api.github.com", memory=mem)
    # First call: schema fetched and cached.
    # Subsequent: schema loaded from memory (instant).
"""

from __future__ import annotations

from typing import Any, Optional


def cache_schema(memory: Any, key: str, schema_json: str) -> None:
    """Store a serialised ToolSchema in AgentMem structured memory.

    Parameters
    ----------
    memory : agentmem.Memory
        A Memory instance.
    key : str
        Cache key, typically ``"schema:<url>"``.
    schema_json : str
        JSON-serialised ToolSchema.
    """
    memory.set(key, schema_json.encode("utf-8"))


def load_cached_schema(memory: Any, key: str) -> Optional[str]:
    """Load a cached ToolSchema from AgentMem structured memory.

    Parameters
    ----------
    memory : agentmem.Memory
        A Memory instance.
    key : str
        Cache key, typically ``"schema:<url>"``.

    Returns
    -------
    str | None
        The cached JSON string, or ``None`` if not found.
    """
    raw = memory.get(key)
    if raw is None:
        return None
    if isinstance(raw, bytes):
        return raw.decode("utf-8")
    if isinstance(raw, str):
        return raw
    return None


def log_tool_call(
    memory: Any,
    service: str,
    method: str,
    success: bool,
    summary: str,
) -> None:
    """Log a tool call as an episodic memory entry.

    Parameters
    ----------
    memory : agentmem.Memory
        A Memory instance.
    service : str
        The service name (e.g. ``"github"``).
    method : str
        The method name (e.g. ``"search_repositories"``).
    success : bool
        Whether the call succeeded.
    summary : str
        Brief result summary.
    """
    status = "success" if success else "failed"
    memory.log_episode(
        action=f"called {service}.{method}",
        result_summary=f"{status}: {summary[:200]}",
    )
