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


def test_upsert_and_get_credentials():
    conn = db.connect(":memory:")
    db.upsert_credential(conn, "github", "Work", "oauth", "enc-token")

    creds = db.get_credentials(conn)
    assert len(creds) == 1
    assert creds[0].service == "github"
    assert creds[0].label == "Work"
    assert creds[0].kind == "oauth"
    assert creds[0].secret_encrypted == "enc-token"


def test_upsert_credential_idempotent_on_service_and_label():
    conn = db.connect(":memory:")
    db.upsert_credential(conn, "github", "Work", "oauth", "token-1")
    db.upsert_credential(conn, "github", "Work", "oauth", "token-2")

    creds = db.get_credentials(conn)
    assert len(creds) == 1
    assert creds[0].secret_encrypted == "token-2"


def test_credentials_for_different_labels_coexist():
    conn = db.connect(":memory:")
    db.upsert_credential(conn, "github", "Work", "oauth", "token-work")
    db.upsert_credential(conn, "github", "Business", "oauth", "token-biz")

    creds = db.get_credentials(conn)
    assert {(c.label, c.secret_encrypted) for c in creds} == {
        ("Work", "token-work"),
        ("Business", "token-biz"),
    }


def test_get_credential_secret_by_id():
    conn = db.connect(":memory:")
    db.upsert_credential(conn, "openai", "default", "api_key", "enc-key")
    cred_id = db.get_credentials(conn)[0].id

    assert db.get_credential_secret(conn, cred_id) == "enc-key"
    assert db.get_credential_secret(conn, 9999) is None


def test_delete_credential():
    conn = db.connect(":memory:")
    db.upsert_credential(conn, "openai", "default", "api_key", "enc-key")
    cred_id = db.get_credentials(conn)[0].id

    db.delete_credential(conn, cred_id)

    assert db.get_credentials(conn) == []
