from unittest.mock import patch

from django.test import TestCase

from cache.models import CacheRecord
from connectors.models import AppSettings, Connector
from sources import registry

from .source import HeyPocketSource, list_recordings, summary

REC = {
    "id": "rec-1", "title": "Weekly sync", "summary": "discussed the roadmap and Q2 goals",
    "duration": 1800, "recording_at": "2026-01-04T10:00:00Z",
    "tags": [{"name": "work"}, {"name": "planning"}],
}
REC2 = {
    "id": "rec-2", "title": "Client call", "summary": "onboarding walkthrough",
    "duration": 900, "recording_at": "2026-01-05T10:00:00Z",
    "tags": [{"name": "client"}],
}


class MapTests(TestCase):
    def test_map_recording(self):
        env = HeyPocketSource().map(REC)
        self.assertEqual(env["id"], "heypocket:heypocket.recording:rec-1")
        self.assertEqual(env["type"], "heypocket.recording")
        self.assertIn("roadmap", env["body_text"])
        self.assertEqual(env["payload"]["duration_seconds"], 1800)
        self.assertEqual(env["payload"]["tags"], ["work", "planning"])

    def test_map_no_id_skipped(self):
        self.assertIsNone(HeyPocketSource().map({"title": "x"}))


class SyncTests(TestCase):
    def setUp(self):
        s = AppSettings.load(); s.embedding_backend = AppSettings.EMBED_STUB; s.save()
        c = Connector.objects.create(kind="pocketai", enabled=True)
        c.credentials = {"api_key": "pk-SECRETKEY"}
        c.save()
        registry.register(HeyPocketSource())

    def test_sync_ingests_recordings(self):
        with patch("connectors.clients.PocketAIClient.recordings", return_value={"data": [REC]}):
            report, cursor = registry.run_sync("heypocket")
        self.assertEqual(report.written, 1)
        self.assertEqual(cursor, "2026-01-04T10:00:00Z")
        self.assertTrue(CacheRecord.objects.filter(pk="heypocket:heypocket.recording:rec-1").exists())

    def test_provider_is_pocketai_connector(self):
        src = registry.get("heypocket")
        self.assertEqual(src.provider_key, "pocketai")
        self.assertIn("heypocket", [s.key for s in registry.enabled()])


class CachedToolTests(TestCase):
    """summary/list_recordings must read the cache, not the live API, so
    AI tool calls don't spam heypocket on every question."""

    def setUp(self):
        s = AppSettings.load(); s.embedding_backend = AppSettings.EMBED_STUB; s.save()
        c = Connector.objects.create(kind="pocketai", enabled=True)
        c.credentials = {"api_key": "pk-SECRETKEY"}
        c.save()
        registry.register(HeyPocketSource())
        with patch("connectors.clients.PocketAIClient.recordings", return_value={"data": [REC, REC2]}):
            registry.run_sync("heypocket")

    def test_summary_reads_the_cache(self):
        with patch("connectors.clients.PocketAIClient.summary", side_effect=AssertionError("must not hit the live API")):
            out = summary(days=365)
        self.assertEqual(out["recordings_count"], 2)
        self.assertEqual(out["total_duration_minutes"], 45.0)
        self.assertIn({"tag": "work", "count": 1}, out["tag_breakdown"])

    def test_list_recordings_filters_by_tag(self):
        with patch("connectors.clients.PocketAIClient.recordings", side_effect=AssertionError("must not hit the live API")):
            out = list_recordings(days=365, tag="client")
        self.assertEqual([r["title"] for r in out], ["Client call"])

