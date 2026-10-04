from entities.service import add_memory, add_relation, get_entity, graph, list_entities, upsert_entity


async def test_upsert_entity_dedup_by_name_case_insensitive(surreal_db, owner):
    first = await upsert_entity(owner, "person", "Alex")
    second = await upsert_entity(owner, "person", "alex")
    assert first["id"] == second["id"]

    rows = await list_entities(owner, kind="person")
    assert len(rows) == 1


async def test_upsert_entity_dedup_by_alias(surreal_db, owner):
    first = await upsert_entity(owner, "person", "Alexander", aliases=["Alex", "Xander"])
    second = await upsert_entity(owner, "person", "alex")
    assert first["id"] == second["id"]

    rows = await list_entities(owner, kind="person")
    assert len(rows) == 1
    assert set(rows[0]["aliases"]) == {"Alex", "Xander"}


async def test_upsert_entity_merges_new_aliases(surreal_db, owner):
    first = await upsert_entity(owner, "person", "Alex", aliases=["Al"])
    again = await upsert_entity(owner, "person", "Alex", aliases=["AJ"])
    assert first["id"] == again["id"]
    assert set(again["aliases"]) == {"Al", "AJ"}


async def test_upsert_entity_different_names_create_distinct_rows(surreal_db, owner):
    alex = await upsert_entity(owner, "person", "Alex")
    sam = await upsert_entity(owner, "person", "Sam")
    assert alex["id"] != sam["id"]
    assert len(await list_entities(owner, kind="person")) == 2


async def test_add_memory(surreal_db, owner):
    from cache.search import upsert

    rec, _ = await upsert(
        owner,
        {
            "id": "heypocket:transcript:t1",
            "source": "heypocket",
            "type": "transcript",
            "external_id": "t1",
            "title": "Call",
            "body_text": "Alex works at Acme.",
        },
    )
    alex = await upsert_entity(owner, "person", "Alex")
    mem = await add_memory(owner, alex["id"], "Works at Acme.", rec.id)
    assert mem["text"] == "Works at Acme."
    assert mem["subject"] == alex["id"]

    entity = await get_entity(owner, alex["id"])
    assert len(entity["memory"]) == 1
    assert entity["memory"][0]["text"] == "Works at Acme."


async def test_add_relation_idempotent(surreal_db, owner):
    alex = await upsert_entity(owner, "person", "Alex")
    acme = await upsert_entity(owner, "organisation", "Acme")

    rel1 = await add_relation(owner, alex["id"], acme["id"], "works_at")
    rel2 = await add_relation(owner, alex["id"], acme["id"], "works_at")
    assert rel1["id"] == rel2["id"]

    entity = await get_entity(owner, alex["id"])
    assert len(entity["relations"]) == 1
    assert entity["relations"][0]["label"] == "works_at"
    assert entity["relations"][0]["direction"] == "out"


async def test_get_entity_not_found(surreal_db, owner):
    from surrealdb import RecordID

    assert await get_entity(owner, RecordID("person", "doesnotexist")) is None


async def test_graph_shape(surreal_db, owner):
    alex = await upsert_entity(owner, "person", "Alex")
    acme = await upsert_entity(owner, "organisation", "Acme")
    await add_relation(owner, alex["id"], acme["id"], "works_at")

    g = await graph(owner)
    assert set(g.keys()) == {"nodes", "edges"}
    assert len(g["nodes"]) == 2
    for node in g["nodes"]:
        assert set(node.keys()) == {"id", "kind", "name"}
        assert isinstance(node["id"], str)
    assert len(g["edges"]) == 1
    edge = g["edges"][0]
    assert set(edge.keys()) == {"source", "target", "label"}
    assert isinstance(edge["source"], str) and isinstance(edge["target"], str)
    assert edge["label"] == "works_at"


async def test_list_entities_all_kinds(surreal_db, owner):
    await upsert_entity(owner, "person", "Alex")
    await upsert_entity(owner, "organisation", "Acme")
    await upsert_entity(owner, "location", "Sydney")

    rows = await list_entities(owner)
    assert {r["kind"] for r in rows} == {"person", "organisation", "location"}
    assert len(rows) == 3
