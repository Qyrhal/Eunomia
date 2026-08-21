from eunomia import db
from eunomia.models import Note


def test_upsert_and_get_notes():
    conn = db.connect(":memory:")
    note = Note(path="a.md", title="A", bucket="work", tags=["work"], mtime=1.0)
    db.upsert_note(conn, note)

    notes = db.get_notes(conn)
    assert len(notes) == 1
    assert notes[0].title == "A"


def test_upsert_is_idempotent_on_path():
    conn = db.connect(":memory:")
    db.upsert_note(conn, Note(path="a.md", title="A", bucket="work", tags=[], mtime=1.0))
    db.upsert_note(conn, Note(path="a.md", title="A renamed", bucket="uni", tags=[], mtime=2.0))

    notes = db.get_notes(conn)
    assert len(notes) == 1
    assert notes[0].title == "A renamed"
    assert notes[0].bucket == "uni"


def test_get_notes_filters_by_bucket():
    conn = db.connect(":memory:")
    db.upsert_note(conn, Note(path="a.md", title="A", bucket="work", tags=[], mtime=1.0))
    db.upsert_note(conn, Note(path="b.md", title="B", bucket="uni", tags=[], mtime=1.0))

    assert [n.path for n in db.get_notes(conn, bucket="work")] == ["a.md"]


def test_sync_notes_removes_deleted_files():
    conn = db.connect(":memory:")
    db.upsert_note(conn, Note(path="a.md", title="A", bucket="work", tags=[], mtime=1.0))

    db.sync_notes(conn, [Note(path="b.md", title="B", bucket="uni", tags=[], mtime=1.0)])

    paths = {n.path for n in db.get_notes(conn)}
    assert paths == {"b.md"}


def test_sync_notes_with_empty_list_clears_table():
    conn = db.connect(":memory:")
    db.upsert_note(conn, Note(path="a.md", title="A", bucket="work", tags=[], mtime=1.0))

    db.sync_notes(conn, [])

    assert db.get_notes(conn) == []
