"""Sources dashboard data: the registered sources, each one's sync health,
and whether the caller has an enabled connector backing it -- the "what's
next / what's not" data for the dashboard."""

from fastapi import APIRouter, Depends, HTTPException

from app.auth import current_user
from app.db import db as get_connection
from app.models_user import User
from sources import registry
from sources.scheduler import sync_source

router = APIRouter(prefix="/sources", tags=["sources"], dependencies=[Depends(current_user)])


async def _sync_status_rows(owner) -> dict[str, dict]:
    conn = get_connection()
    rows = await conn.query("SELECT * FROM sync_status WHERE owner = $owner", {"owner": owner})
    out = {}
    for row in rows:
        raw = row["id"].id if hasattr(row["id"], "id") else str(row["id"])
        _, _, key = raw.partition(":")
        out[key or raw] = row
    return out


async def _record_counts(owner) -> dict[str, int]:
    """Cached-record count per source key, for the dashboard's totals -- a
    single grouped count, not a per-source query."""
    conn = get_connection()
    rows = await conn.query(
        "SELECT source, count() AS count FROM cache_record WHERE owner = $owner AND deleted = false GROUP BY source",
        {"owner": owner},
    )
    return {row["source"]: row["count"] for row in rows}


def _status_out(row: dict | None) -> dict:
    if not row:
        return {"cursor": "", "last_run": None, "last_ok": None, "last_error": "", "consecutive_failures": 0}
    return {
        "cursor": row.get("cursor", ""),
        "last_run": row.get("last_run"),
        "last_ok": row.get("last_ok"),
        "last_error": row.get("last_error", ""),
        "consecutive_failures": row.get("consecutive_failures", 0),
    }


@router.get("")
async def list_sources(user: User = Depends(current_user)) -> list[dict]:
    statuses = await _sync_status_rows(user.id)
    enabled_keys = {src.key for src in await registry.enabled(user.id)}
    counts = await _record_counts(user.id)
    return [
        {
            "key": src.key,
            "label": src.label,
            "provider": src.provider_key,
            "record_types": src.record_types,
            "connected": src.key in enabled_keys,
            "sync_status": _status_out(statuses.get(src.key)),
            "record_count": counts.get(src.key, 0),
        }
        for src in registry.all()
    ]


@router.get("/status")
async def sources_status(user: User = Depends(current_user)) -> dict:
    statuses = await _sync_status_rows(user.id)
    return {key: _status_out(row) for key, row in statuses.items()}


@router.post("/{key}/sync")
async def sync_now(key: str, user: User = Depends(current_user)) -> dict:
    if registry.get(key) is None:
        raise HTTPException(status_code=404, detail=f"no source {key!r}")
    return await sync_source(user.id, key, "poll")
