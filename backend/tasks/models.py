import uuid

from django.db import models


class Project(models.Model):
    """A bucket of tasks — Apple Reminders' "lists", renamed to how you actually think about them."""

    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    name = models.CharField(max_length=100)
    color = models.CharField(max_length=20, default="#0A84FF")
    icon = models.CharField(max_length=50, default="list.bullet")
    order = models.IntegerField(default=0)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["order", "name"]

    def __str__(self):
        return self.name


class Tag(models.Model):
    name = models.CharField(max_length=50, unique=True)

    def __str__(self):
        return self.name


class Task(models.Model):
    class Priority(models.IntegerChoices):
        NONE = 0, "None"
        LOW = 1, "Low"
        MEDIUM = 2, "Medium"
        HIGH = 3, "High"

    class Recurrence(models.TextChoices):
        NONE = "none", "None"
        DAILY = "daily", "Daily"
        WEEKLY = "weekly", "Weekly"
        MONTHLY = "monthly", "Monthly"
        YEARLY = "yearly", "Yearly"

    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    project = models.ForeignKey(Project, on_delete=models.CASCADE, related_name="tasks")
    parent = models.ForeignKey(
        "self", on_delete=models.CASCADE, related_name="subtasks", null=True, blank=True
    )
    title = models.CharField(max_length=500)
    notes = models.TextField(blank=True, default="")
    url = models.URLField(blank=True, default="")

    due_at = models.DateTimeField(null=True, blank=True)
    remind_at = models.DateTimeField(null=True, blank=True)
    allocated_minutes = models.PositiveIntegerField(
        null=True, blank=True, help_text="Time block allocated to this task, in minutes"
    )

    priority = models.IntegerField(choices=Priority.choices, default=Priority.NONE)
    recurrence = models.CharField(
        max_length=10, choices=Recurrence.choices, default=Recurrence.NONE
    )
    flagged = models.BooleanField(default=False)
    completed = models.BooleanField(default=False)
    completed_at = models.DateTimeField(null=True, blank=True)

    tags = models.ManyToManyField(Tag, blank=True, related_name="tasks")

    # arbitrary agent-set key/values — the unstructured half of the task graph (#38)
    props = models.JSONField(default=dict, blank=True)
    # whether this task has a vector in cache_vec (id = "task:<uuid>")
    has_embedding = models.BooleanField(default=False)

    # set by the AI assistant / MCP callers when it creates or edits a task on your behalf
    created_by_ai = models.BooleanField(default=False)

    order = models.IntegerField(default=0)
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["completed", "order", "due_at"]

    def __str__(self):
        return self.title

    def save(self, *args, **kwargs):
        if self.completed and self.completed_at is None:
            from django.utils import timezone

            self.completed_at = timezone.now()
        if not self.completed:
            self.completed_at = None
        super().save(*args, **kwargs)
