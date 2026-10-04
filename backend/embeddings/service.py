"""Embedding service: one interface, swappable backend.

``embed(texts) -> list[list[float]]`` — always :data:`DIM`-dimensional,
order-preserving.

Backend is selected by ``settings.EMBEDDINGS_BACKEND`` (env-only, never a
DB-backed setting — see ``app/config.py``):
  - ``"openai"`` — real OpenAI embeddings API call, model
    ``text-embedding-3-small``, batched.
  - ``"stub"``   — deterministic SHA256-derived vector, zero network, for
    tests.

Results are memoized in the ``embed_cache`` SurrealDB table, keyed by
HMAC(ENCRYPTION_KEY, "backend:model:text") — ported from the old Django
``EmbedCache`` model's memo logic.
"""

import hashlib
import hmac

from surrealdb import RecordID

from app.config import settings
from app.db import db as get_connection

DIM = 1536
BATCH = 64

_async_openai_client = None


def _hmac(s: str) -> str:
    key = (settings.ENCRYPTION_KEY or "").encode()
    return hmac.new(key, s.encode(), hashlib.sha256).hexdigest()


def _stub_vec(text: str) -> list[float]:
    h = hashlib.sha256(text.encode()).digest()
    raw = (h * ((DIM // len(h)) + 1))[:DIM]
    return [(b / 255.0) * 2 - 1 for b in raw]


def _embed_stub(texts: list[str]) -> list[list[float]]:
    return [_stub_vec(t) for t in texts]


def _openai_client():
    global _async_openai_client
    if _async_openai_client is None:
        from openai import AsyncOpenAI

        _async_openai_client = AsyncOpenAI(api_key=settings.OPENAI_API_KEY)
    return _async_openai_client


async def _embed_openai(texts: list[str]) -> list[list[float]]:
    client = _openai_client()
    out: list[list[float]] = []
    for i in range(0, len(texts), BATCH):
        chunk = texts[i : i + BATCH]
        resp = await client.embeddings.create(model="text-embedding-3-small", input=chunk)
        data = sorted(resp.data, key=lambda d: d.index)
        out.extend(d.embedding for d in data)
    for v in out:
        if len(v) != DIM:
            raise RuntimeError(f"OpenAI returned dim {len(v)}, expected {DIM}.")
    return out


def dim() -> int:
    return DIM


async def embed(texts: list[str]) -> list[list[float]]:
    """Embed `texts`, using the SurrealDB-backed memo for ones seen before."""
    if not texts:
        return []

    backend = settings.EMBEDDINGS_BACKEND
    model = "text-embedding-3-small" if backend == "openai" else "stub"
    keys = [_hmac(f"{backend}:{model}:{t}") for t in texts]

    conn = get_connection()
    rows = await conn.query("SELECT text_hmac, vector FROM embed_cache WHERE text_hmac IN $keys", {"keys": keys})
    cached: dict[str, list[float]] = {r["text_hmac"]: r["vector"] for r in rows}

    missing_idx = [i for i, k in enumerate(keys) if k not in cached]
    if missing_idx:
        fresh_texts = [texts[i] for i in missing_idx]
        vecs = await _embed_openai(fresh_texts) if backend == "openai" else _embed_stub(fresh_texts)
        for i, v in zip(missing_idx, vecs):
            k = keys[i]
            await conn.query(
                "UPSERT $id SET text_hmac = $hmac, vector = $vector",
                {"id": RecordID("embed_cache", k), "hmac": k, "vector": v},
            )
            cached[k] = v

    return [cached[k] for k in keys]
