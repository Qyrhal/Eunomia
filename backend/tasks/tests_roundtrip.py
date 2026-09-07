"""#48 round-trip regression tests: props, links, get, and generic search must
work identically from every surface (MCP tools via registry.call, REST tool
endpoint, and the REST task API)."""

from datetime import datetime, timezone as dt_tz

from django.test import TestCase

from cache.models import CacheLink
from cache.search import upsert
from connectors.models import AppSettings
from tasks.graph import create_task, link_task, resolve_task, task_links, vid
from tools import registry

_T = datetime(2026, 1, 1, tzinfo=dt_tz.utc)


class RoundTripBase(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()


class PropsRoundTripTests(RoundTripBase):
    def setUp(self):
        super().setUp()
        from tasks.models import Project

        self.inbox = Project.objects.create(name="Inbox")

    def test_mcp_create_task_echoes_and_persists_props(self):
        out = registry.call(
            "create_task",
            {"title": "Read Dune", "props": {"kind": "book", "pages": 412}, "tags": ["reading"]},
        )
        self.assertTrue(out["created"])
        self.assertEqual(out["props"], {"kind": "book", "pages": 412})
        self.assertEqual(out["tags"], ["reading"])
        from tasks.models import Task

        t = Task.objects.get(pk=out["id"])
        self.assertEqual(t.props, {"kind": "book", "pages": 412})

    def test_mcp_update_task_replaces_props(self):
        tid = create_task("empty props")["id"]
        out = registry.call("update_task", {"id": tid, "props": {"a": 1}})
        self.assertEqual(out["props"], {"a": 1})
        self.assertEqual(resolve_task(tid).props, {"a": 1})

    def test_rest_task_api_round_trips_props(self):
        r = self.client.post(
            "/api/tasks/",
            {"title": "api props", "project": str(self.inbox.id), "props": {"entity": "API", "qty": 3}},
            content_type="application/json",
        )
        self.assertEqual(r.status_code, 201)
        self.assertEqual(r.json()["props"], {"entity": "API", "qty": 3})

        tid = r.json()["id"]
        r = self.client.patch(
            f"/api/tasks/{tid}/", {"props": {"entity": "API", "qty": 4}}, content_type="application/json"
        )
        self.assertEqual(r.status_code, 200)
        self.assertEqual(r.json()["props"], {"entity": "API", "qty": 4})

        r = self.client.get(f"/api/tasks/{tid}/")
        self.assertEqual(r.json()["props"], {"entity": "API", "qty": 4})

    def test_rest_tool_endpoint_creates_task_with_props(self):
        r = self.client.post(
            "/api/tools/create_task",
            {"title": "via tool rest", "props": {"src": "rest"}},
            content_type="application/json",
        )
        self.assertEqual(r.status_code, 200)
        self.assertEqual(r.json()["props"], {"src": "rest"})


class LinkRoundTripTests(RoundTripBase):
    def test_link_task_then_generic_links_sees_it(self):
        a = create_task("first")["id"]
        b = create_task("second")["id"]
        out = link_task(a, "mentions", b)
        self.assertTrue(out["linked"])
        self.assertEqual(out["source_id"], f"task:{a}")
        self.assertEqual(out["target_id"], f"task:{b}")

        # generic links tool, addressed by bare UUID (the #48 repro)
        self.assertEqual(
            registry.call("links", {"id": a}),
            {"links": [{"rel": "mentions", "direction": "out", "target_id": f"task:{b}"}]},
        )
        # and the reverse edge from the target, also by bare UUID
        self.assertEqual(
            registry.call("links", {"id": b}),
            {"links": [{"rel": "mentions", "direction": "in", "target_id": f"task:{a}"}]},
        )
        # task_links accepts both forms too
        self.assertEqual(len(task_links(f"task:{a}")["links"]), 1)

    def test_link_task_to_cache_record_via_registry(self):
        upsert({"id": "up_bank:up.transaction:9", "source": "up_bank", "type": "up.transaction",
                "external_id": "9", "title": "Coffee", "body_text": "flat white",
                "occurred_at": _T, "url": "", "payload": {}, "links": [], "deleted": False})
        tid = create_task("expenses")["id"]
        out = registry.call("link_task", {"id": tid, "rel": "about", "target_id": "up_bank:up.transaction:9"})
        self.assertTrue(out["linked"])

        got = registry.call("links", {"id": tid})["links"]
        self.assertEqual(got, [{"rel": "about", "direction": "out", "target_id": "up_bank:up.transaction:9"}])
        # reverse direction from the cache record side
        back = registry.call("links", {"id": "up_bank:up.transaction:9"})["links"]
        self.assertEqual(back, [{"rel": "about", "direction": "in", "target_id": f"task:{tid}"}])

    def test_rest_task_links_endpoint(self):
        a = create_task("one")["id"]
        b = create_task("two")["id"]
        link_task(a, "blocks", b)

        r = self.client.get(f"/api/tasks/{a}/links/")
        self.assertEqual(r.status_code, 200)
        self.assertEqual(
            r.json()["links"], [{"rel": "blocks", "direction": "out", "target_id": f"task:{b}"}]
        )
        self.assertEqual(self.client.get("/api/tasks/00000000-0000-0000-0000-000000000000/links/").status_code, 404)

    def test_unlink_accepts_bare_uuid_target(self):
        a = create_task("x")["id"]
        b = create_task("y")["id"]
        link_task(a, "mentions", b)
        from tasks.graph import unlink_task

        self.assertTrue(unlink_task(a, "mentions", b)["unlinked"])
        self.assertFalse(CacheLink.objects.filter(source_id=f"task:{a}").exists())


class TaskIdResolutionTests(RoundTripBase):
    def test_resolve_task_accepts_uuid_and_vid_only(self):
        tid = create_task("resolvable")["id"]
        self.assertEqual(str(resolve_task(tid).id), tid)
        self.assertEqual(str(resolve_task(f"task:{tid}").id), tid)
        self.assertIsNone(resolve_task("up_bank:up.transaction:1"))
        self.assertIsNone(resolve_task("not-a-uuid"))
        self.assertIsNone(resolve_task(""))

    def test_generic_get_resolves_task_by_uuid_and_vid(self):
        tid = create_task("find me", notes="the notes", props={"k": "v"})["id"]
        for ref in (tid, f"task:{tid}"):
            out = registry.call("get", {"id": ref})
            self.assertEqual(out["source"], "tasks")
            self.assertEqual(out["type"], "task")
            self.assertEqual(out["title"], "find me")
            self.assertEqual(out["payload"]["props"], {"k": "v"})

    def test_generic_get_unknown_id_is_clean_error(self):
        self.assertEqual(registry.call("get", {"id": "does:not:exist"})["error"], "not found")

    def test_update_and_schedule_accept_vid(self):
        from tasks.graph import schedule_task, update_task

        tid = create_task("before")["id"]
        self.assertTrue(update_task(f"task:{tid}", title="after")["updated"])
        self.assertEqual(resolve_task(tid).title, "after")
        out = schedule_task(f"task:{tid}", "2026-06-01T09:00:00Z")
        self.assertTrue(out["scheduled_for"])
        self.assertIsNotNone(resolve_task(tid).due_at)


class GenericSearchTests(RoundTripBase):
    def setUp(self):
        super().setUp()
        upsert({"id": "up_bank:up.transaction:1", "source": "up_bank", "type": "up.transaction",
                "external_id": "1", "title": "Rent", "body_text": "monthly rent payment",
                "occurred_at": _T, "url": "", "payload": {}, "links": [], "deleted": False})
        create_task("Plan the picnic", notes="park near the lake", tags=["social"])
        create_task("Call the dentist", notes="reschedule cleaning")

    def _titles(self, **kw):
        return [h["title"] for h in registry.call("search", {"query": kw.pop("query"), **kw})["results"]]

    def test_generic_search_finds_tasks_by_exact_title(self):
        self.assertIn("Plan the picnic", self._titles(query="picnic"))

    def test_generic_search_finds_tasks_by_tag(self):
        self.assertIn("Plan the picnic", self._titles(query="social"))

    def test_generic_search_finds_cache_records_and_tasks_together(self):
        titles = self._titles(query="rent", limit=20)
        self.assertIn("Rent", titles)

    def test_generic_search_type_task_filter_excludes_cache_records(self):
        titles = self._titles(query="rent", types=["task"])
        self.assertNotIn("Rent", titles)  # the cache record is filtered out

    def test_generic_search_sources_tasks_only(self):
        titles = self._titles(query="picnic", sources=["tasks"])
        self.assertIn("Plan the picnic", titles)
        self.assertNotIn("Rent", titles)  # cache records are filtered out

    def test_search_tasks_tool_still_works(self):
        out = registry.call("search_tasks", {"query": "picnic"})
        self.assertIn("Plan the picnic", [h["title"] for h in out["results"]])
        self.assertTrue(all(h["vid"].startswith("task:") for h in out["results"]))
