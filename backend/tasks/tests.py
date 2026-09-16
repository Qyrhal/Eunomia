from datetime import timezone as dt_timezone

from django.test import TestCase
from django.utils import timezone

from .demo_seed import DEMO_PREFIX, clear_demo_data, seed_demo_data
from .models import Project, Tag, Task


class TaskModelTests(TestCase):
    def test_completing_a_task_sets_completed_at(self):
        project = Project.objects.create(name="Inbox")
        task = Task.objects.create(project=project, title="Do the thing")
        self.assertIsNone(task.completed_at)

        task.completed = True
        task.save()
        self.assertIsNotNone(task.completed_at)

    def test_uncompleting_a_task_clears_completed_at(self):
        project = Project.objects.create(name="Inbox")
        task = Task.objects.create(project=project, title="Do the thing", completed=True)
        self.assertIsNotNone(task.completed_at)

        task.completed = False
        task.save()
        self.assertIsNone(task.completed_at)


class TaskApiTests(TestCase):
    def setUp(self):
        self.inbox = Project.objects.create(name="Inbox")
        self.work = Project.objects.create(name="Work")

    def test_create_and_filter_by_project(self):
        self.client.post("/api/tasks/", {"title": "personal", "project": str(self.inbox.id)})
        self.client.post("/api/tasks/", {"title": "meeting prep", "project": str(self.work.id)})

        response = self.client.get(f"/api/tasks/?project={self.work.id}")
        self.assertEqual(response.status_code, 200)
        titles = [t["title"] for t in response.json()]
        self.assertEqual(titles, ["meeting prep"])

    def test_project_open_count_excludes_completed_and_subtasks(self):
        parent = Task.objects.create(project=self.inbox, title="parent")
        Task.objects.create(project=self.inbox, title="subtask", parent=parent)
        Task.objects.create(project=self.inbox, title="done", completed=True)

        response = self.client.get("/api/projects/")
        inbox = next(p for p in response.json() if p["id"] == str(self.inbox.id))
        self.assertEqual(inbox["open_count"], 1)

    def test_flag_and_complete_via_patch(self):
        task = Task.objects.create(project=self.inbox, title="flag me")
        response = self.client.patch(
            f"/api/tasks/{task.id}/",
            {"flagged": True, "completed": True},
            content_type="application/json",
        )
        self.assertEqual(response.status_code, 200)
        task.refresh_from_db()
        self.assertTrue(task.flagged)
        self.assertTrue(task.completed)
        self.assertLessEqual(task.completed_at, timezone.now())

    def test_creating_a_task_with_new_tag_names_creates_the_tags(self):
        response = self.client.post(
            "/api/tasks/",
            {"title": "plan trip", "project": str(self.inbox.id), "tags": ["travel", "urgent"]},
            content_type="application/json",
        )
        self.assertEqual(response.status_code, 201)
        self.assertEqual(sorted(response.json()["tags"]), ["travel", "urgent"])
        self.assertEqual(Tag.objects.filter(name__in=["travel", "urgent"]).count(), 2)

    def test_reusing_a_tag_name_does_not_duplicate_it(self):
        Tag.objects.create(name="urgent")
        self.client.post(
            "/api/tasks/",
            {"title": "a", "project": str(self.inbox.id), "tags": ["urgent"]},
            content_type="application/json",
        )
        self.client.post(
            "/api/tasks/",
            {"title": "b", "project": str(self.inbox.id), "tags": ["urgent"]},
            content_type="application/json",
        )
        self.assertEqual(Tag.objects.filter(name="urgent").count(), 1)

    def test_filtering_tasks_by_tag(self):
        travel = Task.objects.create(project=self.inbox, title="pack bags")
        travel.tags.set([Tag.objects.create(name="travel")])
        Task.objects.create(project=self.inbox, title="unrelated")

        response = self.client.get("/api/tasks/?tag=travel")
        titles = [t["title"] for t in response.json()]
        self.assertEqual(titles, ["pack bags"])

    def test_top_level_filter_excludes_or_includes_subtasks(self):
        parent = Task.objects.create(project=self.inbox, title="parent")
        Task.objects.create(project=self.inbox, title="child", parent=parent)

        top_level = [t["title"] for t in self.client.get("/api/tasks/?top_level=true").json()]
        self.assertIn("parent", top_level)
        self.assertNotIn("child", top_level)

        subtasks_only = [t["title"] for t in self.client.get("/api/tasks/?top_level=false").json()]
        self.assertEqual(subtasks_only, ["child"])

    def test_creating_a_subtask_via_parent_field(self):
        parent = Task.objects.create(project=self.inbox, title="parent")
        response = self.client.post(
            "/api/tasks/",
            {"title": "child", "project": str(self.inbox.id), "parent": str(parent.id)},
            content_type="application/json",
        )
        self.assertEqual(response.status_code, 201)

        parent_response = self.client.get(f"/api/tasks/?parent={parent.id}")
        self.assertEqual([t["title"] for t in parent_response.json()], ["child"])

    def test_completing_a_recurring_task_creates_the_next_occurrence(self):
        task = Task.objects.create(
            project=self.inbox,
            title="water plants",
            due_at=timezone.datetime(2026, 1, 1, tzinfo=dt_timezone.utc),
            recurrence=Task.Recurrence.WEEKLY,
        )
        response = self.client.patch(
            f"/api/tasks/{task.id}/", {"completed": True}, content_type="application/json"
        )
        self.assertEqual(response.status_code, 200)

        clones = Task.objects.filter(title="water plants", completed=False)
        self.assertEqual(clones.count(), 1)
        self.assertEqual(clones.first().due_at, timezone.datetime(2026, 1, 8, tzinfo=dt_timezone.utc))

    def test_completing_a_non_recurring_task_does_not_clone_it(self):
        task = Task.objects.create(project=self.inbox, title="one-off")
        self.client.patch(f"/api/tasks/{task.id}/", {"completed": True}, content_type="application/json")
        self.assertEqual(Task.objects.filter(title="one-off").count(), 1)

    def test_task_context_endpoint_returns_matching_emails_and_events(self):
        task = Task.objects.create(project=self.inbox, title="renew passport")
        response = self.client.get(f"/api/tasks/{task.id}/context/")
        self.assertEqual(response.status_code, 200)
        self.assertIn("emails", response.json())
        self.assertIn("events", response.json())


class DemoSeedTests(TestCase):
    def test_seed_creates_prefixed_projects_and_tasks(self):
        result = seed_demo_data(seed=1)
        self.assertEqual(result["projects"], 4)
        self.assertEqual(result["tasks"], 50)
        self.assertEqual(Project.objects.filter(name__startswith=DEMO_PREFIX).count(), 4)
        self.assertEqual(Task.objects.filter(project__name__startswith=DEMO_PREFIX).count(), 50)

    def test_reseeding_replaces_the_previous_batch_not_stacks_on_it(self):
        seed_demo_data(seed=1)
        seed_demo_data(seed=2)
        self.assertEqual(Project.objects.filter(name__startswith=DEMO_PREFIX).count(), 4)
        self.assertEqual(Task.objects.filter(project__name__startswith=DEMO_PREFIX).count(), 50)

    def test_seeding_does_not_touch_real_projects(self):
        real = Project.objects.create(name="Not demo at all")
        Task.objects.create(project=real, title="my real task")

        seed_demo_data(seed=1)

        real.refresh_from_db()
        self.assertEqual(Task.objects.filter(project=real).count(), 1)

    def test_clear_only_removes_demo_projects(self):
        real = Project.objects.create(name="Keep me")
        seed_demo_data(seed=1)

        result = clear_demo_data()

        self.assertEqual(result["projects_removed"], 4)
        self.assertEqual(Project.objects.filter(name__startswith=DEMO_PREFIX).count(), 0)
        self.assertTrue(Project.objects.filter(id=real.id).exists())

    def test_demo_data_endpoint_seeds_and_clears(self):
        response = self.client.post("/api/demo-data")
        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertEqual(data["projects"], 4)
        self.assertEqual(data["tasks"], 50)
        self.assertGreater(data["transactions"], 0)

        response = self.client.delete("/api/demo-data")
        self.assertEqual(response.status_code, 200)
        self.assertEqual(response.json()["projects_removed"], 4)
        self.assertGreater(response.json()["transactions_removed"], 0)

    def test_seeding_also_enables_up_bank_in_demo_mode(self):
        from connectors.models import Connector

        seed_demo_data(seed=1)
        connector = Connector.objects.get(kind=Connector.Kind.UP_BANK)
        self.assertTrue(connector.enabled)
        self.assertTrue(connector.config.get("demo"))

    def test_clearing_disables_up_bank_demo_mode_again(self):
        from connectors.models import Connector

        seed_demo_data(seed=1)
        clear_demo_data()
        connector = Connector.objects.get(kind=Connector.Kind.UP_BANK)
        self.assertFalse(connector.config.get("demo"))
        self.assertFalse(connector.enabled)


class TimezoneHandlingTests(TestCase):
    """Naive agent dates follow AppSettings.user_timezone, not the server zone."""

    def test_naive_due_at_interpreted_in_user_timezone(self):
        from connectors.models import AppSettings

        from .serializers import TaskSerializer

        AppSettings.load()  # ensure the singleton row exists before updating
        AppSettings.objects.update(user_timezone="Australia/Melbourne")
        s = TaskSerializer(context={})
        dt = s.fields["due_at"].to_internal_value("2026-09-07T17:00:00")
        # Melbourne is UTC+10 in September: 5pm local == 07:00 UTC
        self.assertEqual(dt.isoformat(), "2026-09-07T17:00:00+10:00")

    def test_aware_due_at_keeps_explicit_offset(self):
        from .serializers import TaskSerializer

        s = TaskSerializer(context={})
        dt = s.fields["due_at"].to_internal_value("2026-09-07T17:00:00+02:00")
        self.assertEqual(dt.utcoffset().total_seconds(), 7200)
