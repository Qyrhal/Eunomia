"""Daily digest payload builder (#49).

Attached automatically to cron triggers whose spec asks for a digest
(`spec.digest == true` or `spec.payload.kind == "digest"`, e.g. the
`daily_digest` builtin). Hermes relays this payload to the user in the
morning digest.
"""

from datetime import timedelta

from django.utils import timezone

# Cap the item lists so a runaway backlog can't produce a huge webhook body.
_LIST_LIMIT = 50


def _task_brief(t, project_name: str) -> dict:
    return {
        "id": f"task:{t.id}",
        "title": t.title,
        "project": project_name,
        "priority": t.priority,
        "flagged": t.flagged,
        "due_at": t.due_at.isoformat() if t.due_at else None,
        "remind_at": t.remind_at.isoformat() if t.remind_at else None,
    }


def build_digest() -> dict:
    """Open tasks by project, overdue count, today's due + reminders."""
    from tasks.models import Project, Task

    now = timezone.now()
    local_now = timezone.localtime(now)
    day_start = local_now.replace(hour=0, minute=0, second=0, microsecond=0)
    day_end = day_start + timedelta(days=1)

    open_qs = Task.objects.filter(completed=False)
    due_today = list(
        open_qs.filter(due_at__gte=day_start, due_at__lt=day_end).order_by("due_at"))
    reminders_today = list(
        open_qs.filter(remind_at__gte=day_start, remind_at__lt=day_end).order_by("remind_at"))
    overdue_count = open_qs.filter(due_at__lt=now).count()

    project_names = {p.id: p.name for p in Project.objects.all()}
    by_project: dict[str, dict] = {}
    for t in open_qs.only("project_id", "due_at"):
        name = project_names.get(t.project_id, str(t.project_id))
        bucket = by_project.setdefault(name, {"open_count": 0, "overdue_count": 0})
        bucket["open_count"] += 1
        if t.due_at and t.due_at < now:
            bucket["overdue_count"] += 1

    def _briefs(items):
        return [_task_brief(t, project_names.get(t.project_id, str(t.project_id)))
                for t in items[:_LIST_LIMIT]]

    return {
        "kind": "digest",
        "generated_at": now.isoformat(),
        "open_total": open_qs.count(),
        "overdue_count": overdue_count,
        "due_today": _briefs(due_today),
        "reminders_today": _briefs(reminders_today),
        "by_project": [{"project": name, **counts} for name, counts in sorted(by_project.items())],
    }
