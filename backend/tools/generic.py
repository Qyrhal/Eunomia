"""Generic read-only tools over the cache (#7). All strings may contain
`[eunomia:*]` tokens — callers treat them as opaque handles.

#48: tasks are first-class search/get/links citizens alongside cache records.
A task is addressed by its UUID or its graph vid (`task:<uuid>`); hits report
source="tasks", type="task". Filters `sources=["tasks"]` / `types=["task"]`
select just the task leg.
"""

from itertools import zip_longest

from django.core.exceptions import FieldError

from cache import search as cs

_SNIPPET = 200
# columns an agent may sort/filter on — anything else is a clean error, not a 500
_FIELDS = {"occurred_at", "ingested_at", "updated_at", "title", "type", "source", "id", "external_id"}

TASK_SOURCE = "tasks"
TASK_TYPE = "task"


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


def _task_hit(t) -> dict:
    from tasks.timezone_utils import local_fields

    body = t.notes or ""
    return {
        "id": str(t.id),
        "vid": f"{TASK_TYPE}:{t.id}",
        "source": TASK_SOURCE,
        "type": TASK_TYPE,
        "title": t.title,
        "snippet": body[:_SNIPPET] + ("…" if len(body) > _SNIPPET else ""),
        "occurred_at": t.due_at.isoformat() if t.due_at else None,
        "due": local_fields(t.due_at),
        "url": t.url or None,
    }


def _task_full(t) -> dict:
    from tasks.timezone_utils import local_fields

    return {
        "id": str(t.id),
        "vid": f"{TASK_TYPE}:{t.id}",
        "source": TASK_SOURCE,
        "type": TASK_TYPE,
        "external_id": str(t.id),
        "title": t.title,
        "body_text": t.notes,
        "occurred_at": t.due_at.isoformat() if t.due_at else None,
        "due": local_fields(t.due_at),
        "url": t.url or None,
        "payload": {
            "project": t.project.name,
            "tags": list(t.tags.values_list("name", flat=True)),
            "props": t.props,
            "priority": t.priority,
            "completed": t.completed,
            "flagged": t.flagged,
            "allocated_minutes": t.allocated_minutes,
        },
        "links": cs.links(f"{TASK_TYPE}:{t.id}"),
    }


def _resolve_task(id):
    """Bare task UUID or task:<uuid> vid -> Task | None (cache-shaped ids -> None)."""
    from tasks.graph import resolve_task

    return resolve_task(id)


@safe
def search(query, sources=None, types=None, since=None, until=None, mode="hybrid", limit=20):
    limit = min(int(limit), 100)
    hits = [
        _hit(r) for r in cs.search(
            query, sources=sources, types=types, since=since, until=until,
            mode=mode, limit=limit,
        )
    ]
    src, typ = set(sources or []), set(types or [])
    if (not src or TASK_SOURCE in src) and (not typ or TASK_TYPE in typ):
        from tasks.graph import search_task_records

        task_hits = [_task_hit(t) for t in search_task_records(query, mode=mode, limit=limit)]
        # interleave so tasks stay visible even when cache hits fill the limit
        hits = [h for pair in zip_longest(hits, task_hits) for h in pair if h is not None]
    return {"results": hits[:limit]}


@safe
def get(id):
    rec = cs.get(id)
    if rec:
        return _full(rec)
    task = _resolve_task(id)
    if task:
        return _task_full(task)
    return {"error": "not found"}


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
    # tasks key their edges under task:<uuid> — accept the bare UUID too (#48)
    task = _resolve_task(id)
    return {"links": cs.links(f"{TASK_TYPE}:{task.id}" if task else id, rel)}


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
