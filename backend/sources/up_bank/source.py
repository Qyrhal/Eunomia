"""Up Bank source. Wraps connectors.clients.UpBankClient.

Sync: `filter[since]` delta walk over `/transactions` + a full `/accounts` and
`/categories` pull.
Webhook: verify `X-Up-Authenticity-Signature` (HMAC-SHA256 of the raw body
keyed by the webhook secretKey), then re-fetch the referenced transaction.

Tool helpers (`finance_summary`, `list_transactions`, `list_accounts`) read
from the cache (populated by the periodic sync) rather than hitting the live
API, matching the Django version's "no invented metrics" ethos.
"""

import hashlib
import hmac
import json
from datetime import datetime, timedelta, timezone

from connectors.clients import UpBankClient
from sources.base import Source, SyncResult, ToolSpec
from sources.registry import credentials_for


def _parse_dt(value: str | None):
    """cache_record.occurred_at is a SurrealDB `option<datetime>` field -- the
    driver only coerces real `datetime` objects, not ISO strings, so every
    mapper must parse its source's date strings before building the
    envelope."""
    if not value:
        return None
    return datetime.fromisoformat(value)


class UpBankSource(Source):
    key = "up_bank"
    provider = "up_bank"
    label = "Up Bank"
    record_types = ["up.transaction", "up.account", "up.category"]
    auth_kind = "token"

    # -- sync -----------------------------------------------------------------
    async def _client(self) -> UpBankClient:
        return UpBankClient(await credentials_for(self))

    async def sync(self, mode, cursor=None) -> SyncResult:
        client = await self._client()
        since = cursor or (datetime.now(timezone.utc) - timedelta(days=30)).isoformat()
        records: list[dict] = []

        page = await client.transactions({"filter[since]": since, "page[size]": 100})
        records.extend(page.get("data", []))
        for _ in range(20):  # cap the walk
            nxt = page.get("links", {}).get("next")
            if not nxt:
                break
            import httpx

            async with httpx.AsyncClient() as http:
                r = await http.get(nxt, headers=client._headers(), timeout=15)
            page = r.json()
            records.extend(page.get("data", []))

        try:
            records.extend((await client.accounts()).get("data", []))
        except Exception:
            pass

        try:
            records.extend((await client.categories()).get("data", []))
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
        if t == "categories":
            return self._map_category(raw)
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
            "occurred_at": _parse_dt(a.get("createdAt")),
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
            "body_text": f"{a.get('displayName', '')} — {a.get('accountType', '')}",
            "occurred_at": _parse_dt(a.get("createdAt")),
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

    def _map_category(self, raw: dict) -> dict:
        a = raw["attributes"]
        parent = (raw.get("relationships", {}).get("parent", {}).get("data") or {}).get("id")
        return {
            "id": f"up_bank:up.category:{raw['id']}",
            "source": "up_bank",
            "type": "up.category",
            "external_id": raw["id"],
            "title": a.get("name", ""),
            "body_text": a.get("name", ""),
            "occurred_at": None,
            "url": "",
            "payload": {"parent": parent},
            "links": [],
            "deleted": False,
        }

    # -- webhook ----------------------------------------------------------------
    async def webhook(self, request) -> list[dict] | None:
        creds = await credentials_for(self)
        secret = creds.get("webhook_secret_key", "")
        sig = request.headers.get("X-Up-Authenticity-Signature", "")
        body = await request.body()
        if not secret or not hmac.compare_digest(sig, hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()):
            return None

        event = json.loads(body).get("data", {})
        etype = event.get("attributes", {}).get("eventType")
        txn = event.get("relationships", {}).get("transaction", {})
        txn_id = (txn.get("data") or {}).get("id")

        if etype == "TRANSACTION_DELETED" and txn_id:
            now = datetime.now(timezone.utc).isoformat()
            return [
                {
                    "type": "transactions",
                    "id": txn_id,
                    "relationships": {},
                    "attributes": {
                        "description": "",
                        "createdAt": now,
                        "amount": {"value": "0", "valueInBaseUnits": 0, "currencyCode": "AUD"},
                    },
                    "_deleted": True,
                }
            ]

        rel = txn.get("links", {}).get("related")
        if rel:
            import httpx

            client = await self._client()
            async with httpx.AsyncClient() as http:
                resp = await http.get(rel, headers=client._headers(), timeout=15)
            data = resp.json().get("data")
            return [data] if data else None
        return None

    # -- tools ----------------------------------------------------------------
    def tools(self) -> list[ToolSpec]:
        return [
            ToolSpec(
                name="finance_summary",
                schema={
                    "type": "object",
                    "properties": {"since": {"type": "string", "description": "ISO date; default 30d ago"}},
                },
                impl=finance_summary,
            ),
            ToolSpec(
                name="list_transactions",
                schema={
                    "type": "object",
                    "properties": {
                        "days": {"type": "integer", "description": "How many days back to look (default 30)"},
                        "category": {"type": "string", "description": "Up Bank category id, e.g. 'restaurants-and-cafes'"},
                        "limit": {"type": "integer"},
                    },
                },
                impl=list_transactions,
            ),
            ToolSpec(
                name="list_accounts",
                schema={"type": "object", "properties": {}},
                impl=list_accounts,
            ),
        ]


def _iso(dt) -> str | None:
    if dt is None:
        return None
    return dt.isoformat() if hasattr(dt, "isoformat") else dt


async def _category_names() -> dict[str, str]:
    from cache import search as cs

    rows = await cs.list_records(type="up.category", limit=500)
    return {r.external_id: r.title for r in rows}


async def finance_summary(since: str | None = None) -> dict:
    """Balance + spend-by-category/day + recent transactions, computed from the
    cache. Every field comes straight off a transaction or account -- no
    invented metrics (personal bank account)."""
    from cache import search as cs

    since = since or (datetime.now(timezone.utc) - timedelta(days=30)).isoformat()
    category_names = await _category_names()

    accounts = await cs.list_records(type="up.account", limit=200)
    balance_cents = sum((a.payload or {}).get("balance_cents", 0) for a in accounts)

    txns = [
        t
        for t in await cs.list_records(type="up.transaction", limit=2000)
        if _iso(t.occurred_at) and _iso(t.occurred_at) >= since
    ]
    txns.sort(key=lambda t: _iso(t.occurred_at) or "", reverse=True)

    by_cat: dict[str, int] = {}
    by_day: dict[str, int] = {}
    for t in txns:
        cents = (t.payload or {}).get("amount_cents", 0)
        if cents >= 0:
            continue
        cat_id = (t.payload or {}).get("category")
        cat = category_names.get(cat_id, cat_id) or "uncategorised"
        by_cat[cat] = by_cat.get(cat, 0) - cents
        day = (_iso(t.occurred_at) or "")[:10] or "?"
        by_day[day] = by_day.get(day, 0) - cents

    return {
        "since": since,
        "balance": round(balance_cents / 100, 2),
        "accounts": [{"name": a.title, "balance": (a.payload or {}).get("balance")} for a in accounts],
        "spend_by_category": sorted(
            [{"category": k, "amount": round(v / 100, 2)} for k, v in by_cat.items()], key=lambda r: -r["amount"]
        ),
        "spend_by_day": sorted(
            [{"day": k, "amount": round(v / 100, 2)} for k, v in by_day.items()], key=lambda r: r["day"]
        ),
        "recent_transactions": [
            {
                "description": t.title,
                "amount": (t.payload or {}).get("amount"),
                "status": (t.payload or {}).get("status"),
                "occurred_at": _iso(t.occurred_at),
            }
            for t in txns[:20]
        ],
    }


async def list_transactions(days: int = 30, category: str | None = None, limit: int = 50) -> list[dict]:
    """Recent settled + pending transactions from the cache, e.g. for drafting
    finance follow-up tasks (pay a bill, dispute a charge)."""
    from cache import search as cs

    since = (datetime.now(timezone.utc) - timedelta(days=days)).isoformat()
    category_names = await _category_names()

    txns = [
        t
        for t in await cs.list_records(type="up.transaction", limit=2000)
        if _iso(t.occurred_at) and _iso(t.occurred_at) >= since
    ]
    if category:
        txns = [t for t in txns if (t.payload or {}).get("category") == category]
    txns.sort(key=lambda t: _iso(t.occurred_at) or "", reverse=True)

    return [
        {
            "description": t.title,
            "amount": (t.payload or {}).get("amount"),
            "status": (t.payload or {}).get("status"),
            "category": category_names.get((t.payload or {}).get("category"), (t.payload or {}).get("category")),
            "created_at": _iso(t.occurred_at),
        }
        for t in txns[: min(int(limit), 200)]
    ]


async def list_accounts() -> list[dict]:
    """Every cached Up Bank account and its last-synced balance."""
    from cache import search as cs

    accounts = await cs.list_records(type="up.account", limit=200)
    return [
        {
            "name": a.title,
            "balance": (a.payload or {}).get("balance"),
            "account_type": (a.payload or {}).get("account_type"),
            "ownership_type": (a.payload or {}).get("ownership_type"),
        }
        for a in accounts
    ]


SOURCE = UpBankSource()
