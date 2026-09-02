from datetime import datetime, timezone as dt_tz

from django.test import TestCase

from connectors.models import AppSettings
from masking.vault import detokenize

from .ingest import ingest
from .models import CacheLink, CacheRecord

_T = datetime(2026, 1, 1, tzinfo=dt_tz.utc)


def _map(raw):
    if raw.get("drop"):
        return None
    return {
        "id": f"src:x.n:{raw['id']}",
        "source": "src",
        "type": "x.n",
        "external_id": str(raw["id"]),
        "title": raw.get("title", ""),
        "body_text": raw.get("body", ""),
        "occurred_at": _T,
        "url": "",
        "payload": raw.get("payload", {}),
        "links": raw.get("links", []),
        "deleted": raw.get("deleted", False),
    }


class IngestTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()

    def test_happy_path_writes_masks_embeds(self):
        rep = ingest("src", [{"id": 1, "title": "Lunch", "body": "email boss@corp.com re lunch"}], _map)
        self.assertEqual(rep.written, 1)
        rec = CacheRecord.objects.get(pk="src:x.n:1")
        self.assertNotIn("boss@corp.com", rec.body_text)
        self.assertTrue(rec.has_embedding)
        tok = rec.body_text.split("email ")[1].split(" re")[0]
        self.assertEqual(detokenize(tok), "boss@corp.com")

    def test_mapper_none_is_skipped(self):
        rep = ingest("src", [{"id": 2, "drop": True}], _map)
        self.assertEqual((rep.written, rep.skipped, rep.failed), (0, 1, 0))

    def test_unchanged_second_pass_skipped(self):
        rec = {"id": 3, "title": "T", "body": "b"}
        ingest("src", [rec], _map)
        rep = ingest("src", [rec], _map)
        self.assertEqual((rep.written, rep.skipped), (0, 1))

    def test_bad_record_isolated(self):
        def bad_map(raw):
            if raw["id"] == 9:
                raise ValueError("boom")
            return _map(raw)

        rep = ingest("src", [{"id": 8, "title": "ok", "body": "x"}, {"id": 9}, {"id": 10, "title": "ok2", "body": "y"}], bad_map)
        self.assertEqual(rep.written, 2)
        self.assertEqual(rep.failed, 1)
        self.assertEqual(CacheRecord.objects.count(), 2)

    def test_credential_values_tokenized(self):
        rep = ingest(
            "src",
            [{"id": 4, "title": "cfg", "body": "token is SEKRET-abc", "payload": {"k": "SEKRET-abc"}}],
            _map,
            secret_values=["SEKRET-abc"],
        )
        rec = CacheRecord.objects.get(pk="src:x.n:4")
        self.assertNotIn("SEKRET-abc", rec.body_text)
        self.assertNotIn("SEKRET-abc", str(rec.payload))
        self.assertIn("[eunomia:credential:", rec.body_text)

    def test_links_created(self):
        ingest("src", [{"id": 5, "title": "A", "body": "a", "links": [{"rel": "about", "target": "src:x.n:6"}]}], _map)
        self.assertTrue(CacheLink.objects.filter(source_id="src:x.n:5", rel="about").exists())

    def test_deleted_record_not_embedded(self):
        rep = ingest("src", [{"id": 7, "title": "gone", "body": "x", "deleted": True}], _map)
        self.assertEqual(rep.written, 1)
        self.assertFalse(CacheRecord.objects.get(pk="src:x.n:7").has_embedding)
