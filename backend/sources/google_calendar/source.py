"""Google Calendar source (#11) — upcoming + recent events into the cache."""

from datetime import timedelta

from django.utils import timezone

from sources._google import google_client, google_push_webhook, persist_refreshed_token
from sources.base import Source, SyncResult


class GoogleCalendarSource(Source):
    key = "google_calendar"
    provider = "google"
    label = "Google Calendar"
    record_types = ["gcal.event"]
    auth_kind = "oauth"

    def sync(self, mode, cursor=None) -> SyncResult:
        client = google_client(self)
        now = timezone.now()
        time_min = cursor or (now - timedelta(days=7)).isoformat()
        events = client.calendar_events(
            max_results=250, timeMin=time_min, timeMax=(now + timedelta(days=60)).isoformat(),
        )
        persist_refreshed_token(self, client)
        return SyncResult(records=events, cursor=now.isoformat())

    def map(self, raw: dict) -> dict | None:
        if not raw.get("id"):
            return None
        start = (raw.get("start") or {}).get("dateTime") or (raw.get("start") or {}).get("date")
        attendees = [a.get("email") for a in raw.get("attendees", []) if a.get("email")]
        return {
            "id": f"google_calendar:gcal.event:{raw['id']}",
            "source": "google_calendar",
            "type": "gcal.event",
            "external_id": raw["id"],
            "title": raw.get("summary", "(untitled event)"),
            "body_text": " — ".join(
                x for x in (raw.get("summary"), raw.get("location"), raw.get("description")) if x
            ),
            "occurred_at": start,
            "url": raw.get("htmlLink", ""),
            "payload": {
                "location": raw.get("location", ""),
                "attendees": attendees,
                "organizer": (raw.get("organizer") or {}).get("email", ""),
                "status": raw.get("status"),
                "all_day": bool((raw.get("start") or {}).get("date")),
            },
            "links": [],
            "deleted": raw.get("status") == "cancelled",
        }

    def webhook(self, request):
        return google_push_webhook(self, request)
