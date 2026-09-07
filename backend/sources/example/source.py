"""Reference source. Copy this folder, rename, and fill in the four parts:
`sync` (fetch), `map` (raw -> envelope), `tools`.
"""

from datetime import datetime, timezone

from sources.base import Source, SyncResult, ToolSpec


class ExampleSource(Source):
    key = "example"
    label = "Example (reference stub)"
    record_types = ["example.note"]
    auth_kind = "token"

    def sync(self, mode, cursor=None) -> SyncResult:
        # A real source hits its API here, using `cursor` for delta pulls.
        return SyncResult(records=[], cursor=cursor)

    def map(self, raw: dict) -> dict | None:
        if not raw.get("id"):
            return None
        return {
            "id": f"{self.key}:example.note:{raw['id']}",
            "source": self.key,
            "type": "example.note",
            "external_id": str(raw["id"]),
            "title": raw.get("title", ""),
            "body_text": raw.get("text", ""),
            "occurred_at": raw.get("created_at") or datetime.now(timezone.utc),
            "url": raw.get("url", ""),
            "payload": {"raw_keys": sorted(raw.keys())},
            "links": [],
            "deleted": raw.get("deleted", False),
        }

    def tools(self) -> list[ToolSpec]:
        return [
            ToolSpec(
                name="ping",
                schema={"type": "object", "properties": {}},
                impl=lambda: {"ok": True, "source": self.key},
            )
        ]
