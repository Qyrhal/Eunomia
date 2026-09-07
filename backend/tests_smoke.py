"""End-to-end smoke (#45): the whole path in one test —

  source sync -> ingest (embed) -> cache -> MCP/REST tools
"""

import json
from unittest.mock import patch

from django.test import TestCase

from connectors.models import AppSettings, Connector
from sources import registry
from tools.registry import call

PAT = "up:yeah:PA55W0RD-tok"
EMAIL = "landlord@example.com"

TXN = {
    "type": "transactions", "id": "smoke-1",
    "attributes": {
        "description": f"Rent transfer — {EMAIL}", "rawText": None, "message": None,
        "status": "SETTLED", "createdAt": "2026-02-01T00:00:00+11:00", "settledAt": None,
        "amount": {"value": "-2500.00", "valueInBaseUnits": -250000, "currencyCode": "AUD"},
    },
    "relationships": {"category": {"data": {"id": "home"}}},
}
ACCT = {
    "type": "accounts", "id": "smoke-acc",
    "attributes": {"displayName": "Spending", "accountType": "TRANSACTIONAL", "ownershipType": "INDIVIDUAL",
                   "createdAt": "2025-01-01T00:00:00+11:00",
                   "balance": {"value": "1000.00", "valueInBaseUnits": 100000, "currencyCode": "AUD"}},
}


class EndToEndSmokeTests(TestCase):
    def setUp(self):
        s = AppSettings.load()
        s.embedding_backend = AppSettings.EMBED_STUB
        s.save()
        c = Connector.objects.create(kind="up_bank", enabled=True)
        c.credentials = {"personal_access_token": PAT}
        c.save()
        registry.discover()

    def _sync(self):
        with patch("connectors.clients.UpBankClient.transactions",
                   return_value={"data": [TXN], "links": {}}), \
             patch("connectors.clients.UpBankClient.accounts", return_value={"data": [ACCT]}), \
             patch("connectors.clients.UpBankClient.categories", return_value={"data": []}):
            return registry.run_sync("up_bank")

    def test_full_path(self):
        report, _ = self._sync()
        self.assertEqual(report.written, 2)

        res = call("search", {"query": "rent", "mode": "keyword"})
        self.assertTrue(res["results"])
        rid = res["results"][0]["id"]

        rec = call("get", {"id": rid})
        self.assertEqual(rec["id"], rid)
        self.assertIn(EMAIL, rec["body_text"])

        fin = call("up_bank__finance_summary", {"since": "2000-01-01T00:00:00Z"})
        self.assertEqual(fin["balance"], 1000.0)
        self.assertTrue(any(cat["category"] == "home" for cat in fin["spend_by_category"]))

        txns = call("up_bank__list_transactions", {"days": 365})
        self.assertEqual(len(txns), 1)

        accounts = call("up_bank__list_accounts", {})
        self.assertEqual(accounts[0]["name"], "Spending")

    def test_open_connector_tools_are_optional_and_never_raise(self):
        # No open_connector Connector row configured at all — the tools must
        # come back with an error dict, not throw, so a chat/MCP call never
        # 500s just because this optional broker isn't set up.
        self.assertEqual(
            call("open_connector_call", {"action": "github.get_current_user"}),
            {"error": "open_connector is not connected"},
        )
        self.assertEqual(
            call("open_connector_list_connections", {}),
            {"error": "open_connector is not connected"},
        )
