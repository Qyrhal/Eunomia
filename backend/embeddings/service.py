"""Embedding service (#29): one interface, swappable backend.

`embed(texts) -> list[list[float]]` — always :data:`DIM`-dimensional, order-preserving.

Backends, chosen by ``AppSettings.embedding_backend``:
  - ``api``   — POST ``{base_url}/embeddings`` (OpenAI-compatible; also fits Ollama's
                ``/v1/embeddings``). No extra Python deps. The default.
  - ``local`` — sentence-transformers ``BAAI/bge-small-en-v1.5``. Needs the
                ``local-embeddings`` extra (``uv sync --extra local-embeddings``).
  - ``stub``  — deterministic hash vector, for tests / offline.

Switching backend or model invalidates stored vectors — run
``manage.py rebuild_embeddings`` (see #30/#38 wiring).
"""

import hashlib
import hmac

import httpx
from django.conf import settings

DIM = 384
BATCH = 64

_local_model = None


def _cfg():
    from connectors.models import AppSettings

    row = AppSettings.load()
    return {
        "backend": getattr(row, "embedding_backend", None) or "stub",
        "base_url": (getattr(row, "llm_base_url", "") or "").rstrip("/"),
        "model": getattr(row, "embedding_model", "") or "",
        "api_key": getattr(row, "llm_api_key", "") or "",
    }


def _hmac(s: str) -> str:
    key = (getattr(settings, "ENCRYPTION_KEY", "") or "").encode()
    return hmac.new(key, s.encode(), hashlib.sha256).hexdigest()


def _stub_vec(text: str) -> list[float]:
    h = hashlib.sha256(text.encode()).digest()
    raw = (h * ((DIM // len(h)) + 1))[:DIM]
    return [(b / 255.0) * 2 - 1 for b in raw]


def _embed_stub(texts):
    return [_stub_vec(t) for t in texts]


def _embed_local(texts):
    global _local_model
    if _local_model is None:
        try:
            from sentence_transformers import SentenceTransformer
        except ImportError as e:  # pragma: no cover
            raise RuntimeError(
                "embedding_backend='local' needs the local-embeddings extra: "
                "`uv sync --extra local-embeddings`"
            ) from e
        _local_model = SentenceTransformer("BAAI/bge-small-en-v1.5")
    return [v.tolist() for v in _local_model.encode(list(texts), normalize_embeddings=True)]


def _embed_api(texts, cfg):
    if not cfg["base_url"] or not cfg["model"]:
        raise RuntimeError("embedding_backend='api' needs llm_base_url + embedding_model in Settings.")
    headers = {"Authorization": f"Bearer {cfg['api_key']}"} if cfg["api_key"] else {}
    out: list[list[float]] = []
    with httpx.Client(base_url=cfg["base_url"], headers=headers, timeout=60) as client:
        for i in range(0, len(texts), BATCH):
            chunk = texts[i : i + BATCH]
            r = client.post("/embeddings", json={"model": cfg["model"], "input": chunk})
            r.raise_for_status()
            data = sorted(r.json()["data"], key=lambda d: d["index"])
            out.extend(d["embedding"] for d in data)
    for v in out:
        if len(v) != DIM:
            raise RuntimeError(f"API returned dim {len(v)}, expected {DIM} — pick a {DIM}-dim model.")
    return out


def dim() -> int:
    return DIM


def embed(texts: list[str]) -> list[list[float]]:
    """Embed `texts`, using the on-disk memo for ones seen before."""
    if not texts:
        return []
    cfg = _cfg()
    from .models import EmbedCache

    keys = [_hmac(f"{cfg['backend']}:{cfg['model']}:{t}") for t in texts]
    cached = {c.text_hmac: c.vector for c in EmbedCache.objects.filter(text_hmac__in=keys)}

    missing_idx = [i for i, k in enumerate(keys) if k not in cached]
    if missing_idx:
        fresh_texts = [texts[i] for i in missing_idx]
        backend = cfg["backend"]
        if backend == "api" and not (cfg["base_url"] and cfg["model"]):
            # unconfigured API backend: degrade to stub so search still works
            # out of the box. Configuring llm_base_url + embedding_model upgrades it.
            backend = "stub"
        if backend == "local":
            vecs = _embed_local(fresh_texts)
        elif backend == "api":
            vecs = _embed_api(fresh_texts, cfg)
        else:
            vecs = _embed_stub(fresh_texts)
        EmbedCache.objects.bulk_create(
            [EmbedCache(text_hmac=keys[i], vector=v) for i, v in zip(missing_idx, vecs)],
            ignore_conflicts=True,
        )
        for i, v in zip(missing_idx, vecs):
            cached[keys[i]] = v

    return [cached[k] for k in keys]
