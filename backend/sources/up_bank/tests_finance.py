from datetime import datetime, timezone as dt_tz

from django.test import TestCase

from cache.models import CacheRecord
from tools.registry import all_tools

from .source import finance_summary


def _txn(i, cents, cat, day):
    return CacheRecord.objects.create(
        id=f"up_bank:up.transaction:{i}", source="up_bank", type="up.transaction",
        external_id=str(i), title=f"txn {i}", body_text="", content_hash=f"h{i}",
        occurred_at=datetime(2026, 2, day, tzinfo=dt_tz.utc),
        ingested_at=datetime(2026, 2, day, tzinfo=dt_tz.utc),
        updated_at=datetime(2026, 2, day, tzinfo=dt_tz.utc),
        payload={"amount_cents": cents, "amount": f"{cents/100:.2f}", "category": cat, "status": "SETTLED"},
    )


class FinanceSummaryTests(TestCase):
    def setUp(self):
        CacheRecord.objects.create(
            id="up_bank:up.account:a1", source="up_bank", type="up.account", external_id="a1",
            title="Spending", body_text="", content_hash="ha",
            occurred_at=datetime(2026, 1, 1, tzinfo=dt_tz.utc),
            ingested_at=datetime(2026, 1, 1, tzinfo=dt_tz.utc), updated_at=datetime(2026, 1, 1, tzinfo=dt_tz.utc),
            payload={"balance_cents": 50000, "balance": "500.00"},
        )
        _txn(1, -1500, "groceries", 3)
        _txn(2, -500, "groceries", 3)
        _txn(3, -2000, "transport", 4)
        _txn(4, 300000, "salary", 5)  # income — excluded from spend

    def test_balance_from_accounts(self):
        out = finance_summary(since="2026-01-01T00:00:00Z")
        self.assertEqual(out["balance"], 500.0)

    def test_spend_by_category_excludes_income(self):
        out = finance_summary(since="2026-01-01T00:00:00Z")
        cats = {c["category"]: c["amount"] for c in out["spend_by_category"]}
        self.assertEqual(cats, {"groceries": 20.0, "transport": 20.0})
        self.assertNotIn("salary", cats)

    def test_spend_by_day(self):
        out = finance_summary(since="2026-01-01T00:00:00Z")
        days = {d["day"]: d["amount"] for d in out["spend_by_day"]}
        self.assertEqual(days["2026-02-03"], 20.0)
        self.assertEqual(days["2026-02-04"], 20.0)

    def test_since_filter(self):
        out = finance_summary(since="2026-02-04T00:00:00Z")
        self.assertEqual({c["category"] for c in out["spend_by_category"]}, {"transport"})

    def test_registered_as_up_bank_tool(self):
        self.assertIn("up_bank__finance_summary", all_tools())
