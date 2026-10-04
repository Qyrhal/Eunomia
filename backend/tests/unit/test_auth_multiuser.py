"""The core multi-tenancy guarantee: two users with identical-looking data
never see each other's rows via any owner-scoped service function."""

import json

from app.models_user import register_user
from cache.search import list_records, search, upsert
from connectors.crypto import encrypt
from connectors.service import get_connector


async def test_two_users_cannot_see_each_others_connectors(surreal_db):
    user_a = await register_user("a@example.com", "pw-a")
    user_b = await register_user("b@example.com", "pw-b")

    conn = surreal_db
    await conn.query(
        "CREATE connector SET owner = $owner, kind = 'up_bank', enabled = true, credentials_encrypted = $enc",
        {"owner": user_a.id, "enc": encrypt(json.dumps({"personal_access_token": "a-secret"}))},
    )
    await conn.query(
        "CREATE connector SET owner = $owner, kind = 'up_bank', enabled = true, credentials_encrypted = $enc",
        {"owner": user_b.id, "enc": encrypt(json.dumps({"personal_access_token": "b-secret"}))},
    )

    a_conn = await get_connector(user_a.id, "up_bank")
    b_conn = await get_connector(user_b.id, "up_bank")
    assert a_conn is not None and b_conn is not None
    assert a_conn["owner"] == user_a.id
    assert b_conn["owner"] == user_b.id
    assert a_conn["credentials_encrypted"] != b_conn["credentials_encrypted"]


async def test_two_users_cannot_see_each_others_cache_records(surreal_db):
    user_a = await register_user("carol@example.com", "pw-c")
    user_b = await register_user("dave@example.com", "pw-d")

    env = {
        "id": "up_bank:up.transaction:txn-1",
        "source": "up_bank",
        "type": "up.transaction",
        "external_id": "txn-1",
        "title": "Coffee",
        "body_text": "Coffee at the cafe",
    }
    rec_a, _ = await upsert(user_a.id, dict(env))
    rec_b, _ = await upsert(user_b.id, dict(env))
    # same logical id on both sides -- isolation must come from owner scoping,
    # not from the ids happening to differ.
    assert rec_a.id == rec_b.id == "up_bank:up.transaction:txn-1"

    a_records = await list_records(user_a.id, type="up.transaction")
    b_records = await list_records(user_b.id, type="up.transaction")
    assert len(a_records) == 1
    assert len(b_records) == 1

    a_hits = await search(user_a.id, "Coffee", mode="keyword", limit=10)
    b_hits = await search(user_b.id, "Coffee", mode="keyword", limit=10)
    assert len(a_hits) == 1
    assert len(b_hits) == 1

    # user A's view never includes more than their own single record, and
    # vice versa -- confirms no cross-owner leakage even though the
    # underlying cache_record rows share the same logical id.
    assert all(r.id == "up_bank:up.transaction:txn-1" for r in a_records)
    assert all(r.id == "up_bank:up.transaction:txn-1" for r in b_records)
