"""heypocket source -- meeting recordings from heypocketai.com.

Wraps connectors.clients.PocketAIClient. Poll + on-demand only (the public API
has no webhooks). `provider = "pocketai"` matches the Connector kind.
"""

from datetime import datetime, timedelta, timezone

from connectors.clients import PocketAIClient
from sources.base import Source, SyncResult, ToolSpec
from sources.registry import connector_for, credentials_for


def _parse_dt(value: str | None):
    """cache_record.occurred_at is a SurrealDB `option<datetime>` field -- the
    driver only coerces real `datetime` objects, not ISO strings."""
    if not value:
        return None
    return datetime.fromisoformat(value)


class HeyPocketSource(Source):
    key = "heypocket"
    provider = "pocketai"
    label = "heypocket"
    record_types = ["heypocket.recording"]
    auth_kind = "api_key"

    async def _client(self, owner) -> PocketAIClient:
        conn = await connector_for(owner, self)
        base = (conn.get("config") or {}).get("base_url") if conn else None
        return PocketAIClient(await credentials_for(owner, self), base)

    async def sync(self, owner, mode, cursor=None) -> SyncResult:
        start = cursor or (datetime.now(timezone.utc) - timedelta(days=30)).date().isoformat()
        client = await self._client(owner)
        data = (await client.recordings({"start_date": start, "limit": 200})).get("data", [])
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
        body = " ".join(x for x in (raw.get("summary"), raw.get("transcript"), raw.get("notes")) if x) or raw.get(
            "title", ""
        )
        return {
            "id": f"heypocket:heypocket.recording:{rid}",
            "source": "heypocket",
            "type": "heypocket.recording",
            "external_id": str(rid),
            "title": raw.get("title", ""),
            "body_text": body,
            "occurred_at": _parse_dt(raw.get("recording_at") or raw.get("created_at")),
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
            ToolSpec(
                name="search_recordings",
                schema={"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]},
                impl=search_recordings,
            ),
        ]


def _iso(dt) -> str | None:
    if dt is None:
        return None
    return dt.isoformat() if hasattr(dt, "isoformat") else dt


async def summary(owner, days: int = 30) -> dict:
    """Recording count/duration/tags since `days` ago, computed from the cached
    heypocket records (populated by the periodic sync) rather than a live API
    call. Every field comes straight off a recording (`duration`, `tags`) --
    heypocket's API has no dedicated action-items/todos field, so this doesn't
    invent one."""
    from cache import search as cs

    since = (datetime.now(timezone.utc) - timedelta(days=days)).isoformat()
    recs = [
        r
        for r in await cs.list_records(owner, type="heypocket.recording", limit=2000)
        if _iso(r.occurred_at) and _iso(r.occurred_at) >= since
    ]

    tag_counts: dict[str, int] = {}
    for r in recs:
        for tag in (r.payload or {}).get("tags", []):
            tag_counts[tag] = tag_counts.get(tag, 0) + 1

    recs.sort(key=lambda r: _iso(r.occurred_at) or "", reverse=True)

    return {
        "recordings_count": len(recs),
        "total_duration_minutes": round(sum((r.payload or {}).get("duration_seconds", 0) for r in recs) / 60, 1),
        "tag_breakdown": sorted(
            [{"tag": k, "count": v} for k, v in tag_counts.items()], key=lambda row: -row["count"]
        ),
        "recent_recordings": [
            {
                "title": r.title,
                "duration_minutes": round((r.payload or {}).get("duration_seconds", 0) / 60, 1),
                "recorded_at": _iso(r.occurred_at),
                "tags": (r.payload or {}).get("tags", []),
            }
            for r in recs[:10]
        ],
    }


async def list_recordings(owner, days: int = 30, tag: str | None = None, limit: int = 50) -> list[dict]:
    """Cached recordings, optionally filtered by tag -- for surfacing "which
    meeting was that in" without a live API call."""
    from cache import search as cs

    since = (datetime.now(timezone.utc) - timedelta(days=days)).isoformat()
    recs = [
        r
        for r in await cs.list_records(owner, type="heypocket.recording", limit=2000)
        if _iso(r.occurred_at) and _iso(r.occurred_at) >= since
    ]
    recs.sort(key=lambda r: _iso(r.occurred_at) or "", reverse=True)
    if tag:
        recs = [r for r in recs if tag in (r.payload or {}).get("tags", [])]

    return [
        {
            "title": r.title,
            "duration_minutes": round((r.payload or {}).get("duration_seconds", 0) / 60, 1),
            "recorded_at": _iso(r.occurred_at),
            "tags": (r.payload or {}).get("tags", []),
            "url": r.url or None,
        }
        for r in recs[: min(int(limit), 200)]
    ]


async def search_recordings(owner, query: str) -> dict:
    """Hybrid search over cached heypocket recordings."""
    from cache import search as cs

    rows = await cs.search(owner, query, types=["heypocket.recording"], limit=20)
    return {
        "results": [
            {
                "title": r.title,
                "recorded_at": _iso(r.occurred_at),
                "tags": (r.payload or {}).get("tags", []),
                "url": r.url or None,
            }
            for r in rows
        ]
    }


SOURCE = HeyPocketSource()
