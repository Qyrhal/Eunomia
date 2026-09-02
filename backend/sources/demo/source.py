"""Demo source (#44) — pushes the seeded Demo* rows through the real pipeline so
the cache, search, tools and finance_summary all work with no live accounts.

Enable with a `Connector(kind="demo", enabled=True)`; populate the Demo* rows
with `manage.py seed_demo` (which also runs the sync).
"""

from sources.base import Source, SyncResult


class DemoSource(Source):
    key = "demo"
    provider = "demo"
    label = "Demo data"
    record_types = ["up.transaction", "up.account", "gcal.event", "gmail.message", "heypocket.recording"]
    auth_kind = "token"
    secret_fields = []

    def sync(self, mode, cursor=None) -> SyncResult:
        from connectors.models import (
            DemoCalendarEvent, DemoEmail, DemoRecording, DemoTransaction,
        )

        records: list[dict] = []
        accounts = {}
        for t in DemoTransaction.objects.all():
            records.append({"_kind": "txn", "obj": t})
            accounts.setdefault(t.account, 0)
            accounts[t.account] += t.amount_cents
        for name, cents in accounts.items():
            records.append({"_kind": "acct", "name": name, "cents": cents})
        for e in DemoCalendarEvent.objects.all():
            records.append({"_kind": "event", "obj": e})
        for m in DemoEmail.objects.all():
            records.append({"_kind": "email", "obj": m})
        for r in DemoRecording.objects.all():
            records.append({"_kind": "rec", "obj": r})
        return SyncResult(records=records, cursor="demo")

    def map(self, raw: dict) -> dict | None:
        k = raw["_kind"]
        if k == "txn":
            t = raw["obj"]
            return _env(f"demo:up.transaction:{t.id}", "demo", "up.transaction", str(t.id),
                        t.description, f"{t.description} at {t.account}", t.created_at, {
                            "amount_cents": t.amount_cents, "amount": f"{t.amount_cents/100:.2f}",
                            "currency": "AUD", "status": "SETTLED", "category": t.category,
                            "is_income": t.amount_cents > 0})
        if k == "acct":
            return _env(f"demo:up.account:{raw['name']}", "demo", "up.account", raw["name"],
                        raw["name"], f"{raw['name']} account", None,
                        {"balance_cents": raw["cents"], "balance": f"{raw['cents']/100:.2f}"})
        if k == "event":
            e = raw["obj"]
            return _env(f"demo:gcal.event:{e.id}", "demo", "gcal.event", str(e.id),
                        e.summary, e.summary, e.start_at, {"attendees": e.attendees})
        if k == "email":
            m = raw["obj"]
            return _env(f"demo:gmail.message:{m.id}", "demo", "gmail.message", str(m.id),
                        m.subject, f"{m.sender} — {m.subject} — {m.snippet}", m.received_at,
                        {"from": m.sender, "unread": m.unread})
        if k == "rec":
            r = raw["obj"]
            return _env(f"demo:heypocket.recording:{r.id}", "demo", "heypocket.recording", str(r.id),
                        r.title, r.title, r.recorded_at,
                        {"duration_seconds": r.duration_seconds, "tags": r.tags})
        return None


def _env(id, source, type_, ext, title, body, occurred, payload):
    return {
        "id": id, "source": source, "type": type_, "external_id": ext,
        "title": title, "body_text": body,
        "occurred_at": occurred.isoformat() if hasattr(occurred, "isoformat") else occurred,
        "url": "", "payload": payload, "links": [], "deleted": False,
    }
