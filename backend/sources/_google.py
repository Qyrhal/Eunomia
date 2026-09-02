"""Shared plumbing for the three Google Workspace sources (#11).

Not a source package (leading underscore -> discovery skips it). Each Google
source calls `google_client()` and, after using it, `persist_refreshed_token()`
so a refreshed access token is written back to the shared "google" Connector.
"""

from connectors.clients import GoogleClient
from sources.registry import connector_for, credentials_for


def google_client(src) -> GoogleClient:
    return GoogleClient(credentials_for(src))


def persist_refreshed_token(src, client: GoogleClient) -> None:
    conn = connector_for(src)
    if conn:
        conn.credentials = client.refreshed_credentials_dict()
        conn.save(update_fields=["credentials_encrypted", "updated_at"])
