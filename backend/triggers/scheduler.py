"""Schedule + cron trigger dispatch, wired onto the worker's APScheduler (#34)."""

from datetime import timedelta

from django.utils import timezone

from .models import DeliveryLog, Trigger

# Cron triggers are dispatched by a single 5-minute tick that re-reads the
# enabled cron triggers from the DB each time, so enable/disable via
# update_trigger takes effect on the next tick without a worker restart (#49).
# Crons with sub-5-minute periods fire at most once per tick.
CRON_TICK_S = 300


def _payload_for(trg: Trigger) -> dict:
    """Digest-kind cron triggers get the live digest payload; others get their spec payload."""
    spec = trg.spec or {}
    if spec.get("digest") or (spec.get("payload") or {}).get("kind") == "digest":
        from .digest import build_digest

        return build_digest()
    return spec.get("payload", {})


def tick_crons():
    """Every CRON_TICK_S: fire each enabled cron trigger whose expression came due
    in the last tick window. The entity id pins the exact scheduled minute, so
    overlapping ticks cannot double-fire.
    """
    now = timezone.now()
    for trg in Trigger.objects.filter(kind=Trigger.KIND_CRON, enabled=True):
        expr = (trg.spec or {}).get("cron")
        if not expr:
            continue
        due = None
        try:
            from apscheduler.triggers.cron import CronTrigger

            due = CronTrigger.from_crontab(expr).get_next_fire_time(
                None, now - timedelta(seconds=CRON_TICK_S + 1))
        except Exception:
            continue
        if not due or due > now:
            continue
        entity_id = f"cron:{trg.key}:{due.isoformat()}"
        if DeliveryLog.objects.filter(trigger_key=trg.key, entity_id=entity_id, ok=True).exists():
            continue
        from .delivery import fire

        fire(trg, {"id": entity_id, "type": "cron", "title": trg.key},
             payload=_payload_for(trg))


def plan_schedules():
    """Hourly: fire any `schedule` trigger whose anchor+offset landed in the last hour.

    spec = {anchor: "task.due_at"|"task.remind_at"|"record.occurred_at", filter: {...}, offset_s: int}
    """
    from cache.models import CacheRecord
    from tasks.models import Task

    from .delivery import fire

    now = timezone.now()
    window_start = now - timedelta(hours=1)

    for trg in Trigger.objects.filter(kind=Trigger.KIND_SCHEDULE, enabled=True):
        spec = trg.spec or {}
        anchor = spec.get("anchor", "")
        offset = timedelta(seconds=int(spec.get("offset_s", 0)))
        flt = spec.get("filter", {})

        if anchor == "task.due_at":
            qs = Task.objects.filter(completed=False, due_at__isnull=False, **flt)
            items = [(f"task:{t.id}", t.due_at, t.title) for t in qs]
        elif anchor == "task.remind_at":
            qs = Task.objects.filter(completed=False, remind_at__isnull=False, **flt)
            items = [(f"task:{t.id}", t.remind_at, t.title) for t in qs]
        elif anchor == "record.occurred_at":
            qs = CacheRecord.objects.filter(deleted=False, occurred_at__isnull=False, **flt)
            items = [(r.id, r.occurred_at, r.title) for r in qs]
        else:
            continue

        for entity_id, anchor_dt, title in items:
            fire_at = anchor_dt + offset
            if not (window_start <= fire_at <= now):
                continue
            if DeliveryLog.objects.filter(trigger_key=trg.key, entity_id=entity_id, ok=True).exists():
                continue
            fire(trg, {"id": entity_id, "type": "schedule", "title": title},
                 payload={"anchor_at": anchor_dt.isoformat(), "fire_at": fire_at.isoformat()})


def register_trigger_jobs(sched):
    """Called by sources.scheduler.build_scheduler()."""
    sched.add_job(plan_schedules, "interval", seconds=3600, id="plan_schedules",
                  max_instances=1, coalesce=True)
    sched.add_job(tick_crons, "interval", seconds=CRON_TICK_S, id="tick_crons",
                  max_instances=1, coalesce=True)
