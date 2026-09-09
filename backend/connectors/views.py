import os

from django.shortcuts import redirect
from django.utils import timezone
from rest_framework.response import Response
from rest_framework.views import APIView

from .clients import GoogleClient, PocketAIClient, TwentyClient, UpBankClient, google_oauth_flow
from .demo_seed import (
    build_demo_calendar_events_today,
    build_demo_finance_summary,
    build_demo_gmail_unread_count,
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
            elif kind == Connector.Kind.GOOGLE:
                ok = connector.config.get("demo") or bool(connector.credentials.get("refresh_token"))
            elif kind == Connector.Kind.TWENTY:
                ok = connector.config.get("demo") or TwentyClient(connector.credentials, connector.config.get("base_url")).ping()
            else:
                return Response({"detail": "unknown connector"}, status=400)
        except Exception as exc:  # surfaced to the settings UI, not swallowed
            return Response({"ok": False, "error": str(exc)}, status=200)
        return Response({"ok": ok})


def _google_client_config(connector: Connector):
    """Client ID/secret come from the Google connector's saved credentials (set in
    Settings); fall back to env vars so a deployment can also configure it that way."""
    creds = connector.credentials
    client_id = creds.get("client_id") or os.environ.get("GOOGLE_OAUTH_CLIENT_ID", "")
    client_secret = creds.get("client_secret") or os.environ.get("GOOGLE_OAUTH_CLIENT_SECRET", "")
    redirect_uri = os.environ.get(
        "GOOGLE_OAUTH_REDIRECT_URI", "http://localhost:8000/api/connectors/google/callback"
    )
    return {
        "web": {
            "client_id": client_id,
            "client_secret": client_secret,
            "auth_uri": "https://accounts.google.com/o/oauth2/auth",
            "token_uri": "https://oauth2.googleapis.com/token",
            "redirect_uris": [redirect_uri],
        }
    }, redirect_uri


class GoogleAuthStartView(APIView):
    """Redirects the browser into Google's consent screen."""

    def get(self, request):
        connector, _ = Connector.objects.get_or_create(kind=Connector.Kind.GOOGLE)
        config, redirect_uri = _google_client_config(connector)
        if not config["web"]["client_id"] or not config["web"]["client_secret"]:
            return Response(
                {"detail": "Save a Google OAuth client ID and secret in Settings first."},
                status=400,
            )
        flow = google_oauth_flow(config, redirect_uri)
        auth_url, _ = flow.authorization_url(
            access_type="offline", include_granted_scopes="true", prompt="consent"
        )
        return redirect(auth_url)


class GoogleAuthCallbackView(APIView):
    """Google redirects back here with ?code=...; we exchange it and store tokens."""

    def get(self, request):
        connector, _ = Connector.objects.get_or_create(kind=Connector.Kind.GOOGLE)
        config, redirect_uri = _google_client_config(connector)
        flow = google_oauth_flow(config, redirect_uri)
        flow.fetch_token(code=request.query_params.get("code"))
        creds = flow.credentials

        connector.credentials = {
            "token": creds.token,
            "refresh_token": creds.refresh_token,
            "client_id": creds.client_id,
            "client_secret": creds.client_secret,
        }
        connector.enabled = True
        connector.save()

        frontend_url = os.environ.get("FRONTEND_URL", "http://localhost:3000")
        return redirect(f"{frontend_url}/settings?connected=google")


class GoogleCalendarEventsView(APIView):
    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.GOOGLE, enabled=True).first()
        if not connector:
            return Response({"detail": "Google is not connected"}, status=400)
        client = GoogleClient(connector.credentials)
        events = client.calendar_events(max_results=int(request.query_params.get("max", 20)))
        connector.credentials = client.refreshed_credentials_dict()
        connector.save()
        return Response(events)


class GmailMessagesView(APIView):
    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.GOOGLE, enabled=True).first()
        if not connector:
            return Response({"detail": "Google is not connected"}, status=400)
        client = GoogleClient(connector.credentials)
        query = request.query_params.get("q", "is:unread")
        messages = client.gmail_messages(query=query, max_results=int(request.query_params.get("max", 20)))
        connector.credentials = client.refreshed_credentials_dict()
        connector.save()
        return Response(messages)


class UpBankTransactionsView(APIView):
    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
        if not connector:
            return Response({"detail": "Up Bank is not connected"}, status=400)
        data = UpBankClient(connector.credentials).transactions()
        return Response(data)


class UpBankFinanceSummaryView(APIView):
    """Backs the /finance page: balance, spend by category, recent transactions."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
        if not connector:
            return Response({"detail": "Up Bank is not connected"}, status=400)
        import datetime

        days = int(request.query_params.get("days", 30))
        since = timezone.now() - datetime.timedelta(days=days)
        if connector.config.get("demo"):
            return Response(build_demo_finance_summary(since))
        try:
            data = UpBankClient(connector.credentials).finance_summary(since.isoformat())
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        return Response(data)


class SnapshotView(APIView):
    """Live, best-effort figures pulled straight from each connected account —
    only ever the fields those APIs actually expose. Any connector that isn't
    connected, or errors, comes back as null rather than failing the whole call."""

    def get(self, request):
        import datetime

        result = {"google": None, "up_bank": None, "pocketai": None, "twenty_crm": None}

        google = Connector.objects.filter(kind=Connector.Kind.GOOGLE, enabled=True).first()
        if google and (google.config.get("demo") or google.credentials.get("refresh_token")):
            try:
                if google.config.get("demo"):
                    result["google"] = {
                        "calendar_events_today": build_demo_calendar_events_today(),
                        "gmail_unread": build_demo_gmail_unread_count(),
                    }
                else:
                    client = GoogleClient(google.credentials)
                    result["google"] = {
                        "calendar_events_today": client.calendar_events_today(),
                        "gmail_unread": client.gmail_unread_count(),
                    }
                    google.credentials = client.refreshed_credentials_dict()
                    google.save()
            except Exception as exc:
                result["google"] = {"error": str(exc)}

        up_bank = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
        if up_bank and (up_bank.config.get("demo") or up_bank.credentials.get("personal_access_token")):
            try:
                since = (
                    timezone.now() - datetime.timedelta(days=timezone.now().weekday())
                ).replace(hour=0, minute=0, second=0, microsecond=0)
                if up_bank.config.get("demo"):
                    result["up_bank"] = build_demo_week_summary(since)
                else:
                    result["up_bank"] = UpBankClient(up_bank.credentials).week_summary(since.isoformat())
            except Exception as exc:
                result["up_bank"] = {"error": str(exc)}

        pocketai = Connector.objects.filter(kind=Connector.Kind.POCKETAI, enabled=True).first()
        if pocketai and (pocketai.config.get("demo") or pocketai.credentials.get("api_key")):
            try:
                if pocketai.config.get("demo"):
                    result["pocketai"] = {"recordings_count": build_demo_pocket_summary(days=7)["recordings_count"]}
                else:
                    client = PocketAIClient(pocketai.credentials, pocketai.config.get("base_url"))
                    data = client.recordings({"limit": 1})
                    result["pocketai"] = {"recordings_count": len(data.get("data", data.get("recordings", [])))}
            except Exception as exc:
                result["pocketai"] = {"error": str(exc)}

        twenty = Connector.objects.filter(kind=Connector.Kind.TWENTY, enabled=True).first()
        if twenty and (twenty.config.get("demo") or twenty.credentials.get("api_token")):
            try:
                result["twenty_crm"] = TwentyClient(twenty.credentials, twenty.config.get("base_url")).summary()
            except Exception as exc:
                result["twenty_crm"] = {"error": str(exc)}

        return Response(result)


class TwentyCRMPeopleView(APIView):
    """GET /api/connectors/twenty/people -> list people (contacts) from Twenty CRM."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.TWENTY, enabled=True).first()
        if not connector:
            return Response({"detail": "Twenty CRM is not connected"}, status=400)
        limit = int(request.query_params.get("limit", 50))
        cursor = request.query_params.get("cursor")
        try:
            data = TwentyClient(connector.credentials, connector.config.get("base_url")).people(
                limit=limit, cursor=cursor
            )
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        return Response(data)


class TwentyCRMCompaniesView(APIView):
    """GET /api/connectors/twenty/companies -> list companies from Twenty CRM."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.TWENTY, enabled=True).first()
        if not connector:
            return Response({"detail": "Twenty CRM is not connected"}, status=400)
        limit = int(request.query_params.get("limit", 50))
        cursor = request.query_params.get("cursor")
        try:
            data = TwentyClient(connector.credentials, connector.config.get("base_url")).companies(
                limit=limit, cursor=cursor
            )
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        return Response(data)


class TwentyCRMDealsView(APIView):
    """GET /api/connectors/twenty/deals -> list deals from Twenty CRM."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.TWENTY, enabled=True).first()
        if not connector:
            return Response({"detail": "Twenty CRM is not connected"}, status=400)
        limit = int(request.query_params.get("limit", 50))
        cursor = request.query_params.get("cursor")
        try:
            data = TwentyClient(connector.credentials, connector.config.get("base_url")).deals(
                limit=limit, cursor=cursor
            )
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        return Response(data)


class TwentyCRMTasksView(APIView):
    """GET /api/connectors/twenty/tasks -> list tasks from Twenty CRM."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.TWENTY, enabled=True).first()
        if not connector:
            return Response({"detail": "Twenty CRM is not connected"}, status=400)
        limit = int(request.query_params.get("limit", 50))
        cursor = request.query_params.get("cursor")
        try:
            data = TwentyClient(connector.credentials, connector.config.get("base_url")).tasks(
                limit=limit, cursor=cursor
            )
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        return Response(data)


class TwentyCRMNotesView(APIView):
    """GET /api/connectors/twenty/notes -> list notes from Twenty CRM."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.TWENTY, enabled=True).first()
        if not connector:
            return Response({"detail": "Twenty CRM is not connected"}, status=400)
        limit = int(request.query_params.get("limit", 50))
        cursor = request.query_params.get("cursor")
        try:
            data = TwentyClient(connector.credentials, connector.config.get("base_url")).notes(
                limit=limit, cursor=cursor
            )
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        return Response(data)


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

        import datetime

        since = (timezone.now() - datetime.timedelta(days=days)).strftime("%Y-%m-%d")
        try:
            client = PocketAIClient(connector.credentials, connector.config.get("base_url"))
            data = client.summary(since)
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        return Response(data)


def _demo_recordings():
    rows = DemoRecording.objects.all().order_by("-recorded_at")[:50]
    return [
        {
            "id": f"demo-{r.id}",
            "title": r.title,
            "summary": f"Demo recording: {r.title}",
            "duration": r.duration_seconds,
            "tags": [{"name": t} for t in r.tags],
            "recording_at": r.recorded_at.isoformat(),
            "url": "",
        }
        for r in rows
    ]


def _demo_search(query):
    rows = DemoRecording.objects.filter(title__icontains=query).order_by("-recorded_at")[:20]
    return {
        "success": True,
        "data": {
            "results": [
                {
                    "id": f"demo-{r.id}",
                    "title": r.title,
                    "summary": f"Demo recording: {r.title}",
                    "duration": r.duration_seconds,
                    "tags": [{"name": t} for t in r.tags],
                    "recording_at": r.recorded_at.isoformat(),
                }
                for r in rows
            ]
        },
    }


class PocketAIAllView(APIView):
    """All heypocket recordings (paginated) — backs the Meetings page."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.POCKETAI, enabled=True).first()
        if not connector:
            return Response({"detail": "PocketAI is not connected"}, status=400)

        if connector.config.get("demo"):
            return Response({"demo": True, "recordings": _demo_recordings()})

        limit = min(int(request.query_params.get("limit", 50)), 200)
        offset = int(request.query_params.get("offset", 0))
        try:
            client = PocketAIClient(connector.credentials, connector.config.get("base_url"))
            data = client.recordings({"limit": limit, "offset": offset})
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        recordings = data.get("data", data.get("recordings", []))
        return Response({"recordings": recordings, "count": len(recordings)})


class PocketAISearchView(APIView):
    """Full-text search across heypocket recordings.

    The PocketAI /public/search endpoint returns synthesized insights under
    data.userProfile.dynamicContext[] — high-level meeting takeaways, not
    per-recording results. We surface those as "insights" alongside any
    recording-level matches the API may also return."""

    def get(self, request):
        connector = Connector.objects.filter(kind=Connector.Kind.POCKETAI, enabled=True).first()
        if not connector:
            return Response({"detail": "PocketAI is not connected"}, status=400)

        query = request.query_params.get("q", "").strip()
        if not query:
            return Response({"detail": "query param 'q' is required"}, status=400)

        if connector.config.get("demo"):
            return Response({"demo": True, "results": _demo_search(query)})

        try:
            client = PocketAIClient(connector.credentials, connector.config.get("base_url"))
            data = client.search(query)
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        # Normalize the response: the API returns insights under
        # data.userProfile.dynamicContext; pass through results if present.
        user_profile = (data.get("data") or {}).get("userProfile") or {}
        return Response({
            "success": data.get("success", True),
            "insights": user_profile.get("dynamicContext", []),
            "static_facts": user_profile.get("staticFacts", []),
            "recordings": data.get("data", {}).get("recordings", []),
        })


class PocketAIDetailView(APIView):
    """Full detail for a single recording — transcript + summarizations."""

    def get(self, request, recording_id):
        connector = Connector.objects.filter(kind=Connector.Kind.POCKETAI, enabled=True).first()
        if not connector:
            return Response({"detail": "PocketAI is not connected"}, status=400)

        try:
            client = PocketAIClient(connector.credentials, connector.config.get("base_url"))
            data = client.recording(recording_id)
        except Exception as exc:
            return Response({"detail": str(exc)}, status=502)
        return Response(data)
