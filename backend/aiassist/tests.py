from connectors.demo_seed import seed_demo_bank_data, seed_demo_google_data
from django.test import TestCase

from . import tools


class ToolsTests(TestCase):
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
