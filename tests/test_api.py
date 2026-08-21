"""Smoke tests: real HTTP requests against the FastAPI app, backed by a temp vault + temp DB."""

import pytest
from fastapi.testclient import TestClient

from eunomia import config


@pytest.fixture
def client(vault, tmp_path, monkeypatch):
    monkeypatch.setattr(config, "VAULT_PATH", vault)
    monkeypatch.setattr(config, "DB_PATH", str(tmp_path / "test.db"))
    from eunomia.api import app

    return TestClient(app)


def test_health(client):
    resp = client.get("/health")
    assert resp.status_code == 200
    assert resp.json() == {"status": "ok"}


def test_sync_then_dashboard_end_to_end(client):
    sync_resp = client.post("/sync")
    assert sync_resp.status_code == 200
    assert sync_resp.json()["synced"] == 5

    dashboard_resp = client.get("/")
    assert dashboard_resp.status_code == 200
    assert "Tagged Note" in dashboard_resp.text
    assert "work" in dashboard_resp.text

    notes_resp = client.get("/api/notes")
    assert notes_resp.status_code == 200
    assert len(notes_resp.json()) == 5


def test_api_notes_filters_by_bucket(client):
    client.post("/sync")
    resp = client.get("/api/notes", params={"bucket": "uni"})
    assert resp.status_code == 200
    buckets = {n["bucket"] for n in resp.json()}
    assert buckets == {"uni"}


def test_dashboard_with_empty_vault(client, tmp_path, monkeypatch):
    empty_vault = tmp_path / "empty"
    empty_vault.mkdir()
    monkeypatch.setattr(config, "VAULT_PATH", empty_vault)

    client.post("/sync")
    resp = client.get("/")
    assert resp.status_code == 200
