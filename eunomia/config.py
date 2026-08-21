import os
import sqlite3
from pathlib import Path

from . import db

VAULT_PATH = Path(os.environ.get("EUNOMIA_VAULT_PATH", "./vault")).expanduser()
DB_PATH = os.environ.get("EUNOMIA_DB_PATH", "eunomia.db")


def resolve_vault_path(conn: sqlite3.Connection) -> Path:
    """The Settings page's vault path (stored in app_settings) wins over the env var default."""
    override = db.get_setting(conn, "vault_path")
    return Path(override).expanduser() if override else VAULT_PATH
