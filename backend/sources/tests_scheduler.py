from datetime import timedelta

from django.test import TestCase
from django.utils import timezone

from cache.models import CacheRecord
from connectors.models import AppSettings, Connector

from . import registry
from .base import Source, SyncResult
from .models import SyncStatus
from .scheduler import backfill_embeddings, backoff_seconds, poll_all, sync_source


class OKSource(Source):
    key = "sched_ok"
    label = "ok"
    record_types = ["t.n"]

    def sync(self, mode, cursor=None):
        return SyncResult(records=[{"id": "1", "title": "hi", "body": "world"}], cursor="cX")

    def map(self, raw):
        return {
            "id": f"sched_ok:t.n:{raw['id']}", "source": "sched_ok", "type": "t.n",
            "external_id": raw["id"], "title": raw["title"], "body_text": raw["body"],
            "occurred_at": None, "url": "", "payload": {}, "links": [], "deleted": False,
        }


class BadSource(OKSource):
    key = "sched_bad"

    def sync(self, mode, cursor=None):
        raise RuntimeError("upstream 500")


class SchedulerTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()
        registry.register(OKSource())
        registry.register(BadSource())

    def test_sync_source_records_success(self):
        rep = sync_source("sched_ok")
        self.assertEqual(rep["written"], 1)
        st = SyncStatus.objects.get(source_key="sched_ok")
        self.assertEqual(st.cursor, "cX")
        self.assertIsNotNone(st.last_ok)
        self.assertEqual(st.consecutive_failures, 0)

    def test_sync_source_records_failure_without_raising(self):
        rep = sync_source("sched_bad")
        self.assertIn("error", rep)
        st = SyncStatus.objects.get(source_key="sched_bad")
        self.assertEqual(st.consecutive_failures, 1)
        self.assertIn("upstream 500", st.last_error)

    def test_backoff_grows_and_caps(self):
        self.assertEqual(backoff_seconds(0), 900)
        self.assertEqual(backoff_seconds(2), 3600)
        self.assertEqual(backoff_seconds(99), 21600)

    def test_poll_all_skips_sources_inside_backoff_window(self):
        Connector.objects.create(kind="sched_bad", enabled=True)
        SyncStatus.objects.create(
            source_key="sched_bad", consecutive_failures=3,
            last_run=timezone.now() - timedelta(seconds=60),
        )
        results = poll_all()
        self.assertNotIn("sched_bad", [r.get("source") for r in results])

    def test_backfill_embeds_missing(self):
        CacheRecord.objects.create(
            id="x:y.z:1", source="x", type="y.z", external_id="1", title="t",
            body_text="needs a vector", content_hash="h",
            ingested_at=timezone.now(), updated_at=timezone.now(),
        )
        self.assertEqual(backfill_embeddings(), 1)
        self.assertTrue(CacheRecord.objects.get(pk="x:y.z:1").has_embedding)


class SourceEndpointTests(TestCase):
    def setUp(self):
        AppSettings.load().save()
        registry.register(OKSource())

    def test_list_endpoint(self):
        r = self.client.get("/api/sources")
        self.assertEqual(r.status_code, 200)
        self.assertIn("sched_ok", [s["key"] for s in r.json()])

    def test_on_demand_sync_endpoint(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()
        r = self.client.post("/api/sources/sched_ok/sync")
        self.assertEqual(r.status_code, 200)
        self.assertEqual(r.json()["written"], 1)

    def test_on_demand_sync_unknown(self):
        self.assertEqual(self.client.post("/api/sources/nope/sync").status_code, 404)
