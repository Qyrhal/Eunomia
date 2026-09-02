"""Cache write + query API (#6). The only module that knows FTS5 / sqlite-vec
exist — a future Postgres+pgvector backend implements these same signatures.
"""

import hashlib
import json

from django.db import connection, transaction
from django.utils import timezone

from embeddings.service import DIM

from .models import CacheLink, CacheRecord

_RRF_K = 60


def _hash_envelope(env: dict) -> str:
    keys = ("title", "body_text", "url", "occurred_at", "payload", "deleted")
    blob = json.dumps({k: env.get(k) for k in keys}, sort_keys=True, default=str)
    return hashlib.sha256(blob.encode()).hexdigest()


def _fts_write(record_id: str, title: str, body: str):
    with connection.cursor() as cur:
        cur.execute("DELETE FROM cache_record_fts WHERE record_id = %s", [record_id])
        cur.execute(
            "INSERT INTO cache_record_fts (record_id, title, body_text) VALUES (%s, %s, %s)",
            [record_id, title or "", body or ""],
        )


def upsert(env: dict) -> tuple[CacheRecord, bool]:
    """Insert or update one envelope. Returns (record, changed)."""
    now = timezone.now()
    rid = env["id"]
    h = _hash_envelope(env)
    existing = CacheRecord.objects.filter(pk=rid).first()
    if existing and existing.content_hash == h and not existing.deleted:
        CacheRecord.objects.filter(pk=rid).update(ingested_at=now)
        return existing, False

    with transaction.atomic():
        rec, _ = CacheRecord.objects.update_or_create(
            pk=rid,
            defaults=dict(
                source=env["source"],
                type=env["type"],
                external_id=env["external_id"],
                title=env.get("title", ""),
                body_text=env.get("body_text", ""),
                occurred_at=env.get("occurred_at"),
                url=env.get("url", ""),
                payload=env.get("payload", {}),
                content_hash=h,
                ingested_at=now,
                updated_at=now,
                deleted=bool(env.get("deleted", False)),
                has_embedding=False if not existing else existing.has_embedding,
            ),
        )
        _fts_write(rid, rec.title, rec.body_text)
        _reconcile_links(rid, env.get("links", []))
    return rec, True


def _reconcile_links(source_id: str, links: list[dict]):
    CacheLink.objects.filter(source_id=source_id, origin=CacheLink.ORIGIN_SYNC).delete()
    CacheLink.objects.bulk_create(
        [
            CacheLink(source_id=source_id, rel=l["rel"], target_id=l["target"], origin=CacheLink.ORIGIN_SYNC)
            for l in links
        ],
        ignore_conflicts=True,
    )


def set_embedding(record_id: str, vector: list[float]):
    if len(vector) != DIM:
        raise ValueError(f"embedding dim {len(vector)} != {DIM}")
    with connection.cursor() as cur:
        cur.execute("DELETE FROM cache_vec WHERE record_id = %s", [record_id])
        cur.execute(
            "INSERT INTO cache_vec (record_id, embedding) VALUES (%s, %s)",
            [record_id, json.dumps(vector)],
        )
    CacheRecord.objects.filter(pk=record_id).update(has_embedding=True)


def _keyword_ids(q: str, limit: int) -> list[str]:
    with connection.cursor() as cur:
        cur.execute(
            "SELECT record_id FROM cache_record_fts WHERE cache_record_fts MATCH %s "
            "ORDER BY bm25(cache_record_fts) LIMIT %s",
            [q, limit],
        )
        return [r[0] for r in cur.fetchall()]


def _semantic_ids(q: str, limit: int) -> list[str]:
    from embeddings.service import embed

    vec = embed([q])[0]
    with connection.cursor() as cur:
        cur.execute(
            "SELECT record_id FROM cache_vec WHERE embedding MATCH %s AND k = %s ORDER BY distance",
            [json.dumps(vec), limit],
        )
        return [r[0] for r in cur.fetchall()]


def _rrf(*ranked_lists: list[str]) -> list[str]:
    scores: dict[str, float] = {}
    for lst in ranked_lists:
        for rank, rid in enumerate(lst):
            scores[rid] = scores.get(rid, 0.0) + 1.0 / (_RRF_K + rank + 1)
    return [rid for rid, _ in sorted(scores.items(), key=lambda kv: -kv[1])]


def search(q, *, sources=None, types=None, since=None, until=None, mode="hybrid", limit=20, offset=0):
    pool = max(limit * 4, 40)
    if mode == "keyword":
        ids = _keyword_ids(q, pool)
    elif mode == "semantic":
        ids = _semantic_ids(q, pool)
    else:
        ids = _rrf(_keyword_ids(q, pool), _semantic_ids(q, pool))

    qs = CacheRecord.objects.filter(pk__in=ids, deleted=False)
    if sources:
        qs = qs.filter(source__in=sources)
    if types:
        qs = qs.filter(type__in=types)
    if since:
        qs = qs.filter(occurred_at__gte=since)
    if until:
        qs = qs.filter(occurred_at__lte=until)

    order = {rid: i for i, rid in enumerate(ids)}
    rows = sorted(qs, key=lambda r: order.get(r.pk, 1 << 30))
    return rows[offset : offset + limit]


def get(record_id: str) -> CacheRecord | None:
    return CacheRecord.objects.filter(pk=record_id).first()


def list_records(type=None, *, filters=None, sort="-occurred_at", limit=50, offset=0):
    qs = CacheRecord.objects.filter(deleted=False)
    if type:
        qs = qs.filter(type=type)
    for key, val in (filters or {}).items():
        qs = qs.filter(**{key: val})
    return list(qs.order_by(sort)[offset : offset + limit])


def links(record_id: str, rel: str | None = None):
    out = []
    fwd = CacheLink.objects.filter(source_id=record_id)
    back = CacheLink.objects.filter(target_id=record_id)
    if rel:
        fwd = fwd.filter(rel=rel)
        back = back.filter(rel=rel)
    for l in fwd:
        out.append({"rel": l.rel, "direction": "out", "target_id": l.target_id})
    for l in back:
        out.append({"rel": l.rel, "direction": "in", "target_id": l.source_id})
    return out
