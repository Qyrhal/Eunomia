"""Record-rule evaluation. `evaluate_record` is called from the ingest pipeline
(#31 stage 7) for every persisted record.
"""

import operator

from .models import Trigger

_OPS = {
    "eq": operator.eq, "ne": operator.ne,
    "gt": operator.gt, "gte": operator.ge, "lt": operator.lt, "lte": operator.le,
    "contains": lambda a, b: b in (a or ""),
    "in": lambda a, b: a in (b or []),
}


def _field(rec, path: str):
    parts = path.split(".")
    if parts[0] == "payload":
        cur = rec.payload or {}
        for p in parts[1:]:
            cur = cur.get(p) if isinstance(cur, dict) else None
        return cur
    return getattr(rec, parts[0], None)


def _matches(rec, spec: dict) -> bool:
    if spec.get("types") and rec.type not in spec["types"]:
        return False
    if spec.get("sources") and rec.source not in spec["sources"]:
        return False
    for clause in spec.get("match", []):
        field, op, value = clause
        fn = _OPS.get(op)
        if fn is None:
            return False
        try:
            if not fn(_field(rec, field), value):
                return False
        except TypeError:
            return False
    return True


def evaluate_record(rec) -> list[str]:
    """Fire every enabled record_rule trigger that matches `rec`. Returns fired keys."""
    from .delivery import fire

    fired = []
    entity = {"id": rec.id, "type": rec.type, "title": rec.title, "url": rec.url or None}
    for trg in Trigger.objects.filter(kind=Trigger.KIND_RECORD, enabled=True):
        if _matches(rec, trg.spec or {}):
            if fire(trg, entity):
                fired.append(trg.key)
    return fired
