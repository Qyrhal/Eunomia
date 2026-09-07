"""Task graph (#38): tasks the agent can tag, link to cache records, and search
semantically. Links reuse cache.CacheLink with source_id = "task:<uuid>".
Task vectors share the cache_vec table with id "task:<uuid>".

#48: every task id an agent sends — bare UUID or task:<uuid> vid — is accepted
by every tool, and link targets that are tasks are stored as vids so edges
round-trip through the generic tools (get / links / search).
"""

import json
import uuid

from django.db import connection
from django.db.models import Q
from django.utils import timezone

from cache.models import CacheLink
from cache.search import links as _cache_links
from tasks.models import Project, Tag, Task

TASK_PREFIX = "task:"


def vid(task) -> str:
    """Canonical cache-graph id of a task: ``task:<uuid>``. Accepts a Task
    instance, a bare task UUID, or an already-prefixed vid (#48)."""
    id_ = str(task.id if isinstance(task, Task) else (task or ""))
    return id_ if id_.startswith(TASK_PREFIX) else f"{TASK_PREFIX}{id_}"


def resolve_task(id) -> Task | None:
    """Resolve an agent-supplied task reference — bare UUID or task:<uuid> vid —
    to a Task, or None. Cache-record-shaped ids (``source:type:ext``) resolve
    to None rather than raising inside the UUID field."""
    s = str(id or "")
    if s.startswith(TASK_PREFIX):
        s = s[len(TASK_PREFIX):]
    try:
        key = uuid.UUID(s)
    except (ValueError, AttributeError, TypeError):
        return None
    return Task.objects.filter(pk=key).first()


def _embed_text(task) -> str:
    tags = " ".join(task.tags.values_list("name", flat=True))
    return "\n".join(p for p in (task.title, task.notes, tags) if p).strip()


def index_task(task) -> None:
    """(Re)compute the task's vector. Cheap via the embedding memo; safe to call often."""
    from embeddings.service import embed

    text = _embed_text(task)
    if not text:
        return
    vec = embed([text])[0]
    with connection.cursor() as cur:
        cur.execute("DELETE FROM cache_vec WHERE record_id = %s", [vid(task)])
        cur.execute("INSERT INTO cache_vec (record_id, embedding) VALUES (%s, %s)", [vid(task), json.dumps(vec)])
    if not task.has_embedding:
        Task.objects.filter(pk=task.pk).update(has_embedding=True)


# --- agent write tools --------------------------------------------------------

def _aware(value):
    """Coerce an agent-supplied date/datetime string to an aware datetime.

    Aware inputs pass through; naive datetimes and bare dates are interpreted
    in the operator's configured timezone (AppSettings.user_timezone, set by
    the installing agent) — NOT the server's clock zone (homelab boxes run
    UTC). See tasks.timezone_utils.
    """
    from tasks.timezone_utils import parse_user_datetime

    return parse_user_datetime(value)


def _due_fields(task):
    """Canonical + operator-local rendering of a task's due_at for agents."""
    from tasks.timezone_utils import local_fields

    return local_fields(task.due_at)


def create_task(title, notes="", project=None, due_at=None, priority=0, tags=None, props=None):
    proj, _ = Project.objects.get_or_create(name=project or "Inbox")
    task = Task.objects.create(
        project=proj, title=title, notes=notes or "", due_at=_aware(due_at),
        priority=int(priority or 0), props=props or {}, created_by_ai=True,
    )
    for name in tags or []:
        tag, _ = Tag.objects.get_or_create(name=name)
        task.tags.add(tag)
    index_task(task)
    # echo tags/props so the agent can confirm what actually persisted (#48)
    return {
        "id": str(task.id), "vid": vid(task), "created": True,
        "due_at": _due_fields(task),
        "tags": list(task.tags.values_list("name", flat=True)),
        "props": task.props,
    }


def update_task(id, **fields):
    task = resolve_task(id)
    if not task:
        return {"error": f"no task {id}"}
    for key in ("title", "notes", "due_at", "priority", "flagged", "completed", "props"):
        if key in fields and fields[key] is not None:
            setattr(task, key, _aware(fields[key]) if key == "due_at" else fields[key])
    if fields.get("completed"):
        task.completed_at = timezone.now()
    if "tags" in fields and fields["tags"] is not None:
        task.tags.clear()
        for name in fields["tags"]:
            tag, _ = Tag.objects.get_or_create(name=name)
            task.tags.add(tag)
    task.save()
    index_task(task)
    return {
        "id": str(task.id), "vid": vid(task), "updated": True,
        "due_at": _due_fields(task), "props": task.props,
    }


def _link_target(target_id) -> str:
    """Canonical link target: a task reference (bare UUID or vid) becomes a
    vid so task-to-task edges round-trip through the generic links tool (#48);
    cache record ids pass through unchanged."""
    s = str(target_id or "")
    if s.startswith(TASK_PREFIX):
        return s
    t = resolve_task(s)
    return vid(t) if t else s


def link_task(id, rel, target_id):
    task = resolve_task(id)
    if not task:
        return {"error": f"no task {id}"}
    target = _link_target(target_id)
    CacheLink.objects.get_or_create(
        source_id=vid(task), rel=rel, target_id=target,
        defaults={"origin": CacheLink.ORIGIN_AGENT},
    )
    return {"linked": True, "source_id": vid(task), "target_id": target}


def unlink_task(id, rel, target_id):
    n, _ = CacheLink.objects.filter(
        source_id=vid(id), rel=rel, target_id=_link_target(target_id)
    ).delete()
    return {"unlinked": bool(n)}


def schedule_task(id, when):
    task = resolve_task(id)
    if not task:
        return {"error": f"no task {id}"}
    task.due_at = _aware(when)
    task.save(update_fields=["due_at", "updated_at"])
    # a time-relative trigger (#39) can be registered against this due date
    return {"id": str(task.id), "scheduled_for": _due_fields(task)}


def task_links(id, rel=None):
    task = resolve_task(id)
    if not task:
        return {"error": f"no task {id}"}
    return {"links": _cache_links(vid(task), rel)}


def _task_kw_uuids(query, limit):
    """Tasks whose title, notes, or tags mention the query (keyword leg)."""
    return Task.objects.filter(
        Q(title__icontains=query) | Q(notes__icontains=query) | Q(tags__name__icontains=query)
    ).values_list("id", flat=True).distinct()[:limit]


def _task_sem_vids(query, limit):
    """Vids of tasks closest to the query in the shared cache_vec table."""
    from embeddings.service import embed

    vec = embed([query])[0]
    with connection.cursor() as cur:
        cur.execute(
            "SELECT record_id FROM cache_vec WHERE embedding MATCH %s AND k = %s "
            "AND record_id LIKE 'task:%%' ORDER BY distance",
            [json.dumps(vec), limit],
        )
        return [r[0] for r in cur.fetchall()]


def search_task_records(query, *, mode="hybrid", limit=20):
    """Ranked Tasks matching `query` — the task leg of the generic search tool
    (#48). mode mirrors cache.search: keyword (title/notes/tags), semantic
    (shared cache_vec vectors), hybrid (keyword first, then semantic, deduped)."""
    limit = min(int(limit), 100)
    order: list[str] = []
    if mode in ("keyword", "hybrid"):
        order += [str(u) for u in _task_kw_uuids(query, limit * 2)]
    if mode in ("semantic", "hybrid"):
        order += [rid[len(TASK_PREFIX):] for rid in _task_sem_vids(query, limit)]
    ordered = list(dict.fromkeys(order))
    by_id = {str(t.id): t for t in Task.objects.filter(pk__in=ordered)}
    return [by_id[u] for u in ordered if u in by_id]


def search_tasks(query, limit=20):
    rows = search_task_records(query, mode="hybrid", limit=limit)
    return {
        "results": [
            {"id": str(t.id), "vid": vid(t), "title": t.title, "completed": t.completed,
             "due_at": t.due_at.isoformat() if t.due_at else None,
             "due": _due_fields(t),
             "tags": list(t.tags.values_list("name", flat=True))}
            for t in rows
        ]
    }


SCHEMAS = {
    "create_task": {
        "type": "object",
        "properties": {
            "title": {"type": "string"}, "notes": {"type": "string"},
            "project": {"type": "string"}, "due_at": {"type": "string"},
            "priority": {"type": "integer"}, "tags": {"type": "array", "items": {"type": "string"}},
            "props": {"type": "object", "description": "arbitrary JSON key/values stored with the task"},
        },
        "required": ["title"],
    },
    "update_task": {
        "type": "object",
        "properties": {"id": {"type": "string", "description": "task UUID or task:<uuid>"},
                       "title": {"type": "string"}, "notes": {"type": "string"},
                       "due_at": {"type": "string"},
                       "priority": {"type": "integer"},
                       "flagged": {"type": "boolean"}, "completed": {"type": "boolean"},
                       "tags": {"type": "array", "items": {"type": "string"}},
                       "props": {"type": "object", "description": "replaces the task's props object"}},
        "required": ["id"],
    },
    "link_task": {
        "type": "object",
        "properties": {"id": {"type": "string", "description": "task UUID or task:<uuid>"},
                       "rel": {"type": "string"},
                       "target_id": {"type": "string", "description": "task UUID/vid or cache record id"}},
        "required": ["id", "rel", "target_id"],
    },
    "unlink_task": {
        "type": "object",
        "properties": {"id": {"type": "string", "description": "task UUID or task:<uuid>"},
                       "rel": {"type": "string"},
                       "target_id": {"type": "string", "description": "task UUID/vid or cache record id"}},
        "required": ["id", "rel", "target_id"],
    },
    "schedule_task": {
        "type": "object",
        "properties": {"id": {"type": "string", "description": "task UUID or task:<uuid>"},
                       "when": {"type": "string", "description": "ISO 8601"}},
        "required": ["id", "when"],
    },
    "task_links": {
        "type": "object",
        "properties": {"id": {"type": "string", "description": "task UUID or task:<uuid>"},
                       "rel": {"type": "string"}},
        "required": ["id"],
    },
    "search_tasks": {
        "type": "object",
        "properties": {"query": {"type": "string"}, "limit": {"type": "integer"}},
        "required": ["query"],
    },
}

IMPLS = {
    "create_task": create_task, "update_task": update_task,
    "link_task": link_task, "unlink_task": unlink_task,
    "schedule_task": schedule_task, "task_links": task_links,
    "search_tasks": search_tasks,
}


def _on_task_saved(sender, instance, **kwargs):
    # ponytail: re-embeds on every task save. Cheap via the embedding memo (#29);
    # if task-write volume ever spikes, debounce or move to the scheduler.
    try:
        index_task(instance)
    except Exception:
        pass  # embedding backend down — rebuild_embeddings / backfill will catch it


def register():
    from django.db.models.signals import post_save

    from tools.registry import register_tool

    for name, impl in IMPLS.items():
        register_tool(name, SCHEMAS[name], impl)
    post_save.connect(_on_task_saved, sender=Task, dispatch_uid="tasks.graph.index")
