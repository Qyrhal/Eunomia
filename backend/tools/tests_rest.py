from datetime import datetime, timezone as dt_tz

from django.test import TestCase

from cache.search import upsert
from connectors.models import AppSettings

_T = datetime(2026, 1, 1, tzinfo=dt_tz.utc)


class ToolRestTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()
        upsert({"id": "up_bank:up.transaction:1", "source": "up_bank", "type": "up.transaction",
                "external_id": "1", "title": "Rent", "body_text": "monthly rent", "occurred_at": _T,
                "url": "", "payload": {}, "links": [], "deleted": False})

    def test_catalogue_lists_generic_and_task_and_trigger_tools(self):
        r = self.client.get("/api/tools")
        self.assertEqual(r.status_code, 200)
        names = {t["name"] for t in r.json()}
        self.assertTrue({"search", "get", "list", "links", "create_task", "create_trigger"} <= names)
        for t in r.json():
            self.assertIn("schema", t)

    def test_call_search(self):
        r = self.client.post("/api/tools/search", {"query": "rent", "mode": "keyword"}, content_type="application/json")
        self.assertEqual(r.status_code, 200)
        self.assertEqual(r.json()["results"][0]["id"], "up_bank:up.transaction:1")

    def test_call_get(self):
        r = self.client.post("/api/tools/get", {"id": "up_bank:up.transaction:1"}, content_type="application/json")
        self.assertEqual(r.json()["title"], "Rent")

    def test_unknown_tool_404(self):
        self.assertEqual(self.client.post("/api/tools/nope", {}, content_type="application/json").status_code, 404)

    def test_bad_args_400(self):
        r = self.client.post("/api/tools/get", {"wrong": 1}, content_type="application/json")
        self.assertEqual(r.status_code, 400)

    def test_create_task_via_rest(self):
        r = self.client.post("/api/tools/create_task", {"title": "from rest"}, content_type="application/json")
        self.assertEqual(r.status_code, 200)
        self.assertTrue(r.json()["created"])
