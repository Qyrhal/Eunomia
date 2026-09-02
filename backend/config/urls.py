from django.contrib import admin
from django.urls import include, path

urlpatterns = [
    path("admin/", admin.site.urls),
    path("api/", include("tasks.urls")),
    path("api/", include("connectors.urls")),
    path("api/ai/", include("aiassist.urls")),
    path("api/analytics/", include("analytics.urls")),
]
