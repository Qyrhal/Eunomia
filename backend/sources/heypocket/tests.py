from unittest.mock import patch

from django.test import TestCase

from cache.models import CacheRecord
from connectors.models import AppSettings, Connector
from sources import registry

from .source import HeyPocketSource

REC = {
    "id": "rec-1", "title": "Weekly sync", "summary": "discussed the roadmap and Q2 goals",
    "duration": 1800, "recording_at": "2026-01-04T10:00:00Z",
    "tags": [{"name": "work"}, {"name": "planning"}],
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

    def test_api_key_masked_if_it_appears_in_data(self):
        rec = dict(REC, summary="key pk-SECRETKEY leaked")
        with patch("connectors.clients.PocketAIClient.recordings", return_value={"data": [rec]}):
            registry.run_sync("heypocket")
        r = CacheRecord.objects.get(pk="heypocket:heypocket.recording:rec-1")
        self.assertNotIn("pk-SECRETKEY", r.body_text)
