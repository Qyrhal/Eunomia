"""End-to-end smoke (#45): the whole path in one test —

  source sync -> ingest (mask + PII + embed) -> cache -> MCP/REST tools

and the security gate: no raw secret or PII appears in any tool response.
"""

import hashlib
import hmac
import json
from unittest.mock import patch

from django.test import TestCase

from connectors.models import AppSettings, Connector
from masking.models import AuditEvent
from sources import registry
from tools.registry import call

PAT = "up:yeah:PA55W0RD-tok"
EMAIL = "landlord@example.com"
PHONE = "0412 345 678"

TXN = {
    "type": "transactions", "id": "smoke-1",
    "attributes": {
        "description": f"Rent transfer — {EMAIL} {PHONE}", "rawText": None, "message": None,
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
        s.hermes_webhook_url = "http://127.0.0.1:8644/webhooks"
        s.hermes_webhook_secret = "hermes-shared-secret"
        s.save()
        c = Connector.objects.create(kind="up_bank", enabled=True)
        c.credentials = {"personal_access_token": PAT, "webhook_secret_key": "wsk"}
        c.save()
        registry.discover()

    def _sync(self):
        with patch("connectors.clients.UpBankClient.transactions",
                   return_value={"data": [dict(TXN, attributes=dict(TXN["attributes"], message=f"ref {PAT}"))], "links": {}}), \
             patch("connectors.clients.UpBankClient.accounts", return_value={"data": [ACCT]}):
            return registry.run_sync("up_bank")

    def test_full_path_and_no_raw_values_leak(self):
        report, _ = self._sync()
        self.assertEqual(report.written, 2)

        blobs = []

        # 1. search
        res = call("search", {"query": "rent", "mode": "keyword"})
        blobs.append(json.dumps(res))
        self.assertTrue(res["results"])
        rid = res["results"][0]["id"]

        # 2. get
        rec = call("get", {"id": rid})
        blobs.append(json.dumps(rec))
        self.assertEqual(rec["id"], rid)

        # 3. finance_summary
        fin = call("up_bank__finance_summary", {"since": "2000-01-01T00:00:00Z"})
        blobs.append(json.dumps(fin))
        self.assertEqual(fin["balance"], 1000.0)
        self.assertTrue(any(cat["category"] == "home" for cat in fin["spend_by_category"]))

        # --- the security gate ---
        for blob in blobs:
            self.assertNotIn(PAT, blob)
            self.assertNotIn(EMAIL, blob)
            self.assertNotIn("345 678", blob)
        # but the tokens ARE there and resolve
        self.assertIn("[eunomia:", json.dumps(rec))

    def test_token_passed_back_into_a_tool_is_resolved_and_audited(self):
        self._sync()
        rec = call("get", {"id": "up_bank:up.transaction:smoke-1"})
        # pull a token out of the record's body and feed it back through a tool
        import re
        tok = re.search(r"\[eunomia:[a-z_]+:\d+\]", json.dumps(rec)).group(0)

        captured = {}
        from tools import registry as treg
        treg.register_tool("echo", {"type": "object"}, lambda **kw: captured.update(kw) or kw)
        call("echo", {"value": tok})
        self.assertNotIn("[eunomia:", json.dumps(captured))  # resolved to a real value
        self.assertTrue(AuditEvent.objects.filter(kind=AuditEvent.KIND_TOOL_INPUT, actor="tool:echo").exists())
