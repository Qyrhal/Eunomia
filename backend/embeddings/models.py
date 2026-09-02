from django.db import models


class EmbedCache(models.Model):
    """text -> vector memo, keyed by HMAC(ENCRYPTION_KEY, backend:model:text).
    Skips re-embedding identical strings across records and re-syncs."""

    text_hmac = models.CharField(max_length=64, primary_key=True)
    vector = models.JSONField()
    created_at = models.DateTimeField(auto_now_add=True)

    def __str__(self):
        return self.text_hmac[:12]
