from django.urls import path

from .views import SourceListView, SourceSyncView, SourceWebhookView

urlpatterns = [
    path("sources", SourceListView.as_view()),
    path("sources/<str:key>/sync", SourceSyncView.as_view()),
    path("sources/<str:key>/webhook", SourceWebhookView.as_view()),
]
