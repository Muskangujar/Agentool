"""
agentool.integrations.langgraph — LangGraph tool-node adapter.

Wraps an ``agentool.Tool`` as a LangGraph-compatible node function so that
any Agentool-wrapped API can be dropped directly into a LangGraph pipeline.

Usage::

    from langgraph.graph import StateGraph
    from agentool import Tool
    from agentool.integrations.langgraph import agentool_node

    github = Tool("https://api.github.com")
    graph = StateGraph(MyState)
    graph.add_node("github_search", agentool_node(github, "search_repositories"))
"""

from __future__ import annotations

from typing import Any, Callable, Dict, Optional


State = Dict[str, Any]


def agentool_node(
    tool: Any,
    method_name: str,
    *,
    params_key: str = "tool_params",
    result_key: str = "tool_result",
    error_key: str = "tool_error",
) -> Callable[[State], State]:
    """Create a LangGraph node that invokes an Agentool method.

    Parameters
    ----------
    tool : agentool.Tool
        An initialised ``Tool`` instance.
    method_name : str
        The method to call (must exist in ``tool.methods``).
    params_key : str
        State key that holds the ``dict`` of method parameters.
    result_key : str
        State key where the result is written.
    error_key : str
        State key where errors are written on failure.
    """

    def _node(state: State) -> State:
        params = state.get(params_key, {})
        try:
            result = tool.call(method_name, **params)
            return {result_key: result}
        except Exception as exc:
            return {error_key: str(exc)}

    _node.__name__ = f"agentool_{method_name}"
    _node.__doc__ = f"Invoke {method_name} via Agentool."
    return _node
