"""Eunomia MCP server — exposes the full tool registry to Hermes / any MCP client.

    python mcp_server.py                 # stdio (Claude Desktop, local clients)
    python mcp_server.py --http          # streamable HTTP on 127.0.0.1:8765
    python mcp_server.py --http --host 100.x.y.z --port 8765

HTTP requires `Authorization: Bearer $EUNOMIA_API_TOKEN` when that env var is set
(same token as the REST surface — see config/auth.py). Bind to the tailscale /
netbird interface, never 0.0.0.0.

Tools, their schemas, and token de-tokenisation all come from `tools.registry`,
so this file never needs editing when a source or tool is added.
"""

import argparse
import inspect
import os

import django

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "config.settings")
# Single-user local tool server: the MCP transport owns the event loop and the
# tools are short reads/writes, so running the ORM in-loop is fine and keeps
# every call on the one connection that has the FTS5 / sqlite-vec vtables.
os.environ.setdefault("DJANGO_ALLOW_ASYNC_UNSAFE", "1")
django.setup()

from mcp.server.mcpserver import MCPServer  # noqa: E402

from tools.registry import all_tools, call  # noqa: E402

mcp = MCPServer("eunomia", instructions="Eunomia — your masked personal data layer. "
                "Search/get/list your Google, bank and heypocket data; "
                "register watches that ping you. Secrets and PII are already masked.")


def _make_tool(name: str, schema: dict):
    props = (schema or {}).get("properties", {}) or {}
    required = set((schema or {}).get("required", []))
    params = [
        inspect.Parameter(
            key, inspect.Parameter.KEYWORD_ONLY,
            default=inspect.Parameter.empty if key in required else None,
            annotation={"string": str, "integer": int, "boolean": bool,
                        "object": dict, "array": list}.get(spec.get("type"), str),
        )
        for key, spec in props.items()
    ]

    def impl(**kwargs):
        return call(name, {k: v for k, v in kwargs.items() if v is not None})

    impl.__name__ = name
    impl.__signature__ = inspect.Signature(params)
    impl.__doc__ = (schema or {}).get("description", f"Eunomia tool: {name}")
    return impl


def _register_all():
    for name, spec in all_tools().items():
        mcp.add_tool(_make_tool(name, spec["schema"]), name=name)


_register_all()


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--http", action="store_true")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8765)
    args = ap.parse_args()

    if not args.http:
        mcp.run()
    else:
        import uvicorn
        from starlette.middleware.base import BaseHTTPMiddleware
        from starlette.responses import JSONResponse

        token = os.environ.get("EUNOMIA_API_TOKEN", "")

        class Bearer(BaseHTTPMiddleware):
            async def dispatch(self, request, call_next):
                if token:
                    got = request.headers.get("authorization", "")
                    if got != f"Bearer {token}":
                        return JSONResponse({"error": "unauthorized"}, status_code=401)
                return await call_next(request)

        app = mcp.streamable_http_app(host=args.host)
        app.add_middleware(Bearer)
        uvicorn.run(app, host=args.host, port=args.port)
