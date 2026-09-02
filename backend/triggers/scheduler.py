"""Schedule + cron trigger dispatch, wired onto the worker's APScheduler (#34)."""

from datetime import timedelta

from django.utils import timezone

from .models import DeliveryLog, Trigger


def run_cron(trigger_key: str):
    trg = Trigger.objects.filter(pk=trigger_key, enabled=True).first()
    if not trg:
        return
    from .delivery import fire

    fire(trg, {"id": f"cron:{trg.key}:{timezone.now().date()}", "type": "cron"},
         payload=trg.spec.get("payload", {}))


def plan_schedules():
    """Hourly: fire any `schedule` trigger whose anchor+offset landed in the last hour.

    spec = {anchor: "task.due_at"|"record.occurred_at", filter: {...}, offset_s: int}
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
    sched.add_job(plan_schedules, "interval", seconds=3600, id="plan_schedules", max_instances=1)
    for trg in Trigger.objects.filter(kind=Trigger.KIND_CRON, enabled=True):
        cron = (trg.spec or {}).get("cron")
        if not cron:
            continue
        try:
            from apscheduler.triggers.cron import CronTrigger

            sched.add_job(run_cron, CronTrigger.from_crontab(cron), args=[trg.key],
                          id=f"cron:{trg.key}", max_instances=1)
        except Exception:
            continue
