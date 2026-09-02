from django.contrib import admin

from .models import AppSettings, Connector

admin.site.register(AppSettings)
admin.site.register(Connector)
