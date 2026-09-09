"""Thin bearer-token REST clients for the personal connectors.

Each client takes the credentials dict stored (encrypted) on a Connector row.
"""

import httpx

from google.oauth2.credentials import Credentials
from google_auth_oauthlib.flow import Flow
from googleapiclient.discovery import build

GOOGLE_SCOPES = [
    "https://www.googleapis.com/auth/calendar",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/drive.readonly",
]

# ---------------------------------------------------------------------------
# Twenty CRM (self-hosted)
# ---------------------------------------------------------------------------

#: default base URL when none is supplied via config
DEFAULT_TWENTY_BASE_URL = "http://localhost:3001"


class TwentyClient:
    """REST + GraphQL client for Twenty CRM (self-hosted).

    Auth: Bearer token (personal access token or session JWT) supplied as
    ``api_token`` in the connector credentials dict.

    Resource model follows Twenty's API conventions:
      - people    -> /api/v1/people
      - companies -> /api/v1/companies
      - deals     -> /api/v1/deals
      - tasks     -> /api/v1/tasks
      - notes     -> /api/v1/notes
    GraphQL endpoint: /graphql (POST, JSON body with ``query`` field).
    """

    def __init__(self, credentials: dict, base_url: str | None = None):
        self.api_token = credentials.get("api_token", "")
        self.base_url = (
            base_url or credentials.get("base_url", DEFAULT_TWENTY_BASE_URL)
        ).rstrip("/")

    def _headers(self) -> dict:
        if self.api_token:
            return {
                "Authorization": f"Bearer {self.api_token}",
                "Accept": "application/json",
            }
        return {"Accept": "application/json"}

    def _get(self, path: str, params: dict | None = None, timeout: float = 15) -> dict:
        r = httpx.get(
            f"{self.base_url}/{path}",
            headers=self._headers(),
            params=params,
            timeout=timeout,
        )
        r.raise_for_status()
        return r.json() if r.text.strip() else {}

    def _post_graphql(
        self, query: str, variables: dict | None = None, timeout: float = 15
    ) -> dict:
        payload: dict = {"query": query}
        if variables:
            payload["variables"] = variables
        r = httpx.post(
            f"{self.base_url}/graphql",
            headers=self._headers(),
            json=payload,
            timeout=timeout,
        )
        r.raise_for_status()
        return r.json()

    # -- ping ----------------------------------------------------------------

    def ping(self) -> bool:
        """Health check: hit /healthz."""
        try:
            r = httpx.get(f"{self.base_url}/healthz", headers=self._headers(), timeout=10)
            return r.status_code == 200
        except Exception:
            return False

    # -- resource listing ---------------------------------------------------

    def people(self, limit: int = 50, cursor: str | None = None) -> dict:
        """List people (contacts). Returns the raw Twenty list response."""
        params: dict = {"limit": limit} if limit else {}
        if cursor:
            params["after"] = cursor
        return self._get("api/v1/people", params=params)

    def companies(self, limit: int = 50, cursor: str | None = None) -> dict:
        params = {"limit": limit} if limit else {}
        if cursor:
            params["after"] = cursor
        return self._get("api/v1/companies", params=params)

    def deals(self, limit: int = 50, cursor: str | None = None) -> dict:
        params = {"limit": limit} if limit else {}
        if cursor:
            params["after"] = cursor
        return self._get("api/v1/deals", params=params)

    def tasks(self, limit: int = 50, cursor: str | None = None) -> dict:
        params = {"limit": limit} if limit else {}
        if cursor:
            params["after"] = cursor
        return self._get("api/v1/tasks", params=params)

    def notes(self, limit: int = 50, cursor: str | None = None) -> dict:
        params = {"limit": limit} if limit else {}
        if cursor:
            params["after"] = cursor
        return self._get("api/v1/notes", params=params)

    # -- single resource fetch ----------------------------------------------

    def person(self, id: str) -> dict:
        return self._get(f"api/v1/people/{id}")

    def company(self, id: str) -> dict:
        return self._get(f"api/v1/companies/{id}")

    def deal(self, id: str) -> dict:
        return self._get(f"api/v1/deals/{id}")

    def task(self, id: str) -> dict:
        return self._get(f"api/v1/tasks/{id}")

    def note(self, id: str) -> dict:
        return self._get(f"api/v1/notes/{id}")

    # -- GraphQL convenience -------------------------------------------------

    def gql(self, query: str, variables: dict | None = None) -> dict:
        """Run an arbitrary GraphQL query and return the parsed JSON body."""
        return self._post_graphql(query, variables)

    # -- dashboard-style summary --------------------------------------------

    def summary(self) -> dict:
        """Quick counts for the main CRM objects. All fields come straight off
        Twenty's list endpoints — no invented metrics."""
        counts: dict[str, int] = {}
        for name, fn in [
            ("people", self.people),
            ("companies", self.companies),
            ("deals", self.deals),
            ("tasks", self.tasks),
            ("notes", self.notes),
        ]:
            try:
                data = fn(limit=1)
                # Twenty list responses use a ``data`` envelope
                rows = data.get("data") or data.get("items") or []
                counts[name] = len(rows) if isinstance(rows, list) else 0
            except Exception:
                counts[name] = -1  # mark as errored
        return counts


# ---------------------------------------------------------------------------
# Up Bank
# ---------------------------------------------------------------------------


class UpBankClient:
    base_url = "https://api.up.com.au/api/v1"

    def __init__(self, credentials: dict):
        self.token = credentials.get("personal_access_token", "")

    def _headers(self):
        return {"Authorization": f"Bearer {self.token}"}

    def ping(self) -> bool:
        r = httpx.get(f"{self.base_url}/util/ping", headers=self._headers(), timeout=10)
        return r.status_code == 200

    def accounts(self) -> dict:
        r = httpx.get(f"{self.base_url}/accounts", headers=self._headers(), timeout=10)
        r.raise_for_status()
        return r.json()

    def transactions(self, params: dict | None = None) -> dict:
        r = httpx.get(
            f"{self.base_url}/transactions",
            headers=self._headers(),
            params=params or {},
            timeout=10,
        )
        r.raise_for_status()
        return r.json()

    def categories(self) -> dict:
        r = httpx.get(f"{self.base_url}/categories", headers=self._headers(), timeout=10)
        r.raise_for_status()
        return r.json()

    def finance_summary(self, since_iso: str) -> dict:
        """Balance across accounts + settled spend broken down by category since
        ``since_iso``.

        Every field here comes straight off the transaction/account/category
        resources — no invented metrics (this is a personal bank account).
        """
        accounts = self.accounts().get("data", [])
        balance = sum(a["attributes"]["balance"]["valueInBaseUnits"] for a in accounts) / 100

        cat_names = {
            c["id"]: c["attributes"]["name"] for c in self.categories().get("data", [])
        }

        data = self.transactions({"filter[since]": since_iso, "page[size]": 100}).get(
            "data", []
        )
        settled = [t for t in data if t["attributes"]["status"] == "SETTLED"]

        spend_by_category: dict[str, int] = {}
        spend_by_day: dict[str, int] = {}
        for t in settled:
            cents = t["attributes"]["amount"]["valueInBaseUnits"]
            if cents >= 0:
                continue
            cat = t["relationships"].get("category", {}).get("data")
            name = (
                cat_names.get(cat["id"], "Uncategorised") if cat else "Uncategorised"
            )
            spend_by_category[name] = spend_by_category.get(name, 0) - cents
            day = t["attributes"]["createdAt"][:10]
            spend_by_day[day] = spend_by_day.get(day, 0) - cents

        return {
            "balance": round(balance, 2),
            "accounts": [
                {
                    "name": a["attributes"]["displayName"],
                    "balance": a["attributes"]["balance"]["value"],
                }
                for a in accounts
            ],
            "spend_by_category": sorted(
                [
                    {"category": k, "amount": round(v / 100, 2)}
                    for k, v in spend_by_category.items()
                ],
                key=lambda r: -r["amount"],
            ),
            "spend_by_day": sorted(
                [
                    {"day": k, "amount": round(v / 100, 2)}
                    for k, v in spend_by_day.items()
                ],
                key=lambda r: r["day"],
            ),
            "recent_transactions": [
                {
                    "description": t["attributes"]["description"],
                    "amount": t["attributes"]["amount"]["value"],
                    "created_at": t["attributes"]["createdAt"],
                }
                for t in sorted(
                    data, key=lambda t: t["attributes"]["createdAt"], reverse=True
                )[:20]
            ],
        }

    def week_summary(self, since_iso: str) -> dict:
        """Settled transaction count + total spend (negative amounts) since
        ``since_iso``."""
        data = self.transactions({"filter[since]": since_iso, "page[size]": 100})
        rows = [
            t
            for t in data.get("data", [])
            if t.get("attributes", {}).get("status") == "SETTLED"
        ]
        spend_cents = sum(
            -t["attributes"]["amount"]["valueInBaseUnits"]
            for t in rows
            if t["attributes"]["amount"]["valueInBaseUnits"] < 0
        )
        return {"transaction_count": len(rows), "spent": spend_cents / 100}


# ---------------------------------------------------------------------------
# PocketAI (heypocket)
# ---------------------------------------------------------------------------


class PocketAIClient:
    default_base_url = "https://public.heypocketai.com/api/v1"

    def __init__(self, credentials: dict, base_url: str | None = None):
        self.api_key = credentials.get("api_key", "")
        self.base_url = (base_url or self.default_base_url).rstrip("/")

    def _headers(self):
        return {"Authorization": f"Bearer {self.api_key}"}

    def ping(self) -> bool:
        r = httpx.get(
            f"{self.base_url}/public/recordings",
            headers=self._headers(),
            params={"limit": 1},
            timeout=10,
        )
        return r.status_code == 200

    def recordings(self, params: dict | None = None) -> dict:
        r = httpx.get(
            f"{self.base_url}/public/recordings",
            headers=self._headers(),
            params=params or {},
            timeout=10,
        )
        r.raise_for_status()
        return r.json()

    def search(self, query: str) -> dict:
        r = httpx.post(
            f"{self.base_url}/public/search",
            headers=self._headers(),
            json={"query": query},
            timeout=10,
        )
        r.raise_for_status()
        return r.json()

    def recording(self, recording_id: str) -> dict:
        """Full detail for a single recording — transcript + summarizations."""
        r = httpx.get(
            f"{self.base_url}/public/recordings/{recording_id}",
            headers=self._headers(),
            timeout=15,
        )
        r.raise_for_status()
        return r.json()

    def summary(self, since_iso_date: str) -> dict:
        """Recording count/duration/tags since a given date. Every field comes
        straight off the recording resource (``duration``, ``tags``) — the API
        has no dedicated action-items/todos field, so this doesn't invent one."""
        data = self.recordings({"start_date": since_iso_date, "limit": 100}).get(
            "data", []
        )

        tag_counts: dict[str, int] = {}
        for r in data:
            for tag in r.get("tags", []):
                name = tag.get("name", "untagged")
                tag_counts[name] = tag_counts.get(name, 0) + 1

        return {
            "recordings_count": len(data),
            "total_duration_minutes": round(
                sum(r.get("duration", 0) for r in data) / 60, 1
            ),
            "tag_breakdown": sorted(
                [
                    {"tag": k, "count": v}
                    for k, v in tag_counts.items()
                ],
                key=lambda row: -row["count"],
            ),
            "recent_recordings": [
                {
                    "title": r.get("title", ""),
                    "duration_minutes": round(r.get("duration", 0) / 60, 1),
                    "recorded_at": r.get("recording_at") or r.get("created_at"),
                    "tags": [tag.get("name") for tag in r.get("tags", [])],
                }
                for r in sorted(
                    data, key=lambda r: r.get("recording_at") or "", reverse=True
                )[:10]
            ],
        }


# ---------------------------------------------------------------------------
# Google (Calendar + Gmail)
# ---------------------------------------------------------------------------


def google_oauth_flow(client_config: dict, redirect_uri: str) -> Flow:
    return Flow.from_client_config(
        client_config, scopes=GOOGLE_SCOPES, redirect_uri=redirect_uri
    )


def google_credentials_from_stored(credentials: dict) -> Credentials:
    return Credentials(
        token=credentials.get("token"),
        refresh_token=credentials.get("refresh_token"),
        token_uri="https://oauth2.googleapis.com/token",
        client_id=credentials.get("client_id"),
        client_secret=credentials.get("client_secret"),
        scopes=GOOGLE_SCOPES,
    )


class GoogleClient:
    """Wraps the Calendar + Gmail APIs behind one object per stored token set."""

    def __init__(self, credentials: dict):
        self._raw_credentials = credentials
        self._creds = google_credentials_from_stored(credentials)

    def refreshed_credentials_dict(self) -> dict:
        """Call after any API call to persist a refreshed access token."""
        return {
            **self._raw_credentials,
            "token": self._creds.token,
        }

    def calendar_events(
        self, calendar_id: str = "primary", max_results: int = 20, **kwargs
    ) -> list:
        from datetime import datetime, timezone as dt_timezone

        kwargs.setdefault(
            "timeMin", datetime.now(dt_timezone.utc).isoformat()
        )
        service = build("calendar", "v3", credentials=self._creds)
        result = (
            service.events()
            .list(
                calendarId=calendar_id,
                maxResults=max_results,
                singleEvents=True,
                orderBy="startTime",
                **kwargs,
            )
            .execute()
        )
        return result.get("items", [])

    def create_calendar_event(self, calendar_id: str = "primary", **body) -> dict:
        service = build("calendar", "v3", credentials=self._creds)
        return service.events().insert(
            calendarId=calendar_id, body=body
        ).execute()

    def gmail_messages(self, query: str = "is:unread", max_results: int = 20) -> list:
        service = build("gmail", "v1", credentials=self._creds)
        result = (
            service.users()
            .messages()
            .list(userId="me", q=query, maxResults=max_results)
            .execute()
        )
        return result.get("messages", [])

    def gmail_message(self, message_id: str) -> dict:
        service = build("gmail", "v1", credentials=self._creds)
        return service.users().messages().get(
            userId="me", id=message_id, format="full"
        ).execute()

    def gmail_search(self, query: str, max_results: int = 5) -> list[dict]:
        """Compact search results — subject/from/date/snippet, not the full
        body, to keep this cheap enough to hand straight to an LLM as tool
        output."""
        service = build("gmail", "v1", credentials=self._creds)
        listing = (
            service.users()
            .messages()
            .list(userId="me", q=query, maxResults=max_results)
            .execute()
        )
        results = []
        for item in listing.get("messages", []):
            msg = (
                service.users()
                .messages()
                .get(
                    userId="me",
                    id=item["id"],
                    format="metadata",
                    metadataHeaders=["Subject", "From", "Date"],
                )
                .execute()
            )
            headers = {
                h["name"]: h["value"] for h in msg.get("payload", {}).get("headers", [])
            }
            results.append(
                {
                    "subject": headers.get("Subject", ""),
                    "from": headers.get("From", ""),
                    "date": headers.get("Date", ""),
                    "snippet": msg.get("snippet", ""),
                }
            )
        return results

    def gmail_list_detailed(self, query: str, max_results: int = 100) -> list[dict]:
        """id + Subject/From/Date + snippet per message — envelope-shaped, no
        body (#21)."""
        service = build("gmail", "v1", credentials=self._creds)
        listing = (
            service.users()
            .messages()
            .list(userId="me", q=query, maxResults=max_results)
            .execute()
        )
        out = []
        for item in listing.get("messages", []):
            msg = service.users().messages().get(
                userId="me",
                id=item["id"],
                format="metadata",
                metadataHeaders=["Subject", "From", "To", "Date"],
            ).execute()
            headers = {
                h["name"]: h["value"] for h in msg.get("payload", {}).get("headers", [])
            }
            out.append(
                {
                    "id": item["id"],
                    "threadId": msg.get("threadId"),
                    "subject": headers.get("Subject", ""),
                    "from": headers.get("From", ""),
                    "to": headers.get("To", ""),
                    "date": headers.get("Date", ""),
                    "snippet": msg.get("snippet", ""),
                    "labelIds": msg.get("labelIds", []),
                    "internalDate": msg.get("internalDate"),
                }
            )
        return out

    def drive_files(
        self, query: str = "trashed = false", page_size: int = 100
    ) -> list[dict]:
        """File metadata only — body is fetched on demand (#41)."""
        service = build("drive", "v3", credentials=self._creds)
        result = (
            service.files()
            .list(
                q=query,
                pageSize=page_size,
                orderBy="modifiedTime desc",
                fields=(
                    "files(id,name,mimeType,modifiedTime,createdTime,"
                    "webViewLink,owners(emailAddress),size)"
                ),
            )
            .execute()
        )
        return result.get("files", [])

    def drive_file_text(self, file_id: str, mime_type: str = "") -> str:
        """On-demand plain-text body for a Drive file (#41)."""
        service = build("drive", "v3", credentials=self._creds)
        if mime_type == "application/vnd.google-apps.document":
            return (
                service.files()
                .export(fileId=file_id, mimeType="text/plain")
                .execute()
                .decode("utf-8", "replace")
            )
        data = service.files().get_media(fileId=file_id).execute()
        return (
            data.decode("utf-8", "replace") if isinstance(data, bytes) else str(data)
        )

    def gmail_unread_count(self) -> int:
        """Uses the list response's ``resultSizeEstimate`` rather than
        paginating everything."""
        service = build("gmail", "v1", credentials=self._creds)
        result = (
            service.users()
            .messages()
            .list(userId="me", q="is:unread", maxResults=1)
            .execute()
        )
        return result.get("resultSizeEstimate", 0)

    def calendar_events_today(self, calendar_id: str = "primary") -> int:
        from datetime import datetime, timedelta, timezone as dt_timezone

        now = datetime.now(dt_timezone.utc)
        start = now.replace(hour=0, minute=0, second=0, microsecond=0)
        end = start + timedelta(days=1)
        events = self.calendar_events(
            calendar_id=calendar_id,
            max_results=50,
            timeMin=start.isoformat(),
            timeMax=end.isoformat(),
        )
        return len(events)
