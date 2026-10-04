from tools.registry import all_tools


async def test_catalogue_matches_registry(client):
    r = await client.get("/api/tools")
    assert r.status_code == 200
    body = r.json()
    assert set(body.keys()) == set(all_tools().keys())
    assert body["search"] == all_tools()["search"]["schema"]


async def test_catalogue_is_unauthenticated(client):
    r = await client.get("/api/tools")
    assert r.status_code == 200


async def test_call_unknown_tool_is_404(client):
    await client.post("/api/auth/register", json={"email": "tool1@b.com", "password": "testpass123"})
    r = await client.post("/api/tools/does_not_exist", json={})
    assert r.status_code == 404


async def test_call_requires_auth(client):
    r = await client.post("/api/tools/list", json={})
    assert r.status_code == 401


async def test_call_round_trip(client):
    await client.post("/api/auth/register", json={"email": "tool2@b.com", "password": "testpass123"})
    r = await client.post("/api/tools/list", json={})
    assert r.status_code == 200
    assert r.json() == {"results": []}
