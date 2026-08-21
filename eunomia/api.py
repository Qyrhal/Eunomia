# FastAPI app: dashboard + JSON API + manual vault sync + settings (credentials/sign-ins).

import json
import secrets
import sqlite3
import time
from pathlib import Path
from urllib.parse import quote

from fastapi import FastAPI, Form, HTTPException
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from starlette.requests import Request

from . import config, crypto, db, oauth, vault
from .classifier import rules_from_buckets

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
    conn = get_db()
    vault_path = config.resolve_vault_path(conn)
    if not vault_path.exists():
        conn.close()
        raise HTTPException(400, f"Vault not found at {vault_path}. Set it in Settings first.")
    rules = rules_from_buckets(db.get_buckets(conn))
    notes = vault.scan_vault(vault_path, rules)
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
    buckets = db.get_buckets(conn)
    vault_path = config.resolve_vault_path(conn)
    vault_configured = vault_path.exists()
    conn.close()
    notes_json = json.dumps([n.__dict__ for n in notes])
    buckets_json = json.dumps([{"key": b.key, "label": b.label} for b in buckets])
    return templates.TemplateResponse(
        request,
        "dashboard.html",
        {
            "buckets_json": buckets_json,
            "notes_json": notes_json,
            "vault_configured": vault_configured,
        },
    )


@app.post("/notes")
def create_note(title: str = Form(...), bucket: str = Form(...)):
    title = title.strip()
    conn = get_db()
    vault_path = config.resolve_vault_path(conn)
    if not vault_path.exists():
        conn.close()
        raise HTTPException(400, f"Vault not found at {vault_path}. Set it in Settings first.")
    if not title:
        conn.close()
        raise HTTPException(400, "Title is required")

    try:
        vault.create_note(vault_path, title, bucket)
    except FileExistsError as e:
        conn.close()
        raise HTTPException(409, str(e))

    rules = rules_from_buckets(db.get_buckets(conn))
    notes = vault.scan_vault(vault_path, rules)
    db.sync_notes(conn, notes)
    conn.close()
    return {"created": True}


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

    conn = get_db()
    buckets = db.get_buckets(conn)
    vault_path = config.resolve_vault_path(conn)
    conn.close()

    return templates.TemplateResponse(
        request,
        "settings.html",
        {
            "connections": connections,
            "other_credentials": other_credentials,
            "buckets": buckets,
            "vault_path": str(vault_path),
            "vault_configured": vault_path.exists(),
            "error": request.query_params.get("error"),
        },
    )


@app.post("/settings/vault")
def set_vault_path(path: str = Form(...)):
    path = path.strip()
    if not path:
        return RedirectResponse(f"/settings?error={quote('Vault path is required')}", status_code=303)
    resolved = Path(path).expanduser()
    try:
        resolved.mkdir(parents=True, exist_ok=True)
    except OSError as e:
        return RedirectResponse(f"/settings?error={quote(f'Could not use that path: {e}')}", status_code=303)
    conn = get_db()
    db.set_setting(conn, "vault_path", str(resolved))
    conn.close()
    return RedirectResponse("/settings", status_code=303)


@app.post("/settings/buckets")
def add_bucket(key: str = Form(...), label: str = Form(...), keywords: str = Form("")):
    key = key.strip().lower().replace(" ", "-")
    label = label.strip()
    keyword_list = [k.strip() for k in keywords.split(",") if k.strip()]
    if not key or not label:
        return RedirectResponse(f"/settings?error={quote('Key and label are required')}", status_code=303)
    conn = get_db()
    try:
        db.add_bucket(conn, key, label, keyword_list)
    except sqlite3.IntegrityError:
        conn.close()
        return RedirectResponse(f"/settings?error={quote(f'A bucket with key \"{key}\" already exists')}", status_code=303)
    conn.close()
    return RedirectResponse("/settings", status_code=303)


@app.post("/settings/buckets/{bucket_id}/delete")
def delete_bucket(bucket_id: int):
    conn = get_db()
    db.delete_bucket(conn, bucket_id)
    conn.close()
    return RedirectResponse("/settings", status_code=303)


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
