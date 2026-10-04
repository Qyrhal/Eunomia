"""REST surface for the tool registry -- the catalogue is public (schema
only, no data); calling a tool requires auth since it reads the caller's
owned data."""

from fastapi import APIRouter, Depends, HTTPException

from app.auth import current_user
from app.models_user import User
from tools.registry import all_tools, call

router = APIRouter(prefix="/tools", tags=["tools"])


@router.get("")
async def catalogue() -> dict:
    return {name: spec["schema"] for name, spec in all_tools().items()}


@router.post("/{name}")
async def invoke(name: str, body: dict | None = None, user: User = Depends(current_user)) -> dict:
    if name not in all_tools():
        raise HTTPException(status_code=404, detail=f"unknown tool {name!r}")
    return await call(name, body or {}, owner=user.id)
