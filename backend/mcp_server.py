"""Standalone MCP server exposing Eunomia's tasks/calendar tools over stdio.

Run with: uv run mcp_server.py
Point Claude Desktop / another MCP client at this command; it shares the same
sqlite database as the Django app (via DJANGO_SETTINGS_MODULE=config.settings).
"""

import os

import django

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "config.settings")
django.setup()

from mcp.server.mcpserver import MCPServer  # noqa: E402

from aiassist import tools as t  # noqa: E402

mcp = MCPServer("eunomia")


@mcp.tool()
def list_tasks(completed: bool | None = None, flagged: bool | None = None, project_name: str | None = None) -> list[dict]:
    """List tasks/reminders, optionally filtered by completion, flagged state, or project name."""
    return t.list_tasks(completed=completed, flagged=flagged, project_name=project_name)


@mcp.tool()
def create_task(
    title: str,
    notes: str = "",
    project_name: str | None = None,
    due_at: str | None = None,
    priority: int = 0,
    allocated_minutes: int | None = None,
    flagged: bool = False,
) -> dict:
    """Create a new task/reminder."""
    return t.create_task(
        title=title,
        notes=notes,
        project_name=project_name,
        due_at=due_at,
        priority=priority,
        allocated_minutes=allocated_minutes,
        flagged=flagged,
    )


@mcp.tool()
def update_task(task_id: str, **fields) -> dict:
    """Update or complete an existing task by id."""
    return t.update_task(task_id, **fields)


@mcp.tool()
def list_calendar_events(days_ahead: int = 7, max_results: int = 20) -> list[dict] | dict:
    """List upcoming Google Calendar events, e.g. to draft meeting-prep tasks."""
    return t.list_calendar_events(days_ahead=days_ahead, max_results=max_results)


@mcp.tool()
def list_recent_transactions(days: int = 7) -> list[dict] | dict:
    """List recent settled Up Bank transactions, e.g. to draft finance follow-up tasks."""
    return t.list_recent_transactions(days=days)


@mcp.tool()
def search_emails(query: str, max_results: int = 5) -> list[dict] | dict:
    """Search Gmail (subject/sender/date/snippet only) using Gmail search syntax."""
    return t.search_emails(query, max_results=max_results)


if __name__ == "__main__":
    mcp.run()
