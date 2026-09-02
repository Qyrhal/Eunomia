from django.urls import path

from .views import AuditView, VaultListView, VaultRevealView

urlpatterns = [
    path("vault/secrets", VaultListView.as_view()),
    path("vault/reveal", VaultRevealView.as_view()),
    path("vault/audit", AuditView.as_view()),
]
