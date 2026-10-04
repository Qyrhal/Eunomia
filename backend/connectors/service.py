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
from connectors.crypto import decrypt, encrypt

_APP_SETTINGS_DEFAULTS = {
    "embedding_model": "text-embedding-3-small",
    "sync_intervals": {"heypocket": 86400},
    "theme": {},
}

# real, non-pseudo connector kinds the CRUD surface manages (excludes "demo",
# which is a schema-level placeholder, not a connector users configure here).
CONNECTOR_KINDS = ["up_bank", "pocketai", "open_connector"]


def _app_settings_id(owner: RecordID) -> RecordID:
    return RecordID("app_settings", owner.id)


async def get_connector(owner: RecordID, kind: str) -> dict | None:
    conn = get_connection()
    rows = await conn.query(
        "SELECT * FROM connector WHERE owner = $owner AND kind = $kind LIMIT 1",
        {"owner": owner, "kind": kind},
    )
    return rows[0] if rows else None


async def get_or_create_connector(owner: RecordID, kind: str) -> dict:
    row = await get_connector(owner, kind)
    if row:
        return row
    conn = get_connection()
    rows = await conn.query(
        "CREATE connector SET owner = $owner, kind = $kind RETURN AFTER",
        {"owner": owner, "kind": kind},
    )
    return rows[0]


async def list_connectors(owner: RecordID) -> list[dict]:
    return [await get_or_create_connector(owner, kind) for kind in CONNECTOR_KINDS]


async def upsert_connector(
    owner: RecordID,
    kind: str,
    *,
    enabled: bool | None = None,
    config: dict | None = None,
    credentials: dict | None = None,
) -> dict:
    """Partial update of a connector's config/credentials, scoped to `owner`.
    Credentials are merged (not replaced) with whatever is already on file,
    mirroring the old Django serializer -- saving one refreshed secret must
    not drop the others."""
    conn = get_connection()
    row = await get_or_create_connector(owner, kind)
    rid = row["id"]

    sets = {}
    if enabled is not None:
        sets["enabled"] = enabled
    if config is not None:
        sets["config"] = {**(row.get("config") or {}), **config}
    if credentials is not None:
        existing_raw = decrypt(row.get("credentials_encrypted", ""))
        existing = json.loads(existing_raw) if existing_raw else {}
        sets["credentials_encrypted"] = encrypt(json.dumps({**existing, **credentials}))

    if not sets:
        return row

    set_clause = ", ".join(f"{k} = ${k}" for k in sets) + ", updated_at = time::now()"
    rows = await conn.query(f"UPDATE $id SET {set_clause} RETURN AFTER", {"id": rid, **sets})
    return rows[0]


async def update_app_settings(owner: RecordID, **fields) -> dict:
    """Partial update of the caller's `app_settings` row. Each provided field
    replaces its current value outright (no deep merge) -- matching the old
    Django serializer's partial-update semantics. `openai_api_key`, if given,
    is encrypted before storage and stored as `openai_api_key_encrypted`."""
    await get_app_settings(owner)  # ensure the row exists
    conn = get_connection()
    rid = _app_settings_id(owner)

    sets: dict = {}
    if "openai_api_key" in fields:
        sets["openai_api_key_encrypted"] = encrypt(fields.pop("openai_api_key") or "")
    sets.update(fields)

    if not sets:
        return await get_app_settings(owner)

    set_clause = ", ".join(f"{k} = ${k}" for k in sets) + ", updated_at = time::now()"
    rows = await conn.query(f"UPDATE $id SET {set_clause} RETURN AFTER", {"id": rid, **sets})
    return rows[0]


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
