"""Parity guarantee from the plan: every tool the REST/registry surface
exposes must also be registered on the MCP server, and vice versa."""

import mcp_server
from sources.registry import discover
from tools.registry import all_tools


async def test_mcp_tool_set_matches_registry(surreal_db, owner):
    discover()
    mcp_server._register_all(owner)
    try:
        registered = {t.name for t in await mcp_server.mcp.list_tools()}
        assert registered == set(all_tools().keys())
    finally:
        for name in list(all_tools().keys()):
            try:
                mcp_server.mcp.remove_tool(name)
            except Exception:
                pass
