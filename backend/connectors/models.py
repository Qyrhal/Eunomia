import json

from django.db import models

from .crypto import decrypt, encrypt


class AppSettings(models.Model):
    """Singleton row (pk=1) holding everything the Settings page edits."""

    system_prompt = models.TextField(
        default="You are Eunomia, a personal AI assistant that helps manage tasks, "
        "reminders, calendar events and general organisation. Be concise and proactive. "
        "You can read upcoming calendar meetings and recent Up Bank transactions, and turn "
        "them into tasks when asked (e.g. meeting-prep tasks, bill follow-ups)."
    )

    llm_base_url = models.CharField(
        max_length=300, blank=True, default="https://api.openai.com/v1"
    )
    llm_model = models.CharField(max_length=200, blank=True, default="gpt-4o-mini")
    llm_api_key_encrypted = models.TextField(blank=True, default="")

    theme = models.JSONField(
        default=dict,
        blank=True,
        help_text="Arbitrary theme config consumed by the frontend, e.g. "
        '{"mode": "dark", "accent": "#0A84FF"}',
    )

    def save(self, *args, **kwargs):
        self.pk = 1
        super().save(*args, **kwargs)

    @classmethod
    def load(cls) -> "AppSettings":
        obj, _ = cls.objects.get_or_create(pk=1)
        return obj

    @property
    def llm_api_key(self) -> str:
        return decrypt(self.llm_api_key_encrypted)

    @llm_api_key.setter
    def llm_api_key(self, value: str):
        self.llm_api_key_encrypted = encrypt(value)


class Connector(models.Model):
    class Kind(models.TextChoices):
        GOOGLE = "google", "Google (Calendar + Gmail)"
        UP_BANK = "up_bank", "Up Bank"
        POCKETAI = "pocketai", "PocketAI"

    kind = models.CharField(max_length=20, choices=Kind.choices, unique=True)
    enabled = models.BooleanField(default=False)
    # non-secret config, e.g. {"calendar_ids": [...]}
    config = models.JSONField(default=dict, blank=True)
    # secret material (tokens, API keys), stored as an encrypted JSON blob
    credentials_encrypted = models.TextField(blank=True, default="")
    updated_at = models.DateTimeField(auto_now=True)

    def __str__(self):
        return self.get_kind_display()

    @property
    def credentials(self) -> dict:
        raw = decrypt(self.credentials_encrypted)
        return json.loads(raw) if raw else {}

    @credentials.setter
    def credentials(self, value: dict):
        self.credentials_encrypted = encrypt(json.dumps(value))


class DemoTransaction(models.Model):
    """Fake Up Bank transactions for demo mode — populated by `seed_demo_data`,
    read by the finance endpoints instead of the real Up Bank API whenever the
    Up Bank connector's config has `"demo": true`."""

    account = models.CharField(max_length=100)
    description = models.CharField(max_length=200)
    category = models.CharField(max_length=100)
    amount_cents = models.IntegerField(help_text="Negative = spend, positive = income")
    created_at = models.DateTimeField()

    class Meta:
        ordering = ["-created_at"]

    def __str__(self):
        return f"{self.description} ({self.amount_cents / 100:.2f})"


class DemoCalendarEvent(models.Model):
    """Fake Google Calendar events for demo mode."""

    summary = models.CharField(max_length=200)
    start_at = models.DateTimeField()
    end_at = models.DateTimeField()
    attendees = models.JSONField(default=list)

    class Meta:
        ordering = ["start_at"]

    def __str__(self):
        return self.summary


class DemoEmail(models.Model):
    """Fake Gmail messages for demo mode."""

    subject = models.CharField(max_length=200)
    sender = models.CharField(max_length=200)
    snippet = models.CharField(max_length=300)
    received_at = models.DateTimeField()
    unread = models.BooleanField(default=False)

    class Meta:
        ordering = ["-received_at"]

    def __str__(self):
        return self.subject


class DemoRecording(models.Model):
    """Fake PocketAI recordings for demo mode."""

    title = models.CharField(max_length=200)
    duration_seconds = models.IntegerField()
    tags = models.JSONField(default=list)
    recorded_at = models.DateTimeField()

    class Meta:
        ordering = ["-recorded_at"]

    def __str__(self):
        return self.title
