"""
agentool.integrations.agentid — Optional AgentID credential injection.

When an ``AgentIdentity`` (from ``agentidentity-auth``) is passed to the
``Tool`` constructor, Agentool checks whether stored credentials exist for
the target API.  If yes, they are injected automatically on every call.
If no, a clear error tells the user exactly what to run.

Usage::

    from agentid import AgentIdentity
    from agentool import Tool

    identity = AgentIdentity(name="research-bot", project="phd-lab")
    github = Tool("https://api.github.com", identity=identity)
    # Credentials injected automatically.  No hardcoded API keys.
"""

from __future__ import annotations

from typing import Any, Optional


def extract_fingerprint(identity: Any) -> str:
    """Extract the cryptographic fingerprint from an AgentIdentity object.

    Parameters
    ----------
    identity : AgentIdentity
        An object from ``agentidentity-auth`` with a ``.fingerprint`` property.

    Returns
    -------
    str
        The ``ag:sha256:...`` fingerprint string.

    Raises
    ------
    TypeError
        If the object doesn't have the expected interface.
    """
    fp = getattr(identity, "fingerprint", None)
    if fp is None:
        raise TypeError(
            "Expected an AgentIdentity object with a .fingerprint property, "
            f"got {type(identity).__name__!r}."
        )
    return fp


def get_agent_token(identity: Any, scopes: list[str] | None = None) -> Optional[bytes]:
    """Mint a fresh scoped token from the identity, if the identity supports it.

    Parameters
    ----------
    identity : AgentIdentity
        An AgentIdentity object.
    scopes : list[str] | None
        Permission scopes (e.g. ``["read:api", "write:data"]``).

    Returns
    -------
    bytes | None
        Raw binary token, or ``None`` if minting is not available.
    """
    mint = getattr(identity, "mint_token", None)
    if mint is None:
        return None
    return mint(scopes=scopes or [], ttl_seconds=900, max_calls=0)
