from rest_framework.response import Response
from rest_framework.views import APIView

from . import audit
from .models import VaultSecret


class VaultRevealView(APIView):
    """POST {"token": "[eunomia:...]"} -> the real value. Audited (#36).
    The only endpoint that hands a raw value back to a human."""

    def post(self, request):
        token = (request.data or {}).get("token", "")
        if not token:
            return Response({"detail": "token required"}, status=400)
        return Response(audit.reveal(token, actor="frontend"))


class VaultListView(APIView):
    def get(self, request):
        rows = VaultSecret.objects.order_by("type", "token").values(
            "token", "kind", "type", "source", "first_seen", "last_used"
        )
        return Response(list(rows))


class AuditView(APIView):
    def get(self, request):
        return Response(audit.query(kind=request.query_params.get("kind"),
                                    limit=int(request.query_params.get("limit", 100))))
