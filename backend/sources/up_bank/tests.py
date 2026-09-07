import hashlib
import hmac
import json
from unittest.mock import patch

from django.test import TestCase

from cache.models import CacheRecord
from connectors.models import AppSettings, Connector
from sources import registry

from .source import UpBankSource

TXN = {
    "type": "transactions", "id": "txn-1",
    "attributes": {
        "description": "Ona Coffee", "rawText": "ONA COFFEE CANBERRA", "message": None,
        "status": "SETTLED", "createdAt": "2026-01-05T09:00:00+11:00", "settledAt": "2026-01-05T09:05:00+11:00",
        "amount": {"value": "-5.50", "valueInBaseUnits": -550, "currencyCode": "AUD"},
    },
    "relationships": {"category": {"data": {"id": "restaurants-and-cafes"}}},
}
ACCT = {
    "type": "accounts", "id": "acc-1",
    "attributes": {"displayName": "Spending", "accountType": "TRANSACTIONAL", "ownershipType": "INDIVIDUAL",
                   "createdAt": "2025-01-01T00:00:00+11:00",
                   "balance": {"value": "123.45", "valueInBaseUnits": 12345, "currencyCode": "AUD"}},
}


class MapTests(TestCase):
    def test_map_transaction(self):
        env = UpBankSource().map(TXN)
        self.assertEqual(env["id"], "up_bank:up.transaction:txn-1")
        self.assertEqual(env["type"], "up.transaction")
        self.assertEqual(env["title"], "Ona Coffee")
        self.assertIn("ONA COFFEE", env["body_text"])
        self.assertEqual(env["payload"]["amount_cents"], -550)
        self.assertEqual(env["payload"]["category"], "restaurants-and-cafes")
        self.assertFalse(env["payload"]["is_income"])

    def test_map_account(self):
        env = UpBankSource().map(ACCT)
        self.assertEqual(env["type"], "up.account")
        self.assertEqual(env["payload"]["balance_cents"], 12345)

    def test_map_unknown_type_skipped(self):
        self.assertIsNone(UpBankSource().map({"type": "pings", "id": "x"}))


class SyncTests(TestCase):
    def setUp(self):
        s = AppSettings.load(); s.embedding_backend = AppSettings.EMBED_STUB; s.save()
        c = Connector.objects.create(kind="up_bank", enabled=True)
        c.credentials = {"personal_access_token": "up:yeah:SECRETPAT"}
        c.save()
        registry.register(UpBankSource())

    def test_sync_pulls_and_ingests(self):
        with patch("connectors.clients.UpBankClient.transactions", return_value={"data": [TXN], "links": {}}), \
             patch("connectors.clients.UpBankClient.accounts", return_value={"data": [ACCT]}):
            report, cursor = registry.run_sync("up_bank")
        self.assertEqual(report.written, 2)
        self.assertEqual(cursor, "2026-01-05T09:00:00+11:00")
        self.assertTrue(CacheRecord.objects.filter(pk="up_bank:up.transaction:txn-1").exists())
        self.assertTrue(CacheRecord.objects.filter(pk="up_bank:up.account:acc-1").exists())


class WebhookTests(TestCase):
    def setUp(self):
        c = Connector.objects.create(kind="up_bank", enabled=True)
        c.credentials = {"personal_access_token": "p", "webhook_secret_key": "wsk"}
        c.save()
        self.src = UpBankSource()
        registry.register(self.src)

    def _req(self, body: bytes, sig: str):
        class R:
            def __init__(s): s.body = body; s.headers = {"X-Up-Authenticity-Signature": sig}
        return R()

    def test_valid_signature_refetches_transaction(self):
        body = json.dumps({"data": {"attributes": {"eventType": "TRANSACTION_CREATED"},
                                    "relationships": {"transaction": {"links": {"related": "https://api.up/txn-1"}}}}}).encode()
        sig = hmac.new(b"wsk", body, hashlib.sha256).hexdigest()
        with patch("httpx.get") as g:
            g.return_value.json.return_value = {"data": TXN}
            out = self.src.webhook(self._req(body, sig))
        self.assertEqual(out[0]["id"], "txn-1")

    def test_bad_signature_rejected(self):
        self.assertIsNone(self.src.webhook(self._req(b"{}", "nope")))

    def test_deleted_event_yields_deleted_envelope(self):
        body = json.dumps({"data": {"attributes": {"eventType": "TRANSACTION_DELETED"},
                                    "relationships": {"transaction": {"data": {"id": "txn-9"}}}}}).encode()
        sig = hmac.new(b"wsk", body, hashlib.sha256).hexdigest()
        out = self.src.webhook(self._req(body, sig))
        self.assertTrue(out[0]["_deleted"])
        self.assertTrue(self.src.map(out[0])["deleted"])
