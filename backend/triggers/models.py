from django.db import models


class Trigger(models.Model):
    KIND_RECORD, KIND_SCHEDULE, KIND_CRON = "record_rule", "schedule", "cron"
    KIND_CHOICES = [(KIND_RECORD, "record rule"), (KIND_SCHEDULE, "schedule"), (KIND_CRON, "cron")]

    key = models.CharField(max_length=80, primary_key=True)
    kind = models.CharField(max_length=16, choices=KIND_CHOICES)
    enabled = models.BooleanField(default=True)
    spec = models.JSONField(default=dict)
    webhook_route = models.CharField(max_length=200, blank=True, default="")
    dedupe_window_s = models.IntegerField(default=3600)
    last_fired_at = models.DateTimeField(null=True, blank=True)
    fire_count = models.IntegerField(default=0)
    created_at = models.DateTimeField(auto_now_add=True)

    def __str__(self):
        return f"{self.key} ({self.kind})"


class DeliveryLog(models.Model):
    trigger_key = models.CharField(max_length=80)
    entity_id = models.CharField(max_length=400, blank=True, default="")
    attempt = models.IntegerField(default=1)
    http_status = models.IntegerField(null=True, blank=True)
    ok = models.BooleanField(default=False)
    dead = models.BooleanField(default=False)
    detail = models.CharField(max_length=300, blank=True, default="")
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        indexes = [models.Index(fields=["trigger_key", "entity_id", "created_at"])]
