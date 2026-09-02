"""Tool definitions + execution shared between the chat endpoint and the MCP server."""

from datetime import timedelta

from django.utils import timezone

from connectors import demo_seed
from connectors.clients import GoogleClient, UpBankClient
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
            "name": "list_calendar_events",
            "description": "List upcoming Google Calendar events, e.g. to draft meeting-prep tasks. "
            "Requires Google to be connected.",
            "parameters": {
                "type": "object",
                "properties": {
                    "days_ahead": {"type": "integer", "description": "How many days ahead to look (default 7)"},
                    "max_results": {"type": "integer"},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "list_recent_transactions",
            "description": "List recent settled Up Bank transactions, e.g. to draft finance follow-up tasks "
            "(pay a bill, dispute a charge, review a subscription). Requires Up Bank to be connected.",
            "parameters": {
                "type": "object",
                "properties": {
                    "days": {"type": "integer", "description": "How many days back to look (default 7)"},
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "search_emails",
            "description": "Search Gmail (subject/sender/date/snippet only). Use Gmail search syntax, "
            "e.g. 'from:sam picnic' or 'newer_than:14d receipt'. Requires Google to be connected.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "max_results": {"type": "integer"},
                },
                "required": ["query"],
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


def list_calendar_events(days_ahead=7, max_results=20) -> list[dict] | dict:
    connector = Connector.objects.filter(kind=Connector.Kind.GOOGLE, enabled=True).first()
    if not connector:
        return {"error": "Google is not connected. Ask the user to connect it in Settings."}
    if connector.config.get("demo"):
        return demo_seed.build_demo_calendar_events(days_ahead, max_results)
    client = GoogleClient(connector.credentials)
    now = timezone.now()
    events = client.calendar_events(
        max_results=max_results,
        timeMin=now.isoformat(),
        timeMax=(now + timedelta(days=days_ahead)).isoformat(),
    )
    connector.credentials = client.refreshed_credentials_dict()
    connector.save()
    return [
        {
            "summary": e.get("summary"),
            "start": e.get("start"),
            "end": e.get("end"),
            "attendees": [a.get("email") for a in e.get("attendees", [])],
        }
        for e in events
    ]


def list_recent_transactions(days=7) -> list[dict] | dict:
    connector = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
    if not connector:
        return {"error": "Up Bank is not connected. Ask the user to connect it in Settings."}
    if connector.config.get("demo"):
        return demo_seed.build_demo_transactions(days)
    since = (timezone.now() - timedelta(days=days)).isoformat()
    data = UpBankClient(connector.credentials).transactions({"filter[since]": since, "page[size]": 50})
    return [
        {
            "description": t["attributes"]["description"],
            "amount": t["attributes"]["amount"]["value"],
            "status": t["attributes"]["status"],
            "created_at": t["attributes"]["createdAt"],
        }
        for t in data.get("data", [])
    ]


def search_emails(query, max_results=5) -> list[dict] | dict:
    connector = Connector.objects.filter(kind=Connector.Kind.GOOGLE, enabled=True).first()
    if not connector:
        return {"error": "Google is not connected. Ask the user to connect it in Settings."}
    if connector.config.get("demo"):
        return demo_seed.build_demo_gmail_search(query, max_results)
    client = GoogleClient(connector.credentials)
    results = client.gmail_search(query, max_results=max_results)
    connector.credentials = client.refreshed_credentials_dict()
    connector.save()
    return results


TOOL_IMPLS = {
    "list_tasks": list_tasks,
    "create_task": create_task,
    "update_task": update_task,
    "list_calendar_events": list_calendar_events,
    "list_recent_transactions": list_recent_transactions,
    "search_emails": search_emails,
}
