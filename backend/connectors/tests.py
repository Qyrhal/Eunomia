from unittest.mock import patch

from django.test import TestCase
from django.utils import timezone

from .demo_seed import (
    build_demo_finance_summary,
    build_demo_week_summary,
    clear_demo_bank_data,
    clear_demo_google_data,
    clear_demo_pocket_data,
    seed_demo_bank_data,
    seed_demo_google_data,
    seed_demo_pocket_data,
)
from .models import AppSettings, Connector, DemoCalendarEvent, DemoEmail, DemoRecording, DemoTransaction


class ConnectorListViewTests(TestCase):
    def test_repeated_gets_do_not_error(self):
        # Regression test: ConnectorListView used to call
        # `connectors.setdefault(kind, Connector.objects.create(kind=kind))`,
        # but dict.setdefault evaluates its default eagerly, so the second GET
        # tried to re-create a row that already existed and hit the unique
        # constraint on `kind`.
        for _ in range(3):
            response = self.client.get("/api/connectors")
            self.assertEqual(response.status_code, 200)
        self.assertEqual(Connector.objects.count(), len(Connector.Kind.choices))


class AppSettingsCryptoTests(TestCase):
    def test_llm_api_key_round_trips_encrypted(self):
        settings_row = AppSettings.load()
        settings_row.llm_api_key = "sk-secret"
        settings_row.save()

        reloaded = AppSettings.objects.get(pk=1)
        self.assertNotEqual(reloaded.llm_api_key_encrypted, "sk-secret")
        self.assertEqual(reloaded.llm_api_key, "sk-secret")

    def test_api_never_exposes_the_key_only_whether_it_is_set(self):
        settings_row = AppSettings.load()
        settings_row.llm_api_key = "sk-secret"
        settings_row.save()

        response = self.client.get("/api/settings")
        self.assertEqual(response.status_code, 200)
        self.assertNotIn("sk-secret", response.content.decode())
        self.assertTrue(response.json()["llm_api_key_set"])


class ConnectorCredentialsTests(TestCase):
    def test_credentials_round_trip_encrypted_and_stay_off_list_endpoint(self):
        connector = Connector.objects.create(kind=Connector.Kind.UP_BANK)
        connector.credentials = {"personal_access_token": "up:yeah:secret"}
        connector.save()

        reloaded = Connector.objects.get(kind=Connector.Kind.UP_BANK)
        self.assertNotIn("secret", reloaded.credentials_encrypted)
        self.assertEqual(reloaded.credentials["personal_access_token"], "up:yeah:secret")

        response = self.client.get("/api/connectors")
        self.assertNotIn("secret", response.content.decode())

    def test_patching_one_credential_field_does_not_drop_the_others(self):
        # Regression test: ConnectorSerializer.update used to do
        # `instance.credentials = credentials`, replacing the whole dict.
        # Saving just the client secret after the client ID was already
        # stored would silently wipe the client ID.
        self.client.patch(
            "/api/connectors/google",
            {"credentials": {"client_id": "abc.apps.googleusercontent.com"}},
            content_type="application/json",
        )
        self.client.patch(
            "/api/connectors/google",
            {"credentials": {"client_secret": "GOCSPX-secret"}},
            content_type="application/json",
        )

        connector = Connector.objects.get(kind=Connector.Kind.GOOGLE)
        self.assertEqual(connector.credentials["client_id"], "abc.apps.googleusercontent.com")
        self.assertEqual(connector.credentials["client_secret"], "GOCSPX-secret")


class DemoBankDataTests(TestCase):
    def test_seed_enables_up_bank_and_creates_transactions(self):
        seed_demo_bank_data(seed=1)
        connector = Connector.objects.get(kind=Connector.Kind.UP_BANK)
        self.assertTrue(connector.enabled)
        self.assertTrue(connector.config["demo"])
        self.assertGreater(DemoTransaction.objects.count(), 0)

    def test_clear_removes_transactions_and_disables_demo_mode(self):
        seed_demo_bank_data(seed=1)
        result = clear_demo_bank_data()
        self.assertGreater(result["transactions_removed"], 0)
        self.assertEqual(DemoTransaction.objects.count(), 0)
        connector = Connector.objects.get(kind=Connector.Kind.UP_BANK)
        self.assertNotIn("demo", connector.config)
        self.assertFalse(connector.enabled)

    def test_clear_preserves_a_real_connection_if_one_exists(self):
        connector = Connector.objects.create(kind=Connector.Kind.UP_BANK)
        connector.credentials = {"personal_access_token": "up:yeah:real"}
        connector.save()

        seed_demo_bank_data(seed=1)  # flips it into demo mode on top
        clear_demo_bank_data()

        connector.refresh_from_db()
        # demo mode is gone, but the real credential is still there, so it re-enables
        self.assertNotIn("demo", connector.config)
        self.assertTrue(connector.enabled)
        self.assertEqual(connector.credentials["personal_access_token"], "up:yeah:real")

    def test_finance_summary_endpoint_serves_demo_data_without_a_real_token(self):
        seed_demo_bank_data(seed=1)
        response = self.client.get("/api/connectors/up_bank/finance-summary?days=30")
        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertIn("balance", data)
        self.assertTrue(len(data["recent_transactions"]) > 0)
        self.assertTrue(len(data["spend_by_day"]) > 0)

    def test_snapshot_endpoint_reports_demo_up_bank_activity(self):
        seed_demo_bank_data(seed=1)
        response = self.client.get("/api/connectors/snapshot")
        data = response.json()
        self.assertIsNotNone(data["up_bank"])
        self.assertNotIn("error", data["up_bank"])

    def test_build_demo_finance_summary_only_counts_spend_not_income(self):
        DemoTransaction.objects.create(
            account="Spending", description="Salary", category="Income",
            amount_cents=300000, created_at=timezone.now(),
        )
        DemoTransaction.objects.create(
            account="Spending", description="Coffee", category="Dining out",
            amount_cents=-550, created_at=timezone.now(),
        )
        summary = build_demo_finance_summary(timezone.now() - timezone.timedelta(days=1))
        self.assertEqual(summary["spend_by_category"], [{"category": "Dining out", "amount": 5.5}])
        self.assertEqual(summary["balance"], 2994.50)

    def test_saving_a_real_credential_ends_demo_mode(self):
        seed_demo_bank_data(seed=1)
        self.client.patch(
            "/api/connectors/up_bank",
            {"credentials": {"personal_access_token": "up:yeah:real"}},
            content_type="application/json",
        )
        connector = Connector.objects.get(kind=Connector.Kind.UP_BANK)
        self.assertNotIn("demo", connector.config)
        self.assertEqual(connector.credentials["personal_access_token"], "up:yeah:real")

    def test_config_patches_merge_rather_than_replace(self):
        connector = Connector.objects.create(kind=Connector.Kind.POCKETAI, config={"base_url": "https://example.com"})
        self.client.patch(
            "/api/connectors/pocketai",
            {"config": {"other_setting": "x"}},
            content_type="application/json",
        )
        connector.refresh_from_db()
        self.assertEqual(connector.config["base_url"], "https://example.com")
        self.assertEqual(connector.config["other_setting"], "x")

    def test_build_demo_week_summary_shape(self):
        DemoTransaction.objects.create(
            account="Spending", description="Coffee", category="Dining out",
            amount_cents=-550, created_at=timezone.now(),
        )
        summary = build_demo_week_summary(timezone.now() - timezone.timedelta(days=1))
        self.assertEqual(summary, {"transaction_count": 1, "spent": 5.5})


class DemoGoogleDataTests(TestCase):
    def test_seed_enables_google_and_creates_events_and_emails(self):
        seed_demo_google_data(seed=1)
        connector = Connector.objects.get(kind=Connector.Kind.GOOGLE)
        self.assertTrue(connector.enabled)
        self.assertTrue(connector.config["demo"])
        self.assertGreater(DemoCalendarEvent.objects.count(), 0)
        self.assertGreater(DemoEmail.objects.count(), 0)

    def test_clear_removes_everything_and_disables_demo_mode(self):
        seed_demo_google_data(seed=1)
        result = clear_demo_google_data()
        self.assertGreater(result["events_removed"], 0)
        self.assertGreater(result["emails_removed"], 0)
        self.assertEqual(DemoCalendarEvent.objects.count(), 0)
        self.assertEqual(DemoEmail.objects.count(), 0)
        connector = Connector.objects.get(kind=Connector.Kind.GOOGLE)
        self.assertNotIn("demo", connector.config)
        self.assertFalse(connector.enabled)

    def test_snapshot_reports_demo_google_activity_without_a_real_token(self):
        seed_demo_google_data(seed=1)
        response = self.client.get("/api/connectors/snapshot")
        data = response.json()["google"]
        self.assertIsNotNone(data)
        self.assertNotIn("error", data)
        self.assertIn("calendar_events_today", data)
        self.assertIn("gmail_unread", data)


class DemoPocketDataTests(TestCase):
    def test_seed_enables_pocketai_and_creates_recordings(self):
        seed_demo_pocket_data(seed=1)
        connector = Connector.objects.get(kind=Connector.Kind.POCKETAI)
        self.assertTrue(connector.enabled)
        self.assertTrue(connector.config["demo"])
        self.assertGreater(DemoRecording.objects.count(), 0)

    def test_clear_removes_recordings_and_disables_demo_mode(self):
        seed_demo_pocket_data(seed=1)
        result = clear_demo_pocket_data()
        self.assertGreater(result["recordings_removed"], 0)
        self.assertEqual(DemoRecording.objects.count(), 0)
        connector = Connector.objects.get(kind=Connector.Kind.POCKETAI)
        self.assertNotIn("demo", connector.config)
        self.assertFalse(connector.enabled)

    def test_summary_endpoint_serves_demo_data_without_a_real_key(self):
        seed_demo_pocket_data(seed=1)
        response = self.client.get("/api/connectors/pocketai/summary?days=30")
        self.assertEqual(response.status_code, 200)
        data = response.json()
        self.assertGreater(data["recordings_count"], 0)
        self.assertGreater(data["total_duration_minutes"], 0)
        self.assertTrue(len(data["recent_recordings"]) > 0)

    def test_summary_endpoint_requires_connection(self):
        response = self.client.get("/api/connectors/pocketai/summary")
        self.assertEqual(response.status_code, 400)

    def test_recordings_never_claim_a_todos_field_that_does_not_exist(self):
        # Grounding check: PocketAI's API has no dedicated action-items/todos
        # field, so the summary must never fabricate one.
        seed_demo_pocket_data(seed=1)
        response = self.client.get("/api/connectors/pocketai/summary?days=30")
        data = response.json()
        self.assertNotIn("todos", data)
        self.assertNotIn("action_items", data)


class PocketAIClientTests(TestCase):
    def test_search_posts_query_body(self):
        from connectors.clients import PocketAIClient

        with patch("connectors.clients.httpx.post") as post:
            post.return_value.json.return_value = {"success": True, "data": {"userProfile": {}}}
            result = PocketAIClient({"api_key": "pk-x"}).search("meeting")

        self.assertEqual(result, {"success": True, "data": {"userProfile": {}}})
        url = post.call_args[0][0]
        self.assertEqual(url, "https://public.heypocketai.com/api/v1/public/search")
        self.assertEqual(post.call_args[1]["json"], {"query": "meeting"})
