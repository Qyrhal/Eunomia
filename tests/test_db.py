import sqlite3

import pytest

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


def test_connect_seeds_default_buckets():
    conn = db.connect(":memory:")
    keys = [b.key for b in db.get_buckets(conn)]
    assert keys == ["uni", "work", "business", "other"]


def test_connect_does_not_reseed_on_second_call_with_same_db():
    conn = sqlite3.connect(":memory:")
    conn.executescript(db.SCHEMA)
    db._seed_default_buckets(conn)
    db._seed_default_buckets(conn)
    assert len(db.get_buckets(conn)) == 4


def test_add_bucket():
    conn = db.connect(":memory:")
    db.add_bucket(conn, "side-project", "Side Project", ["startup-x"])

    keys = [b.key for b in db.get_buckets(conn)]
    assert "side-project" in keys

    added = next(b for b in db.get_buckets(conn) if b.key == "side-project")
    assert added.label == "Side Project"
    assert added.keywords == ["startup-x"]


def test_add_bucket_duplicate_key_raises():
    conn = db.connect(":memory:")
    with pytest.raises(sqlite3.IntegrityError):
        db.add_bucket(conn, "work", "Work Again", [])


def test_delete_bucket():
    conn = db.connect(":memory:")
    db.add_bucket(conn, "side-project", "Side Project", [])
    bucket_id = next(b.id for b in db.get_buckets(conn) if b.key == "side-project")

    db.delete_bucket(conn, bucket_id)

    keys = [b.key for b in db.get_buckets(conn)]
    assert "side-project" not in keys


def test_delete_bucket_cannot_remove_other():
    conn = db.connect(":memory:")
    other_id = next(b.id for b in db.get_buckets(conn) if b.key == "other")

    db.delete_bucket(conn, other_id)

    keys = [b.key for b in db.get_buckets(conn)]
    assert "other" in keys


def test_app_settings_get_set():
    conn = db.connect(":memory:")
    assert db.get_setting(conn, "vault_path") is None

    db.set_setting(conn, "vault_path", "/home/me/vault")
    assert db.get_setting(conn, "vault_path") == "/home/me/vault"


def test_app_settings_set_overwrites():
    conn = db.connect(":memory:")
    db.set_setting(conn, "vault_path", "/first")
    db.set_setting(conn, "vault_path", "/second")
    assert db.get_setting(conn, "vault_path") == "/second"
