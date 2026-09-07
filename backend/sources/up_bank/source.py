"""Up Bank source (#12). Wraps connectors.clients.UpBankClient.

Sync: `filter[since]` delta walk over `/transactions` + a full `/accounts` pull.
Webhook: verify `X-Up-Authenticity-Signature` (HMAC-SHA256 of the raw body keyed
by the webhook secretKey), then re-fetch the referenced transaction (#19).
"""

import hashlib
import hmac
from datetime import timedelta

from django.utils import timezone

from connectors.clients import UpBankClient
from sources.base import Source, SyncResult, ToolSpec
from sources.registry import credentials_for


class UpBankSource(Source):
    key = "up_bank"
    provider = "up_bank"
    label = "Up Bank"
    record_types = ["up.transaction", "up.account"]
    auth_kind = "token"

    # -- sync -----------------------------------------------------------------
    def _client(self) -> UpBankClient:
        return UpBankClient(credentials_for(self))

    def sync(self, mode, cursor=None) -> SyncResult:
        client = self._client()
        since = cursor or (timezone.now() - timedelta(days=30)).isoformat()
        records: list[dict] = []

        page = client.transactions({"filter[since]": since, "page[size]": 100})
        records.extend(page.get("data", []))
        for _ in range(20):  # cap the walk
            nxt = page.get("links", {}).get("next")
            if not nxt:
                break
            import httpx

            page = httpx.get(nxt, headers=client._headers(), timeout=15).json()
            records.extend(page.get("data", []))

        try:
            records.extend(client.accounts().get("data", []))
        except Exception:
            pass

        newest = max(
            (r["attributes"]["createdAt"] for r in records if r.get("type") == "transactions"),
            default=cursor or since,
        )
        return SyncResult(records=records, cursor=newest)

    # -- map ----------------------------------------------------------------
    def map(self, raw: dict) -> dict | None:
        t = raw.get("type")
        if t == "transactions":
            return self._map_txn(raw)
        if t == "accounts":
            return self._map_account(raw)
        return None

    def _map_txn(self, raw: dict) -> dict:
        a = raw["attributes"]
        cat = (raw.get("relationships", {}).get("category", {}).get("data") or {}).get("id")
        body = " ".join(x for x in (a.get("description"), a.get("rawText"), a.get("message")) if x)
        return {
            "id": f"up_bank:up.transaction:{raw['id']}",
            "source": "up_bank",
            "type": "up.transaction",
            "external_id": raw["id"],
            "title": a.get("description", ""),
            "body_text": body,
            "occurred_at": a.get("createdAt"),
            "url": "",
            "payload": {
                "amount": a["amount"]["value"],
                "amount_cents": a["amount"]["valueInBaseUnits"],
                "currency": a["amount"]["currencyCode"],
                "status": a.get("status"),
                "settled_at": a.get("settledAt"),
                "category": cat,
                "is_income": a["amount"]["valueInBaseUnits"] > 0,
            },
            "links": [],
            "deleted": bool(raw.get("_deleted")),
        }

    def _map_account(self, raw: dict) -> dict:
        a = raw["attributes"]
        return {
            "id": f"up_bank:up.account:{raw['id']}",
            "source": "up_bank",
            "type": "up.account",
            "external_id": raw["id"],
            "title": a.get("displayName", ""),
            "body_text": f"{a.get('displayName','')} — {a.get('accountType','')}",
            "occurred_at": a.get("createdAt"),
            "url": "",
            "payload": {
                "balance": a["balance"]["value"],
                "balance_cents": a["balance"]["valueInBaseUnits"],
                "account_type": a.get("accountType"),
                "ownership_type": a.get("ownershipType"),
            },
            "links": [],
            "deleted": False,
        }

    # -- webhook ----------------------------------------------------------------
    def webhook(self, request) -> list[dict] | None:
        creds = credentials_for(self)
        secret = creds.get("webhook_secret_key", "")
        sig = request.headers.get("X-Up-Authenticity-Signature", "")
        body = request.body
        if not secret or not hmac.compare_digest(
            sig, hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()
        ):
            return None

        import json

        event = json.loads(body).get("data", {})
        etype = event.get("attributes", {}).get("eventType")
        txn = event.get("relationships", {}).get("transaction", {})
        txn_id = (txn.get("data") or {}).get("id")

        if etype == "TRANSACTION_DELETED" and txn_id:
            now = timezone.now().isoformat()
            return [{
                "type": "transactions", "id": txn_id, "relationships": {},
                "attributes": {"description": "", "createdAt": now,
                               "amount": {"value": "0", "valueInBaseUnits": 0, "currencyCode": "AUD"}},
                "_deleted": True,
            }]

        rel = txn.get("links", {}).get("related")
        if rel:
            import httpx

            data = httpx.get(rel, headers=self._client()._headers(), timeout=15).json().get("data")
            return [data] if data else None
        return None

    # -- tools ----------------------------------------------------------------
    def tools(self) -> list[ToolSpec]:
        return [
            ToolSpec(
                name="ping",
                schema={"type": "object", "properties": {}},
                impl=lambda: {"ok": self._client().ping()},
            ),
            ToolSpec(
                name="finance_summary",
                schema={"type": "object", "properties": {"since": {"type": "string", "description": "ISO date; default 30d ago"}}},
                impl=finance_summary,
            ),
        ]


def finance_summary(since: str | None = None) -> dict:
    """Balance + spend-by-category/day + recent transactions, computed from the
    cached Up Bank records (#42). Every field comes straight off a transaction or
    account — no invented metrics (it's a personal bank account).
    """
    from datetime import timedelta

    from django.utils import timezone

    from cache.models import CacheRecord

    since = since or (timezone.now() - timedelta(days=30)).isoformat()

    accounts = CacheRecord.objects.filter(type="up.account", deleted=False)
    balance_cents = sum((a.payload or {}).get("balance_cents", 0) for a in accounts)

    txns = CacheRecord.objects.filter(
        type="up.transaction", deleted=False, occurred_at__gte=since
    ).order_by("-occurred_at")

    by_cat: dict[str, int] = {}
    by_day: dict[str, int] = {}
    for t in txns:
        cents = (t.payload or {}).get("amount_cents", 0)
        if cents >= 0:
            continue
        cat = (t.payload or {}).get("category") or "uncategorised"
        by_cat[cat] = by_cat.get(cat, 0) - cents
        day = t.occurred_at.date().isoformat() if t.occurred_at else "?"
        by_day[day] = by_day.get(day, 0) - cents

    return {
        "since": since,
        "balance": round(balance_cents / 100, 2),
        "accounts": [
            {"name": a.title, "balance": (a.payload or {}).get("balance")}
            for a in accounts
        ],
        "spend_by_category": sorted(
            [{"category": k, "amount": round(v / 100, 2)} for k, v in by_cat.items()],
            key=lambda r: -r["amount"],
        ),
        "spend_by_day": sorted(
            [{"day": k, "amount": round(v / 100, 2)} for k, v in by_day.items()],
            key=lambda r: r["day"],
        ),
        "recent_transactions": [
            {"description": t.title, "amount": (t.payload or {}).get("amount"),
             "status": (t.payload or {}).get("status"),
             "occurred_at": t.occurred_at.isoformat() if t.occurred_at else None}
            for t in txns[:20]
        ],
    }
