from rest_framework.response import Response
from rest_framework.views import APIView

from . import registry
from .models import SyncStatus
from .scheduler import sync_source


class SourceListView(APIView):
    def get(self, request):
        status = {s.source_key: s for s in SyncStatus.objects.all()}
        return Response([
            {
                "key": s.key,
                "label": s.label,
                "provider": s.provider_key,
                "record_types": s.record_types,
                "enabled": s in registry.enabled(),
                "last_run": (st.last_run.isoformat() if (st := status.get(s.key)) and st.last_run else None),
                "last_error": (status[s.key].last_error if s.key in status else ""),
            }
            for s in registry.all()
        ])


class SourceSyncView(APIView):
    """POST /api/sources/<key>/sync -> run one sync now."""

    def post(self, request, key):
        if registry.get(key) is None:
            return Response({"detail": f"no source {key}"}, status=404)
        return Response(sync_source(key, mode="poll"))
