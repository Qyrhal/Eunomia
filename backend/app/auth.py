"""Single static bearer token in front of the whole API surface.

Set ``EUNOMIA_API_TOKEN`` to require ``Authorization: Bearer <token>`` on every
``/api/`` route. Leave it unset for open access (local dev, fully trusted LAN).
The MCP server imports :func:`token_ok` so the same token guards both
transports.
"""

import secrets

from fastapi import Header, HTTPException

from app.config import settings


def token_ok(auth_header: str | None) -> bool:
    """True if the request may proceed: no token configured, or a matching bearer."""
    configured = settings.EUNOMIA_API_TOKEN or ""
    if not configured:
        return True
    if not auth_header:
        return False
    scheme, _, value = auth_header.partition(" ")
    return scheme.lower() == "bearer" and secrets.compare_digest(value, configured)


def require_token(authorization: str | None = Header(default=None)) -> None:
    if not token_ok(authorization):
        raise HTTPException(status_code=401, detail="Missing or invalid API token.")
