from django.test import TestCase

from cache.models import CacheRecord
from connectors.models import AppSettings, Connector

from . import registry
from .base import Source, SyncResult, ToolSpec


class FakeSource(Source):
    key = "fake"
    label = "Fake"
    record_types = ["fake.thing"]

    def __init__(self, records=None):
        self._records = records or []

    def sync(self, mode, cursor=None):
        return SyncResult(records=self._records, cursor="c2")

    def map(self, raw):
        return {
            "id": f"fake:fake.thing:{raw['id']}",
            "source": "fake",
            "type": "fake.thing",
            "external_id": str(raw["id"]),
            "title": raw.get("title", ""),
            "body_text": raw.get("body", ""),
            "occurred_at": None,
            "url": "",
            "payload": {},
            "links": [],
            "deleted": False,
        }

    def tools(self):
        return [ToolSpec(name="hi", schema={"type": "object"}, impl=lambda: "hi")]


class DiscoveryTests(TestCase):
    def test_example_source_is_discovered(self):
        registry.discover()
        self.assertIsNotNone(registry.get("example"))
        self.assertIn("example", [s.key for s in registry.all()])

    def test_tool_registry_namespaces_by_source_key(self):
        registry.discover()
        self.assertIn("example__ping", registry.tool_registry())


class RunSyncTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()
        registry.register(FakeSource(records=[{"id": 1, "title": "One", "body": "hello world"}]))

    def test_run_sync_pipes_records_into_the_cache(self):
        report, cursor = registry.run_sync("fake")
        self.assertEqual(report.written, 1)
        self.assertEqual(cursor, "c2")
        self.assertTrue(CacheRecord.objects.filter(pk="fake:fake.thing:1").exists())

    def test_run_sync_unknown_source_raises(self):
        with self.assertRaises(KeyError):
            registry.run_sync("nope")

    def test_enabled_reflects_connector_rows(self):
        self.assertNotIn("fake", [s.key for s in registry.enabled()])
        Connector.objects.create(kind="fake", enabled=True)
        self.assertIn("fake", [s.key for s in registry.enabled()])

    def test_enabled_excludes_demo_mode_connectors(self):
        # Regression: a connector left in demo mode (no real credentials) must
        # not be polled for real by the scheduler — it used to be, and spammed
        # the live API with empty/garbage auth every cycle.
        Connector.objects.create(kind="fake", enabled=True, config={"demo": True})
        self.assertNotIn("fake", [s.key for s in registry.enabled()])


class ProviderSharingTests(TestCase):
    def test_sources_can_share_one_provider_connector(self):
        class A(FakeSource):
            key = "prov_a"
            provider = "prov"

        class B(FakeSource):
            key = "prov_b"
            provider = "prov"

        registry.register(A())
        registry.register(B())
        Connector.objects.create(kind="prov", enabled=True)

        keys = {s.key for s in registry.enabled()}
        self.assertTrue({"prov_a", "prov_b"} <= keys)
