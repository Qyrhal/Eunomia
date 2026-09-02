from datetime import timedelta

from django.test import TestCase
from django.utils import timezone

from . import audit
from .boundary import resolve_outbound, resolve_tool_input
from .models import AuditEvent
from .vault import tokenize


class AuditTests(TestCase):
    def test_tool_input_resolution_is_logged_with_tokens_not_values(self):
        t = tokenize("boss@corp.com", "email")
        resolve_tool_input({"to": t}, actor="tool:send_email")
        e = AuditEvent.objects.get(kind=AuditEvent.KIND_TOOL_INPUT)
        self.assertEqual(e.actor, "tool:send_email")
        self.assertEqual(e.tokens, [t])
        self.assertNotIn("boss@corp.com", str(e.tokens) + e.detail)

    def test_outbound_resolution_logged(self):
        t = tokenize("sk-123", "up_pat")
        resolve_outbound({"headers": {"Authorization": f"Bearer {t}"}}, actor="source:up_bank")
        self.assertTrue(AuditEvent.objects.filter(kind=AuditEvent.KIND_OUTBOUND, actor="source:up_bank").exists())

    def test_no_tokens_no_noise_log(self):
        resolve_tool_input({"q": "just text"}, actor="tool:search")
        self.assertEqual(AuditEvent.objects.count(), 0)

    def test_reveal_returns_value_and_logs(self):
        t = tokenize("secret@x.com", "email")
        out = audit.reveal(t, actor="frontend")
        self.assertEqual(out["value"], "secret@x.com")
        e = AuditEvent.objects.get(kind=AuditEvent.KIND_REVEAL)
        self.assertEqual(e.tokens, [t])

    def test_reveal_unknown_token(self):
        out = audit.reveal("[eunomia:email:999]")
        self.assertEqual(out, {"error": "unknown token"})
        self.assertEqual(AuditEvent.objects.filter(kind=AuditEvent.KIND_REVEAL).count(), 1)

    def test_purge_old(self):
        audit.record(AuditEvent.KIND_REVEAL, "x", ["[eunomia:email:1]"])
        AuditEvent.objects.update(created_at=timezone.now() - timedelta(days=200))
        audit.record(AuditEvent.KIND_REVEAL, "x", ["[eunomia:email:2]"])
        self.assertEqual(audit.purge_old(), 1)
        self.assertEqual(AuditEvent.objects.count(), 1)


class VaultEndpointTests(TestCase):
    def test_reveal_endpoint(self):
        t = tokenize("a@b.com", "email")
        r = self.client.post("/api/vault/reveal", {"token": t}, content_type="application/json")
        self.assertEqual(r.status_code, 200)
        self.assertEqual(r.json()["value"], "a@b.com")

    def test_reveal_endpoint_needs_token(self):
        self.assertEqual(self.client.post("/api/vault/reveal", {}, content_type="application/json").status_code, 400)

    def test_secrets_list_endpoint_hides_values(self):
        tokenize("a@b.com", "email")
        r = self.client.get("/api/vault/secrets")
        self.assertEqual(r.status_code, 200)
        row = r.json()[0]
        self.assertIn("token", row)
        self.assertNotIn("value_encrypted", row)
        self.assertNotIn("value", row)

    def test_audit_endpoint(self):
        t = tokenize("a@b.com", "email")
        self.client.post("/api/vault/reveal", {"token": t}, content_type="application/json")
        r = self.client.get("/api/vault/audit")
        self.assertEqual(r.status_code, 200)
        self.assertEqual(r.json()[0]["kind"], "reveal")
