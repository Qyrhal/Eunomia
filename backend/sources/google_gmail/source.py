"""Gmail source (#11) — message metadata + snippet into the cache. No bodies."""

from datetime import timedelta

from django.utils import timezone

from sources._google import google_client, persist_refreshed_token
from sources.base import Source, SyncResult


class GmailSource(Source):
    key = "google_gmail"
    provider = "google"
    label = "Gmail"
    record_types = ["gmail.message"]
    auth_kind = "oauth"
    secret_fields = ["token", "refresh_token", "client_secret"]

    def sync(self, mode, cursor=None) -> SyncResult:
        client = google_client(self)
        since = cursor or (timezone.now() - timedelta(days=30)).strftime("%Y/%m/%d")
        msgs = client.gmail_list_detailed(f"after:{since}", max_results=200)
        persist_refreshed_token(self, client)
        newest = max((m.get("internalDate") or "0" for m in msgs), default="0")
        # Gmail search granularity is a day; keep the day cursor, dedupe on id.
        next_cursor = timezone.now().strftime("%Y/%m/%d") if msgs else cursor or since
        return SyncResult(records=msgs, cursor=next_cursor)

    def map(self, raw: dict) -> dict | None:
        if not raw.get("id"):
            return None
        ts = raw.get("internalDate")
        occurred = None
        if ts:
            from datetime import datetime, timezone as dt_tz

            occurred = datetime.fromtimestamp(int(ts) / 1000, dt_tz.utc).isoformat()
        return {
            "id": f"google_gmail:gmail.message:{raw['id']}",
            "source": "google_gmail",
            "type": "gmail.message",
            "external_id": raw["id"],
            "title": raw.get("subject", "(no subject)"),
            "body_text": " — ".join(x for x in (raw.get("from"), raw.get("subject"), raw.get("snippet")) if x),
            "occurred_at": occurred or raw.get("date"),
            "url": f"https://mail.google.com/mail/u/0/#inbox/{raw.get('threadId') or raw['id']}",
            "payload": {
                "from": raw.get("from", ""),
                "to": raw.get("to", ""),
                "labels": raw.get("labelIds", []),
                "unread": "UNREAD" in raw.get("labelIds", []),
                "thread_id": raw.get("threadId"),
            },
            "links": [],
            "deleted": False,
        }
