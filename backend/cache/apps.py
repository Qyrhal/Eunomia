from django.apps import AppConfig
from django.db.backends.signals import connection_created


def _load_sqlite_vec(sender, connection, **kwargs):
    if connection.vendor != "sqlite":
        return
    raw = connection.connection
    try:
        raw.enable_load_extension(True)
        import sqlite_vec

        sqlite_vec.load(raw)
    finally:
        try:
            raw.enable_load_extension(False)
        except Exception:
            pass


class CacheConfig(AppConfig):
    default_auto_field = "django.db.models.BigAutoField"
    name = "cache"

    def ready(self):
        connection_created.connect(_load_sqlite_vec, dispatch_uid="cache.load_sqlite_vec")
