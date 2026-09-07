from django.utils import timezone
from rest_framework.response import Response
from rest_framework.views import APIView

from .clients import OpenConnectorClient, PocketAIClient, UpBankClient
from .demo_seed import (
    build_demo_finance_summary,
    build_demo_pocket_summary,
    build_demo_week_summary,
)
from .models import AppSettings, Connector
from .serializers import AppSettingsSerializer, ConnectorSerializer


class SettingsView(APIView):
    def get(self, request):
        return Response(AppSettingsSerializer(AppSettings.load()).data)

    def patch(self, request):
        instance = AppSettings.load()
        serializer = AppSettingsSerializer(instance, data=request.data, partial=True)
        serializer.is_valid(raise_exception=True)
        serializer.save()
        return Response(serializer.data)


class ConnectorListView(APIView):
    def get(self, request):
        connectors = {c.kind: c for c in Connector.objects.all()}
        for kind, _ in Connector.Kind.choices:
            if kind not in connectors:
                connectors[kind] = Connector.objects.create(kind=kind)
        ordered = [connectors[k] for k, _ in Connector.Kind.choices]
        return Response(ConnectorSerializer(ordered, many=True).data)


class ConnectorDetailView(APIView):
    def get(self, request, kind):
        connector, _ = Connector.objects.get_or_create(kind=kind)
        return Response(ConnectorSerializer(connector).data)

    def patch(self, request, kind):
        connector, _ = Connector.objects.get_or_create(kind=kind)
        serializer = ConnectorSerializer(connector, data=request.data, partial=True)
        serializer.is_valid(raise_exception=True)
        serializer.save()
        return Response(serializer.data)


class ConnectorTestView(APIView):
    def post(self, request, kind):
        connector, _ = Connector.objects.get_or_create(kind=kind)
        try:
            if kind == Connector.Kind.UP_BANK:
                ok = connector.config.get("demo") or UpBankClient(connector.credentials).ping()
            elif kind == Connector.Kind.POCKETAI:
                ok = connector.config.get("demo") or PocketAIClient(
                    connector.credentials, connector.config.get("base_url")
                ).ping()
            elif kind == Connector.Kind.OPEN_CONNECTOR:
                ok = OpenConnectorClient(connector.credentials, connector.config.get("base_url")).ping()
            else:
                return Response({"detail": "unknown connector"}, status=400)
        except Exception as exc:  # surfaced to the settings UI, not swallowed
            return Response({"ok": False, "error": str(exc)}, status=200)
        return Response({"ok": ok})


class UpBankTransactionsView(APIView):
    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
        if not connector:
            return Response({"detail": "Up Bank is not connected"}, status=400)
        data = UpBankClient(connector.credentials).transactions()
        return Response(data)


class UpBankFinanceSummaryView(APIView):
    """Backs the /finance page: balance, spend by category, recent transactions.

    Always reads from the cache (populated by the periodic sync) rather than
    calling Up Bank live on every page load."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
        if not connector:
            return Response({"detail": "Up Bank is not connected"}, status=400)
        import datetime

        days = int(request.query_params.get("days", 30))
        since = timezone.now() - datetime.timedelta(days=days)
        if connector.config.get("demo"):
            return Response(build_demo_finance_summary(since))
        from sources.up_bank.source import finance_summary

        return Response(finance_summary(since.isoformat()))


class SnapshotView(APIView):
    """Best-effort figures for each connected account, read from the cache
    (populated by the periodic sync) rather than the live API. Any connector
    that isn't connected comes back as null rather than failing the whole call."""

    def get(self, request):
        import datetime

        result = {"up_bank": None, "pocketai": None}

        up_bank = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
        if up_bank and (up_bank.config.get("demo") or up_bank.credentials.get("personal_access_token")):
            since = (
                timezone.now() - datetime.timedelta(days=timezone.now().weekday())
            ).replace(hour=0, minute=0, second=0, microsecond=0)
            if up_bank.config.get("demo"):
                result["up_bank"] = build_demo_week_summary(since)
            else:
                from sources.up_bank.source import week_summary

                result["up_bank"] = week_summary(since.isoformat())

        pocketai = Connector.objects.filter(kind=Connector.Kind.POCKETAI, enabled=True).first()
        if pocketai and (pocketai.config.get("demo") or pocketai.credentials.get("api_key")):
            if pocketai.config.get("demo"):
                result["pocketai"] = {"recordings_count": build_demo_pocket_summary(days=7)["recordings_count"]}
            else:
                from sources.heypocket.source import summary

                result["pocketai"] = {"recordings_count": summary(days=7)["recordings_count"]}

        return Response(result)


class PocketAISummaryView(APIView):
    """Backs the dashboard's Pocket card: recording count/duration, tag
    breakdown, recent recordings. Grounded in real recording fields (duration,
    tags) — PocketAI's API doesn't expose a dedicated action-items/todos
    field, so this doesn't invent one."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.POCKETAI, enabled=True).first()
        if not connector:
            return Response({"detail": "PocketAI is not connected"}, status=400)

        days = int(request.query_params.get("days", 30))
        if connector.config.get("demo"):
            return Response(build_demo_pocket_summary(days))

        from sources.heypocket.source import summary

        return Response(summary(days))
