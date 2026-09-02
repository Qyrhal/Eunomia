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

    def test_tokenize_text_masks_email_and_is_reversible(self):
        out = tokenize_text("ping me at Sam@Example.com about it", source="gmail")
        self.assertNotIn("Sam@Example.com", out)
        self.assertRegex(out, r"\[eunomia:email:\d+\]")
        tok = out.split("ping me at ")[1].split(" about")[0]
        self.assertEqual(detokenize(tok), "Sam@Example.com")

    def test_tokenize_text_masks_multiple_spans(self):
        out = tokenize_text("card 4242 4242 4242 4242 email a@b.com", source="x")
        self.assertNotIn("4242 4242", out)
        self.assertNotIn("a@b.com", out)
        self.assertIn("[eunomia:card:", out)
        self.assertIn("[eunomia:email:", out)


class PIIScanTests(TestCase):
    def test_email(self):
        from .pii import scan

        spans = scan("reach a.b+c@x.co.uk now")
        self.assertEqual([(s[2]) for s in spans], ["email"])

    def test_luhn_card_only(self):
        from .pii import scan

        self.assertTrue(any(s[2] == "card" for s in scan("pay 4242424242424242")))
        self.assertFalse(any(s[2] == "card" for s in scan("ref 1234567812345678")))  # fails Luhn

    def test_au_phone(self):
        from .pii import scan

        self.assertTrue(any(s[2] == "phone" for s in scan("call 0412 345 678 today")))

    def test_bank_bsb_account(self):
        from .pii import scan

        self.assertTrue(any(s[2] == "bank_acct" for s in scan("acct 123-456 12345678")))

    def test_key_shaped_needs_entropy_and_mixed_classes(self):
        from .pii import scan

        self.assertTrue(any(s[2] == "key_shaped" for s in scan("token sk_live_9f8Q2xKp1mZ7Rw3aB6cD0eF")))
        self.assertFalse(any(s[2] == "key_shaped" for s in scan("aaaaaaaaaaaaaaaaaaaaaaaaaaaa")))
        self.assertFalse(any(s[2] == "key_shaped" for s in scan("thisisjustaverylongplainenglishword")))

    def test_overlaps_resolved_by_confidence(self):
        from .pii import scan

        # a Luhn card is also 16 digits; card (0.95) must beat any tokenish match
        spans = scan("4242424242424242")
        self.assertEqual([s[2] for s in spans], ["card"])


class BoundaryTests(TestCase):
    def test_resolve_tool_input_deep(self):
        t = tokenize("a@x.com", "email")
        out = resolve_tool_input({"to": t, "cc": [t], "meta": {"from": t}})
        self.assertEqual(out, {"to": "a@x.com", "cc": ["a@x.com"], "meta": {"from": "a@x.com"}})

    def test_resolve_outbound_leaves_unknown_tokens(self):
        self.assertEqual(resolve_outbound("hi [eunomia:email:404]"), "hi [eunomia:email:404]")

    def test_detokenize_is_confined_to_boundary_and_vault(self):
        """Production code may call detokenize only from the boundary or the vault.
        Test files (test*.py) are exempt — they legitimately assert on real values."""
        root = Path(__file__).resolve().parent.parent
        hits = subprocess.run(
            ["grep", "-rl", "--include=*.py", "detokenize", str(root)],
            capture_output=True, text=True,
        ).stdout.split()
        # boundary = the two resolve_* paths; audit.reveal = the one human-reveal path (#22)
        allowed = {"masking/boundary.py", "masking/vault.py", "masking/audit.py"}
        offenders = {
            rel
            for h in hits
            if (rel := h[len(str(root)) + 1:]) not in allowed
            and not Path(rel).name.startswith("test")
            and "/migrations/" not in rel
        }
        self.assertEqual(offenders, set(), f"detokenize used outside the boundary: {offenders}")
