"""Task graph (#38): tasks the agent can tag, link to cache records, and search
semantically. Links reuse cache.CacheLink with source_id = "task:<uuid>".
Task vectors share the cache_vec table with id "task:<uuid>".
"""

import json

from django.db import connection
from django.utils import timezone

from cache.models import CacheLink
from cache.search import links as _cache_links
from tasks.models import Project, Tag, Task

TASK_PREFIX = "task:"


def vid(task) -> str:
    return f"{TASK_PREFIX}{task.id}"


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

def create_task(title, notes="", project=None, due_at=None, priority=0, tags=None, props=None):
    proj, _ = Project.objects.get_or_create(name=project or "Inbox")
    task = Task.objects.create(
        project=proj, title=title, notes=notes or "", due_at=due_at or None,
        priority=int(priority or 0), props=props or {}, created_by_ai=True,
    )
    for name in tags or []:
        tag, _ = Tag.objects.get_or_create(name=name)
        task.tags.add(tag)
    index_task(task)
    return {"id": str(task.id), "created": True}


def update_task(id, **fields):
    task = Task.objects.filter(pk=id).first()
    if not task:
        return {"error": f"no task {id}"}
    for key in ("title", "notes", "due_at", "priority", "flagged", "completed", "props"):
        if key in fields and fields[key] is not None:
            setattr(task, key, fields[key])
    if fields.get("completed"):
        task.completed_at = timezone.now()
    if "tags" in fields and fields["tags"] is not None:
        task.tags.clear()
        for name in fields["tags"]:
            tag, _ = Tag.objects.get_or_create(name=name)
            task.tags.add(tag)
    task.save()
    index_task(task)
    return {"id": str(task.id), "updated": True}


def link_task(id, rel, target_id):
    if not Task.objects.filter(pk=id).exists():
        return {"error": f"no task {id}"}
    CacheLink.objects.get_or_create(
        source_id=f"{TASK_PREFIX}{id}", rel=rel, target_id=target_id,
        defaults={"origin": CacheLink.ORIGIN_AGENT},
    )
    return {"linked": True}


def unlink_task(id, rel, target_id):
    n, _ = CacheLink.objects.filter(source_id=f"{TASK_PREFIX}{id}", rel=rel, target_id=target_id).delete()
    return {"unlinked": bool(n)}


def schedule_task(id, when):
    task = Task.objects.filter(pk=id).first()
    if not task:
        return {"error": f"no task {id}"}
    task.due_at = when
    task.save(update_fields=["due_at", "updated_at"])
    # a time-relative trigger (#39) can be registered against this due date
    return {"id": str(task.id), "scheduled_for": when}


def task_links(id, rel=None):
    return {"links": _cache_links(f"{TASK_PREFIX}{id}", rel)}


def search_tasks(query, limit=20):
    from embeddings.service import embed

    limit = min(int(limit), 100)
    kw = list(
        Task.objects.filter(title__icontains=query).values_list("id", flat=True)[: limit * 2]
    ) + list(
        Task.objects.filter(notes__icontains=query).values_list("id", flat=True)[: limit * 2]
    )
    kw_ids = [f"{TASK_PREFIX}{i}" for i in kw]

    vec = embed([query])[0]
    with connection.cursor() as cur:
        cur.execute(
            "SELECT record_id FROM cache_vec WHERE embedding MATCH %s AND k = %s "
            "AND record_id LIKE 'task:%%' ORDER BY distance",
            [json.dumps(vec), limit],
        )
        sem_ids = [r[0] for r in cur.fetchall()]

    seen, order = set(), []
    for rid in kw_ids + sem_ids:
        if rid not in seen:
            seen.add(rid)
            order.append(rid)
    uuids = [rid[len(TASK_PREFIX):] for rid in order][:limit]
    by_id = {str(t.id): t for t in Task.objects.filter(pk__in=uuids)}
    rows = [by_id[u] for u in uuids if u in by_id]
    return {
        "results": [
            {"id": str(t.id), "title": t.title, "completed": t.completed,
             "due_at": t.due_at.isoformat() if t.due_at else None,
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
            "props": {"type": "object"},
        },
        "required": ["title"],
    },
    "update_task": {
        "type": "object",
        "properties": {
            "id": {"type": "string"}, "title": {"type": "string"}, "notes": {"type": "string"},
            "due_at": {"type": "string"}, "priority": {"type": "integer"},
            "flagged": {"type": "boolean"}, "completed": {"type": "boolean"},
            "tags": {"type": "array", "items": {"type": "string"}}, "props": {"type": "object"},
        },
        "required": ["id"],
    },
    "link_task": {
        "type": "object",
        "properties": {"id": {"type": "string"}, "rel": {"type": "string"}, "target_id": {"type": "string"}},
        "required": ["id", "rel", "target_id"],
    },
    "unlink_task": {
        "type": "object",
        "properties": {"id": {"type": "string"}, "rel": {"type": "string"}, "target_id": {"type": "string"}},
        "required": ["id", "rel", "target_id"],
    },
    "schedule_task": {
        "type": "object",
        "properties": {"id": {"type": "string"}, "when": {"type": "string", "description": "ISO 8601"}},
        "required": ["id", "when"],
    },
    "task_links": {"type": "object", "properties": {"id": {"type": "string"}, "rel": {"type": "string"}}, "required": ["id"]},
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
