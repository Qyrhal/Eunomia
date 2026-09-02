from django.apps import AppConfig
from django.db.models.signals import post_migrate


def _seed_builtins(sender, **kwargs):
    if sender.name != "triggers":
        return
    from .tools import ensure_builtins

    ensure_builtins()


class TriggersConfig(AppConfig):
    default_auto_field = "django.db.models.BigAutoField"
    name = "triggers"

    def ready(self):
        from . import tools

        tools.register()
        post_migrate.connect(_seed_builtins, dispatch_uid="triggers.seed_builtins")
