from django.db import models


class CacheRecord(models.Model):
    """One normalised item pulled from a source (#21). `body_text` and `payload`
    are already tokenised (#27) — no raw secrets/PII live here."""

    id = models.CharField(primary_key=True, max_length=400)  # f"{source}:{type}:{external_id}"
    source = models.CharField(max_length=64)
    type = models.CharField(max_length=64)              # "<family>.<noun>"
    external_id = models.CharField(max_length=255)
    title = models.CharField(max_length=500, blank=True, default="")
    body_text = models.TextField(blank=True, default="")
    occurred_at = models.DateTimeField(null=True, blank=True)
    url = models.CharField(max_length=800, blank=True, default="")
    payload = models.JSONField(default=dict, blank=True)
    content_hash = models.CharField(max_length=64, db_index=True)
    ingested_at = models.DateTimeField()
    updated_at = models.DateTimeField()
    deleted = models.BooleanField(default=False)
    has_embedding = models.BooleanField(default=False)

    class Meta:
        indexes = [
            models.Index(fields=["source", "type"]),
            models.Index(fields=["occurred_at"]),
            models.Index(fields=["deleted"]),
        ]

    def __str__(self):
        return self.id


class CacheLink(models.Model):
    """Typed edge between two cache records."""

    ORIGIN_SYNC, ORIGIN_AGENT = "sync", "agent"

    source_id = models.CharField(max_length=400)
    rel = models.CharField(max_length=40)
    target_id = models.CharField(max_length=400)
    origin = models.CharField(max_length=8, default=ORIGIN_SYNC)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        constraints = [
            models.UniqueConstraint(fields=["source_id", "rel", "target_id"], name="uniq_cache_link"),
        ]
        indexes = [
            models.Index(fields=["source_id"]),
            models.Index(fields=["target_id"]),
        ]

    def __str__(self):
        return f"{self.source_id} -{self.rel}-> {self.target_id}"
