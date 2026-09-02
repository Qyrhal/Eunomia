from django.test import TestCase

from cache.models import CacheLink
from connectors.models import AppSettings
from tasks.graph import (
    create_task, link_task, schedule_task, search_tasks, task_links,
    unlink_task, update_task, vid,
)
from tasks.models import Task
from tools import registry


class TaskGraphTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()

    def test_create_with_tags_props_and_embedding(self):
        out = create_task("Plan the picnic", notes="park near the lake", tags=["social"], props={"mood": "fun"})
        t = Task.objects.get(pk=out["id"])
        self.assertEqual(list(t.tags.values_list("name", flat=True)), ["social"])
        self.assertEqual(t.props, {"mood": "fun"})
        self.assertTrue(t.has_embedding)

    def test_update_fields_and_tags(self):
        tid = create_task("x")["id"]
        update_task(tid, title="y", completed=True, tags=["a", "b"])
        t = Task.objects.get(pk=tid)
        self.assertEqual(t.title, "y")
        self.assertTrue(t.completed)
        self.assertIsNotNone(t.completed_at)
        self.assertEqual(set(t.tags.values_list("name", flat=True)), {"a", "b"})

    def test_link_and_unlink_to_cache_id(self):
        tid = create_task("linked")["id"]
        link_task(tid, "about", "up_bank:up.transaction:9")
        self.assertEqual(
            CacheLink.objects.get(source_id=f"task:{tid}").origin, CacheLink.ORIGIN_AGENT
        )
        self.assertEqual(task_links(tid)["links"][0]["target_id"], "up_bank:up.transaction:9")
        unlink_task(tid, "about", "up_bank:up.transaction:9")
        self.assertFalse(CacheLink.objects.filter(source_id=f"task:{tid}").exists())

    def test_schedule_sets_due(self):
        tid = create_task("do it")["id"]
        schedule_task(tid, "2026-06-01T09:00:00Z")
        self.assertIsNotNone(Task.objects.get(pk=tid).due_at)

    def test_search_tasks_keyword_and_semantic(self):
        create_task("Buy groceries", notes="milk eggs bread")
        create_task("Call dentist", notes="reschedule cleaning")
        hits = search_tasks("groceries")["results"]
        self.assertIn("Buy groceries", [h["title"] for h in hits])

    def test_tools_registered(self):
        reg = registry.all_tools()
        for name in ("create_task", "update_task", "link_task", "schedule_task", "search_tasks"):
            self.assertIn(name, reg)

    def test_orm_created_task_gets_indexed_by_signal(self):
        from tasks.models import Project

        proj = Project.objects.create(name="Inbox")
        t = Task.objects.create(project=proj, title="via orm not the tool")
        t.refresh_from_db()
        self.assertTrue(t.has_embedding)

    def test_vid_shape(self):
        t = Task.objects.get(pk=create_task("v")["id"])
        self.assertEqual(vid(t), f"task:{t.id}")


class DateCoercionTests(TestCase):
    def setUp(self):
        from connectors.models import AppSettings
        s = AppSettings.load(); s.embedding_backend = AppSettings.EMBED_STUB; s.save()

    def test_create_task_accepts_a_bare_date_string(self):
        from django.utils import timezone as tz
        out = create_task("call bank", due_at="2026-09-02")
        t = Task.objects.get(pk=out["id"])
        self.assertIsNotNone(t.due_at)
        self.assertFalse(tz.is_naive(t.due_at))

    def test_schedule_task_accepts_iso_datetime(self):
        tid = create_task("x")["id"]
        from tasks.graph import schedule_task
        schedule_task(tid, "2026-06-01T09:00:00Z")
        self.assertIsNotNone(Task.objects.get(pk=tid).due_at)
