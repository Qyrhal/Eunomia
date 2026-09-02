"""Generic read-only tools over the cache (#7). All strings may contain
`[eunomia:*]` tokens — callers treat them as opaque handles.
"""

from django.core.exceptions import FieldError

from cache import search as cs

_SNIPPET = 200
# columns an agent may sort/filter on — anything else is a clean error, not a 500
_FIELDS = {"occurred_at", "ingested_at", "updated_at", "title", "type", "source", "id", "external_id"}


def safe(fn):
    """A tool must never raise on bad agent input — return an error dict instead."""
    def wrapper(*a, **kw):
        try:
            return fn(*a, **kw)
        except (FieldError, ValueError, TypeError, KeyError) as e:
            return {"error": f"{fn.__name__}: {e}"}
    wrapper.__name__ = fn.__name__
    return wrapper


def _hit(rec) -> dict:
    body = rec.body_text or ""
    return {
        "id": rec.id,
        "source": rec.source,
        "type": rec.type,
        "title": rec.title,
        "snippet": body[:_SNIPPET] + ("…" if len(body) > _SNIPPET else ""),
        "occurred_at": rec.occurred_at.isoformat() if rec.occurred_at else None,
        "url": rec.url or None,
    }


def _full(rec) -> dict:
    return {
        "id": rec.id,
        "source": rec.source,
        "type": rec.type,
        "external_id": rec.external_id,
        "title": rec.title,
        "body_text": rec.body_text,
        "occurred_at": rec.occurred_at.isoformat() if rec.occurred_at else None,
        "url": rec.url or None,
        "payload": rec.payload,
        "links": cs.links(rec.id),
    }


@safe
def search(query, sources=None, types=None, since=None, until=None, mode="hybrid", limit=20):
    rows = cs.search(
        query, sources=sources, types=types, since=since, until=until,
        mode=mode, limit=min(int(limit), 100),
    )
    return {"results": [_hit(r) for r in rows]}


@safe
def get(id):
    rec = cs.get(id)
    return _full(rec) if rec else {"error": "not found"}


@safe
def list(type=None, filters=None, sort="-occurred_at", limit=50):
    if sort.lstrip("-") not in _FIELDS:
        return {"error": f"unknown sort field {sort!r}; use one of {sorted(_FIELDS)}"}
    bad = {k.split("__")[0] for k in (filters or {})} - _FIELDS - {"payload"}
    if bad:
        return {"error": f"unknown filter field(s) {sorted(bad)}; use one of {sorted(_FIELDS)} or payload__*"}
    rows = cs.list_records(type=type, filters=filters or {}, sort=sort, limit=min(int(limit), 200))
    return {"results": [_hit(r) for r in rows]}


@safe
def links(id, rel=None):
    return {"links": cs.links(id, rel)}


SCHEMAS = {
    "search": {
        "type": "object",
        "properties": {
            "query": {"type": "string"},
            "sources": {"type": "array", "items": {"type": "string"}},
            "types": {"type": "array", "items": {"type": "string"}},
            "since": {"type": "string", "description": "ISO 8601"},
            "until": {"type": "string", "description": "ISO 8601"},
            "mode": {"type": "string", "enum": ["keyword", "semantic", "hybrid"]},
            "limit": {"type": "integer"},
        },
        "required": ["query"],
    },
    "get": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]},
    "list": {
        "type": "object",
        "properties": {
            "type": {"type": "string"},
            "filters": {"type": "object"},
            "sort": {"type": "string"},
            "limit": {"type": "integer"},
        },
    },
    "links": {
        "type": "object",
        "properties": {"id": {"type": "string"}, "rel": {"type": "string"}},
        "required": ["id"],
    },
}

IMPLS = {"search": search, "get": get, "list": list, "links": links}
