import pytest

from app.config import settings
from embeddings.service import DIM, _stub_vec, embed


def test_stub_vec_deterministic():
    assert _stub_vec("hello") == _stub_vec("hello")
    assert _stub_vec("hello") != _stub_vec("world")
    assert len(_stub_vec("hello")) == DIM


@pytest.mark.usefixtures("surreal_db")
async def test_stub_backend_returns_dim_vectors(monkeypatch):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")
    vecs = await embed(["hello", "world"])
    assert len(vecs) == 2
    assert all(len(v) == DIM for v in vecs)
    assert vecs[0] != vecs[1]


async def test_embed_empty_list_short_circuits():
    # no DB needed -- embed([]) returns before touching the connection
    assert await embed([]) == []


async def test_memo_cache_hit_skips_recompute(monkeypatch, surreal_db):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    calls = {"n": 0}
    import embeddings.service as svc

    real_stub = svc._embed_stub

    def counting_stub(texts):
        calls["n"] += 1
        return real_stub(texts)

    monkeypatch.setattr(svc, "_embed_stub", counting_stub)

    first = await embed(["repeat me"])
    assert calls["n"] == 1

    second = await embed(["repeat me"])
    # second call is a full memo hit -- the stub backend is never invoked again
    assert calls["n"] == 1
    assert first == second


async def test_memo_cache_partial_miss_only_computes_new(monkeypatch, surreal_db):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    await embed(["seen before"])

    import embeddings.service as svc

    calls = {"texts": None}
    real_stub = svc._embed_stub

    def recording_stub(texts):
        calls["texts"] = list(texts)
        return real_stub(texts)

    monkeypatch.setattr(svc, "_embed_stub", recording_stub)

    vecs = await embed(["seen before", "brand new"])
    assert calls["texts"] == ["brand new"]
    assert len(vecs) == 2
