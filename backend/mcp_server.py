"""Eunomia MCP server -- exposes the full tool registry to Hermes / any MCP client.

    python mcp_server.py --token <personal-api-token>         # stdio (Claude Desktop, local clients)
    python mcp_server.py --token <...> --http                 # streamable HTTP on 127.0.0.1:8765
    python mcp_server.py --token <...> --http --host 100.x.y.z --port 8765

The token can also be given via the EUNOMIA_API_TOKEN env var instead of
--token. Unlike the REST surface, an MCP connection isn't an interactive
login -- it's authenticated once, at startup, by resolving a personal API
token (see `app/models_user.py::verify_api_token`) to a single `User`. Every
tool call made over that connection runs as that one owner, matching how
Claude Desktop/Code configs work: one token per MCP server config entry.

HTTP mode reuses `verify_api_token` for its bearer-token check too (same
token as stdio mode, not a separate static secret). Bind to the tailscale /
netbird interface, never 0.0.0.0.

Tools, their schemas, and arg dispatch all come from `tools.registry`, so
this file never needs editing when a source or tool is added.
"""

import argparse
import asyncio
import inspect
import os
import sys

from mcp.server.mcpserver import MCPServer

from tools.registry import all_tools, call

mcp = MCPServer(
    "eunomia",
    instructions="Eunomia -- your personal data layer. Search/get/list your "
    "bank and heypocket data; manage tasks; register watches that ping you.",
)

_TYPE_MAP = {"string": str, "integer": int, "boolean": bool, "object": dict, "array": list}


def _make_tool(name: str, schema: dict, owner):
    props = (schema or {}).get("properties", {}) or {}
    required = set((schema or {}).get("required", []))
    params = [
        inspect.Parameter(
            key,
            inspect.Parameter.KEYWORD_ONLY,
            default=inspect.Parameter.empty if key in required else None,
            annotation=_TYPE_MAP.get(spec.get("type"), str),
        )
        for key, spec in props.items()
    ]

    async def impl(**kwargs):
        return await call(name, {k: v for k, v in kwargs.items() if v is not None}, owner)

    impl.__name__ = name
    impl.__signature__ = inspect.Signature(params)
    impl.__doc__ = (schema or {}).get("description", f"Eunomia tool: {name}")
    return impl


def _register_all(owner):
    for name, spec in all_tools().items():
        mcp.add_tool(_make_tool(name, spec["schema"], owner), name=name)


async def _resolve_owner(token: str):
    from app.models_user import verify_api_token

    user = await verify_api_token(token)
    if user is None:
        print("error: invalid API token", file=sys.stderr)
        sys.exit(1)
    return user


async def _startup(token: str):
    """Connect to the DB, discover sources, and resolve the owner -- once,
    before serving."""
    from app.db import connect_db, ensure_schema
    from sources.registry import discover

    db = await connect_db()
    await ensure_schema(db)
    discover()
    user = await _resolve_owner(token)
    _register_all(user.id)
    return user


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--token", default=os.environ.get("EUNOMIA_API_TOKEN", ""))
    ap.add_argument("--http", action="store_true")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8765)
    args = ap.parse_args()

    if not args.token:
        print("error: --token or EUNOMIA_API_TOKEN is required", file=sys.stderr)
        sys.exit(1)

    asyncio.run(_startup(args.token))

    if not args.http:
        mcp.run()
    else:
        import uvicorn
        from starlette.middleware.base import BaseHTTPMiddleware
        from starlette.responses import JSONResponse

        from app.models_user import verify_api_token

        class Bearer(BaseHTTPMiddleware):
            async def dispatch(self, request, call_next):
                got = request.headers.get("authorization", "")
                scheme, _, value = got.partition(" ")
                if scheme.lower() != "bearer" or not value or await verify_api_token(value) is None:
                    return JSONResponse({"error": "unauthorized"}, status_code=401)
                return await call_next(request)

        app = mcp.streamable_http_app(host=args.host)
        app.add_middleware(Bearer)
        uvicorn.run(app, host=args.host, port=args.port)


if __name__ == "__main__":
    main()
