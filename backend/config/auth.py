"""Single static bearer token in front of the whole API surface.

Set ``EUNOMIA_API_TOKEN`` to require ``Authorization: Bearer <token>`` on every
``/api/`` route. Leave it unset for open access (local dev, fully trusted LAN).
The MCP server (#14) imports :func:`token_ok` so the same token guards both
transports. Django admin keeps its own session auth and is unaffected.
"""

import secrets

from django.conf import settings
from rest_framework.permissions import BasePermission


def token_ok(auth_header: str | None) -> bool:
    """True if the request may proceed: no token configured, or a matching bearer."""
    configured = getattr(settings, "EUNOMIA_API_TOKEN", "") or ""
    if not configured:
        return True
    if not auth_header:
        return False
    scheme, _, value = auth_header.partition(" ")
    return scheme.lower() == "bearer" and secrets.compare_digest(value, configured)


class HasApiToken(BasePermission):
    message = "Missing or invalid API token."

    def has_permission(self, request, view):
        return token_ok(request.META.get("HTTP_AUTHORIZATION"))
