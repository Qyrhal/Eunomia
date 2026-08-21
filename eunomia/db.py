# SQLite storage for classified notes. Single file, no ORM.

import json
import sqlite3

from .models import Note

SCHEMA = """
CREATE TABLE IF NOT EXISTS notes (
    path TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    bucket TEXT NOT NULL,
    tags TEXT NOT NULL,
    mtime REAL NOT NULL
);
"""


def connect(db_path: str) -> sqlite3.Connection:
    conn = sqlite3.connect(db_path)
    conn.execute(SCHEMA)
    conn.commit()
    return conn


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
