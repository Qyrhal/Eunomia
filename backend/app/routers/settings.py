"""Per-user app settings: embedding model, sync intervals, theme, and the
encrypted OpenAI API key."""

from fastapi import APIRouter, Depends
from pydantic import BaseModel

from app.auth import current_user
from app.db import db as get_connection
from app.models_user import User
from connectors.service import get_app_settings, update_app_settings

router = APIRouter(prefix="/settings", tags=["settings"], dependencies=[Depends(current_user)])


class SettingsUpdate(BaseModel):
    embedding_model: str | None = None
    sync_intervals: dict | None = None
    theme: dict | None = None
    openai_api_key: str | None = None


def _out(row: dict) -> dict:
    return {
        "embedding_model": row.get("embedding_model", ""),
        "sync_intervals": row.get("sync_intervals") or {},
        "theme": row.get("theme") or {},
        "openai_api_key_set": bool(row.get("openai_api_key_encrypted")),
    }


@router.get("")
async def read_settings(user: User = Depends(current_user)) -> dict:
    return _out(await get_app_settings(user.id))


@router.patch("")
async def patch_settings(body: SettingsUpdate, user: User = Depends(current_user)) -> dict:
    fields = body.model_dump(exclude_unset=True)
    row = await update_app_settings(user.id, **fields)
    return _out(row)


@router.post("/complete-onboarding")
async def complete_onboarding(user: User = Depends(current_user)) -> dict:
    conn = get_connection()
    await conn.query("UPDATE $id SET onboarded_at = time::now()", {"id": user.id})
    return {"ok": True}
