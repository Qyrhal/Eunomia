from django.urls import path
from rest_framework.routers import DefaultRouter

from .views import ProjectViewSet, SeedDemoDataView, TagViewSet, TaskContextView, TaskLinksView, TaskViewSet

router = DefaultRouter()
router.register("projects", ProjectViewSet)
router.register("tags", TagViewSet)
router.register("tasks", TaskViewSet)

urlpatterns = [
    path("demo-data", SeedDemoDataView.as_view()),
    path("tasks/<uuid:pk>/context/", TaskContextView.as_view()),
    path("tasks/<uuid:pk>/links/", TaskLinksView.as_view()),
] + router.urls
