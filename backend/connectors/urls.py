from django.urls import path

from .views import (
    ConnectorDetailView,
    ConnectorListView,
    ConnectorTestView,
    GmailMessagesView,
    GoogleAuthCallbackView,
    GoogleAuthStartView,
    GoogleCalendarEventsView,
    PocketAISummaryView,
    PocketAISearchView,
    PocketAIDetailView,
    PocketAIAllView,
    SettingsView,
    SnapshotView,
    UpBankFinanceSummaryView,
    UpBankTransactionsView,
)

urlpatterns = [
    path("settings", SettingsView.as_view()),
    path("connectors", ConnectorListView.as_view()),
    path("connectors/snapshot", SnapshotView.as_view()),
    path("connectors/<str:kind>", ConnectorDetailView.as_view()),
    path("connectors/<str:kind>/test", ConnectorTestView.as_view()),
    path("connectors/google/auth/start", GoogleAuthStartView.as_view()),
    path("connectors/google/callback", GoogleAuthCallbackView.as_view()),
    path("connectors/google/calendar/events", GoogleCalendarEventsView.as_view()),
    path("connectors/google/gmail/messages", GmailMessagesView.as_view()),
    path("connectors/up_bank/transactions", UpBankTransactionsView.as_view()),
    path("connectors/up_bank/finance-summary", UpBankFinanceSummaryView.as_view()),
    path("connectors/pocketai/summary", PocketAISummaryView.as_view()),
    path("connectors/pocketai/recordings", PocketAIAllView.as_view()),
    path("connectors/pocketai/recording/<str:recording_id>", PocketAIDetailView.as_view()),
    path("connectors/pocketai/search", PocketAISearchView.as_view()),
]
