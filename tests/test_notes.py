"""Smoke tests for note creation and the vault-configured gate."""

from eunomia import config


def test_dashboard_shows_new_note_button_when_vault_configured(client):
    resp = client.get("/")
    assert resp.status_code == 200
    assert 'id="new-note-btn"' in resp.text


def test_dashboard_prompts_for_vault_when_not_configured(client, tmp_path, monkeypatch):
    monkeypatch.setattr(config, "VAULT_PATH", tmp_path / "does-not-exist")
    resp = client.get("/")
    assert resp.status_code == 200
    assert 'id="new-note-btn"' not in resp.text
    assert "Set up vault to add notes" in resp.text


def test_create_note_success(client):
    resp = client.post("/notes", data={"title": "My New Idea", "bucket": "work"})
    assert resp.status_code == 200
    assert resp.json() == {"created": True}

    notes_resp = client.get("/api/notes")
    titles = [n["title"] for n in notes_resp.json()]
    assert "My New Idea" in titles


def test_create_note_rejects_when_vault_not_configured(client, tmp_path, monkeypatch):
    monkeypatch.setattr(config, "VAULT_PATH", tmp_path / "does-not-exist")
    resp = client.post("/notes", data={"title": "Orphan Note", "bucket": "work"})
    assert resp.status_code == 400
    assert "Vault not found" in resp.json()["detail"]


def test_create_note_rejects_blank_title(client):
    resp = client.post("/notes", data={"title": "   ", "bucket": "work"})
    assert resp.status_code == 400


def test_create_note_rejects_duplicate_title(client):
    client.post("/notes", data={"title": "Duplicate", "bucket": "work"})
    resp = client.post("/notes", data={"title": "Duplicate", "bucket": "work"})
    assert resp.status_code == 409


def test_sync_rejects_when_vault_not_configured(client, tmp_path, monkeypatch):
    monkeypatch.setattr(config, "VAULT_PATH", tmp_path / "does-not-exist")
    resp = client.post("/sync")
    assert resp.status_code == 400
