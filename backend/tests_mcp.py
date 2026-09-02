import asyncio
from datetime import datetime, timezone as dt_tz

from django.test import TestCase

from cache.search import upsert
from connectors.models import AppSettings
from tools.registry import all_tools

_T = datetime(2026, 1, 1, tzinfo=dt_tz.utc)


class McpServerTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()

    def _server(self):
        import mcp_server

        return mcp_server.mcp

    def test_registers_every_registry_tool(self):
        server = self._server()
        listed = asyncio.new_event_loop().run_until_complete(server.list_tools())
        self.assertEqual({t.name for t in listed}, set(all_tools().keys()))

    def test_tool_schema_carries_params(self):
        server = self._server()
        listed = asyncio.new_event_loop().run_until_complete(server.list_tools())
        search = next(t for t in listed if t.name == "search")
        schema = search.inputSchema if hasattr(search, "inputSchema") else search.input_schema
        self.assertIn("query", schema["properties"])
        self.assertIn("query", schema.get("required", []))

    def test_registered_wrapper_routes_to_registry(self):
        # The SDK's async tool runner spawns a worker thread whose sqlite
        # connection can't see the :memory: test DB's FTS vtables — a test-only
        # artifact. True end-to-end MCP invocation is covered by the #45 smoke
        # test against a file DB. Here: prove the wrapper we registered calls
        # registry.call for the right tool.
        import mcp_server

        called = {}
        orig = mcp_server.call
        try:
            mcp_server.call = lambda name, args: called.setdefault("hit", (name, args))
            wrapper = mcp_server._make_tool("search", {"properties": {"query": {"type": "string"}},
                                                       "required": ["query"]})
            wrapper(query="rent")
        finally:
            mcp_server.call = orig
        self.assertEqual(called["hit"], ("search", {"query": "rent"}))
