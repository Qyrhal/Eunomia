"""Entity-memory REST surface: list/get/graph over the person/organisation/
location + memory + relates_to graph, owner-scoped like every other router."""

from fastapi import APIRouter, Depends, HTTPException

from app.auth import current_user
from app.models_user import User
from entities import service

router = APIRouter(prefix="/entities", tags=["entities"], dependencies=[Depends(current_user)])


@router.get("")
async def list_entities(kind: str | None = None, user: User = Depends(current_user)) -> list[dict]:
    if kind is not None and kind not in service.KINDS:
        raise HTTPException(status_code=400, detail=f"unknown kind {kind!r}; use one of {service.KINDS}")
    return await service.list_entities(user.id, kind=kind)


@router.get("/graph")
async def entity_graph(user: User = Depends(current_user)) -> dict:
    return await service.graph(user.id)


@router.get("/{entity_id}")
async def get_entity(entity_id: str, user: User = Depends(current_user)) -> dict:
    entity = await service.get_entity(user.id, entity_id)
    if entity is None:
        raise HTTPException(status_code=404, detail="not found")
    return entity
