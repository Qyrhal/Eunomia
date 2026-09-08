"""Tool definitions + execution shared between the chat endpoint and the MCP server."""

from datetime import timedelta

from django.utils import timezone

from connectors import demo_seed
from connectors.clients import GoogleClient, UpBankClient
from connectors.models import Connector

TOOL_SCHEMAS = [
    {
        "type": "function",
        "function": {
            "name": "list_calendar_events",
            "description": "List upcoming Google Calendar events. Requires Google to be connected.",
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
            "description": "List recent settled Up Bank transactions, e.g. to spot a bill, a charge, or a subscription. Requires Up Bank to be connected.",
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
    "list_calendar_events": list_calendar_events,
    "list_recent_transactions": list_recent_transactions,
    "search_emails": search_emails,
}
