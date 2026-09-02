from django.urls import path

from .views import (
    AiContributionView,
    CompletionsOverTimeView,
    OverviewView,
    PriorityBreakdownView,
    ProjectBreakdownView,
    UpcomingLoadView,
    WeekOverWeekView,
)

urlpatterns = [
    path("overview", OverviewView.as_view()),
    path("completions", CompletionsOverTimeView.as_view()),
    path("upcoming-load", UpcomingLoadView.as_view()),
    path("project-breakdown", ProjectBreakdownView.as_view()),
    path("priority-breakdown", PriorityBreakdownView.as_view()),
    path("ai-contribution", AiContributionView.as_view()),
    path("week-over-week", WeekOverWeekView.as_view()),
]
