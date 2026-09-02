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


def google_push_webhook(src, request):
    """Google push channels POST an empty body + X-Goog-* headers — a "something
    changed" ping. Verify the channel token, then return a delta sync's records.

    A public HTTPS callback is required to *register* the channel (blocked on a
    tailscale-only box — see docs/research/google-workspace-push.md); this is the
    receiving half, ready for when a relay exists.
    """
    conn = connector_for(src)
    expected = (conn.config or {}).get("watch_channel_token") if conn else None
    got = request.headers.get("X-Goog-Channel-Token")
    if not expected or got != expected:
        return None
    if request.headers.get("X-Goog-Resource-State") == "sync":
        return []  # initial handshake ping, nothing to pull yet

    from sources.models import SyncStatus

    st = SyncStatus.objects.filter(source_key=src.key).first()
    return src.sync("webhook", st.cursor if st else None).records
