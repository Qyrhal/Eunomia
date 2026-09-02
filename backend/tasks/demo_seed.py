"""Faker-powered demo data — realistic-looking tasks/projects to populate the
dashboard with, for trying the app out or taking screenshots. Every project it
creates is prefixed so it can be told apart from real data and cleared cleanly.
"""

import random
from datetime import timedelta

from connectors.demo_seed import (
    clear_demo_bank_data,
    clear_demo_google_data,
    clear_demo_pocket_data,
    seed_demo_bank_data,
    seed_demo_google_data,
    seed_demo_pocket_data,
)
from django.utils import timezone
from faker import Faker

from .models import Project, Task

DEMO_PREFIX = "Demo — "

DEMO_PROJECTS = [
    ("Personal", "#a8752f"),
    ("Work", "#1f8a75"),
    ("Errands", "#b03f28"),
    ("Side project", "#235c99"),
]


def seed_demo_data(seed: int | None = None) -> dict:
    fake = Faker()
    if seed is not None:
        Faker.seed(seed)
        random.seed(seed)

    clear_demo_data()

    projects = [
        Project.objects.create(name=f"{DEMO_PREFIX}{name}", color=color, order=i)
        for i, (name, color) in enumerate(DEMO_PROJECTS)
    ]

    now = timezone.now()
    to_backdate = []
    total_tasks = 50
    for _ in range(total_tasks):
        project = random.choice(projects)
        completed = random.random() < 0.6
        task = Task.objects.create(
            project=project,
            title=fake.sentence(nb_words=5).rstrip("."),
            notes=fake.sentence(nb_words=12) if random.random() < 0.5 else "",
            due_at=(now + timedelta(days=random.randint(-45, 14), hours=random.randint(-8, 8)))
            if random.random() < 0.7
            else None,
            priority=random.choices([0, 1, 2, 3], weights=[40, 25, 20, 15])[0],
            flagged=random.random() < 0.12,
            completed=completed,
            created_by_ai=random.random() < 0.2,
            allocated_minutes=random.choice([None, 15, 30, 45, 60, 90]),
        )
        if completed:
            # Task.save() stamps completed_at=now on create; backdate it after
            # the fact (via update(), bypassing save()) so the completions
            # chart has a spread of history instead of one spike today.
            backdated = now - timedelta(days=random.randint(0, 56), hours=random.randint(0, 23))
            to_backdate.append((task.id, backdated))

    for task_id, backdated in to_backdate:
        Task.objects.filter(id=task_id).update(completed_at=backdated)

    bank = seed_demo_bank_data(seed=seed)
    google = seed_demo_google_data(seed=seed)
    pocket = seed_demo_pocket_data(seed=seed)

    return {
        "projects": len(projects),
        "tasks": total_tasks,
        "transactions": bank["transactions"],
        "calendar_events": google["events"],
        "emails": google["emails"],
        "recordings": pocket["recordings"],
    }


def clear_demo_data() -> dict:
    qs = Project.objects.filter(name__startswith=DEMO_PREFIX)
    count = qs.count()
    qs.delete()
    bank = clear_demo_bank_data()
    google = clear_demo_google_data()
    pocket = clear_demo_pocket_data()
    return {
        "projects_removed": count,
        "transactions_removed": bank["transactions_removed"],
        "calendar_events_removed": google["events_removed"],
        "emails_removed": google["emails_removed"],
        "recordings_removed": pocket["recordings_removed"],
    }
