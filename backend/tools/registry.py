"""The one tool registry consumed by both the MCP server and the REST
surface. Merges: generic cache tools + per-source tools (once `sources`
exists) + anything registered at import time via :func:`register_tool`.

Async end to end: REST needs `async def` handlers for FastAPI concurrency and
the `mcp` SDK supports async tool callables in both stdio and HTTP modes.
"""

from sources.registry import tool_registry as source_tools

from . import generic

_EXTRA: dict[str, dict] = {}


def register_tool(name: str, schema: dict, impl) -> None:
    """Used by anything adding tools at import time (e.g. a future tasks app)."""
    _EXTRA[name] = {"schema": schema, "impl": impl}


def all_tools() -> dict[str, dict]:
    reg: dict[str, dict] = {}
    for name, impl in generic.IMPLS.items():
        reg[name] = {"schema": generic.SCHEMAS[name], "impl": impl}

    for fq_name, spec in source_tools().items():
        reg[fq_name] = {"schema": spec.schema, "impl": spec.impl}

    reg.update(_EXTRA)
    return reg


async def call(name: str, args: dict, owner) -> dict:
    tools = all_tools()
    if name not in tools:
        return {"error": f"unknown tool {name}"}
    return await tools[name]["impl"](owner, **(args or {}))
