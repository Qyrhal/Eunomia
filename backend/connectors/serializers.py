from rest_framework import serializers

from .models import AppSettings, Connector


class AppSettingsSerializer(serializers.ModelSerializer):
    hermes_webhook_secret = serializers.CharField(write_only=True, required=False, allow_blank=True)
    hermes_webhook_secret_set = serializers.SerializerMethodField()

    class Meta:
        model = AppSettings
        fields = [
            "embedding_backend",
            "embedding_model",
            "llm_base_url",
            "user_timezone",
            "hermes_webhook_url",
            "hermes_webhook_secret",
            "hermes_webhook_secret_set",
            "pii_allowlist",
            "pii_disabled_sources",
            "pii_min_confidence",
            "vip_senders",
            "sync_intervals",
            "theme",
        ]

    def get_hermes_webhook_secret_set(self, obj):
        return bool(obj.hermes_webhook_secret_encrypted)

    def update(self, instance, validated_data):
        webhook_secret = validated_data.pop("hermes_webhook_secret", None)
        for attr, value in validated_data.items():
            setattr(instance, attr, value)
        if webhook_secret:
            instance.hermes_webhook_secret = webhook_secret
        instance.save()
        return instance


class ConnectorSerializer(serializers.ModelSerializer):
    credentials = serializers.DictField(write_only=True, required=False)
    credentials_set = serializers.SerializerMethodField()

    class Meta:
        model = Connector
        fields = ["kind", "enabled", "config", "credentials", "credentials_set", "updated_at"]
        read_only_fields = ["kind", "updated_at"]

    def get_credentials_set(self, obj):
        return bool(obj.credentials_encrypted)

    def update(self, instance, validated_data):
        credentials = validated_data.pop("credentials", None)
        config = validated_data.pop("config", None)
        for attr, value in validated_data.items():
            setattr(instance, attr, value)
        if credentials is not None:
            # merge, don't replace — saving one field (e.g. just a refreshed
            # secret) must not silently drop the others already on file.
            instance.credentials = {**instance.credentials, **credentials}
            # a real credential coming in always ends demo mode.
            if instance.config.get("demo"):
                instance.config = {k: v for k, v in instance.config.items() if k != "demo"}
        if config is not None:
            instance.config = {**instance.config, **config}
        instance.save()
        return instance
