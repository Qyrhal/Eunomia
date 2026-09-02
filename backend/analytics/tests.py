from datetime import timedelta

from django.test import TestCase
from django.utils import timezone

from tasks.models import Project, Task


class OverviewTests(TestCase):
    def test_counts(self):
        project = Project.objects.create(name="Inbox")
        now = timezone.now()
        Task.objects.create(project=project, title="open")
        Task.objects.create(project=project, title="overdue", due_at=now - timedelta(days=1))
        Task.objects.create(project=project, title="flagged", flagged=True)
        Task.objects.create(project=project, title="done", completed=True)

        response = self.client.get("/api/analytics/overview")
        data = response.json()
        self.assertEqual(data["open"], 3)
        self.assertEqual(data["completed"], 1)
        self.assertEqual(data["overdue"], 1)
        self.assertEqual(data["flagged"], 1)


class ProjectBreakdownTests(TestCase):
    def test_groups_open_tasks_by_project(self):
        inbox = Project.objects.create(name="Inbox", color="#111111")
        work = Project.objects.create(name="Work", color="#222222")
        Task.objects.create(project=inbox, title="a")
        Task.objects.create(project=inbox, title="b")
        Task.objects.create(project=work, title="c", completed=True)

        response = self.client.get("/api/analytics/project-breakdown")
        rows = {r["project__name"]: r["count"] for r in response.json()}
        self.assertEqual(rows.get("Inbox"), 2)
        self.assertNotIn("Work", rows)  # only completed task, filtered out


class UpcomingLoadTests(TestCase):
    def test_minutes_are_summed_not_counted(self):
        # Regression test: this endpoint used to do Count("allocated_minutes")
        # instead of Sum, so multiple 30-minute tasks on the same day reported
        # a "minutes" count instead of a total.
        project = Project.objects.create(name="Inbox")
        due = timezone.now() + timedelta(days=1)
        Task.objects.create(project=project, title="a", due_at=due, allocated_minutes=30)
        Task.objects.create(project=project, title="b", due_at=due, allocated_minutes=45)

        response = self.client.get("/api/analytics/upcoming-load")
        rows = response.json()
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["count"], 2)
        self.assertEqual(rows[0]["minutes"], 75)


class AiContributionTests(TestCase):
    def test_only_counts_open_tasks(self):
        project = Project.objects.create(name="Inbox")
        Task.objects.create(project=project, title="ai open", created_by_ai=True)
        Task.objects.create(project=project, title="ai done", created_by_ai=True, completed=True)
        Task.objects.create(project=project, title="human open", created_by_ai=False)

        response = self.client.get("/api/analytics/ai-contribution")
        data = response.json()
        self.assertEqual(data["ai_created"], 1)
        self.assertEqual(data["human_created"], 1)
