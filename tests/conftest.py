from pathlib import Path

import pytest
from cryptography.fernet import Fernet
from fastapi.testclient import TestClient

from eunomia import config


@pytest.fixture
def vault(tmp_path: Path) -> Path:
    """A small fake Obsidian vault covering each classification path."""
    (tmp_path / "Uni").mkdir()
    (tmp_path / "Work").mkdir()

    (tmp_path / "tagged.md").write_text(
        "---\ntags: [work]\n---\n# Tagged Note\nSome content.\n"
    )
    (tmp_path / "Uni" / "lecture.md").write_text("# Lecture Notes\nNo frontmatter, folder says uni.\n")
    (tmp_path / "hashtag.md").write_text("# Hashtag Note\nThis is #business related.\n")
    (tmp_path / "plain.md").write_text("# Plain Note\nNothing to classify this.\n")
    (tmp_path / "bad_frontmatter.md").write_text("---\n: not valid yaml : [\n---\n# Broken\nBody text.\n")

    return tmp_path


@pytest.fixture
def client(vault, tmp_path, monkeypatch):
    monkeypatch.setattr(config, "VAULT_PATH", vault)
    monkeypatch.setattr(config, "DB_PATH", str(tmp_path / "test.db"))
    monkeypatch.setenv("EUNOMIA_MASTER_KEY", Fernet.generate_key().decode())
    from eunomia.api import app

    return TestClient(app)
