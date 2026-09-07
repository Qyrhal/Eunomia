"""Tool definitions + execution shared between the chat endpoint and the MCP server."""

from django.utils import timezone

from connectors import demo_seed
from connectors.models import Connector
from tasks.models import Project, Task

TOOL_SCHEMAS = [
    {
        "type": "function",
        "function": {
            "name": "list_tasks",
            "description": "List tasks/reminders, optionally filtered.",
            "parameters": {
                "type": "object",
                "properties": {
                    "completed": {"type": "boolean"},
                    "flagged": {"type": "boolean"},
                    "project_name": {"type": "string"},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "create_task",
            "description": "Create a new task/reminder.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "notes": {"type": "string"},
                    "project_name": {"type": "string", "description": "Existing project name; created if missing"},
                    "due_at": {"type": "string", "description": "ISO 8601 datetime"},
                    "priority": {"type": "integer", "description": "0=none,1=low,2=medium,3=high"},
                    "allocated_minutes": {"type": "integer"},
                    "flagged": {"type": "boolean"},
                },
                "required": ["title"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "update_task",
            "description": "Update or complete an existing task by id.",
            "parameters": {
                "type": "object",
                "properties": {
                    "task_id": {"type": "string"},
                    "title": {"type": "string"},
                    "notes": {"type": "string"},
                    "due_at": {"type": "string"},
                    "priority": {"type": "integer"},
                    "flagged": {"type": "boolean"},
                    "completed": {"type": "boolean"},
                },
                "required": ["task_id"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "list_recent_transactions",
            "description": "List recent settled Up Bank transactions, e.g. to draft finance follow-up tasks "
            "(pay a bill, dispute a charge, review a subscription). Requires Up Bank to be connected. "
            "Reads from the cache populated by the periodic sync, not a live API call.",
            "parameters": {
                "type": "object",
                "properties": {
                    "days": {"type": "integer", "description": "How many days back to look (default 7)"},
                },
            },
        },
    },
]


def _get_or_create_project(name: str | None) -> Project:
    name = name or "Inbox"
    obj, _ = Project.objects.get_or_create(name=name)
    return obj


def list_tasks(completed=None, flagged=None, project_name=None) -> list[dict]:
    qs = Task.objects.all()
    if completed is not None:
        qs = qs.filter(completed=completed)
    if flagged is not None:
        qs = qs.filter(flagged=flagged)
    if project_name:
        qs = qs.filter(project__name=project_name)
    return [
        {
            "id": str(t.id),
            "title": t.title,
            "due_at": t.due_at.isoformat() if t.due_at else None,
            "priority": t.priority,
            "flagged": t.flagged,
            "completed": t.completed,
            "project": t.project.name,
        }
        for t in qs[:50]
    ]


def create_task(title, notes="", project_name=None, due_at=None, priority=0, allocated_minutes=None, flagged=False) -> dict:
    task = Task.objects.create(
        project=_get_or_create_project(project_name),
        title=title,
        notes=notes or "",
        due_at=due_at or None,
        priority=priority or 0,
        allocated_minutes=allocated_minutes,
        flagged=bool(flagged),
        created_by_ai=True,
    )
    return {"id": str(task.id), "title": task.title, "created": True}


def update_task(task_id, **fields) -> dict:
    task = Task.objects.filter(id=task_id).first()
    if not task:
        return {"error": f"no task with id {task_id}"}
    for key in ("title", "notes", "due_at", "priority", "flagged", "completed"):
        if key in fields and fields[key] is not None:
            setattr(task, key, fields[key])
    if fields.get("completed"):
        task.completed_at = timezone.now()
    task.save()
    return {"id": str(task.id), "updated": True}


def list_recent_transactions(days=7) -> list[dict] | dict:
    connector = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
    if not connector:
        return {"error": "Up Bank is not connected. Ask the user to connect it in Settings."}
    if connector.config.get("demo"):
        return demo_seed.build_demo_transactions(days)
    from sources.up_bank.source import list_transactions

    return list_transactions(days=days)


TOOL_IMPLS = {
    "list_tasks": list_tasks,
    "create_task": create_task,
    "update_task": update_task,
    "list_recent_transactions": list_recent_transactions,
}
