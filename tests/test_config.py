from pathlib import Path

from eunomia import config, db


def test_resolve_vault_path_falls_back_to_env_default(monkeypatch):
    monkeypatch.setattr(config, "VAULT_PATH", Path("/env/default"))
    conn = db.connect(":memory:")
    assert config.resolve_vault_path(conn) == Path("/env/default")


def test_resolve_vault_path_prefers_db_override(monkeypatch):
    monkeypatch.setattr(config, "VAULT_PATH", Path("/env/default"))
    conn = db.connect(":memory:")
    db.set_setting(conn, "vault_path", "/custom/vault")

    assert config.resolve_vault_path(conn) == Path("/custom/vault")
