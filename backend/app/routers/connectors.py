"""Connector CRUD + per-connector data endpoints (finance summary, PocketAI
recordings) + the cross-connector dashboard snapshot. Ported from the old
Django `connectors/views.py`, owner-scoped, no Google/Twenty/demo-mode."""

import datetime

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from app.auth import current_user
from app.models_user import User
from connectors.clients import OpenConnectorClient, PocketAIClient, UpBankClient
from connectors.service import (
    CONNECTOR_KINDS,
    credentials_for,
    get_or_create_connector,
    list_connectors,
    upsert_connector,
)

router = APIRouter(prefix="/connectors", tags=["connectors"], dependencies=[Depends(current_user)])
snapshot_router = APIRouter(tags=["connectors"], dependencies=[Depends(current_user)])


class ConnectorUpdate(BaseModel):
    enabled: bool | None = None
    config: dict | None = None
    credentials: dict | None = None


def _out(row: dict) -> dict:
    return {
        "kind": row["kind"],
        "enabled": row.get("enabled", False),
        "config": row.get("config") or {},
        "credentials_set": bool(row.get("credentials_encrypted")),
        "updated_at": row.get("updated_at"),
    }


def _client_for(kind: str, config: dict, credentials: dict):
    if kind == "up_bank":
        return UpBankClient(credentials)
    if kind == "pocketai":
        return PocketAIClient(credentials, config.get("base_url"))
    if kind == "open_connector":
        return OpenConnectorClient(credentials, config.get("base_url"))
    return None


@router.get("")
async def list_all(user: User = Depends(current_user)) -> list[dict]:
    return [_out(row) for row in await list_connectors(user.id)]


@router.get("/up_bank/finance-summary")
async def up_bank_finance_summary(days: int = 30, user: User = Depends(current_user)) -> dict:
    row = await get_or_create_connector(user.id, "up_bank")
    creds = await credentials_for(user.id, "up_bank")
    if not row.get("enabled") or not creds.get("personal_access_token"):
        raise HTTPException(status_code=400, detail="Up Bank is not connected")
    since = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=days)
    return await UpBankClient(creds).finance_summary(since.isoformat())


@router.get("/pocketai/summary")
async def pocketai_summary(days: int = 30, user: User = Depends(current_user)) -> dict:
    client = await _pocketai_client_or_400(user)
    since = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=days)).date().isoformat()
    return await client.summary(since)


@router.get("/pocketai/all")
async def pocketai_all(limit: int = 50, user: User = Depends(current_user)) -> dict:
    client = await _pocketai_client_or_400(user)
    return await client.recordings({"limit": limit})


@router.get("/pocketai/search")
async def pocketai_search(query: str, user: User = Depends(current_user)) -> dict:
    client = await _pocketai_client_or_400(user)
    return await client.search(query)


@router.get("/pocketai/detail/{recording_id}")
async def pocketai_detail(recording_id: str, user: User = Depends(current_user)) -> dict:
    client = await _pocketai_client_or_400(user)
    return await client.recording(recording_id)


async def _pocketai_client_or_400(user: User) -> PocketAIClient:
    row = await get_or_create_connector(user.id, "pocketai")
    creds = await credentials_for(user.id, "pocketai")
    if not row.get("enabled") or not creds.get("api_key"):
        raise HTTPException(status_code=400, detail="PocketAI is not connected")
    return PocketAIClient(creds, (row.get("config") or {}).get("base_url"))


@router.get("/{kind}")
async def get_one(kind: str, user: User = Depends(current_user)) -> dict:
    if kind not in CONNECTOR_KINDS:
        raise HTTPException(status_code=404, detail="unknown connector")
    return _out(await get_or_create_connector(user.id, kind))


@router.put("/{kind}")
async def put_one(kind: str, body: ConnectorUpdate, user: User = Depends(current_user)) -> dict:
    if kind not in CONNECTOR_KINDS:
        raise HTTPException(status_code=404, detail="unknown connector")
    row = await upsert_connector(
        user.id, kind, enabled=body.enabled, config=body.config, credentials=body.credentials
    )
    return _out(row)


@router.post("/{kind}/test")
async def test_one(kind: str, user: User = Depends(current_user)) -> dict:
    if kind not in CONNECTOR_KINDS:
        raise HTTPException(status_code=404, detail="unknown connector")
    row = await get_or_create_connector(user.id, kind)
    creds = await credentials_for(user.id, kind)
    client = _client_for(kind, row.get("config") or {}, creds)
    try:
        ok = await client.ping()
    except Exception as exc:  # surfaced to the settings UI, not swallowed
        return {"ok": False, "error": str(exc)}
    return {"ok": ok}


@snapshot_router.get("/snapshot")
async def snapshot(user: User = Depends(current_user)) -> dict:
    """Best-effort figures for each connected account, read live from each
    client. A connector that isn't connected, or whose call fails, comes back
    as null rather than failing the whole request."""
    result: dict = {"up_bank": None, "pocketai": None}

    now = datetime.datetime.now(datetime.timezone.utc)
    week_start = (now - datetime.timedelta(days=now.weekday())).replace(
        hour=0, minute=0, second=0, microsecond=0
    )

    up_bank = await get_or_create_connector(user.id, "up_bank")
    up_bank_creds = await credentials_for(user.id, "up_bank")
    if up_bank.get("enabled") and up_bank_creds.get("personal_access_token"):
        try:
            result["up_bank"] = await UpBankClient(up_bank_creds).week_summary(week_start.isoformat())
        except Exception:
            pass

    pocketai = await get_or_create_connector(user.id, "pocketai")
    pocketai_creds = await credentials_for(user.id, "pocketai")
    if pocketai.get("enabled") and pocketai_creds.get("api_key"):
        try:
            since = (now - datetime.timedelta(days=7)).date().isoformat()
            client = PocketAIClient(pocketai_creds, (pocketai.get("config") or {}).get("base_url"))
            summary = await client.summary(since)
            result["pocketai"] = {"recordings_count": summary["recordings_count"]}
        except Exception:
            pass

    return result
