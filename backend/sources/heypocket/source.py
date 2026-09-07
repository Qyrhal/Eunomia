"""heypocket source (#13) — meeting recordings from heypocketai.com.

Wraps connectors.clients.PocketAIClient. Poll + on-demand only (the public API
has no webhooks). `provider = "pocketai"` matches the existing Connector kind.
"""

from datetime import timedelta

from django.utils import timezone

from connectors.clients import PocketAIClient
from sources.base import Source, SyncResult, ToolSpec
from sources.registry import connector_for, credentials_for


class HeyPocketSource(Source):
    key = "heypocket"
    provider = "pocketai"
    label = "heypocket"
    record_types = ["heypocket.recording"]
    auth_kind = "api_key"

    def _client(self) -> PocketAIClient:
        conn = connector_for(self)
        base = (conn.config or {}).get("base_url") if conn else None
        return PocketAIClient(credentials_for(self), base)

    def sync(self, mode, cursor=None) -> SyncResult:
        start = cursor or (timezone.now() - timedelta(days=30)).date().isoformat()
        data = self._client().recordings({"start_date": start, "limit": 200}).get("data", [])
        newest = max(
            (r.get("recording_at") or r.get("created_at") or "" for r in data),
            default=cursor or start,
        )
        return SyncResult(records=data, cursor=newest or start)

    def map(self, raw: dict) -> dict | None:
        rid = raw.get("id") or raw.get("recording_id")
        if not rid:
            return None
        tags = [t.get("name") for t in raw.get("tags", []) if isinstance(t, dict)]
        body = " ".join(
            x for x in (raw.get("summary"), raw.get("transcript"), raw.get("notes")) if x
        ) or raw.get("title", "")
        return {
            "id": f"heypocket:heypocket.recording:{rid}",
            "source": "heypocket",
            "type": "heypocket.recording",
            "external_id": str(rid),
            "title": raw.get("title", ""),
            "body_text": body,
            "occurred_at": raw.get("recording_at") or raw.get("created_at"),
            "url": raw.get("url") or raw.get("share_url", ""),
            "payload": {
                "duration_seconds": raw.get("duration", 0),
                "tags": tags,
            },
            "links": [],
            "deleted": bool(raw.get("deleted")),
        }

    def tools(self) -> list[ToolSpec]:
        return [
            ToolSpec(
                name="search_recordings",
                schema={"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]},
                impl=lambda query: self._client().search(query),
            ),
        ]
