"""Trigger registration tools — Hermes (or the frontend) creates/edits watches."""

from .models import DeliveryLog, Trigger


def _dump(t: Trigger) -> dict:
    return {
        "key": t.key, "kind": t.kind, "enabled": t.enabled, "spec": t.spec,
        "webhook_route": t.webhook_route, "dedupe_window_s": t.dedupe_window_s,
        "fire_count": t.fire_count,
        "last_fired_at": t.last_fired_at.isoformat() if t.last_fired_at else None,
    }


def create_trigger(key, kind, spec, webhook_route="", dedupe_window_s=3600):
    if kind not in dict(Trigger.KIND_CHOICES):
        return {"error": f"bad kind {kind}"}
    t, created = Trigger.objects.update_or_create(
        key=key,
        defaults=dict(kind=kind, spec=spec, webhook_route=webhook_route,
                      dedupe_window_s=dedupe_window_s, enabled=True),
    )
    return {"key": t.key, "created": created}


def list_triggers():
    return {"triggers": [_dump(t) for t in Trigger.objects.all()]}


def update_trigger(key, **fields):
    t = Trigger.objects.filter(pk=key).first()
    if not t:
        return {"error": f"no trigger {key}"}
    for f in ("kind", "spec", "webhook_route", "dedupe_window_s", "enabled"):
        if f in fields and fields[f] is not None:
            setattr(t, f, fields[f])
    t.save()
    return {"key": key, "updated": True}


def delete_trigger(key):
    n, _ = Trigger.objects.filter(pk=key).delete()
    return {"deleted": bool(n)}


def test_trigger(key):
    t = Trigger.objects.filter(pk=key).first()
    if not t:
        return {"error": f"no trigger {key}"}
    from .delivery import fire

    ok = fire(t, {"id": f"test:{key}", "type": "test", "title": "synthetic test event"},
              payload={"test": True})
    return {"delivered": ok}


def delivery_log(key=None, limit=50):
    qs = DeliveryLog.objects.all().order_by("-id")
    if key:
        qs = qs.filter(trigger_key=key)
    return {"deliveries": [
        {"trigger": d.trigger_key, "entity": d.entity_id, "attempt": d.attempt,
         "status": d.http_status, "ok": d.ok, "dead": d.dead, "detail": d.detail,
         "at": d.created_at.isoformat()}
        for d in qs[:min(int(limit), 200)]
    ]}


SCHEMAS = {
    "create_trigger": {
        "type": "object",
        "properties": {
            "key": {"type": "string"},
            "kind": {"type": "string", "enum": ["record_rule", "schedule", "cron"]},
            "spec": {"type": "object"},
            "webhook_route": {"type": "string"},
            "dedupe_window_s": {"type": "integer"},
        },
        "required": ["key", "kind", "spec"],
    },
    "list_triggers": {"type": "object", "properties": {}},
    "update_trigger": {
        "type": "object",
        "properties": {"key": {"type": "string"}, "spec": {"type": "object"},
                       "enabled": {"type": "boolean"}, "webhook_route": {"type": "string"},
                       "dedupe_window_s": {"type": "integer"}},
        "required": ["key"],
    },
    "delete_trigger": {"type": "object", "properties": {"key": {"type": "string"}}, "required": ["key"]},
    "test_trigger": {"type": "object", "properties": {"key": {"type": "string"}}, "required": ["key"]},
    "delivery_log": {"type": "object", "properties": {"key": {"type": "string"}, "limit": {"type": "integer"}}},
}

IMPLS = {
    "create_trigger": create_trigger, "list_triggers": list_triggers,
    "update_trigger": update_trigger, "delete_trigger": delete_trigger,
    "test_trigger": test_trigger, "delivery_log": delivery_log,
}

BUILTINS = [
    dict(key="big_transaction", kind="record_rule",
         spec={"types": ["up.transaction"], "match": [["payload.amount_cents", "lt", -20000]]}),
    dict(key="vip_email", kind="record_rule",
         spec={"types": ["gmail.message"], "match": [["payload.vip", "eq", True]]}),
    dict(key="task_due_soon", kind="schedule",
         spec={"anchor": "task.due_at", "filter": {}, "offset_s": -86400}),
    dict(key="daily_digest", kind="cron", spec={"cron": "0 8 * * *", "payload": {"kind": "digest"}}),
]


def ensure_builtins():
    for b in BUILTINS:
        Trigger.objects.get_or_create(
            key=b["key"], defaults=dict(kind=b["kind"], spec=b["spec"], enabled=False))


def register():
    from tools.registry import register_tool

    for name, impl in IMPLS.items():
        register_tool(name, SCHEMAS[name], impl)
