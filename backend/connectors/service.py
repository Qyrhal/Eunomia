"""Minimal connector credential/config read layer.

Full CRUD/REST for connectors comes in a later phase -- this is just enough
for `sources/` to read a connector's enabled flag, non-secret config, and
decrypted credentials, plus the app-wide settings singleton.
"""

import json

from surrealdb import RecordID

from app.db import db as get_connection
from connectors.crypto import decrypt

_APP_SETTINGS_ID = RecordID("app_settings", "singleton")

_APP_SETTINGS_DEFAULTS = {
    "embedding_model": "text-embedding-3-small",
    "sync_intervals": {"heypocket": 86400},
    "theme": {},
}


async def get_connector(kind: str) -> dict | None:
    conn = get_connection()
    rows = await conn.query("SELECT * FROM connector WHERE kind = $kind LIMIT 1", {"kind": kind})
    return rows[0] if rows else None


async def credentials_for(kind: str) -> dict:
    row = await get_connector(kind)
    if not row:
        return {}
    raw = decrypt(row.get("credentials_encrypted", ""))
    return json.loads(raw) if raw else {}


async def get_app_settings() -> dict:
    """The `app_settings:singleton` row, creating it with defaults if missing."""
    conn = get_connection()
    row = await conn.select(_APP_SETTINGS_ID)
    if isinstance(row, list):
        row = row[0] if row else None
    if row:
        return row

    rows = await conn.query(
        "UPSERT $id SET embedding_model = $embedding_model, sync_intervals = $sync_intervals, "
        "theme = $theme, updated_at = time::now() RETURN AFTER",
        {"id": _APP_SETTINGS_ID, **_APP_SETTINGS_DEFAULTS},
    )
    return rows[0]
