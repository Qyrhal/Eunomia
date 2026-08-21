# FastAPI app: dashboard + JSON API + manual vault sync.

from pathlib import Path

from fastapi import FastAPI
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates
from starlette.requests import Request

from . import config, db, vault
from .models import BUCKETS

app = FastAPI(title="Eunomia")
templates = Jinja2Templates(directory=str(Path(__file__).parent / "templates"))


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
    grouped = {b: [n for n in notes if n.bucket == b] for b in BUCKETS}
    return templates.TemplateResponse(request, "dashboard.html", {"grouped": grouped})
