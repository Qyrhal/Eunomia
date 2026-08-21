# FastAPI app: dashboard + JSON API + manual vault sync + settings (credentials/sign-ins).

import json
import secrets
import time
from pathlib import Path
from urllib.parse import quote

from fastapi import FastAPI, Form, HTTPException
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from starlette.requests import Request

from . import config, crypto, db, oauth, vault
from .models import BUCKETS

app = FastAPI(title="Eunomia")
app.mount("/static", StaticFiles(directory=str(Path(__file__).parent / "static")), name="static")
templates = Jinja2Templates(directory=str(Path(__file__).parent / "templates"))

# Sign-in first: each connection here gets a "Sign in" button as the primary action.
# API keys are the fallback, offered underneath, for when the account has no OAuth app configured
# or when the service doesn't support OAuth at all (e.g. HeyPocket).
CONNECTIONS = [
    {"service": "github", "name": "GitHub", "oauth": True, "labels": ["Work", "Business"]},
    {"service": "linear", "name": "Linear", "oauth": True, "labels": ["Work", "Business"]},
    {"service": "slack", "name": "Slack", "oauth": True, "labels": ["Personal"]},
    {"service": "heypocket", "name": "HeyPocket", "oauth": False, "labels": ["Personal"]},
]

# In-memory OAuth state -> {service, label, expires}. Self-hosted single instance; lost on
# restart is fine, a dropped in-flight sign-in just needs to be retried.
_oauth_state: dict[str, dict] = {}
_OAUTH_STATE_TTL = 600


def get_db():
    return db.connect(config.DB_PATH)


@app.get("/health")
def health():
    return {"status": "ok"}


@app.post("/sync")
def sync():
    notes = vault.scan_vault(config.VAULT_PATH)
    conn = get_db()
    db.sync_notes(conn, notes)
    conn.close()
    return {"synced": len(notes)}


@app.get("/api/notes")
def api_notes(bucket: str | None = None):
    conn = get_db()
    notes = db.get_notes(conn, bucket)
    conn.close()
    return [n.__dict__ for n in notes]


@app.get("/", response_class=HTMLResponse)
def dashboard(request: Request):
    conn = get_db()
    notes = db.get_notes(conn)
    conn.close()
    notes_json = json.dumps([n.__dict__ for n in notes])
    buckets_json = json.dumps(list(BUCKETS))
    return templates.TemplateResponse(
        request, "dashboard.html", {"buckets_json": buckets_json, "notes_json": notes_json}
    )


def _redirect_uri(request: Request, service: str) -> str:
    return str(request.url_for("oauth_callback", service=service))


@app.get("/settings", response_class=HTMLResponse)
def settings_page(request: Request):
    conn = get_db()
    creds = db.get_credentials(conn)
    conn.close()

    by_service_label = {(c.service, c.label): c for c in creds}
    connections = []
    for conn_def in CONNECTIONS:
        service = conn_def["service"]
        rows = []
        for label in conn_def["labels"]:
            cred = by_service_label.get((service, label))
            rows.append(
                {
                    "label": label,
                    "connected": cred is not None,
                    "kind": cred.kind if cred else None,
                    "credential_id": cred.id if cred else None,
                }
            )
        connections.append(
            {
                "service": service,
                "name": conn_def["name"],
                "oauth_supported": conn_def["oauth"],
                "oauth_configured": oauth.is_configured(service) if conn_def["oauth"] else False,
                "rows": rows,
            }
        )

    known_services = {c["service"] for c in CONNECTIONS}
    other_credentials = [c for c in creds if c.service not in known_services]

    return templates.TemplateResponse(
        request,
        "settings.html",
        {
            "connections": connections,
            "other_credentials": other_credentials,
            "error": request.query_params.get("error"),
        },
    )


@app.post("/settings/api-key")
def add_api_key(service: str = Form(...), label: str = Form(...), key: str = Form(...)):
    service = service.strip().lower()
    label = label.strip()
    key = key.strip()
    if not service or not label or not key:
        return RedirectResponse(
            f"/settings?error={quote('Service, label, and key are all required')}", status_code=303
        )
    try:
        encrypted = crypto.encrypt(key)
    except RuntimeError as e:
        return RedirectResponse(f"/settings?error={quote(str(e))}", status_code=303)
    conn = get_db()
    db.upsert_credential(conn, service, label, "api_key", encrypted)
    conn.close()
    return RedirectResponse("/settings", status_code=303)


@app.post("/settings/credentials/{credential_id}/delete")
def delete_credential(credential_id: int):
    conn = get_db()
    db.delete_credential(conn, credential_id)
    conn.close()
    return RedirectResponse("/settings", status_code=303)


@app.get("/oauth/{service}/start")
def oauth_start(service: str, label: str, request: Request):
    if service not in oauth.PROVIDERS:
        raise HTTPException(404, "Unknown service")
    if not oauth.is_configured(service):
        msg = f"{oauth.PROVIDERS[service]['name']} sign-in isn't configured on this server yet"
        return RedirectResponse(f"/settings?error={quote(msg)}", status_code=303)
    now = time.time()
    for token, entry in list(_oauth_state.items()):
        if entry["expires"] < now:
            del _oauth_state[token]

    state = secrets.token_urlsafe(16)
    _oauth_state[state] = {"service": service, "label": label, "expires": now + _OAUTH_STATE_TTL}
    url = oauth.authorize_url(service, _redirect_uri(request, service), state)
    return RedirectResponse(url, status_code=303)


@app.get("/oauth/{service}/callback", name="oauth_callback")
def oauth_callback(service: str, code: str, state: str, request: Request):
    entry = _oauth_state.pop(state, None)
    if not entry or entry["service"] != service or entry["expires"] < time.time():
        msg = "Sign-in expired or invalid, please try again"
        return RedirectResponse(f"/settings?error={quote(msg)}", status_code=303)

    try:
        token = oauth.exchange_code_for_token(service, code, _redirect_uri(request, service))
        encrypted = crypto.encrypt(token)
    except Exception as e:
        return RedirectResponse(f"/settings?error={quote(str(e))}", status_code=303)

    conn = get_db()
    db.upsert_credential(conn, service, entry["label"], "oauth", encrypted)
    conn.close()
    return RedirectResponse("/settings", status_code=303)
