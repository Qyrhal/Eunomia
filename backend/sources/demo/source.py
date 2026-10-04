"""Demo source -- pushes synthetic Up Bank + heypocket data through the real
ingest pipeline so the cache, search, and tools all work with no live
accounts and no external API calls.

Deviation from the Django version: the old implementation read persisted
`DemoTransaction`/`DemoRecording` rows (seeded by a management command) out of
the database. The SurrealDB schema (already defined by a previous phase) has
no such demo tables and this task's scope is "use the schema, don't redefine
it" -- so this version generates the synthetic records inline, in `sync()`,
with a fixed seed for reproducibility, instead of reading them back from a
dedicated table.
"""

import random
from datetime import datetime, timedelta, timezone

from sources.base import Source, SyncResult

ACCOUNTS = ["Spending", "Saver"]

MERCHANTS_BY_CATEGORY = {
    "Groceries": ["Woolworths", "Coles", "Aldi", "IGA"],
    "Transport": ["Uber", "Opal", "Shell", "BP"],
    "Dining out": ["Guzman y Gomez", "Corner Cafe", "Deliveroo", "Menulog"],
    "Subscriptions": ["Netflix", "Spotify", "iCloud", "GitHub"],
    "Shopping": ["Amazon", "Kmart", "Bunnings", "Officeworks"],
    "Entertainment": ["Ticketek", "Steam", "Event Cinemas"],
}

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


def _gen_transactions(rng: random.Random, now: datetime) -> list[dict]:
    rows = []
    for i in range(55):
        category = rng.choice(list(MERCHANTS_BY_CATEGORY))
        rows.append(
            {
                "_kind": "txn",
                "id": f"txn{i}",
                "account": rng.choices(ACCOUNTS, weights=[85, 15])[0],
                "description": rng.choice(MERCHANTS_BY_CATEGORY[category]),
                "category": category,
                "amount_cents": -rng.randint(500, 12000),
                "created_at": now - timedelta(days=rng.randint(0, 45), hours=rng.randint(0, 23)),
            }
        )
    for i, day in enumerate([3, 17, 31]):
        rows.append(
            {
                "_kind": "txn",
                "id": f"salary{i}",
                "account": "Spending",
                "description": "Salary",
                "category": "Income",
                "amount_cents": rng.randint(250000, 400000),
                "created_at": now - timedelta(days=day),
            }
        )
    rows.append(
        {
            "_kind": "txn",
            "id": "transfer0",
            "account": "Saver",
            "description": "Transfer from Spending",
            "category": "Transfer",
            "amount_cents": rng.randint(100000, 300000),
            "created_at": now - timedelta(days=rng.randint(30, 44)),
        }
    )
    return rows


def _gen_recordings(rng: random.Random, now: datetime) -> list[dict]:
    return [
        {
            "_kind": "rec",
            "id": f"rec{i}",
            "title": rng.choice(POCKET_TITLES),
            "duration_seconds": rng.randint(600, 3600),
            "tags": rng.sample(POCKET_TAGS, k=rng.randint(1, 2)),
            "recorded_at": now - timedelta(days=rng.randint(0, 30), hours=rng.randint(0, 23)),
        }
        for i in range(16)
    ]


class DemoSource(Source):
    key = "demo"
    provider = "demo"
    label = "Demo data"
    record_types = ["up.transaction", "up.account", "heypocket.recording"]
    auth_kind = "token"

    async def sync(self, mode, cursor=None) -> SyncResult:
        rng = random.Random(42)
        now = datetime.now(timezone.utc)

        records: list[dict] = _gen_transactions(rng, now)
        accounts: dict[str, int] = {}
        for t in records:
            accounts.setdefault(t["account"], 0)
            accounts[t["account"]] += t["amount_cents"]
        for name, cents in accounts.items():
            records.append({"_kind": "acct", "name": name, "cents": cents})
        records.extend(_gen_recordings(rng, now))

        return SyncResult(records=records, cursor="demo")

    def map(self, raw: dict) -> dict | None:
        k = raw["_kind"]
        if k == "txn":
            return _env(
                f"demo:up.transaction:{raw['id']}",
                "demo",
                "up.transaction",
                raw["id"],
                raw["description"],
                f"{raw['description']} at {raw['account']}",
                raw["created_at"],
                {
                    "amount_cents": raw["amount_cents"],
                    "amount": f"{raw['amount_cents'] / 100:.2f}",
                    "currency": "AUD",
                    "status": "SETTLED",
                    "category": raw["category"],
                    "is_income": raw["amount_cents"] > 0,
                },
            )
        if k == "acct":
            return _env(
                f"demo:up.account:{raw['name']}",
                "demo",
                "up.account",
                raw["name"],
                raw["name"],
                f"{raw['name']} account",
                None,
                {"balance_cents": raw["cents"], "balance": f"{raw['cents'] / 100:.2f}"},
            )
        if k == "rec":
            return _env(
                f"demo:heypocket.recording:{raw['id']}",
                "demo",
                "heypocket.recording",
                raw["id"],
                raw["title"],
                raw["title"],
                raw["recorded_at"],
                {"duration_seconds": raw["duration_seconds"], "tags": raw["tags"]},
            )
        return None


def _env(id, source, type_, ext, title, body, occurred, payload):
    return {
        "id": id,
        "source": source,
        "type": type_,
        "external_id": ext,
        "title": title,
        "body_text": body,
        "occurred_at": occurred,
        "url": "",
        "payload": payload,
        "links": [],
        "deleted": False,
    }


SOURCE = DemoSource()
