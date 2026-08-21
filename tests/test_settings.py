"""Smoke tests for the settings page: API-key storage, credential deletion, and the OAuth
sign-in flow (token exchange is mocked — no real network calls to GitHub/Slack/Linear)."""

from eunomia import oauth


def test_settings_page_loads(client):
    resp = client.get("/settings")
    assert resp.status_code == 200
    assert "GitHub" in resp.text
    assert "Sign in" in resp.text
    assert "or add an API key instead" in resp.text
    assert "Buckets" in resp.text
    assert "Vault" in resp.text


def test_settings_shows_default_buckets(client):
    resp = client.get("/settings")
    for label in ["Uni", "Work", "Business", "Other"]:
        assert label in resp.text


def test_set_vault_path(client, tmp_path):
    new_vault = tmp_path / "my-vault"
    resp = client.post("/settings/vault", data={"path": str(new_vault)})
    assert resp.status_code == 200
    assert new_vault.exists()
    assert str(new_vault) in resp.text


def test_set_vault_path_requires_value(client):
    resp = client.post("/settings/vault", data={"path": "  "})
    assert "required" in resp.text


def test_add_bucket_shows_up_in_settings(client):
    resp = client.post(
        "/settings/buckets",
        data={"key": "side-project", "label": "Side Project", "keywords": "startup-x, sx"},
    )
    assert resp.status_code == 200
    assert "Side Project" in resp.text
    assert "startup-x" in resp.text


def test_add_bucket_requires_key_and_label(client):
    resp = client.post("/settings/buckets", data={"key": "", "label": "", "keywords": ""})
    assert "required" in resp.text


def test_add_duplicate_bucket_key_shows_error(client):
    resp = client.post("/settings/buckets", data={"key": "work", "label": "Work Again", "keywords": ""})
    assert "already exists" in resp.text


def test_delete_bucket(client):
    from eunomia import config, db

    client.post("/settings/buckets", data={"key": "side-project", "label": "Side Project", "keywords": ""})
    conn = db.connect(config.DB_PATH)
    bucket_id = next(b.id for b in db.get_buckets(conn) if b.key == "side-project")
    conn.close()

    resp = client.post(f"/settings/buckets/{bucket_id}/delete")
    assert resp.status_code == 200
    assert f"/settings/buckets/{bucket_id}/delete" not in resp.text


def test_cannot_delete_other_bucket(client):
    from eunomia import config, db

    conn = db.connect(config.DB_PATH)
    other_id = next(b.id for b in db.get_buckets(conn) if b.key == "other")
    conn.close()

    resp = client.post(f"/settings/buckets/{other_id}/delete")
    assert resp.status_code == 200
    assert "Other" in resp.text


def test_add_api_key_shows_as_connected(client):
    resp = client.post(
        "/settings/api-key", data={"service": "github", "label": "Work", "key": "ghp_abc123"}
    )
    assert resp.status_code == 200  # TestClient follows the redirect
    assert "API key set" in resp.text


def test_add_llm_provider_key(client):
    resp = client.post(
        "/settings/api-key", data={"service": "openai", "label": "default", "key": "sk-abc"}
    )
    assert resp.status_code == 200
    assert "openai" in resp.text


def test_add_api_key_requires_all_fields(client):
    resp = client.post("/settings/api-key", data={"service": "", "label": "Work", "key": "x"})
    assert "required" in resp.text


def test_delete_credential(client):
    client.post("/settings/api-key", data={"service": "heypocket", "label": "Personal", "key": "hp_key"})
    settings_resp = client.get("/settings")
    assert "API key set" in settings_resp.text

    # Find the credential id from the connections listing via a fresh DB read.
    from eunomia import config, db

    conn = db.connect(config.DB_PATH)
    cred_id = db.get_credentials(conn)[0].id
    conn.close()

    resp = client.post(f"/settings/credentials/{cred_id}/delete")
    assert resp.status_code == 200
    assert "Not connected" in resp.text


def test_oauth_start_redirects_to_settings_when_not_configured(client, monkeypatch):
    monkeypatch.delenv("EUNOMIA_GITHUB_CLIENT_ID", raising=False)
    monkeypatch.delenv("EUNOMIA_GITHUB_CLIENT_SECRET", raising=False)

    resp = client.get("/oauth/github/start", params={"label": "Work"}, follow_redirects=False)
    assert resp.status_code == 303
    assert "/settings" in resp.headers["location"]


def test_oauth_start_redirects_to_github_when_configured(client, monkeypatch):
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_ID", "id123")
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_SECRET", "sec123")

    resp = client.get("/oauth/github/start", params={"label": "Work"}, follow_redirects=False)
    assert resp.status_code == 303
    assert resp.headers["location"].startswith("https://github.com/login/oauth/authorize")


def test_oauth_callback_stores_credential(client, monkeypatch):
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_ID", "id123")
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_SECRET", "sec123")

    start_resp = client.get("/oauth/github/start", params={"label": "Work"}, follow_redirects=False)
    location = start_resp.headers["location"]
    state = location.split("state=")[1].split("&")[0]

    monkeypatch.setattr(oauth, "exchange_code_for_token", lambda *a, **kw: "gho_faketoken")

    callback_resp = client.get(
        "/oauth/github/callback", params={"code": "codeXYZ", "state": state}
    )
    assert callback_resp.status_code == 200
    assert "Signed in" in callback_resp.text


def test_oauth_callback_rejects_unknown_state(client, monkeypatch):
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_ID", "id123")
    monkeypatch.setenv("EUNOMIA_GITHUB_CLIENT_SECRET", "sec123")

    resp = client.get(
        "/oauth/github/callback", params={"code": "codeXYZ", "state": "not-a-real-state"}
    )
    assert resp.status_code == 200
    assert "expired or invalid" in resp.text
