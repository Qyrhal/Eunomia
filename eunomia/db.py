# SQLite storage for classified notes, connected-service credentials, buckets, and app settings.
# Single file, no ORM.

import json
import sqlite3
import time

from .models import Bucket, Credential, DEFAULT_BUCKETS, Note

SCHEMA = """
CREATE TABLE IF NOT EXISTS notes (
    path TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    bucket TEXT NOT NULL,
    tags TEXT NOT NULL,
    mtime REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    service TEXT NOT NULL,
    label TEXT NOT NULL,
    kind TEXT NOT NULL,
    secret_encrypted TEXT NOT NULL,
    account TEXT,
    created_at REAL NOT NULL,
    UNIQUE(service, label)
);

CREATE TABLE IF NOT EXISTS buckets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key TEXT UNIQUE NOT NULL,
    label TEXT NOT NULL,
    keywords TEXT NOT NULL,
    sort_order INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"""


def connect(db_path: str) -> sqlite3.Connection:
    conn = sqlite3.connect(db_path)
    conn.executescript(SCHEMA)
    conn.commit()
    _seed_default_buckets(conn)
    return conn


def _seed_default_buckets(conn: sqlite3.Connection) -> None:
    if conn.execute("SELECT 1 FROM buckets LIMIT 1").fetchone():
        return
    for order, (key, label, keywords) in enumerate(DEFAULT_BUCKETS):
        conn.execute(
            "INSERT INTO buckets (key, label, keywords, sort_order) VALUES (?, ?, ?, ?)",
            (key, label, json.dumps(keywords), order),
        )
    conn.commit()


def upsert_note(conn: sqlite3.Connection, note: Note) -> None:
    conn.execute(
        """INSERT INTO notes (path, title, bucket, tags, mtime)
           VALUES (?, ?, ?, ?, ?)
           ON CONFLICT(path) DO UPDATE SET
               title=excluded.title, bucket=excluded.bucket,
               tags=excluded.tags, mtime=excluded.mtime""",
        (note.path, note.title, note.bucket, json.dumps(note.tags), note.mtime),
    )
    conn.commit()


def sync_notes(conn: sqlite3.Connection, notes: list[Note]) -> None:
    """Upsert current notes and drop any DB rows for files no longer in the vault."""
    seen = {n.path for n in notes}
    for note in notes:
        upsert_note(conn, note)
    if seen:
        placeholders = ",".join("?" * len(seen))
        conn.execute(f"DELETE FROM notes WHERE path NOT IN ({placeholders})", tuple(seen))
    else:
        conn.execute("DELETE FROM notes")
    conn.commit()


def get_notes(conn: sqlite3.Connection, bucket: str | None = None) -> list[Note]:
    if bucket:
        rows = conn.execute(
            "SELECT path, title, bucket, tags, mtime FROM notes WHERE bucket = ? ORDER BY title",
            (bucket,),
        )
    else:
        rows = conn.execute("SELECT path, title, bucket, tags, mtime FROM notes ORDER BY bucket, title")
    return [
        Note(path=r[0], title=r[1], bucket=r[2], tags=json.loads(r[3]), mtime=r[4])
        for r in rows.fetchall()
    ]


def upsert_credential(
    conn: sqlite3.Connection,
    service: str,
    label: str,
    kind: str,
    secret_encrypted: str,
    account: str | None = None,
) -> None:
    conn.execute(
        """INSERT INTO credentials (service, label, kind, secret_encrypted, account, created_at)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(service, label) DO UPDATE SET
               kind=excluded.kind, secret_encrypted=excluded.secret_encrypted,
               account=excluded.account, created_at=excluded.created_at""",
        (service, label, kind, secret_encrypted, account, time.time()),
    )
    conn.commit()


def get_credentials(conn: sqlite3.Connection) -> list[Credential]:
    rows = conn.execute(
        "SELECT id, service, label, kind, secret_encrypted, account, created_at "
        "FROM credentials ORDER BY service, label"
    )
    return [
        Credential(
            id=r[0], service=r[1], label=r[2], kind=r[3],
            secret_encrypted=r[4], account=r[5], created_at=r[6],
        )
        for r in rows.fetchall()
    ]


def get_credential_secret(conn: sqlite3.Connection, credential_id: int) -> str | None:
    row = conn.execute(
        "SELECT secret_encrypted FROM credentials WHERE id = ?", (credential_id,)
    ).fetchone()
    return row[0] if row else None


def delete_credential(conn: sqlite3.Connection, credential_id: int) -> None:
    conn.execute("DELETE FROM credentials WHERE id = ?", (credential_id,))
    conn.commit()


def get_buckets(conn: sqlite3.Connection) -> list[Bucket]:
    rows = conn.execute("SELECT id, key, label, keywords, sort_order FROM buckets ORDER BY sort_order")
    return [
        Bucket(id=r[0], key=r[1], label=r[2], keywords=json.loads(r[3]), sort_order=r[4])
        for r in rows.fetchall()
    ]


def add_bucket(conn: sqlite3.Connection, key: str, label: str, keywords: list[str]) -> None:
    max_order = conn.execute("SELECT COALESCE(MAX(sort_order), -1) FROM buckets").fetchone()[0]
    conn.execute(
        "INSERT INTO buckets (key, label, keywords, sort_order) VALUES (?, ?, ?, ?)",
        (key, label, json.dumps(keywords), max_order + 1),
    )
    conn.commit()


def delete_bucket(conn: sqlite3.Connection, bucket_id: int) -> None:
    conn.execute("DELETE FROM buckets WHERE id = ? AND key != 'other'", (bucket_id,))
    conn.commit()


def get_setting(conn: sqlite3.Connection, key: str) -> str | None:
    row = conn.execute("SELECT value FROM app_settings WHERE key = ?", (key,)).fetchone()
    return row[0] if row else None


def set_setting(conn: sqlite3.Connection, key: str, value: str) -> None:
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?, ?) "
        "ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )
    conn.commit()
