"""Sync scheduler jobs (#34). APScheduler drives these from `manage.py run_worker`.
Each job is also directly callable — that's what the tests and the on-demand
endpoint use.
"""

import logging

from django.utils import timezone

from . import registry
from .models import SyncStatus

log = logging.getLogger("eunomia.sync")

_BACKOFF = [900, 1800, 3600, 7200, 21600]  # seconds, by consecutive_failures


def backoff_seconds(failures: int) -> int:
    return _BACKOFF[min(failures, len(_BACKOFF) - 1)]


def sync_source(key: str, mode: str = "poll") -> dict:
    """Run one source sync, record health, apply backoff. Never raises."""
    st, _ = SyncStatus.objects.get_or_create(source_key=key)
    now = timezone.now()
    st.last_run = now
    try:
        report, cursor = registry.run_sync(key, mode, st.cursor or None)
        st.cursor = cursor or ""
        st.last_ok = now
        st.last_error = ""
        st.consecutive_failures = 0
        st.last_report = report.as_dict()
        st.save()
        log.info("sync %s: %s", key, report.as_dict())
        return report.as_dict()
    except Exception as e:
        st.consecutive_failures += 1
        st.last_error = f"{type(e).__name__}: {e}"
        st.save()
        log.warning("sync %s failed (%d): %s", key, st.consecutive_failures, e)
        return {"source": key, "error": st.last_error}


def poll_all() -> list[dict]:
    """Poll every enabled source whose backoff window has elapsed."""
    out = []
    now = timezone.now()
    for src in registry.enabled():
        st = SyncStatus.objects.filter(source_key=src.key).first()
        if st and st.consecutive_failures and st.last_run:
            wait = backoff_seconds(st.consecutive_failures)
            if (now - st.last_run).total_seconds() < wait:
                continue
        out.append(sync_source(src.key))
    return out


def renew_watch_channels() -> dict:
    """Re-arm Google Calendar/Drive push channels nearing their 7-day expiry.

    Only runs when the google Connector has a `watch_callback_url` in its config
    (a public HTTPS endpoint — see docs/research/google-workspace-push.md). On a
    tailscale-only box there is none, so this is a logged no-op and polling
    covers freshness.
    """
    from connectors.models import Connector

    conn = Connector.objects.filter(kind="google", enabled=True).first()
    url = (conn.config or {}).get("watch_callback_url") if conn else None
    if not url:
        log.debug("renew_watch_channels: no watch_callback_url — polling only")
        return {"skipped": "no public callback url"}
    # Registration via events.watch / changes.watch goes here once a relay exists.
    return {"noop": True, "callback": url}


def backfill_embeddings(limit: int = 200) -> int:
    """Re-embed cache records + tasks that missed embedding (backend was down)."""
    from cache.models import CacheRecord
    from cache.search import set_embedding
    from embeddings.service import embed
    from tasks.graph import index_task
    from tasks.models import Task

    n = 0
    recs = list(
        CacheRecord.objects.filter(has_embedding=False, deleted=False)
        .exclude(body_text="")[:limit]
    )
    if recs:
        vecs = embed([f"{r.title}\n{r.body_text}" for r in recs])
        for r, v in zip(recs, vecs):
            set_embedding(r.id, v)
            n += 1
    for t in Task.objects.filter(has_embedding=False)[:limit]:
        index_task(t)
        n += 1
    return n


def build_scheduler():
    """Wire jobs onto a BlockingScheduler. Import-light so tests can skip it."""
    from apscheduler.schedulers.blocking import BlockingScheduler

    from connectors.models import AppSettings

    sched = BlockingScheduler(timezone=str(timezone.get_current_timezone()))
    settings_row = AppSettings.load()

    for src in registry.enabled():
        interval = settings_row.sync_interval(src.provider_key)
        sched.add_job(
            sync_source, "interval", seconds=interval, args=[src.key],
            id=f"sync:{src.key}", max_instances=1, coalesce=True,
        )
    sched.add_job(poll_all, "interval", seconds=300, id="poll_all", max_instances=1, coalesce=True)
    sched.add_job(backfill_embeddings, "interval", seconds=600, id="backfill_embeddings", max_instances=1)
    sched.add_job(renew_watch_channels, "interval", hours=1, id="renew_watch_channels", max_instances=1)

    return sched
