from django.core.management import call_command
from django.test import TestCase
from django.utils import timezone

from cache.models import CacheRecord
from connectors.models import (
    AppSettings, Connector, DemoCalendarEvent, DemoEmail, DemoRecording, DemoTransaction,
)
from sources import registry

from .source import DemoSource


class DemoSourceTests(TestCase):
    def setUp(self):
        s = AppSettings.load(); s.embedding_backend = AppSettings.EMBED_STUB; s.save()
        now = timezone.now()
        DemoTransaction.objects.create(account="Spending", description="Coffee", category="cafe",
                                       amount_cents=-550, created_at=now)
        DemoTransaction.objects.create(account="Spending", description="Salary", category="Income",
                                       amount_cents=300000, created_at=now)
        DemoCalendarEvent.objects.create(summary="Standup", start_at=now, end_at=now, attendees=["a@x.com"])
        DemoEmail.objects.create(subject="Hi", sender="b@x.com", snippet="hello", received_at=now, unread=True)
        DemoRecording.objects.create(title="Sync", duration_seconds=600, tags=["work"], recorded_at=now)
        Connector.objects.create(kind="demo", enabled=True)
        registry.register(DemoSource())

    def test_sync_lands_all_types_in_the_cache(self):
        report, _ = registry.run_sync("demo")
        types = set(CacheRecord.objects.values_list("type", flat=True))
        self.assertTrue(
            {"up.transaction", "up.account", "gcal.event", "gmail.message", "heypocket.recording"} <= types
        )
        self.assertTrue(report.written >= 6)

    def test_demo_account_balance_is_derived(self):
        registry.run_sync("demo")
        acct = CacheRecord.objects.get(type="up.account")
        # -550 + 300000
        self.assertEqual(acct.payload["balance_cents"], 299450)

    def test_finance_summary_works_over_demo_data(self):
        registry.run_sync("demo")
        from sources.up_bank.source import finance_summary

        out = finance_summary(since="2000-01-01T00:00:00Z")
        self.assertEqual(out["balance"], 2994.5)
        self.assertTrue(any(c["category"] == "cafe" for c in out["spend_by_category"]))


class SeedDemoCommandTests(TestCase):
    def setUp(self):
        s = AppSettings.load(); s.embedding_backend = AppSettings.EMBED_STUB; s.save()

    def test_seed_then_clear(self):
        call_command("seed_demo", "--seed", "1")
        self.assertTrue(CacheRecord.objects.filter(source="demo").exists())
        self.assertTrue(Connector.objects.filter(kind="demo", enabled=True).exists())
        call_command("seed_demo", "--clear")
        self.assertFalse(CacheRecord.objects.filter(source="demo").exists())
        self.assertFalse(Connector.objects.filter(kind="demo").exists())
