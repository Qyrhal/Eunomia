import json

import respx
from httpx import Response

from app.config import settings
from connectors.crypto import encrypt
from sources.up_bank.source import UpBankSource

TXN = {
    "type": "transactions",
    "id": "txn-1",
    "attributes": {
        "description": "Ona Coffee",
        "rawText": "ONA COFFEE CANBERRA",
        "message": None,
        "status": "SETTLED",
        "createdAt": "2026-01-05T09:00:00+11:00",
        "settledAt": "2026-01-05T09:05:00+11:00",
        "amount": {"value": "-5.50", "valueInBaseUnits": -550, "currencyCode": "AUD"},
    },
    "relationships": {"category": {"data": {"id": "restaurants-and-cafes"}}},
}
ACCT = {
    "type": "accounts",
    "id": "acc-1",
    "attributes": {
        "displayName": "Spending",
        "accountType": "TRANSACTIONAL",
        "ownershipType": "INDIVIDUAL",
        "createdAt": "2025-01-01T00:00:00+11:00",
        "balance": {"value": "123.45", "valueInBaseUnits": 12345, "currencyCode": "AUD"},
    },
}
CATEGORY = {
    "type": "categories",
    "id": "restaurants-and-cafes",
    "attributes": {"name": "Restaurants and cafes"},
    "relationships": {"parent": {"data": {"id": "good-life"}}},
}


def test_map_transaction():
    env = UpBankSource().map(TXN)
    assert env["id"] == "up_bank:up.transaction:txn-1"
    assert env["type"] == "up.transaction"
    assert env["title"] == "Ona Coffee"
    assert "ONA COFFEE" in env["body_text"]
    assert env["payload"]["amount_cents"] == -550
    assert env["payload"]["category"] == "restaurants-and-cafes"
    assert env["payload"]["is_income"] is False


def test_map_account():
    env = UpBankSource().map(ACCT)
    assert env["type"] == "up.account"
    assert env["payload"]["balance_cents"] == 12345


def test_map_category():
    env = UpBankSource().map(CATEGORY)
    assert env["type"] == "up.category"
    assert env["title"] == "Restaurants and cafes"
    assert env["payload"]["parent"] == "good-life"


def test_map_unknown_type_skipped():
    assert UpBankSource().map({"type": "pings", "id": "x"}) is None


@respx.mock
async def test_sync_pulls_transactions_accounts_categories(surreal_db, monkeypatch):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    conn = surreal_db
    await conn.query(
        "CREATE connector SET kind = 'up_bank', enabled = true, credentials_encrypted = $enc",
        {"enc": encrypt(json.dumps({"personal_access_token": "up:yeah:SECRETPAT"}))},
    )

    respx.get("https://api.up.com.au/api/v1/transactions").mock(
        return_value=Response(200, json={"data": [TXN], "links": {}})
    )
    respx.get("https://api.up.com.au/api/v1/accounts").mock(return_value=Response(200, json={"data": [ACCT]}))
    respx.get("https://api.up.com.au/api/v1/categories").mock(
        return_value=Response(200, json={"data": [CATEGORY]})
    )

    src = UpBankSource()
    result = await src.sync("poll")

    kinds = {r["type"] for r in result.records}
    assert kinds == {"transactions", "accounts", "categories"}
    assert result.cursor == "2026-01-05T09:00:00+11:00"
