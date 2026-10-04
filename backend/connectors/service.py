"""Minimal connector credential/config read layer.

Full CRUD/REST for connectors comes in a later phase -- this is just enough
for `sources/` to read a connector's enabled flag, non-secret config, and
decrypted credentials, plus a user's app-wide settings row.

Every function takes an explicit `owner` (the user's `RecordID`) as its first
argument and scopes its SurrealDB query to that owner -- connectors and
app_settings are per-user now, not global singletons.
"""

import json

from surrealdb import RecordID

from app.db import db as get_connection
from connectors.crypto import decrypt

_APP_SETTINGS_DEFAULTS = {
    "embedding_model": "text-embedding-3-small",
    "sync_intervals": {"heypocket": 86400},
    "theme": {},
}


def _app_settings_id(owner: RecordID) -> RecordID:
    return RecordID("app_settings", owner.id)


async def get_connector(owner: RecordID, kind: str) -> dict | None:
    conn = get_connection()
    rows = await conn.query(
        "SELECT * FROM connector WHERE owner = $owner AND kind = $kind LIMIT 1",
        {"owner": owner, "kind": kind},
    )
    return rows[0] if rows else None


async def credentials_for(owner: RecordID, kind: str) -> dict:
    row = await get_connector(owner, kind)
    if not row:
        return {}
    raw = decrypt(row.get("credentials_encrypted", ""))
    return json.loads(raw) if raw else {}


async def get_app_settings(owner: RecordID) -> dict:
    """The `app_settings:⟨owner_id⟩` row, creating it with defaults if missing."""
    conn = get_connection()
    rid = _app_settings_id(owner)
    row = await conn.select(rid)
    if isinstance(row, list):
        row = row[0] if row else None
    if row:
        return row

    rows = await conn.query(
        "UPSERT $id SET owner = $owner, embedding_model = $embedding_model, "
        "sync_intervals = $sync_intervals, theme = $theme, updated_at = time::now() RETURN AFTER",
        {"id": rid, "owner": owner, **_APP_SETTINGS_DEFAULTS},
    )
    return rows[0]
