"""Fake connector data for demo mode. Each `seed_demo_*` function generates a
batch of rows and flips the matching connector into demo mode (`config.demo =
True`); the API views and AI tools then build the exact same response shape
the real client would, just from local rows instead of a live API call —
see `is_*_demo_mode` and the `build_demo_*` functions used by
connectors/views.py and aiassist/tools.py.
"""

import random
from collections import defaultdict
from datetime import datetime, timedelta

from django.utils import timezone
from faker import Faker

from .models import Connector, DemoRecording, DemoTransaction

ACCOUNTS = ["Spending", "Saver"]

MERCHANTS_BY_CATEGORY = {
    "Groceries": ["Woolworths", "Coles", "Aldi", "IGA"],
    "Transport": ["Uber", "Opal", "Shell", "BP"],
    "Dining out": ["Guzman y Gomez", "Corner Cafe", "Deliveroo", "Menulog"],
    "Subscriptions": ["Netflix", "Spotify", "iCloud", "GitHub"],
    "Shopping": ["Amazon", "Kmart", "Bunnings", "Officeworks"],
    "Entertainment": ["Ticketek", "Steam", "Event Cinemas"],
}


def seed_demo_bank_data(seed: int | None = None) -> dict:
    fake = Faker()
    if seed is not None:
        Faker.seed(seed)
        random.seed(seed)

    clear_demo_bank_data()

    now = timezone.now()
    rows = []
    for _ in range(55):
        category = random.choice(list(MERCHANTS_BY_CATEGORY))
        rows.append(
            DemoTransaction(
                # a savings account realistically sees far fewer day-to-day debits
                account=random.choices(ACCOUNTS, weights=[85, 15])[0],
                description=random.choice(MERCHANTS_BY_CATEGORY[category]),
                category=category,
                amount_cents=-random.randint(500, 12000),
                created_at=now - timedelta(days=random.randint(0, 45), hours=random.randint(0, 23)),
            )
        )
    for _ in range(2):
        rows.append(
            DemoTransaction(
                account="Spending",
                description="Salary",
                category="Income",
                amount_cents=random.randint(250000, 400000),
                created_at=now - timedelta(days=random.choice([3, 17, 31])),
            )
        )
    # keep the Saver account plausible with a standing balance of its own
    rows.append(
        DemoTransaction(
            account="Saver",
            description="Transfer from Spending",
            category="Transfer",
            amount_cents=random.randint(100000, 300000),
            created_at=now - timedelta(days=random.randint(30, 44)),
        )
    )
    DemoTransaction.objects.bulk_create(rows)

    connector, _ = Connector.objects.get_or_create(kind=Connector.Kind.UP_BANK)
    connector.enabled = True
    connector.config = {**connector.config, "demo": True}
    connector.save()

    return {"transactions": len(rows)}


def clear_demo_bank_data() -> dict:
    count = DemoTransaction.objects.count()
    DemoTransaction.objects.all().delete()

    connector = Connector.objects.filter(kind=Connector.Kind.UP_BANK).first()
    if connector and connector.config.get("demo"):
        connector.config = {k: v for k, v in connector.config.items() if k != "demo"}
        connector.enabled = bool(connector.credentials_encrypted)
        connector.save()

    return {"transactions_removed": count}


def is_demo_mode() -> bool:
    connector = Connector.objects.filter(kind=Connector.Kind.UP_BANK, enabled=True).first()
    return bool(connector and connector.config.get("demo"))


def build_demo_finance_summary(since: datetime) -> dict:
    qs = DemoTransaction.objects.filter(created_at__gte=since)
    balance = sum(t.amount_cents for t in DemoTransaction.objects.all()) / 100

    balances_by_account: dict[str, int] = defaultdict(int)
    for t in DemoTransaction.objects.all():
        balances_by_account[t.account] += t.amount_cents

    spend_by_category: dict[str, int] = defaultdict(int)
    spend_by_day: dict[str, int] = defaultdict(int)
    for t in qs.filter(amount_cents__lt=0):
        spend_by_category[t.category] += -t.amount_cents
        spend_by_day[t.created_at.strftime("%Y-%m-%d")] += -t.amount_cents

    return {
        "balance": round(balance, 2),
        "accounts": [{"name": name, "balance": f"{cents / 100:.2f}"} for name, cents in balances_by_account.items()],
        "spend_by_category": sorted(
            [{"category": k, "amount": round(v / 100, 2)} for k, v in spend_by_category.items()],
            key=lambda r: -r["amount"],
        ),
        "spend_by_day": sorted(
            [{"day": k, "amount": round(v / 100, 2)} for k, v in spend_by_day.items()],
            key=lambda r: r["day"],
        ),
        "recent_transactions": [
            {
                "description": t.description,
                "amount": f"{'+' if t.amount_cents >= 0 else ''}{t.amount_cents / 100:.2f}",
                "created_at": t.created_at.isoformat(),
            }
            for t in qs.order_by("-created_at")[:20]
        ],
    }


def build_demo_week_summary(since: datetime) -> dict:
    qs = DemoTransaction.objects.filter(created_at__gte=since)
    spent = -sum(t.amount_cents for t in qs.filter(amount_cents__lt=0))
    return {"transaction_count": qs.count(), "spent": round(spent / 100, 2)}


def build_demo_transactions(days: int = 7) -> list[dict]:
    """Shape matches `aiassist.tools.list_recent_transactions`'s real-API output."""
    since = timezone.now() - timedelta(days=days)
    qs = DemoTransaction.objects.filter(created_at__gte=since).order_by("-created_at")
    return [
        {
            "description": t.description,
            "amount": f"{'+' if t.amount_cents >= 0 else ''}{t.amount_cents / 100:.2f}",
            "status": "SETTLED",
            "created_at": t.created_at.isoformat(),
        }
        for t in qs
    ]


# ---- PocketAI ----

POCKET_TITLES = [
    "Weekly standup",
    "1:1 with manager",
    "Client call — Acme Corp",
    "Sprint planning",
    "Design review",
    "Onboarding call",
    "Product sync",
    "Retro",
    "All-hands",
    "Customer interview",
]
POCKET_TAGS = ["work", "client", "internal", "personal"]


def seed_demo_pocket_data(seed: int | None = None) -> dict:
    fake = Faker()
    if seed is not None:
        Faker.seed(seed)
        random.seed(seed)

    clear_demo_pocket_data()

    now = timezone.now()
    rows = [
        DemoRecording(
            title=random.choice(POCKET_TITLES),
            duration_seconds=random.randint(600, 3600),
            tags=random.sample(POCKET_TAGS, k=random.randint(1, 2)),
            recorded_at=now - timedelta(days=random.randint(0, 30), hours=random.randint(0, 23)),
        )
        for _ in range(16)
    ]
    DemoRecording.objects.bulk_create(rows)

    connector, _ = Connector.objects.get_or_create(kind=Connector.Kind.POCKETAI)
    connector.enabled = True
    connector.config = {**connector.config, "demo": True}
    connector.save()

    return {"recordings": len(rows)}


def clear_demo_pocket_data() -> dict:
    count = DemoRecording.objects.count()
    DemoRecording.objects.all().delete()

    connector = Connector.objects.filter(kind=Connector.Kind.POCKETAI).first()
    if connector and connector.config.get("demo"):
        connector.config = {k: v for k, v in connector.config.items() if k != "demo"}
        connector.enabled = bool(connector.credentials_encrypted)
        connector.save()

    return {"recordings_removed": count}


def is_pocket_demo_mode() -> bool:
    connector = Connector.objects.filter(kind=Connector.Kind.POCKETAI, enabled=True).first()
    return bool(connector and connector.config.get("demo"))


def build_demo_pocket_summary(days: int = 30) -> dict:
    since = timezone.now() - timedelta(days=days)
    qs = DemoRecording.objects.filter(recorded_at__gte=since)

    tag_counts: dict[str, int] = defaultdict(int)
    for r in qs:
        for t in r.tags:
            tag_counts[t] += 1

    return {
        "recordings_count": qs.count(),
        "total_duration_minutes": round(sum(r.duration_seconds for r in qs) / 60, 1),
        "tag_breakdown": sorted(
            [{"tag": k, "count": v} for k, v in tag_counts.items()], key=lambda r: -r["count"]
        ),
        "recent_recordings": [
            {
                "title": r.title,
                "duration_minutes": round(r.duration_seconds / 60, 1),
                "recorded_at": r.recorded_at.isoformat(),
                "tags": r.tags,
            }
            for r in qs.order_by("-recorded_at")[:10]
        ],
    }
