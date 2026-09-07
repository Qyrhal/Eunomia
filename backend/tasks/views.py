import django_filters
from rest_framework import viewsets
from rest_framework.generics import get_object_or_404
from rest_framework.response import Response
from rest_framework.views import APIView

from .demo_seed import clear_demo_data, seed_demo_data
from .models import Project, Tag, Task
from .serializers import ProjectSerializer, TagSerializer, TaskSerializer


class ProjectViewSet(viewsets.ModelViewSet):
    queryset = Project.objects.all()
    serializer_class = ProjectSerializer


class TagViewSet(viewsets.ModelViewSet):
    queryset = Tag.objects.all()
    serializer_class = TagSerializer


class TaskFilter(django_filters.FilterSet):
    due_before = django_filters.IsoDateTimeFilter(field_name="due_at", lookup_expr="lte")
    due_after = django_filters.IsoDateTimeFilter(field_name="due_at", lookup_expr="gte")
    tag = django_filters.CharFilter(method="filter_tag")
    top_level = django_filters.BooleanFilter(method="filter_top_level")

    class Meta:
        model = Task
        fields = ["project", "completed", "flagged", "priority", "parent", "created_by_ai"]

    def filter_tag(self, queryset, name, value):
        return queryset.filter(tags__name=value)

    def filter_top_level(self, queryset, name, value):
        return queryset.filter(parent__isnull=True) if value else queryset.filter(parent__isnull=False)


class TaskViewSet(viewsets.ModelViewSet):
    queryset = Task.objects.all().prefetch_related("tags", "subtasks")
    serializer_class = TaskSerializer
    filterset_class = TaskFilter
    search_fields = ["title", "notes"]


class TaskContextView(APIView):
    """GET -> Up Bank transactions / heypocket recordings whose title mentions
    this task's title, e.g. surfacing the charge behind a "dispute this" task.
    Read-only, backed by the cache (FTS/embedding search) — no live API calls."""

    def get(self, request, pk):
        from cache.search import search as cache_search

        task = get_object_or_404(Task, pk=pk)
        words = [w for w in task.title.split() if len(w) > 3]
        query = " ".join(words[:4]) or task.title

        hits = cache_search(query, sources=["up_bank", "heypocket"], mode="keyword", limit=6)
        transactions = [h for h in hits if h.type == "up.transaction"][:3]
        recordings = [h for h in hits if h.type == "heypocket.recording"][:3]

        return Response({
            "transactions": [
                {"description": h.title, "amount": (h.payload or {}).get("amount"), "occurred_at": h.occurred_at}
                for h in transactions
            ],
            "recordings": [{"title": h.title, "occurred_at": h.occurred_at} for h in recordings],
        })


class SeedDemoDataView(APIView):
    """One-button demo data: a handful of "Demo — " prefixed projects with
    ~50 faker-generated tasks spread across the last two months, so the
    dashboard has something to look at. Re-seeding replaces the previous batch."""

    def post(self, request):
        return Response(seed_demo_data())

    def delete(self, request):
        return Response(clear_demo_data())
