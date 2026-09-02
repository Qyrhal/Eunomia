"""Source discovery + the operations that run across all sources."""

import importlib
import pkgutil

from .base import Source

_REGISTRY: dict[str, Source] = {}


def register(src: Source):
    if not src.key:
        raise ValueError(f"{src!r} has no key")
    _REGISTRY[src.key] = src


def discover():
    """Import every ``sources/<pkg>/`` that exposes a module-level ``SOURCE``."""
    import sources as pkg

    for mod in pkgutil.iter_modules(pkg.__path__):
        if not mod.ispkg or mod.name.startswith("_"):
            continue
        try:
            sub = importlib.import_module(f"sources.{mod.name}")
        except Exception:  # a broken source must not kill startup
            continue
        src = getattr(sub, "SOURCE", None)
        if isinstance(src, Source):
            register(src)


def get(key: str) -> Source | None:
    return _REGISTRY.get(key)


def all() -> list[Source]:
    return list(_REGISTRY.values())


def connector_for(src: Source):
    """The connectors.Connector row that holds this source's credentials."""
    from connectors.models import Connector

    return Connector.objects.filter(kind=src.provider_key).first()


def credentials_for(src: Source) -> dict:
    conn = connector_for(src)
    return (conn.credentials or {}) if conn else {}


def enabled() -> list[Source]:
    from connectors.models import Connector

    on = set(
        Connector.objects.filter(enabled=True).values_list("kind", flat=True)
    )
    return [s for s in _REGISTRY.values() if s.provider_key in on]


def _secret_values(src: Source) -> list[str]:
    """Resolve `src.secret_fields` against the stored credentials dict."""
    conn = connector_for(src)
    if not conn:
        return []
    creds = conn.credentials or {}
    out = []
    for path in src.secret_fields:
        cur = creds
        for part in path.split("."):
            cur = cur.get(part) if isinstance(cur, dict) else None
        if isinstance(cur, str) and cur:
            out.append(cur)
    return out


def run_sync(key: str, mode: str = "poll", cursor: str | None = None):
    """Sync one source through the ingest pipeline. Returns (IngestReport, cursor)."""
    from cache.ingest import ingest

    src = get(key)
    if src is None:
        raise KeyError(f"no source {key!r}")
    result = src.sync(mode, cursor)
    report = ingest(key, result.records, src.map, secret_values=_secret_values(src))
    return report, result.cursor


def tool_registry() -> dict:
    """Every source's per-source tools, keyed by fully-qualified name.
    The generic tools (#7) are merged in by the tool layer (#37)."""
    reg: dict[str, object] = {}
    for src in _REGISTRY.values():
        for t in src.tools():
            reg[f"{src.key}__{t.name}"] = t  # __ not . — OpenAI-compatible function-name rules
    return reg
