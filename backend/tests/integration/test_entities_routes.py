"""REST round-trip for /api/entities, owner-scoped -- mirrors
`tests/unit/test_auth_multiuser.py`'s cross-owner isolation pattern."""

from entities.service import add_relation, upsert_entity


async def _owner_id(surreal_db, email: str):
    rows = await surreal_db.query("SELECT * FROM user WHERE email = $email LIMIT 1", {"email": email})
    return rows[0]["id"]


async def test_entities_require_auth(client):
    r = await client.get("/api/entities")
    assert r.status_code == 401
    r = await client.get("/api/entities/graph")
    assert r.status_code == 401


async def test_list_get_and_graph_round_trip(client, surreal_db):
    await client.post("/api/auth/register", json={"email": "ent1@b.com", "password": "testpass123"})
    owner = await _owner_id(surreal_db, "ent1@b.com")

    alex = await upsert_entity(owner, "person", "Alex")
    acme = await upsert_entity(owner, "organisation", "Acme")
    await add_relation(owner, alex["id"], acme["id"], "works_at")

    r = await client.get("/api/entities")
    assert r.status_code == 200
    assert {e["name"] for e in r.json()} == {"Alex", "Acme"}

    r = await client.get("/api/entities", params={"kind": "person"})
    assert r.status_code == 200
    assert [e["name"] for e in r.json()] == ["Alex"]

    r = await client.get("/api/entities", params={"kind": "bogus"})
    assert r.status_code == 400

    entity_id = str(alex["id"])
    r = await client.get(f"/api/entities/{entity_id}")
    assert r.status_code == 200
    body = r.json()
    assert body["name"] == "Alex"
    assert len(body["relations"]) == 1
    assert body["relations"][0]["label"] == "works_at"

    r = await client.get("/api/entities/graph")
    assert r.status_code == 200
    g = r.json()
    assert len(g["nodes"]) == 2
    assert len(g["edges"]) == 1


async def test_get_unknown_entity_is_404(client):
    await client.post("/api/auth/register", json={"email": "ent2@b.com", "password": "testpass123"})
    r = await client.get("/api/entities/person:doesnotexist")
    assert r.status_code == 404


async def test_two_users_do_not_see_each_others_entities(client, surreal_db):
    await client.post("/api/auth/register", json={"email": "ent_a@b.com", "password": "testpass123"})
    owner_a = await _owner_id(surreal_db, "ent_a@b.com")
    alice = await upsert_entity(owner_a, "person", "Alice")

    await client.post("/api/auth/logout")
    await client.post("/api/auth/register", json={"email": "ent_b@b.com", "password": "testpass123"})
    owner_b = await _owner_id(surreal_db, "ent_b@b.com")
    await upsert_entity(owner_b, "person", "Bob")

    r = await client.get("/api/entities")
    assert r.status_code == 200
    names = {e["name"] for e in r.json()}
    assert names == {"Bob"}

    # user B's session cannot fetch user A's entity by id either
    r = await client.get(f"/api/entities/{alice['id']}")
    assert r.status_code == 404

    r = await client.get("/api/entities/graph")
    assert len(r.json()["nodes"]) == 1
