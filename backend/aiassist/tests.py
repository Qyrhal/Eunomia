from connectors.demo_seed import seed_demo_bank_data
from django.test import TestCase

from tasks.models import Project, Task

from . import tools


class ToolsTests(TestCase):
    def test_create_task_creates_project_if_missing_and_marks_ai_authored(self):
        result = tools.create_task(title="prep for standup", project_name="Meetings")
        self.assertTrue(result["created"])

        task = Task.objects.get(id=result["id"])
        self.assertTrue(task.created_by_ai)
        self.assertEqual(task.project.name, "Meetings")

    def test_create_task_defaults_to_inbox(self):
        result = tools.create_task(title="misc")
        task = Task.objects.get(id=result["id"])
        self.assertEqual(task.project.name, "Inbox")

    def test_list_tasks_filters_by_project_name(self):
        work = Project.objects.create(name="Work")
        Project.objects.create(name="Home")
        Task.objects.create(project=work, title="in work")
        tools.create_task(title="in home", project_name="Home")

        results = tools.list_tasks(project_name="Work")
        self.assertEqual([r["title"] for r in results], ["in work"])

    def test_update_task_completes_and_sets_timestamp(self):
        project = Project.objects.create(name="Inbox")
        task = Task.objects.create(project=project, title="finish this")

        result = tools.update_task(str(task.id), completed=True)
        self.assertTrue(result["updated"])

        task.refresh_from_db()
        self.assertTrue(task.completed)
        self.assertIsNotNone(task.completed_at)

    def test_update_task_unknown_id_returns_error_not_exception(self):
        result = tools.update_task("00000000-0000-0000-0000-000000000000", completed=True)
        self.assertIn("error", result)

    def test_list_recent_transactions_without_up_bank_returns_error(self):
        result = tools.list_recent_transactions()
        self.assertIn("error", result)

    def test_list_recent_transactions_uses_demo_data_when_up_bank_is_in_demo_mode(self):
        seed_demo_bank_data(seed=1)
        results = tools.list_recent_transactions(days=45)
        self.assertIsInstance(results, list)
        self.assertGreater(len(results), 0)
        self.assertIn("amount", results[0])
