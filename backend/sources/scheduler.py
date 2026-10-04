"""Sync scheduler jobs. An APScheduler `AsyncIOScheduler` drives these from
the FastAPI app's lifespan. Each job is also directly callable -- that's what
the tests and any on-demand endpoint use.
"""

import logging
from datetime import datetime, timezone

from surrealdb import RecordID

from . import registry

log = logging.getLogger("eunomia.sync")

_BACKOFF = [900, 1800, 3600, 7200, 21600]  # seconds, by consecutive_failures


def backoff_seconds(failures: int) -> int:
    return _BACKOFF[min(failures, len(_BACKOFF) - 1)]


def _sync_status_id(owner: RecordID, key: str) -> RecordID:
    return RecordID("sync_status", f"{owner.id}:{key}")


async def _all_user_ids() -> list[RecordID]:
    from app.db import db as get_connection

    conn = get_connection()
    rows = await conn.query("SELECT id FROM user")
    return [r["id"] for r in rows]


async def _get_sync_status(conn, owner: RecordID, key: str) -> dict:
    row = await conn.select(_sync_status_id(owner, key))
    if isinstance(row, list):
        row = row[0] if row else None
    if row:
        return row
    rows = await conn.query(
        "UPSERT $id SET owner = $owner, cursor = '', consecutive_failures = 0, last_error = '', "
        "last_report = {} RETURN AFTER",
        {"id": _sync_status_id(owner, key), "owner": owner},
    )
    return rows[0]


async def sync_source(owner: RecordID, key: str, mode: str = "poll") -> dict:
    """Run one source sync for `owner`, record health, apply backoff. Never raises."""
    from app.db import db as get_connection

    conn = get_connection()
    st = await _get_sync_status(conn, owner, key)
    now = datetime.now(timezone.utc)

    try:
        report, cursor = await registry.run_sync(owner, key, mode, st.get("cursor") or None)
        report_dict = report.as_dict()
        await conn.query(
            "UPDATE $id SET last_run = $now, cursor = $cursor, last_ok = $now, "
            "last_error = '', consecutive_failures = 0, last_report = $report",
            {"id": _sync_status_id(owner, key), "now": now, "cursor": cursor or "", "report": report_dict},
        )
        log.info("sync %s/%s: %s", owner, key, report_dict)
        return report_dict
    except Exception as e:
        error = f"{type(e).__name__}: {e}"
        failures = int(st.get("consecutive_failures", 0)) + 1
        await conn.query(
            "UPDATE $id SET last_run = $now, consecutive_failures = $failures, last_error = $error",
            {"id": _sync_status_id(owner, key), "now": now, "failures": failures, "error": error},
        )
        log.warning("sync %s/%s failed (%d): %s", owner, key, failures, e)
        return {"source": key, "error": error}


async def poll_all() -> list[dict]:
    """Poll every enabled source, for every user, whose backoff window has
    elapsed."""
    from app.db import db as get_connection

    conn = get_connection()
    out = []
    now = datetime.now(timezone.utc)
    for owner in await _all_user_ids():
        for src in await registry.enabled(owner):
            row = await conn.select(_sync_status_id(owner, src.key))
            if isinstance(row, list):
                row = row[0] if row else None
            if row and row.get("consecutive_failures") and row.get("last_run"):
                wait = backoff_seconds(row["consecutive_failures"])
                last_run = row["last_run"]
                if hasattr(last_run, "tzinfo") and (now - last_run).total_seconds() < wait:
                    continue
            out.append(await sync_source(owner, src.key))
    return out


async def backfill_embeddings(limit: int = 200) -> int:
    """Re-embed cache records that missed embedding (backend was down), for
    every user."""
    from app.db import db as get_connection
    from cache.search import set_embedding
    from embeddings.service import embed

    conn = get_connection()
    n = 0
    for owner in await _all_user_ids():
        rows = await conn.query(
            "SELECT * FROM cache_record WHERE owner = $owner AND embedding = NONE "
            "AND deleted = false AND body_text != '' LIMIT $limit",
            {"owner": owner, "limit": limit},
        )
        if not rows:
            continue
        texts = [f"{r.get('title', '')}\n{r.get('body_text', '')}" for r in rows]
        vecs = await embed(texts)
        for r, v in zip(rows, vecs):
            rid = r["id"]
            raw = rid.id if hasattr(rid, "id") else str(rid)
            _, _, record_id = raw.partition(":")  # strip the owner-id prefix baked into the record id
            await set_embedding(owner, record_id or raw, v)
            n += 1
    return n


async def build_scheduler():
    """Wire jobs onto an AsyncIOScheduler. Import-light so tests can skip it.
    Must be awaited from a running event loop (the app's lifespan).

    Enumerates (owner, source_key) pairs across every user's enabled
    connectors -- there is no single global "enabled sources" set anymore."""
    from apscheduler.schedulers.asyncio import AsyncIOScheduler

    from connectors.service import get_app_settings

    sched = AsyncIOScheduler()

    for owner in await _all_user_ids():
        settings_row = await get_app_settings(owner)
        sync_intervals = dict(settings_row.get("sync_intervals") or {})
        sync_intervals.setdefault("heypocket", 86400)  # heypocket: 24h, per the plan

        for src in await registry.enabled(owner):
            interval = int(sync_intervals.get(src.key, 900))
            sched.add_job(
                sync_source,
                "interval",
                seconds=interval,
                args=[owner, src.key],
                id=f"sync:{owner.id}:{src.key}",
                max_instances=1,
                coalesce=True,
            )

    sched.add_job(poll_all, "interval", seconds=300, id="poll_all", max_instances=1, coalesce=True)
    sched.add_job(backfill_embeddings, "interval", seconds=600, id="backfill_embeddings", max_instances=1)

    return sched
