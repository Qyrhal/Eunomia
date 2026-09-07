import hashlib
import hmac
import json

from django.test import TestCase

from cache.models import CacheRecord
from connectors.models import AppSettings, Connector
from sources import registry
from sources.up_bank.source import UpBankSource

TXN = {
    "type": "transactions", "id": "wh-1",
    "attributes": {"description": "Rent", "rawText": None, "message": None, "status": "SETTLED",
                   "createdAt": "2026-02-01T00:00:00+11:00", "settledAt": None,
                   "amount": {"value": "-1200", "valueInBaseUnits": -120000, "currencyCode": "AUD"}},
    "relationships": {},
}


class UpBankWebhookEndpointTests(TestCase):
    def setUp(self):
        s = AppSettings.load(); s.embedding_backend = AppSettings.EMBED_STUB; s.save()
        c = Connector.objects.create(kind="up_bank", enabled=True)
        c.credentials = {"personal_access_token": "p", "webhook_secret_key": "wsk"}
        c.save()
        registry.register(UpBankSource())

    def test_valid_webhook_ingests_without_bearer(self):
        body = json.dumps({"data": {"attributes": {"eventType": "TRANSACTION_CREATED"},
                                    "relationships": {"transaction": {"links": {"related": "https://api.up/wh-1"}}}}})
        sig = hmac.new(b"wsk", body.encode(), hashlib.sha256).hexdigest()
        from unittest.mock import patch
        with patch("httpx.get") as g:
            g.return_value.json.return_value = {"data": TXN}
            r = self.client.post("/api/sources/up_bank/webhook", body,
                                 content_type="application/json", HTTP_X_UP_AUTHENTICITY_SIGNATURE=sig)
        self.assertEqual(r.status_code, 200)
        self.assertEqual(r.json()["written"], 1)
        self.assertTrue(CacheRecord.objects.filter(pk="up_bank:up.transaction:wh-1").exists())

    def test_bad_signature_400(self):
        r = self.client.post("/api/sources/up_bank/webhook", "{}",
                             content_type="application/json", HTTP_X_UP_AUTHENTICITY_SIGNATURE="bad")
        self.assertEqual(r.status_code, 400)

    def test_unknown_source_404(self):
        self.assertEqual(self.client.post("/api/sources/nope/webhook", "{}", content_type="application/json").status_code, 404)
