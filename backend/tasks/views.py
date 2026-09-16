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


class TaskLinksView(APIView):
    """GET /api/tasks/<id>/links/ -> the task's typed graph edges (both
    directions), the same list the task_links MCP tool returns (#48)."""

    def get(self, request, pk):
        from tasks.graph import task_links

        out = task_links(pk)
        if "error" in out:
            return Response(out, status=404)
        return Response(out)


class TaskContextView(APIView):
    """GET -> a quick, non-AI look at anything already connected that mentions
    this task's title: a matching calendar event or email thread. Read-only,
    no LLM round-trip — just the same demo-aware tool functions the assistant uses."""

    def get(self, request, pk):
        from aiassist.tools import list_calendar_events, search_emails

        task = get_object_or_404(Task, pk=pk)
        words = [w for w in task.title.split() if len(w) > 3]
        query = " ".join(words[:4]) or task.title

        emails = search_emails(query, max_results=3)
        if isinstance(emails, dict):
            emails = []

        events = list_calendar_events(days_ahead=14, max_results=20)
        if isinstance(events, dict):
            events = []
        keywords = {w.lower() for w in words}
        events = [e for e in events if keywords & set((e.get("summary") or "").lower().split())][:3]

        return Response({"emails": emails, "events": events})


class SeedDemoDataView(APIView):
    """One-button demo data: a handful of "Demo — " prefixed projects with
    ~50 faker-generated tasks spread across the last two months, so the
    dashboard has something to look at. Re-seeding replaces the previous batch."""

    def post(self, request):
        return Response(seed_demo_data())

    def delete(self, request):
        return Response(clear_demo_data())
