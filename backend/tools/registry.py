"""The one tool registry consumed by both the MCP server (#14) and the REST
surface (#40). Merges: generic cache tools (#7) + per-source tools (#5) +
task tools (#38, registered by the tasks app on import)."""

from . import generic

_EXTRA: dict[str, dict] = {}


def register_tool(name: str, schema: dict, impl):
    """Used by the tasks app (#38) and anything else adding tools at import time."""
    _EXTRA[name] = {"schema": schema, "impl": impl}


def all_tools() -> dict[str, dict]:
    reg: dict[str, dict] = {}
    for name, impl in generic.IMPLS.items():
        reg[name] = {"schema": generic.SCHEMAS[name], "impl": impl}

    from sources.registry import tool_registry as source_tools

    for fq_name, spec in source_tools().items():
        reg[fq_name] = {"schema": spec.schema, "impl": spec.impl}

    reg.update(_EXTRA)
    return reg


def call(name: str, args: dict):
    """Resolve tokens in `args` (the de-tokenization boundary), then run the tool."""
    from masking.boundary import resolve_tool_input

    tools = all_tools()
    if name not in tools:
        return {"error": f"unknown tool {name}"}
    return tools[name]["impl"](**resolve_tool_input(args or {}))
