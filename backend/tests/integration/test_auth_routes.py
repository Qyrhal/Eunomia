async def test_register_sets_cookie_and_returns_user(client):
    r = await client.post("/api/auth/register", json={"email": "a@b.com", "password": "testpass123"})
    assert r.status_code == 200
    body = r.json()
    assert body["email"] == "a@b.com"
    assert body["onboarded"] is False
    assert "eunomia_session" in r.cookies


async def test_register_duplicate_email_is_409(client):
    await client.post("/api/auth/register", json={"email": "dup@b.com", "password": "testpass123"})
    r = await client.post("/api/auth/register", json={"email": "dup@b.com", "password": "other-pass"})
    assert r.status_code == 409


async def test_login_wrong_password_is_401(client):
    await client.post("/api/auth/register", json={"email": "c@b.com", "password": "rightpass"})
    r = await client.post("/api/auth/login", json={"email": "c@b.com", "password": "wrongpass"})
    assert r.status_code == 401


async def test_login_then_me_roundtrip(client):
    await client.post("/api/auth/register", json={"email": "d@b.com", "password": "testpass123"})
    r = await client.post("/api/auth/login", json={"email": "d@b.com", "password": "testpass123"})
    assert r.status_code == 200
    assert "eunomia_session" in r.cookies

    me = await client.get("/api/auth/me")
    assert me.status_code == 200
    assert me.json()["email"] == "d@b.com"


async def test_me_without_cookie_is_401(client):
    r = await client.get("/api/auth/me")
    assert r.status_code == 401


async def test_logout_clears_cookie(client):
    await client.post("/api/auth/register", json={"email": "e@b.com", "password": "testpass123"})
    await client.post("/api/auth/logout")
    me = await client.get("/api/auth/me")
    assert me.status_code == 401


async def test_token_issues_api_token(client):
    await client.post("/api/auth/register", json={"email": "f@b.com", "password": "testpass123"})
    r = await client.post("/api/auth/token")
    assert r.status_code == 200
    token = r.json()["token"]
    assert isinstance(token, str) and len(token) > 10

    # the token works as a Bearer credential (current_user tries Bearer first)
    r2 = await client.get("/api/auth/me", headers={"Authorization": f"Bearer {token}"})
    assert r2.status_code == 200
    assert r2.json()["email"] == "f@b.com"


async def test_bootstrap_reflects_user_existence(client):
    r = await client.get("/api/auth/bootstrap")
    assert r.json() == {"has_users": False}

    await client.post("/api/auth/register", json={"email": "g@b.com", "password": "testpass123"})
    r = await client.get("/api/auth/bootstrap")
    assert r.json() == {"has_users": True}
