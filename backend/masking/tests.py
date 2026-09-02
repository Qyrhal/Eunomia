import subprocess
from pathlib import Path

from django.test import TestCase

from .boundary import resolve_outbound, resolve_tool_input
from .models import VaultSecret
from .vault import detokenize, rotate, tokenize, tokenize_text


class VaultTests(TestCase):
    def test_tokenize_is_stable_and_reversible(self):
        t1 = tokenize("sk-live-abc123", "up_pat", kind=VaultSecret.KIND_CREDENTIAL, source="up_bank")
        t2 = tokenize("sk-live-abc123", "up_pat", kind=VaultSecret.KIND_CREDENTIAL, source="up_bank")
        self.assertEqual(t1, t2)
        self.assertEqual(t1, "[eunomia:up_pat:1]")
        self.assertEqual(detokenize(t1), "sk-live-abc123")

    def test_distinct_values_get_distinct_incrementing_tokens(self):
        a = tokenize("a@x.com", "email")
        b = tokenize("b@x.com", "email")
        self.assertEqual({a, b}, {"[eunomia:email:1]", "[eunomia:email:2]"})

    def test_email_normalized_before_lookup(self):
        a = tokenize("Sam@Example.com ", "email")
        b = tokenize("sam@example.com", "email")
        self.assertEqual(a, b)

    def test_phone_punctuation_stripped_before_lookup(self):
        self.assertEqual(tokenize("0400 111 222", "phone"), tokenize("0400-111-222", "phone"))

    def test_same_string_different_type_is_two_tokens(self):
        a = tokenize("12345678", "phone")
        b = tokenize("12345678", "bank_acct")
        self.assertNotEqual(a, b)

    def test_detokenize_unknown_returns_none(self):
        self.assertIsNone(detokenize("[eunomia:email:999]"))

    def test_rotate_repoints_token(self):
        t = tokenize("old-token", "google_oauth_token", kind=VaultSecret.KIND_CREDENTIAL)
        rotate(t, "new-token")
        self.assertEqual(detokenize(t), "new-token")
        # rotated value now resolves to the same token
        self.assertEqual(tokenize("new-token", "google_oauth_token", kind=VaultSecret.KIND_CREDENTIAL), t)

    def test_value_hmac_is_not_the_plain_hash(self):
        tokenize("a@x.com", "email")
        row = VaultSecret.objects.get()
        import hashlib

        self.assertNotEqual(row.value_hmac, hashlib.sha256(b"email:a@x.com").hexdigest())

    def test_tokenize_text_noop_without_detector(self):
        # pii.py (#28) not shipped yet -> passthrough
        self.assertEqual(tokenize_text("email me at a@x.com"), "email me at a@x.com")


class BoundaryTests(TestCase):
    def test_resolve_tool_input_deep(self):
        t = tokenize("a@x.com", "email")
        out = resolve_tool_input({"to": t, "cc": [t], "meta": {"from": t}})
        self.assertEqual(out, {"to": "a@x.com", "cc": ["a@x.com"], "meta": {"from": "a@x.com"}})

    def test_resolve_outbound_leaves_unknown_tokens(self):
        self.assertEqual(resolve_outbound("hi [eunomia:email:404]"), "hi [eunomia:email:404]")

    def test_detokenize_is_confined_to_boundary_and_vault(self):
        root = Path(__file__).resolve().parent.parent
        hits = subprocess.run(
            ["grep", "-rl", "--include=*.py", "detokenize", str(root)],
            capture_output=True, text=True,
        ).stdout.split()
        allowed = {"masking/boundary.py", "masking/vault.py", "masking/tests.py"}
        offenders = {h[len(str(root)) + 1:] for h in hits} - allowed
        self.assertEqual(offenders, set(), f"detokenize used outside the boundary: {offenders}")
