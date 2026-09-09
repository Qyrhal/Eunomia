"""Twenty CRM source for Eunomia.

Sync: walks each resource type (people, companies, deals, tasks, notes)
     via the REST list endpoints, using a cursor for delta pulls.
Map:  each raw resource -> a cache envelope keyed by `twenty.<type>`.
Tools: ping, summary, and per-type list/read helpers surfaced as MCP tools.
"""

from datetime import datetime, timezone

from connectors.clients import TwentyClient
from sources.base import Source, SyncResult, ToolSpec
from sources.registry import credentials_for

# resource types Eunomia knows how to ingest from Twenty
RESOURCE_TYPES = [
    ("people", "twenty.person"),
    ("companies", "twenty.company"),
    ("deals", "twenty.deal"),
    ("tasks", "twenty.task"),
    ("notes", "twenty.note"),
]


class TwentyCMSSource(Source):
    key = "twenty_crm"
    provider = "twenty"
    label = "Twenty CRM"
    record_types = [t for _, t in RESOURCE_TYPES]
    auth_kind = "token"
    secret_fields = ["api_token"]

    # -- client -----------------------------------------------------------------

    def _client(self) -> TwentyClient:
        return TwentyClient(credentials_for(self), base_url=credentials_for(self).get("base_url"))

    # -- sync ------------------------------------------------------------------

    def sync(self, mode: str, cursor: str | None = None) -> SyncResult:
        client = self._client()
        records: list[dict] = []

        for rest_name, cache_type in RESOURCE_TYPES:
            try:
                data = self._list_all(client, rest_name)
                for raw in (data.get("data") or data.get("items") or []):
                    mapped = self.map(raw, cache_type)
                    if mapped:
                        records.append(mapped)
            except Exception:
                # one failing resource type must not kill the whole sync
                pass

        # cursor = newest updated_at across all fetched records
        newest = cursor
        for r in records:
            ts = r.get("occurred_at")
            if ts and (newest is None or ts > newest):
                newest = ts
        return SyncResult(records=records, cursor=newest)

    def _list_all(self, client: TwentyClient, rest_name: str, max_pages: int = 50) -> dict:
        """Paginate through a Twenty list endpoint, returning the merged result."""
        # first page
        page = getattr(client, rest_name)(limit=100)
        merged = page
        data_list = page.get("data") or page.get("items") or []
        for _ in range(max_pages - 1):
            # Twenty uses cursor-based pagination; check for next link
            links = page.get("links") or {}
            nxt = links.get("next") or page.get("meta", {}).get("nextCursor")
            if not nxt:
                break
            # follow the next link (absolute URL) or re-query with cursor
            if nxt.startswith("http"):
                import httpx

                page = httpx.get(nxt, headers=client._headers(), timeout=15).json()
            else:
                page = getattr(client, rest_name)(limit=100, cursor=nxt)
            data_list.extend(page.get("data") or page.get("items") or [])
            if not page.get("links", {}).get("next") and not page.get("meta", {}).get("nextCursor"):
                break
        # re-attach the data list to the original envelope shape
        if "data" in merged:
            merged["data"] = data_list
        elif "items" in merged:
            merged["items"] = data_list
        return merged

    # -- map -----------------------------------------------------------------

    def map(self, raw: dict, cache_type: str | None = None) -> dict | None:
        if not raw.get("id"):
            return None

        if cache_type is None:
            # derive the cache type from the resource's REST name (inferred from url)
            url = raw.get("urls", {}).get("self") or raw.get("url") or ""
            for rest_name, ct in RESOURCE_TYPES:
                if f"/{rest_name}/" in url or url.endswith(f"/{rest_name}/"):
                    cache_type = ct
                    break
            if cache_type is None:
                cache_type = "twenty.unknown"

        attrs = raw.get("attributes", {}) or {}
        title = attrs.get("name") or attrs.get("firstName", "") or attrs.get("companyName", "") or ""

        # common updated/created fields Twenty may expose
        updated_at = attrs.get("updatedAt") or attrs.get("updated_at") or raw.get("updatedAt") or raw.get("updated_at")
        created_at = attrs.get("createdAt") or attrs.get("created_at") or raw.get("createdAt") or raw.get("created_at")

        body_parts: list[str] = []
        if attrs.get("email"):
            body_parts.append(f"Email: {attrs['email']}")
        if attrs.get("phone"):
            body_parts.append(f"Phone: {attrs['phone']}")
        if attrs.get("description"):
            body_parts.append(attrs["description"])
        if attrs.get("note") or attrs.get("content"):
            body_parts.append(attrs.get("note") or attrs.get("content"))
        if attrs.get("stage") or attrs.get("status"):
            body_parts.append(f"Status: {attrs.get('stage') or attrs.get('status')}")
        if attrs.get("value") is not None:
            body_parts.append(f"Value: {attrs['value']}")

        return {
            "id": f"twenty:{cache_type}:{raw['id']}",
            "source": self.key,
            "type": cache_type,
            "external_id": str(raw["id"]),
            "title": title,
            "body_text": " | ".join(x for x in body_parts if x),
            "occurred_at": updated_at or created_at or datetime.now(timezone.utc).isoformat(),
            "url": raw.get("links", {}).get("self") or url,
            "payload": {
                "raw_type": raw.get("type", ""),
                "attributes": {k: v for k, v in attrs.items() if k not in ("description", "note", "content")},
            },
            "links": [
                {"href": raw.get("links", {}).get("self"), "rel": "self"},
            ],
            "deleted": raw.get("meta", {}).get("deleted", False) or False,
        }

    # -- tools -----------------------------------------------------------------

    def tools(self) -> list[ToolSpec]:
        return [
            ToolSpec(
                name="ping",
                schema={"type": "object", "properties": {}},
                impl=lambda self=self: {"ok": self._client().ping()},
            ),
            ToolSpec(
                name="summary",
                schema={"type": "object", "properties": {}},
                impl=lambda self=self: self._client().summary(),
            ),
            ToolSpec(
                name="people",
                schema={
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "description": "max results, default 50"},
                        "cursor": {"type": "string", "description": "pagination cursor"},
                    },
                },
                impl=lambda limit=50, cursor=None, self=self: self._client().people(limit=limit, cursor=cursor),
            ),
            ToolSpec(
                name="companies",
                schema={
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "description": "max results, default 50"},
                        "cursor": {"type": "string", "description": "pagination cursor"},
                    },
                },
                impl=lambda limit=50, cursor=None, self=self: self._client().companies(limit=limit, cursor=cursor),
            ),
            ToolSpec(
                name="deals",
                schema={
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "description": "max results, default 50"},
                        "cursor": {"type": "string", "description": "pagination cursor"},
                    },
                },
                impl=lambda limit=50, cursor=None, self=self: self._client().deals(limit=limit, cursor=cursor),
            ),
            ToolSpec(
                name="tasks",
                schema={
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "description": "max results, default 50"},
                        "cursor": {"type": "string", "description": "pagination cursor"},
                    },
                },
                impl=lambda limit=50, cursor=None, self=self: self._client().tasks(limit=limit, cursor=cursor),
            ),
            ToolSpec(
                name="notes",
                schema={
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "description": "max results, default 50"},
                        "cursor": {"type": "string", "description": "pagination cursor"},
                    },
                },
                impl=lambda limit=50, cursor=None, self=self: self._client().notes(limit=limit, cursor=cursor),
            ),
            ToolSpec(
                name="person",
                schema={"type": "object", "properties": {"id": {"type": "string"}}},
                impl=lambda id, self=self: self._client().person(id),
            ),
            ToolSpec(
                name="company",
                schema={"type": "object", "properties": {"id": {"type": "string"}}},
                impl=lambda id, self=self: self._client().company(id),
            ),
            ToolSpec(
                name="deal",
                schema={"type": "object", "properties": {"id": {"type": "string"}}},
                impl=lambda id, self=self: self._client().deal(id),
            ),
            ToolSpec(
                name="task",
                schema={"type": "object", "properties": {"id": {"type": "string"}}},
                impl=lambda id, self=self: self._client().task(id),
            ),
            ToolSpec(
                name="note",
                schema={"type": "object", "properties": {"id": {"type": "string"}}},
                impl=lambda id, self=self: self._client().note(id),
            ),
        ]

    def webhook(self, request) -> list[dict] | None:
        # Twenty can push webhooks; for now we don't handle them.
        return None
