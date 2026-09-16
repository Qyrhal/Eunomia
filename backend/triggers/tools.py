"""Trigger registration tools — Hermes (or the frontend) creates/edits watches."""

from .models import DeliveryLog, Trigger

# Default webhook route for the built-in triggers: Hermes receives these on
# {hermes_webhook_url}/eunomia (route "eunomia" is the gateway's Eunomia adapter).
DEFAULT_ROUTE = "eunomia"


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


def enable_builtins(webhook_route=None):
    """Enable the built-in task_due_soon + daily_digest triggers with a real route (#49).

    Idempotent and safe to call repeatedly: never disables anything and leaves
    user-edited specs alone. With no argument the default route ("eunomia") is
    only filled in when the trigger has no route yet; an explicitly passed
    route (including "" for the bare hermes_webhook_url) overrides what is set.
    """
    route_provided = webhook_route is not None
    route = (webhook_route or "").strip("/")
    out = {}
    for key in ("task_due_soon", "daily_digest"):
        t = Trigger.objects.filter(pk=key).first()
        if not t:
            ensure_builtins()
            t = Trigger.objects.get(pk=key)
        changed = {}
        if not t.enabled:
            t.enabled = True
            changed["enabled"] = True
        if route_provided:
            if t.webhook_route != route:
                t.webhook_route = route
                changed["webhook_route"] = route
        elif route and not t.webhook_route:
            t.webhook_route = route
            changed["webhook_route"] = route
        if changed:
            t.save()
        out[key] = {"enabled": t.enabled, "webhook_route": t.webhook_route}
    return {"enabled": out}


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
    "enable_builtins": {
        "type": "object",
        "properties": {"webhook_route": {"type": "string", "description": 'route under hermes_webhook_url; default "eunomia"'}},
    },
    "test_trigger": {"type": "object", "properties": {"key": {"type": "string"}}, "required": ["key"]},
    "delivery_log": {"type": "object", "properties": {"key": {"type": "string"}, "limit": {"type": "integer"}}},
}

IMPLS = {
    "create_trigger": create_trigger, "list_triggers": list_triggers,
    "update_trigger": update_trigger, "delete_trigger": delete_trigger,
    "enable_builtins": enable_builtins, "test_trigger": test_trigger,
    "delivery_log": delivery_log,
}

BUILTINS = [
    dict(key="big_transaction", kind="record_rule", webhook_route=DEFAULT_ROUTE,
         spec={"types": ["up.transaction"], "match": [["payload.amount_cents", "lt", -20000]]}),
    dict(key="vip_email", kind="record_rule", webhook_route=DEFAULT_ROUTE,
         spec={"types": ["gmail.message"], "match": [["payload.vip", "eq", True]]}),
    dict(key="task_due_soon", kind="schedule", webhook_route=DEFAULT_ROUTE,
         spec={"anchor": "task.due_at", "filter": {}, "offset_s": -86400}),
    dict(key="daily_digest", kind="cron", webhook_route=DEFAULT_ROUTE,
         spec={"cron": "0 8 * * *", "digest": True, "payload": {"kind": "digest"}}),
]


def ensure_builtins():
    for b in BUILTINS:
        t, created = Trigger.objects.get_or_create(
            key=b["key"], defaults=dict(kind=b["kind"], spec=b["spec"],
                                        webhook_route=b["webhook_route"], enabled=False))
        # backfill the default route on builtins seeded before #49, without
        # clobbering anything an operator already configured
        if not created and not t.webhook_route:
            Trigger.objects.filter(pk=t.pk, webhook_route="").update(
                webhook_route=b["webhook_route"])


def register():
    from tools.registry import register_tool

    for name, impl in IMPLS.items():
        register_tool(name, SCHEMAS[name], impl)
