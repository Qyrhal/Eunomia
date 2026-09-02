"""Audit trail for token → value crossings (#36). Never stores real values."""

from datetime import timedelta

from django.utils import timezone

from .models import AuditEvent

RETENTION_DAYS = 90  # ponytail: fixed; lift to AppSettings if it ever needs tuning


def record(kind: str, actor: str, tokens, detail: str = "") -> None:
    toks = sorted(set(tokens))
    if not toks and kind != AuditEvent.KIND_REVEAL:
        return  # nothing was resolved — don't log noise
    AuditEvent.objects.create(kind=kind, actor=actor or "", tokens=toks, detail=detail[:300])


def reveal(token: str, actor: str = "frontend") -> dict:
    """The one sanctioned path a real value crosses to a human."""
    from .vault import detokenize

    value = detokenize(token)
    record(AuditEvent.KIND_REVEAL, actor, [token], "hit" if value is not None else "unknown token")
    if value is None:
        return {"error": "unknown token"}
    return {"token": token, "value": value}


def query(kind: str | None = None, limit: int = 100) -> list[dict]:
    qs = AuditEvent.objects.all().order_by("-id")
    if kind:
        qs = qs.filter(kind=kind)
    return [
        {"kind": e.kind, "actor": e.actor, "tokens": e.tokens, "detail": e.detail,
         "at": e.created_at.isoformat()}
        for e in qs[: min(int(limit), 500)]
    ]


def purge_old(days: int = RETENTION_DAYS) -> int:
    cutoff = timezone.now() - timedelta(days=days)
    n, _ = AuditEvent.objects.filter(created_at__lt=cutoff).delete()
    return n
