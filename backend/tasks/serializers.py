import calendar
from datetime import timedelta

from rest_framework import serializers

from .models import Project, Tag, Task


def _advance(dt, recurrence):
    """Next occurrence of `dt` for a recurrence rule, using calendar-correct
    month/year arithmetic (not a 30-day approximation)."""
    if recurrence == Task.Recurrence.DAILY:
        return dt + timedelta(days=1)
    if recurrence == Task.Recurrence.WEEKLY:
        return dt + timedelta(weeks=1)
    if recurrence == Task.Recurrence.MONTHLY:
        month = dt.month % 12 + 1
        year = dt.year + (dt.month // 12)
        day = min(dt.day, calendar.monthrange(year, month)[1])
        return dt.replace(year=year, month=month, day=day)
    if recurrence == Task.Recurrence.YEARLY:
        day = dt.day if not (dt.month == 2 and dt.day == 29) else 28
        return dt.replace(year=dt.year + 1, day=day)
    return None


class TagSerializer(serializers.ModelSerializer):
    class Meta:
        model = Tag
        fields = ["id", "name"]


class TagListField(serializers.ListField):
    """Tags by name, creating any that don't exist yet — the natural way tags
    work in a personal to-do app (type a new name, it just becomes a tag),
    rather than requiring a separate POST /api/tags/ first."""

    child = serializers.CharField()

    def to_representation(self, value):
        return [tag.name for tag in value.all()]

    def to_internal_value(self, data):
        names = super().to_internal_value(data)
        seen = []
        for raw in names:
            name = raw.strip()
            if name and name not in seen:
                seen.append(name)
        return [Tag.objects.get_or_create(name=name)[0] for name in seen]


class ProjectSerializer(serializers.ModelSerializer):
    open_count = serializers.SerializerMethodField()

    class Meta:
        model = Project
        fields = ["id", "name", "color", "icon", "order", "created_at", "open_count"]

    def get_open_count(self, obj):
        return obj.tasks.filter(completed=False, parent__isnull=True).count()


class TaskSerializer(serializers.ModelSerializer):
    tags = TagListField(required=False)
    subtask_count = serializers.IntegerField(source="subtasks.count", read_only=True)

    class Meta:
        model = Task
        fields = [
            "id",
            "project",
            "parent",
            "title",
            "notes",
            "url",
            "due_at",
            "remind_at",
            "allocated_minutes",
            "priority",
            "recurrence",
            "flagged",
            "completed",
            "completed_at",
            "tags",
            "created_by_ai",
            "order",
            "created_at",
            "updated_at",
            "subtask_count",
        ]
        read_only_fields = ["completed_at", "created_at", "updated_at"]

    def create(self, validated_data):
        tags = validated_data.pop("tags", [])
        task = Task.objects.create(**validated_data)
        task.tags.set(tags)
        return task

    def update(self, instance, validated_data):
        tags = validated_data.pop("tags", None)
        just_completed = (
            validated_data.get("completed") is True
            and not instance.completed
            and instance.recurrence != Task.Recurrence.NONE
            and instance.due_at is not None
        )
        next_due = _advance(instance.due_at, instance.recurrence) if just_completed else None

        for attr, value in validated_data.items():
            setattr(instance, attr, value)
        instance.save()
        if tags is not None:
            instance.tags.set(tags)

        if next_due is not None:
            clone = Task.objects.create(
                project=instance.project,
                parent=instance.parent,
                title=instance.title,
                notes=instance.notes,
                url=instance.url,
                due_at=next_due,
                priority=instance.priority,
                recurrence=instance.recurrence,
                allocated_minutes=instance.allocated_minutes,
            )
            clone.tags.set(instance.tags.all())

        return instance
