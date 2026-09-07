from django.apps import AppConfig


class ConnectorsConfig(AppConfig):
    name = 'connectors'

    def ready(self):
        from . import tools

        tools.register()
