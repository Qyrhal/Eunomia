from datetime import datetime, timezone as dt_tz

from django.test import TestCase

from cache.search import upsert
from connectors.models import AppSettings
from masking.vault import tokenize

from . import registry
from .generic import get, search

_T = datetime(2026, 1, 1, tzinfo=dt_tz.utc)


def _env(i, title, body):
    return {
        "id": f"up_bank:up.transaction:{i}", "source": "up_bank", "type": "up.transaction",
        "external_id": str(i), "title": title, "body_text": body, "occurred_at": _T,
        "url": "", "payload": {}, "links": [], "deleted": False,
    }


class GenericToolTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()
        upsert(_env(1, "Rent", "monthly rent payment to landlord"))
        upsert(_env(2, "Coffee", "flat white"))

    def test_search_returns_hits_with_snippet(self):
        out = search("rent", mode="keyword")
        self.assertEqual(out["results"][0]["id"], "up_bank:up.transaction:1")
        self.assertIn("snippet", out["results"][0])

    def test_get_full_record_or_error(self):
        self.assertEqual(get("up_bank:up.transaction:2")["title"], "Coffee")
        self.assertEqual(get("missing")["error"], "not found")


class RegistryTests(TestCase):
    def test_all_tools_has_generic_plus_source_tools(self):
        from sources.registry import discover

        discover()
        reg = registry.all_tools()
        self.assertIn("search", reg)
        self.assertIn("get", reg)
        self.assertIn("example.ping", reg)

    def test_register_tool_and_call(self):
        registry.register_tool("echo", {"type": "object"}, lambda **kw: kw)
        self.assertEqual(registry.call("echo", {"a": 1}), {"a": 1})

    def test_call_resolves_tokens_in_args(self):
        tok = tokenize("secret@x.com", "email")
        registry.register_tool("passthru", {"type": "object"}, lambda **kw: kw)
        self.assertEqual(registry.call("passthru", {"to": tok}), {"to": "secret@x.com"})

    def test_unknown_tool(self):
        self.assertEqual(registry.call("nope", {}), {"error": "unknown tool nope"})
