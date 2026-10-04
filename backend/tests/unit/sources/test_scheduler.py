import json

import respx
from httpx import Response
from surrealdb import RecordID

from app.config import settings
from connectors.crypto import encrypt
from sources import registry
from sources.scheduler import backoff_seconds, build_scheduler, sync_source
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


def test_backoff_table():
    assert backoff_seconds(0) == 900
    assert backoff_seconds(1) == 1800
    assert backoff_seconds(2) == 3600
    assert backoff_seconds(3) == 7200
    assert backoff_seconds(4) == 21600
    # clamps at the last entry past the table's length
    assert backoff_seconds(5) == 21600
    assert backoff_seconds(100) == 21600


@respx.mock
async def test_sync_source_success_records_health(surreal_db, monkeypatch):
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

        report = await sync_source("up_bank")
        assert report["written"] == 1

        st = await conn.select(RecordID("sync_status", "up_bank"))
        if isinstance(st, list):
            st = st[0]
        assert st["consecutive_failures"] == 0
        assert st["last_error"] == ""
        assert st["cursor"] == "2026-01-05T09:00:00+11:00"
    finally:
        registry._REGISTRY.clear()


async def test_sync_source_failure_records_health_without_raising(surreal_db):
    registry._REGISTRY.clear()
    try:
        # no source registered under this key -> run_sync raises KeyError internally
        result = await sync_source("no-such-source")
        assert "error" in result

        st = await surreal_db.select(RecordID("sync_status", "no-such-source"))
        if isinstance(st, list):
            st = st[0]
        assert st["consecutive_failures"] == 1
        assert "KeyError" in st["last_error"]
    finally:
        registry._REGISTRY.clear()


async def test_build_scheduler_heypocket_interval_is_86400s(surreal_db):
    """The plan calls for heypocket to sync every 24h (86400s), not the
    900s default applied to other sources."""
    registry._REGISTRY.clear()
    from sources.heypocket.source import HeyPocketSource

    registry.register(HeyPocketSource())
    try:
        conn = surreal_db
        await conn.query("CREATE connector SET kind = 'pocketai', enabled = true")

        sched = await build_scheduler()
        job = sched.get_job("sync:heypocket")
        assert job is not None
        assert job.trigger.interval.total_seconds() == 86400
    finally:
        registry._REGISTRY.clear()
