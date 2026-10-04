"""Cache write + query API. The only module that knows SurrealDB's BM25/MTREE
indexes exist — ``search.py``'s external signatures (``upsert``, ``set_embedding``,
``search``, ``get``, ``list_records``, ``links``) are the swap point for any
future backend, same principle as the old Django+sqlite-vec version.
"""

import hashlib
import json
from dataclasses import dataclass, field
from datetime import datetime, timezone

from surrealdb import RecordID

from app.db import db as get_connection
from embeddings.service import DIM

_RRF_K = 60


@dataclass
class CacheRecord:
    id: str  # literal "{source}:{type}:{external_id}" (no table prefix)
    source: str
    type: str
    external_id: str
    title: str = ""
    body_text: str = ""
    occurred_at: datetime | None = None
    url: str = ""
    payload: dict = field(default_factory=dict)
    content_hash: str = ""
    ingested_at: datetime | None = None
    updated_at: datetime | None = None
    deleted: bool = False
    embedding: list[float] | None = None


def _rid(record_id: str) -> RecordID:
    return RecordID("cache_record", record_id)


def _literal(rid: RecordID | str) -> str:
    return rid.id if isinstance(rid, RecordID) else str(rid)


def _row_to_record(row: dict) -> CacheRecord:
    return CacheRecord(
        id=_literal(row["id"]),
        source=row.get("source", ""),
        type=row.get("type", ""),
        external_id=row.get("external_id", ""),
        title=row.get("title", ""),
        body_text=row.get("body_text", ""),
        occurred_at=row.get("occurred_at"),
        url=row.get("url", ""),
        payload=row.get("payload") or {},
        content_hash=row.get("content_hash", ""),
        ingested_at=row.get("ingested_at"),
        updated_at=row.get("updated_at"),
        deleted=bool(row.get("deleted", False)),
        embedding=row.get("embedding"),
    )


async def _select_one(conn, rid: RecordID) -> dict | None:
    res = await conn.select(rid)
    if isinstance(res, list):
        return res[0] if res else None
    return res


def _hash_envelope(env: dict) -> str:
    keys = ("title", "body_text", "url", "occurred_at", "payload", "deleted")
    blob = json.dumps({k: env.get(k) for k in keys}, sort_keys=True, default=str)
    return hashlib.sha256(blob.encode()).hexdigest()


async def _reconcile_links(conn, rid: RecordID, links_spec: list[dict]) -> None:
    await conn.query("DELETE linked_to WHERE in = $id AND origin = 'sync'", {"id": rid})
    for link in links_spec:
        try:
            await conn.query(
                "RELATE $in->linked_to->$out SET rel = $rel, origin = 'sync'",
                {"in": rid, "out": _rid(link["target"]), "rel": link["rel"]},
            )
        except Exception:
            # (in, out, rel) unique index — edge already exists; idempotent
            # no-op, matching the old get_or_create-style behavior.
            pass


async def upsert(env: dict) -> tuple[CacheRecord, bool]:
    """Insert or update one envelope. Returns (record, changed)."""
    conn = get_connection()
    rid = _rid(env["id"])
    h = _hash_envelope(env)

    existing = await _select_one(conn, rid)
    now = datetime.now(timezone.utc)
    if existing and existing.get("content_hash") == h and not existing.get("deleted"):
        await conn.query("UPDATE $id SET ingested_at = $now", {"id": rid, "now": now})
        existing["ingested_at"] = now
        return _row_to_record(existing), False

    rows = await conn.query(
        "UPSERT $id SET source = $source, type = $type, external_id = $external_id, "
        "title = $title, body_text = $body_text, occurred_at = $occurred_at, url = $url, "
        "payload = $payload, content_hash = $content_hash, ingested_at = $ingested_at, "
        "updated_at = $updated_at, deleted = $deleted RETURN AFTER",
        {
            "id": rid,
            "source": env["source"],
            "type": env["type"],
            "external_id": env["external_id"],
            "title": env.get("title", ""),
            "body_text": env.get("body_text", ""),
            "occurred_at": env.get("occurred_at"),
            "url": env.get("url", ""),
            "payload": env.get("payload", {}),
            "content_hash": h,
            "ingested_at": now,
            "updated_at": now,
            "deleted": bool(env.get("deleted", False)),
        },
    )
    rec = _row_to_record(rows[0])
    await _reconcile_links(conn, rid, env.get("links", []))
    return rec, True


async def set_embedding(record_id: str, vector: list[float]) -> None:
    if len(vector) != DIM:
        raise ValueError(f"embedding dim {len(vector)} != {DIM}")
    conn = get_connection()
    await conn.query("UPDATE $id SET embedding = $embedding", {"id": _rid(record_id), "embedding": vector})


async def _keyword_ids(conn, q: str, limit: int) -> list[str]:
    # cache_record_fts_idx is a composite BM25 index over (title, body_text), but
    # this SurrealDB version only resolves the `@N@` match operator against the
    # FIRST field of a composite search index (title) -- body_text-only matches
    # raise "no suitable index". Use the index for title, then fall back to a
    # plain substring scan for body_text so keyword search still covers both
    # fields (functionally correct; just not BM25-ranked for body-only hits).
    rows = await conn.query(
        "SELECT id, search::score(1) AS score FROM cache_record "
        "WHERE title @1@ $q AND deleted = false ORDER BY score DESC LIMIT $limit",
        {"q": q, "limit": limit},
    )
    ids = [_literal(r["id"]) for r in rows]
    if len(ids) < limit:
        seen = [_rid(i) for i in ids]
        extra = await conn.query(
            "SELECT id FROM cache_record WHERE "
            "string::contains(string::lowercase(body_text), string::lowercase($q)) "
            "AND deleted = false AND id NOT IN $seen LIMIT $limit",
            {"q": q, "seen": seen, "limit": limit - len(ids)},
        )
        ids.extend(_literal(r["id"]) for r in extra)
    return ids[:limit]


async def _semantic_ids(conn, q: str, limit: int) -> list[str]:
    from embeddings.service import embed

    vec = (await embed([q]))[0]
    # the KNN `<|K|>` operator requires a literal integer -- it cannot be a bound
    # parameter -- so `limit` (always an int from this module's call sites) is
    # interpolated directly rather than passed as $limit.
    rows = await conn.query(
        f"SELECT id FROM cache_record WHERE embedding <|{int(limit)}|> $vec AND deleted = false",
        {"vec": vec},
    )
    return [_literal(r["id"]) for r in rows]


def _rrf(*ranked_lists: list[str]) -> list[str]:
    scores: dict[str, float] = {}
    for lst in ranked_lists:
        for rank, rid in enumerate(lst):
            scores[rid] = scores.get(rid, 0.0) + 1.0 / (_RRF_K + rank + 1)
    return [rid for rid, _ in sorted(scores.items(), key=lambda kv: -kv[1])]


async def search(
    q,
    *,
    sources=None,
    types=None,
    since=None,
    until=None,
    mode="hybrid",
    limit=20,
    offset=0,
) -> list[CacheRecord]:
    conn = get_connection()
    pool = max(limit * 4, 40)
    if mode == "keyword":
        ids = await _keyword_ids(conn, q, pool)
    elif mode == "semantic":
        ids = await _semantic_ids(conn, q, pool)
    else:
        ids = _rrf(await _keyword_ids(conn, q, pool), await _semantic_ids(conn, q, pool))

    if not ids:
        return []

    conditions = ["id IN $ids", "deleted = false"]
    params: dict = {"ids": [_rid(i) for i in ids]}
    if sources:
        conditions.append("source IN $sources")
        params["sources"] = sources
    if types:
        conditions.append("type IN $types")
        params["types"] = types
    if since:
        conditions.append("occurred_at >= $since")
        params["since"] = since
    if until:
        conditions.append("occurred_at <= $until")
        params["until"] = until

    rows = await conn.query(f"SELECT * FROM cache_record WHERE {' AND '.join(conditions)}", params)
    order = {rid: i for i, rid in enumerate(ids)}
    recs = sorted((_row_to_record(r) for r in rows), key=lambda r: order.get(r.id, 1 << 30))
    return recs[offset : offset + limit]


async def get(record_id: str) -> CacheRecord | None:
    conn = get_connection()
    row = await _select_one(conn, _rid(record_id))
    return _row_to_record(row) if row else None


async def list_records(type=None, *, filters=None, sort="-occurred_at", limit=50, offset=0) -> list[CacheRecord]:
    conn = get_connection()
    conditions = ["deleted = false"]
    params: dict = {}
    if type:
        conditions.append("type = $type")
        params["type"] = type
    for key, val in (filters or {}).items():
        field_name = key.split("__", 1)[0]
        param_name = f"filter_{field_name}"
        conditions.append(f"{field_name} = ${param_name}")
        params[param_name] = val

    field_name = sort.lstrip("-")
    direction = "DESC" if sort.startswith("-") else "ASC"
    params["limit"] = limit
    params["offset"] = offset
    rows = await conn.query(
        f"SELECT * FROM cache_record WHERE {' AND '.join(conditions)} "
        f"ORDER BY {field_name} {direction} LIMIT $limit START $offset",
        params,
    )
    return [_row_to_record(r) for r in rows]


async def links(record_id: str, rel: str | None = None) -> list[dict]:
    conn = get_connection()
    rid = _rid(record_id)
    out = []
    fwd_q = "SELECT rel, out FROM linked_to WHERE in = $id" + (" AND rel = $rel" if rel else "")
    back_q = "SELECT rel, in FROM linked_to WHERE out = $id" + (" AND rel = $rel" if rel else "")
    params = {"id": rid, **({"rel": rel} if rel else {})}
    for row in await conn.query(fwd_q, params):
        out.append({"rel": row["rel"], "direction": "out", "target_id": _literal(row["out"])})
    for row in await conn.query(back_q, params):
        out.append({"rel": row["rel"], "direction": "in", "target_id": _literal(row["in"])})
    return out
