from rest_framework import serializers

from .models import AppSettings, Connector


class AppSettingsSerializer(serializers.ModelSerializer):
    llm_api_key = serializers.CharField(write_only=True, required=False, allow_blank=True)
    llm_api_key_set = serializers.SerializerMethodField()

    class Meta:
        model = AppSettings
        fields = [
            "embedding_backend",
            "embedding_model",
            "llm_base_url",
            "llm_api_key",
            "llm_api_key_set",
            "sync_intervals",
            "theme",
        ]

    def get_llm_api_key_set(self, obj):
        return bool(obj.llm_api_key_encrypted)

    def update(self, instance, validated_data):
        api_key = validated_data.pop("llm_api_key", None)
        for attr, value in validated_data.items():
            setattr(instance, attr, value)
        if api_key:
            instance.llm_api_key = api_key
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
