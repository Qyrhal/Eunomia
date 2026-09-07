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
                name="summary",
                schema={"type": "object", "properties": {"days": {"type": "integer", "description": "default 30"}}},
                impl=summary,
            ),
            ToolSpec(
                name="list_recordings",
                schema={
                    "type": "object",
                    "properties": {
                        "days": {"type": "integer", "description": "default 30"},
                        "tag": {"type": "string"},
                        "limit": {"type": "integer"},
                    },
                },
                impl=list_recordings,
            ),
        ]


def summary(days: int = 30) -> dict:
    """Recording count/duration/tags since `days` ago, computed from the cached
    heypocket records (populated by the periodic sync) rather than a live API
    call. Every field comes straight off a recording (`duration`, `tags`) —
    heypocket's API has no dedicated action-items/todos field, so this doesn't
    invent one."""
    from datetime import timedelta

    from django.utils import timezone

    from cache.models import CacheRecord

    since = timezone.now() - timedelta(days=days)
    recs = CacheRecord.objects.filter(type="heypocket.recording", deleted=False, occurred_at__gte=since)

    tag_counts: dict[str, int] = {}
    for r in recs:
        for tag in (r.payload or {}).get("tags", []):
            tag_counts[tag] = tag_counts.get(tag, 0) + 1

    return {
        "recordings_count": recs.count(),
        "total_duration_minutes": round(
            sum((r.payload or {}).get("duration_seconds", 0) for r in recs) / 60, 1
        ),
        "tag_breakdown": sorted(
            [{"tag": k, "count": v} for k, v in tag_counts.items()], key=lambda row: -row["count"]
        ),
        "recent_recordings": [
            {
                "title": r.title,
                "duration_minutes": round((r.payload or {}).get("duration_seconds", 0) / 60, 1),
                "recorded_at": r.occurred_at.isoformat() if r.occurred_at else None,
                "tags": (r.payload or {}).get("tags", []),
            }
            for r in recs.order_by("-occurred_at")[:10]
        ],
    }


def list_recordings(days: int = 30, tag: str | None = None, limit: int = 50) -> list[dict]:
    """Cached recordings, optionally filtered by tag — for surfacing "which
    meeting was that in" without a live API call."""
    from datetime import timedelta

    from django.utils import timezone

    from cache.models import CacheRecord

    since = timezone.now() - timedelta(days=days)
    recs = CacheRecord.objects.filter(
        type="heypocket.recording", deleted=False, occurred_at__gte=since
    ).order_by("-occurred_at")
    if tag:
        # filtered in Python, not the DB — JSONField array-containment lookups
        # aren't reliably supported on SQLite.
        recs = [r for r in recs if tag in (r.payload or {}).get("tags", [])]

    return [
        {
            "title": r.title,
            "duration_minutes": round((r.payload or {}).get("duration_seconds", 0) / 60, 1),
            "recorded_at": r.occurred_at.isoformat() if r.occurred_at else None,
            "tags": (r.payload or {}).get("tags", []),
            "url": r.url or None,
        }
        for r in list(recs)[: min(int(limit), 200)]
    ]
