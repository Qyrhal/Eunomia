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
