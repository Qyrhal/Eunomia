"""Entity-memory tools, registered into the shared registry (`tools/registry.py`)
alongside the generic cache tools and per-source tools -- owner-scoped via the
same `call(name, args, owner)` signature used everywhere else."""

from tools.generic import safe
from tools.registry import register_tool

from . import service


@safe
async def entities_search(owner, query, kind=None):
    needle = (query or "").strip().lower()
    rows = await service.list_entities(owner, kind=kind)
    hits = [r for r in rows if needle in r["name"].lower() or any(needle in a.lower() for a in r["aliases"])]
    return {"results": hits}


@safe
async def entities_get(owner, id):
    entity = await service.get_entity(owner, id)
    return entity if entity else {"error": "not found"}


@safe
async def entities_graph(owner):
    return await service.graph(owner)


register_tool(
    "entities_search",
    {
        "type": "object",
        "properties": {
            "query": {"type": "string"},
            "kind": {"type": "string", "enum": list(service.KINDS)},
        },
        "required": ["query"],
    },
    entities_search,
)

register_tool(
    "entities_get",
    {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]},
    entities_get,
)

register_tool("entities_graph", {"type": "object", "properties": {}}, entities_graph)
