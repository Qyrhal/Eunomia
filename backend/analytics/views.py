from datetime import timedelta

from django.db.models import Count, Sum
from django.db.models.functions import TruncDate
from django.utils import timezone
from rest_framework.response import Response
from rest_framework.views import APIView

from tasks.models import Task


class OverviewView(APIView):
    """Headline numbers for the dashboard's stat tiles."""

    def get(self, request):
        now = timezone.now()
        qs = Task.objects.filter(parent__isnull=True)
        return Response(
            {
                "open": qs.filter(completed=False).count(),
                "completed": qs.filter(completed=True).count(),
                "overdue": qs.filter(completed=False, due_at__lt=now).count(),
                "flagged": qs.filter(completed=False, flagged=True).count(),
                "due_today": qs.filter(
                    completed=False, due_at__date=now.date()
                ).count(),
            }
        )


class CompletionsOverTimeView(APIView):
    """Daily completion counts for the last N days, for a line/bar chart."""

    def get(self, request):
        days = int(request.query_params.get("days", 30))
        since = timezone.now() - timedelta(days=days)
        rows = (
            Task.objects.filter(completed=True, completed_at__gte=since)
            .annotate(day=TruncDate("completed_at"))
            .values("day")
            .annotate(count=Count("id"))
            .order_by("day")
        )
        return Response(list(rows))


class UpcomingLoadView(APIView):
    """Allocated minutes per upcoming day, i.e. a simple workload projection."""

    def get(self, request):
        days = int(request.query_params.get("days", 14))
        now = timezone.now()
        until = now + timedelta(days=days)
        rows = (
            Task.objects.filter(completed=False, due_at__gte=now, due_at__lte=until)
            .annotate(day=TruncDate("due_at"))
            .values("day")
            .annotate(count=Count("id"), minutes=Sum("allocated_minutes"))
            .order_by("day")
        )
        return Response(list(rows))


class ProjectBreakdownView(APIView):
    """Open task count per project, for a bar chart."""

    def get(self, request):
        rows = (
            Task.objects.filter(completed=False)
            .values("project__name", "project__color")
            .annotate(count=Count("id"))
            .order_by("-count")
        )
        return Response(list(rows))


class PriorityBreakdownView(APIView):
    """Open task count per priority level."""

    def get(self, request):
        rows = (
            Task.objects.filter(completed=False)
            .values("priority")
            .annotate(count=Count("id"))
            .order_by("priority")
        )
        labels = dict(Task.Priority.choices)
        return Response(
            [{"priority": r["priority"], "label": labels[r["priority"]], "count": r["count"]} for r in rows]
        )


class AiContributionView(APIView):
    """How many of your open tasks the assistant created vs. you creating them yourself."""

    def get(self, request):
        qs = Task.objects.filter(parent__isnull=True, completed=False)
        return Response(
            {
                "ai_created": qs.filter(created_by_ai=True).count(),
                "human_created": qs.filter(created_by_ai=False).count(),
            }
        )


class WeekOverWeekView(APIView):
    """Completion rate this week vs. last week, for a headline delta stat."""

    def get(self, request):
        now = timezone.now()
        start_of_week = (now - timedelta(days=now.weekday())).replace(
            hour=0, minute=0, second=0, microsecond=0
        )
        start_of_last_week = start_of_week - timedelta(days=7)

        this_week = Task.objects.filter(completed=True, completed_at__gte=start_of_week).count()
        last_week = Task.objects.filter(
            completed=True, completed_at__gte=start_of_last_week, completed_at__lt=start_of_week
        ).count()

        delta_pct = None
        if last_week:
            delta_pct = round((this_week - last_week) / last_week * 100)
        elif this_week:
            delta_pct = 100

        return Response({"this_week": this_week, "last_week": last_week, "delta_pct": delta_pct})
