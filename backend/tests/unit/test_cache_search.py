from cache.search import _rrf, get, links, list_records, search, set_embedding, upsert


def test_rrf_pure_fusion_no_db():
    # k=60 reciprocal rank fusion over fake ranked id lists -- no DB involved
    keyword = ["a", "b", "c"]
    semantic = ["c", "a", "d"]
    fused = _rrf(keyword, semantic)

    # "a" appears at rank 0 in both lists -- highest combined score
    assert fused[0] == "a"
    assert set(fused) == {"a", "b", "c", "d"}


def test_rrf_single_list_preserves_order():
    assert _rrf(["x", "y", "z"]) == ["x", "y", "z"]


def test_rrf_disjoint_lists_concatenate_by_score():
    fused = _rrf(["a"], ["b"])
    # both at rank 0 of their own list -- tied score, both present
    assert set(fused) == {"a", "b"}


async def test_upsert_idempotent_via_content_hash(surreal_db, owner):
    env = {
        "id": "src:kind:1",
        "source": "src",
        "type": "kind",
        "external_id": "1",
        "title": "Hello",
        "body_text": "World",
    }
    rec1, changed1 = await upsert(owner, env)
    assert changed1 is True
    assert rec1.id == "src:kind:1"

    rec2, changed2 = await upsert(owner, dict(env))
    assert changed2 is False
    assert rec2.content_hash == rec1.content_hash

    # changing content produces a new hash and a write
    env3 = dict(env, title="Changed")
    rec3, changed3 = await upsert(owner, env3)
    assert changed3 is True
    assert rec3.content_hash != rec1.content_hash


async def test_upsert_then_get(surreal_db, owner):
    rec, _ = await upsert(owner, {
        "id": "src:kind:2",
        "source": "src",
        "type": "kind",
        "external_id": "2",
        "title": "Findable",
        "body_text": "body",
    })
    fetched = await get(owner, rec.id)
    assert fetched is not None
    assert fetched.title == "Findable"

    assert await get(owner, "src:kind:does-not-exist") is None


async def test_search_round_trip_keyword_and_semantic(surreal_db, owner):
    from app.config import settings

    settings.EMBEDDINGS_BACKEND = "stub"
    from embeddings.service import embed

    rec_a, _ = await upsert(owner, {
        "id": "src:kind:a",
        "source": "src",
        "type": "kind",
        "external_id": "a",
        "title": "budget roadmap",
        "body_text": "quarterly budget planning",
    })
    await set_embedding(owner, rec_a.id, (await embed([f"{rec_a.title}\n{rec_a.body_text}"]))[0])

    rec_b, _ = await upsert(owner, {
        "id": "src:kind:b",
        "source": "src",
        "type": "kind",
        "external_id": "b",
        "title": "unrelated",
        "body_text": "something else entirely",
    })
    await set_embedding(owner, rec_b.id, (await embed([f"{rec_b.title}\n{rec_b.body_text}"]))[0])

    hybrid = await search(owner, "budget", mode="hybrid", limit=10)
    assert rec_a.id in [r.id for r in hybrid]

    keyword = await search(owner, "budget", mode="keyword", limit=10)
    assert rec_a.id in [r.id for r in keyword]
    assert rec_b.id not in [r.id for r in keyword]


async def test_list_records_filters_by_type(surreal_db, owner):
    await upsert(owner, {
        "id": "src:kindA:1", "source": "src", "type": "kindA", "external_id": "1",
        "title": "t1", "body_text": "b1",
    })
    await upsert(owner, {
        "id": "src:kindB:1", "source": "src", "type": "kindB", "external_id": "1",
        "title": "t2", "body_text": "b2",
    })
    rows = await list_records(owner, type="kindA")
    assert [r.id for r in rows] == ["src:kindA:1"]


async def test_links_forward_backward_and_idempotent(surreal_db, owner):
    target, _ = await upsert(owner, {
        "id": "src:kind:target", "source": "src", "type": "kind", "external_id": "target",
        "title": "t", "body_text": "b",
    })
    origin, _ = await upsert(owner, {
        "id": "src:kind:origin", "source": "src", "type": "kind", "external_id": "origin",
        "title": "t", "body_text": "b",
        "links": [{"rel": "mentions", "target": target.id}],
    })

    forward = await links(owner, origin.id)
    assert forward == [{"rel": "mentions", "direction": "out", "target_id": target.id}]

    backward = await links(owner, target.id)
    assert backward == [{"rel": "mentions", "direction": "in", "target_id": origin.id}]

    # re-upserting with the same link must not duplicate or raise
    await upsert(owner, {
        "id": "src:kind:origin", "source": "src", "type": "kind", "external_id": "origin",
        "title": "t2", "body_text": "b2",
        "links": [{"rel": "mentions", "target": target.id}],
    })
    assert await links(owner, origin.id) == forward
