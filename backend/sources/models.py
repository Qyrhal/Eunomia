from django.db import models


class SyncStatus(models.Model):
    """Per-source sync health, surfaced in the admin frontend (#16)."""

    source_key = models.CharField(max_length=64, primary_key=True)
    cursor = models.CharField(max_length=500, blank=True, default="")
    last_run = models.DateTimeField(null=True, blank=True)
    last_ok = models.DateTimeField(null=True, blank=True)
    last_error = models.TextField(blank=True, default="")
    consecutive_failures = models.IntegerField(default=0)
    last_report = models.JSONField(default=dict, blank=True)

    def __str__(self):
        return self.source_key
