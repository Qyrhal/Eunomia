from django.db import models


class VaultSecret(models.Model):
    """One masked value. The agent sees `token`; the real value lives here,
    Fernet-encrypted. `value_hmac` (keyed with ENCRYPTION_KEY) is the reverse
    index for value -> token, and is useless to anyone without the key."""

    KIND_CREDENTIAL = "credential"
    KIND_PII = "pii"
    KIND_CHOICES = [(KIND_CREDENTIAL, "credential"), (KIND_PII, "pii")]

    token = models.CharField(max_length=64, primary_key=True)
    value_encrypted = models.TextField()
    value_hmac = models.CharField(max_length=64, unique=True)
    kind = models.CharField(max_length=16, choices=KIND_CHOICES)
    type = models.CharField(max_length=40)
    source = models.CharField(max_length=64, blank=True, default="")
    first_seen = models.DateTimeField(auto_now_add=True)
    last_used = models.DateTimeField(auto_now_add=True)

    class Meta:
        indexes = [models.Index(fields=["type"])]

    def __str__(self):
        return self.token


class AuditEvent(models.Model):
    """Append-only: every time a token became a real value, or a value was
    revealed to a human. Records the token strings and the actor — never the
    real values."""

    KIND_REVEAL = "reveal"        # a human clicked "reveal" in the vault inspector
    KIND_TOOL_INPUT = "tool_input"  # tokens resolved on the way into a tool
    KIND_OUTBOUND = "outbound"    # tokens re-hydrated for an external API call
    KIND_CHOICES = [(KIND_REVEAL, "reveal"), (KIND_TOOL_INPUT, "tool input"), (KIND_OUTBOUND, "outbound")]

    kind = models.CharField(max_length=16, choices=KIND_CHOICES)
    actor = models.CharField(max_length=120, blank=True, default="")
    tokens = models.JSONField(default=list)
    detail = models.CharField(max_length=300, blank=True, default="")
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        indexes = [models.Index(fields=["created_at"]), models.Index(fields=["kind"])]

    def __str__(self):
        return f"{self.kind} {self.actor} {self.tokens}"
