from django.contrib import admin
from django.urls import include, path

urlpatterns = [
    path("admin/", admin.site.urls),
    path("api/", include("tasks.urls")),
    path("api/", include("connectors.urls")),
    path("api/", include("sources.urls")),
    path("api/", include("masking.urls")),
    path("api/", include("tools.urls")),
    path("api/analytics/", include("analytics.urls")),
]
