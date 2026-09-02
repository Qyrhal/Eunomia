from django.urls import path

from .views import ToolCallView, ToolCatalogueView

urlpatterns = [
    path("tools", ToolCatalogueView.as_view()),
    path("tools/<str:name>", ToolCallView.as_view()),
]
