"""
agentool.integrations.crewai — CrewAI tool adapter.

Wraps an ``agentool.Tool`` as a CrewAI-compatible ``BaseTool`` so that any
Agentool-wrapped API can be used as a CrewAI agent tool.

Usage::

    from crewai import Agent
    from agentool import Tool
    from agentool.integrations.crewai import AgentoolCrewTool

    github = Tool("https://api.github.com")
    crew_tool = AgentoolCrewTool(github, "search_repositories")

    researcher = Agent(
        role="Researcher",
        tools=[crew_tool],
    )
"""

from __future__ import annotations

from typing import Any, Optional


class AgentoolCrewTool:
    """Adapts an Agentool method into the CrewAI tool interface.

    CrewAI expects tools with ``name``, ``description``, and a ``_run``
    method.  This adapter delegates to ``Tool.call()``.

    Parameters
    ----------
    tool : agentool.Tool
        An initialised ``Tool`` instance.
    method_name : str
        The method to expose.
    """

    def __init__(self, tool: Any, method_name: str) -> None:
        self._tool = tool
        self._method_name = method_name

        # Locate the method metadata for name/description
        match = [m for m in tool.methods if m.name == method_name]
        if not match:
            available = [m.name for m in tool.methods]
            raise ValueError(
                f"Method {method_name!r} not found.  Available: {available}"
            )
        method = match[0]
        self.name: str = method.name
        self.description: str = method.description

    def _run(self, **kwargs: Any) -> Any:
        """Execute the tool method with the given keyword arguments."""
        return self._tool.call(self._method_name, **kwargs)

    def run(self, **kwargs: Any) -> Any:
        """Public alias for ``_run`` (some CrewAI versions call this)."""
        return self._run(**kwargs)
