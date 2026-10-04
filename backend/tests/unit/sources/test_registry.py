import json

import respx
from httpx import Response

from app.config import settings
from connectors.crypto import encrypt
from sources import registry
from sources.up_bank.source import UpBankSource

TXN = {
    "type": "transactions",
    "id": "txn-1",
    "attributes": {
        "description": "Ona Coffee",
        "rawText": "",
        "message": None,
        "status": "SETTLED",
        "createdAt": "2026-01-05T09:00:00+11:00",
        "settledAt": "2026-01-05T09:05:00+11:00",
        "amount": {"value": "-5.50", "valueInBaseUnits": -550, "currencyCode": "AUD"},
    },
    "relationships": {"category": {"data": None}},
}


def test_discover_registers_known_sources():
    registry._REGISTRY.clear()
    registry.discover()
    try:
        keys = set(registry._REGISTRY.keys())
        assert {"up_bank", "heypocket", "demo", "example"} <= keys
    finally:
        registry._REGISTRY.clear()


async def test_enabled_filters_by_connector_enabled_and_demo_flag(surreal_db):
    registry._REGISTRY.clear()
    registry.register(UpBankSource())
    try:
        conn = surreal_db
        await conn.query("CREATE connector SET kind = 'up_bank', enabled = true")
        enabled = await registry.enabled()
        assert [s.key for s in enabled] == ["up_bank"]

        await conn.query("UPDATE connector SET config = {demo: true} WHERE kind = 'up_bank'")
        enabled = await registry.enabled()
        assert enabled == []
    finally:
        registry._REGISTRY.clear()


async def test_credentials_for_decrypts_connector_credentials(surreal_db):
    registry._REGISTRY.clear()
    src = UpBankSource()
    registry.register(src)
    try:
        conn = surreal_db
        await conn.query(
            "CREATE connector SET kind = 'up_bank', enabled = true, credentials_encrypted = $enc",
            {"enc": encrypt(json.dumps({"personal_access_token": "secret"}))},
        )
        creds = await registry.credentials_for(src)
        assert creds == {"personal_access_token": "secret"}
    finally:
        registry._REGISTRY.clear()


async def test_credentials_for_no_connector_returns_empty_dict(surreal_db):
    registry._REGISTRY.clear()
    src = UpBankSource()
    registry.register(src)
    try:
        creds = await registry.credentials_for(src)
        assert creds == {}
    finally:
        registry._REGISTRY.clear()


@respx.mock
async def test_run_sync_calls_ingest(surreal_db, monkeypatch):
    monkeypatch.setattr(settings, "EMBEDDINGS_BACKEND", "stub")

    registry._REGISTRY.clear()
    registry.register(UpBankSource())
    try:
        conn = surreal_db
        await conn.query(
            "CREATE connector SET kind = 'up_bank', enabled = true, credentials_encrypted = $enc",
            {"enc": encrypt(json.dumps({"personal_access_token": "secret"}))},
        )

        respx.get("https://api.up.com.au/api/v1/transactions").mock(
            return_value=Response(200, json={"data": [TXN], "links": {}})
        )
        respx.get("https://api.up.com.au/api/v1/accounts").mock(return_value=Response(200, json={"data": []}))
        respx.get("https://api.up.com.au/api/v1/categories").mock(return_value=Response(200, json={"data": []}))

        report, cursor = await registry.run_sync("up_bank", "poll", None)
        assert report.written == 1
        assert cursor == "2026-01-05T09:00:00+11:00"
    finally:
        registry._REGISTRY.clear()


async def test_run_sync_unknown_source_raises_keyerror():
    registry._REGISTRY.clear()
    import pytest

    with pytest.raises(KeyError):
        await registry.run_sync("no-such-source")


def test_tool_registry_namespaces_tool_names():
    registry._REGISTRY.clear()
    registry.register(UpBankSource())
    try:
        tools = registry.tool_registry()
        assert "up_bank__finance_summary" in tools
        assert "up_bank__list_transactions" in tools
        assert "up_bank__list_accounts" in tools
    finally:
        registry._REGISTRY.clear()
