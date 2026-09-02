from unittest.mock import patch

from connectors.demo_seed import seed_demo_bank_data, seed_demo_google_data
from django.test import TestCase

from tasks.models import Project, Task

from . import tools, views


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

    def test_list_calendar_events_without_google_returns_error(self):
        result = tools.list_calendar_events()
        self.assertIn("error", result)

    def test_list_recent_transactions_without_up_bank_returns_error(self):
        result = tools.list_recent_transactions()
        self.assertIn("error", result)

    def test_search_emails_without_google_returns_error(self):
        result = tools.search_emails("picnic")
        self.assertIn("error", result)

    def test_list_calendar_events_uses_demo_data_when_google_is_in_demo_mode(self):
        seed_demo_google_data(seed=1)
        results = tools.list_calendar_events()
        self.assertIsInstance(results, list)
        self.assertGreater(len(results), 0)
        self.assertIn("summary", results[0])

    def test_search_emails_uses_demo_data_when_google_is_in_demo_mode(self):
        seed_demo_google_data(seed=1)
        results = tools.search_emails("anything")
        self.assertIsInstance(results, list)
        self.assertGreater(len(results), 0)
        self.assertIn("subject", results[0])

    def test_list_recent_transactions_uses_demo_data_when_up_bank_is_in_demo_mode(self):
        seed_demo_bank_data(seed=1)
        results = tools.list_recent_transactions(days=45)
        self.assertIsInstance(results, list)
        self.assertGreater(len(results), 0)
        self.assertIn("amount", results[0])


class GenerateTaskDetailsViewTests(TestCase):
    def test_requires_a_title(self):
        response = self.client.post("/api/ai/generate-task-details", {}, content_type="application/json")
        self.assertEqual(response.status_code, 400)

    def test_without_llm_configured_returns_400(self):
        response = self.client.post(
            "/api/ai/generate-task-details", {"title": "Picnic with Sam"}, content_type="application/json"
        )
        self.assertEqual(response.status_code, 400)
        self.assertIn("Settings", response.json()["detail"])

    def test_success_shape_and_only_uses_read_only_tools(self):
        with patch("aiassist.views.run_tool_loop") as mock_loop:
            mock_loop.return_value = (
                {"role": "assistant", "content": "Bring the blanket you bought at the shop from that receipt."},
                [{"tool": "list_recent_transactions", "args": {}, "result": []}],
            )
            response = self.client.post(
                "/api/ai/generate-task-details",
                {"title": "Picnic with Sam", "project_name": "Weekend"},
                content_type="application/json",
            )

        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertIn("blanket", data["description"])
        self.assertEqual(len(data["tool_trace"]), 1)

        # the tool set handed to the model must not include anything that mutates data
        _, kwargs = mock_loop.call_args
        tool_names = {s["function"]["name"] for s in kwargs["tool_schemas"]}
        self.assertNotIn("create_task", tool_names)
        self.assertNotIn("update_task", tool_names)
        self.assertNotIn("list_tasks", tool_names)

    def test_passes_both_title_and_existing_notes_as_context(self):
        with patch("aiassist.views.run_tool_loop") as mock_loop:
            mock_loop.return_value = ({"role": "assistant", "content": "..."}, [])
            self.client.post(
                "/api/ai/generate-task-details",
                {"title": "Picnic with Sam", "notes": "already booked the park"},
                content_type="application/json",
            )

        args, _ = mock_loop.call_args
        user_message = args[1][0]["content"]
        self.assertIn("Picnic with Sam", user_message)
        self.assertIn("already booked the park", user_message)


class SuggestTasksViewTests(TestCase):
    def test_without_llm_configured_returns_400(self):
        response = self.client.post("/api/ai/suggest-tasks")
        self.assertEqual(response.status_code, 400)

    def test_parses_a_json_array_response_into_suggestions(self):
        with patch("aiassist.views.run_tool_loop") as mock_loop:
            mock_loop.return_value = (
                {
                    "role": "assistant",
                    "content": '[{"title": "Prep for standup", "notes": "review yesterday\'s PRs", "due_at": null}]',
                },
                [{"tool": "list_calendar_events", "args": {}, "result": []}],
            )
            response = self.client.post("/api/ai/suggest-tasks")

        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertEqual(len(data["suggestions"]), 1)
        self.assertEqual(data["suggestions"][0]["title"], "Prep for standup")
        self.assertEqual(len(data["tool_trace"]), 1)

    def test_tolerates_a_model_that_wraps_the_array_in_prose_or_fences(self):
        with patch("aiassist.views.run_tool_loop") as mock_loop:
            mock_loop.return_value = (
                {"role": "assistant", "content": 'Sure, here you go:\n```json\n[{"title": "Pay Netflix"}]\n```'},
                [],
            )
            response = self.client.post("/api/ai/suggest-tasks")

        self.assertEqual(response.json()["suggestions"], [{"title": "Pay Netflix"}])

    def test_empty_array_response_means_no_suggestions(self):
        with patch("aiassist.views.run_tool_loop") as mock_loop:
            mock_loop.return_value = ({"role": "assistant", "content": "[]"}, [])
            response = self.client.post("/api/ai/suggest-tasks")

        self.assertEqual(response.json()["suggestions"], [])

    def test_unparseable_response_degrades_to_no_suggestions_not_an_error(self):
        with patch("aiassist.views.run_tool_loop") as mock_loop:
            mock_loop.return_value = ({"role": "assistant", "content": "not json at all"}, [])
            response = self.client.post("/api/ai/suggest-tasks")

        self.assertEqual(response.status_code, 200)
        self.assertEqual(response.json()["suggestions"], [])

    def test_only_uses_read_only_tools(self):
        with patch("aiassist.views.run_tool_loop") as mock_loop:
            mock_loop.return_value = ({"role": "assistant", "content": "[]"}, [])
            self.client.post("/api/ai/suggest-tasks")

        _, kwargs = mock_loop.call_args
        tool_names = {s["function"]["name"] for s in kwargs["tool_schemas"]}
        self.assertNotIn("create_task", tool_names)
        self.assertNotIn("update_task", tool_names)


class ResearchToolRestrictionTests(TestCase):
    def test_research_tools_are_read_only(self):
        self.assertNotIn("create_task", views.RESEARCH_TOOL_IMPLS)
        self.assertNotIn("update_task", views.RESEARCH_TOOL_IMPLS)
        self.assertIn("search_emails", views.RESEARCH_TOOL_IMPLS)
        self.assertIn("list_calendar_events", views.RESEARCH_TOOL_IMPLS)
        self.assertIn("list_recent_transactions", views.RESEARCH_TOOL_IMPLS)
