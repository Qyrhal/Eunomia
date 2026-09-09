import json

from django.db import models

from .crypto import decrypt, encrypt


class AppSettings(models.Model):
    """Singleton row (pk=1) holding deployment-wide configuration."""

    # --- external embedding API (the "api" embedding backend / #29) ---
    EMBED_API, EMBED_LOCAL, EMBED_STUB = "api", "local", "stub"
    embedding_backend = models.CharField(
        max_length=10,
        default=EMBED_STUB,
        choices=[(EMBED_API, "OpenAI-compatible API"), (EMBED_LOCAL, "Local (sentence-transformers)"), (EMBED_STUB, "Stub")],
    )
    embedding_model = models.CharField(max_length=200, blank=True, default="")
    llm_base_url = models.CharField(
        max_length=300, blank=True, default="",
        help_text="OpenAI-compatible base URL for the 'api' embedding backend (also fits Ollama's /v1).",
    )
    llm_api_key_encrypted = models.TextField(blank=True, default="")

    # --- agent/operator timezone (naive agent dates are interpreted here) ---
    user_timezone = models.CharField(
        max_length=64,
        blank=True,
        default="",
        help_text="IANA timezone (e.g. Australia/Melbourne) used to interpret "
        "naive agent-supplied dates and render datetimes back to the agent. "
        "Set once at deployment by the installing agent; empty = UTC.",
    )

    # --- Hermes notification channel (#9 / #39) ---
    hermes_webhook_url = models.CharField(
        max_length=400, blank=True, default="",
        help_text="Base URL of the Hermes gateway webhook adapter, e.g. http://127.0.0.1:8644/webhooks",
    )
    hermes_webhook_secret_encrypted = models.TextField(blank=True, default="")

    # --- PII detector knobs (#28) ---
    pii_allowlist = models.JSONField(default=list, blank=True)
    pii_disabled_sources = models.JSONField(default=list, blank=True)
    pii_min_confidence = models.FloatField(default=0.5)

    # --- misc ---
    vip_senders = models.JSONField(default=list, blank=True, help_text="Sender addresses for the 'VIP email' trigger.")
    sync_intervals = models.JSONField(default=dict, blank=True, help_text='{"up_bank": 900, ...} seconds per source.')

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

    @property
    def hermes_webhook_secret(self) -> str:
        return decrypt(self.hermes_webhook_secret_encrypted)

    @hermes_webhook_secret.setter
    def hermes_webhook_secret(self, value: str):
        self.hermes_webhook_secret_encrypted = encrypt(value)

    def sync_interval(self, source_key: str, default: int = 900) -> int:
        return int((self.sync_intervals or {}).get(source_key, default))


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
