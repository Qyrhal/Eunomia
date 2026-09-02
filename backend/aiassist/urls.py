from django.urls import path

from .views import ChatStreamView, ChatView, GenerateTaskDetailsView, SuggestTasksView

urlpatterns = [
    path("chat", ChatView.as_view()),
    path("chat/stream", ChatStreamView.as_view()),
    path("generate-task-details", GenerateTaskDetailsView.as_view()),
    path("suggest-tasks", SuggestTasksView.as_view()),
]
