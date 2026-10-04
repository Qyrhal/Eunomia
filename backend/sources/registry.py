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
    """Import every `sources/<pkg>/` that exposes a module-level `SOURCE`."""
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


async def connector_for(src: Source) -> dict | None:
    """The connector row that holds this source's credentials."""
    from connectors.service import get_connector

    return await get_connector(src.provider_key)


async def credentials_for(src: Source) -> dict:
    import json

    from connectors.crypto import decrypt

    conn = await connector_for(src)
    if not conn:
        return {}
    raw = decrypt(conn.get("credentials_encrypted", ""))
    return json.loads(raw) if raw else {}


async def enabled() -> list[Source]:
    from app.db import db as get_connection

    conn = get_connection()
    rows = await conn.query("SELECT kind, config FROM connector WHERE enabled = true")
    # filtered in Python, not the DB -- a missing "demo" key and an explicit
    # false both need to count as "not demo mode".
    on = {r["kind"] for r in rows if not (r.get("config") or {}).get("demo")}
    return [s for s in _REGISTRY.values() if s.provider_key in on]


async def run_sync(key: str, mode: str = "poll", cursor: str | None = None):
    """Sync one source through the ingest pipeline. Returns (IngestReport, cursor)."""
    from cache.ingest import ingest

    src = get(key)
    if src is None:
        raise KeyError(f"no source {key!r}")
    result = await src.sync(mode, cursor)
    report = await ingest(key, result.records, src.map)
    return report, result.cursor


def tool_registry() -> dict:
    """Every source's per-source tools, keyed by fully-qualified name.
    The generic tools are merged in by the tool layer."""
    reg: dict[str, object] = {}
    for src in _REGISTRY.values():
        for t in src.tools():
            reg[f"{src.key}__{t.name}"] = t  # __ not . -- OpenAI-compatible function-name rules
    return reg
