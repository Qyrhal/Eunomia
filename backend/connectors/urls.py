from django.urls import path

from .views import (
    ConnectorDetailView,
    ConnectorListView,
    ConnectorTestView,
    PocketAISummaryView,
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
    path("connectors/up_bank/transactions", UpBankTransactionsView.as_view()),
    path("connectors/up_bank/finance-summary", UpBankFinanceSummaryView.as_view()),
    path("connectors/pocketai/summary", PocketAISummaryView.as_view()),
]
